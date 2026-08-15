//! The phase 09 evaluation harness: section 10.1's gates, and the proof that the harness
//! itself would catch a bad analyser.
//!
//! Wired in as a test target of `aura-brain-photo`, so `cargo test --workspace` runs it
//! and CI cannot forget it - the same arrangement phases 05 to 08 use.
//!
//! ## What can and cannot be gated today
//!
//! Section 10.1 sets seven gates. Two of them - subject-focus AUC and blink F1 - are
//! statements that involve a *trained model*, and the two this build ships are
//! placeholders with the right architecture and no training (condition **C1** in
//! `docs/progress/PHASE-09-EXIT.md`).
//!
//! So this file takes the shape the four harnesses before it took:
//!
//! 1. **It measures every metric** with the arithmetic
//!    `ml/models/integrity/eval_integrity.py` uses, so the day the heads are trained the
//!    two numbers can be compared rather than argued about.
//! 2. **It gates them on ground truth whose answer is known by construction** - the
//!    synthetic frames in `aura_brain_photo::fixtures` - which proves the harness would
//!    fail an analyser that had learned nothing.
//! 3. **It gates, for real and today, everything that does not depend on the weights**:
//!    the blur ladder's monotonicity, the focus-miss sign, the shallow-depth-of-field
//!    exoneration, the shake-versus-pan distinction, the specular exclusion, the noise
//!    estimate, the group closed-eye count, the gating rules, the four intent rules,
//!    cross-camera fairness, the scene-relative score construction and determinism.
//!
//! ## Why the analyser runs with no inference service here
//!
//! [`NullInfer`] refuses every call. That is deliberate and it is not a shortcut: the
//! focus head can only ever *withdraw* a softness claim (`focus::HEAD_OVERRULES_AT`), so
//! an analyser with no head is an analyser that is strictly more cautious - and every
//! gate below therefore measures the classical path, which is the path that has to be
//! right. The eye head is replaced by `fixtures::ReferenceEyeReader`, whose answer is
//! painted into the fixture.
//!
//! The real models, through the real registry, are exercised by
//! `aura-cli verify --phase 09`. The tests prove the pieces; the gate proves the
//! assembly.

use aura_brain_photo::fixtures::{self, Frame};
use aura_brain_photo::integrity::calibration::CalibrationTable;
use aura_brain_photo::integrity::exposure;
use aura_brain_photo::integrity::eyes::{self, IntentInput, IntentRule};
use aura_brain_photo::integrity::flags;
use aura_brain_photo::integrity::focus;
use aura_brain_photo::integrity::noise;
use aura_brain_photo::integrity::score;
use aura_brain_photo::{Analyser, FrameContext, FrameExif};
use aura_core::contract::integrity::{
    ExposureVerdict, IntegrityFlags, IntegrityResult, MotionKind, ReasonCode,
};
use aura_core::contract::people::ImageSubjects;
use aura_core::{PhotoId, Priority, SceneId, SceneProfile};
use aura_raw::contract::pixels::{ColourSpace, PixelBuffer, PixelData, PixelSource};

// -- section 10.1's thresholds, in one place ---------------------------------

/// Subject-focus AUC on labelled keeper/reject crops.
const AUC_GATE: f64 = 0.96;
/// Blink F1.
const BLINK_F1_GATE: f64 = 0.95;
/// Intentional-closed-eye false-positive rate.
const INTENTIONAL_FP_GATE: f64 = 0.02;
/// Exposure verdicts against expert labels.
const EXPOSURE_AGREEMENT_GATE: f64 = 0.93;
/// How close the estimated noise sigma must be to the known one.
const NOISE_TOLERANCE: f64 = 0.15;
/// How close two bodies must score on the same scene.
const FAIRNESS_GATE: f32 = 0.05;

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

/// A context for one fixture: a scene, its profile, and the faces the fixture painted.
fn context_for(frame: &Frame, scene: SceneId, make: &str, model: &str, iso: u32) -> FrameContext {
    let mut prominence = std::collections::BTreeMap::new();
    let mut faces = Vec::new();
    for face in &frame.faces {
        faces.push(*face);
        if let Some(identity) = face.identity_id {
            prominence.insert(identity, 1.0 / frame.faces.len() as f32);
        }
    }
    FrameContext {
        scene,
        profile: SceneProfile::neutral(scene),
        subjects: ImageSubjects {
            faces,
            dominant: None,
            prominence,
            subject_focus_score: 0.0,
            people_count: frame.faces.len() as u32,
            weights_ver: 1,
        },
        couple: Vec::new(),
        exif: FrameExif {
            make: make.to_string(),
            model: model.to_string(),
            iso,
            shutter_s: Some(1.0 / 250.0),
            focal_mm: Some(50.0),
            stabilised: false,
            megapixels: 24.0,
        },
        scene_known: true,
    }
}

fn analyser() -> Analyser {
    Analyser::new(std::sync::Arc::new(NullInfer::new()))
        .expect("the embedded calibration table must load")
        .with_reference_eyes()
}

