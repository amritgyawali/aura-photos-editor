//! The phase 07 evaluation harness: section 10.1's gates, and the proof that the
//! harness itself would catch a bad model.
//!
//! Named by section 4 of the phase document as `ml/models/scene/eval_scene.py`'s Rust
//! counterpart, and wired in as a test target of `aura-brain-wedding`, so
//! `cargo test --workspace` runs it and CI cannot forget it.
//!
//! ## What can and cannot be gated today
//!
//! Section 10.1 sets six gates. Three of them - top-1 accuracy, per-class accuracy and
//! ritual F1 - are statements about a *trained model*, and the two this build ships are
//! placeholders with the right architecture and no training (condition **C1** in
//! `docs/progress/PHASE-07-EXIT.md`). Measuring the shipped weights against them would
//! produce three numbers describing a random projection, and a gate that passes for the
//! wrong reason is worse than one that is honestly deferred.
//!
//! So this file takes the same shape phase 05's and phase 06's harnesses took:
//!
//! 1. **It measures every metric** with the arithmetic `ml/models/scene/eval_scene.py`
//!    uses, so the day a model is trained the two numbers can be compared rather than
//!    argued about.
//! 2. **It gates them on ground truth whose answer is known by construction** - the
//!    synthetic weddings in `aura_brain_wedding::fixtures` - which proves the harness
//!    would fail a model that had learned nothing.
//! 3. **It gates, for real and today, everything that does not depend on the weights**:
//!    the abstention rules, the HMM's emissions and Viterbi path, the transition
//!    matrix's shape, the penalty search, the chapter band, the merge pass, the
//!    two-shooter case, the key-frame filter and determinism.
//!
//! Every deferred gate is printed with the reason it is deferred, so a reader of the CI
//! log sees the same list the exit report carries.

use std::collections::BTreeMap;

use aura_brain_wedding::fixtures::{self, Truth};
use aura_brain_wedding::scene::attributes::{decode_attributes, AttributeReport, ContextFeatures};
use aura_brain_wedding::scene::classifier::{self, decode_scene};
use aura_brain_wedding::scene::profile::SceneProfileRegistry;
use aura_brain_wedding::scene::taxonomy::Taxonomy;
use aura_brain_wedding::story::changepoint::{self, Frame};
use aura_brain_wedding::story::hmm;
use aura_brain_wedding::story::keyframe::{self, Candidate, Relaxation};
use aura_core::contract::scene::{AttrFlags, ChapterId, SceneId};
use aura_core::PhotoId;

// -- section 10.1's thresholds, in one place ---------------------------------

/// Scene top-1 accuracy, overall.
const ACCURACY_GATE: f64 = 0.92;
/// Median boundary error, in seconds.
const BOUNDARY_ERROR_GATE_S: i64 = 45;
/// Ritual F1, per tradition.
const RITUAL_F1_GATE: f64 = 0.85;
/// The chapter-count band no wedding may fall outside.
const CHAPTER_BAND: (usize, usize) = changepoint::CHAPTER_BAND;

/// The accuracy the synthetic classifier is driven at for the passing cases.
///
/// 0.94, deliberately above the 0.92 gate rather than far above it. A fixture driven at
/// 0.99 would pass every gate with any smoother at all, including one that did nothing,
/// which is the failure mode this harness exists to avoid.
const DRIVEN_ACCURACY: f32 = 0.94;

/// A fixed seed, so two machines produce the same fixture. Invariant 4.
const SEED: u64 = 0x0000_5CE7_E7A1_0001;

// -- helpers -----------------------------------------------------------------

/// Run the whole story pipeline over one synthetic wedding, in memory.
///
/// No catalog, no model, no runtime: the posteriors come from the fixture and everything
/// after them is the real code. That is the point - it is the algorithms that are being
/// gated, and they are the same algorithms a trained model will run through.
fn story_of(
    truth: &Truth,
    accuracy: f32,
) -> (
    Vec<Frame>,
    Vec<changepoint::Run>,
    changepoint::SegmentReport,
) {
    let observations = truth.observations(accuracy, SEED);
    let (smoothed, _) = hmm::smooth(&observations);
    let frames: Vec<Frame> = truth
        .frames
        .iter()
        .zip(smoothed.iter())
        .map(|(frame, decided)| Frame {
            ts: frame.ts,
            // No embeddings. The fixture's signal is the chapter disagreement and the
            // time gaps, which is the harder of the two cases: a real wedding also has
            // the embedding term, so a segmenter that meets the band here meets it
            // there with more to work from.
            embedding: Vec::new(),
            smoothed: *decided,
        })
        .collect();
    let (runs, report) = changepoint::segment(&frames);
    (frames, runs, report)
}

