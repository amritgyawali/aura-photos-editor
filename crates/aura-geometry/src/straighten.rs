//! Levelling, with restraint. PHASE-23 section 6.2.
//!
//! Three gates before anything turns, and the third is the one that makes this phase
//! different from every straightening tool that ships in a RAW editor:
//!
//! 1. **Confidence.** Phase 11's horizon estimate must reach [`STRAIGHTEN_ACT_AT`], which is
//!    0.70 - deliberately higher than phase 11's own 0.60 floor. The difference is the
//!    difference between reporting an estimate and turning somebody's photograph by it.
//! 2. **The band.** Between [`MIN_ROTATE_DEG`] and [`MAX_ROTATE_DEG`]. Below the band the
//!    rotation costs a resample and buys a change nobody can see; above it, section 6.2 says
//!    "larger tilts are treated as intentional and left alone", and phase 11's own
//!    `tilt_intentional` withdraws the correction anywhere inside the band too.
//! 3. **The crop it implies.** A rotation is not free: levelling a frame means cropping to the
//!    largest rectangle that still fits inside it, and if that rectangle cuts a face or falls
//!    below the scene's resolution floor then the rotation is **reduced until it does not**,
//!    and abandoned if no angle works. Section 6.2's own words: "if it cannot, the rotation is
//!    reduced or skipped."
//!
//! The third gate is why this module returns a rectangle as well as an angle, and why it takes
//! the protected regions. A straightening tool that hands a caller an angle and lets the crop
//! be somebody else's problem is a tool that levels a family formal by cropping the
//! grandmother out of the left edge.

use aura_core::contract::geometry::{
    GeometryCode, GeometryReason, MAX_ROTATE_DEG, MIN_ROTATE_DEG, STRAIGHTEN_ACT_AT,
};
use aura_core::contract::integrity::CropRect;

use crate::safety::{self, SafetyInput};

/// How many angles the reduction loop tries before giving up.
///
/// Twelve, from the wanted angle down to zero. A bisection would find *an* angle that works
/// and this wants the **largest** one that does, which is not the same thing when the safe set
/// is not an interval - and it is not, because reducing the angle grows the inscribed
/// rectangle in one axis and shifts it in the other. Phase 15 wrote the same note about its
/// illuminant walk for the same reason.
pub const REDUCTION_STEPS: usize = 12;

/// What levelling one frame needs to know.
#[derive(Debug, Clone)]
pub struct StraightenInput {
    /// Phase 11's tilt, in degrees, positive clockwise.
    pub tilt_deg: f32,
    /// Phase 11's confidence in it.
    pub horizon_conf: f32,
    /// Phase 11's judgement that the tilt reads as a decision.
    pub tilt_intentional: bool,
    /// Width over height of the frame being levelled.
    pub aspect: f32,
}

/// What levelling decided.
#[derive(Debug, Clone, PartialEq)]
pub struct Straightened {
    /// The angle to turn by. Zero when nothing is turned.
    pub rotate_deg: f32,
    /// The confidence carried onto the plan.
    pub rotate_conf: f32,
    /// The rectangle the rotation implies, normalised. The whole frame when nothing turned.
    pub rect: CropRect,
    /// Why.
    pub reasons: Vec<GeometryReason>,
}

impl Straightened {
    /// Nothing turned, for one reason.
    fn untouched(code: GeometryCode, conf: f32) -> Self {
        Self {
            rotate_deg: 0.0,
            rotate_conf: conf,
            rect: CropRect::FULL,
            reasons: vec![GeometryReason::plain(code, -0.02)],
        }
    }
}

/// Decide how far to turn the frame, and to what rectangle.
#[must_use]
pub fn decide(input: &StraightenInput, safety: &SafetyInput<'_>) -> Straightened {
    if input.horizon_conf < STRAIGHTEN_ACT_AT {
        return Straightened::untouched(GeometryCode::HorizonUncertain, input.horizon_conf);
    }
    if input.tilt_intentional {
        return Straightened::untouched(GeometryCode::TiltIntentional, input.horizon_conf);
    }
    let wanted = -input.tilt_deg;
    if wanted.abs() < MIN_ROTATE_DEG {
        return Straightened::untouched(GeometryCode::TiltNegligible, input.horizon_conf);
    }
    if wanted.abs() > MAX_ROTATE_DEG {
        // Above the band a tilt is a decision even when phase 11 did not label it one. The
        // reason is the same one and it is worth saying with the same code, because a
        // photographer looking at an untouched eleven-degree frame wants to be told that AURA
        // read it as deliberate rather than that AURA failed.
        return Straightened::untouched(GeometryCode::TiltIntentional, input.horizon_conf);
    }

    // Gate three. Walk down from the wanted angle and take the first one whose implied crop is
    // safe. `REDUCTION_STEPS` inclusive of the wanted angle itself, so an unobstructed frame
    // is levelled fully on the first try.
    for step in 0..REDUCTION_STEPS {
        let scale = 1.0 - step as f32 / REDUCTION_STEPS as f32;
        let angle = wanted * scale;
        if angle.abs() < MIN_ROTATE_DEG {
            break;
        }
        let rect = inscribed(angle, input.aspect);
        if safety::is_safe(rect, safety) {
            let mut reasons = vec![GeometryReason::plain(GeometryCode::Levelled, 0.08)];
            if step > 0 {
                reasons.push(GeometryReason::frame(
                    GeometryCode::RotationReduced,
                    format!(
                        "The frame was levelled {:.1} degrees of the {:.1} it needed: turning \
                         it further would have cropped into somebody.",
                        angle.abs(),
                        wanted.abs()
                    ),
                    -0.04,
                ));
            }
            return Straightened {
                rotate_deg: angle,
                rotate_conf: input.horizon_conf,
                rect,
                reasons,
            };
        }
    }
    Straightened::untouched(GeometryCode::RotationRefused, input.horizon_conf)
}

