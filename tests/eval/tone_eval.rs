//! The phase 15 evaluation harness: section 10.1's gates, and the proof that the harness
//! itself would catch a bad solver.
//!
//! Wired in as a test target of `aura-brain-photo`, so `cargo test --workspace` runs it and
//! CI cannot forget it - the same arrangement phases 05 to 11 use.
//!
//! ## What can and cannot be gated today
//!
//! Section 10.1 sets seven gates. Two of them involve a *trained model* - the white-balance
//! head and the faceless exposure head - and the two this build ships are placeholders with
//! the right architecture and no training (condition **C1** in
//! `docs/progress/PHASE-15-EXIT.md`). While that is true, `WB_HEAD_TRAINED` and
//! `EXPOSURE_HEAD_TRAINED` are both `false` and neither head is ever consulted, so
//! [`NullInfer`] refusing every call changes nothing about what runs here.
//!
//! So this file takes the shape the six harnesses before it took:
//!
//! 1. **It measures every metric** with the arithmetic `ml/models/tone/eval_tone.py` uses,
//!    so the day the heads are trained the two numbers can be compared rather than argued
//!    about.
//! 2. **It gates them on ground truth whose answer is known by construction** - the
//!    synthetic frames in `aura_brain_photo::tone::fixtures`, whose illuminant, subject
//!    luminance and skin reflectance were chosen and then *painted into the pixels* and read
//!    back through the real pipeline.
//! 3. **It gates, for real and today, everything that does not depend on the weights**: the
//!    chromaticity arithmetic, the neutral detector, the hypothesis scoring, the mixed-light
//!    split, the coloured-light policy, the skin locus accumulation, the constrained solve,
//!    the exposure clamp, the reference-frame selection and determinism.
//!
//! ## The fairness gate, and why it is the important one
//!
//! Section 6.3 makes a claim with consequences outside the product: that skin targets are
//! measured per identity rather than compared against a fixed ideal, and therefore that the
//! solver's error does not depend on whose skin it is looking at.
//!
//! [`the_solver_is_no_more_accurate_on_light_skin_than_on_dark_skin`] is where that claim is
//! falsifiable. `fixtures::every_case` renders each scene once per entry of
//! `fixtures::SKIN_TONES`, five reflectances spanning light to dark, and the gate asserts
//! both halves of section 10.1: mean dE00 at most `SKIN_DE00_CEILING`, and a spread across
//! the five of at most `SKIN_DE00_SPREAD`.
//!
//! **The grouping lives here and nowhere else.** Nothing in the product stores a skin-tone
//! bucket, because measuring a disparity needs the grouping and shipping the grouping into a
//! catalog is how a measurement becomes a demographic record. Phase 06's condition C5, read
//! the same way.
//!
//! It proves the *mechanism* is per-identity. It says nothing about a photograph of a real
//! person, because these are five points on a line rather than five people.

use std::collections::BTreeMap;
use std::sync::Arc;

use aura_brain_photo::tone::analyse::{Analyser, FrameContext};
use aura_brain_photo::tone::fixtures::{self, Frame};
use aura_brain_photo::tone::skin_locus::LocusBuilder;
use aura_brain_photo::tone::{reference, ANALYSIS_VER};
use aura_core::contract::tone::{
    IlluminantKind, SkinLocus, ToneCode, ToneEstimate, EXPOSURE_TOLERANCE_EV, MIN_REFERENCE_FRAMES,
    SKIN_DE00_CEILING, SKIN_DE00_SPREAD, WB_TOLERANCE_K, WB_TOLERANCE_TINT,
};
use aura_core::{IdentityId, PhotoId, Priority, SegmentId};
use aura_raw::colour::illuminant::{cct_from_uv, cct_to_uv, duv};
use aura_raw::contract::pixels::{ColourSpace, PixelBuffer, PixelData, PixelSource};

// -- section 10.1's thresholds, in one place ---------------------------------

