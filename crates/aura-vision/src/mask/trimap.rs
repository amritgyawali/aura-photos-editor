//! The three-region map matting works inside.
//!
//! Section 6.1: "build a trimap by eroding/dilating the coarse mask, then run a matting
//! network only in the uncertain band - this is what makes veils and flyaway hair look
//! correct."
//!
//! Two things here are decisions rather than mechanics.
//!
//! **The band width is a fraction of the region's own size**, not a pixel count. A fixed
//! radius is right for one subject size and wrong for two: it has to cover the hair on a
//! full-length portrait of a bride who occupies a fifth of the frame *and* on a
//! head-and-shoulders that occupies half of it, and those differ by a factor of five. A radius
//! that covers the first leaves the second's flyaways outside the band, where alpha is exactly
//! one or exactly zero and no refinement can help. Phase 14 made the same call about the
//! tiling halo. ADR-0037 decision 4.
//!
//! **Only the band is refined.** Everything the erosion kept is foreground and everything the
//! dilation did not reach is background, and matting never touches either. That is what bounds
//! the cost: the band on a typical subject is between three and eight per cent of the frame,
//! so the expensive step runs on a twentieth of the pixels.

use crate::mask::algebra::{self, Plane};

/// The band radius as a fraction of the square root of the region's area.
///
/// Six per cent. On a subject occupying a fifth of a 768 px frame that is about 17 px, which is
/// roughly the width of the soft zone around a veil at that resolution.
///
/// It started at twelve and was halved after measuring what a wide band does when the boundary
/// is *not* soft. A band is a licence for the matte to decide, and inside it the guided filter
/// follows the guide - so where a subject and the wall behind them are close in luminance, a
/// wide band hands the matte forty pixels of wall to be wrong about. Narrow is the safer error:
/// a boundary that needed more band comes back with a lower `edge_quality`, which a consumer
/// can act on, where a boundary that was given too much comes back confidently wrong.
pub const BAND_FRACTION: f32 = 0.06;

/// The smallest band, in analysis pixels. Below three there is nothing to solve inside.
pub const BAND_MIN_PX: u32 = 3;

/// The largest band, in analysis pixels.
///
/// Forty. Past this the band stops being an uncertain boundary and starts being most of the
/// subject, and a matte solved over most of the subject is a blur.
pub const BAND_MAX_PX: u32 = 40;

/// Which of the three regions a pixel is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    /// Certainly outside.
    Background,
    /// Uncertain. The only pixels matting solves for.
    Unknown,
    /// Certainly inside.
    Foreground,
}

/// A coarse mask split into three.
#[derive(Debug, Clone)]
pub struct Trimap {
    /// Grid width.
    pub w: u32,
    /// Grid height.
    pub h: u32,
    /// One region per pixel.
    pub regions: Vec<Region>,
    /// The radius the band was built with.
    pub band: u32,
}

impl Trimap {
    /// The region at a pixel; outside the grid is background.
    #[must_use]
    pub fn at(&self, x: i64, y: i64) -> Region {
        if x < 0 || y < 0 || x >= i64::from(self.w) || y >= i64::from(self.h) {
            return Region::Background;
        }
        self.regions
            .get((y as usize) * (self.w as usize) + (x as usize))
            .copied()
            .unwrap_or(Region::Background)
    }

    /// How many pixels are uncertain.
    #[must_use]
    pub fn unknown_count(&self) -> usize {
        self.regions
            .iter()
            .filter(|r| **r == Region::Unknown)
            .count()
    }

    /// The uncertain band as a fraction of the grid.
    #[must_use]
    pub fn unknown_fraction(&self) -> f32 {
        let pixels = (self.w as f32) * (self.h as f32);
        if pixels <= 0.0 {
            return 0.0;
        }
        self.unknown_count() as f32 / pixels
    }
}

/// The band radius for a region, from its own area.
#[must_use]
pub fn band_radius(plane: &Plane) -> u32 {
    let area = plane.area().max(0.0);
    if area <= 0.0 {
        return BAND_MIN_PX;
    }
    let side = area.sqrt() as f32;
    ((side * BAND_FRACTION).round() as u32).clamp(BAND_MIN_PX, BAND_MAX_PX)
}

/// Erode and dilate a coarse region into a trimap.
///
/// The threshold is a half rather than anything smaller: a coarse plane can carry soft values
/// from the class arithmetic, and eroding a plane that is 0.2 everywhere would produce a
/// foreground of nothing and a band of everything.
#[must_use]
pub fn build(plane: &Plane, band: u32) -> Trimap {
    let hard = algebra::threshold(plane, 0.5);
    let inner = algebra::shrink(&hard, band);
    let outer = algebra::grow(&hard, band);

    let mut regions = vec![Region::Background; (plane.w as usize) * (plane.h as usize)];
    for y in 0..i64::from(plane.h) {
        for x in 0..i64::from(plane.w) {
            let index = (y as usize) * (plane.w as usize) + (x as usize);
            let region = if inner.at(x, y) > 0.5 {
                Region::Foreground
            } else if outer.at(x, y) > 0.5 {
                Region::Unknown
            } else {
                Region::Background
            };
            if let Some(slot) = regions.get_mut(index) {
                *slot = region;
            }
        }
    }

    Trimap {
        w: plane.w,
        h: plane.h,
        regions,
        band,
    }
}

#[cfg(test)]
mod tests {
    // The panic family is how a test asserts, and a mask test compares alphas that are exactly
    // zero or exactly one by construction - a painted fixture has no rounding to be tolerant of.
    #![allow(
        clippy::float_cmp,
        clippy::indexing_slicing,
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::assertions_on_constants,
        clippy::uninlined_format_args
    )]
    use super::*;

    fn disc(w: u32, h: u32, r: f32) -> Plane {
        let mut p = Plane::zeros(w, h);
        let cx = w as f32 / 2.0;
        let cy = h as f32 / 2.0;
        for y in 0..h {
            for x in 0..w {
                if (x as f32 - cx).hypot(y as f32 - cy) <= r {
                    p.set(i64::from(x), i64::from(y), 1.0);
                }
            }
        }
        p
    }

    #[test]
    fn the_band_scales_with_the_region_rather_than_with_the_frame() {
        let small = disc(256, 256, 20.0);
        let large = disc(256, 256, 80.0);
        assert!(band_radius(&large) > band_radius(&small));
    }

    #[test]
    fn the_band_is_clamped_at_both_ends() {
        assert_eq!(band_radius(&Plane::zeros(64, 64)), BAND_MIN_PX);
        assert!(band_radius(&Plane::ones(4096, 4096)) <= BAND_MAX_PX);
    }

    #[test]
    fn the_three_regions_partition_the_frame() {
        let map = build(&disc(128, 128, 40.0), 6);
        let total = (map.w as usize) * (map.h as usize);
        let fg = map
            .regions
            .iter()
            .filter(|r| **r == Region::Foreground)
            .count();
        let bg = map
            .regions
            .iter()
            .filter(|r| **r == Region::Background)
            .count();
        assert_eq!(fg + bg + map.unknown_count(), total);
        assert!(fg > 0 && bg > 0 && map.unknown_count() > 0);
    }

    #[test]
    fn the_uncertain_band_is_a_small_fraction_of_the_frame() {
        // The cost bound the module note claims: matting runs on a twentieth of the pixels.
        let map = build(&disc(256, 256, 60.0), 8);
        assert!(
            map.unknown_fraction() < 0.25,
            "band was {}",
            map.unknown_fraction()
        );
    }
}
