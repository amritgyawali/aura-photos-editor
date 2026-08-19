//! Fitting a phase 14 recipe to reproduce a photograph the photographer already delivered.
//!
//! PHASE-17 section 6.1's fallback path, for the photographer who kept JPEGs and not
//! catalogues - which is most of them:
//!
//! > Optimise a small parameter vector (exposure, WB, tone, curve, HSL) by coordinate descent
//! > on a perceptual loss (dE00 on a downsampled grid plus histogram matching), typically
//! > 60-120 render iterations at 512 px.
//!
//! ## The loss, and why it has two terms
//!
//! **Mean dE00 on a sub-sampled grid** is the term that matters: it is a perceptual distance in
//! CIE `Lab`, it is what section 10.1 is measured in, and it is what a photographer would call
//! "the same colour".
//!
//! **A luminance-histogram distance** is the second term and it exists because dE00 on a grid is
//! nearly flat with respect to the black point. Moving `blacks` by ten points changes a handful
//! of pixels a great deal and most pixels not at all, so a mean over a grid barely moves and the
//! coordinate sweep for that parameter has no gradient to follow. The histogram term is sensitive
//! to exactly the thing the mean is blind to.
//!
//! [`HISTOGRAM_WEIGHT`] is what balances them, and it is deliberately small: the histogram term
//! is there to break ties along directions the dE00 term cannot see, not to become the objective.
//!
//! ## Why coordinate descent and not a gradient
//!
//! There is no automatic differentiation in this workspace and twelve parameters do not justify
//! adding one. More usefully, the parameters are **not all smooth in the loss**: highlight
//! recovery has a knee, a curve anchor moves a plateau, and the white point does nothing at all
//! until it starts clipping. A coordinate sweep with a shrinking step handles a kink without a
//! Jacobian, and it cannot diverge - every accepted move strictly decreases the loss, so the
//! sequence is monotone and bounded below by zero.
//!
//! ## The rejection is the point
//!
//! A pair whose final residual is above [`REJECT_DE00`] is thrown out, counted, and named. A
//! residual the develop pipeline cannot express is unmodelled work - a local dodge, a
//! composite, a heavy crop, a sky replacement - and fitting it anyway puts that work into a
//! *global* tone delta. That is how a style profile learns to lift every shadow in a wedding
//! because forty frames had a brightened face. ADR-0035 decision 4.
//!
//! [`Fitted::localised`] is the second half of the same check: a residual that is *concentrated*
//! rather than spread is the signature of local retouching even when its mean is acceptable, and
//! that pair is rejected as [`StyleCode::UnmodelledWork`] rather than as a bad fit.

pub mod optimise;

use aura_core::contract::style::StyleCode;
use aura_raw::colour::de2000::{ciede2000, xyz_d65_to_lab, Lab};
use aura_raw::colour::matrix::{apply as apply_matrix, SRGB_TO_XYZ_D65};

pub use optimise::{fit, Budget, Candidate, PARAMS};

/// Above this residual a pair is rejected rather than learned from, in dE00.
///
/// Three. Section 10.1's style-match gate is 2.5 dE00 on held-out pairs, so a *training* pair
/// that the pipeline cannot reproduce to within three has nothing to teach about parameters -
/// whatever it teaches is about the work the pipeline cannot express. Setting the two equal was
/// the first attempt and it rejected pairs whose only problem was that the fit had not converged
/// yet; half a unit of headroom is what separates "the pipeline cannot do this" from "the
/// optimiser stopped early".
pub const REJECT_DE00: f32 = 3.0;

/// The weight of the histogram term in the loss.
///
/// A fifth. Large enough that the black point has a gradient to follow, small enough that the
/// objective is still perceptual: at a half the fitter began matching a photographer's *contrast
/// histogram* at the cost of a visible colour shift, which is a lower loss and a worse fit.
pub const HISTOGRAM_WEIGHT: f32 = 0.2;

/// The long edge the fit runs at, in pixels.
///
/// 512, section 6.1's own figure. The parameters being recovered are global, so the extra
/// resolution buys nothing and costs sixteen times the render; and the sub-sampled grid over a
/// 512 px frame is already several thousand samples per iteration.
pub const FIT_LONG_EDGE: u32 = 512;

/// How many samples the dE00 grid takes.
///
/// 1,024, on a regular lattice rather than at random. A regular lattice is reproducible -
/// invariant 4 - and it is also *better* here: a random sample of a photograph over-represents
/// whatever is largest, and a wedding photograph is mostly one wall.
pub const GRID_SAMPLES: usize = 1024;

/// How many tiles the localisation check divides the frame into, per axis.
///
/// Eight, so sixty-four tiles. Enough that a retouched face occupies one or two of them and not
/// so many that a tile is smaller than the thing being looked for.
pub const LOCAL_TILES: usize = 8;

/// How many times the worst tile's residual may exceed the frame's mean before the work is
/// called local.
///
/// Four. A global grade that the fitter reproduced imperfectly is imperfect roughly everywhere;
/// a dodged face is perfect everywhere except one tile. Four separates them on every fixture and
/// it is the number the gate is measured against.
pub const LOCAL_RATIO: f32 = 4.0;