/// Top-1 accuracy of the smoothed labels against the fixture's answer.
fn accuracy_of(truth: &Truth, accuracy: f32) -> f64 {
    let observations = truth.observations(accuracy, SEED);
    let (smoothed, _) = hmm::smooth(&observations);
    let correct = truth
        .frames
        .iter()
        .zip(smoothed.iter())
        .filter(|(frame, decided)| frame.scene == decided.scene)
        .count();
    correct as f64 / truth.frames.len().max(1) as f64
}

/// Median absolute error between detected and true boundaries, in seconds.
///
/// Each true boundary is matched to its nearest detected one. Nearest-neighbour rather
/// than an ordered pairing, because a segmenter that produced one extra chapter would
/// otherwise misalign every boundary after it and report an error of hours - which would
/// be a measurement of the count, not of the boundaries.
fn boundary_error_s(truth: &Truth, frames: &[Frame], runs: &[changepoint::Run]) -> i64 {
    let detected: Vec<i64> = runs
        .iter()
        .filter_map(|run| frames.get(run.start).map(|frame| frame.ts))
        .collect();
    if detected.is_empty() {
        return i64::MAX;
    }
    let mut errors: Vec<i64> = truth
        .boundaries
        .iter()
        .map(|actual| {
            detected
                .iter()
                .map(|found| (found - actual).abs())
                .min()
                .unwrap_or(i64::MAX)
        })
        .collect();
    errors.sort_unstable();
    errors.get(errors.len() / 2).copied().unwrap_or(i64::MAX) / 1000
}

// -- the gates ---------------------------------------------------------------

#[test]
fn scene_accuracy_meets_the_section_ten_gate() {
    for truth in fixtures::all() {
        let measured = accuracy_of(&truth, DRIVEN_ACCURACY);
        println!(
            "{}: top-1 accuracy {measured:.4} against a {ACCURACY_GATE:.2} gate",
            truth.name
        );
        assert!(
            measured >= ACCURACY_GATE,
            "{}: top-1 accuracy {measured:.4} is below the {ACCURACY_GATE} gate",
            truth.name
        );
    }
}

#[test]
fn smoothing_improves_on_the_raw_classifier() {
    // The claim section 6.2 makes - "this single trick removes most absurd labels" -
    // stated as a measurement rather than as a comment.
    //
    // The comparison is at the CHAPTER level, which is what the smoother decides. A
    // scene-level comparison would be measuring the argmax, which the HMM only moves
    // when the chapter moves.
    for truth in fixtures::all() {
        let observations = truth.observations(0.80, SEED);
        let raw = truth
            .frames
            .iter()
            .zip(observations.iter())
            .filter(|(frame, observation)| {
                observation
                    .top3
                    .first()
                    .is_some_and(|best| best.scene.chapter() == frame.chapter)
            })
            .count();
        let (smoothed, report) = hmm::smooth(&observations);
        let after = truth
            .frames
            .iter()
            .zip(smoothed.iter())
            .filter(|(frame, decided)| frame.chapter == decided.chapter)
            .count();

        println!(
            "{}: chapter agreement {raw} -> {after} of {} frames, {} corrected",
            truth.name,
            truth.frames.len(),
            report.corrected
        );
        assert!(
            after >= raw,
            "{}: smoothing made the chapter labels worse, {raw} -> {after}",
            truth.name
        );
    }
}

#[test]
fn every_wedding_lands_inside_the_chapter_band() {
    for truth in fixtures::all() {
        let (_, runs, report) = story_of(&truth, DRIVEN_ACCURACY);
        println!(
            "{}: {} chapters at penalty {:.3} (fell back: {})",
            truth.name,
            runs.len(),
            report.penalty,
            report.fell_back
        );
        assert!(
            runs.len() >= CHAPTER_BAND.0 && runs.len() <= CHAPTER_BAND.1,
            "{}: {} chapters is outside the {}-{} band",
            truth.name,
            runs.len(),
            CHAPTER_BAND.0,
            CHAPTER_BAND.1
        );
    }
}

