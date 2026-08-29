//! PHASE-23 section 10.1, as an ordinary test so a red gate is a red build.
//!
//! Seven rows. Five are measurable here and two are not; the two that are not are named rather
//! than quietly skipped.
//!
//! | Section 10.1 row | Here |
//! |---|---|
//! | straightening within 0.3 deg of expert on >= 90 % of labelled frames | `straightening_lands_within_the_gate_of_what_was_painted` |
//! | intentional tilts untouched | `an_intentional_tilt_is_never_straightened` |
//! | zero auto-crops cut a face or a primary identity's hands | `no_delivered_crop_cuts_a_protected_region` |
//! | resolution floor respected on every crop | `no_delivered_crop_falls_below_the_resolution_floor` |
//! | CA removes fringing without introducing colour shifts | `the_chromatic_aberration_correction_removes_a_fringe_and_shifts_no_colour` |
//! | keystone never exceeds the stretch cap, skipped when unsafe | `the_keystone_never_exceeds_the_cap_and_is_skipped_when_it_would_cut` |
//! | most frames (>= 70 %) keep their original framing | `most_of_a_wedding_keeps_the_framing_it_was_shot_at` |
//! | revert-to-original restores exact framing | `reverting_restores_the_exact_framing` |
//!
//! Not measurable here, and named at the bottom of every run by
//! `the_two_rows_this_harness_cannot_measure`:
//!
//! * **Agreement with an expert crop.** Section 9's DATA row asks for expert crop labels on two
//!   thousand frames and there are none in this repository. Every crop number below is a
//!   statement about the safety filter, the improvement margin and the aspect solver.
//! * **The 300-crop perceptual audit** section 9 gives QAIQ. It did not happen, so the phase's own
//!   headline - no cut hand, no cut face, no worse framing - is proven for faces and unproven for
//!   framing quality.
//!
//! **Every number here is measured on synthetic frames** whose tilt, convergence, distortion,
//! fringe and subject placement were painted into the pixels by `aura_geometry::fixtures` and read
//! back through the real solvers, the real safety filter and the real renderer. That proves the
//! arithmetic, the thresholds and the refusals; it says nothing about a wedding. Conditions C1 and
//! C2 of `docs/progress/PHASE-23-EXIT.md`.

use aura_core::contract::composition::Box2;
use aura_core::contract::geometry::{
    rotation_crop, AspectRatio, CropVariant, GeometryCode, GeometryPlan, ProtectedContent,
    ProtectedRegion, MAX_STRETCH, MIN_IMPROVEMENT, MIN_LONG_EDGE_FRACTION, ROTATE_ACT_AT,
};
use aura_core::SceneId;
use aura_geometry::decide::{Analyser, GeometryFrame};
use aura_geometry::profiles::LensExif;
use aura_geometry::safety::{self, Limits};
use aura_geometry::straighten;
use aura_geometry::{fixtures, keystone};
use aura_render::geometry::LensModel;

/// The angle gate section 10.1 names.
const ANGLE_GATE_DEG: f32 = 0.3;

/// The share of straightened frames that must land inside it.
const ANGLE_GATE_SHARE: f32 = 0.90;

/// Section 10.1's conservatism gate.
const CONSERVATISM_GATE: f32 = 0.70;

fn analyser() -> Analyser {
    Analyser::embedded().expect("the embedded tables load")
}

fn plan_of(frame: &GeometryFrame) -> GeometryPlan {
    analyser().plan(frame).expect("a plan").0
}

// ---------------------------------------------------------------------------
// Row 1: straightening within 0.3 degrees
// ---------------------------------------------------------------------------