/// The luminance histogram used by the second loss term.
pub const HISTOGRAM_BINS: usize = 32;

/// What one fit produced.
#[derive(Debug, Clone, PartialEq)]
pub struct Fitted {
    /// The recovered parameters, as an [`optimise::Candidate`].
    pub candidate: Candidate,
    /// The mean dE00 the fit could not remove.
    pub residual_de00: f32,
    /// The worst tile's dE00, for [`Fitted::localised`].
    pub worst_tile_de00: f32,
    /// How many renders it took.
    pub iterations: u32,
}

impl Fitted {
    /// True when the residual is concentrated in a few tiles rather than spread over the frame.
    ///
    /// The signature of local retouching. It is checked **even when the mean residual passes**,
    /// because a dodged face in a 512 px frame is about one percent of the pixels and can hide
    /// inside an acceptable mean - and a pair like that teaches the profile that this
    /// photographer lifts shadows globally, which they do not.
    #[must_use]
    pub fn localised(&self) -> bool {
        self.residual_de00 > 0.05 && self.worst_tile_de00 > self.residual_de00 * LOCAL_RATIO
    }

    /// Why this pair cannot be learned from, when it cannot.
    ///
    /// The order matters: a localised residual is reported as unmodelled work even when it is
    /// also too high, because the two send a support engineer to different places.
    #[must_use]
    pub fn rejection(&self) -> Option<StyleCode> {
        if self.localised() {
            Some(StyleCode::UnmodelledWork)
        } else if self.residual_de00 > REJECT_DE00 {
            Some(StyleCode::FitResidualTooHigh)
        } else {
            None
        }
    }
}

/// The perceptual loss between a rendered candidate and the photographer's own final.
///
/// Both buffers are interleaved 8-bit sRGB of the same size. The dE00 term walks a regular
/// lattice; the histogram term compares thirty-two luminance bins over the whole frame.
#[must_use]
pub fn loss(rendered: &[u8], target: &[u8], width: usize, height: usize) -> f32 {
    let (mean, _) = grid_de00(rendered, target, width, height);
    let histogram = histogram_distance(rendered, target);
    mean + HISTOGRAM_WEIGHT * histogram
}

/// The mean dE00 over the lattice, and the worst tile's mean.
///
/// One walk, both numbers: the localisation check is free if it rides along with the loss, and
/// a second walk over a 512 px frame per iteration is not.
#[must_use]
pub fn grid_de00(rendered: &[u8], target: &[u8], width: usize, height: usize) -> (f32, f32) {
    if width == 0 || height == 0 {
        return (0.0, 0.0);
    }
    let stride = ((width * height) as f32 / GRID_SAMPLES as f32)
        .sqrt()
        .max(1.0) as usize;
    let mut total = 0.0_f64;
    let mut count = 0_u32;
    let mut tiles = [(0.0_f64, 0_u32); LOCAL_TILES * LOCAL_TILES];

    let mut y = 0;
    while y < height {
        let mut x = 0;
        while x < width {
            let offset = (y * width + x) * 3;
            let (Some(a), Some(b)) = (
                rendered.get(offset..offset + 3),
                target.get(offset..offset + 3),
            ) else {
                x += stride;
                continue;
            };
            let delta = ciede2000(lab_of(a), lab_of(b));
            total += delta;
            count += 1;

            let tile_x = (x * LOCAL_TILES / width).min(LOCAL_TILES - 1);
            let tile_y = (y * LOCAL_TILES / height).min(LOCAL_TILES - 1);
            if let Some(slot) = tiles.get_mut(tile_y * LOCAL_TILES + tile_x) {
                slot.0 += delta;
                slot.1 += 1;
            }
            x += stride;
        }
        y += stride;
    }

    if count == 0 {
        return (0.0, 0.0);
    }
    let mean = (total / f64::from(count)) as f32;
    let worst = tiles
        .iter()
        .filter(|(_, n)| *n >= 4)
        .map(|(sum, n)| (sum / f64::from(*n)) as f32)
        .fold(0.0_f32, f32::max);
    (mean, worst)
}

/// The L1 distance between two luminance histograms, normalised to `0..1`.
#[must_use]
pub fn histogram_distance(rendered: &[u8], target: &[u8]) -> f32 {
    let left = histogram(rendered);
    let right = histogram(target);
    let mut total = 0.0_f32;
    for index in 0..HISTOGRAM_BINS {
        let a = left.get(index).copied().unwrap_or(0.0);
        let b = right.get(index).copied().unwrap_or(0.0);
        total += (a - b).abs();
    }
    // Two disjoint histograms differ by two, so halving puts the range at 0..1.
    total * 0.5
}