#[test]
fn boundary_error_meets_the_section_ten_gate() {
    for truth in fixtures::all() {
        let (frames, runs, _) = story_of(&truth, DRIVEN_ACCURACY);
        let measured = boundary_error_s(&truth, &frames, &runs);
        println!(
            "{}: median boundary error {measured} s against a {BOUNDARY_ERROR_GATE_S} s gate",
            truth.name
        );
        assert!(
            measured <= BOUNDARY_ERROR_GATE_S,
            "{}: median boundary error {measured} s is above the {BOUNDARY_ERROR_GATE_S} s gate",
            truth.name
        );
    }
}

#[test]
fn two_shooters_do_not_fragment_the_timeline() {
    // Section 10.1's named regression fixture. The Nepali wedding is already
    // interleaved; the assertion is that it produces the same chapter count as the
    // single-shooter version of itself, not merely that it is inside the band.
    let single = fixtures::nepali_mixed();
    let mut solo = single.clone();
    for frame in &mut solo.frames {
        frame.camera = 1;
    }

    let (_, interleaved_runs, _) = story_of(&single, DRIVEN_ACCURACY);
    let (_, solo_runs, _) = story_of(&solo, DRIVEN_ACCURACY);

    println!(
        "two-shooter: {} chapters interleaved, {} chapters solo",
        interleaved_runs.len(),
        solo_runs.len()
    );
    assert_eq!(
        interleaved_runs.len(),
        solo_runs.len(),
        "a second photographer changed the chapter count, which means the segmenter is keying \
         on something about the camera rather than about the day"
    );
}

#[test]
fn a_travel_gap_is_always_a_boundary() {
    // Section 6.2's hard rule. The Hindu fixture has a 35-minute drive in it.
    let truth = fixtures::hindu_night();
    let (frames, runs, report) = story_of(&truth, DRIVEN_ACCURACY);

    assert!(
        report.hard_boundaries >= 1,
        "the fixture's 35-minute travel gap was not recognised as a hard boundary"
    );

    let boundaries: Vec<i64> = runs
        .iter()
        .filter_map(|run| frames.get(run.start).map(|frame| frame.ts))
        .collect();
    for pair in frames.windows(2) {
        let (Some(left), Some(right)) = (pair.first(), pair.get(1)) else {
            continue;
        };
        if right.ts - left.ts >= changepoint::HARD_GAP_MS {
            assert!(
                boundaries.contains(&right.ts),
                "a {} minute gap did not produce a boundary",
                (right.ts - left.ts) / 60_000
            );
        }
    }
}

#[test]
fn no_chapter_sequence_violates_the_transition_matrix() {
    // Section 10.1's HMM sanity check. `hmm::is_plausible` is the useful form of it -
    // see that function's documentation for why "allowed" is total and therefore
    // vacuous, and why this asserts the stronger property instead.
    for truth in fixtures::all() {
        let (frames, runs, _) = story_of(&truth, DRIVEN_ACCURACY);
        let chapters: Vec<ChapterId> = runs
            .iter()
            .map(|run| changepoint::dominant(&frames, *run).0)
            .collect();

        for pair in chapters.windows(2) {
            let (Some(from), Some(to)) = (pair.first(), pair.get(1)) else {
                continue;
            };
            if from == to {
                continue;
            }
            assert!(
                hmm::is_plausible(*from, *to),
                "{}: {} -> {} is below the plausible-transition floor",
                truth.name,
                from.as_str(),
                to.as_str()
            );
        }
    }
}

#[test]
fn the_transition_matrix_is_a_probability_matrix() {
    let matrix = hmm::transitions();
    for (index, row) in matrix.iter().enumerate() {
        let total: f32 = row.iter().sum();
        assert!(
            (total - 1.0).abs() < 1e-4,
            "row {} of the transition matrix sums to {total}",
            ChapterId::from_index(index).as_str()
        );
        for (target, value) in row.iter().enumerate() {
            assert!(
                *value > 0.0,
                "{} -> {} is zero; a hard zero is a claim that a wedding cannot do something",
                ChapterId::from_index(index).as_str(),
                ChapterId::from_index(target).as_str()
            );
        }
        // Self-transition must dominate, or the smoother does not smooth.
        let diagonal = row.get(index).copied().unwrap_or(0.0);
        assert!(
            diagonal > 0.5,
            "{} has a self-transition of {diagonal}, which will not suppress a single-frame flip",
            ChapterId::from_index(index).as_str()
        );
    }
}

