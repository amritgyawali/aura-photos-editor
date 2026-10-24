//! Executes `aura_recipe::reference_grade` (ADR-0160).
//!
//! The transform is defined on display colour, where the edited reference was measured, so the
//! stage carries each working-space pixel into linear display sRGB with the output transform's
//! own shoulder and matrix, applies the transform in OKLab, and returns through their inverse.
//! That is a change of coordinates applied and undone inside one stage, as
//! `tonemap::curve_domain_encode` is: no transfer function is encoded here and the buffer
//! leaves the stage scene-referred. A pixel the transform does not move comes back untouched,
//! and scene light above the display range is carried through unless the transform darkens
//! that pixel.
//!
//! [`Evaluator`] is public because the fitter in `aura-app` predicts the result with the same
//! arithmetic it is rendered with. Two copies of a colour transform are two answers to what a
//! matched photograph looks like.
use aura_raw::colour::curve::{filmic_lite, filmic_lite_inverse, monotone_tangents, pchip};
use aura_raw::colour::working_space::linear_srgb_to_working_f32;
use aura_recipe::reference_grade::{
    ReferenceGrade, AXIS_NODES, CHROMA_NODES, CHROMA_SPAN, SECTORS, TONE_NODES, ZONES,
};
use aura_recipe::Recipe;
use rayon::prelude::*;

use crate::contract::render::OutputColour;

/// Samples in each one-dimensional lookup table built from the stored curves.
const LUT: usize = 1024;
/// Sector spacing in radians.
const SECTOR_WIDTH: f32 = std::f32::consts::TAU / SECTORS as f32;

