//! Whether the upright lines in this frame are architecture, and how far they may be pulled.
//!
//! Section 6.2: "Keystone correction is limited to frames with strong architectural verticals and
//! capped so no axis is stretched beyond a documented factor."
//!
//! ## The measurement, and why it is a measurement
//!
//! Phase 11 sets `CompositionFlags::VERTICALS_CONVERGING` from the spread of the vertical family,
//! and a flag is a yes or a no. A correction needs a *magnitude*, so this module measures one:
//! the near-vertical edge energy in the top third of the frame against the same in the bottom
//! third, weighted by how far each edge sits from the centre. A camera pointed up at a building
//! narrows the top; a camera pointed down widens it; a camera held level does neither.
//!
//! ## The people test is the whole safety of this operator
//!
//! A wedding is mostly people, and the vertical family in a photograph of six guests is six
//! guests rather than a building. Correcting *that* leans everybody outward, which looks like a
//! lens fault nobody can name. So every protected region is **subtracted from the vertical
//! family** before the share is computed, and
//! [`aura_core::contract::geometry::KEYSTONE_MIN_VERTICAL_SHARE`] is what is left having to be
//! architecture. It is the same idea as phase 21's flyaway detector refusing a strand over a
//! detailed background: a measurement cannot tell a pillar from a person, so it is given only the
//! pixels where a person is not.
//!
//! ## The cap is measured on the warp, not on the slider
//!
//! [`aura_render::geometry::stretch_of`] is the one implementation, shared with the renderer that
//! applies it. A cap on the slider would be a cap on two different things: the same slider value
//! on a 16:9 frame and a 4:5 frame stretches by different amounts, and the defect a photographer
//! sees does not care which of the two produced it.

use aura_core::contract::composition::Box2;
use aura_core::contract::geometry::{
    GeometryCode, Keystone, ProtectedRegion, KEYSTONE_ACT_AT, KEYSTONE_MIN_VERTICAL_SHARE,
    MAX_STRETCH,
};
use aura_render::geometry::{stretch_of, KEYSTONE_MAX_P};

use crate::safety::{self, Limits};

/// How near vertical an edge must be to join the vertical family.
///
/// A gradient is perpendicular to the edge it sits on, so a *vertical* edge has a gradient that
/// is almost entirely horizontal. Four to one is about fourteen degrees off vertical, which is
/// wide enough to catch the converging sides of a doorway and narrow enough to exclude the
/// diagonal of a staircase - and a staircase corrected as though it were upright is a photograph
/// with a leaning building in it.
pub const VERTICAL_RATIO: f32 = 4.0;

/// The share of the frame's height each measurement band covers.
///
/// A third at the top and a third at the bottom, with the middle third unused. The middle is
/// where a convergence is smallest and where the people are, so including it would dilute the
/// signal with the pixels least able to carry it.
pub const BAND: f32 = 1.0 / 3.0;

/// How much of the measured convergence a full correction removes.
///
/// The mapping from a measured convergence in `0..1` onto the recipe's `-100..100` slider. Set so
/// that a convergence at [`KEYSTONE_ACT_AT`] asks for a warp near the low end of the usable range
/// and a convergence of one asks for a warp the cap will trim - which is the correct shape: the
/// cap should bind on the frames that need the most correction rather than on none of them.
pub const CORRECTION_GAIN: f32 = 110.0;

/// What the vertical family in this frame looks like.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Verticals {
    /// How much the family converges, `0..1`. Positive when the top is narrower.
    ///
    /// Signed, because a camera pointed *down* at a table diverges rather than converging and the
    /// correction runs the other way. An unsigned measurement would correct both in the same
    /// direction and make one of the two worse.
    pub convergence: f32,
    /// The share of the frame's gradient energy that is near-vertical and not on a person, `0..1`.
    pub share: f32,
}

