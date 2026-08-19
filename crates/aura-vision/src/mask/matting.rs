//! Alpha in the uncertain band: a guided filter, solved in closed form.
//!
//! # Why there is no network here
//!
//! Section 6.1 asks for a matting network inside the trimap band.
//! [`MATTING_HEAD_TRAINED`] is `false` and the head is registered, signed and carded without
//! being consulted, for two reasons in the order they mattered.
//!
//! The interpreter in `aura-infer` implements a documented ONNX opset 13 subset with no
//! `Resize` and no `ConvTranspose` (ADR-0007), so a matting decoder cannot be executed in this
//! build at all. And - the reason that would still hold if it could - the guided filter is not
//! a placeholder standing in for the real thing. It *is* a matting algorithm with a closed
//! form, it is what most matting networks are refined by anyway, and its failure mode is a
//! slightly soft edge rather than a confidently wrong one. ADR-0037 decision 3.
//!
//! # What the closed form is
//!
//! Inside a window around each pixel, alpha is modelled as an affine function of the guide:
//! `a = k * I + b`. Least squares over the window gives
//!
//! ```text
//! k = cov(I, p) / (var(I) + eps)
//! b = mean(p) - k * mean(I)
//! ```
//!
//! and the output is the mean of `k * I + b` over every window containing the pixel. `eps` is
//! what decides how much a change in the guide has to matter before the matte follows it, and
//! it is the one parameter that has to be argued about; see [`EPSILON`].
//!
//! This is exactly the arithmetic that makes a veil work: inside the band, the guide - the
//! photograph's own luminance - varies smoothly from the dress through the veil to the wall,
//! and the affine model turns that variation into partial alpha rather than into a decision.

use crate::mask::algebra::{self, Plane};
use crate::mask::segment::Features;
use crate::mask::trimap::{Region, Trimap};

/// Whether the learned matting head is trained in this build. It is not.
pub const MATTING_HEAD_TRAINED: bool = false;

/// The name the matting head is registered under.
pub const MATTING_MODEL: &str = "alpha_matting";

/// The window radius the affine model is fitted over, as a fraction of the band.
///
/// Half. The window has to be wide enough to contain both sides of the boundary - otherwise
/// the variance in it is noise and the model has nothing to fit - and narrow enough that the
/// affine assumption holds, which it does not across a whole subject.
pub const WINDOW_FRACTION: f32 = 0.5;

/// The regularisation in the closed form.
///
/// `1e-4` on luminance values that live in `0 ..= 1` with a scene-referred exposure, which is
/// about a hundredth of a stop of variance. Larger and a veil against a wall of nearly the
/// same brightness produces `k` near zero and the matte reverts to the coarse mask - which is
/// the exact case the matting exists for. Smaller and sensor noise inside a flat background
/// starts steering the alpha, which is a matte that fizzes along an edge that should be clean.
pub const EPSILON: f32 = 1e-4;

/// The guide variance below which a window carries no information about a boundary.
///
/// `4e-4` on linear luminance, which is a standard deviation of two per cent - about the
/// variance sensor noise leaves in a flat wall and well below what any real edge produces.
///
/// **This is the guard that stops the matte from creeping outwards.** Where the guide is flat,
/// the closed form degenerates: `k` goes to zero and the output becomes `mean(p)` over the
/// window, which is a *blur of the coarse mask*. A blurred mask still reads as about a half
/// several pixels outside the true boundary, so a band pixel sitting in a featureless wall
/// comes back at 0.5 and everything downstream treats it as half inside the subject. Ten pixels
/// of wall around every person is exactly the halo section 10.1 audits for, manufactured by the
/// refinement that was supposed to prevent it.
///
/// Below the floor the coarse answer is kept unchanged, which is the honest fallback: the
/// photograph does not contain the boundary, so there is nothing to refine it with.
pub const VARIANCE_FLOOR: f32 = 4e-4;