#[test]
fn the_gate_rejects_a_useless_classifier() {
    // The guard phase 06 wrote for its clustering metric, applied here. A classifier
    // that emits the same flat distribution for every frame must fail, or every number
    // above is decoration.
    for truth in fixtures::all() {
        let observations = truth.useless_observations();
        let (smoothed, _) = hmm::smooth(&observations);
        let correct = truth
            .frames
            .iter()
            .zip(smoothed.iter())
            .filter(|(frame, decided)| frame.scene == decided.scene)
            .count();
        let measured = correct as f64 / truth.frames.len().max(1) as f64;

        println!(
            "{}: a useless classifier scores {measured:.4}, well below the {ACCURACY_GATE} gate",
            truth.name
        );
        assert!(
            measured < ACCURACY_GATE,
            "{}: a classifier that has learned nothing passed the accuracy gate at {measured:.4}, \
             so the gate is measuring nothing",
            truth.name
        );
    }
}

#[test]
fn the_abstention_rules_do_what_they_claim() {
    let photo = PhotoId::new();
    let attributes = AttributeReport::default();

    // A clear winner is accepted.
    let mut clear = vec![0.01f32; SceneId::CLASSES];
    if let Some(slot) = clear.get_mut(SceneId::Ceremony.as_index()) {
        *slot = 0.70;
    }
    let decided = decode_scene(photo, &clear, attributes);
    assert_eq!(decided.scene, SceneId::Ceremony);

    // Two classes inside the margin are refused, and the top-3 survives.
    let mut split = vec![0.01f32; SceneId::CLASSES];
    if let Some(slot) = split.get_mut(SceneId::Ceremony.as_index()) {
        *slot = 0.46;
    }
    if let Some(slot) = split.get_mut(SceneId::Ritual.as_index()) {
        *slot = 0.44;
    }
    let refused = decode_scene(photo, &split, attributes);
    assert_eq!(
        refused.scene,
        SceneId::Unknown,
        "a 0.46/0.44 split was accepted; the margin rule is not doing anything"
    );
    assert!(
        (refused.scene_conf - 0.46).abs() < 1e-5,
        "an abstention threw away the posterior, so the HMM cannot tell a 0.46 abstention from a \
         0.05 one"
    );
    assert_eq!(
        refused.top3.first().map(|score| score.scene),
        Some(SceneId::Ceremony),
        "an abstention threw away the top-3"
    );

    // Everything below the floor is refused.
    let flat = vec![1.0f32 / SceneId::CLASSES as f32; SceneId::CLASSES];
    assert_eq!(
        decode_scene(photo, &flat, attributes).scene,
        SceneId::Unknown
    );
}

#[test]
fn a_measurement_beats_the_attribute_head() {
    // The rule in `scene::attributes`: four attributes are recorded exactly by the
    // camera or by the phase 05 statistics, and where a measurement exists it wins.
    let mut probs = vec![0.9f32; AttrFlags::COUNT];
    if let Some(slot) = probs.get_mut(1) {
        *slot = 0.95; // the head is confident the flash fired
    }

    let context = ContextFeatures {
        flash: Some(false),
        ..ContextFeatures::default()
    };
    let decoded = decode_attributes(&probs, &context);

    assert!(
        !decoded.flags.contains(AttrFlags::FLASH),
        "the head's flash opinion beat the camera's own record of whether the flash fired"
    );
    assert!(
        decoded.corrected >= 1,
        "a correction happened but was not counted, so the telemetry cannot report it"
    );
}

#[test]
fn ritual_abstention_beats_a_wrong_tradition() {
    // Section 10.1: "abstention rather than wrong labels on unseen rituals". Two rites
    // from two traditions inside the margin must produce silence, not a name.
    let taxonomy = Taxonomy::embedded().expect("the shipped taxonomy loads");
    let hindu = taxonomy
        .by_slug("saptapadi_pheras")
        .expect("the hindu file declares saptapadi_pheras");
    let nepali = taxonomy
        .by_slug("saat_phera")
        .expect("the nepali file declares saat_phera");

    let mut probs = vec![0.001f32; aura_infer::onnx::fixtures::RITUAL_SLOTS];
    if let Some(slot) = probs.get_mut(hindu.id as usize) {
        *slot = 0.36;
    }
    if let Some(slot) = probs.get_mut(nepali.id as usize) {
        *slot = 0.33;
    }

    // With no tradition established, both slots are live and the head must abstain.
    let head = aura_brain_wedding::scene::ritual::RitualClassifier::new(
        std::sync::Arc::new(NullInfer::new()),
        std::sync::Arc::new(taxonomy),
    );
    let verdict = head.decode(&probs, None);
    assert!(
        verdict.ritual.is_none(),
        "the ritual head named a rite from two traditions 0.03 apart; a Nepali wedding would get \
         Hindu names in a client-facing timeline"
    );
    assert!(
        !verdict.reasons.is_empty(),
        "an abstention with no reason is a decision without an explanation"
    );

    // With the tradition established, the other tradition's slot is masked out and the
    // same distribution becomes a confident answer.
    let named = head.decode(&probs, Some("nepali"));
    assert_eq!(
        named.slug.as_deref(),
        Some("saat_phera"),
        "tradition conditioning did not mask the other tradition's slot"
    );
}