/// Measure the vertical family, ignoring everything inside a protected region.
///
/// `gx` and `gy` are `aura_render::spatial::sobel_planes` over the proxy's luminance. The two
/// components rather than the magnitude, because a magnitude has no orientation in it and this
/// module's entire job is to tell upright lines from the rest.
#[must_use]
pub fn measure(
    gx: &[f32],
    gy: &[f32],
    width: usize,
    height: usize,
    protected: &[ProtectedRegion],
) -> Verticals {
    if width < 8 || height < 8 {
        return Verticals::default();
    }
    let top_end = ((height as f32) * BAND) as usize;
    let bottom_start = height - top_end.max(1);

    let mut total = 0.0f64;
    let mut vertical = 0.0f64;
    // Energy-weighted mean distance from the frame's centre line, per band.
    let mut top = (0.0f64, 0.0f64);
    let mut bottom = (0.0f64, 0.0f64);

    for y in 0..height {
        let v = (y as f32 + 0.5) / height as f32;
        for x in 0..width {
            let (Some(&ex), Some(&ey)) = (gx.get(y * width + x), gy.get(y * width + x)) else {
                continue;
            };
            let magnitude = f64::from(ex.hypot(ey));
            if magnitude <= 0.0 {
                continue;
            }
            total += magnitude;
            if ex.abs() < ey.abs() * VERTICAL_RATIO {
                continue;
            }
            let u = (x as f32 + 0.5) / width as f32;
            if protected.iter().any(|region| contains(region.area, u, v)) {
                // A person, and this operator has no business measuring one. Counted in `total`
                // and not in `vertical`, so a frame full of people has a *low share* rather than
                // a high one - which is what stops the correction running at all.
                continue;
            }
            vertical += magnitude;
            let spread = f64::from((u - 0.5).abs());
            if y < top_end {
                top = (top.0 + magnitude * spread, top.1 + magnitude);
            } else if y >= bottom_start {
                bottom = (bottom.0 + magnitude * spread, bottom.1 + magnitude);
            }
        }
    }

    if total <= 0.0 || top.1 <= 0.0 || bottom.1 <= 0.0 {
        return Verticals::default();
    }
    let top_spread = top.0 / top.1;
    let bottom_spread = bottom.0 / bottom.1;
    let widest = top_spread.max(bottom_spread);
    // The two bands are thirds, so their energy centroids sit at a sixth and five sixths of the
    // frame - two thirds apart rather than one whole frame apart. Dividing by that separation is
    // what turns a measurement *between the bands* into the convergence *across the frame*, which
    // is what `KEYSTONE_ACT_AT` is written about.
    //
    // Without it the measurement under-reports by a third, and the symptom is a threshold a
    // correct implementation cannot reach on a real building - phase 22's lesson, which is that a
    // threshold is a statement about the instrument as well as about the world.
    let separation = (1.0 - f64::from(BAND)).max(1e-3);
    let convergence = if widest <= 1e-9 {
        0.0
    } else {
        (((bottom_spread - top_spread) / widest) / separation) as f32
    };
    Verticals {
        convergence: convergence.clamp(-1.0, 1.0),
        share: (vertical / total) as f32,
    }
}

/// What the solver decided about a perspective correction.
#[derive(Debug, Clone, PartialEq)]
pub struct Correction {
    /// The correction, or `None` when none is made.
    pub keystone: Option<Keystone>,
    /// The rectangle the correction leaves usable, in normalised frame coordinates.
    ///
    /// A keystone magnifies to hide the corners it opens, and the magnification is exactly
    /// [`aura_render::geometry::stretch_of`] - so the usable rectangle is a centred box of that
    /// reciprocal on each axis. The crop search is bounded by it, which is what makes "the crop is
    /// computed before the correction is agreed to" true here as well as for a rotation.
    pub bounds: Box2,
    /// Why.
    pub code: GeometryCode,
}

impl Correction {
    /// No correction, for the stated reason.
    #[must_use]
    pub const fn none(code: GeometryCode) -> Self {
        Self {
            keystone: None,
            bounds: Box2::FULL,
            code,
        }
    }
}

/// Decide whether to correct converging verticals, and how far.
///
/// `frame_aspect` is the frame's width over its height in pixels, and it is in here because the
/// stretch a slider value produces depends on it.
#[must_use]
pub fn solve(
    verticals: Verticals,
    frame_aspect: f32,
    protected: &[ProtectedRegion],
    limits: Limits,
) -> Correction {
    if verticals.share < KEYSTONE_MIN_VERTICAL_SHARE {
        return Correction::none(GeometryCode::KeystoneNoArchitecture);
    }
    if verticals.convergence.abs() < KEYSTONE_ACT_AT {
        return Correction::none(GeometryCode::KeystoneNotNeeded);
    }

    let wanted = (verticals.convergence * CORRECTION_GAIN).clamp(-100.0, 100.0);
    let (vertical, capped) = cap(wanted, frame_aspect);
    if vertical.abs() < 1e-3 {
        return Correction::none(GeometryCode::KeystoneStretchCapped);
    }
    let stretch = stretch_of(vertical, 0.0, frame_aspect);
    if !stretch.is_finite() {
        return Correction::none(GeometryCode::KeystoneStretchCapped);
    }

    let keystone = Keystone {
        vertical,
        horizontal: 0.0,
        stretch,
        convergence: verticals.convergence.abs(),
    }
    .clamped();
    if !keystone.within_cap() {
        // Belt and braces: `cap` already solved for a slider inside the ceiling, and the contract
        // gets to refuse the answer anyway. A guarantee enforced in one layer lasts until
        // somebody writes a second caller.
        return Correction::none(GeometryCode::KeystoneStretchCapped);
    }

    let limits = limits.floored();
    let bounds = usable(stretch);
    let safe = protected
        .iter()
        .all(|region| safety::inside(region, bounds, limits.margin));
    if !safe {
        return Correction::none(GeometryCode::KeystoneRefused);
    }

    Correction {
        keystone: Some(keystone),
        bounds,
        code: if capped {
            GeometryCode::KeystoneStretchCapped
        } else {
            GeometryCode::KeystoneApplied
        },
    }
}