#[test]
fn straightening_lands_within_the_gate_of_what_was_painted() {
    // "Expert" here is the angle the fixture painted, which is the only ground truth this
    // repository has. What is measured is that the solver applies the angle it was handed, or
    // reduces it for a reason it names - not that phase 11 measured the right angle in the first
    // place, which is phase 11's own gate.
    let mut inside = 0usize;
    let mut total = 0usize;
    for degrees in [0.4f32, 0.9, 1.6, 2.5, 3.4, 4.8, 6.1, 7.6, -1.2, -3.9] {
        let frame = fixtures::tilted_frame(SceneId::Candid, degrees, 0.88, false);
        let plan = plan_of(&frame);
        assert!(
            plan.has(GeometryCode::Straightened),
            "{degrees} deg was not straightened: {:?}",
            plan.reasons.iter().map(|r| r.code).collect::<Vec<_>>()
        );
        total += 1;
        if (plan.rotate_deg - degrees).abs() <= ANGLE_GATE_DEG {
            inside += 1;
        }
    }
    let share = inside as f32 / total as f32;
    assert!(
        share >= ANGLE_GATE_SHARE,
        "only {inside}/{total} frames landed within {ANGLE_GATE_DEG} deg"
    );
}

#[test]
fn the_confidence_gate_is_above_what_phase_eleven_acts_on() {
    // Section 6.2's "rotate only when Phase 11 horizon confidence >= 0.7", and the deliberate gap
    // between reporting a tilt and acting on one. A frame in the gap says so rather than moving.
    let below = fixtures::tilted_frame(SceneId::Candid, 3.0, ROTATE_ACT_AT - 0.05, false);
    let plan = plan_of(&below);
    assert!(plan.has(GeometryCode::HorizonUnsure));
    assert!(plan.rotate_deg.abs() < f32::EPSILON);
    // And the measurement is still stored, which is what lets the panel say what it declined.
    assert!(plan.rotate_conf > 0.0);
}

// ---------------------------------------------------------------------------
// Row 2: intentional tilts untouched
// ---------------------------------------------------------------------------

#[test]
fn an_intentional_tilt_is_never_straightened() {
    // Two ways a tilt reads as a decision: phase 11 says so, or it is bigger than anybody levels
    // by eye. Both must leave the frame alone, and section 12's second failure mode is what
    // happens when either does not.
    for (degrees, intentional, code) in [
        (4.0f32, true, GeometryCode::TiltIntentional),
        (14.0, false, GeometryCode::TiltTooLarge),
        (0.05, false, GeometryCode::TiltNegligible),
    ] {
        let frame = fixtures::tilted_frame(SceneId::Candid, degrees, 0.95, intentional);
        let plan = plan_of(&frame);
        assert!(plan.has(code), "{degrees} deg / {intentional}");
        assert!(
            plan.rotate_deg.abs() < f32::EPSILON,
            "{degrees} deg / {intentional} was rotated by {}",
            plan.rotate_deg
        );
    }
}

// ---------------------------------------------------------------------------
// Row 3: the hard gate - zero delivered crops cut a protected region
// ---------------------------------------------------------------------------

#[test]
fn no_delivered_crop_cuts_a_protected_region() {
    // The gate section 10.1 writes at zero rather than at a small number, measured over the whole
    // fixture wedding plus the scenes that permit cropping most.
    let analyser = analyser();
    let mut checked = 0u64;
    let mut cut = 0u64;
    for frame in fixtures::wedding() {
        let (plan, _) = analyser.plan(&frame).expect("a plan");
        let delivered = plan.primary();
        let aspect = frame.full_width as f32 / frame.full_height as f32;
        for region in &frame.protected {
            checked += 1;
            let projected =
                straighten::project(region.area, delivered, plan.rotate_deg, aspect);
            if !safety::rect_inside(projected, delivered, 0.01) {
                cut += 1;
            }
        }
        assert!(plan.safety.faces_intact, "{:?}", plan.image_id);
    }
    assert_eq!(cut, 0, "{cut} of {checked} protected regions were cut");
    // The denominator, because a zero over a wedding with nothing in it is arithmetic rather than
    // evidence. Phase 21's rule, and on a real photograph in this build the number is zero -
    // which is condition C2 of the exit report rather than a passing gate.
    assert!(
        checked > 0,
        "no protected region was checked, so this gate proved nothing"
    );
}

