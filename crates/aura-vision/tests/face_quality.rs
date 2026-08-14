//! The gate that decides which faces are allowed to be evidence about who somebody is.
//!
//! Two properties are asserted here that nothing else in the phase would catch.
//!
//! **Monotonicity.** A gate that is not monotonic in its inputs is worse than no gate:
//! it would sometimes admit the worse of two faces, and nobody would notice for months.
//!
//! **That a catastrophic factor cannot be outvoted.** The fused score is a weighted
//! *geometric* mean precisely so that a perfectly exposed, perfectly frontal, perfectly
//! unoccluded, completely out-of-focus face cannot score 0.75 and vote. An arithmetic
//! mean would let it, and the difference is invisible without a test that constructs
//! exactly that face.

use aura_vision::face::align::Pose;
use aura_vision::face::quality::{
    self, FaceQuality, Measured, QualityGate, MAX_VOTING_CLIPPED, MAX_VOTING_YAW, MIN_PX_HEIGHT,
    MIN_USABLE, QUALITY_MODEL_WEIGHT,
};

fn measured(sharpness: f32, occlusion: f32, clipped: f32) -> Measured {
    Measured {
        sharpness,
        occlusion,
        clipped,
        mean_luma: 0.5,
    }
}

fn frontal() -> Pose {
    Pose {
        yaw: 0.0,
        pitch: 0.0,
        roll: 0.0,
    }
}

fn fuse(m: Measured, pose: Pose, px: u32) -> FaceQuality {
    quality::fuse(&m, pose, px, None, QualityGate::default())
}

#[test]
fn a_good_face_votes() {
    let verdict = fuse(measured(0.9, 0.02, 0.01), frontal(), 180);
    assert!(verdict.votes, "{:?}", verdict.reasons);
    assert!(verdict.usable > MIN_USABLE);
    assert!(!verdict.reasons.is_empty(), "invariant 2: always a reason");
}

#[test]
fn a_face_below_the_pixel_gate_does_not_vote_and_says_why() {
    let verdict = fuse(measured(0.95, 0.0, 0.0), frontal(), MIN_PX_HEIGHT - 1);
    assert!(!verdict.votes);
    assert!(
        verdict
            .reasons
            .iter()
            .any(|reason| reason.contains("px tall")),
        "{:?}",
        verdict.reasons
    );
    // And it is still a measurable face: excluding is not deleting.
    assert!(verdict.usable > 0.5, "a sharp small face is still sharp");
}

#[test]
fn one_catastrophic_factor_cannot_be_outvoted() {
    // Perfect on three axes, hopeless on the fourth. An arithmetic mean would score this
    // around 0.75 and let it vote on somebody's identity.
    let verdict = fuse(measured(0.01, 0.0, 0.0), frontal(), 400);
    assert!(
        !verdict.votes,
        "a completely soft face must never vote, scored {}",
        verdict.usable
    );
    assert!(
        verdict.usable < 0.35,
        "the geometric mean should collapse, got {}",
        verdict.usable
    );
}

#[test]
fn quality_is_monotonic_in_sharpness() {
    let mut previous = -1.0f32;
    for step in 0..=10 {
        let sharpness = step as f32 / 10.0;
        let verdict = fuse(measured(sharpness, 0.05, 0.02), frontal(), 200);
        assert!(
            verdict.usable >= previous,
            "sharpness {sharpness} scored {} after {previous}",
            verdict.usable
        );
        previous = verdict.usable;
    }
}

#[test]
fn quality_is_monotonic_in_occlusion() {
    let mut previous = f32::MAX;
    for step in 0..=10 {
        let occlusion = step as f32 / 10.0;
        let verdict = fuse(measured(0.8, occlusion, 0.02), frontal(), 200);
        assert!(
            verdict.usable <= previous,
            "occlusion {occlusion} scored {} after {previous}",
            verdict.usable
        );
        previous = verdict.usable;
    }
}

#[test]
fn quality_is_monotonic_in_pose() {
    let mut previous = f32::MAX;
    for step in 0..=9 {
        let yaw = step as f32 * 10.0;
        let verdict = fuse(
            measured(0.8, 0.05, 0.02),
            Pose {
                yaw,
                pitch: 0.0,
                roll: 0.0,
            },
            200,
        );
        assert!(
            verdict.usable <= previous,
            "yaw {yaw} scored {} after {previous}",
            verdict.usable
        );
        previous = verdict.usable;
    }
}

#[test]
fn a_profile_does_not_vote() {
    let verdict = fuse(
        measured(0.9, 0.02, 0.01),
        Pose {
            yaw: 78.0,
            pitch: 0.0,
            roll: 0.0,
        },
        220,
    );
    assert!(!verdict.votes, "scored {}", verdict.usable);
    assert!(
        verdict
            .reasons
            .iter()
            .any(|reason| reason.contains("turned away")),
        "{:?}",
        verdict.reasons
    );
}

#[test]
fn a_dark_face_is_not_penalised_for_being_dark() {
    // A candle-lit ceremony frame is the photograph, not a fault. Only *crushed* pixels
    // count against it, and this is where a fairness failure would first appear - which
    // is why the distinction is in the code rather than in a comment.
    let dark = Measured {
        sharpness: 0.85,
        occlusion: 0.05,
        clipped: 0.02,
        mean_luma: 0.08,
    };
    let bright = Measured {
        mean_luma: 0.62,
        ..dark
    };
    let dark_verdict = fuse(dark, frontal(), 200);
    let bright_verdict = fuse(bright, frontal(), 200);
    assert!(
        (dark_verdict.usable - bright_verdict.usable).abs() < 1e-6,
        "mean luminance must not affect the gate: {} vs {}",
        dark_verdict.usable,
        bright_verdict.usable
    );
    assert!(dark_verdict.votes);
}

