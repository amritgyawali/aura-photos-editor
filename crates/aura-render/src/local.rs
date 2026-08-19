//! The processor reference for PHASE-19's local light application.
//!
//! `aura-brain-photo` decides *what* to do to the light inside a photograph;
//! `aura_recipe::Mask` carries the decision; this module is what turns the decision into
//! pixels. The three WGSL files - `luminosity_mask.wgsl`, `freq_sep.wgsl` and
//! `local_apply.wgsl` - are the same arithmetic for a device, and
//! `crates/aura-render/tests/shader_parity.rs` holds the two to the same constants.
//!
//! ## Why this exists on a build that cannot run any of it
//!
//! Because the mask this needs is phase 18's and phase 18 has not shipped, so
//! `graph::plan` emits [`crate::contract::render::SkipReason::MaskGeneratorAbsent`] for every
//! generated mask a phase 19 plan writes, and none of these functions is called on a real
//! frame. What ships is the reference: the moment phase 18 supplies a matte, the application
//! is here, it is tested, and it agrees with the shader. The alternative - writing it when the
//! matte arrives - is how a GPU backend turns up a year later against a reference that has
//! moved twice, which is ADR-0029 section 4's own argument applied one phase later.
//!
//! ## Everything here is linear
//!
//! Invariant 8. `crates/aura-render/tests/colour_discipline.rs` is a grep as a test: the only
//! `powf` in this module is inside [`luminosity_weight`], where it produces a *weight* rather
//! than a colour, and the encoded value never leaves the function.

use aura_recipe::MaskParams;

/// The luminance a face is pivoted around, in perceptual units.
///
/// Faces are not mid-grey. A well-exposed face sits between 0.30 and 0.60, so a curve pivoted
/// at 0.5 would treat a correctly lit ceremony face as a shadow and put the whole lift where
/// the highlights are. The same constant as `luminosity_mask.wgsl`'s `FACE_PIVOT` and as
/// `aura_brain_photo::local::luminosity::FACE_PIVOT`; the shader parity test holds the first
/// two together and `tests/eval/local_eval.rs` holds the third.
pub const FACE_PIVOT: f32 = 0.48;

/// How many `shadows` units one stop of shadow-region lift is worth.
pub const SHADOWS_PER_EV: f32 = 60.0;

/// How many `highlights` units of restraint one stop of lift buys. Negative in use.
pub const HIGHLIGHTS_PER_EV: f32 = 18.0;

/// The units the shaping grids are stored in: 1/200 of a stop.
pub const SHAPING_UNIT_EV: f32 = 0.005;

/// The encoding exponent the perceptual weighting assumes.
pub const ENCODING_GAMMA: f32 = 2.2;

/// Rec.2020 luminance of a linear triple.
#[must_use]
pub fn luma(rgb: [f32; 3]) -> f32 {
    0.262_700f32.mul_add(
        rgb[0],
        0.677_998f32.mul_add(rgb[1], 0.059_302 * rgb[2]),
    )
}