/// What the learned head would say about the alpha at a pixel.
///
/// Always `None` in this build, and a function rather than an absence so the call site reads
/// the same before and after the head is trained.
#[must_use]
pub fn alpha_hint(_features: &Features, _map: &Trimap) -> Option<Plane> {
    if MATTING_HEAD_TRAINED {
        return None;
    }
    None
}

/// A solved boundary, and how much of it the photograph actually determined.
///
/// `trust` is the fraction of the uncertain band whose guide carried enough variance to solve
/// with - see [`VARIANCE_FLOOR`]. It is returned rather than recomputed because it is the one
/// number that separates "this boundary is crisp" from "this boundary was not in the picture
/// and the coarse mask was kept", and those two produce *identical* alpha values: both are hard
/// zeros and ones. A decisiveness measure alone rates the second higher than the first, which
/// is precisely backwards.
#[derive(Debug, Clone)]
pub struct Matte {
    /// The refined alpha.
    pub alpha: Plane,
    /// How much of the band was solvable, `0.0 ..= 1.0`.
    pub trust: f32,
}

/// Refine a coarse region inside its uncertain band.
///
/// Outside the band nothing moves: the eroded interior stays at one and everything the
/// dilation did not reach stays at zero. That is what bounds both the cost and the damage - a
/// matte that could change a pixel in the middle of somebody's face is a matte that can put a
/// hole in a face mask.
#[must_use]
pub fn refine(features: &Features, coarse: &Plane, map: &Trimap) -> Matte {
    if let Some(learned) = alpha_hint(features, map) {
        return Matte {
            alpha: learned,
            trust: 1.0,
        };
    }
    if map.unknown_count() == 0 || coarse.is_degenerate() {
        return Matte {
            alpha: coarse.clone(),
            trust: 0.0,
        };
    }

    let radius = ((map.band as f32 * WINDOW_FRACTION).round() as u32).max(1);

    // The guide is the frame's own luminance on the analysis grid. Luminance rather than the
    // three channels because a colour guided filter needs a 3x3 inverse per window, and the
    // boundary this is solving - hair against a background - is a luminance boundary in every
    // wedding photograph that has ever needed it.
    let guide = Plane::from_vec_unclamped(features.w, features.h, features.luma.clone());
    let hard = algebra::threshold(coarse, 0.5);

    let mean_i = algebra::box_blur(&guide, radius);
    let mean_p = algebra::box_blur(&hard, radius);
    let mut cross = Plane::zeros(guide.w, guide.h);
    let mut square = Plane::zeros(guide.w, guide.h);
    for (index, slot) in cross.a.iter_mut().enumerate() {
        let i = guide.a.get(index).copied().unwrap_or(0.0);
        let p = hard.a.get(index).copied().unwrap_or(0.0);
        *slot = i * p;
    }
    for (index, slot) in square.a.iter_mut().enumerate() {
        let i = guide.a.get(index).copied().unwrap_or(0.0);
        *slot = i * i;
    }
    let mean_cross = algebra::box_blur(&cross, radius);
    let mean_square = algebra::box_blur(&square, radius);

    let mut k = Plane::zeros(guide.w, guide.h);
    let mut b = Plane::zeros(guide.w, guide.h);
    let mut informative = Plane::zeros(guide.w, guide.h);
    for index in 0..k.a.len() {
        let mi = mean_i.a.get(index).copied().unwrap_or(0.0);
        let mp = mean_p.a.get(index).copied().unwrap_or(0.0);
        let m_cross = mean_cross.a.get(index).copied().unwrap_or(0.0);
        let m_square = mean_square.a.get(index).copied().unwrap_or(0.0);
        let cov = m_cross - mi * mp;
        let var = (m_square - mi * mi).max(0.0);
        let slope = cov / (var + EPSILON);
        if let Some(slot) = k.a.get_mut(index) {
            // A slope can be negative - a dark subject against a bright background - so it is
            // written into the buffer directly rather than through `Plane::set`, which clamps.
            // The bound keeps a near-zero variance from producing an enormous slope.
            *slot = slope.clamp(-8.0, 8.0);
        }
        if let Some(slot) = b.a.get_mut(index) {
            *slot = mp - slope * mi;
        }
        if let Some(slot) = informative.a.get_mut(index) {
            *slot = if var >= VARIANCE_FLOOR { 1.0 } else { 0.0 };
        }
    }
    // Smoothed over the same window as the coefficients, so the fallback fades in rather than
    // switching at a pixel - a hard switch between the matte and the coarse mask is a visible
    // step along the boundary, which is the artefact rather than the fix.
    let informative = algebra::box_blur(&informative, radius);
    let mean_k = algebra::box_blur(&k, radius);
    let mean_b = algebra::box_blur(&b, radius);

    let mut out = coarse.clone();
    for y in 0..i64::from(coarse.h) {
        for x in 0..i64::from(coarse.w) {
            match map.at(x, y) {
                Region::Foreground => out.set(x, y, 1.0),
                Region::Background => out.set(x, y, 0.0),
                Region::Unknown => {
                    let index = (y as usize) * (coarse.w as usize) + (x as usize);
                    let i = guide.a.get(index).copied().unwrap_or(0.0);
                    let kv = mean_k.a.get(index).copied().unwrap_or(0.0);
                    let bv = mean_b.a.get(index).copied().unwrap_or(0.0);
                    let matte = (kv * i + bv).clamp(0.0, 1.0);
                    let trust = informative
                        .a
                        .get(index)
                        .copied()
                        .unwrap_or(0.0)
                        .clamp(0.0, 1.0);
                    let fallback = hard.at(x, y);
                    out.set(x, y, matte * trust + fallback * (1.0 - trust));
                }
            }
        }
    }

    let mut solvable = 0.0_f64;
    let mut band = 0.0_f64;
    for y in 0..i64::from(coarse.h) {
        for x in 0..i64::from(coarse.w) {
            if map.at(x, y) != Region::Unknown {
                continue;
            }
            band += 1.0;
            solvable += f64::from(informative.at(x, y).clamp(0.0, 1.0));
        }
    }
    Matte {
        alpha: out,
        trust: if band > 0.0 {
            (solvable / band) as f32
        } else {
            0.0
        },
    }
}