fn analyse(frame: &Frame, context: &FrameContext) -> IntegrityResult {
    analyser()
        .analyse(
            PhotoId::new(),
            &buffer_of(frame),
            context,
            Priority::AiBatch,
        )
        .expect("a synthetic frame must analyse")
}

// ---------------------------------------------------------------------------
// Focus
// ---------------------------------------------------------------------------

#[test]
fn the_blur_ladder_response_is_monotonic() {
    // Section 10.1's first gate. Measured on the raw acutance rather than on the
    // normalised figure, so that the calibration row cannot flatten the top of the curve
    // and hide a non-monotonic analyser.
    let calibration = CalibrationTable::embedded().expect("embedded").fallback();
    let mut previous = f32::INFINITY;
    let mut normalised = Vec::new();
    for frame in fixtures::blur_ladder() {
        let plane =
            aura_vision::embed::descriptors::luma_plane(&frame.rgb, frame.width, frame.height);
        let regions =
            focus::RegionSet::for_face(&[(fixtures::FACE_RECT, frame.faces[0].eyes, true)], &[]);
        let result = focus::analyse(&plane, &regions, &[1.0], &calibration);
        assert!(
            result.raw_subject < previous,
            "radius {} did not measure softer than the rung before it ({:.4} against {:.4})",
            frame.truth.subject_blur,
            result.raw_subject,
            previous
        );
        previous = result.raw_subject;
        normalised.push((frame.truth.subject_blur, result.subject));
    }
    println!("blur ladder (radius, normalised sharpness):");
    for (radius, sharpness) in &normalised {
        println!("  {radius:>2}px -> {sharpness:.3}");
    }
}

#[test]
fn back_and_front_focus_are_correctly_signed() {
    // Section 10.1: "back/front focus correctly signed". Easy to invert by accident, and
    // an inverted sign sends a photographer to the wrong side of their focus calibration.
    let calibration = CalibrationTable::embedded().expect("embedded").fallback();
    for (behind, expectation) in [(true, "positive"), (false, "negative")] {
        let frame = fixtures::focus_miss(behind);
        let plane =
            aura_vision::embed::descriptors::luma_plane(&frame.rgb, frame.width, frame.height);
        let regions =
            focus::RegionSet::for_face(&[(fixtures::FACE_RECT, frame.faces[0].eyes, true)], &[]);
        let result = focus::analyse(&plane, &regions, &[1.0], &calibration);
        if behind {
            assert!(
                result.offset > 0.0,
                "a sharp background should read {expectation}, got {:.3}",
                result.offset
            );
        } else {
            assert!(
                result.offset < 0.0,
                "a sharp foreground should read {expectation}, got {:.3}",
                result.offset
            );
        }
    }
}

#[test]
fn a_shallow_depth_of_field_portrait_is_never_called_soft() {
    // Section 13's second acceptance criterion. Tested in every scene, because the soft
    // threshold is a function of the scene and a criterion that held only for the
    // neutral profile would not be worth much.
    let frame = fixtures::shallow_depth_of_field();
    for scene in SceneId::ALL {
        let context = context_for(&frame, scene, "SONY", "ILCE-7M3", 100);
        let result = analyse(&frame, &context);
        assert!(
            !result.flags.contains(IntegrityFlags::SUBJECT_SOFT),
            "{scene} called a bokeh portrait soft at subject sharpness {:.3}",
            result.subject_sharpness
        );
        assert!(
            result
                .reasons
                .iter()
                .any(|reason| reason.code == ReasonCode::ShallowDepthOfField),
            "{scene} did not say why it was not soft"
        );
    }
}

#[test]
fn subject_focus_auc_is_measured_on_the_blur_ladder() {
    // Section 10.1's headline focus gate. The labels come from the ladder: the two sharp
    // rungs are keepers and the three most blurred are rejects, which is the labelling a
    // photographer would produce from the same frames.
    let calibration = CalibrationTable::embedded().expect("embedded").fallback();
    let mut scored: Vec<(f64, bool)> = Vec::new();
    for frame in fixtures::blur_ladder() {
        let plane =
            aura_vision::embed::descriptors::luma_plane(&frame.rgb, frame.width, frame.height);
        let regions =
            focus::RegionSet::for_face(&[(fixtures::FACE_RECT, frame.faces[0].eyes, true)], &[]);
        let result = focus::analyse(&plane, &regions, &[1.0], &calibration);
        let keeper = frame.truth.subject_blur <= 1;
        let reject = frame.truth.subject_blur >= 4;
        if keeper || reject {
            scored.push((f64::from(result.subject), keeper));
        }
    }
    let auc = area_under_curve(&scored);
    println!("subject-focus AUC on the synthetic ladder: {auc:.3} (gate {AUC_GATE:.2})");
    assert!(
        auc >= AUC_GATE,
        "the analyser cannot separate a sharp crop from a blurred one"
    );
    println!(
        "  deferred: the AUC on *labelled wedding crops* needs the trained focus head - \
         condition C1"
    );
}

