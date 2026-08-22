//! How wide the blur on this frame actually is, measured from its own edges.
//!
//! Section 6.2's first bullet: "Estimate the blur kernel width from edge profiles". This module
//! is that estimate and nothing else - it makes no decision, and
//! [`crate::sharpen`] is what decides whether the number it produces is worth acting on.
//!
//! ## What an edge profile is, and why the measurement is a quantile rather than a mean
//!
//! A step edge convolved with a Gaussian of width sigma has a gradient profile that *is* that
//! Gaussian, so the width of the gradient ridge across an edge is the kernel. Measuring it means
//! walking perpendicular to an edge and finding how far the gradient stays above half its peak -
//! the full width at half maximum, which for a Gaussian is `2.355 * sigma`.
//!
//! A photograph has edges of every kind in it: a hard specular boundary that is sharper than the
//! lens, a soft shadow terminator that is genuinely gradual, and the subject's own edges in
//! between. A **mean** over all of them measures the scene rather than the lens, and it is
//! dominated by the soft ones because there are more of them. What the deconvolution needs is the
//! width of the *sharpest* structures the frame contains, because that is what the optical system
//! actually managed - so the estimate is a low quantile of the per-edge widths.
//! [`SHARPEST_QUANTILE`] is that choice.
//!
//! ## The estimate is deliberately biased toward "sharper than it is"
//!
//! A kernel estimate that is too small produces a weaker deconvolution, which loses nothing. One
//! that is too large produces ringing, which cannot be undone. The quantile is low for that
//! reason as well as for the optical one, and
//! [`aura_core::contract::restore::SHARPEN_KERNEL_HI`] is the backstop: a frame that measures
//! wider than the band is refused rather than deconvolved harder.

use aura_render::spatial;

/// Which quantile of the per-edge widths is taken as the kernel.
///
/// The twentieth percentile. See the module header: a mean measures the scene and a minimum
/// measures the noisiest single edge in the frame, so the estimate is the narrow end of the
/// distribution without being its extreme.
pub const SHARPEST_QUANTILE: f32 = 0.20;

/// How strong an edge has to be, as a share of the frame's strongest, to be profiled at all.
///
/// A quarter. Below this the gradient ridge is comparable with the noise on it and the
/// half-maximum crossing is not a measurement of anything.
pub const MIN_EDGE_STRENGTH: f32 = 0.25;

/// The furthest the profile walks from an edge, in samples.
///
/// Six. A kernel wider than this is well past `SHARPEN_KERNEL_HI` and will be refused anyway, so
/// walking further only costs time and lets one edge's profile run into the next one's.
pub const MAX_WALK: usize = 6;

/// The fewest edges a frame must contain before an estimate means anything.
///
/// Twenty-four. Below this the quantile is being taken over a handful of samples and a single
/// specular boundary decides the answer for the whole photograph.
pub const MIN_EDGES: usize = 24;

/// The conversion from a full width at half maximum to a Gaussian sigma.
pub const FWHM_TO_SIGMA: f32 = 1.0 / 2.354_82;

/// What the frame's own edges say about its blur.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KernelEstimate {
    /// The estimated Gaussian sigma, in pixels.
    pub sigma: f32,
    /// How many edges the estimate was taken over.
    pub edges: u32,
    /// The frame's strongest gradient, for the caller's own thresholds.
    pub peak: f32,
}

impl KernelEstimate {
    /// The estimate for a frame with nothing measurable in it.
    pub const NONE: Self = Self {
        sigma: 0.0,
        edges: 0,
        peak: 0.0,
    };

    /// True when enough edges contributed for the number to mean something.
    #[must_use]
    pub const fn is_reliable(&self) -> bool {
        self.edges as usize >= MIN_EDGES
    }
}