/// The largest slider value inside [`MAX_STRETCH`], and whether the cap bound.
///
/// A closed form rather than a search: the stretch is `1 / (1 - |p|)` and `p` is linear in the
/// slider, so the slider at the cap is the cap's own `p` scaled back through
/// [`KEYSTONE_MAX_P`] and the frame's aspect. Solving it exactly rather than stepping down to it
/// is what keeps the correction at the cap rather than just under it.
#[must_use]
pub fn cap(wanted: f32, frame_aspect: f32) -> (f32, bool) {
    let aspect = if frame_aspect.is_finite() && frame_aspect > 0.0 {
        frame_aspect.clamp(0.1, 10.0)
    } else {
        1.5
    };
    if stretch_of(wanted, 0.0, aspect) <= MAX_STRETCH + 1e-6 {
        return (wanted, false);
    }
    // `p_max` is what `MAX_STRETCH` allows; the slider that produces it is `p_max` undone through
    // the same scaling `keystone_coefficients` applies.
    let p_max = 1.0 - 1.0 / MAX_STRETCH;
    let slider = p_max * aspect / KEYSTONE_MAX_P * 100.0;
    (slider.copysign(wanted).clamp(-100.0, 100.0), true)
}

/// The rectangle a correction of this stretch leaves usable.
///
/// A centred box whose sides are the reciprocal of the magnification, because the warp magnifies
/// uniformly to fill the corners it opened and what is delivered is therefore the middle of the
/// frame. This is the crop the correction costs, and it is computed before the correction is
/// agreed to.
#[must_use]
pub fn usable(stretch: f32) -> Box2 {
    if !stretch.is_finite() || stretch <= 1.0 {
        return Box2::FULL;
    }
    let side = (1.0 / stretch).clamp(0.0, 1.0);
    Box2 {
        x: (1.0 - side) / 2.0,
        y: (1.0 - side) / 2.0,
        w: side,
        h: side,
    }
}