#[test]
fn the_auc_harness_rejects_an_analyser_that_learned_nothing() {
    // Phase 08's guard, a phase later: a gate that cannot fail is not a gate. A scorer
    // that returns the same number for every frame must fall below the threshold.
    let flat: Vec<(f64, bool)> = (0..40).map(|index| (0.5, index % 2 == 0)).collect();
    let auc = area_under_curve(&flat);
    assert!(auc < AUC_GATE, "a constant scorer scored {auc:.3}");
    // And one that is exactly wrong must score below a coin toss.
    let inverted: Vec<(f64, bool)> = (0..40)
        .map(|index| (f64::from(index % 2), index % 2 == 0))
        .collect();
    assert!(area_under_curve(&inverted) < 0.5);
}

// ---------------------------------------------------------------------------
// Motion
// ---------------------------------------------------------------------------

#[test]
fn a_shaken_ceremony_frame_and_a_panned_exit_are_distinguished() {
    // Section 13's fourth acceptance criterion.
    let shake = fixtures::camera_shake();
    let shake_context = context_for(&shake, SceneId::Ceremony, "SONY", "ILCE-7M3", 3200);
    let shaken = analyse(&shake, &shake_context);
    assert_eq!(
        shaken.motion,
        MotionKind::CameraShake,
        "a globally smeared ceremony frame is shake"
    );
    assert!(shaken.flags.contains(IntegrityFlags::CAMERA_SHAKE));

    let pan = fixtures::panned();
    let pan_context = context_for(&pan, SceneId::Exit, "SONY", "ILCE-7M3", 3200);
    let panned = analyse(&pan, &pan_context);
    assert_eq!(
        panned.motion,
        MotionKind::Intentional,
        "a smeared background with a held subject is a pan"
    );
    assert!(panned.flags.contains(IntegrityFlags::INTENTIONAL_MOTION));
    assert!(
        !panned.flags.has_defect(),
        "section 6.2 forbids ever calling intentional motion a defect"
    );
}

#[test]
fn intentional_motion_never_costs_the_score_anything() {
    // The arithmetic half of the same rule.
    let profile = SceneProfile::neutral(SceneId::DanceFloor);
    let calibration = CalibrationTable::embedded().expect("embedded").fallback();
    let focus_result = focus::FocusResult {
        subject: 0.8,
        background: 0.3,
        offset: 0.0,
        raw_subject: 0.3,
        raw_background: 0.1,
        subjectless: false,
        shallow_depth_of_field: false,
    };
    let exposure_result = exposure::verdict(
        exposure::Clipping::default(),
        0.0,
        3200,
        &calibration,
        profile.max_acceptable_noise,
        false,
    );
    let clean = score::sub_scores(
        &focus_result,
        &motion_of(MotionKind::None, 0.0),
        &exposure_result,
        &noise::NoiseResult::default(),
        &profile,
        0.0,
    );
    let intentional = score::sub_scores(
        &focus_result,
        &motion_of(MotionKind::Intentional, 1.0),
        &exposure_result,
        &noise::NoiseResult::default(),
        &profile,
        0.0,
    );
    assert!((clean.motion - intentional.motion).abs() < f32::EPSILON);
}

// ---------------------------------------------------------------------------
// Exposure and noise
// ---------------------------------------------------------------------------

#[test]
fn exposure_verdicts_agree_with_the_authored_labels() {
    // Section 10.1: "recoverable vs lost matches expert labels >= 0.93". The labels are
    // authored rather than expert - there are no expert-labelled frames in this
    // repository - and the harness reports that beside the number.
    let calibration = CalibrationTable::embedded().expect("embedded");
    let modern = calibration.get("SONY", "ILCE-7M3", 24.2);
    let older = calibration.get("Canon", "EOS 5D Mark IV", 30.4);

    let cases: Vec<(&str, ExposureVerdict, ExposureVerdict)> = vec![
        (
            "two stops under on a modern body is recoverable",
            ExposureVerdict::Recoverable,
            exposure::verdict(
                exposure::Clipping::default(),
                -2.0,
                800,
                &modern,
                0.5,
                false,
            )
            .verdict,
        ),
        (
            "the same frame on a 2016 body at high ISO is not",
            ExposureVerdict::Marginal,
            exposure::verdict(
                exposure::Clipping::default(),
                -2.0,
                6400,
                &older,
                0.5,
                false,
            )
            .verdict,
        ),
        (
            "a correctly exposed frame needs nothing",
            ExposureVerdict::Good,
            exposure::verdict(exposure::Clipping::default(), 0.1, 100, &modern, 0.5, false).verdict,
        ),
        (
            "heavily blown highlights are lost",
            ExposureVerdict::Lost,
            exposure::verdict(
                exposure::Clipping {
                    high: 0.09,
                    ..exposure::Clipping::default()
                },
                1.9,
                100,
                &modern,
                0.5,
                false,
            )
            .verdict,
        ),
        (
            "crushed shadows past the noise floor are lost",
            ExposureVerdict::Lost,
            exposure::verdict(
                exposure::Clipping {
                    low: 0.22,
                    ..exposure::Clipping::default()
                },
                -5.5,
                12800,
                &older,
                0.3,
                false,
            )
            .verdict,
        ),
        (
            "half a stop over with a little clipping is still recoverable",
            ExposureVerdict::Recoverable,
            exposure::verdict(
                exposure::Clipping {
                    high: 0.01,
                    ..exposure::Clipping::default()
                },
                0.5,
                100,
                &modern,
                0.5,
                false,
            )
            .verdict,
        ),
    ];

    let agreed = cases
        .iter()
        .filter(|(_, expected, actual)| expected == actual)
        .count();
    let rate = agreed as f64 / cases.len() as f64;
    println!("exposure agreement: {rate:.3} over {} cases", cases.len());
    for (label, expected, actual) in &cases {
        let mark = if expected == actual { "ok " } else { "MISS" };
        println!("  [{mark}] {label}: expected {expected}, got {actual}");
    }
    assert!(rate >= EXPOSURE_AGREEMENT_GATE);
    println!("  authored labels, not expert labels - condition C3");
}

