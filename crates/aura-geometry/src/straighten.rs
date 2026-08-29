//! Whether this frame is off level by an amount worth a resample, and how far it may actually go.
//!
//! Section 6.2: "Rotate only when Phase 11 horizon confidence >= 0.7 and the correction is
//! between 0.2 and 8 degrees; larger tilts are treated as intentional and left alone" and
//! "Rotation implies cropping; the crop is computed to stay inside the safety rules, and if it
//! cannot, the rotation is reduced or skipped."
//!
//! ## The crop is computed before the rotation is agreed to
//!
//! [`solve`] walks the angle down in steps and stops at the first one whose induced crop keeps
//! every protected region inside. There is no branch in which a frame is rotated and *then*
//! something is found to be missing, because by then the pixels are gone - a delivered frame with
//! somebody's hand outside it looks exactly like a frame that was shot that way.
//!
//! ## Nothing here fills a corner
//!
//! Rotating opens four triangles and section 2.2 puts content-aware fill in phase 24. The
//! triangles are removed by [`aura_core::contract::geometry::rotation_crop`], and when removing
//! them would breach a safety rule the rotation is **abandoned** rather than the corners
//! invented.
//!
//! ## Two thresholds for one measurement, and they are not the same threshold
//!
//! Phase 11 reports a tilt above `HORIZON_ACT_AT` of 0.60 and this phase acts on one above
//! [`aura_core::contract::geometry::ROTATE_ACT_AT`] of 0.70. Reporting costs a sentence in a
//! panel; acting costs a resample and a crop. A frame between the two is one where AURA says the
//! horizon looks off and declines to move it, which is [`GeometryCode::HorizonUnsure`] and is the
//! honest answer rather than a gap.

use aura_core::contract::composition::Box2;
use aura_core::contract::geometry::{
    rotation_crop, GeometryCode, ProtectedRegion, ROTATE_ACT_AT, ROTATE_MIN_DEG,
};

use crate::safety::{self, Limits};

/// How many reductions [`solve`] tries between the wanted angle and none.
///
/// Eight. The ladder is linear in the angle rather than a bisection, and that is the same choice
/// phase 15 made for its illuminant correction and for the same reason: the set of acceptable
/// angles is not an interval. A rotation that cuts a face at four degrees can be fine at three
/// and cut a *different* face at two, because the induced crop moves both edges at once - so a
/// bisection returns an arbitrary member of the set rather than the largest one.
pub const REDUCTION_STEPS: usize = 8;

/// How many scales below the maximum inscribed crop the solver will look for room to translate.
///
/// Four. [`rotation_crop`] returns the largest **centred** rectangle, which has no slack to move
/// in by construction; a slightly smaller one does, and that freedom is what lets a frame with a
/// face near one edge be levelled at all. Past about four steps the crop has given up more
/// resolution than the tilt was worth, and reducing the angle is the better trade.
pub const TRANSLATION_SCALES: usize = 4;

/// How many offsets are tried along each axis at each scale.
pub const TRANSLATION_STEPS: usize = 5;

/// What phase 11 measured about this frame's horizon.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Horizon {
    /// How far off level, in degrees, positive clockwise. The angle a correction would undo.
    pub tilt_deg: f32,
    /// How sure phase 11 is, `0..1`.
    pub confidence: f32,
    /// True when phase 11 says the tilt reads as a decision.
    pub intentional: bool,
    /// False when there was no horizon in the frame to measure against.
    pub present: bool,
}

impl Default for Horizon {
    /// No horizon at all, which is what a frame phase 11 has not judged gets.
    fn default() -> Self {
        Self {
            tilt_deg: 0.0,
            confidence: 0.0,
            intentional: false,
            present: false,
        }
    }
}