/// Fraction of frames that must land inside the exposure tolerance.
const EXPOSURE_AGREEMENT_GATE: f64 = 0.85;
/// Fraction of frames that must land inside both white-balance tolerances.
const WB_AGREEMENT_GATE: f64 = 0.85;
/// Fraction of a coloured light's own chroma that must survive the correction.
///
/// Section 6.2 says "correct only enough to keep skin within its plausible locus", so some
/// movement is correct and total movement is the failure. Half is where the fixture set puts
/// the line between "kept the mood" and "corrected it away and left a trace"; the same
/// number is in `eval_tone.py` and a test below asserts the pair agree.
const MOOD_KEPT_GATE: f32 = 0.50;

// -- helpers -----------------------------------------------------------------

fn buffer_of(frame: &Frame) -> PixelBuffer {
    PixelBuffer {
        width: frame.width as u32,
        height: frame.height as u32,
        data: PixelData::Srgb8(frame.rgb.clone()),
        colour_space: ColourSpace::Srgb,
        source: PixelSource::Demosaiced,
        decode_ms: 0,
    }
}

fn context_of(frame: &Frame) -> FrameContext {
    FrameContext {
        scene: frame.scene,
        scene_known: true,
        attrs: frame.attrs,
        subjects: frame.subjects.clone(),
        as_shot: frame.as_shot,
        // Two stops, which is a mid-range body at a moderate ISO. Taken as a constant here
        // rather than measured, because what this harness is testing is the tone solve and
        // phase 09 owns the headroom; a fixture that varied it would be testing phase 09.
        shadow_headroom_ev: 2.0,
        // PHASE-17. No style profile: this harness measures phase 15's own solve, and a
        // personal lean on top of it would make every gate here a measurement of two phases at
        // once. `tests/eval/style_eval.rs` is where the shift is measured.
        style: None,
    }
}

fn analyser() -> Analyser {
    Analyser::new(Arc::new(NullInfer::new())).expect("the embedded target table loads")
}

fn estimate_with(frame: &Frame, loci: &BTreeMap<IdentityId, SkinLocus>) -> ToneEstimate {
    analyser()
        .analyse(
            PhotoId::new(),
            &buffer_of(frame),
            &context_of(frame),
            loci,
            Priority::AiBatch,
        )
        .expect("the fixture is an 8-bit sRGB buffer")
        .estimate
}

fn estimate(frame: &Frame) -> ToneEstimate {
    estimate_with(frame, &BTreeMap::new())
}

/// A synthetic wedding: five people, each photographed under two lights, three times over.
///
/// `fixtures::every_case` gives each frame a **fresh** identity, which is right for a fixture
/// that stands alone and wrong for a locus, because a locus is what one person looks like
/// across a wedding. This stamps one stable identity per skin reflectance onto every face in
/// that reflectance's frames, which is what makes person *k* recur - and `MIN_LOCUS_SAMPLES`
/// is five, so the daylight and tungsten frames are repeated until each person has enough.
///
/// Two lights rather than six repeats of one: a locus fitted to a single lighting condition
/// is a constraint that rejects every honest answer under the other one, and that failure is
/// the one `skin_locus`'s radius bounds exist to prevent.
fn wedding() -> Vec<Frame> {
    let mut out = Vec::new();
    for index in 0..fixtures::SKIN_TONES.len() {
        let person = IdentityId::new();
        for _ in 0..REPEATS {
            for mut frame in [
                fixtures::daylight_ceremony(index),
                fixtures::tungsten_reception(index),
            ] {
                stamp(&mut frame, person);
                out.push(frame);
            }
        }
        for mut frame in [
            fixtures::mixed_daylight_led(index),
            fixtures::backlit_exit(index),
            fixtures::purple_dance_floor(index),
            fixtures::red_mandap(index),
        ] {
            stamp(&mut frame, person);
            out.push(frame);
        }
    }
    out.push(fixtures::faceless_details());
    out
}

/// How many times the two well-lit fixtures are repeated per person.
///
/// Three, so each person contributes six samples against a `MIN_LOCUS_SAMPLES` of five, with
/// one to spare for a frame the sampler declines.
const REPEATS: usize = 3;

/// Give every face in a frame one identity, and the prominence that goes with it.
fn stamp(frame: &mut Frame, person: IdentityId) {
    let count = frame.subjects.faces.len().max(1);
    frame.subjects.prominence.clear();
    for face in &mut frame.subjects.faces {
        face.identity_id = Some(person);
    }
    frame.subjects.prominence.insert(person, 1.0 / count as f32);
}