/// Estimate the blur kernel from a linear RGB frame's own edges.
///
/// `pixels` is interleaved linear RGB, `width * height * 3` long.
#[must_use]
pub fn estimate(pixels: &[f32], width: usize, height: usize) -> KernelEstimate {
    if width < 8 || height < 8 || pixels.len() < width * height * 3 {
        return KernelEstimate::NONE;
    }
    let plane = spatial::luma_plane(pixels, width, height);
    let gradient = spatial::gradient_plane(&plane, width, height);
    let peak = gradient.iter().copied().fold(0.0_f32, f32::max);
    if peak <= 1e-6 {
        return KernelEstimate::NONE;
    }
    let floor = peak * MIN_EDGE_STRENGTH;

    let mut widths: Vec<f32> = Vec::new();
    // The interior only: a profile that walks off the frame is a profile the clamp has flattened,
    // and a flattened profile reads as a wide kernel. Phase 18's resampler defect, in the shape
    // this module could have had it - reading outside a plane is how a measurement invents a
    // gradient that is not there.
    for y in MAX_WALK..height.saturating_sub(MAX_WALK) {
        for x in MAX_WALK..width.saturating_sub(MAX_WALK) {
            let index = y * width + x;
            let centre = gradient.get(index).copied().unwrap_or(0.0);
            if centre < floor {
                continue;
            }
            // Only ridge maxima, so one edge contributes one sample rather than one per pixel of
            // its own width - which would weight wide edges more heavily and bias the estimate
            // toward exactly the thing it is trying to measure.
            let horizontal = ridge_peak(&gradient, width, index, 1);
            let vertical = ridge_peak(&gradient, width, index, width);
            if !horizontal && !vertical {
                continue;
            }
            let step = if horizontal { 1 } else { width };
            if let Some(fwhm) = half_maximum_width(&gradient, width, height, index, step) {
                widths.push(fwhm * FWHM_TO_SIGMA);
            }
        }
    }

    if widths.len() < MIN_EDGES {
        return KernelEstimate {
            sigma: 0.0,
            edges: widths.len() as u32,
            peak,
        };
    }
    widths.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let position = ((widths.len() as f32 - 1.0) * SHARPEST_QUANTILE).round() as usize;
    let sigma = widths.get(position).copied().unwrap_or(0.0);
    KernelEstimate {
        sigma,
        edges: widths.len() as u32,
        peak,
    }
}

/// True when this sample is a local maximum of the gradient along one axis.
fn ridge_peak(gradient: &[f32], width: usize, index: usize, step: usize) -> bool {
    if index < step || index + step >= gradient.len() {
        return false;
    }
    // A ridge that runs off the row is not a ridge along this axis.
    if step == 1 && (index.is_multiple_of(width) || (index + 1).is_multiple_of(width)) {
        return false;
    }
    let centre = gradient.get(index).copied().unwrap_or(0.0);
    let before = gradient.get(index - step).copied().unwrap_or(0.0);
    let after = gradient.get(index + step).copied().unwrap_or(0.0);
    centre >= before && centre > after
}

/// The full width at half maximum of one gradient ridge, in samples.
///
/// Returns `None` when the ridge does not fall to half its peak inside [`MAX_WALK`], which is a
/// ridge too wide to be a kernel this phase will act on and is therefore better excluded than
/// clamped - a clamped width would report exactly `MAX_WALK` for every gradual shadow in the
/// frame and drag the quantile with it.
fn half_maximum_width(
    gradient: &[f32],
    width: usize,
    height: usize,
    index: usize,
    step: usize,
) -> Option<f32> {
    let peak = gradient.get(index).copied().unwrap_or(0.0);
    if peak <= 1e-6 {
        return None;
    }
    let half = peak * 0.5;
    let limit = width * height;

    let mut before = None;
    for distance in 1..=MAX_WALK {
        let offset = distance * step;
        if offset > index {
            return None;
        }
        let value = gradient.get(index - offset).copied().unwrap_or(0.0);
        if value <= half {
            // Linear interpolation between the two samples that straddle the half maximum, so the
            // estimate has sub-sample resolution. Without it every kernel in the frame is an
            // integer number of pixels and the quantile becomes a histogram with four bins.
            let previous = gradient.get(index - offset + step).copied().unwrap_or(peak);
            let span = (previous - value).max(1e-9);
            before = Some(distance as f32 - (half - value) / span);
            break;
        }
    }

    let mut after = None;
    for distance in 1..=MAX_WALK {
        let offset = distance * step;
        if index + offset >= limit {
            return None;
        }
        let value = gradient.get(index + offset).copied().unwrap_or(0.0);
        if value <= half {
            let previous = gradient.get(index + offset - step).copied().unwrap_or(peak);
            let span = (previous - value).max(1e-9);
            after = Some(distance as f32 - (half - value) / span);
            break;
        }
    }

    match (before, after) {
        (Some(a), Some(b)) => Some(a + b),
        _ => None,
    }
}