/// The largest rectangle of the frame's own aspect ratio that fits inside it once rotated.
///
/// **The frame's own aspect ratio, not the largest area.** The classical
/// `rotatedRectWithMaxArea` result is bigger and is the wrong answer here: it returns a
/// rectangle whose shape depends on the angle, so levelling a 3:2 frame by two degrees would
/// deliver a 1.72:1 one and levelling it by four would deliver something else again. A
/// photographer who asked for a straighten did not ask for a reframe, and a gallery whose
/// frames are each a slightly different shape is a gallery that cannot be laid out.
///
/// With the shape fixed the closed form is short. A rectangle `sW x sH` fits inside a `W x H`
/// rectangle rotated by `t` exactly when its own rotated bounding box fits, which is two
/// inequalities:
///
/// ```text
///   sW cos + sH sin <= W        and        sW sin + sH cos <= H
/// ```
///
/// so `s` is the smaller of `W / (W cos + H sin)` and `H / (W sin + H cos)`, and there is
/// nothing to branch on.
#[must_use]
pub fn inscribed(angle_deg: f32, aspect: f32) -> CropRect {
    if angle_deg.abs() < f32::EPSILON || aspect <= 0.0 {
        return CropRect::FULL;
    }
    // Work in pixels of a unit-height frame, so the aspect is carried explicitly.
    let (w, h) = (aspect, 1.0f32);
    let radians = angle_deg.to_radians().abs();
    let (sin_a, cos_a) = (radians.sin(), radians.cos());
    let by_width = w / (w * cos_a + h * sin_a);
    let by_height = h / (w * sin_a + h * cos_a);
    let scale = by_width.min(by_height).clamp(0.0, 1.0);
    CropRect {
        x: (1.0 - scale) / 2.0,
        y: (1.0 - scale) / 2.0,
        w: scale,
        h: scale,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::geometry::{ProtectedKind, ProtectedRegion};

    const ASPECT: f32 = 1.5;

    fn open() -> SafetyInput<'static> {
        SafetyInput::permissive()
    }

    fn face_at(x: f32, y: f32) -> ProtectedRegion {
        ProtectedRegion {
            kind: ProtectedKind::Face,
            identity: None,
            rect: CropRect {
                x,
                y,
                w: 0.08,
                h: 0.10,
            },
            primary: true,
        }
    }

    #[test]
    fn a_clear_frame_is_levelled_fully() {
        let out = decide(
            &StraightenInput {
                tilt_deg: 2.4,
                horizon_conf: 0.86,
                tilt_intentional: false,
                aspect: ASPECT,
            },
            &open(),
        );
        assert!((out.rotate_deg + 2.4).abs() < 1e-4, "{}", out.rotate_deg);
        assert!(out.rect.w < 1.0, "a rotation always implies a crop");
        assert!(out.reasons.iter().any(|r| r.code == GeometryCode::Levelled));
    }

    #[test]
    fn nothing_below_the_confidence_gate_is_turned() {
        for conf in [0.0, 0.59, 0.69] {
            let out = decide(
                &StraightenInput {
                    tilt_deg: 3.0,
                    horizon_conf: conf,
                    tilt_intentional: false,
                    aspect: ASPECT,
                },
                &open(),
            );
            assert_eq!(out.rotate_deg, 0.0, "turned at confidence {conf}");
            assert!(out
                .reasons
                .iter()
                .any(|r| r.code == GeometryCode::HorizonUncertain));
        }
    }

    #[test]
    fn a_deliberate_tilt_and_a_large_one_are_both_left_alone() {
        let deliberate = decide(
            &StraightenInput {
                tilt_deg: 4.0,
                horizon_conf: 0.95,
                tilt_intentional: true,
                aspect: ASPECT,
            },
            &open(),
        );
        assert_eq!(deliberate.rotate_deg, 0.0);
        let large = decide(
            &StraightenInput {
                tilt_deg: 11.5,
                horizon_conf: 0.95,
                tilt_intentional: false,
                aspect: ASPECT,
            },
            &open(),
        );
        assert_eq!(large.rotate_deg, 0.0);
        for out in [deliberate, large] {
            assert!(out
                .reasons
                .iter()
                .any(|r| r.code == GeometryCode::TiltIntentional));
        }
    }

    #[test]
    fn a_tilt_below_the_band_is_negligible_rather_than_uncertain() {
        let out = decide(
            &StraightenInput {
                tilt_deg: 0.11,
                horizon_conf: 0.99,
                tilt_intentional: false,
                aspect: ASPECT,
            },
            &open(),
        );
        assert_eq!(out.rotate_deg, 0.0);
        assert!(out
            .reasons
            .iter()
            .any(|r| r.code == GeometryCode::TiltNegligible));
    }

    #[test]
    fn a_face_near_the_edge_reduces_the_rotation_rather_than_being_cropped() {
        // A face just inside the frame's left edge. The full 7-degree crop would cut it.
        let regions = vec![face_at(0.035, 0.44)];
        let input = SafetyInput {
            regions: &regions,
            aspect: ASPECT,
            resolution_floor: 0.60,
        };
        let out = decide(
            &StraightenInput {
                tilt_deg: 7.0,
                horizon_conf: 0.92,
                tilt_intentional: false,
                aspect: ASPECT,
            },
            &input,
        );
        assert!(
            out.rotate_deg == 0.0 || out.rotate_deg.abs() < 7.0,
            "the rotation was not reduced: {}",
            out.rotate_deg
        );
        if out.rotate_deg.abs() > 0.0 {
            assert!(safety::is_safe(out.rect, &input), "an unsafe rect survived");
            assert!(out
                .reasons
                .iter()
                .any(|r| r.code == GeometryCode::RotationReduced));
        } else {
            assert!(out
                .reasons
                .iter()
                .any(|r| r.code == GeometryCode::RotationRefused));
        }
    }

    #[test]
    fn a_face_dead_centre_never_blocks_a_rotation() {
        let regions = vec![face_at(0.46, 0.44)];
        let input = SafetyInput {
            regions: &regions,
            aspect: ASPECT,
            resolution_floor: 0.60,
        };
        let out = decide(
            &StraightenInput {
                tilt_deg: 3.0,
                horizon_conf: 0.92,
                tilt_intentional: false,
                aspect: ASPECT,
            },
            &input,
        );
        assert!((out.rotate_deg + 3.0).abs() < 1e-4);
    }

    #[test]
    fn the_inscribed_rectangle_shrinks_with_the_angle_and_is_centred() {
        let mut last = 1.0f32;
        for angle in [0.0f32, 1.0, 3.0, 5.0, 8.0] {
            let rect = inscribed(angle, ASPECT);
            let area = rect.w * rect.h;
            assert!(area <= last + 1e-6, "{angle} degrees grew the rectangle");
            last = area;
            assert!((rect.x * 2.0 + rect.w - 1.0).abs() < 1e-5, "not centred");
            assert!((rect.y * 2.0 + rect.h - 1.0).abs() < 1e-5, "not centred");
        }
        assert!((inscribed(0.0, ASPECT).w - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn the_inscribed_rectangle_keeps_the_frames_aspect_ratio() {
        for angle in [1.0f32, 4.0, 8.0] {
            for aspect in [1.5f32, 1.0, 0.6667] {
                let rect = inscribed(angle, aspect);
                let got = (rect.w * aspect) / rect.h;
                assert!(
                    (got - aspect).abs() < 1e-3,
                    "{angle} deg at {aspect}: got {got}"
                );
            }
        }
    }

    #[test]
    fn the_inscribed_rectangle_actually_fits_inside_the_rotated_frame() {
        // Every corner of the proposed rectangle, rotated back into the source, must land
        // inside the frame. This is the property the closed form is a shortcut for, and the
        // shortcut has two branches - so it is checked on both sides of the branch.
        for angle in [0.5f32, 2.0, 5.5, 8.0, -3.0] {
            for aspect in [1.5f32, 0.6667, 1.0] {
                let rect = inscribed(angle, aspect);
                let radians = -angle.to_radians();
                let (sin, cos) = radians.sin_cos();
                for corner in [
                    (rect.x, rect.y),
                    (rect.x + rect.w, rect.y),
                    (rect.x, rect.y + rect.h),
                    (rect.x + rect.w, rect.y + rect.h),
                ] {
                    let dx = (corner.0 - 0.5) * aspect;
                    let dy = corner.1 - 0.5;
                    let sx = 0.5 + (dx * cos - dy * sin) / aspect;
                    let sy = 0.5 + (dx * sin + dy * cos);
                    assert!(
                        (-1e-3..=1.0 + 1e-3).contains(&sx) && (-1e-3..=1.0 + 1e-3).contains(&sy),
                        "{angle} deg at {aspect}: corner {corner:?} -> ({sx}, {sy})"
                    );
                }
            }
        }
    }
}