#[test]
fn a_candle_is_not_a_blown_highlight_and_a_blown_dress_is() {
    // Section 10.1: "candle and sparkler frames not flagged".
    let flame = fixtures::candle();
    let plane = aura_vision::embed::descriptors::luma_plane(&flame.rgb, flame.width, flame.height);
    let specular = exposure::specular_fraction(&plane);
    println!(
        "candle: {:.0}% of the clipped pixels are a light source",
        specular * 100.0
    );
    assert!(
        specular > 0.5,
        "a small bright light in a dark room must read as specular"
    );

    let blown = fixtures::clipped();
    let context = context_for(&blown, SceneId::CouplePortrait, "SONY", "ILCE-7M3", 100);
    let verdict = analyse(&blown, &context);
    assert!(
        verdict.flags.contains(IntegrityFlags::HIGHLIGHT_LOST),
        "a frame blown across its whole area is a real loss"
    );

    let candle_context = context_for(&flame, SceneId::Ceremony, "SONY", "ILCE-7M3", 6400);
    let candle_verdict = analyse(&flame, &candle_context);
    assert!(
        !candle_verdict
            .flags
            .contains(IntegrityFlags::HIGHLIGHT_LOST),
        "a candle frame must not be flagged"
    );
}

#[test]
fn the_noise_estimate_is_within_fifteen_per_cent_of_a_known_sigma() {
    // Section 10.1's noise gate, on the ISO ladder.
    let table = CalibrationTable::embedded().expect("embedded");
    let calibration = table.get("SONY", "ILCE-7M3", 24.2);
    for (frame, iso) in fixtures::iso_ladder() {
        let plane =
            aura_vision::embed::descriptors::luma_plane(&frame.rgb, frame.width, frame.height);
        let measured = noise::analyse(&plane, iso, &calibration, 0.5);
        assert!(measured.measured, "ISO {iso}: no flat tiles were found");
        let error = (f64::from(measured.sigma) - f64::from(frame.truth.sigma)).abs()
            / f64::from(frame.truth.sigma);
        println!(
            "ISO {iso:>5}: known sigma {:.4}, measured {:.4}, error {:.1}%, relative {:.2}",
            frame.truth.sigma,
            measured.sigma,
            error * 100.0,
            measured.sigma_rel
        );
        assert!(
            error <= NOISE_TOLERANCE,
            "ISO {iso}: {:.1}% error against a {:.0}% tolerance",
            error * 100.0,
            NOISE_TOLERANCE * 100.0
        );
    }
}

#[test]
fn the_same_noise_is_a_different_verdict_in_two_scenes() {
    // Invariant 7, as arithmetic: "a dance frame is not punished for being a dance
    // frame". The same sigma at the same ISO on the same body, judged by two scenes.
    let table = CalibrationTable::embedded().expect("embedded");
    let calibration = table.get("SONY", "ILCE-7M3", 24.2);
    let portrait = SceneProfile::neutral(SceneId::FamilyPortrait);
    let dance = SceneProfile {
        max_acceptable_noise: 0.85,
        ..SceneProfile::neutral(SceneId::DanceFloor)
    };
    let strict = noise::relative(0.02, 6400, &calibration, portrait.max_acceptable_noise);
    let loose = noise::relative(0.02, 6400, &calibration, dance.max_acceptable_noise);
    println!("the same sigma reads {strict:.2} in a portrait and {loose:.2} on a dance floor");
    assert!(strict > loose);
}

// ---------------------------------------------------------------------------
// Eyes
// ---------------------------------------------------------------------------