#[cfg(test)]
// `-D warnings` on the command line beats the crate-level `cfg_attr(test, allow(..))`
// block, so a test that compares two floats it computed itself needs the allow here.
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use aura_render::bands;

    /// A frame of vertical bars, blurred by a known radius.
    fn bars(width: usize, height: usize, period: usize, radius: usize) -> Vec<f32> {
        let mut plane = vec![0.0f32; width * height];
        for y in 0..height {
            for x in 0..width {
                plane[y * width + x] = if (x / period).is_multiple_of(2) {
                    0.15
                } else {
                    0.75
                };
            }
        }
        let blurred = if radius == 0 {
            plane
        } else {
            bands::blur(&plane, width, height, radius)
        };
        let mut pixels = Vec::with_capacity(width * height * 3);
        for value in blurred {
            pixels.extend_from_slice(&[value, value, value]);
        }
        pixels
    }

    #[test]
    fn a_frame_with_nothing_in_it_estimates_nothing() {
        let flat = vec![0.4f32; 64 * 64 * 3];
        let estimate = estimate(&flat, 64, 64);
        assert_eq!(estimate.edges, 0);
        assert!(!estimate.is_reliable());
        assert_eq!(estimate.sigma, 0.0);
    }

    #[test]
    fn a_frame_too_small_to_profile_estimates_nothing() {
        let tiny = vec![0.4f32; 4 * 4 * 3];
        assert_eq!(estimate(&tiny, 4, 4), KernelEstimate::NONE);
    }

    #[test]
    fn a_blurrier_frame_measures_a_wider_kernel() {
        // The property the whole module exists for, and the only one that has to hold for the
        // decision above it to be sound: the estimate has to be monotone in the actual blur.
        let sharp = estimate(&bars(96, 96, 12, 1), 96, 96);
        let soft = estimate(&bars(96, 96, 12, 3), 96, 96);
        let softer = estimate(&bars(96, 96, 12, 5), 96, 96);
        assert!(sharp.is_reliable(), "{sharp:?}");
        assert!(soft.is_reliable(), "{soft:?}");
        assert!(softer.is_reliable(), "{softer:?}");
        assert!(
            soft.sigma > sharp.sigma,
            "radius 3 measured {} against radius 1's {}",
            soft.sigma,
            sharp.sigma
        );
        assert!(
            softer.sigma > soft.sigma,
            "radius 5 measured {} against radius 3's {}",
            softer.sigma,
            soft.sigma
        );
    }

    #[test]
    fn the_estimate_ignores_a_frame_with_too_few_edges() {
        // One edge in the middle of an otherwise flat frame. The quantile over a handful of
        // samples is decided by whichever pixel happened to be noisiest, so the estimate reports
        // its edge count and a sigma of zero rather than a plausible number.
        let width = 64;
        let height = 64;
        let mut pixels = vec![0.2f32; width * height * 3];
        for y in 30..34 {
            for x in 0..width {
                for offset in 0..3 {
                    pixels[(y * width + x) * 3 + offset] = 0.8;
                }
            }
        }
        let estimate = estimate(&pixels, width, height);
        assert!(!estimate.is_reliable() || estimate.edges >= MIN_EDGES as u32);
    }

    #[test]
    fn a_soft_shadow_does_not_drag_the_estimate() {
        // The reason the quantile is low. A frame containing both a hard edge and a broad
        // gradient must measure the hard edge, because that is what the optical system managed;
        // a mean would measure the gradient because there are more pixels in it.
        let width = 128;
        let height = 64;
        let mut plane = vec![0.0f32; width * height];
        for y in 0..height {
            for x in 0..width {
                // Left half: a broad ramp. Right half: hard bars.
                plane[y * width + x] = if x < width / 2 {
                    0.2 + 0.5 * (x as f32 / (width as f32 / 2.0))
                } else if ((x - width / 2) / 6) % 2 == 0 {
                    0.15
                } else {
                    0.75
                };
            }
        }
        let blurred = bands::blur(&plane, width, height, 1);
        let mut pixels = Vec::with_capacity(width * height * 3);
        for value in blurred {
            pixels.extend_from_slice(&[value, value, value]);
        }
        let with_ramp = estimate(&pixels, width, height);
        let bars_only = estimate(&bars(128, 64, 6, 1), 128, 64);
        assert!(with_ramp.is_reliable(), "{with_ramp:?}");
        assert!(
            (with_ramp.sigma - bars_only.sigma).abs() < 0.35,
            "the ramp moved the estimate from {} to {}",
            bars_only.sigma,
            with_ramp.sigma
        );
    }

    #[test]
    fn the_contract_floor_sits_above_the_estimator_own_floor() {
        // **The defect phase 22 shipped first, as a permanent guard.** A Sobel gradient ridge
        // across a mathematically perfect step edge is two samples wide, so this estimator cannot
        // report a sigma below `2 / 2.35482 = 0.849` for anything. A contract floor under that
        // number is a floor no photograph is ever under, and the consequence is not a subtle bias:
        // *every frame in every wedding* is reported as recoverably soft and deconvolved.
        //
        // This asserts the two numbers against each other rather than asserting either alone, so
        // that a change to the estimator which moved its floor - a different operator, a different
        // interpolation - fails here instead of quietly re-opening the defect.
        let perfect = crate::fixtures::edge_plate(96, 96, 8);
        let measured = estimate(&perfect, 96, 96);
        assert!(measured.is_reliable(), "{measured:?}");
        assert!(
            measured.sigma < aura_core::contract::restore::SHARPEN_KERNEL_LO,
            "the sharpest image that can exist measures {} against a floor of {}",
            measured.sigma,
            aura_core::contract::restore::SHARPEN_KERNEL_LO
        );

        // And the lace plate, which is the other synthetic frame in this phase that has no blur
        // in it at all.
        let lace = crate::fixtures::lace_plate(96, 96);
        let measured = estimate(&lace, 96, 96);
        assert!(measured.sigma < aura_core::contract::restore::SHARPEN_KERNEL_LO);
    }

    #[test]
    fn a_genuinely_soft_frame_lands_inside_the_band() {
        // The other half of the same guard: the floor must not be so high that nothing reaches it.
        let hard = crate::fixtures::edge_plate(96, 96, 8);
        let plane = aura_render::spatial::luma_plane(&hard, 96, 96);
        let blurred = aura_render::bands::blur(&plane, 96, 96, 1);
        let mut pixels = Vec::with_capacity(96 * 96 * 3);
        for value in blurred {
            pixels.extend_from_slice(&[value, value, value]);
        }
        let measured = estimate(&pixels, 96, 96);
        assert!(
            measured.sigma >= aura_core::contract::restore::SHARPEN_KERNEL_LO
                && measured.sigma <= aura_core::contract::restore::SHARPEN_KERNEL_HI,
            "a softened frame measured {} outside {}..{}",
            measured.sigma,
            aura_core::contract::restore::SHARPEN_KERNEL_LO,
            aura_core::contract::restore::SHARPEN_KERNEL_HI
        );
    }
}