/// How much of a lift this pixel should receive, `0..1`.
///
/// One at black, zero at and above the pivot, smooth in between. **The one idea in phase 19**:
/// a flat lift inside a face mask preserves the ratio between the shadow side and the lit
/// side, which is what "glowing" means; weighting the lift by this reduces the ratio, which
/// reads as better lighting rather than as more light.
#[must_use]
pub fn luminosity_weight(rgb: [f32; 3]) -> f32 {
    let encoded = luma(rgb).max(0.0).powf(1.0 / ENCODING_GAMMA);
    let x = ((FACE_PIVOT - encoded) / FACE_PIVOT).clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// The highlight side of the same curve.
///
/// Deliberately **not** `1.0 - luminosity_weight`: the shadow curve reaches zero at the pivot
/// and this one starts there, so between black and the pivot they do not sum to one and the
/// two terms cannot both act on the same pixel at full strength.
#[must_use]
pub fn highlight_weight(rgb: [f32; 3]) -> f32 {
    let encoded = luma(rgb).max(0.0).powf(1.0 / ENCODING_GAMMA);
    let x = ((encoded - FACE_PIVOT) / (1.0 - FACE_PIVOT)).clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Soften a generated mask's alpha by a feather.
#[must_use]
pub fn feathered_alpha(alpha: f32, feather: f32) -> f32 {
    let f = feather.clamp(0.01, 1.0);
    smoothstep(0.5 - f * 0.5, 0.5 + f * 0.5, alpha)
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if (edge1 - edge0).abs() <= f32::EPSILON {
        return f32::from(x >= edge1);
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The face-lighting half of a mask's parameters, applied to one linear pixel.
///
/// The exposure term is flat inside the mask; the shadow term is weighted by the luminosity
/// mask so shadows move and highlights do not; the highlight term is weighted by the opposite
/// curve and is clamped so it can never brighten.
///
/// ## Both weights read the *input* pixel, and that is load-bearing
///
/// The first version evaluated each weight on the partially-edited value - the shadow weight
/// after the exposure had moved, the highlight weight after both. It reads naturally and it is
/// wrong, because the weight then grows with `alpha` at the same time as the term it scales
/// does: the restraint grows quadratically while the lift grows linearly, and past about half
/// alpha the restraint overtakes.
///
/// The symptom is the thing this whole phase exists to avoid. A bright pixel received *more*
/// lift at the mask's edge than at its centre, which is the exact shape of a bright rim just
/// inside the boundary - a halo, produced by arithmetic that looked conservative.
/// `tests/eval/local_eval.rs` caught it, and the fix is that a luminosity mask is a function
/// of the *original* tone, evaluated once, so the whole edit is linear in the matte.
#[must_use]
pub fn apply_face_light(rgb: [f32; 3], params: &MaskParams, alpha: f32) -> [f32; 3] {
    if alpha <= 0.0 {
        return rgb;
    }
    let shadow_share = luminosity_weight(rgb);
    let highlight_share = highlight_weight(rgb);
    let mut ev = params.exposure.unwrap_or(0.0);
    if let Some(shadows) = params.shadows {
        ev += (f32::from(shadows) / SHADOWS_PER_EV) * shadow_share;
    }
    if let Some(highlights) = params.highlights {
        ev += ((f32::from(highlights) / HIGHLIGHTS_PER_EV) * highlight_share).min(0.0);
    }
    scale(rgb, (ev * alpha).exp2())
}

/// The background half: a luminance reduction and a move toward the pixel's own grey.
///
/// The saturation term is a mix rather than a multiply, so a background that is calmed does
/// not shift hue on the way down. Both terms are clamped at zero: this phase calms a
/// background and never enriches one.
#[must_use]
pub fn apply_background(rgb: [f32; 3], params: &MaskParams, alpha: f32) -> [f32; 3] {
    if alpha <= 0.0 {
        return rgb;
    }
    let out = scale(
        rgb,
        (params.exposure.unwrap_or(0.0).min(0.0) * alpha).exp2(),
    );
    let Some(saturation) = params.saturation else {
        return out;
    };
    let grey = luma(out);
    let amount = 1.0 + (f32::from(saturation).min(0.0) / 100.0) * alpha;
    let amount = amount.max(0.0);
    [
        grey + (out[0] - grey) * amount,
        grey + (out[1] - grey) * amount,
        grey + (out[2] - grey) * amount,
    ]
}

/// The shine reduction: luminance only.
///
/// The pixel's chromaticity is held and only its brightness moves, so the texture under the
/// sheen survives. There is no radius, no smoothing strength and no texture parameter in this
/// function or in the type it reads - which is the boundary between this phase and phase 20.
#[must_use]
pub fn apply_shine(rgb: [f32; 3], reduction_ev: f32, alpha: f32) -> [f32; 3] {
    if alpha <= 0.0 || reduction_ev >= 0.0 {
        return rgb;
    }
    let before = luma(rgb);
    if before <= 0.0 {
        return rgb;
    }
    let after = before * (reduction_ev * alpha).exp2();
    scale(rgb, after / before)
}

/// One sample of a shaping grid, applied.
///
/// `units` is the stored 1/200-stop value from `local_light_shaping`'s regenerated grid.
#[must_use]
pub fn apply_shaping(rgb: [f32; 3], low_units: i8, mid_units: i8, alpha: f32) -> [f32; 3] {
    if alpha <= 0.0 {
        return rgb;
    }
    let ev = (f32::from(low_units) + f32::from(mid_units)) * SHAPING_UNIT_EV * alpha;
    scale(rgb, ev.exp2())
}

fn scale(rgb: [f32; 3], factor: f32) -> [f32; 3] {
    [
        (rgb[0] * factor).max(0.0),
        (rgb[1] * factor).max(0.0),
        (rgb[2] * factor).max(0.0),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grey(linear: f32) -> [f32; 3] {
        [linear, linear, linear]
    }

    #[test]
    fn the_luminosity_curve_lifts_shadows_and_leaves_highlights() {
        let shadow = luminosity_weight(grey(0.01));
        let mid = luminosity_weight(grey(0.18));
        let highlight = luminosity_weight(grey(0.75));
        assert!(shadow > mid, "{shadow} then {mid}");
        assert!(mid > highlight, "{mid} then {highlight}");
        assert!(highlight.abs() < f32::EPSILON, "a highlight was lifted");
    }

    #[test]
    fn the_two_curves_do_not_both_act_at_full_strength() {
        for linear in [0.005f32, 0.02, 0.05, 0.18, 0.4, 0.8] {
            let sum = luminosity_weight(grey(linear)) + highlight_weight(grey(linear));
            assert!(sum <= 1.0 + 1e-5, "the curves overlap at {linear}: {sum}");
        }
    }

    #[test]
    fn a_shadow_lift_moves_a_dark_pixel_more_than_a_bright_one() {
        let params = MaskParams {
            exposure: Some(0.0),
            shadows: Some(60),
            ..MaskParams::default()
        };
        let dark = apply_face_light(grey(0.02), &params, 1.0);
        let bright = apply_face_light(grey(0.60), &params, 1.0);
        let dark_ratio = dark[0] / 0.02;
        let bright_ratio = bright[0] / 0.60;
        assert!(
            dark_ratio > bright_ratio * 1.5,
            "the lift was flat: {dark_ratio} against {bright_ratio}"
        );
    }

    #[test]
    fn the_edit_is_linear_in_the_matte() {
        // The property that makes a rim impossible, and the one a partially-edited weight
        // broke. However bright the pixel, the edit must grow with the matte and never peak
        // part-way through the falloff.
        let params = MaskParams {
            exposure: Some(0.45),
            shadows: Some(40),
            highlights: Some(-12),
            ..MaskParams::default()
        };
        for linear in [0.02f32, 0.08, 0.18, 0.42, 0.80] {
            let pixel = grey(linear);
            let full = (luma(apply_face_light(pixel, &params, 1.0)) - luma(pixel)).abs();
            let mut previous = 0.0f32;
            for step in 0..=100 {
                let alpha = step as f32 / 100.0;
                let edit = (luma(apply_face_light(pixel, &params, alpha)) - luma(pixel)).abs();
                assert!(
                    edit <= full + 1e-5,
                    "at {linear} the edit peaked at alpha {alpha}: {edit} against {full}"
                );
                assert!(edit >= previous - 1e-5, "the edit weakened at alpha {alpha}");
                previous = edit;
            }
        }
    }

    #[test]
    fn the_highlight_term_can_never_brighten() {
        let params = MaskParams {
            highlights: Some(-40),
            ..MaskParams::default()
        };
        for linear in [0.01f32, 0.18, 0.5, 0.9] {
            let out = apply_face_light(grey(linear), &params, 1.0);
            assert!(out[0] <= linear + 1e-6, "a restraint brightened {linear}");
        }
    }

    #[test]
    fn a_zero_alpha_changes_nothing() {
        let params = MaskParams {
            exposure: Some(1.0),
            shadows: Some(80),
            ..MaskParams::default()
        };
        let same = |out: [f32; 3]| {
            assert!(
                out.iter().all(|value| (value - 0.2).abs() < 1e-6),
                "a zero-alpha pixel moved to {out:?}"
            );
        };
        same(apply_face_light(grey(0.2), &params, 0.0));
        same(apply_background(grey(0.2), &params, 0.0));
        same(apply_shine(grey(0.2), -0.5, 0.0));
        same(apply_shaping(grey(0.2), 30, 0, 0.0));
    }

    #[test]
    fn a_background_saturation_reduction_does_not_shift_hue() {
        let params = MaskParams {
            saturation: Some(-50),
            ..MaskParams::default()
        };
        let before = [0.30f32, 0.10, 0.05];
        let after = apply_background(before, &params, 1.0);
        // The move is toward the pixel's own grey, so the ordering of the channels and the
        // sign of every difference from grey survive.
        let grey_before = luma(before);
        let grey_after = luma(after);
        for channel in 0..3 {
            let a = before[channel] - grey_before;
            let b = after[channel] - grey_after;
            assert!(a.signum() == b.signum() || b.abs() < 1e-6, "channel {channel}");
            assert!(b.abs() <= a.abs() + 1e-6, "channel {channel} got more saturated");
        }
    }

    #[test]
    fn a_background_may_never_be_enriched() {
        let params = MaskParams {
            exposure: Some(2.0),
            saturation: Some(80),
            ..MaskParams::default()
        };
        let out = apply_background(grey(0.2), &params, 1.0);
        assert!(out[0] <= 0.2 + 1e-6, "a background was brightened");
    }

    #[test]
    fn shine_reduction_holds_the_chromaticity() {
        let before = [0.90f32, 0.80, 0.70];
        let after = apply_shine(before, -0.5, 1.0);
        let ratio_before = before[0] / before[2];
        let ratio_after = after[0] / after[2];
        assert!((ratio_before - ratio_after).abs() < 1e-5, "the hue moved");
        assert!(luma(after) < luma(before));
    }

    #[test]
    fn shaping_is_bounded_by_the_unit_it_is_stored_in() {
        // A sixth of a stop is 33 units. Even the full i8 range must stay small enough that a
        // shaping map cannot be mistaken for an exposure adjustment.
        let out = apply_shaping(grey(0.18), 127, 0, 1.0);
        assert!(out[0] < 0.18 * 2.0f32.powf(0.64), "127 units is more than 0.64 EV");
    }
}