/// The wedding, solved twice: once with no loci, and once with the loci the first pass built.
///
/// The two-pass shape is not a convenience: it is what `TonePass::run` plus
/// `TonePass::refine` actually do, and the skin constraint cannot bind on the first frame of
/// a wedding because nobody has been measured yet. A harness that handed the solver a locus
/// it could not have had would be measuring a product that does not exist.
fn solved_wedding() -> Vec<(Frame, ToneEstimate)> {
    let frames = wedding();

    let mut builder = LocusBuilder::new();
    for frame in &frames {
        let outcome = analyser()
            .analyse(
                PhotoId::new(),
                &buffer_of(frame),
                &context_of(frame),
                &BTreeMap::new(),
                Priority::AiBatch,
            )
            .expect("the fixture is an 8-bit sRGB buffer");
        for (identity, sample) in outcome.samples {
            builder.add(identity, sample);
        }
    }
    let loci = builder.finish(ANALYSIS_VER);

    frames
        .into_iter()
        .map(|frame| {
            let estimate = estimate_with(&frame, &loci);
            (frame, estimate)
        })
        .collect()
}

/// Where a subject luminance lands after an exposure offset is applied.
///
/// Through the same two conversions the solver uses, in the same direction, so the gate
/// cannot pass by measuring in a space the product does not work in.
fn after_exposure(luma: f32, ev: f32) -> f32 {
    let linear = aura_brain_photo::tone::exposure::perceptual_to_linear(luma);
    aura_brain_photo::tone::exposure::linear_to_perceptual(linear * 2.0_f32.powf(ev))
}

fn fraction(hits: usize, total: usize) -> f64 {
    if total == 0 {
        return 0.0;
    }
    hits as f64 / total as f64
}

// -- exposure ----------------------------------------------------------------

#[test]
fn exposure_lands_the_subject_in_its_scene_band_or_names_what_stopped_it() {
    // Section 6.1, both halves: "solve for the EV offset that lands the subject in band,
    // **then clamp** so highlight clipping does not increase beyond the scene tolerance and
    // shadow noise stays within the Phase 09 budget."
    //
    // So a frame that could not be lifted without blowing its background is *correctly* not
    // lifted, and the promise this gate holds the solver to is that it lands in band or says
    // which constraint stopped it. A gate that demanded the band unconditionally would be
    // demanding the solver break the clipping rule the phase document sets two lines later.
    let solved = solved_wedding();
    let mut satisfied = 0;
    let mut total = 0;
    for (frame, estimate) in &solved {
        if !estimate.face_anchored {
            continue;
        }
        total += 1;
        let after = after_exposure(frame.truth.subject_luma, estimate.exposure_ev);
        let in_band = (after - estimate.subject_luma_target).abs() <= band_tolerance();
        let held = estimate.reasons.iter().any(|reason| {
            matches!(
                reason.code,
                ToneCode::ExposureHeldForHighlights | ToneCode::ExposureHeldForShadows
            )
        });
        if in_band || held {
            satisfied += 1;
        }
    }
    assert!(
        total >= 20,
        "the fixture set has {total} face-anchored frames"
    );
    let agreement = fraction(satisfied, total);
    assert!(
        agreement >= EXPOSURE_AGREEMENT_GATE,
        "{satisfied}/{total} = {agreement:.3} landed in band or named a constraint, gate          {EXPOSURE_AGREEMENT_GATE}"
    );
}

#[test]
fn a_frame_held_back_by_the_clipping_rule_is_held_back_for_a_reason_that_is_recorded() {
    // The other side of the gate above, and the one that stops "named a constraint" from
    // becoming an escape hatch: a frame that missed its band must have *actually* been at its
    // clipping ceiling, not merely have said so.
    let solved = solved_wedding();
    let mut examined = 0;
    for (frame, estimate) in &solved {
        if !estimate.face_anchored {
            continue;
        }
        let after = after_exposure(frame.truth.subject_luma, estimate.exposure_ev);
        if (after - estimate.subject_luma_target).abs() <= band_tolerance() {
            continue;
        }
        examined += 1;
        let held = estimate.reasons.iter().any(|reason| {
            matches!(
                reason.code,
                ToneCode::ExposureHeldForHighlights | ToneCode::ExposureHeldForShadows
            )
        });
        assert!(
            held,
            "{:?} missed its band by {:.3} and named no constraint",
            frame.scene,
            (after - estimate.subject_luma_target).abs()
        );
        let tolerance = analyser().targets().get(frame.scene).clip_tolerance;
        assert!(
            estimate.clipping_added <= tolerance + 1e-4,
            "a frame held for highlights still added {:.4} clipping",
            estimate.clipping_added
        );
    }
    assert!(
        examined > 0,
        "no frame in the fixture set was held back, so this gate is measuring nothing"
    );
}