/// What the solver decided.
#[derive(Debug, Clone, PartialEq)]
pub struct Rotation {
    /// The angle to apply, in degrees. Zero when nothing is rotated.
    pub applied_deg: f32,
    /// The angle the horizon asked for, whether or not it was applied.
    ///
    /// Stored beside the applied angle so a refusal can say what it refused. A row carrying only
    /// the applied angle could not answer "why is this photograph still crooked".
    pub wanted_deg: f32,
    /// The rectangle the rotation leaves usable, in normalised frame coordinates.
    ///
    /// The whole frame when nothing was rotated. This is what the crop search is bounded by, so
    /// a straightened frame and an unrotated one go through exactly the same search afterwards.
    pub bounds: Box2,
    /// Why.
    pub code: GeometryCode,
}

impl Rotation {
    /// A frame that was not rotated, for the stated reason.
    #[must_use]
    pub const fn none(wanted_deg: f32, code: GeometryCode) -> Self {
        Self {
            applied_deg: 0.0,
            wanted_deg,
            bounds: Box2::FULL,
            code,
        }
    }

    /// True when this rotation moves the frame.
    #[must_use]
    pub fn is_applied(&self) -> bool {
        self.applied_deg.abs() >= f32::EPSILON
    }
}

/// Decide how far this frame may be levelled.
///
/// `max_deg` is the ceiling from `crop_rules.toml`, which a studio may lower and nobody may
/// raise.
#[must_use]
pub fn solve(
    horizon: Horizon,
    width: u32,
    height: u32,
    protected: &[ProtectedRegion],
    limits: Limits,
    max_deg: f32,
) -> Rotation {
    let wanted = if horizon.tilt_deg.is_finite() {
        horizon.tilt_deg
    } else {
        0.0
    };

    // The five gates, in the order that answers a photographer's question most directly. Every
    // one of them is a refusal, and between them they account for the great majority of a
    // wedding - which is the shape section 10.1 asks for rather than a shortfall.
    if !horizon.present {
        return Rotation::none(wanted, GeometryCode::HorizonAbsent);
    }
    if horizon.intentional {
        return Rotation::none(wanted, GeometryCode::TiltIntentional);
    }
    if wanted.abs() < ROTATE_MIN_DEG {
        return Rotation::none(wanted, GeometryCode::TiltNegligible);
    }
    if wanted.abs() > max_deg {
        return Rotation::none(wanted, GeometryCode::TiltTooLarge);
    }
    if horizon.confidence < ROTATE_ACT_AT {
        return Rotation::none(wanted, GeometryCode::HorizonUnsure);
    }

    let limits = limits.floored();
    for step in 0..=REDUCTION_STEPS {
        let factor = 1.0 - step as f32 / REDUCTION_STEPS as f32;
        let angle = wanted * factor;
        if angle.abs() < ROTATE_MIN_DEG {
            break;
        }
        if let Some(bounds) = usable(angle, width, height, protected, limits) {
            return Rotation {
                applied_deg: angle,
                wanted_deg: wanted,
                bounds,
                // Reduced rather than applied when the ladder had to step at all. The two codes
                // are different sentences in the panel and different rows in the histogram: one
                // says AURA levelled the photograph, the other says it levelled it part of the
                // way and why.
                code: if step == 0 {
                    GeometryCode::Straightened
                } else {
                    GeometryCode::RotationReduced
                },
            };
        }
    }
    Rotation::none(wanted, GeometryCode::RotationRefused)
}