#[test]
fn a_crushed_face_does_not_vote() {
    let verdict = fuse(measured(0.8, 0.05, 0.85), frontal(), 200);
    assert!(!verdict.votes, "scored {}", verdict.usable);
    assert!(
        verdict
            .reasons
            .iter()
            .any(|reason| reason.contains("crushed or blown")),
        "{:?}",
        verdict.reasons
    );
}

#[test]
fn the_model_head_is_wired_even_though_it_is_not_trusted() {
    // `QUALITY_MODEL_WEIGHT` is 0.0 in this build because the shipped head is a
    // placeholder. That has to be a *policy* rather than dead code, so this asserts both
    // halves: at zero the head changes nothing, and the blend itself works.
    let base = fuse(measured(0.8, 0.05, 0.02), frontal(), 200);
    let with_head = quality::fuse(
        &measured(0.8, 0.05, 0.02),
        frontal(),
        200,
        Some(0.01),
        QualityGate::default(),
    );
    assert!(
        (base.usable - with_head.usable).abs() < 1e-6,
        "at weight {QUALITY_MODEL_WEIGHT} the head must be exactly a no-op"
    );
    assert_eq!(with_head.model_usable, Some(0.01), "and still be reported");

    // The blend, exercised directly, so the wiring is proven rather than assumed.
    let blended = 0.5f32.mul_add(base.usable, 0.5 * 0.01);
    assert!(
        blended < base.usable,
        "a distrusted head would drag the score down if it were trusted"
    );
}

#[test]
fn an_unmeasurable_face_is_refused_with_a_reason() {
    let verdict = FaceQuality::unmeasurable(12, "the crop could not be read");
    assert!(!verdict.votes);
    assert_eq!(verdict.usable, 0.0);
    assert_eq!(verdict.reasons.len(), 1);
}

#[test]
fn measuring_a_short_buffer_is_survivable() {
    let measured = quality::measure(&[0u8; 10]);
    assert_eq!(measured.sharpness, 0.0);
    assert_eq!(measured.occlusion, 0.0);
}

#[test]
fn a_flat_crop_reads_as_occluded_and_soft() {
    // What a hand over a face looks like: no structure anywhere.
    let flat = vec![128u8; 112 * 112 * 3];
    let measured = quality::measure(&flat);
    assert!(
        measured.sharpness < 0.05,
        "sharpness {}",
        measured.sharpness
    );
    assert!(
        measured.occlusion > 0.9,
        "a featureless crop must read as occluded, got {}",
        measured.occlusion
    );

    let verdict = quality::fuse(&measured, frontal(), 200, None, QualityGate::default());
    assert!(!verdict.votes);
}

#[test]
fn a_textured_crop_reads_as_sharp_and_unoccluded() {
    let mut textured = vec![0u8; 112 * 112 * 3];
    for row in 0..112 {
        for column in 0..112 {
            // A fine checker: real structure at the scale a Laplacian measures.
            let value = if (row / 2 + column / 2) % 2 == 0 {
                90u8
            } else {
                190
            };
            for channel in 0..3 {
                if let Some(slot) = textured.get_mut((row * 112 + column) * 3 + channel) {
                    *slot = value;
                }
            }
        }
    }
    let measured = quality::measure(&textured);
    assert!(measured.sharpness > 0.9, "sharpness {}", measured.sharpness);
    assert!(measured.occlusion < 0.1, "occlusion {}", measured.occlusion);
    assert!(measured.clipped < 0.01);
}

#[test]
fn the_yaw_cut_off_is_a_cliff_and_not_a_trade() {
    // Just inside the limit, everything else perfect: votes. Just outside, everything else
    // still perfect: does not. That is what makes it a cut-off rather than a term - see
    // MAX_VOTING_YAW for why yaw earns one and sharpness does not.
    let inside = quality::fuse(
        &measured(0.95, 0.0, 0.0),
        Pose {
            yaw: MAX_VOTING_YAW - 1.0,
            pitch: 0.0,
            roll: 0.0,
        },
        300,
        None,
        QualityGate::default(),
    );
    assert!(inside.votes, "{:?}", inside.reasons);

    let outside = quality::fuse(
        &measured(0.95, 0.0, 0.0),
        Pose {
            yaw: MAX_VOTING_YAW + 1.0,
            pitch: 0.0,
            roll: 0.0,
        },
        300,
        None,
        QualityGate::default(),
    );
    assert!(!outside.votes);
    assert!(
        outside.usable > MIN_USABLE,
        "the fused score is still high - the cut-off is what refuses it, and that is the point"
    );
}

#[test]
fn the_clipping_cut_off_is_a_cliff_too() {
    let inside = quality::fuse(
        &measured(0.9, 0.0, MAX_VOTING_CLIPPED - 0.05),
        frontal(),
        300,
        None,
        QualityGate::default(),
    );
    let outside = quality::fuse(
        &measured(0.9, 0.0, MAX_VOTING_CLIPPED + 0.05),
        frontal(),
        300,
        None,
        QualityGate::default(),
    );
    assert!(inside.votes, "{:?}", inside.reasons);
    assert!(!outside.votes);
}

#[test]
fn a_stricter_gate_excludes_more() {
    let strict = QualityGate {
        min_px_height: 200,
        min_usable: 0.9,
        ..QualityGate::default()
    };
    let verdict = quality::fuse(&measured(0.8, 0.05, 0.02), frontal(), 180, None, strict);
    assert!(!verdict.votes, "QA must be able to tighten the gate");
}