/// Linear sRGB to OKLab (Ottosson 2020).
#[must_use]
pub fn oklab_from_linear_srgb(rgb: [f32; 3]) -> [f32; 3] {
    let [r, g, b] = rgb;
    let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

/// OKLab to linear sRGB, unclamped.
#[must_use]
pub fn linear_srgb_from_oklab(lab: [f32; 3]) -> [f32; 3] {
    let [lightness, a, b] = lab;
    let l = lightness + 0.396_337_78 * a + 0.215_803_76 * b;
    let m = lightness - 0.105_561_346 * a - 0.063_854_17 * b;
    let s = lightness - 0.089_484_18 * a - 1.291_485_5 * b;
    let (l, m, s) = (l * l * l, m * m * m, s * s * s);
    [
        4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s,
        -1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s,
        -0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s,
    ]
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A plateau with smooth shoulders: one inside `[lo, hi]`, zero beyond `soft` outside it.
fn window(x: f32, lo: f32, hi: f32, soft: f32) -> f32 {
    smoothstep(lo - soft, lo, x) * (1.0 - smoothstep(hi, hi + soft, x))
}

fn table(nodes: &[f32], span: f32) -> Vec<f32> {
    let n = nodes.len().max(2);
    let xs: Vec<f32> = (0..n).map(|i| span * i as f32 / (n - 1) as f32).collect();
    let tangents = monotone_tangents(&xs, nodes);
    (0..=LUT)
        .map(|i| pchip(&xs, nodes, &tangents, span * i as f32 / LUT as f32))
        .collect()
}

fn lookup(table: &[f32], position: f32) -> f32 {
    let x = position.clamp(0.0, 1.0) * LUT as f32;
    let i = (x as usize).min(LUT - 1);
    let t = x - i as f32;
    let a = table.get(i).copied().unwrap_or(0.0);
    let b = table.get(i + 1).copied().unwrap_or(a);
    a + (b - a) * t
}

fn axis(nodes: &[[f32; 2]], lightness: f32) -> [f32; 2] {
    let x = lightness.clamp(0.0, 1.0) * (AXIS_NODES - 1) as f32;
    let i = (x as usize).min(AXIS_NODES - 2);
    let t = x - i as f32;
    let a = nodes.get(i).copied().unwrap_or([0.0; 2]);
    let b = nodes.get(i + 1).copied().unwrap_or(a);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

fn zone(nodes: &[f32], lightness: f32) -> f32 {
    let x = lightness.clamp(0.0, 1.0) * (ZONES - 1) as f32;
    let i = (x as usize).min(ZONES - 2);
    let t = x - i as f32;
    let a = nodes.get(i).copied().unwrap_or(1.0);
    let b = nodes.get(i + 1).copied().unwrap_or(a);
    a + (b - a) * t
}

/// Circular linear interpolation between sector centres.
fn sector(nodes: &[f32], hue: f32) -> f32 {
    let p = (hue / SECTOR_WIDTH).rem_euclid(SECTORS as f32);
    let i = (p as usize).min(SECTORS - 1);
    let t = p - i as f32;
    let a = nodes.get(i).copied().unwrap_or(0.0);
    let b = nodes.get((i + 1) % SECTORS).copied().unwrap_or(a);
    a + (b - a) * t
}

/// How much of a pixel is in the colour range of human skin, before the protection strength.
///
/// Measured OKLab hues of skin from pale pink to deep brown fall between about 30 and 70
/// degrees at chroma 0.03 to 0.13. Wood, sand and an orange dress share the range, so the
/// protection is a limit on how far a hue may turn rather than a refusal to change it.
#[must_use]
pub fn skin_likeness(lab: [f32; 3]) -> f32 {
    let chroma = lab[1].hypot(lab[2]);
    let hue = lab[2].atan2(lab[1]).to_degrees();
    window(hue, 28.0, 72.0, 15.0)
        * window(chroma, 0.03, 0.14, 0.02)
        * window(lab[0], 0.3, 0.92, 0.12)
}

#[derive(Debug, Clone)]
pub struct Evaluator {
    tone: Vec<f32>,
    chroma: Vec<f32>,
    chroma_slope: f32,
    cast_in: Vec<[f32; 2]>,
    cast_out: Vec<[f32; 2]>,
    gain: Vec<f32>,
    shift: Vec<f32>,
    light: Vec<f32>,
    zone: Vec<f32>,
    skin: f32,
    skin_fidelity: f32,
}

impl Evaluator {
    #[must_use]
    pub fn new(grade: &ReferenceGrade) -> Self {
        let chroma = table(&grade.chroma, CHROMA_SPAN);
        let last = grade.chroma.len().saturating_sub(1);
        let step = CHROMA_SPAN / (CHROMA_NODES - 1) as f32;
        let chroma_slope = match (
            grade.chroma.get(last.saturating_sub(1)),
            grade.chroma.get(last),
        ) {
            (Some(a), Some(b)) => ((b - a) / step).max(0.0),
            _ => 1.0,
        };
        debug_assert_eq!(grade.tone.len(), TONE_NODES);
        Self {
            tone: table(&grade.tone, 1.0),
            chroma,
            chroma_slope,
            cast_in: grade.cast_in.clone(),
            cast_out: grade.cast_out.clone(),
            gain: grade.hue_gain.clone(),
            shift: grade.hue_shift.clone(),
            light: grade.hue_light.clone(),
            zone: grade.zone_gain.clone(),
            skin: grade.skin,
            skin_fidelity: grade.skin_fidelity,
        }
    }

    fn chroma_curve(&self, chroma: f32) -> f32 {
        if chroma <= CHROMA_SPAN {
            lookup(&self.chroma, chroma / CHROMA_SPAN)
        } else {
            self.chroma.last().copied().unwrap_or(CHROMA_SPAN)
                + (chroma - CHROMA_SPAN) * self.chroma_slope
        }
    }

    /// The transform in OKLab, before gamut mapping.
    #[must_use]
    pub fn oklab(&self, lab: [f32; 3]) -> [f32; 3] {
        let [lightness, a, b] = lab;
        let lightness = lightness.clamp(0.0, 1.0);
        let source = axis(&self.cast_in, lightness);
        let (ra, rb) = (a - source[0], b - source[1]);
        let chroma = ra.hypot(rb);
        let hue = rb.atan2(ra);
        let coloured = smoothstep(0.012, 0.05, chroma);
        let protect = self.skin * skin_likeness(lab);

        let mut out_lightness = lookup(&self.tone, lightness);
        let mut ratio = if chroma > 1e-6 {
            self.chroma_curve(chroma) / chroma
        } else {
            self.chroma_curve(1e-6) / 1e-6
        };
        ratio *= 1.0 + coloured * (sector(&self.gain, hue) - 1.0);
        ratio *= zone(&self.zone, out_lightness);
        let mut turn = coloured * sector(&self.shift, hue);
        let mut lift =
            coloured * sector(&self.light, hue) * 4.0 * out_lightness * (1.0 - out_lightness);
        if protect > 0.0 {
            ratio += protect * (ratio.clamp(0.55, 1.25) - ratio);
            turn += protect * (turn.clamp(-0.07, 0.07) - turn);
            lift += protect * (lift.clamp(-0.03, 0.03) - lift);
        }
        out_lightness = (out_lightness + lift).clamp(0.0, 1.0);
        let (sin, cos) = (hue + turn).sin_cos();
        let relative = [chroma * ratio * cos, chroma * ratio * sin];

        // The neutral axis moves from where this photograph had it to where the reference had
        // it. On skin, the part of that move across the skin's own hue is what turns a face
        // green or grey; the part along it is warmth, and stays.
        let target = axis(&self.cast_out, out_lightness);
        let mut cast = [target[0] - source[0], target[1] - source[1]];
        let absolute = a.hypot(b);
        if protect > 0.0 && absolute > 0.01 {
            let along = (cast[0] * a + cast[1] * b) / absolute;
            let across = [
                cast[0] - along * a / absolute,
                cast[1] - along * b / absolute,
            ];
            cast[0] -= protect * 0.8 * across[0];
            cast[1] -= protect * 0.8 * across[1];
        }
        let mut result = [
            out_lightness,
            relative[0] + source[0] + cast[0],
            relative[1] + source[1] + cast[1],
        ];
        // Keep this weight independent of lightness. Fading a displacement cap across a
        // narrow lightness interval can reverse an otherwise monotone tone gradient.
        let fidelity = self.skin_fidelity * skin_likeness([0.6, a, b]);
        if fidelity > 0.0 {
            // Bound the joint result, including neutral-axis and tone changes. Limiting
            // hue alone cannot prevent a strong palette fit from recolouring a face.
            let delta = [result[1] - a, result[2] - b];
            let scale = (0.025 / delta[0].hypot(delta[1]).max(1e-6)).min(1.0);
            let protected = [
                lightness + (result[0] - lightness).clamp(-0.08, 0.08),
                a + delta[0] * scale,
                b + delta[1] * scale,
            ];
            for (value, safe) in result.iter_mut().zip(protected) {
                *value += fidelity * (safe - *value);
            }
        }
        result
    }

    /// The whole display transform: linear sRGB in `0..=1` to linear sRGB in `0..=1`.
    ///
    /// A colour the transform pushes outside the display gamut keeps its lightness and hue and
    /// loses only the chroma the display cannot show, so a stronger saturation never clips one
    /// channel and turns a hue.
    #[must_use]
    pub fn linear_srgb(&self, rgb: [f32; 3]) -> [f32; 3] {
        let lab = oklab_from_linear_srgb(rgb.map(|v| v.clamp(0.0, 1.0)));
        let [lightness, a, b] = self.oklab(lab);
        let inside = |c: [f32; 3]| c.iter().all(|v| (-1e-4..=1.0 + 1e-4).contains(v));
        let mut out = linear_srgb_from_oklab([lightness, a, b]);
        if !inside(out) {
            let (mut lo, mut hi) = (0.0_f32, 1.0_f32);
            for _ in 0..12 {
                let mid = 0.5 * (lo + hi);
                if inside(linear_srgb_from_oklab([lightness, a * mid, b * mid])) {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            out = linear_srgb_from_oklab([lightness, a * lo, b * lo]);
        }
        out.map(|v| v.clamp(0.0, 1.0))
    }
}

fn luminance(rgb: [f32; 3]) -> f32 {
    0.212_6 * rgb[0] + 0.715_2 * rgb[1] + 0.072_2 * rgb[2]
}

/// The stage. A malformed or absent transform leaves the buffer alone, as every recipe
/// extension stage does; validation refused it when it was written.
pub(crate) fn apply(rgb: &mut [f32], recipe: &Recipe) {
    let Ok(Some(grade)) = aura_recipe::reference_grade::read(recipe) else {
        return;
    };
    if grade.is_identity() {
        return;
    }
    let evaluator = Evaluator::new(&grade);
    map_display(rgb, |display| evaluator.linear_srgb(display));
}

/// Execute a display-linear colour mapping and return to the original working domain.
/// Shared by fitted reference transforms so preview and export use one shoulder round trip.
pub(super) fn map_display(rgb: &mut [f32], mapping: impl Fn([f32; 3]) -> [f32; 3] + Sync) {
    let to_display = crate::output::working_to_output(OutputColour::Srgb);
    let ceiling = filmic_lite(crate::output::DISPLAY_PEAK);
    let back = |linear: [f32; 3]| {
        linear_srgb_to_working_f32(linear).map(|v| filmic_lite_inverse(v.max(0.0).min(ceiling)))
    };
    rgb.par_chunks_mut(3).for_each(|pixel| {
        let [r, g, b] = [
            pixel.first().copied().unwrap_or(0.0),
            pixel.get(1).copied().unwrap_or(0.0),
            pixel.get(2).copied().unwrap_or(0.0),
        ];
        let rolled = [r, g, b].map(|v| filmic_lite(v.max(0.0)));
        let display = crate::colour::apply_f32(to_display, rolled).map(|v| v.clamp(0.0, 1.0));
        let graded = mapping(display);
        if graded
            .iter()
            .zip(display)
            .all(|(x, y)| (x - y).abs() <= 1e-7)
        {
            return;
        }
        let base = back(display);
        let moved = back(graded);
        // Light beyond what the display shows is kept unless the transform darkens the pixel,
        // so a matte highlight is matte and an unchanged one keeps its headroom.
        let darkened = (luminance(display) - luminance(graded)).max(0.0);
        let keep = (1.0 - 20.0 * darkened).clamp(0.0, 1.0);
        for (slot, (m, (o, base))) in pixel
            .iter_mut()
            .zip(moved.iter().zip([r, g, b].iter().zip(base)))
        {
            *slot = m + keep * (o - base);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_recipe::reference_grade::{self as model, write};

    fn grade() -> ReferenceGrade {
        let mut grade = ReferenceGrade::identity();
        for (i, v) in grade.tone.iter_mut().enumerate() {
            let x = i as f32 / (TONE_NODES - 1) as f32;
            *v = 0.08 + 0.84 * x;
        }
        grade.cast_out = vec![[-0.02, -0.03]; AXIS_NODES];
        grade.cast_out[AXIS_NODES - 1] = [0.01, 0.03];
        grade.chroma = grade.chroma.iter().map(|v| v * 1.3).collect();
        grade.hue_shift[4] = 0.3;
        grade
    }

    #[test]
    fn oklab_round_trips_and_white_is_neutral() {
        for rgb in [
            [1.0, 1.0, 1.0],
            [0.2, 0.5, 0.9],
            [0.8, 0.1, 0.05],
            [0.0, 0.0, 0.0],
        ] {
            let back = linear_srgb_from_oklab(oklab_from_linear_srgb(rgb));
            for (a, b) in rgb.iter().zip(back) {
                assert!((a - b).abs() < 1e-4, "{rgb:?} -> {back:?}");
            }
        }
        let white = oklab_from_linear_srgb([1.0; 3]);
        assert!((white[0] - 1.0).abs() < 1e-3 && white[1].abs() < 1e-3 && white[2].abs() < 1e-3);
    }

    #[test]
    fn palette_skin_fidelity_bounds_joint_tone_and_colour_without_dimming_other_colours() {
        let mut grade = grade();
        grade.cast_out.fill([-0.06, 0.06]);
        grade.tone = grade.tone.iter().map(|v| (v + 0.2).min(1.0)).collect();
        let unprotected = Evaluator::new(&grade);
        grade.skin_fidelity = 1.0;
        let protected = Evaluator::new(&grade);
        let skin = [0.65, 0.06, 0.06];
        assert!((skin_likeness(skin) - 1.0).abs() < 1e-5);
        let output = protected.oklab(skin);
        assert!((output[0] - skin[0]).abs() <= 0.08001);
        assert!((output[1] - skin[1]).hypot(output[2] - skin[2]) <= 0.02501);
        let blue = [0.6, 0.01, -0.12];
        assert_eq!(
            unprotected.oklab(blue).map(f32::to_bits),
            protected.oklab(blue).map(f32::to_bits)
        );
    }

    #[test]
    fn palette_skin_fidelity_preserves_a_monotone_fixed_colour_tone_ramp() {
        let mut grade = ReferenceGrade::identity();
        grade
            .tone
            .iter_mut()
            .for_each(|v| *v = (*v + 0.25).min(1.0));
        grade.skin_fidelity = 1.0;
        let evaluator = Evaluator::new(&grade);
        let mut previous = 0.0;
        for step in 0..=1024 {
            let lightness = step as f32 / 1024.0;
            let result = evaluator.oklab([lightness, 0.06, 0.06]);
            assert!(result[0] + 1e-6 >= previous, "tone reversal at {lightness}");
            previous = result[0];
        }
    }

    #[test]
    fn the_identity_moves_no_colour() {
        let evaluator = Evaluator::new(&ReferenceGrade::identity());
        for rgb in [
            [0.3, 0.2, 0.1],
            [0.9, 0.9, 0.95],
            [0.05, 0.4, 0.2],
            [0.0, 0.0, 1.0],
        ] {
            let out = evaluator.linear_srgb(rgb);
            for (a, b) in rgb.iter().zip(out) {
                assert!((a - b).abs() < 2e-4, "{rgb:?} -> {out:?}");
            }
        }
    }

    #[test]
    fn the_stage_leaves_an_absent_or_identity_transform_bit_identical() {
        let mut recipe = aura_recipe::fixtures::reference();
        let mut pixels = vec![0.2, 0.3, 0.4, 5.0, 4.0, 3.0, -0.01, 0.5, 0.1];
        let original = pixels.clone();
        apply(&mut pixels, &recipe);
        assert_eq!(pixels, original);
        write(&mut recipe, &ReferenceGrade::identity()).unwrap();
        apply(&mut pixels, &recipe);
        assert_eq!(pixels, original);
    }

    #[test]
    fn the_stage_renders_exactly_what_the_evaluator_predicts() {
        let mut recipe = aura_recipe::fixtures::reference();
        write(&mut recipe, &grade()).unwrap();
        let evaluator = Evaluator::new(&grade());
        let to_display = crate::output::working_to_output(OutputColour::Srgb);
        // Colours inside the display gamut and below the shoulder, so the round trip is exact.
        let mut pixels = Vec::new();
        for i in 0..64 {
            let t = i as f32 / 63.0;
            pixels.extend_from_slice(&linear_srgb_to_working_f32([
                0.05 + 0.5 * t,
                0.3 * (1.0 - t) + 0.02,
                0.2 + 0.1 * t,
            ]));
        }
        let display: Vec<[f32; 3]> = pixels
            .chunks_exact(3)
            .map(|p| {
                crate::colour::apply_f32(to_display, [p[0], p[1], p[2]].map(filmic_lite))
                    .map(|v| v.clamp(0.0, 1.0))
            })
            .collect();
        apply(&mut pixels, &recipe);
        for (p, before) in pixels.chunks_exact(3).zip(display) {
            let rendered =
                crate::colour::apply_f32(to_display, [p[0], p[1], p[2]].map(filmic_lite));
            let predicted = evaluator.linear_srgb(before);
            for (a, b) in rendered.iter().zip(predicted) {
                assert!((a - b).abs() < 2e-4, "{rendered:?} vs {predicted:?}");
            }
        }
    }

    #[test]
    fn a_matte_white_point_reaches_highlights_above_the_display_range() {
        let mut recipe = aura_recipe::fixtures::reference();
        write(&mut recipe, &grade()).unwrap();
        let mut pixels = vec![6.0, 6.0, 6.0];
        apply(&mut pixels, &recipe);
        let to_display = crate::output::working_to_output(OutputColour::Srgb);
        let shown = crate::colour::apply_f32(
            to_display,
            [pixels[0], pixels[1], pixels[2]].map(filmic_lite),
        );
        assert!(shown.iter().all(|v| *v < 0.97), "{shown:?}");
    }

    #[test]
    fn saturation_beyond_the_gamut_keeps_the_hue_instead_of_clipping_a_channel() {
        let mut strong = ReferenceGrade::identity();
        strong.chroma = strong.chroma.iter().map(|v| v * 2.5).collect();
        let evaluator = Evaluator::new(&strong);
        let input = [0.6, 0.15, 0.05];
        let out = evaluator.linear_srgb(input);
        assert!(out.iter().all(|v| (0.0..=1.0).contains(v)));
        let hue = |rgb: [f32; 3]| {
            let lab = oklab_from_linear_srgb(rgb);
            lab[2].atan2(lab[1])
        };
        assert!((hue(input) - hue(out)).abs() < 0.03, "{input:?} -> {out:?}");
    }

    #[test]
    fn skin_protection_limits_the_turn_and_the_cross_cast_on_skin_only() {
        let mut turned = ReferenceGrade::identity();
        turned.hue_shift = vec![0.5; model::SECTORS];
        turned.cast_out = vec![[-0.04, 0.0]; AXIS_NODES];
        let skin = oklab_from_linear_srgb([0.745, 0.413, 0.262]);
        let sky = oklab_from_linear_srgb([0.2, 0.4, 0.8]);
        let hue_change = |e: &Evaluator, lab: [f32; 3]| {
            let out = e.oklab(lab);
            (out[2].atan2(out[1]) - lab[2].atan2(lab[1])).abs()
        };
        let open = Evaluator::new(&turned);
        turned.skin = 1.0;
        let guarded = Evaluator::new(&turned);
        assert!(skin_likeness(skin) > 0.9 && skin_likeness(sky) < 0.01);
        assert!(hue_change(&guarded, skin) < 0.5 * hue_change(&open, skin));
        assert!((hue_change(&guarded, sky) - hue_change(&open, sky)).abs() < 1e-6);
    }
}