/// The largest rectangle a rotation by `degrees` leaves usable, or `None` when nothing fits.
///
/// The centred maximum first, then progressively smaller rectangles translated to reach whatever
/// is near an edge. [`rotation_crop`] returns the largest *centred* rectangle, which is tight and
/// therefore has nowhere to move; a smaller one has slack, and using it is what lets a frame with
/// a face close to one side be levelled at all rather than refused outright.
#[must_use]
pub fn usable(
    degrees: f32,
    width: u32,
    height: u32,
    protected: &[ProtectedRegion],
    limits: Limits,
) -> Option<Box2> {
    let max = rotation_crop(width, height, degrees);
    if max.w <= 1e-4 || max.h <= 1e-4 {
        return None;
    }
    let fits = |rect: Box2| -> bool {
        if !inside_the_rotated_frame(rect, degrees, limits.frame_aspect) {
            return false;
        }
        protected.iter().all(|region| {
            safety::rect_inside(
                project(region.area, rect, degrees, limits.frame_aspect),
                rect,
                limits.margin,
            )
        })
    };
    if fits(max) {
        return Some(max);
    }
    // The hull is what has to be reached. Nothing to reach means the centred crop was refused by
    // the frame itself, which cannot happen for a rectangle `rotation_crop` returned - so the
    // early return here is a guard rather than a path.
    safety::hull(protected, limits.margin)?;

    for scale_step in 1..=TRANSLATION_SCALES {
        let scale = 1.0 - scale_step as f32 * 0.03;
        let w = max.w * scale;
        let h = max.h * scale;
        if w <= 1e-4 || h <= 1e-4 {
            break;
        }
        for oy in 0..TRANSLATION_STEPS {
            for ox in 0..TRANSLATION_STEPS {
                let t = |n: usize, extent: f32| {
                    let span = (max.w.min(max.h)) * (1.0 - scale);
                    let u = n as f32 / (TRANSLATION_STEPS - 1).max(1) as f32 - 0.5;
                    let _ = extent;
                    u * span * 2.0
                };
                let rect = Box2 {
                    x: max.x + (max.w - w) / 2.0 + t(ox, max.w),
                    y: max.y + (max.h - h) / 2.0 + t(oy, max.h),
                    w,
                    h,
                }
                .clamped();
                if rect.w <= 1e-4 || rect.h <= 1e-4 {
                    continue;
                }
                if fits(rect) {
                    return Some(rect);
                }
            }
        }
    }
    None
}

/// Where a region in the frame's own coordinates lands relative to a rotated crop.
///
/// The renderer takes the crop rectangle out of the source and rotates about **its** centre -
/// `spatial::crop_rotate` maps an output offset `(dx, dy)` to a source offset
/// `(dx cos a - dy sin a, dx sin a + dy cos a)`. So a source point's place in the delivered frame
/// is that map run backwards, and this is that inverse, returning the axis-aligned bounding box
/// of the region's four rotated corners.
///
/// **The rotation happens in pixel units, not in normalised ones.** A normalised frame is not
/// square, so rotating a normalised coordinate by fifteen degrees rotates it by fifteen degrees
/// on a square frame and by something else on every other one - which would put a face inside the
/// arithmetic and outside the photograph.
#[must_use]
pub fn project(area: Box2, rect: Box2, degrees: f32, frame_aspect: f32) -> Box2 {
    let aspect = if frame_aspect.is_finite() && frame_aspect > 0.0 {
        frame_aspect
    } else {
        1.5
    };
    let angle = -degrees.to_radians();
    // Backwards: the renderer applies `R(angle)`, so a source offset is `R(-angle)` of an output
    // one.
    let (sin, cos) = (-angle).sin_cos();
    let cx = rect.x + rect.w / 2.0;
    let cy = rect.y + rect.h / 2.0;

    let mut min = (f32::MAX, f32::MAX);
    let mut max = (f32::MIN, f32::MIN);
    for (px, py) in [
        (area.x, area.y),
        (area.x + area.w, area.y),
        (area.x, area.y + area.h),
        (area.x + area.w, area.y + area.h),
    ] {
        let dx = (px - cx) * aspect;
        let dy = py - cy;
        let ox = dx * cos - dy * sin;
        let oy = dx * sin + dy * cos;
        let x = cx + ox / aspect;
        let y = cy + oy;
        min = (min.0.min(x), min.1.min(y));
        max = (max.0.max(x), max.1.max(y));
    }
    Box2 {
        x: min.0,
        y: min.1,
        w: max.0 - min.0,
        h: max.1 - min.1,
    }
}