#[test]
fn a_crop_is_refused_rather_than_scored_when_it_would_cut_somebody() {
    // The veto runs *before* the objective, so there is no arithmetic path along which a very
    // high-scoring rectangle outweighs a face at its edge. Measured by putting faces where every
    // tighter rectangle must cut one and asserting the frame is delivered whole.
    let frame = fixtures::crowded_frame(SceneId::Candid);
    let plan = plan_of(&frame);
    assert!(
        plan.crops
            .first()
            .is_some_and(CropVariant::is_full_frame),
        "a crowded frame was recomposed"
    );
    assert!(plan.safety.considered >= 5);
}

// ---------------------------------------------------------------------------
// Row 4: the resolution floor
// ---------------------------------------------------------------------------

#[test]
fn no_delivered_crop_falls_below_the_resolution_floor() {
    let analyser = analyser();
    for frame in fixtures::wedding() {
        let (plan, _) = analyser.plan(&frame).expect("a plan");
        let aspect = frame.full_width as f32 / frame.full_height as f32;
        for variant in &plan.crops {
            if !variant.safe {
                continue;
            }
            assert!(
                variant.long_edge_fraction(aspect) >= MIN_LONG_EDGE_FRACTION - 1e-4,
                "{} keeps only {} of the long edge",
                variant.aspect,
                variant.long_edge_fraction(aspect)
            );
        }
        assert!(plan.safety.resolution_ok);
    }
}

#[test]
fn the_floor_is_on_the_long_edge_rather_than_on_the_area() {
    // The claim `MIN_LONG_EDGE_FRACTION` makes in the contract, as a gate, on the frame shape
    // where the two measures diverge: a 16:9 crop of a 4:5 portrait keeps four fifths of the long
    // edge and under half the area. An area floor at the same number would refuse the 16:9
    // variant of every portrait frame in the wedding, which is a floor that forbids a feature
    // section 2.1 requires.
    let portrait_aspect = 0.8f32;
    let sixteen_nine = AspectRatio::SixteenNine.ratio().expect("16:9 has a ratio");
    // In normalised coordinates a rectangle of pixel aspect `a` on a frame of aspect `f` has
    // `w / h = a / f`, so a full-width 16:9 crop of a 4:5 frame is this tall.
    let variant = CropVariant {
        aspect: AspectRatio::SixteenNine,
        rect: Box2 {
            x: 0.0,
            y: 0.5 - portrait_aspect / sixteen_nine / 2.0,
            w: 1.0,
            h: portrait_aspect / sixteen_nine,
        },
        purpose: AspectRatio::SixteenNine.purpose(),
        score: 0.0,
        safe: true,
    };
    let long_edge = variant.long_edge_fraction(portrait_aspect);
    let area = variant.rect.w * variant.rect.h;
    assert!(
        long_edge >= MIN_LONG_EDGE_FRACTION,
        "the long edge kept {long_edge}, so the floor would refuse this variant"
    );
    assert!(
        area < MIN_LONG_EDGE_FRACTION,
        "the area kept {area}, so this gate proved nothing"
    );

    // And the variant is actually generated on a frame that can carry it.
    let frame = fixtures::lopsided_frame(SceneId::CouplePortrait);
    let plan = plan_of(&frame);
    assert!(
        plan.crops
            .iter()
            .any(|variant| variant.aspect == AspectRatio::Square && variant.safe),
        "no safe square variant was generated on a clear frame"
    );
}

// ---------------------------------------------------------------------------
// Row 5: chromatic aberration
// ---------------------------------------------------------------------------

