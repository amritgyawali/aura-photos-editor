//! The veto. Which rectangles may not be delivered, and why.
//!
//! Section 6.3's hard constraints: "every detected face fully inside, primary identities' hands
//! and joined hands inside, resolution >= 60 % of the original long edge, and the moment's key
//! content preserved."
//!
//! ## This runs before the score, and there is no arithmetic path around it
//!
//! [`check`] is called on every candidate rectangle *before* [`crate::crop::objective`] is
//! compared to anything, and an unsafe candidate is dropped rather than penalised. That is
//! deliberate and it is phase 20's protected-feature rule in a different domain: a penalty is a
//! number that a large enough improvement somewhere else can outweigh, and there is no
//! composition so good that it is worth a hand out of frame.
//!
//! ## The margin is what makes "fully inside" mean fully inside
//!
//! A face whose boundary sits exactly on the crop's boundary is a face with the resampler's own
//! filter kernel hanging off the end of the photograph, which at export is a visibly soft or
//! clipped rim on somebody's ear. [`aura_core::contract::geometry::SAFETY_MARGIN`] is the floor
//! and `crop_rules.toml` may raise it; nothing may lower it.
//!
//! ## Why a report rather than a boolean
//!
//! `CropSafetyReport::considered` is the denominator, and on this build it is usually **zero**:
//! phase 06's detector is a placeholder, so the safety filter over a real photograph has nothing
//! to protect. A report that said only `faces_intact: true` would be indistinguishable between a
//! frame with six faces all inside and a frame nobody could find a face in, and section 10.1's
//! hard gate - "zero auto-crops cut a detected face" - is arithmetic rather than evidence over
//! the second. Phase 08's rule: say what the denominator is.

use aura_core::contract::composition::Box2;
use aura_core::contract::geometry::{
    CropSafetyReport, CropVariant, GeometryCode, ProtectedContent, ProtectedRegion,
    MIN_LONG_EDGE_FRACTION, SAFETY_MARGIN,
};

/// What the filter is checking against.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// The frame's width divided by its height, in pixels.
    pub frame_aspect: f32,
    /// How far inside the crop's edge a protected region must sit. At or above [`SAFETY_MARGIN`].
    pub margin: f32,
    /// The smallest share of the original long edge a crop may keep.
    pub min_long_edge: f32,
}

impl Default for Limits {
    /// A 3:2 frame at the contract's own floors.
    fn default() -> Self {
        Self {
            frame_aspect: 1.5,
            margin: SAFETY_MARGIN,
            min_long_edge: MIN_LONG_EDGE_FRACTION,
        }
    }
}

impl Limits {
    /// The floors applied, so a caller cannot pass a margin below the contract's.
    ///
    /// Clamped rather than refused, because this is the last line rather than the first: the
    /// config loader already refuses a file that lowers either, and a caller that constructed
    /// `Limits` by hand should not be able to route around that by being the second caller.
    #[must_use]
    pub fn floored(mut self) -> Self {
        self.margin = self.margin.max(SAFETY_MARGIN);
        self.min_long_edge = self.min_long_edge.max(MIN_LONG_EDGE_FRACTION);
        if !self.frame_aspect.is_finite() || self.frame_aspect <= 0.0 {
            self.frame_aspect = 1.5;
        }
        self
    }
}

/// What the filter found about one rectangle.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    /// The report, ready to store.
    pub report: CropSafetyReport,
    /// Every rule this rectangle broke, worst first. Empty when it broke none.
    pub codes: Vec<GeometryCode>,
}

impl Outcome {
    /// True when every hard constraint held.
    #[must_use]
    pub fn is_safe(&self) -> bool {
        self.codes.is_empty()
    }
}

/// True when `region` sits inside `rect` with `margin` to spare.
///
/// [`ProtectedRegion::inside`] is the same predicate at the contract's own margin. This one
/// takes the margin as an argument because `crop_rules.toml` may raise it, and a studio that
/// asked for more room around a face and got the contract's minimum would have a setting that
/// silently did nothing.
#[must_use]
pub fn inside(region: &ProtectedRegion, rect: Box2, margin: f32) -> bool {
    rect_inside(region.area, rect, margin)
}