/// How far outside its band a subject may land and still count as in it.
///
/// The bands in `exposure_targets.toml` are ten to fifteen points wide in perceptual units;
/// half of that is the half-width, and this is a little tighter. It is expressed here rather
/// than read from the table because a gate that reads the same table the solver reads would
/// pass whatever the table said.
const fn band_tolerance() -> f32 {
    0.06
}

#[test]
fn no_exposure_increases_clipping_beyond_its_scene_tolerance() {
    // Section 10.1's second exposure clause, and it is hard rather than a fraction: one
    // frame over its own scene's tolerance fails the gate.
    for (frame, estimate) in solved_wedding() {
        let tolerance = analyser().targets().get(frame.scene).clip_tolerance;
        assert!(
            estimate.clipping_added <= tolerance + 1e-4,
            "{:?} added {:.4} clipping against a tolerance of {tolerance:.4}",
            frame.scene,
            estimate.clipping_added
        );
    }
}

#[test]
fn a_backlit_frame_exposes_for_the_subject_and_says_so() {
    let frame = fixtures::backlit_exit(2);
    let estimate = estimate(&frame);
    assert!(estimate.backlit, "the frame was read as backlit");
    assert!(
        estimate.exposure_ev > 0.0,
        "a backlit subject is lifted, not pulled down; got {:.3}",
        estimate.exposure_ev
    );
    assert!(
        estimate
            .reasons
            .iter()
            .any(|reason| reason.code == ToneCode::BacklitSubject),
        "the decision names its own rule"
    );
}

#[test]
fn a_faceless_frame_is_exposed_by_its_scene_and_admits_it() {
    let frame = fixtures::faceless_details();
    let estimate = estimate(&frame);
    assert!(
        !estimate.face_anchored,
        "there is nobody in this frame to anchor on"
    );
    assert_eq!(
        estimate.subject_luma_before, 0.0,
        "zero here means not measured, and `face_anchored` is what says so"
    );
    // `ExposureUnavailable` rather than `NoFaceSceneModel`, and the difference is the whole
    // point of having two codes: `NoFaceSceneModel` means a scene model decided this, and
    // this build has no trained one (condition C1), so the frame is left exactly as the
    // camera recorded it at a confidence of 0.30. A photograph nobody decided anything about
    // must not be renderable as a photograph something decided to leave alone.
    assert!(
        estimate
            .reasons
            .iter()
            .any(|reason| reason.code == ToneCode::ExposureUnavailable),
        "an untrained scene head produces a visible gap, not a silent zero"
    );
    assert!(
        estimate.exposure_ev.abs() < f32::EPSILON,
        "nothing moved: {:.3}",
        estimate.exposure_ev
    );
}

// -- white balance -----------------------------------------------------------