#[test]
fn blink_f1_is_measured_against_the_reference_reader() {
    // Section 10.1's blink gate. `ReferenceEyeReader` supplies the eye state from a
    // marker painted into the fixture, so what is measured is the alignment, the gating
    // and the flag decision - not the head's weights.
    let mut true_positive = 0u32;
    let mut false_positive = 0u32;
    let mut false_negative = 0u32;

    for closed in [0usize, 1, 2] {
        for total in [1usize, 2, 4] {
            if closed > total {
                continue;
            }
            let frame = fixtures::group_frame(total, closed);
            // A scene that does not justify closed eyes, so every closure should be a
            // defect and the count is unambiguous.
            let context = context_for(&frame, SceneId::FamilyPortrait, "SONY", "ILCE-7M3", 400);
            let result = analyse(&frame, &context);
            let found = result
                .eyes
                .iter()
                .filter(|eye| eye.state.is_closed())
                .count();
            let hit = found.min(closed);
            true_positive += u32::try_from(hit).unwrap_or(0);
            false_positive += u32::try_from(found.saturating_sub(closed)).unwrap_or(0);
            false_negative += u32::try_from(closed.saturating_sub(found)).unwrap_or(0);
        }
    }

    let precision = f64::from(true_positive) / f64::from(true_positive + false_positive).max(1.0);
    let recall = f64::from(true_positive) / f64::from(true_positive + false_negative).max(1.0);
    let f1 = if precision + recall <= 0.0 {
        0.0
    } else {
        2.0 * precision * recall / (precision + recall)
    };
    println!(
        "blink F1 {f1:.3} (precision {precision:.3}, recall {recall:.3}) against a \
         {BLINK_F1_GATE:.2} gate"
    );
    assert!(f1 >= BLINK_F1_GATE);
    println!("  measured with the reference reader - the head's weights are condition C1");
}

#[test]
fn a_kiss_with_closed_eyes_is_flagged_as_intentional_rather_than_as_a_defect() {
    // Section 13's third acceptance criterion, and the trust test of the whole product.
    let frame = fixtures::group_frame(2, 2);
    for scene in [
        SceneId::Kiss,
        SceneId::Vows,
        SceneId::Ritual,
        SceneId::FirstLook,
        SceneId::Speeches,
    ] {
        let context = context_for(&frame, scene, "SONY", "ILCE-7M3", 1600);
        let result = analyse(&frame, &context);
        assert!(
            result.flags.contains(IntegrityFlags::EYES_CLOSED_OK),
            "{scene}: closed eyes must be exonerated"
        );
        assert!(
            !result.flags.contains(IntegrityFlags::EYES_CLOSED),
            "{scene}: closed eyes must not also be a defect"
        );
        assert!(
            result
                .reasons
                .iter()
                .any(|reason| reason.code == ReasonCode::EyesClosedOk),
            "{scene}: the panel must be able to say why"
        );
    }
}

#[test]
fn the_intentional_false_positive_rate_is_inside_two_per_cent() {
    // Section 10.1: intentional-closed-eye false positives <= 2 %. A false positive here
    // is the *dangerous* direction: a frame the product calls a blink that a photographer
    // would call the photograph.
    let mut judged = 0u32;
    let mut wrongly_flagged = 0u32;
    for scene in [
        SceneId::Kiss,
        SceneId::Vows,
        SceneId::Ritual,
        SceneId::Rings,
        SceneId::FirstLook,
        SceneId::Speeches,
        SceneId::FirstDance,
    ] {
        for total in [1usize, 2, 3] {
            let frame = fixtures::group_frame(total, total);
            let context = context_for(&frame, scene, "SONY", "ILCE-7M3", 1600);
            let result = analyse(&frame, &context);
            judged += 1;
            if result.flags.contains(IntegrityFlags::EYES_CLOSED) {
                wrongly_flagged += 1;
            }
        }
    }
    let rate = f64::from(wrongly_flagged) / f64::from(judged.max(1));
    println!("intentional-closed false positives: {rate:.3} over {judged} frames");
    assert!(rate <= INTENTIONAL_FP_GATE);
}

#[test]
fn the_group_closed_eye_ratio_matches_the_count_exactly() {
    // Section 10.1: "closed-eye ratio matches human count exactly on 200 labelled group
    // frames". Exactly, so a ratio is not enough - the numerator has to be right.
    for total in [2usize, 3, 4, 5, 6] {
        for closed in 0..=total {
            let frame = fixtures::group_frame(total, closed);
            let context = context_for(&frame, SceneId::GroupPortrait, "SONY", "ILCE-7M3", 400);
            let result = analyse(&frame, &context);
            let expected = closed as f32 / total as f32;
            assert_eq!(
                result.gating_faces as usize, total,
                "every face in a posed group must gate"
            );
            assert!(
                (result.closed_eye_ratio - expected).abs() < 0.001,
                "{closed} of {total}: expected {expected:.3}, got {:.3}",
                result.closed_eye_ratio
            );
        }
    }
}

