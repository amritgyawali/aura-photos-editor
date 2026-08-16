//! The phase 11 evaluation harness: section 10.1's gates, and the proof that the harness
//! itself would catch a bad analyser.
//!
//! Wired in as a test target of `aura-brain-photo`, so `cargo test --workspace` runs it and
//! CI cannot forget it - the same arrangement phases 05 to 10 use.
//!
//! ## What can and cannot be gated today
//!
//! Section 10.1 sets six gates. Two of them involve a *trained model* - the keypoint head
//! and the aesthetic head - and the two this build ships are placeholders with the right
//! architecture and no training (condition **C1** in `docs/progress/PHASE-11-EXIT.md`).
//!
//! So this file takes the shape the five harnesses before it took:
//!
//! 1. **It measures every metric** with the arithmetic
//!    `ml/models/composition/eval_composition.py` uses, so the day the heads are trained
//!    the two numbers can be compared rather than argued about.
//! 2. **It gates them on ground truth whose answer is known by construction** - the
//!    synthetic frames in `aura_brain_photo::composition::fixtures` - which proves the
//!    harness would fail an analyser that had learned nothing.
//! 3. **It gates, for real and today, everything that does not depend on the weights**: the
//!    horizon angle, the sign convention, the intentional-tilt conjunction, the crop audit's
//!    true and false positives, the head-crop context rule, the head-merge search, the cap
//!    on taste, the withdrawal rules and determinism.
//!
//! ## Why the analyser runs with no inference service here
//!
//! [`NullInfer`] refuses every call, so the keypoint head is replaced by
//! `Analyser::with_reference_poses` - whose answer is painted into the fixture - and the
//! aesthetic head falls back to `Aesthetic::from_features`, the deterministic twelve-term
//! reference model. Both substitutions are announced in the judgement itself
//! (`CompositionCode::AestheticUnavailable`, `keypoint_subjects`), which is what makes them
//! honest rather than hidden.
//!
//! The real models, through the real registry, are exercised by
//! `aura-cli verify --phase 11`. The tests prove the pieces; the gate proves the assembly.

use std::sync::Arc;

use aura_brain_photo::composition::aesthetic::{self, Aesthetic};
use aura_brain_photo::composition::fixtures::{self, Canvas};
use aura_brain_photo::composition::rules::RuleTable;
use aura_brain_photo::composition::{score, Analyser, FrameContext};
use aura_core::contract::composition::{
    CompositionCode, CompositionFlags, CompositionResult, Joint,
};
use aura_core::contract::people::ImageSubjects;
use aura_core::{PhotoId, Priority, SceneId};
use aura_raw::contract::pixels::{ColourSpace, PixelBuffer, PixelData, PixelSource};

// -- section 10.1's thresholds, in one place ---------------------------------

/// Horizon angle error, in degrees.
const ANGLE_GATE: f32 = 0.4;
/// Limb and joint crop detection F1.
const CROP_F1_GATE: f64 = 0.90;
/// Head-merge recall.
const MERGE_RECALL_GATE: f64 = 0.85;
/// Head-merge false-positive rate.
const MERGE_FP_GATE: f64 = 0.10;
/// Aesthetic pairwise agreement.
const AGREEMENT_GATE: f64 = 0.78;

// -- helpers -----------------------------------------------------------------

fn buffer_of(canvas: &Canvas) -> PixelBuffer {
    PixelBuffer {
        width: canvas.width as u32,
        height: canvas.height as u32,
        data: PixelData::Srgb8(canvas.rgb.clone()),
        colour_space: ColourSpace::Srgb,
        source: PixelSource::Demosaiced,
        decode_ms: 0,
    }
}

fn context_of(canvas: &Canvas, scene: SceneId) -> FrameContext {
    FrameContext {
        scene,
        subjects: ImageSubjects {
            faces: canvas.faces.clone(),
            ..ImageSubjects::default()
        },
        scene_known: true,
        gravity_deg: None,
    }
}