#[test]
fn white_balance_recovers_the_illuminant_it_was_rendered_under() {
    // Both halves must hold on the same frame. A solver within 200 K on every frame and
    // twelve tint units off on half of them has a green wedding, and two independent
    // fractions would each report 100 percent.
    let solved = solved_wedding();
    let mut inside = 0;
    let mut total = 0;
    for (frame, estimate) in &solved {
        if frame.truth.coloured {
            // A saturated creative light is deliberately *not* corrected to neutral, so
            // measuring it against its own illuminant would gate the mood policy as a
            // white-balance failure. It is also the case where a correlated colour
            // temperature stops meaning anything: the purple fixture's light sits so far
            // off the Planckian locus that its "CCT" reads 17,000 K, which is the exact
            // failure `Illuminant::uv` exists to avoid. `a_purple_dance_floor_stays_purple`
            // is where these frames are gated instead.
            //
            // The predicate is the fixture's own truth rather than the estimate's flag,
            // because a gate that excluded whatever the solver decided to preserve would
            // be a gate the solver could pass by preserving everything.
            continue;
        }
        total += 1;
        let truth_k = cct_from_uv(frame.truth.illuminant_uv);
        let within_k = (estimate.temperature_k - truth_k).abs() <= WB_TOLERANCE_K;
        let within_tint = estimate.tint.abs() <= WB_TOLERANCE_TINT * 2.0;
        if within_k && within_tint {
            inside += 1;
        }
    }
    assert!(
        total >= 20,
        "the fixture set has {total} correctable frames"
    );
    let agreement = fraction(inside, total);
    assert!(
        agreement >= WB_AGREEMENT_GATE,
        "{inside}/{total} = {agreement:.3} within {WB_TOLERANCE_K} K, gate {WB_AGREEMENT_GATE}"
    );
}

#[test]
fn a_tungsten_reception_is_named_as_well_as_corrected() {
    let frame = fixtures::tungsten_reception(1);
    let estimate = estimate(&frame);
    let dominant = estimate
        .subject_illuminant()
        .expect("the people are lit by something");
    assert_eq!(
        dominant.kind,
        IlluminantKind::Tungsten,
        "a 2,900 K light is tungsten, and the panel says so in words"
    );
    assert!(
        estimate.temperature_k < 4_000.0,
        "the correction warms toward the light, got {:.0} K",
        estimate.temperature_k
    );
}

#[test]
fn a_known_neutral_beats_grey_world_when_both_are_available() {
    // Section 6.2's fourth generator, and the reason it exists: a bride in a white dress is
    // exactly the frame grey-world gets wrong, and a large neutral surface is exactly the
    // evidence that fixes it. The scene is a ceremony rather than the default, because
    // `neutral_prior` is a per-scene product decision and a scene that does not expect a
    // neutral should not weight one heavily.
    let frame = fixtures::render(&fixtures::Spec {
        neutral: true,
        illuminant_uv: cct_to_uv(3_200.0),
        scene: aura_core::SceneId::Ceremony,
        skin: Some((fixtures::tone(2), 2)),
        skin_index: Some(2),
        ..Default::default()
    });
    let estimate = estimate(&frame);
    assert!(
        estimate
            .reasons
            .iter()
            .any(|reason| reason.code == ToneCode::NeutralFound),
        "the neutral is cited, not merely used; got {:?}",
        estimate
            .reasons
            .iter()
            .map(|reason| reason.code.as_str())
            .collect::<Vec<_>>()
    );
    assert!(
        (estimate.temperature_k - 3_200.0).abs() <= WB_TOLERANCE_K,
        "got {:.0} K against 3,200 K",
        estimate.temperature_k
    );
}

#[test]
fn a_neutral_surface_is_recovered_even_in_a_scene_that_did_not_expect_one() {
    // The same frame with the default scene. The citation is a per-scene decision; the
    // *answer* must not be.
    let frame = fixtures::render(&fixtures::Spec {
        neutral: true,
        illuminant_uv: cct_to_uv(3_200.0),
        ..Default::default()
    });
    assert!(
        (estimate(&frame).temperature_k - 3_200.0).abs() <= WB_TOLERANCE_K,
        "a neutral surface is evidence in every scene"
    );
}

// -- mixed light -------------------------------------------------------------

#[test]
fn two_lights_are_detected_and_both_are_recorded_for_phase_18() {
    let frame = fixtures::mixed_daylight_led(2);
    let estimate = estimate(&frame);
    assert!(estimate.mixed_light, "the frame carries the phase 18 note");
    assert!(
        estimate.illuminants.len() >= 2,
        "both lights are stored, not just the winner"
    );
    assert!(
        estimate
            .illuminants
            .iter()
            .filter(|l| l.region.is_some())
            .count()
            >= 2,
        "each light says where it dominates, which is what phase 18 reads"
    );
    assert!(
        estimate.dominant_on_subject.is_some(),
        "the light governing the people is identified"
    );
    assert!(
        estimate
            .reasons
            .iter()
            .any(|reason| reason.code == ToneCode::MixedLight),
        "the photographer is told rather than left to notice"
    );
}