/// True when every corner of a rotated crop still reads inside the frame.
///
/// The forward map rather than the inverse: these are the four points the renderer will sample,
/// and a corner outside `0..1` is a black triangle in the delivered photograph. Nothing in this
/// phase fills one.
#[must_use]
pub fn inside_the_rotated_frame(rect: Box2, degrees: f32, frame_aspect: f32) -> bool {
    let aspect = if frame_aspect.is_finite() && frame_aspect > 0.0 {
        frame_aspect
    } else {
        1.5
    };
    let angle = -degrees.to_radians();
    let (sin, cos) = angle.sin_cos();
    let cx = rect.x + rect.w / 2.0;
    let cy = rect.y + rect.h / 2.0;
    for (px, py) in [
        (rect.x, rect.y),
        (rect.x + rect.w, rect.y),
        (rect.x, rect.y + rect.h),
        (rect.x + rect.w, rect.y + rect.h),
    ] {
        let dx = (px - cx) * aspect;
        let dy = py - cy;
        let sx = cx + (dx * cos - dy * sin) / aspect;
        let sy = cy + (dx * sin + dy * cos);
        if !(-1e-4..=1.0 + 1e-4).contains(&sx) || !(-1e-4..=1.0 + 1e-4).contains(&sy) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::geometry::{ProtectedContent, ROTATE_MAX_DEG};

    fn face(x: f32, y: f32) -> ProtectedRegion {
        ProtectedRegion::anonymous(
            ProtectedContent::Face,
            Box2 {
                x,
                y,
                w: 0.06,
                h: 0.08,
            },
        )
    }

    fn limits() -> Limits {
        Limits {
            frame_aspect: 1.5,
            ..Limits::default()
        }
    }

    #[test]
    fn each_gate_produces_its_own_code() {
        let base = Horizon {
            tilt_deg: 2.0,
            confidence: 0.9,
            intentional: false,
            present: true,
        };
        let cases = [
            (
                Horizon {
                    present: false,
                    ..base
                },
                GeometryCode::HorizonAbsent,
            ),
            (
                Horizon {
                    intentional: true,
                    ..base
                },
                GeometryCode::TiltIntentional,
            ),
            (
                Horizon {
                    tilt_deg: 0.05,
                    ..base
                },
                GeometryCode::TiltNegligible,
            ),
            (
                Horizon {
                    tilt_deg: 20.0,
                    ..base
                },
                GeometryCode::TiltTooLarge,
            ),
            (
                Horizon {
                    confidence: 0.65,
                    ..base
                },
                GeometryCode::HorizonUnsure,
            ),
        ];
        for (horizon, code) in cases {
            let out = solve(horizon, 6000, 4000, &[], limits(), ROTATE_MAX_DEG);
            assert_eq!(out.code, code, "{horizon:?}");
            assert!(!out.is_applied());
            // The wanted angle survives every refusal, which is what lets the panel say what was
            // declined rather than only that something was.
            assert!((out.wanted_deg - horizon.tilt_deg).abs() < 1e-6);
        }
    }

    #[test]
    fn a_clean_frame_is_levelled_all_the_way() {
        let out = solve(
            Horizon {
                tilt_deg: 3.0,
                confidence: 0.85,
                intentional: false,
                present: true,
            },
            6000,
            4000,
            &[],
            limits(),
            ROTATE_MAX_DEG,
        );
        assert_eq!(out.code, GeometryCode::Straightened);
        assert!((out.applied_deg - 3.0).abs() < 1e-6);
        // And it cost a crop, which is the whole reason the ladder exists.
        assert!(out.bounds.w < 1.0 && out.bounds.h < 1.0);
    }

    #[test]
    fn a_face_in_the_corner_reduces_the_rotation_rather_than_cutting_it() {
        // A face right in the top-left corner, where the rotation crop bites hardest.
        let protected = [face(0.02, 0.03)];
        let out = solve(
            Horizon {
                tilt_deg: 7.0,
                confidence: 0.9,
                intentional: false,
                present: true,
            },
            6000,
            4000,
            &protected,
            limits(),
            ROTATE_MAX_DEG,
        );
        assert!(
            matches!(
                out.code,
                GeometryCode::RotationReduced | GeometryCode::RotationRefused
            ),
            "{:?}",
            out.code
        );
        if out.is_applied() {
            assert!(out.applied_deg.abs() < 7.0);
            for region in &protected {
                let projected = project(region.area, out.bounds, out.applied_deg, 1.5);
                assert!(
                    safety::rect_inside(projected, out.bounds, limits().margin),
                    "the reduced rotation still cuts the face"
                );
            }
        }
    }

    #[test]
    fn a_face_dead_centre_never_costs_the_rotation_anything() {
        let protected = [face(0.47, 0.46)];
        let out = solve(
            Horizon {
                tilt_deg: 4.0,
                confidence: 0.9,
                intentional: false,
                present: true,
            },
            6000,
            4000,
            &protected,
            limits(),
            ROTATE_MAX_DEG,
        );
        assert_eq!(out.code, GeometryCode::Straightened);
        assert!((out.applied_deg - 4.0).abs() < 1e-6);
    }

    #[test]
    fn a_studio_may_lower_the_ceiling_and_the_solver_obeys_it() {
        let horizon = Horizon {
            tilt_deg: 5.0,
            confidence: 0.9,
            intentional: false,
            present: true,
        };
        assert_eq!(
            solve(horizon, 6000, 4000, &[], limits(), 3.0).code,
            GeometryCode::TiltTooLarge
        );
        assert_eq!(
            solve(horizon, 6000, 4000, &[], limits(), 8.0).code,
            GeometryCode::Straightened
        );
    }

    #[test]
    fn the_projection_is_the_inverse_of_what_the_renderer_does() {
        // A point mapped into the delivered frame and back must land where it started. If this
        // drifts, the safety filter is clearing rectangles against a photograph the renderer is
        // not going to produce.
        let rect = Box2 {
            x: 0.1,
            y: 0.1,
            w: 0.8,
            h: 0.8,
        };
        let point = Box2 {
            x: 0.30,
            y: 0.62,
            w: 0.0,
            h: 0.0,
        };
        let there = project(point, rect, 6.0, 1.5);
        let back = project(there, rect, -6.0, 1.5);
        assert!((back.x - point.x).abs() < 1e-4, "{} != {}", back.x, point.x);
        assert!((back.y - point.y).abs() < 1e-4, "{} != {}", back.y, point.y);
    }

    #[test]
    fn a_zero_rotation_projects_a_region_onto_itself() {
        let area = Box2 {
            x: 0.2,
            y: 0.3,
            w: 0.1,
            h: 0.1,
        };
        let projected = project(area, Box2::FULL, 0.0, 1.5);
        assert!((projected.x - area.x).abs() < 1e-6);
        assert!((projected.y - area.y).abs() < 1e-6);
        assert!((projected.w - area.w).abs() < 1e-6);
    }

    #[test]
    fn the_rotated_crop_that_the_contract_returns_is_inside_the_frame() {
        // The contract's own guarantee, checked here against the forward map this module uses -
        // two implementations of the same idea agreeing is what stops a plan reporting one crop
        // and a render producing another.
        for degrees in [0.5f32, 1.0, 3.0, 5.0, 8.0] {
            for (w, h) in [(6000u32, 4000u32), (4000, 6000), (5000, 5000)] {
                let rect = rotation_crop(w, h, degrees);
                assert!(
                    inside_the_rotated_frame(rect, degrees, w as f32 / h as f32),
                    "{degrees} deg on {w}x{h} left the frame"
                );
            }
        }
    }
}