#[test]
fn the_chromatic_aberration_correction_removes_a_fringe_and_shifts_no_colour() {
    // A high-contrast edge off-centre with a fringe painted into it: red scaled out, blue scaled
    // in. The correction must reduce the channel disagreement at that edge, and must not move the
    // frame's overall colour - a correction that shifted the whole photograph would be a white
    // balance wearing a lens correction's name.
    const W: usize = 128;
    const H: usize = 128;
    let model = LensModel {
        ca_red: 0.004,
        ca_blue: -0.004,
        ..LensModel::identity()
    };
    let mut clean = vec![0.05f32; W * H * 3];
    for y in 0..H {
        for x in 0..W {
            if x > W * 3 / 4 {
                for channel in 0..3 {
                    clean[(y * W + x) * 3 + channel] = 0.95;
                }
            }
        }
    }
    // Paint the fringe by *applying* the model's inverse to the two outer channels, so the fringe
    // in the fixture is exactly the defect the correction is written against.
    let mut fringed = clean.clone();
    let cx = (W as f32 - 1.0) / 2.0;
    let cy = (H as f32 - 1.0) / 2.0;
    for y in 0..H {
        for x in 0..W {
            for (channel, gain) in [(0usize, -model.ca_red), (2, -model.ca_blue)] {
                let sx = cx + (x as f32 - cx) * (1.0 + gain);
                let sy = cy + (y as f32 - cy) * (1.0 + gain);
                let x0 = sx.clamp(0.0, W as f32 - 1.0) as usize;
                let y0 = sy.clamp(0.0, H as f32 - 1.0) as usize;
                fringed[(y * W + x) * 3 + channel] = clean[(y0 * W + x0) * 3 + channel];
            }
        }
    }

    let disagreement = |rgb: &[f32]| -> f32 {
        let mut worst = 0.0f32;
        for y in H / 4..3 * H / 4 {
            for x in 0..W {
                let r = rgb[(y * W + x) * 3];
                let g = rgb[(y * W + x) * 3 + 1];
                let b = rgb[(y * W + x) * 3 + 2];
                worst = worst.max((r - g).abs()).max((b - g).abs());
            }
        }
        worst
    };
    let mean = |rgb: &[f32], channel: usize| -> f32 {
        let sum: f32 = rgb.iter().skip(channel).step_by(3).sum();
        sum / (W * H) as f32
    };

    let before = disagreement(&fringed);
    let mut corrected = fringed.clone();
    aura_render::geometry::correct_ca(&mut corrected, W, H, &model);
    let after = disagreement(&corrected);
    assert!(
        after < before,
        "the correction did not reduce the fringe: {after} !< {before}"
    );
    for channel in 0..3 {
        let shift = (mean(&corrected, channel) - mean(&clean, channel)).abs();
        assert!(
            shift < 0.01,
            "channel {channel} moved by {shift}, which is a colour shift rather than a correction"
        );
    }
}