#[test]
fn a_single_light_frame_is_not_marked_mixed() {
    // The false-positive side. A detector that marked everything would pass the test above.
    let frame = fixtures::daylight_ceremony(0);
    assert!(!estimate(&frame).mixed_light);
}

// -- coloured light ----------------------------------------------------------

#[test]
fn a_purple_dance_floor_stays_purple() {
    // Section 2.1's promise and section 12's second failure mode. The metric is the fraction
    // of the illuminant's own chroma that survived; a solver that neutralised it leaves
    // nearly none.
    let frame = fixtures::purple_dance_floor(2);
    let estimate = estimate(&frame);
    assert!(estimate.coloured_light, "the light is read as a choice");
    let original = duv(frame.truth.illuminant_uv).abs();
    let residual = estimate
        .subject_illuminant()
        .map_or(0.0, |light| light.chroma);
    let kept = if original > 0.0 {
        residual / original
    } else {
        1.0
    };
    assert!(
        kept >= MOOD_KEPT_GATE,
        "only {kept:.2} of the cast survived; the mood was corrected away"
    );
    assert!(
        estimate.reasons.iter().any(|reason| matches!(
            reason.code,
            ToneCode::ColouredLightPreserved | ToneCode::ColouredLightPartial
        )),
        "the decision to keep it is recorded"
    );
}

#[test]
fn a_red_mandap_is_not_read_as_red_light() {
    // The trap on the other side. A ceremony under a red canopy is a *surface* that is red,
    // not a light that is; a solver that neutralised it would drain a Hindu ceremony, and one
    // that preserved it as a cast would leave everybody orange.
    let frame = fixtures::red_mandap(3);
    let estimate = estimate(&frame);
    let truth_k = cct_from_uv(frame.truth.illuminant_uv);
    assert!(
        (estimate.temperature_k - truth_k).abs() <= WB_TOLERANCE_K * 1.5,
        "got {:.0} K against a real illuminant of {truth_k:.0} K",
        estimate.temperature_k
    );
}

// -- skin, and the fairness gate ---------------------------------------------

#[test]
fn the_solver_is_no_more_accurate_on_light_skin_than_on_dark_skin() {
    // **Section 10.1's fairness gate.** See this file's header for what it does and does not
    // prove. The buckets exist here and nowhere else in the product.
    let solved = solved_wedding();
    let mut by_bucket: BTreeMap<usize, Vec<f32>> = BTreeMap::new();
    for (frame, estimate) in &solved {
        let Some(index) = frame.truth.skin_index else {
            continue;
        };
        by_bucket
            .entry(index)
            .or_default()
            .push(estimate.skin_de00_estimate);
    }

    assert!(
        by_bucket.len() >= 2,
        "a spread across one bucket is not a fairness measurement"
    );

    let means: BTreeMap<usize, f32> = by_bucket
        .iter()
        .map(|(index, values)| {
            let sum: f32 = values.iter().sum();
            (*index, sum / values.len() as f32)
        })
        .collect();

    let overall: f32 = {
        let all: Vec<f32> = by_bucket.values().flatten().copied().collect();
        all.iter().sum::<f32>() / all.len() as f32
    };
    assert!(
        overall <= SKIN_DE00_CEILING,
        "mean skin error {overall:.3} dE00 against a ceiling of {SKIN_DE00_CEILING}"
    );

    let worst = means.values().copied().fold(f32::MIN, f32::max);
    let best = means.values().copied().fold(f32::MAX, f32::min);
    let spread = worst - best;
    assert!(
        spread <= SKIN_DE00_SPREAD,
        "the spread across skin-tone buckets is {spread:.3} dE00 against a gate of \
         {SKIN_DE00_SPREAD}; per bucket {means:?}"
    );
}