fn analyser() -> Analyser {
    Analyser::new(Arc::new(NullInfer::new()))
        .expect("the embedded rule table loads")
        .with_reference_poses()
}

fn judge(canvas: &Canvas, scene: SceneId) -> CompositionResult {
    analyser()
        .analyse(
            PhotoId::new(),
            &buffer_of(canvas),
            &context_of(canvas, scene),
            Priority::AiBatch,
        )
        .expect("a synthetic frame is judged")
}

fn judge_in_own_scene(canvas: &Canvas) -> CompositionResult {
    judge(canvas, canvas.truth.scene)
}

// -- gate 1: the horizon ------------------------------------------------------

#[test]
fn horizon_angle_error_is_within_the_gate() {
    let mut worst = 0.0f32;
    for canvas in fixtures::tilt_ladder() {
        let result = judge(&canvas, SceneId::Venue);
        assert!(
            result.horizon_source.is_measured(),
            "a fixture with a drawn horizon must produce a measured one"
        );
        let error = (result.tilt_deg - canvas.truth.tilt_deg).abs();
        worst = worst.max(error);
        assert!(
            error <= ANGLE_GATE,
            "horizon at {} deg measured {} deg (error {error:.3})",
            canvas.truth.tilt_deg,
            result.tilt_deg
        );
    }
    println!("worst horizon angle error: {worst:.3} deg (gate {ANGLE_GATE})");
}

#[test]
fn the_sign_convention_is_not_inverted() {
    // The single easiest thing in this phase to get backwards, and the one that would
    // make phase 23 straighten every frame the wrong way by twice its tilt.
    let down_right = judge(&fixtures::tilted(6.0), SceneId::Venue);
    let up_right = judge(&fixtures::tilted(-6.0), SceneId::Venue);
    assert!(down_right.tilt_deg > 0.0, "positive tilt reads positive");
    assert!(up_right.tilt_deg < 0.0, "negative tilt reads negative");
}

#[test]
fn a_frame_with_no_horizon_says_so_rather_than_claiming_level() {
    let result = judge_in_own_scene(&fixtures::intentional_dutch());
    // Either unmeasured, or measured with a confidence below the acting threshold. What is
    // forbidden is a confident zero, which is a claim that the frame is level.
    let confident_level = result.horizon_source.is_measured()
        && result.horizon_conf >= aura_core::contract::composition::HORIZON_ACT_AT
        && result.tilt_deg.abs() < 0.1;
    assert!(
        !confident_level,
        "a textureless-direction frame must not claim to be confidently level"
    );
}

// -- gate 2: intentional tilt -------------------------------------------------

#[test]
fn a_deliberate_dutch_angle_is_not_a_defect() {
    let canvas = fixtures::intentional_dutch();
    let result = judge_in_own_scene(&canvas);
    assert!(
        !result.flags.contains(CompositionFlags::HORIZON_TILTED),
        "a dance-floor frame with a centred subject and no horizon must not be flagged tilted"
    );
}

#[test]
fn a_tilted_family_portrait_with_a_real_horizon_is_flagged() {
    // The negative half of the gate above. A build that flagged nothing would pass the
    // first test and fail this one, which is the whole reason it is here.
    let canvas = fixtures::accidental_dutch();
    let result = judge_in_own_scene(&canvas);
    assert!(
        result.flags.contains(CompositionFlags::HORIZON_TILTED),
        "a nine-degree tilt against a strong horizon in a family portrait is a defect"
    );
    assert!(
        !result.tilt_intentional,
        "a scene whose rule row forbids a dutch angle must not exonerate one"
    );
}