/// True when a normalised point is inside a rectangle.
fn contains(area: Box2, x: f32, y: f32) -> bool {
    x >= area.x && x <= area.x + area.w && y >= area.y && y <= area.y + area.h
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::geometry::ProtectedContent;

    /// A frame with two straight lines in it that converge toward the top by `lean` of the
    /// frame's width, painted into the pixels.
    fn converging(width: usize, height: usize, lean: f32) -> (Vec<f32>, Vec<f32>) {
        let mut plane = vec![0.15f32; width * height];
        for y in 0..height {
            let t = y as f32 / height as f32;
            // At the top the pair is `lean` narrower on each side than at the bottom.
            let inset = lean * (1.0 - t);
            for (side, base) in [(0usize, 0.22f32), (1, 0.78f32)] {
                let centre = if side == 0 {
                    base + inset
                } else {
                    base - inset
                };
                let x = (centre * width as f32) as isize;
                for dx in -1isize..=1 {
                    let px = x + dx;
                    if px >= 0 && (px as usize) < width {
                        if let Some(slot) = plane.get_mut(y * width + px as usize) {
                            *slot = 0.90;
                        }
                    }
                }
            }
        }
        aura_render::spatial::sobel_planes(&plane, width, height)
    }

    #[test]
    fn a_converging_pair_is_measured_and_a_parallel_one_is_not() {
        let (w, h) = (200usize, 300usize);
        let (gx, gy) = converging(w, h, 0.10);
        let leaning = measure(&gx, &gy, w, h, &[]);
        assert!(
            leaning.convergence > KEYSTONE_ACT_AT,
            "convergence {}",
            leaning.convergence
        );
        assert!(leaning.share > KEYSTONE_MIN_VERTICAL_SHARE, "{}", leaning.share);

        let (gx, gy) = converging(w, h, 0.0);
        let parallel = measure(&gx, &gy, w, h, &[]);
        assert!(
            parallel.convergence.abs() < KEYSTONE_ACT_AT,
            "convergence {}",
            parallel.convergence
        );
    }

    #[test]
    fn verticals_that_are_people_do_not_count_as_architecture() {
        // The safety of this whole operator, as a test: the same converging pair, with the two
        // lines covered by protected regions. What is left must not be enough to act on.
        let (w, h) = (200usize, 300usize);
        let (gx, gy) = converging(w, h, 0.10);
        let people = [
            ProtectedRegion::anonymous(
                ProtectedContent::Face,
                Box2 {
                    x: 0.10,
                    y: 0.0,
                    w: 0.30,
                    h: 1.0,
                },
            ),
            ProtectedRegion::anonymous(
                ProtectedContent::Face,
                Box2 {
                    x: 0.60,
                    y: 0.0,
                    w: 0.30,
                    h: 1.0,
                },
            ),
        ];
        let masked = measure(&gx, &gy, w, h, &people);
        assert!(
            masked.share < KEYSTONE_MIN_VERTICAL_SHARE,
            "share {} - a frame of people read as architecture",
            masked.share
        );
        assert_eq!(
            solve(masked, 200.0 / 300.0, &people, Limits::default()).code,
            GeometryCode::KeystoneNoArchitecture
        );
    }

    #[test]
    fn the_cap_binds_on_a_large_correction_and_not_on_a_small_one() {
        for aspect in [1.5f32, 16.0 / 9.0, 0.8, 1.0] {
            let (small, capped_small) = cap(20.0, aspect);
            assert!(!capped_small || small.abs() <= 20.0);
            let (large, capped_large) = cap(100.0, aspect);
            assert!(capped_large, "a full-slider keystone was not capped at {aspect}");
            assert!(
                stretch_of(large, 0.0, aspect) <= MAX_STRETCH + 1e-4,
                "{aspect}: {} exceeds the cap",
                stretch_of(large, 0.0, aspect)
            );
            // And the capped value is *at* the cap rather than well under it, which is what
            // solving in closed form buys over stepping down.
            assert!(stretch_of(large, 0.0, aspect) > MAX_STRETCH - 0.01);
            let _ = small;
        }
    }

    #[test]
    fn a_correction_never_exceeds_the_cap_whatever_the_convergence() {
        for convergence in [0.4f32, 0.6, 0.8, 1.0] {
            for aspect in [1.5f32, 0.8] {
                let out = solve(
                    Verticals {
                        convergence,
                        share: 0.30,
                    },
                    aspect,
                    &[],
                    Limits {
                        frame_aspect: aspect,
                        ..Limits::default()
                    },
                );
                if let Some(keystone) = out.keystone {
                    assert!(
                        keystone.within_cap(),
                        "{convergence} at {aspect} produced {}",
                        keystone.stretch
                    );
                }
            }
        }
    }

    #[test]
    fn a_correction_that_would_crop_into_somebody_is_refused() {
        // A face hard against the left edge: the magnification that hides the opened corners
        // takes it out of frame, so the correction is abandoned rather than the face cut.
        let edge = [ProtectedRegion::anonymous(
            ProtectedContent::PrimaryFace,
            Box2 {
                x: 0.01,
                y: 0.45,
                w: 0.06,
                h: 0.08,
            },
        )];
        let out = solve(
            Verticals {
                convergence: 0.8,
                share: 0.30,
            },
            1.5,
            &edge,
            Limits::default(),
        );
        assert_eq!(out.code, GeometryCode::KeystoneRefused);
        assert!(out.keystone.is_none());
    }

    #[test]
    fn the_usable_rectangle_is_the_reciprocal_of_the_magnification() {
        assert!((usable(1.0).w - 1.0).abs() < 1e-6);
        let bounds = usable(1.10);
        assert!((bounds.w - 1.0 / 1.10).abs() < 1e-6);
        // Centred, so the two margins are equal.
        assert!((bounds.x - (1.0 - bounds.w) / 2.0).abs() < 1e-6);
    }

    #[test]
    fn a_flat_frame_asks_for_nothing() {
        let (w, h) = (64usize, 64usize);
        let plane = vec![0.4f32; w * h];
        let (gx, gy) = aura_render::spatial::sobel_planes(&plane, w, h);
        let measured = measure(&gx, &gy, w, h, &[]);
        assert_eq!(measured, Verticals::default());
        assert_eq!(
            solve(measured, 1.0, &[], Limits::default()).code,
            GeometryCode::KeystoneNoArchitecture
        );
    }
}