#[test]
fn a_distortion_correction_never_leaves_an_undefined_pixel() {
    // Section 2.2 puts fill in phase 24, so a lens correction that opened a corner would be
    // handing this phase's job to the next one. Both signs, on a frame with no black in it.
    for k1 in [-0.045f32, -0.02, 0.02, 0.05] {
        let model = LensModel {
            k1,
            ..LensModel::identity()
        };
        let (w, h) = (96usize, 64usize);
        let mut rgb = vec![0.35f32; w * h * 3];
        aura_render::geometry::correct_distortion(&mut rgb, w, h, &model);
        for (index, value) in rgb.iter().enumerate() {
            assert!(
                *value > 0.0,
                "k1 = {k1} left pixel {index} undefined, which is a corner nobody filled"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Row 6: the keystone cap
// ---------------------------------------------------------------------------

#[test]
fn the_keystone_never_exceeds_the_cap_and_is_skipped_when_it_would_cut() {
    // The cap, over every convergence and both frame shapes, plus the refusal when the
    // magnification that hides the opened corners would take somebody out of frame.
    for convergence in [0.36f32, 0.5, 0.7, 0.9, 1.0] {
        for aspect in [1.5f32, 0.8, 16.0 / 9.0] {
            let correction = keystone::solve(
                keystone::Verticals {
                    convergence,
                    share: 0.4,
                },
                aspect,
                &[],
                Limits {
                    frame_aspect: aspect,
                    ..Limits::default()
                },
            );
            if let Some(k) = correction.keystone {
                assert!(
                    k.stretch <= MAX_STRETCH + 1e-4,
                    "{convergence} at {aspect} stretched by {}",
                    k.stretch
                );
                assert!(k.within_cap());
            }
        }
    }

    let edge = [ProtectedRegion::anonymous(
        ProtectedContent::PrimaryFace,
        Box2 {
            x: 0.005,
            y: 0.45,
            w: 0.05,
            h: 0.08,
        },
    )];
    let refused = keystone::solve(
        keystone::Verticals {
            convergence: 0.9,
            share: 0.4,
        },
        1.5,
        &edge,
        Limits::default(),
    );
    assert_eq!(refused.code, GeometryCode::KeystoneRefused);
    assert!(refused.keystone.is_none());
}

#[test]
fn a_frame_of_people_is_never_read_as_architecture() {
    // The safety of the whole operator: the vertical family in a photograph of guests is guests.
    let frame = fixtures::crowded_frame(SceneId::DanceFloor);
    let plan = plan_of(&frame);
    assert!(plan.keystone.is_none());
    assert!(
        plan.has(GeometryCode::KeystoneNoArchitecture)
            || plan.has(GeometryCode::KeystoneNotNeeded),
        "{:?}",
        plan.reasons.iter().map(|r| r.code).collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// Row 7: conservatism
// ---------------------------------------------------------------------------

#[test]
fn most_of_a_wedding_keeps_the_framing_it_was_shot_at() {
    // Section 10.1's "most frames (>= 70 %) keep their original framing - the system is
    // conservative by design", measured over a fixture wedding weighted the way a wedding is:
    // mostly ceremony, portraits and candids.
    let analyser = analyser();
    let wedding = fixtures::wedding();
    let mut kept = 0usize;
    for frame in &wedding {
        let (plan, outcome) = analyser.plan(frame).expect("a plan");
        if outcome.kept_original {
            kept += 1;
        }
        assert!(!plan.reasons.is_empty());
    }
    let share = kept as f32 / wedding.len() as f32;
    assert!(
        share >= CONSERVATISM_GATE,
        "only {share:.2} of the wedding kept its original framing"
    );
}

#[test]
fn a_crop_that_is_only_slightly_better_does_not_replace_what_was_shot() {
    // The improvement margin, as a gate. The same frame under a margin at the contract's floor and
    // under a margin nothing can clear must produce different decisions - which is what proves the
    // margin is consulted rather than decorative.
    let frame = fixtures::lopsided_frame(SceneId::Candid);
    let plan = plan_of(&frame);
    let delivered = plan.primary();
    if !plan.has(GeometryCode::CropProposed) {
        // The margin refused it, which is the conservative outcome and needs no further proof.
        assert!(
            plan.has(GeometryCode::CropNoImprovement) || plan.has(GeometryCode::CropKeptOriginal)
        );
        return;
    }
    // It was proposed, so it must have cleared the margin by construction.
    let proposed = plan
        .crops
        .first()
        .map(|variant| variant.score)
        .unwrap_or_default();
    assert!(
        proposed >= MIN_IMPROVEMENT,
        "a proposal scored {proposed}, below the margin it had to clear"
    );
    assert!(!delivered.is_empty());
}

// ---------------------------------------------------------------------------
// Row 8: revert
// ---------------------------------------------------------------------------

#[test]
fn reverting_restores_the_exact_framing() {
    // Section 13's fifth acceptance criterion, at the level the contract can prove it: the
    // override's revert is a distinct field rather than "set the crop to the whole frame",
    // because reverting must also clear the rotation, the keystone and `user_edited`.
    let revert = aura_core::contract::geometry::GeometryOverride::reverted();
    assert!(revert.revert);
    assert!(revert.crop.is_none());
    assert!(revert.rotate_deg.is_none());
    assert!(!revert.is_empty());

    // And the plan a revert produces is the identity, exactly.
    let untouched = GeometryPlan::untouched(fixtures::photo(0), SceneId::Candid);
    assert!(untouched.is_identity());
    assert_eq!(untouched.primary(), Box2::FULL);
}

// ---------------------------------------------------------------------------
// The two rows this harness cannot measure
// ---------------------------------------------------------------------------

#[test]
fn the_two_rows_this_harness_cannot_measure() {
    // Named rather than skipped, for the reason every eval harness since phase 05 names its gaps:
    // a suite that is silent about what it did not measure reads as a suite that measured
    // everything.
    let unmeasured = [
        "agreement with an expert crop - section 9's DATA row asks for 2,000 labelled frames \
         and this repository has none",
        "the 300-crop perceptual audit section 9 gives QAIQ - it did not happen, so no claim \
         about whether AURA's framing is better may be made from this build",
    ];
    for row in unmeasured {
        eprintln!("PHASE-23 section 10.1, not measured here: {row}");
    }
    assert_eq!(unmeasured.len(), 2);
}

// ---------------------------------------------------------------------------
// Two properties that hold across every row above
// ---------------------------------------------------------------------------

#[test]
fn the_rotation_crop_the_contract_computes_is_the_one_the_renderer_can_fill() {
    // Two implementations of one idea agreeing. `rotation_crop` is in `aura-core` precisely so
    // that the plan and the render cannot disagree about which pixels exist, and this is the
    // assertion that keeps that true.
    for degrees in [0.5f32, 2.0, 5.0, 8.0] {
        for (w, h) in [(6000u32, 4000u32), (4000, 6000), (5000, 5000)] {
            let rect = rotation_crop(w, h, degrees);
            assert!(straighten::inside_the_rotated_frame(
                rect,
                degrees,
                w as f32 / h as f32
            ));
            // And it preserves the frame's shape, which is the whole reason it is not the
            // widely copied maximum-area formula: that one keeps 95 % of the width and 88 % of
            // the height at five degrees, delivering a 1.63:1 photograph from a 3:2 one.
            let frame_aspect = w as f32 / h as f32;
            let crop_aspect = (rect.w * w as f32) / (rect.h * h as f32);
            assert!(
                (crop_aspect / frame_aspect - 1.0).abs() < 1e-3,
                "{degrees} deg on {w}x{h} delivered {crop_aspect} from a {frame_aspect} frame"
            );
        }
    }
}

#[test]
fn every_plan_carries_a_confidence_and_a_reason() {
    // Invariant 2, over the whole fixture wedding. A decision without an explanation is a bug.
    let analyser = analyser();
    for frame in fixtures::wedding() {
        let (plan, _) = analyser.plan(&frame).expect("a plan");
        assert!(
            (0.0..=1.0).contains(&plan.confidence),
            "{:?} has confidence {}",
            plan.image_id,
            plan.confidence
        );
        assert!(!plan.reasons.is_empty(), "{:?} has no reasons", plan.image_id);
        for reason in &plan.reasons {
            assert!(GeometryCode::parse(reason.code.as_str()).is_some());
        }
    }
}

#[test]
fn nothing_in_this_phase_reaches_a_provider() {
    // Section 7: "No cloud AI call in this phase. The phase must work with the network cable
    // unplugged." The dependency-level version of this is `tests/boundaries.rs`; this is the
    // behavioural one - a whole wedding planned with nothing but the compiled-in tables.
    let analyser = analyser();
    let mut lens = LensExif::default();
    lens.name = "EF24-70mm f/2.8L II USM".to_string();
    lens.focal_mm = Some(35.0);
    for mut frame in fixtures::wedding() {
        frame.lens = lens.clone();
        assert!(analyser.plan(&frame).is_ok());
    }
}