/// True when `area` sits inside `rect` with `margin` to spare.
///
/// The same predicate over a bare rectangle, for the one caller that has projected a region
/// through a rotation and holds the result rather than the region:
/// [`crate::straighten::usable`]. Two functions rather than one taking a rectangle, because
/// every *other* caller has a region and should keep carrying its kind - a filter that had
/// only rectangles could not say which rule a refusal broke.
#[must_use]
pub fn rect_inside(area: Box2, rect: Box2, margin: f32) -> bool {
    let margin = margin.max(SAFETY_MARGIN);
    area.x >= rect.x + margin - 1e-6
        && area.y >= rect.y + margin - 1e-6
        && area.x + area.w <= rect.x + rect.w - margin + 1e-6
        && area.y + area.h <= rect.y + rect.h - margin + 1e-6
}

/// Check one rectangle against every protected region and the resolution floor.
///
/// The rectangle is in normalised frame coordinates **after** any rotation and keystone, and the
/// regions are in frame coordinates **before** them - which is why the caller maps the regions
/// rather than the rectangle. See [`crate::straighten::project`].
#[must_use]
pub fn check(rect: Box2, regions: &[ProtectedRegion], limits: Limits) -> Outcome {
    let limits = limits.floored();
    let variant = CropVariant {
        aspect: aura_core::contract::geometry::AspectRatio::Original,
        rect,
        purpose: aura_core::contract::geometry::CropPurpose::Primary,
        score: 0.0,
        safe: true,
    };
    let long_edge_fraction = variant.long_edge_fraction(limits.frame_aspect);

    let mut faces_intact = true;
    let mut content_kept = true;
    let mut at_risk = 0u32;
    let mut cuts_face = false;
    let mut cuts_hands = false;
    let mut drops_key = false;

    for region in regions {
        if inside(region, rect, limits.margin) {
            continue;
        }
        at_risk = at_risk.saturating_add(1);
        match region.kind {
            ProtectedContent::PrimaryFace | ProtectedContent::Face => {
                faces_intact = false;
                cuts_face = true;
            }
            ProtectedContent::Hands | ProtectedContent::JoinedHands => {
                content_kept = false;
                cuts_hands = true;
            }
            ProtectedContent::MomentKey => {
                content_kept = false;
                drops_key = true;
            }
        }
    }

    let resolution_ok = long_edge_fraction >= limits.min_long_edge - 1e-6;

    // Worst first, and the order is `ProtectedContent::ALL`'s: a cut face is unmissable and a
    // cut hand is easy to miss, which is why the hands code ranks where it does rather than
    // below the resolution one.
    let mut codes = Vec::new();
    if cuts_face {
        codes.push(GeometryCode::CropCutsFace);
    }
    if cuts_hands {
        codes.push(GeometryCode::CropCutsHands);
    }
    if drops_key {
        codes.push(GeometryCode::CropDropsMomentKey);
    }
    if !resolution_ok {
        codes.push(GeometryCode::CropBelowResolution);
    }

    Outcome {
        report: CropSafetyReport {
            faces_intact,
            resolution_ok,
            content_kept,
            considered: u32::try_from(regions.len()).unwrap_or(u32::MAX),
            at_risk,
            long_edge_fraction,
            regions: worst_first(regions),
        },
        codes,
    }
}

/// The regions a report carries, worst kind first and capped.
///
/// The counts in the report are **not** capped - a frame with sixty faces has sixty regions
/// checked and sixty counted, and what is bounded is how many are stored and rendered. A panel
/// listing sixty rectangles is a panel nobody reads; a count that stopped at sixteen is a
/// denominator that lies.
#[must_use]
pub fn worst_first(regions: &[ProtectedRegion]) -> Vec<ProtectedRegion> {
    let mut out = regions.to_vec();
    out.sort_by(|left, right| {
        left.kind.cmp(&right.kind).then_with(|| {
            // Largest first inside a kind, so the sixteen that survive the cap are the sixteen a
            // photographer would notice. `total_cmp` rather than `partial_cmp` because a
            // degenerate region must not silently reorder the list.
            (right.area.w * right.area.h).total_cmp(&(left.area.w * left.area.h))
        })
    });
    out.truncate(CropSafetyReport::MAX_REGIONS);
    out
}