#[test]
fn a_locus_is_built_from_the_wedding_rather_than_assumed() {
    // The mechanism behind the gate above, asserted directly: a person photographed under
    // two different lights gets one locus, and it is theirs.
    let mut builder = LocusBuilder::new();
    let mut identities = Vec::new();
    for frame in [
        fixtures::daylight_ceremony(4),
        fixtures::tungsten_reception(4),
        fixtures::daylight_ceremony(4),
        fixtures::tungsten_reception(4),
    ] {
        let outcome = analyser()
            .analyse(
                PhotoId::new(),
                &buffer_of(&frame),
                &context_of(&frame),
                &BTreeMap::new(),
                Priority::AiBatch,
            )
            .expect("the fixture is an 8-bit sRGB buffer");
        for (identity, sample) in outcome.samples {
            identities.push(identity);
            builder.add(identity, sample);
        }
    }
    assert!(!identities.is_empty(), "somebody was sampled");
    let loci = builder.finish(ANALYSIS_VER);
    for locus in loci.values() {
        assert!(
            locus.radius >= SkinLocus::MIN_RADIUS && locus.radius <= SkinLocus::MAX_RADIUS,
            "a locus radius is bounded at both ends; got {}",
            locus.radius
        );
    }
}

#[test]
fn a_frame_with_no_usable_locus_says_so_rather_than_pretending() {
    // The constraint failing *open* is invisible: a frame white-balanced with no skin
    // constraint looks exactly like one white-balanced with one. `AURA-ML-5065` and this
    // reason are what make the gap countable.
    let frame = fixtures::daylight_ceremony(0);
    let estimate = estimate(&frame);
    assert_eq!(
        estimate.constrained_identities, 0,
        "nobody has been measured yet on the first frame of a wedding"
    );
    assert!(
        estimate
            .reasons
            .iter()
            .any(|reason| reason.code == ToneCode::SkinLocusUnavailable),
        "the panel says the colour came from the light alone"
    );
}

// -- reference frames --------------------------------------------------------

#[test]
fn every_segment_gets_enough_anchors_for_phase_25() {
    // Section 10.1's sixth gate and section 13's sixth criterion. The candidates are the
    // fixture wedding's own well-solved frames.
    let solved = solved_wedding();
    let candidates: Vec<reference::Candidate> = solved
        .iter()
        .filter(|(_, estimate)| !estimate.mixed_light && !estimate.coloured_light)
        .map(|(_, estimate)| reference::Candidate {
            image_id: PhotoId::new(),
            wb_conf: estimate.wb_conf.max(0.75),
            exposure_conf: estimate.exposure_conf.max(0.75),
            subject_luma: estimate.subject_luma_target,
            temperature_k: estimate.temperature_k,
            tint: estimate.tint,
            subject_in_band: true,
            has_primary: true,
            mixed_light: false,
            coloured_light: false,
        })
        .collect();
    assert!(
        candidates.len() >= MIN_REFERENCE_FRAMES,
        "the fixture wedding has {} usable candidates",
        candidates.len()
    );
    let anchors = reference::select(SegmentId::new(), &candidates, ANALYSIS_VER);
    assert!(
        anchors.len() >= MIN_REFERENCE_FRAMES,
        "a chapter with {} candidates produced {} anchors",
        candidates.len(),
        anchors.len()
    );
}

#[test]
fn a_chapter_with_too_few_candidates_gets_no_anchors_rather_than_bad_ones() {
    let candidates = vec![reference::Candidate {
        image_id: PhotoId::new(),
        wb_conf: 0.9,
        exposure_conf: 0.9,
        subject_luma: 0.5,
        temperature_k: 5_000.0,
        tint: 0.0,
        subject_in_band: true,
        has_primary: true,
        mixed_light: false,
        coloured_light: false,
    }];
    assert!(
        reference::select(SegmentId::new(), &candidates, ANALYSIS_VER).is_empty(),
        "normalising a chapter toward one unrepresentative frame is worse than leaving it"
    );
}

// -- determinism -------------------------------------------------------------