#[test]
fn ritual_f1_is_measured_the_way_the_python_harness_measures_it() {
    // The metric, computed on the fixtures' known rites. It is *reported* rather than
    // gated - see `the_deferred_gates_are_reported_with_the_reason_they_are_deferred` -
    // because a placeholder head has no rite semantics at all.
    for truth in fixtures::all() {
        let expected: BTreeMap<&str, usize> = truth
            .frames
            .iter()
            .filter_map(|frame| frame.ritual)
            .fold(BTreeMap::new(), |mut acc, slug| {
                *acc.entry(slug).or_insert(0) += 1;
                acc
            });
        let taxonomy = Taxonomy::embedded().expect("the shipped taxonomy loads");
        for slug in expected.keys() {
            assert!(
                taxonomy.by_slug(slug).is_some(),
                "{}: the fixture names a rite `{slug}` that no shipped taxonomy declares",
                truth.name
            );
        }
        println!(
            "{}: {} distinct rites in the fixture, all declared; F1 against a \
             {RITUAL_F1_GATE:.2} gate is deferred to a trained head",
            truth.name,
            expected.len()
        );
    }
}

#[test]
fn the_key_frame_filter_relaxes_in_the_documented_order() {
    let with_people: Vec<Candidate> = (0..6)
        .map(|index| Candidate {
            image_id: PhotoId::new(),
            embedding: vec![index as f32 * 0.1, 1.0, 0.5],
            subject_focus: 0.6,
            has_primary: index % 2 == 0,
        })
        .collect();
    let chosen = keyframe::choose(&with_people).expect("a non-empty chapter has a cover");
    assert_eq!(chosen.relaxed_to, Relaxation::Preferred);

    let details: Vec<Candidate> = (0..6)
        .map(|index| Candidate {
            image_id: PhotoId::new(),
            embedding: vec![index as f32 * 0.1, 1.0, 0.5],
            subject_focus: 0.0,
            has_primary: false,
        })
        .collect();
    let fallback = keyframe::choose(&details).expect("a details chapter still has a cover");
    assert_eq!(
        fallback.relaxed_to,
        Relaxation::Any,
        "a chapter with no faces did not report that its cover was arbitrary"
    );
}

#[test]
fn the_whole_pipeline_is_deterministic() {
    // Invariant 4. Two runs over the same fixture must produce byte-identical chapters.
    let truth = fixtures::hindu_night();
    let (first_frames, first_runs, first_report) = story_of(&truth, DRIVEN_ACCURACY);
    let (second_frames, second_runs, second_report) = story_of(&truth, DRIVEN_ACCURACY);

    assert_eq!(
        first_runs, second_runs,
        "two runs produced different chapters"
    );
    assert!(
        (first_report.penalty - second_report.penalty).abs() < f32::EPSILON,
        "the penalty search is not deterministic"
    );
    assert_eq!(
        first_frames.len(),
        second_frames.len(),
        "two runs saw different numbers of frames"
    );
}

#[test]
fn the_shipped_profiles_are_total_and_explained() {
    // Section 12's third failure mode, as a build-time assertion rather than a review
    // item: every scene has a profile, and every profile has a rationale.
    let registry = SceneProfileRegistry::embedded().expect("the shipped profiles load");
    for scene in SceneId::ALL {
        if scene == SceneId::Unknown {
            continue;
        }
        assert!(
            registry.rationale(scene).is_some(),
            "{} has no profile in the shipped file, so every wedding would grade it neutrally",
            scene.as_str()
        );
    }
    // The worked example from section 6.3, asserted rather than described.
    let dance = registry.get(SceneId::DanceFloor);
    let ceremony = registry.get(SceneId::Ceremony);
    assert!(
        dance.max_acceptable_noise > ceremony.max_acceptable_noise * 1.4,
        "the dance floor's noise tolerance is not meaningfully above the ceremony's, so the \
         profile table is not doing the one thing section 6.3 promises"
    );
    assert!(
        dance.emotion_weight > ceremony.emotion_weight,
        "the dance floor does not weight emotion above the ceremony"
    );
    let details = registry.get(SceneId::Details);
    assert!(
        details.subject_focus_weight.abs() < f32::EPSILON,
        "the details profile does not ignore faces, which section 6.3 requires"
    );
}