fn histogram(rgb: &[u8]) -> [f32; HISTOGRAM_BINS] {
    let mut bins = [0.0_f32; HISTOGRAM_BINS];
    let mut count = 0_u32;
    for pixel in rgb.chunks_exact(3) {
        let (Some(r), Some(g), Some(b)) = (pixel.first(), pixel.get(1), pixel.get(2)) else {
            continue;
        };
        // Rec.709 luma over the encoded values. Encoded rather than linear on purpose: the
        // black point and the white point are placements in the *output*, and matching a
        // photographer's histogram means matching the one they looked at.
        let luma = 0.2126 * f32::from(*r) + 0.7152 * f32::from(*g) + 0.0722 * f32::from(*b);
        let bin = ((luma / 256.0) * HISTOGRAM_BINS as f32) as usize;
        if let Some(slot) = bins.get_mut(bin.min(HISTOGRAM_BINS - 1)) {
            *slot += 1.0;
        }
        count += 1;
    }
    if count > 0 {
        for slot in &mut bins {
            *slot /= count as f32;
        }
    }
    bins
}

fn lab_of(pixel: &[u8]) -> Lab {
    let linear = [
        srgb_to_linear(f64::from(pixel.first().copied().unwrap_or(0)) / 255.0),
        srgb_to_linear(f64::from(pixel.get(1).copied().unwrap_or(0)) / 255.0),
        srgb_to_linear(f64::from(pixel.get(2).copied().unwrap_or(0)) / 255.0),
    ];
    // `xyz_d65_to_lab` rather than `xyz_to_lab` with a hand-written white: the D65 white point
    // is already written down once in `aura-raw` and a second copy of three constants is a
    // second copy that drifts.
    xyz_d65_to_lab(apply_matrix(SRGB_TO_XYZ_D65, linear))
}

fn srgb_to_linear(value: f64) -> f64 {
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(width: usize, height: usize, rgb: [u8; 3]) -> Vec<u8> {
        let mut out = Vec::with_capacity(width * height * 3);
        for _ in 0..width * height {
            out.extend_from_slice(&rgb);
        }
        out
    }

    #[test]
    fn a_perfect_reproduction_has_zero_loss() {
        let target = flat(64, 64, [120, 110, 100]);
        assert!(loss(&target, &target, 64, 64).abs() < 1e-4);
    }

    #[test]
    fn the_loss_grows_with_the_colour_difference() {
        let target = flat(64, 64, [120, 110, 100]);
        let near = flat(64, 64, [122, 112, 102]);
        let far = flat(64, 64, [180, 90, 60]);
        assert!(loss(&near, &target, 64, 64) < loss(&far, &target, 64, 64));
    }

    #[test]
    fn the_histogram_term_sees_a_black_point_the_mean_barely_does() {
        // Two frames that agree everywhere except in a small dark region. The dE00 mean over
        // the grid is dominated by the agreeing majority; the histogram is not.
        let width = 64;
        let height = 64;
        let mut target = flat(width, height, [128, 128, 128]);
        let mut moved = target.clone();
        for index in 0..(width * 4) {
            let offset = index * 3;
            if let Some(slice) = target.get_mut(offset..offset + 3) {
                slice.copy_from_slice(&[10, 10, 10]);
            }
            if let Some(slice) = moved.get_mut(offset..offset + 3) {
                slice.copy_from_slice(&[40, 40, 40]);
            }
        }
        assert!(histogram_distance(&moved, &target) > 0.0);
    }

    #[test]
    fn a_concentrated_residual_is_called_local_and_a_spread_one_is_not() {
        let local = Fitted {
            candidate: Candidate::neutral(),
            residual_de00: 0.5,
            worst_tile_de00: 6.0,
            iterations: 40,
        };
        assert!(local.localised());
        assert_eq!(local.rejection(), Some(StyleCode::UnmodelledWork));

        let spread = Fitted {
            candidate: Candidate::neutral(),
            residual_de00: 1.2,
            worst_tile_de00: 2.0,
            iterations: 40,
        };
        assert!(!spread.localised());
        assert_eq!(spread.rejection(), None);
    }

    #[test]
    fn a_high_spread_residual_is_a_bad_fit_rather_than_unmodelled_work() {
        let bad = Fitted {
            candidate: Candidate::neutral(),
            residual_de00: 5.0,
            worst_tile_de00: 6.0,
            iterations: 120,
        };
        assert_eq!(bad.rejection(), Some(StyleCode::FitResidualTooHigh));
    }

    #[test]
    fn a_perfect_fit_is_never_called_local_however_small_the_numbers_are() {
        let perfect = Fitted {
            candidate: Candidate::neutral(),
            residual_de00: 0.001,
            worst_tile_de00: 0.02,
            iterations: 12,
        };
        assert!(!perfect.localised());
    }

    #[test]
    fn the_worst_tile_finds_a_region_the_mean_hides() {
        let width = 64;
        let height = 64;
        let target = flat(width, height, [128, 128, 128]);
        let mut rendered = target.clone();
        // Paint one 8x8 tile far away.
        for y in 0..8 {
            for x in 0..8 {
                let offset = (y * width + x) * 3;
                if let Some(slice) = rendered.get_mut(offset..offset + 3) {
                    slice.copy_from_slice(&[220, 40, 40]);
                }
            }
        }
        let (mean, worst) = grid_de00(&rendered, &target, width, height);
        assert!(worst > mean * LOCAL_RATIO, "mean {mean} worst {worst}");
    }
}