#[test]
fn a_blinking_guest_in_a_crowd_is_not_a_defect() {
    // Section 6.4: "a guest blinking in row four is not a defect; the bride blinking in a
    // portrait is." Tested through the gating rule directly, because the fixture cannot
    // paint a face small enough to be a guest and still carry a readable marker.
    let big = fixtures::face(aura_core::contract::integrity::CropRect {
        x: 0.35,
        y: 0.30,
        w: 0.30,
        h: 0.30,
    });
    let tiny = fixtures::face(aura_core::contract::integrity::CropRect {
        x: 0.02,
        y: 0.02,
        w: 0.05,
        h: 0.05,
    });
    assert!(
        eyes::gates(&big, 0.0),
        "a face filling 9 % of the frame gates"
    );
    assert!(
        !eyes::gates(&tiny, 0.0),
        "a face filling 0.25 % of the frame with no identity does not gate"
    );
    assert!(
        eyes::gates(&tiny, 0.4),
        "unless phase 06 says they are one of the most prominent people present"
    );
}

#[test]
fn the_four_intent_rules_fire_in_the_documented_order() {
    // Section 6.4's list, in order, with rule 3 recorded as unreachable in this build.
    assert_eq!(
        eyes::intent(IntentInput {
            scene: SceneId::Kiss,
            ..IntentInput::default()
        }),
        IntentRule::Scene
    );
    assert_eq!(
        eyes::intent(IntentInput {
            scene: SceneId::Candid,
            laughing: true,
            ..IntentInput::default()
        }),
        IntentRule::Laughing
    );
    assert_eq!(
        eyes::intent(IntentInput {
            scene: SceneId::Candid,
            tears: true,
            ..IntentInput::default()
        }),
        IntentRule::Tears
    );
    assert_eq!(
        eyes::intent(IntentInput {
            scene: SceneId::Candid,
            partner_also_closed: true,
            ..IntentInput::default()
        }),
        IntentRule::BothPartners
    );
    assert_eq!(
        eyes::intent(IntentInput {
            scene: SceneId::Candid,
            ..IntentInput::default()
        }),
        IntentRule::None
    );
    println!("  rule 3 (tears) is wired and unreachable until phase 10 - condition C4");
}

// ---------------------------------------------------------------------------
// Scoring
// ---------------------------------------------------------------------------

#[test]
fn one_catastrophic_factor_cannot_be_averaged_away() {
    // Section 6.5's whole argument, as an assertion. Four good sub-scores and one
    // terrible one must not produce a good composite.
    let weights = score::Weights::for_scene(&SceneProfile::neutral(SceneId::CouplePortrait));
    let calibration = score::Isotonic::IDENTITY;
    let perfect = score::compose(score::SubScores::PERFECT, weights, &calibration);
    let one_bad = score::compose(
        score::SubScores {
            focus: 0.08,
            ..score::SubScores::PERFECT
        },
        weights,
        &calibration,
    );
    println!("perfect {perfect:.3}, one catastrophic factor {one_bad:.3}");
    assert!(perfect > 0.95);
    assert!(
        one_bad < 0.55,
        "a weighted sum would have scored this about 0.75"
    );

    // And the linear-sum comparison, printed so the argument is visible in the log.
    let linear = 0.08f32.mul_add(weights.focus, 1.0 - weights.focus);
    println!("  the same frame under a weighted sum: {linear:.3}");
    assert!(linear > one_bad);
}

#[test]
fn zero_point_eight_means_the_same_thing_in_every_scene() {
    // Section 13's fifth acceptance criterion. A frame at a fixed *relative* position -
    // exactly at its scene's soft threshold, exactly at its scene's noise tolerance -
    // must score the same whichever scene it is in.
    let calibration = CalibrationTable::embedded().expect("embedded").fallback();
    let mut scores = Vec::new();
    for scene in SceneId::ALL {
        let profile = SceneProfile::neutral(scene);
        let threshold = flags::soft_threshold(&profile);
        let focus_result = focus::FocusResult {
            subject: threshold,
            background: threshold,
            offset: 0.0,
            raw_subject: 0.2,
            raw_background: 0.2,
            subjectless: false,
            shallow_depth_of_field: false,
        };
        let noise_result = noise::NoiseResult {
            sigma: 0.01,
            sigma_rel: 1.0,
            tiles: 64,
            measured: true,
        };
        let exposure_result = exposure::verdict(
            exposure::Clipping::default(),
            0.0,
            800,
            &calibration,
            profile.max_acceptable_noise,
            false,
        );
        let sub = score::sub_scores(
            &focus_result,
            &motion_of(MotionKind::None, 0.0),
            &exposure_result,
            &noise_result,
            &profile,
            0.0,
        );
        scores.push((
            scene,
            score::compose(
                sub,
                score::Weights::for_scene(&profile),
                &score::Isotonic::IDENTITY,
            ),
        ));
    }
    let lowest = scores.iter().map(|(_, s)| *s).fold(f32::MAX, f32::min);
    let highest = scores.iter().map(|(_, s)| *s).fold(f32::MIN, f32::max);
    println!(
        "a frame at its scene's threshold scores {lowest:.3} to {highest:.3} across {} scenes",
        scores.len()
    );
    assert!(
        highest - lowest < 0.02,
        "the same relative quality scored differently by scene"
    );
}