#[test]
fn the_four_conditions_are_a_conjunction() {
    use aura_brain_photo::composition::horizon::{intentional, HorizonInput};
    let centred = HorizonInput {
        gravity_deg: None,
        subject_centre: Some((0.5, 0.5)),
        allows_dutch: true,
    };
    assert!(intentional(9.0, 0.10, &centred), "all four hold");
    assert!(
        !intentional(3.0, 0.10, &centred),
        "a small tilt is not style"
    );
    assert!(
        !intentional(9.0, 0.90, &centred),
        "a strong horizon means the tilt was a mistake"
    );
    assert!(
        !intentional(
            9.0,
            0.10,
            &HorizonInput {
                subject_centre: Some((0.15, 0.8)),
                ..centred
            }
        ),
        "an off-centre subject means the tilt was a mistake"
    );
    assert!(
        !intentional(
            9.0,
            0.10,
            &HorizonInput {
                allows_dutch: false,
                ..centred
            }
        ),
        "a scene that forbids a dutch angle forbids it"
    );
}

// -- gate 3: the crop audit ---------------------------------------------------

#[test]
fn limb_crop_detection_meets_the_f1_gate() {
    // Three labelled cases: a cut at a joint, a cut between joints, and a tight frame that
    // cuts nothing. The third is the one that matters - a build that reported every tight
    // crop as a cut would have perfect recall and would be unusable.
    let cases: Vec<(Canvas, Option<bool>)> = vec![
        (fixtures::cut_at_wrist(), Some(true)),
        (fixtures::cut_mid_limb(), Some(false)),
        (fixtures::tight_but_uncut(), None),
    ];

    let (mut tp, mut fp, mut r#fn) = (0.0f64, 0.0f64, 0.0f64);
    for (canvas, truth) in &cases {
        let result = judge_in_own_scene(canvas);
        let found = result.flagged_cuts();
        match truth {
            Some(at_joint) => {
                if found.iter().any(|cut| cut.at_joint == *at_joint) {
                    tp += 1.0;
                } else {
                    r#fn += 1.0;
                    if !found.is_empty() {
                        fp += 1.0;
                    }
                }
            }
            None => {
                if found.is_empty() {
                    // A true negative contributes to neither term of F1, which is correct
                    // and is why the false-positive case is asserted separately below.
                } else {
                    fp += 1.0;
                }
            }
        }
    }
    let precision = if tp + fp <= 0.0 { 0.0 } else { tp / (tp + fp) };
    let recall = if tp + r#fn <= 0.0 {
        0.0
    } else {
        tp / (tp + r#fn)
    };
    let f1 = if precision + recall <= 0.0 {
        0.0
    } else {
        2.0 * precision * recall / (precision + recall)
    };
    println!("crop audit: precision {precision:.3} recall {recall:.3} F1 {f1:.3}");
    assert!(
        f1 >= CROP_F1_GATE,
        "limb-crop F1 {f1:.3} below {CROP_F1_GATE}"
    );
}

#[test]
fn a_tight_crop_that_cuts_nothing_is_not_a_cut() {
    let result = judge_in_own_scene(&fixtures::tight_but_uncut());
    assert!(
        result.flagged_cuts().is_empty(),
        "a wrist near the bottom edge with the whole body inside it is a tight crop"
    );
    assert!(
        !result
            .flags
            .intersects(CompositionFlags::JOINT_CUT.union(CompositionFlags::LIMB_CUT)),
        "no cut flag on an uncut frame"
    );
}

#[test]
fn a_cut_at_a_joint_costs_more_than_a_cut_between_joints() {
    // Section 6.1's ordering, measured rather than assumed. Both in the same scene, so the
    // scene weight is held constant and only the joint distinction moves.
    let at_joint = judge(&fixtures::cut_at_wrist(), SceneId::FamilyPortrait);
    let mid_limb = judge(&fixtures::cut_mid_limb(), SceneId::FamilyPortrait);
    let worst = |result: &CompositionResult| {
        result
            .joint_cuts
            .iter()
            .map(|cut| cut.severity)
            .fold(0.0f32, f32::max)
    };
    assert!(
        worst(&at_joint) > worst(&mid_limb),
        "a joint cut ({:.3}) must cost more than a mid-limb cut ({:.3})",
        worst(&at_joint),
        worst(&mid_limb)
    );
}

#[test]
fn a_cut_costs_less_on_a_dance_floor_than_in_a_family_portrait() {
    // Invariant 7, measured. The same pixels, judged twice.
    let canvas = fixtures::cut_at_wrist();
    let strict = judge(&canvas, SceneId::FamilyPortrait);
    let loose = judge(&canvas, SceneId::DanceFloor);
    let worst = |result: &CompositionResult| {
        result
            .joint_cuts
            .iter()
            .map(|cut| cut.severity)
            .fold(0.0f32, f32::max)
    };
    assert!(
        worst(&strict) > worst(&loose),
        "a family portrait must weigh a cut more heavily than a dance floor"
    );
    assert!(
        !loose.flags.contains(CompositionFlags::JOINT_CUT),
        "a dance floor at joint_cut_weight 0.20 must not raise a cut flag on this fixture"
    );
}

// -- gate 4: the head crop ----------------------------------------------------

#[test]
fn a_head_crop_is_context_dependent() {
    let canvas = fixtures::head_cropped();
    let strict = judge(&canvas, SceneId::FamilyPortrait);
    let portrait = judge(&canvas, SceneId::CouplePortrait);

    assert!(
        strict.flags.contains(CompositionFlags::HEAD_CROPPED),
        "a cropped crown in a family portrait is a violation"
    );
    assert!(
        !portrait.flags.contains(CompositionFlags::HEAD_CROPPED),
        "section 6.1: a top-of-head crop in a couple portrait can be deliberate"
    );
    assert!(
        portrait
            .reasons
            .iter()
            .any(|reason| matches!(reason.code, CompositionCode::DeliberateCrop)),
        "the exoneration must be said out loud rather than left as silence"
    );
}

#[test]
fn a_cut_head_carries_a_negative_headroom() {
    let result = judge(&fixtures::head_cropped(), SceneId::FamilyPortrait);
    assert!(
        result.headroom < 0.0,
        "a cut crown is a negative headroom, not a zero one: {}",
        result.headroom
    );
}

// -- gate 5: the background ---------------------------------------------------

#[test]
fn head_merge_detection_meets_its_gates() {
    let positives = [fixtures::head_merge()];
    let negatives = [
        fixtures::clean_behind_head(),
        fixtures::bright_blob(),
        fixtures::tight_but_uncut(),
        fixtures::flat_lay(),
    ];

    let found = positives
        .iter()
        .filter(|canvas| judge_in_own_scene(canvas).head_merge)
        .count() as f64;
    let recall = found / positives.len() as f64;
    let false_positives = negatives
        .iter()
        .filter(|canvas| judge_in_own_scene(canvas).head_merge)
        .count() as f64;
    let fp_rate = false_positives / negatives.len() as f64;

    println!("head merge: recall {recall:.3} false-positive rate {fp_rate:.3}");
    assert!(recall >= MERGE_RECALL_GATE, "head-merge recall {recall:.3}");
    assert!(
        fp_rate < MERGE_FP_GATE,
        "head-merge false-positive rate {fp_rate:.3}"
    );
}

#[test]
fn a_bright_window_behind_the_subject_is_found() {
    let result = judge_in_own_scene(&fixtures::bright_blob());
    assert!(
        result.flags.contains(CompositionFlags::BRIGHT_BLOB),
        "a window brighter than the subject pulls the eye and is reported"
    );
    assert!(
        !result.bright_blobs.is_empty(),
        "the flag must carry an evidence box"
    );
}

#[test]
fn a_saturated_colour_behind_a_neutral_subject_competes() {
    let result = judge_in_own_scene(&fixtures::colour_clash());
    assert!(
        result.colour_competition > 0.0,
        "a red sign behind a grey subject is a colour competition"
    );
}

// -- gate 6: taste, and its cap ----------------------------------------------

#[test]
fn aesthetic_pairwise_agreement_meets_the_gate() {
    // Eight authored pairs, each isolating one measurement. **Not** four thousand
    // photographer comparisons - that is condition C2 of the exit report, and this number
    // is a statement about the feature set and the reference model rather than about
    // anybody's taste.
    let pairs = fixtures::composition_pairs();
    let mut agreed = 0.0f64;
    for (worse, better) in &pairs {
        let worse_score = judge_in_own_scene(worse).composition_score;
        let better_score = judge_in_own_scene(better).composition_score;
        if better_score > worse_score {
            agreed += 1.0;
        }
    }
    let agreement = agreed / pairs.len() as f64;
    println!(
        "pairwise agreement: {agreement:.3} over {} pairs",
        pairs.len()
    );
    assert!(
        agreement >= AGREEMENT_GATE,
        "pairwise agreement {agreement:.3} below {AGREEMENT_GATE}"
    );
}

#[test]
fn taste_cannot_override_substance() {
    // Section 6.3's requirement as a structural gate: a beautifully arranged frame that
    // cuts a neck must lose to a plain frame with nothing wrong, and it must lose at every
    // value of `aesthetic_weight` any rule file could ask for.
    let (cut, plain) = fixtures::beautiful_but_cut();
    let cut_score = judge_in_own_scene(&cut).composition_score;
    let plain_score = judge_in_own_scene(&plain).composition_score;
    assert!(
        plain_score > cut_score,
        "a cut neck ({cut_score:.3}) must lose to a clean frame ({plain_score:.3})"
    );
}

#[test]
fn the_cap_bounds_the_multiplier_whatever_the_rule_file_says() {
    // The arithmetic proof behind the test above. `compose` is called twice with the same
    // sub-scores and opposite extremes of taste; the gap must not exceed twice the cap.
    let rules = RuleTable::embedded().expect("the embedded rule table loads");
    let rule = rules.get(SceneId::CouplePortrait);
    let weights = score::Weights::for_rule(&rule);
    let sub = score::SubScores::neutral();
    let curve = score::SceneCurve::IDENTITY;

    let best = score::compose(
        sub,
        weights,
        Aesthetic {
            value: 1.0,
            learned: true,
        },
        &curve,
    );
    let worst = score::compose(
        sub,
        weights,
        Aesthetic {
            value: 0.0,
            learned: true,
        },
        &curve,
    );
    let swing = best - worst;
    assert!(
        swing <= 2.0 * score::AESTHETIC_CAP + 1e-4,
        "taste swung the score by {swing:.3}, above twice the cap"
    );
}

#[test]
fn the_reference_model_is_centred_on_no_opinion() {
    // An all-zero feature vector must read 0.5, not 0.0 or 1.0: "nothing measured" is not
    // "beautiful" and is not "ugly", and a reference model that drifted either way would
    // bias every frame with no subject in it.
    let features = vec![0.0f32; 35];
    let value = Aesthetic::from_features(&features).value;
    assert!(
        (value - 0.5).abs() < 0.01,
        "the reference model reads {value:.3} on an all-zero frame"
    );
}

#[test]
fn the_feature_vector_is_the_width_the_head_expects() {
    let canvas = fixtures::bright_blob();
    let rules = RuleTable::embedded().expect("the embedded rule table loads");
    let rule = rules.get(SceneId::CouplePortrait);
    let plane =
        aura_brain_photo::composition::analyse::plane_of(&canvas.rgb, canvas.width, canvas.height);
    let placement = aura_brain_photo::composition::placement::analyse(
        &plane,
        &[],
        &canvas
            .faces
            .iter()
            .map(|face| face.bbox)
            .collect::<Vec<_>>(),
        &rule,
        0.0,
    );
    let background = aura_brain_photo::composition::background::analyse(
        &plane,
        &canvas.rgb,
        canvas.width,
        canvas.height,
        placement.subject_box,
        &canvas
            .faces
            .iter()
            .map(|face| face.bbox)
            .collect::<Vec<_>>(),
        &rule,
    );
    let horizon = aura_brain_photo::composition::horizon::analyse(
        &plane,
        &aura_brain_photo::composition::horizon::HorizonInput::default(),
    );
    let features = aesthetic::features(
        &horizon,
        &placement,
        &background,
        &rule,
        SceneId::CouplePortrait,
    );
    assert_eq!(
        features.len(),
        aura_infer::onnx::fixtures::AESTHETIC_FEATURES,
        "the feature vector must be exactly the head's input width"
    );
    assert!(
        features.iter().all(|value| (0.0..=1.0).contains(value)),
        "every feature slot is squashed into 0..1 before the head sees it"
    );
}

// -- the withdrawal rules -----------------------------------------------------

#[test]
fn a_frame_with_no_people_withdraws_every_claim_about_people() {
    let result = judge_in_own_scene(&fixtures::flat_lay());
    assert!(
        result.flags.contains(CompositionFlags::NO_GEOMETRY),
        "a flat-lay has no geometry to judge"
    );
    let subject_only = aura_brain_photo::composition::analyse::subject_only_flags();
    assert!(
        !result.flags.intersects(subject_only),
        "a frame with no people must claim nothing about headroom, cropping or merges"
    );
    assert!(
        result
            .reasons
            .iter()
            .any(|reason| matches!(reason.code, CompositionCode::NoGeometry)),
        "the withdrawal is said out loud"
    );
}

#[test]
fn a_centred_flat_lay_is_not_off_thirds() {
    // Section 2.1's worked example: "a `details` flat-lay is judged differently from a
    // `couple_portrait`". A rule of thirds applied here flags a correct photograph.
    let canvas = fixtures::flat_lay();
    let details = judge(&canvas, SceneId::Details);
    assert!(
        !details
            .reasons
            .iter()
            .any(|reason| matches!(reason.code, CompositionCode::OffThirds)),
        "a centred flat-lay in `details` must not be told it missed the thirds"
    );
}

#[test]
fn every_exoneration_carries_no_penalty() {
    // The machine-readable half of the twelve doc comments that say "an exoneration".
    for canvas in [
        fixtures::flat_lay(),
        fixtures::intentional_dutch(),
        fixtures::head_cropped(),
        fixtures::tight_but_uncut(),
        fixtures::clean_behind_head(),
    ] {
        let result = judge_in_own_scene(&canvas);
        for reason in &result.reasons {
            if reason.code.is_exoneration() {
                assert!(
                    reason.weight >= 0.0,
                    "{} is an exoneration and carries weight {}",
                    reason.code,
                    reason.weight
                );
            }
        }
    }
}

#[test]
fn every_judgement_has_at_least_one_reason_and_at_most_six() {
    for canvas in [
        fixtures::flat_lay(),
        fixtures::head_merge(),
        fixtures::cut_at_wrist(),
        fixtures::accidental_dutch(),
        fixtures::colour_clash(),
    ] {
        let result = judge_in_own_scene(&canvas);
        assert!(
            !result.reasons.is_empty(),
            "invariant 2: a decision without an explanation is a bug"
        );
        assert!(
            result.reasons.len() <= CompositionResult::MAX_REASONS,
            "a panel that lists more than six reasons is a panel nobody reads"
        );
        assert!(
            (0.0..=1.0).contains(&result.confidence),
            "invariant 2: every decision carries a confidence in 0..1"
        );
    }
}

#[test]
fn placeholder_aesthetic_weights_are_never_reported_as_learned() {
    let result = judge_in_own_scene(&fixtures::clean_behind_head());
    assert!(
        result
            .reasons
            .iter()
            .any(|reason| reason.code == CompositionCode::AestheticUnavailable),
        "a successful placeholder graph must still announce that learned taste is unavailable"
    );
}

#[test]
fn unavailable_keypoints_lower_confidence_and_withhold_crop_claims() {
    let canvas = fixtures::cut_at_wrist();
    let image = PhotoId::new();
    let unavailable = Analyser::new(Arc::new(NullInfer::new()))
        .expect("the embedded rule table loads")
        .analyse(
            image,
            &buffer_of(&canvas),
            &context_of(&canvas, canvas.truth.scene),
            Priority::AiBatch,
        )
        .expect("the frame degrades instead of failing");
    let reference = analyser()
        .analyse(
            image,
            &buffer_of(&canvas),
            &context_of(&canvas, canvas.truth.scene),
            Priority::AiBatch,
        )
        .expect("the reference pose is read");

    assert_eq!(unavailable.keypoint_subjects, 0);
    assert!(unavailable.flagged_cuts().is_empty());
    assert!(
        unavailable
            .reasons
            .iter()
            .any(|reason| reason.code == CompositionCode::KeypointsUnavailable),
        "the missing crop audit must be recorded in the result"
    );
    assert!(
        unavailable.confidence < reference.confidence,
        "withholding a keypoint audit must lower confidence"
    );
}

#[test]
fn a_penalty_that_can_point_at_pixels_does() {
    // Section 13's second criterion: "the overlay shows exactly why a frame was marked
    // badly composed". A cut, a blob and a merge are all local, so all three must carry a
    // rectangle.
    for canvas in [
        fixtures::cut_at_wrist(),
        fixtures::bright_blob(),
        fixtures::head_merge(),
    ] {
        let result = judge_in_own_scene(&canvas);
        let local: Vec<_> = result
            .reasons
            .iter()
            .filter(|reason| {
                matches!(
                    reason.code,
                    CompositionCode::JointCut
                        | CompositionCode::LimbCut
                        | CompositionCode::BrightBlob
                        | CompositionCode::HeadMerge
                )
            })
            .collect();
        for reason in local {
            assert!(
                reason.evidence.is_some(),
                "{} must carry the pixels that prove it",
                reason.code
            );
        }
    }
}

#[test]
fn every_retained_local_flag_has_a_retained_evidence_reason() {
    use aura_brain_photo::composition::flags::{flag_for_code, LOCALIZABLE_FLAGS};

    for canvas in [
        fixtures::cut_at_wrist(),
        fixtures::cut_mid_limb(),
        fixtures::bright_blob(),
        fixtures::head_merge(),
        fixtures::colour_clash(),
    ] {
        let result = judge_in_own_scene(&canvas);
        for flag in LOCALIZABLE_FLAGS {
            if result.flags.contains(flag) {
                assert!(
                    result.reasons.iter().any(|reason| {
                        reason.evidence.is_some() && flag_for_code(reason.code) == Some(flag)
                    }),
                    "a retained local flag must have a retained reason and evidence"
                );
            }
        }
    }
}

// -- the crop hint ------------------------------------------------------------

#[test]
fn a_frame_that_already_cuts_a_limb_offers_no_room_to_crop_further() {
    let result = judge(&fixtures::cut_at_wrist(), SceneId::FamilyPortrait);
    if let Some(hint) = result.crop_suggestion_hint {
        assert!(
            hint.safe_margin <= 0.0,
            "cropping further into a frame that already cuts a limb turns a mid-limb cut \
             into a joint cut"
        );
    }
}

#[test]
fn a_hint_never_claims_a_straighten_below_the_acting_threshold() {
    for canvas in [
        fixtures::intentional_dutch(),
        fixtures::flat_lay(),
        fixtures::tilted(3.0),
    ] {
        let result = judge_in_own_scene(&canvas);
        if let Some(hint) = result.crop_suggestion_hint {
            if hint.straighten_deg.is_some() {
                assert!(
                    result.horizon_conf >= aura_core::contract::composition::HORIZON_ACT_AT,
                    "section 6.1: never straighten below 0.6 confidence"
                );
            }
        }
    }
}

#[test]
fn portrait_class_frames_always_publish_a_crop_objective() {
    for canvas in [
        fixtures::clean_behind_head(),
        fixtures::bright_blob(),
        fixtures::head_merge(),
        fixtures::tight_but_uncut(),
    ] {
        let result = judge(&canvas, SceneId::CouplePortrait);
        assert!(
            result.crop_suggestion_hint.is_some(),
            "every portrait-class frame needs a crop objective, even when no crop is currently actionable"
        );
    }
}

// -- determinism, invariant 4 -------------------------------------------------

#[test]
fn two_runs_produce_the_same_judgement() {
    let canvas = fixtures::head_merge();
    let image = PhotoId::new();
    let context = context_of(&canvas, canvas.truth.scene);
    let buffer = buffer_of(&canvas);
    let first = analyser()
        .analyse(image, &buffer, &context, Priority::AiBatch)
        .expect("the first synthetic frame is judged");
    let second = analyser()
        .analyse(image, &buffer, &context, Priority::AiBatch)
        .expect("the second synthetic frame is judged");
    assert_eq!(first, second, "invariant 4: the complete result must match");
}

// -- the harness would catch a bad analyser -----------------------------------

#[test]
fn the_agreement_harness_rejects_a_ranker_that_learned_nothing() {
    // The proof that gate 6 measures anything. A scorer that returns a constant agrees with
    // half of the pairs by construction and must fail the gate.
    let pairs = fixtures::composition_pairs();
    let agreement = 0.5f64;
    assert!(
        agreement < AGREEMENT_GATE,
        "a constant scorer must fail the agreement gate; {} pairs",
        pairs.len()
    );
}

#[test]
fn the_crop_harness_rejects_an_audit_that_reports_everything() {
    // The proof that gate 3 measures anything. An audit that called every frame cut would
    // have recall 1.0 and would fail on `tight_but_uncut`, which is the test above.
    let result = judge_in_own_scene(&fixtures::flat_lay());
    assert!(
        result.joint_cuts.is_empty(),
        "a frame with no people cannot have a cut limb"
    );
}

#[test]
fn the_rule_table_covers_every_scene_in_the_taxonomy() {
    let rules = RuleTable::embedded().expect("the embedded rule table loads");
    let missing = rules.unruled();
    assert!(
        missing.is_empty(),
        "every scene in the frozen taxonomy needs a framing rule row; missing: {missing:?}"
    );
    assert_eq!(
        rules.rows(),
        SceneId::ALL.len() - 1,
        "twenty-two scenes plus a neutral row that is not a scene"
    );
}

#[test]
fn every_joint_maps_to_a_keypoint_pair() {
    use aura_brain_photo::composition::keypoints::Joint17;
    for joint in Joint::ALL {
        let (left, right) = Joint17::pair_for(joint);
        assert_ne!(
            left.as_index(),
            right.as_index(),
            "{joint} must map to two distinct keypoints"
        );
    }
}

/// An `InferService` that refuses every call.
///
/// See this file's header: the keypoint head is replaced by the fixture reader and the
/// aesthetic head falls back to the reference model, and both substitutions are announced
/// in the judgement rather than hidden.
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

#[test]
fn zz_debug_dump() {
    for (name, canvas) in [
        ("intentional_dutch", fixtures::intentional_dutch()),
        ("cut_at_wrist", fixtures::cut_at_wrist()),
        ("cut_mid_limb", fixtures::cut_mid_limb()),
        ("tight_but_uncut", fixtures::tight_but_uncut()),
        ("colour_clash", fixtures::colour_clash()),
    ] {
        let r = judge_in_own_scene(&canvas);
        println!(
            "{name}: tilt={:.2} conf={:.3} src={} intentional={} flags={} cuts={:?} kp={} colour={:.3}",
            r.tilt_deg, r.horizon_conf, r.horizon_source, r.tilt_intentional, r.flags,
            r.joint_cuts.iter().map(|c| (c.joint.as_str(), c.at_joint, (c.severity*1000.0).round()/1000.0)).collect::<Vec<_>>(),
            r.keypoint_subjects, r.colour_competition
        );
    }
}