/// How well determined the boundary turned out to be, `0.0 ..= 1.0`.
///
/// Section 6.4: "edge quality estimated from matting uncertainty and gradient agreement".
/// Both halves are here and they are multiplied rather than averaged, because they fail for
/// different reasons and either one failing is enough to make a boundary untrustworthy.
///
/// * **Decisiveness** is how far the solved alphas in the band sit from a half. A band full of
///   0.5 is a matte that could not decide, which is what a backlit veil against a bright wall
///   produces.
/// * **Agreement** is how well the alpha gradient lines up with the photograph's own gradient
///   inside the band. A matte whose edge runs where the picture has no edge is a matte that
///   followed the coarse mask rather than the pixels.
/// * **Trust** is [`Matte::trust`]: how much of the band the guide could be solved in at all.
///   It is the term that makes the other two safe. Without it, a boundary the photograph does
///   not contain scores *highest* of all - the fallback keeps hard zeros and ones, which is
///   maximally decisive - and a dark suit against a dark wall would be reported as the
///   cleanest edge in the wedding.
#[must_use]
pub fn edge_quality(features: &Features, matte: &Matte, map: &Trimap) -> f32 {
    let alpha = &matte.alpha;
    let unknown = map.unknown_count();
    if unknown == 0 {
        // Nothing was uncertain, so nothing was refined. That is a hard boundary rather than a
        // good one, and `EdgeQuality::Binary` is what the caller records.
        return 0.5;
    }

    let mut decisive = 0.0_f64;
    let mut agreement = 0.0_f64;
    let mut agreement_n = 0.0_f64;

    for y in 0..i64::from(alpha.h) {
        for x in 0..i64::from(alpha.w) {
            if map.at(x, y) != Region::Unknown {
                continue;
            }
            let a = alpha.at(x, y);
            decisive += f64::from((a - 0.5).abs() * 2.0);

            let da = (alpha.at(x + 1, y) - a).hypot(alpha.at(x, y + 1) - a);
            if da <= 1e-4 {
                continue;
            }
            let here = features.luma_at(x, y);
            let di = (features.luma_at(x + 1, y) - here).hypot(features.luma_at(x, y + 1) - here);
            let scale = features.median_texture.max(1e-5) * 2.0;
            agreement += f64::from((di / scale).min(1.0));
            agreement_n += 1.0;
        }
    }

    let decisive = (decisive / unknown as f64) as f32;
    let agreement = if agreement_n > 0.0 {
        (agreement / agreement_n) as f32
    } else {
        // No alpha gradient inside the band at all: the matte is flat, which is the
        // "could not decide" case again rather than a clean edge.
        0.0
    };
    let trust = matte.trust.clamp(0.0, 1.0);
    // The geometric mean of three, so any one of them failing takes the result down. Phase 12
    // fused four sub-scores this way and for the same reason: an arithmetic mean lets two good
    // numbers carry a fatal third.
    (decisive.clamp(0.0, 1.0) * agreement.clamp(0.0, 1.0) * trust).cbrt()
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
    use crate::mask::trimap;
    use crate::mask::MaskFrame;

    /// A frame with a hard vertical boundary: dark on the left, bright on the right.
    fn edge_frame(w: u32, h: u32, at: u32) -> MaskFrame {
        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        for _y in 0..h {
            for x in 0..w {
                let v = if x < at { 0.1 } else { 0.8 };
                rgb.extend_from_slice(&[v, v, v]);
            }
        }
        MaskFrame::new(rgb, w, h)
    }

    fn left_half(w: u32, h: u32, at: u32) -> Plane {
        let mut p = Plane::zeros(w, h);
        for y in 0..h {
            for x in 0..at {
                p.set(i64::from(x), i64::from(y), 1.0);
            }
        }
        p
    }

    #[test]
    fn the_head_is_not_consulted() {
        assert!(!MATTING_HEAD_TRAINED);
        let features = Features::measure(&edge_frame(16, 16, 8));
        let map = trimap::build(&left_half(16, 16, 8), 2);
        assert!(alpha_hint(&features, &map).is_none());
    }

    #[test]
    fn matting_never_touches_the_certain_interior() {
        let frame = edge_frame(64, 32, 32);
        let features = Features::measure(&frame);
        let coarse = left_half(64, 32, 32);
        let map = trimap::build(&coarse, 5);
        let out = refine(&features, &coarse, &map).alpha;
        assert_eq!(out.at(2, 16), 1.0, "the interior moved");
        assert_eq!(out.at(61, 16), 0.0, "the exterior moved");
    }

    #[test]
    fn a_matte_over_a_real_boundary_is_decisive() {
        let frame = edge_frame(64, 32, 32);
        let features = Features::measure(&frame);
        let coarse = left_half(64, 32, 32);
        let map = trimap::build(&coarse, 5);
        let out = refine(&features, &coarse, &map).alpha;
        // The solved band tracks the photograph's own step rather than sitting at a half.
        assert!(out.at(28, 16) > 0.6, "{}", out.at(28, 16));
        assert!(out.at(36, 16) < 0.4, "{}", out.at(36, 16));
    }

    #[test]
    fn a_matte_over_a_boundary_that_is_not_in_the_picture_scores_badly() {
        // A flat grey frame with a mask edge down the middle: there is nothing to agree with.
        let mut rgb = Vec::new();
        for _ in 0..(64 * 32) {
            rgb.extend_from_slice(&[0.5, 0.5, 0.5]);
        }
        let features = Features::measure(&MaskFrame::new(rgb, 64, 32));
        let coarse = left_half(64, 32, 32);
        let map = trimap::build(&coarse, 5);
        let out = refine(&features, &coarse, &map);
        let flat = edge_quality(&features, &out, &map);

        let sharp_frame = edge_frame(64, 32, 32);
        let sharp_features = Features::measure(&sharp_frame);
        let sharp = refine(&sharp_features, &coarse, &map);
        let good = edge_quality(&sharp_features, &sharp, &map);

        assert!(good > flat, "flat {flat} was not worse than real {good}");
    }
}