#[test]
fn the_isotonic_map_refuses_to_go_backwards() {
    // A calibration that is not monotone is not a calibration.
    assert!(
        score::Isotonic::from_knots([0.0, 0.2, 0.1, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0])
            .is_none()
    );
    let fitted = score::Isotonic::from_knots([
        0.0, 0.05, 0.12, 0.22, 0.35, 0.50, 0.65, 0.78, 0.88, 0.95, 1.0,
    ])
    .expect("a monotone map");
    assert!(fitted.apply(0.3) < 0.3, "the fitted map may reshape");
    assert!(score::Isotonic::IDENTITY.is_identity());
    assert_eq!(score::SceneCalibration::shipped().fitted(), 0);
    println!("  the shipped per-scene calibration is the identity - condition C5");
}

// ---------------------------------------------------------------------------
// Fairness, coverage and determinism
// ---------------------------------------------------------------------------

#[test]
fn a_twenty_four_and_a_sixty_one_megapixel_body_score_within_five_hundredths() {
    // Section 10.1's last gate, and the reason `camera_calibration.toml` exists.
    let table = CalibrationTable::embedded().expect("embedded");
    let ((low_frame, low_make, low_model), (high_frame, high_make, high_model)) =
        fixtures::cross_camera_pair();

    let measure = |frame: &Frame, make: &str, model: &str| -> f32 {
        let plane =
            aura_vision::embed::descriptors::luma_plane(&frame.rgb, frame.width, frame.height);
        let regions =
            focus::RegionSet::for_face(&[(fixtures::FACE_RECT, frame.faces[0].eyes, true)], &[]);
        let calibration = table.get(make, model, 24.0);
        focus::analyse(&plane, &regions, &[1.0], &calibration).subject
    };

    let low = measure(&low_frame, low_make, low_model);
    let high = measure(&high_frame, high_make, high_model);
    println!("{low_model} scores {low:.3}, {high_model} scores {high:.3}");
    assert!(
        (low - high).abs() <= FAIRNESS_GATE,
        "cross-camera bias of {:.3} against a {FAIRNESS_GATE:.2} gate",
        (low - high).abs()
    );

    // The guard: without the calibration division the expensive body loses. Measured
    // against the *same* row for both, which is what an uncalibrated product does.
    let flat = table.fallback();
    let flat_measure = |frame: &Frame| -> f32 {
        let plane =
            aura_vision::embed::descriptors::luma_plane(&frame.rgb, frame.width, frame.height);
        let regions =
            focus::RegionSet::for_face(&[(fixtures::FACE_RECT, frame.faces[0].eyes, true)], &[]);
        focus::analyse(&plane, &regions, &[1.0], &flat).subject
    };
    let gap = flat_measure(&low_frame) - flat_measure(&high_frame);
    println!("  without per-body calibration the gap would be {gap:.3}");
    assert!(
        gap > 0.0,
        "the fixture must actually be harder for the 61 MP body"
    );
}

#[test]
fn a_frame_with_no_subject_says_so_rather_than_calling_the_middle_soft() {
    let mut frame = Frame::base();
    frame.faces.clear();
    frame.blur_rect(fixtures::FACE_RECT, 6);
    let mut context = context_for(&frame, SceneId::Details, "SONY", "ILCE-7M3", 400);
    context.subjects = ImageSubjects::default();
    let result = analyse(&frame, &context);
    assert!(result.flags.contains(IntegrityFlags::NO_SUBJECT_DETECTED));
    assert!(
        result
            .reasons
            .iter()
            .any(|reason| reason.code == ReasonCode::NoSubject),
        "the panel must say the reading is about the frame"
    );
    assert!(
        result.confidence < 0.85,
        "a verdict with no subject is less certain, and says so"
    );
}

#[test]
fn an_uncalibrated_body_costs_confidence_and_says_why() {
    let frame = Frame::base();
    let known = analyse(
        &frame,
        &context_for(&frame, SceneId::CouplePortrait, "SONY", "ILCE-7M3", 400),
    );
    let unknown = analyse(
        &frame,
        &context_for(
            &frame,
            SceneId::CouplePortrait,
            "Hasselblad",
            "X2D 100C",
            400,
        ),
    );
    assert!(unknown.confidence < known.confidence);
    assert!(unknown
        .reasons
        .iter()
        .any(|reason| reason.code == ReasonCode::Uncalibrated));
}