/// The tightest rectangle that still contains every protected region, with the margin.
///
/// What the crop search is bounded by, and what [`crate::straighten`] compares a rotation's
/// induced crop against. `None` when there is nothing to protect, which is not the same as the
/// whole frame: a caller that received `Box2::FULL` here could not tell "everything must stay"
/// from "nothing had to".
#[must_use]
pub fn hull(regions: &[ProtectedRegion], margin: f32) -> Option<Box2> {
    let margin = margin.max(SAFETY_MARGIN);
    let mut bounds: Option<(f32, f32, f32, f32)> = None;
    for region in regions {
        let (x0, y0) = (region.area.x - margin, region.area.y - margin);
        let (x1, y1) = (
            region.area.x + region.area.w + margin,
            region.area.y + region.area.h + margin,
        );
        bounds = Some(match bounds {
            None => (x0, y0, x1, y1),
            Some((ax0, ay0, ax1, ay1)) => (ax0.min(x0), ay0.min(y0), ax1.max(x1), ay1.max(y1)),
        });
    }
    bounds.map(|(x0, y0, x1, y1)| {
        Box2 {
            x: x0,
            y: y0,
            w: x1 - x0,
            h: y1 - y0,
        }
        .clamped()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::geometry::AspectRatio;

    fn region(kind: ProtectedContent, x: f32, y: f32, w: f32, h: f32) -> ProtectedRegion {
        ProtectedRegion::anonymous(kind, Box2 { x, y, w, h })
    }

    #[test]
    fn nothing_protected_is_safe_and_says_the_denominator_was_zero() {
        let outcome = check(Box2::FULL, &[], Limits::default());
        assert!(outcome.is_safe());
        assert_eq!(outcome.report.considered, 0);
        assert!(outcome.report.is_safe());
    }

    #[test]
    fn a_face_at_the_crops_edge_is_cut_even_though_the_arithmetic_contains_it() {
        // The whole reason the margin exists: the face is inside the rectangle by every test a
        // naive containment check would apply, and the resampler still reads off the end of it.
        let face = region(ProtectedContent::Face, 0.20, 0.40, 0.10, 0.10);
        // Wide enough to clear the resolution floor on its own, so this test measures the margin
        // and only the margin. A rectangle that broke two rules would pass while proving one.
        let exactly = Box2 {
            x: 0.20,
            y: 0.20,
            w: 0.75,
            h: 0.75,
        };
        let outcome = check(exactly, &[face.clone()], Limits::default());
        assert_eq!(outcome.codes, vec![GeometryCode::CropCutsFace]);
        assert!(!outcome.report.faces_intact);
        assert_eq!(outcome.report.at_risk, 1);

        // One per cent further out and the same face is safe.
        let clear = Box2 {
            x: 0.18,
            y: 0.18,
            w: 0.79,
            h: 0.79,
        };
        assert!(check(clear, &[face], Limits::default()).is_safe());
    }

    #[test]
    fn each_kind_of_content_raises_its_own_code_and_the_right_flag() {
        let limits = Limits::default();
        let outside = Box2 {
            x: 0.90,
            y: 0.90,
            w: 0.08,
            h: 0.08,
        };
        let rect = Box2 {
            x: 0.0,
            y: 0.0,
            w: 0.6,
            h: 0.6,
        };
        for (kind, code, faces, content) in [
            (
                ProtectedContent::PrimaryFace,
                GeometryCode::CropCutsFace,
                false,
                true,
            ),
            (ProtectedContent::Face, GeometryCode::CropCutsFace, false, true),
            (
                ProtectedContent::Hands,
                GeometryCode::CropCutsHands,
                true,
                false,
            ),
            (
                ProtectedContent::JoinedHands,
                GeometryCode::CropCutsHands,
                true,
                false,
            ),
            (
                ProtectedContent::MomentKey,
                GeometryCode::CropDropsMomentKey,
                true,
                false,
            ),
        ] {
            let outcome = check(
                rect,
                &[ProtectedRegion::anonymous(kind, outside)],
                limits,
            );
            assert!(outcome.codes.contains(&code), "{kind:?}");
            assert_eq!(outcome.report.faces_intact, faces, "{kind:?}");
            assert_eq!(outcome.report.content_kept, content, "{kind:?}");
        }
    }

    #[test]
    fn the_resolution_floor_is_on_the_long_edge_and_a_square_variant_survives_it() {
        // The claim `MIN_LONG_EDGE_FRACTION` makes in the contract, as a test. A 1:1 crop of a
        // 3:2 frame keeps two thirds of the long edge and 44 % of the area; an area floor would
        // refuse every square in the wedding.
        let limits = Limits {
            frame_aspect: 1.5,
            ..Limits::default()
        };
        let square = Box2 {
            x: 1.0 / 6.0,
            y: 0.0,
            w: 2.0 / 3.0,
            h: 1.0,
        };
        let outcome = check(square, &[], limits);
        assert!(outcome.report.resolution_ok, "{:?}", outcome.report);
        assert!((outcome.report.long_edge_fraction - 2.0 / 3.0).abs() < 1e-3);

        let too_tight = Box2 {
            x: 0.3,
            y: 0.3,
            w: 0.4,
            h: 0.4,
        };
        assert!(check(too_tight, &[], limits)
            .codes
            .contains(&GeometryCode::CropBelowResolution));
    }

    #[test]
    fn a_report_caps_what_it_stores_and_never_caps_what_it_counts() {
        let many: Vec<ProtectedRegion> = (0..60)
            .map(|i| {
                let t = i as f32 / 60.0;
                region(ProtectedContent::Face, t * 0.9, 0.4, 0.02, 0.02)
            })
            .collect();
        let outcome = check(Box2::FULL, &many, Limits::default());
        assert_eq!(outcome.report.considered, 60);
        assert_eq!(outcome.report.regions.len(), CropSafetyReport::MAX_REGIONS);
    }

    #[test]
    fn the_hull_of_nothing_is_nothing_rather_than_the_whole_frame() {
        assert!(hull(&[], SAFETY_MARGIN).is_none());
        let hull = hull(
            &[
                region(ProtectedContent::Face, 0.2, 0.2, 0.1, 0.1),
                region(ProtectedContent::Face, 0.6, 0.5, 0.1, 0.1),
            ],
            0.01,
        )
        .expect("two regions have a hull");
        assert!(hull.x <= 0.19 && hull.y <= 0.19);
        assert!(hull.x + hull.w >= 0.70 && hull.y + hull.h >= 0.60);
    }

    #[test]
    fn a_caller_cannot_pass_a_margin_below_the_contracts_own() {
        let face = region(ProtectedContent::Face, 0.10, 0.10, 0.10, 0.10);
        let touching = Box2 {
            x: 0.10,
            y: 0.10,
            w: 0.80,
            h: 0.80,
        };
        let outcome = check(
            touching,
            &[face],
            Limits {
                margin: 0.0,
                ..Limits::default()
            },
        );
        assert!(!outcome.is_safe(), "a zero margin was accepted");
    }

    #[test]
    fn a_variants_long_edge_fraction_matches_what_the_filter_measures() {
        // The filter and the contract must agree about how much of a frame a rectangle keeps,
        // because one of them decides and the other is stored.
        let limits = Limits {
            frame_aspect: 2.0,
            ..Limits::default()
        };
        let rect = Box2 {
            x: 0.1,
            y: 0.0,
            w: 0.8,
            h: 1.0,
        };
        let variant = CropVariant {
            aspect: AspectRatio::Original,
            rect,
            purpose: aura_core::contract::geometry::CropPurpose::Primary,
            score: 0.0,
            safe: true,
        };
        let outcome = check(rect, &[], limits);
        assert!(
            (outcome.report.long_edge_fraction - variant.long_edge_fraction(2.0)).abs() < 1e-6
        );
    }
}