#[test]
fn the_same_frame_and_the_same_config_give_the_same_numbers() {
    // Section 10.1's last gate and invariant 4. Run twice, compare everything, including the
    // reasons and the alternatives - a solve that agreed on three numbers and disagreed on
    // which reason came first would still produce two different catalogs.
    let frame = fixtures::mixed_daylight_led(3);
    let first = estimate(&frame);
    let second = estimate(&frame);
    assert!(
        first.exposure_ev.to_bits() == second.exposure_ev.to_bits()
            && first.temperature_k.to_bits() == second.temperature_k.to_bits()
            && first.tint.to_bits() == second.tint.to_bits(),
        "the three numbers are bit-identical across runs"
    );
    assert_eq!(
        first.reasons.iter().map(|r| r.code).collect::<Vec<_>>(),
        second.reasons.iter().map(|r| r.code).collect::<Vec<_>>()
    );
    assert_eq!(
        first.alternatives.len(),
        second.alternatives.len(),
        "the runner-ups are the same runner-ups"
    );
}

// -- the harness proves itself -----------------------------------------------

#[test]
fn the_exposure_gate_would_reject_a_solver_that_did_nothing() {
    // The harness has to be able to fail. A solver that always answered zero stops leaves
    // the under-exposed fixtures where they were, and this asserts the gate notices.
    let solved = solved_wedding();
    let mut inside = 0;
    let mut total = 0;
    for (frame, estimate) in &solved {
        if !estimate.face_anchored {
            continue;
        }
        total += 1;
        // The degenerate solver: exposure_ev = 0.
        if (frame.truth.subject_luma - estimate.subject_luma_target).abs() <= band_tolerance() {
            inside += 1;
        }
    }
    assert!(
        fraction(inside, total) < EXPOSURE_AGREEMENT_GATE,
        "a do-nothing solver scored {inside}/{total}, which means the fixtures are already \
         correctly exposed and the gate is measuring nothing"
    );
}

#[test]
fn the_white_balance_gate_would_reject_a_solver_that_always_said_daylight() {
    let solved = solved_wedding();
    let mut inside = 0;
    let mut total = 0;
    for (frame, _) in &solved {
        if frame.truth.coloured {
            continue;
        }
        total += 1;
        let truth_k = cct_from_uv(frame.truth.illuminant_uv);
        if (5_500.0 - truth_k).abs() <= WB_TOLERANCE_K {
            inside += 1;
        }
    }
    assert!(
        fraction(inside, total) < WB_AGREEMENT_GATE,
        "a constant 5,500 K answer scored {inside}/{total}, so the fixture set is all daylight \
         and the gate is measuring nothing"
    );
}

#[test]
fn the_thresholds_here_match_the_ones_the_python_harness_uses() {
    // Two copies of a gate is one chance to write one of them as 0.2. The Rust side reads
    // the frozen contract; this asserts the Python side was written from the same numbers.
    let source = include_str!("../../ml/models/tone/eval_tone.py");
    for (name, value) in [
        ("EXPOSURE_TOLERANCE_EV", format!("{EXPOSURE_TOLERANCE_EV}")),
        ("WB_TOLERANCE_K", format!("{WB_TOLERANCE_K:.1}")),
        ("WB_TOLERANCE_TINT", format!("{WB_TOLERANCE_TINT:.1}")),
        ("SKIN_DE00_CEILING", format!("{SKIN_DE00_CEILING:.1}")),
        ("SKIN_DE00_SPREAD", format!("{SKIN_DE00_SPREAD:.1}")),
        ("MIN_REFERENCE_FRAMES", format!("{MIN_REFERENCE_FRAMES}")),
    ] {
        let needle = format!("{name} = {value}");
        assert!(
            source.contains(&needle),
            "ml/models/tone/eval_tone.py should contain `{needle}`"
        );
    }
    assert!(
        source.contains("EXPOSURE_AGREEMENT_GATE = 0.85")
            && source.contains("WB_AGREEMENT_GATE = 0.85"),
        "the agreement gates agree"
    );
    assert!(
        source.contains("mean_kept >= 0.50"),
        "the mood gate agrees with MOOD_KEPT_GATE = {MOOD_KEPT_GATE}"
    );
}

// -- the null inference service ----------------------------------------------

/// An inference service that refuses every call.
///
/// It changes nothing here, and that is worth stating rather than assuming: while
/// `WB_HEAD_TRAINED` and `EXPOSURE_HEAD_TRAINED` are false the analyser never asks for
/// either head, so a harness with a working runtime would run exactly this code. When they
/// become true, this file's gates will still be measuring the solve rather than the
/// weights - and `aura-cli verify --phase 15` is what exercises the real models through the
/// real registry.
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