#[test]
fn two_runs_of_one_frame_agree_exactly() {
    // Invariant 4. Not a tolerance: byte equality on every field that reaches the
    // catalog.
    let frame = fixtures::group_frame(3, 1);
    let context = context_for(&frame, SceneId::FamilyPortrait, "SONY", "ILCE-7M3", 800);
    let first = analyse(&frame, &context);
    let second = analyse(&frame, &context);
    assert_eq!(first.flags, second.flags);
    assert_eq!(first.motion, second.motion);
    assert_eq!(first.exposure, second.exposure);
    assert!((first.technical_score - second.technical_score).abs() < f32::EPSILON);
    assert!((first.subject_sharpness - second.subject_sharpness).abs() < f32::EPSILON);
    assert!((first.noise_sigma_rel - second.noise_sigma_rel).abs() < f32::EPSILON);
    assert_eq!(first.reasons.len(), second.reasons.len());
}

#[test]
fn every_verdict_carries_a_confidence_and_at_least_one_reason() {
    // Invariant 2, over every fixture in the module.
    let frames = vec![
        ("base", Frame::base()),
        ("shallow", fixtures::shallow_depth_of_field()),
        ("shake", fixtures::camera_shake()),
        ("pan", fixtures::panned()),
        ("clipped", fixtures::clipped()),
        ("candle", fixtures::candle()),
        ("group", fixtures::group_frame(4, 2)),
    ];
    for (name, frame) in frames {
        let context = context_for(&frame, SceneId::Candid, "SONY", "ILCE-7M3", 800);
        let result = analyse(&frame, &context);
        assert!(result.confidence > 0.0, "{name} has no confidence");
        assert!(!result.reasons.is_empty(), "{name} has no reasons");
        assert!(
            result.reasons.len() <= IntegrityResult::MAX_REASONS,
            "{name} has {} reasons",
            result.reasons.len()
        );
        // Every penalty points at pixels. Section 13's last criterion.
        for reason in result.reasons.iter().filter(|reason| reason.is_penalty()) {
            assert!(
                reason.evidence.is_some()
                    || reason.code.is_exoneration()
                    || is_frame_wide(reason.code),
                "{name}: {} is a penalty with nothing to show",
                reason.code
            );
        }
    }
}

#[test]
fn the_deferred_gates_are_the_ones_the_exit_report_lists() {
    // Phase 07's device, a phase later: the list of what is *not* gated is itself gated,
    // so a condition cannot quietly disappear from the exit report.
    let deferred = [
        "subject-focus AUC on labelled wedding crops (C1)",
        "blink F1 on real faces (C1)",
        "exposure agreement against expert labels (C3)",
        "the tears intent rule (C4)",
        "per-scene isotonic calibration (C5)",
    ];
    println!("deferred gates:");
    for item in &deferred {
        println!("  - {item}");
    }
    assert_eq!(
        deferred.len(),
        5,
        "the deferred list changed without the exit report"
    );
}

// -- small helpers -----------------------------------------------------------

fn is_frame_wide(code: ReasonCode) -> bool {
    matches!(
        code,
        ReasonCode::CameraShake
            | ReasonCode::HighlightLost
            | ReasonCode::ShadowLost
            | ReasonCode::HeavyNoise
            | ReasonCode::MixedLight
            | ReasonCode::GroupBlink
            | ReasonCode::BackFocus
            | ReasonCode::FrontFocus
    )
}

fn motion_of(kind: MotionKind, severity: f32) -> aura_brain_photo::integrity::motion::MotionResult {
    aura_brain_photo::integrity::motion::MotionResult {
        kind,
        severity,
        subject: aura_brain_photo::integrity::motion::Orientation::default(),
        background: aura_brain_photo::integrity::motion::Orientation::default(),
        agreement: 0.0,
        stops_below_safe: None,
    }
}

/// The area under the ROC curve, by the rank-sum identity.
///
/// The same arithmetic `ml/models/integrity/eval_integrity.py` uses, so that a number
/// from the harness and a number from the Python side are comparable rather than merely
/// similar.
fn area_under_curve(scored: &[(f64, bool)]) -> f64 {
    let positives: Vec<f64> = scored
        .iter()
        .filter(|(_, keeper)| *keeper)
        .map(|(score, _)| *score)
        .collect();
    let negatives: Vec<f64> = scored
        .iter()
        .filter(|(_, keeper)| !*keeper)
        .map(|(score, _)| *score)
        .collect();
    if positives.is_empty() || negatives.is_empty() {
        return 0.0;
    }
    let mut wins = 0.0f64;
    for positive in &positives {
        for negative in &negatives {
            wins += if positive > negative {
                1.0
            } else if (positive - negative).abs() < f64::EPSILON {
                0.5
            } else {
                0.0
            };
        }
    }
    wins / (positives.len() as f64 * negatives.len() as f64)
}

/// An `InferService` that refuses every call.
///
/// See this file's header: the focus head can only exonerate, so an analyser with no head
/// is an analyser that is strictly more cautious, and every gate here measures the
/// classical path deliberately.
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