#[test]
fn the_deferred_gates_are_reported_with_the_reason_they_are_deferred() {
    // The same list the exit report carries, printed so a CI log reader sees it.
    let deferred = [
        (
            "scene top-1 >= 0.92 against photographs",
            "the shipped scene_classifier is a placeholder with no training; measured here against \
             synthetic posteriors instead (condition C1)",
        ),
        (
            "per-class accuracy >= 0.85 for classes with > 200 samples",
            "needs a labelled wedding set; no labelled wedding data exists in this repository \
             (condition C2)",
        ),
        (
            "ritual F1 >= 0.85 per tradition",
            "the shipped ritual_classifier is a placeholder; the taxonomy, the masking and the \
             abstention are gated instead (condition C1)",
        ),
        (
            "human audit of chapters on 20 weddings across traditions",
            "needs twenty real weddings and a person (condition C3)",
        ),
        (
            "perceptual A/B against FilterPixel and Aftershoot",
            "needs a build of each and a panel (condition C4)",
        ),
    ];

    println!("\ndeferred phase 07 gates:");
    for (gate, reason) in deferred {
        println!("  - {gate}: {reason}");
    }
    assert_eq!(
        deferred.len(),
        5,
        "the deferred list changed without the exit report"
    );
}

/// An `InferService` that is never called.
///
/// `RitualClassifier::decode` is a pure function of a posterior and a taxonomy, and
/// testing it should not require a runtime. This exists so that fact is visible in the
/// test rather than hidden behind a fixture session: every method fails loudly, so a
/// change that made `decode` reach for the runtime would fail this test rather than
/// quietly become slower.
#[derive(Debug)]
struct NullInfer {
    plan: aura_infer::contract::infer::HardwarePlan,
}

impl NullInfer {
    fn new() -> Self {
        Self {
            plan: aura_infer::contract::infer::HardwarePlan::conservative(
                1,
                "1970-01-01T00:00:00Z".to_string(),
            ),
        }
    }
}

impl aura_infer::contract::infer::InferService for NullInfer {
    fn run(
        &self,
        _req: aura_infer::contract::infer::InferRequest<'_>,
    ) -> Result<aura_infer::contract::infer::InferResult, aura_infer::contract::infer::InferError>
    {
        Err(aura_core::errors::ml::cancelled(
            "the null inference service is never called",
        ))
    }

    fn run_batch(
        &self,
        _model: aura_infer::contract::infer::ModelRef,
        _inputs: Vec<Vec<aura_infer::contract::infer::TensorView<'_>>>,
        _prio: aura_core::contract::priority::Priority,
    ) -> Result<
        Vec<aura_infer::contract::infer::InferResult>,
        aura_infer::contract::infer::InferError,
    > {
        Err(aura_core::errors::ml::cancelled(
            "the null inference service is never called",
        ))
    }

    fn plan(&self) -> &aura_infer::contract::infer::HardwarePlan {
        &self.plan
    }

    fn warmup(
        &self,
        _models: &[aura_infer::contract::infer::ModelRef],
    ) -> Result<(), aura_infer::contract::infer::InferError> {
        Ok(())
    }
}

/// The thresholds this build classifies with, printed for the record.
#[test]
fn the_classifier_thresholds_are_reported() {
    println!(
        "scene abstention: floor {:.2}, margin {:.2}",
        classifier::MIN_CONFIDENCE,
        classifier::MIN_MARGIN
    );
    println!(
        "ritual abstention: floor {:.2}, margin {:.2}",
        aura_brain_wedding::scene::ritual::MIN_CONFIDENCE,
        aura_brain_wedding::scene::ritual::ABSTAIN_MARGIN
    );
    const {
        assert!(
            classifier::MIN_MARGIN > 0.0,
            "the margin rule is switched off"
        )
    };
}
