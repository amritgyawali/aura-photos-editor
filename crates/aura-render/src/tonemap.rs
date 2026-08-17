//! The creative stages: exposure, tone, curve, HSL, local contrast and saturation.
//!
//! Every function here takes and returns **linear working-space** values. Nothing bakes a
//! display transfer function; `output` is the only module that does, and
//! `only_output_encodes_a_transfer_function` is that rule as a test.
//!
//! # The curve domain, which is not an output transform
//!
//! A point curve drawn on a histogram is drawn in a perceptual domain: dragging the middle
//! of the curve up brightens the mid-tones, and in linear light "the middle" is at 0.18
//! rather than at 0.5. So [`curve_domain_encode`] maps linear light into a perceptual
//! domain, the curve is applied there, and [`curve_domain_decode`] maps back. The pair is
//! **invertible and applied in both directions in the same stage**, so nothing is baked: a
//! curve that changes nothing returns the input to within `f32` rounding, and
//! `the_identity_curve_is_the_identity` asserts it.
//!
//! This is the distinction ADR-0029 section 2 draws. An output transform is one-way and
//! terminal; a domain mapping is a change of coordinates.
//!
//! # Why tone stages move luminance rather than channels
//!
//! Lifting R, G and B independently desaturates as it lifts. A wedding rendered that way has
//! grey shadows under a red mandap and grey highlights in a gold sari. Every tone stage
//! computes a target luminance and calls `colour::set_luma`, which holds the pixel's colour
//! ratio exactly.

use std::collections::BTreeMap;

use aura_recipe::{Bw, Curve, HslShift, HSL_BANDS};

use crate::colour::{luma, set_luma};

/// Middle grey in linear light. The pivot every contrast and tone stage turns about.
pub const MID_GREY: f32 = 0.18;

/// The exponent of the curve domain. 2.2 rather than the sRGB piecewise curve because the
/// domain map has to be cheap, exactly invertible in `f32`, and defined for values above 1.
pub const CURVE_GAMMA: f32 = 2.2;

/// Linear light into the perceptual domain the point curve is drawn in.
///
/// See the module documentation: this is a change of coordinates, not an output transform.
#[must_use]
pub fn curve_domain_encode(linear: f32) -> f32 {
    if linear <= 0.0 {
        0.0
    } else {
        linear.powf(1.0 / CURVE_GAMMA)
    }
}

/// The inverse of [`curve_domain_encode`].
#[must_use]
pub fn curve_domain_decode(encoded: f32) -> f32 {
    if encoded <= 0.0 {
        0.0
    } else {
        encoded.powf(CURVE_GAMMA)
    }
}

/// Multiply by `2^stops`.
#[must_use]
pub fn exposure(rgb: [f32; 3], stops: f32) -> [f32; 3] {
    if stops == 0.0 {
        return rgb;
    }
    let gain = stops.exp2();
    [
        rgb.first().copied().unwrap_or(0.0) * gain,
        rgb.get(1).copied().unwrap_or(0.0) * gain,
        rgb.get(2).copied().unwrap_or(0.0) * gain,
    ]
}

/// The four tone sliders, as one pass.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Tone {
    /// `-1.0 ..= 1.0`, from `recipe.global.highlights / 100`.
    pub highlights: f32,
    /// `-1.0 ..= 1.0`.
    pub shadows: f32,
    /// `-1.0 ..= 1.0`.
    pub whites: f32,
    /// `-1.0 ..= 1.0`.
    pub blacks: f32,
}

impl Tone {
    /// True when this changes nothing.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.highlights == 0.0 && self.shadows == 0.0 && self.whites == 0.0 && self.blacks == 0.0
    }
}

/// Apply the four tone sliders to one pixel.
///
/// Each slider owns a band of the tone range, expressed as a smooth weight over the
/// perceptual domain so that the bands overlap the way they do in an editor rather than
/// meeting at a hard edge. The gains are applied to luminance and multiply, so two sliders
/// pulling in opposite directions cancel rather than fight.
///
/// The band centres - 0.82 for whites, 0.62 for highlights, 0.33 for shadows, 0.12 for
/// blacks - are the same ones every raw converter uses, and the widths are chosen so that
/// adjacent weights sum to roughly one across the range.
#[must_use]
pub fn tone(rgb: [f32; 3], t: Tone) -> [f32; 3] {
    if t.is_identity() {
        return rgb;
    }
    let l = luma(rgb);
    if l <= 0.0 {
        return rgb;
    }
    let e = curve_domain_encode(l).clamp(0.0, 2.0);

    let w_whites = band(e, 0.82, 0.30);
    let w_high = band(e, 0.62, 0.26);
    let w_shadow = band(e, 0.33, 0.26);
    let w_black = band(e, 0.12, 0.22);

    // A slider at full travel is at most a 1.6x change in its own band, which is what makes
    // four sliders composable without any of them being able to crush the image on its own.
    let gain = (1.0 + 0.6 * t.whites * w_whites)
        * (1.0 + 0.6 * t.highlights * w_high)
        * (1.0 + 0.6 * t.shadows * w_shadow)
        * (1.0 + 0.6 * t.blacks * w_black);

    set_luma(rgb, (l * gain).max(0.0))
}

/// A smooth, bounded weight centred on `centre` with half-width `width`.
fn band(x: f32, centre: f32, width: f32) -> f32 {
    let d = ((x - centre) / width).abs();
    if d >= 1.0 {
        0.0
    } else {
        let t = 1.0 - d;
        t * t * (3.0 - 2.0 * t) / 1.0
    }
}

/// An S-curve about middle grey, in the perceptual domain.
///
/// `amount` is `-1.0 ..= 1.0`. Positive steepens, negative flattens; the pivot is
/// [`MID_GREY`], so contrast never changes the exposure of a mid-tone.
#[must_use]
pub fn contrast(rgb: [f32; 3], amount: f32) -> [f32; 3] {
    if amount == 0.0 {
        return rgb;
    }
    let l = luma(rgb);
    if l <= 0.0 {
        return rgb;
    }
    let pivot = curve_domain_encode(MID_GREY);
    let e = curve_domain_encode(l);
    // A power about the pivot: slope > 1 steepens, < 1 flattens, and the pivot is fixed.
    let slope = if amount >= 0.0 {
        1.0 + amount
    } else {
        1.0 / (1.0 - amount)
    };
    let shifted = pivot * ((e / pivot.max(1e-6)).max(0.0)).powf(slope);
    set_luma(rgb, curve_domain_decode(shifted))
}

/// A 1,024-entry lookup over the curve domain, built from the recipe's control points.
#[derive(Debug, Clone)]
pub struct CurveLut {
    table: Vec<f32>,
    identity: bool,
}

impl CurveLut {
    /// Number of entries. 1,024 rather than 256 because the curve is applied to luminance
    /// in a domain that runs past 1.0, and a 256-entry table quantises visibly in the
    /// deepest shadows where a curve is most often used.
    pub const ENTRIES: usize = 1024;

    /// Build a monotone interpolation through the recipe's points.
    ///
    /// Fritsch-Carlson, which is the interpolation every raw converter uses for a tone
    /// curve, because it is the one that cannot overshoot: a cubic spline through four
    /// user-placed points can dip below its own control points and produce a band of
    /// inverted contrast that looks exactly like a bug in the renderer.
    #[must_use]
    pub fn build(curve: &Curve) -> Self {
        let identity = curve.is_identity();
        let mut table = vec![0.0f32; Self::ENTRIES];

        let xs: Vec<f32> = curve
            .points
            .iter()
            .map(|p| f32::from(p[0]) / 255.0)
            .collect();
        let ys: Vec<f32> = curve
            .points
            .iter()
            .map(|p| f32::from(p[1]) / 255.0)
            .collect();
        let n = xs.len();
        if n < 2 {
            for (i, slot) in table.iter_mut().enumerate() {
                *slot = i as f32 / (Self::ENTRIES - 1) as f32;
            }
            return Self { table, identity };
        }

        // Secant slopes, then the Fritsch-Carlson limiter.
        let mut secant = vec![0.0f32; n.saturating_sub(1)];
        for i in 0..n - 1 {
            let dx = xs.get(i + 1).copied().unwrap_or(1.0) - xs.get(i).copied().unwrap_or(0.0);
            let dy = ys.get(i + 1).copied().unwrap_or(1.0) - ys.get(i).copied().unwrap_or(0.0);
            if let Some(slot) = secant.get_mut(i) {
                *slot = if dx.abs() < 1e-9 { 0.0 } else { dy / dx };
            }
        }
        let mut tangent = vec![0.0f32; n];
        if let Some(first) = secant.first().copied() {
            if let Some(slot) = tangent.first_mut() {
                *slot = first;
            }
        }
        if let Some(last) = secant.last().copied() {
            if let Some(slot) = tangent.last_mut() {
                *slot = last;
            }
        }
        for i in 1..n - 1 {
            let a = secant.get(i - 1).copied().unwrap_or(0.0);
            let b = secant.get(i).copied().unwrap_or(0.0);
            if let Some(slot) = tangent.get_mut(i) {
                *slot = if a * b <= 0.0 {
                    0.0
                } else {
                    f32::midpoint(a, b)
                };
            }
        }
        for i in 0..n - 1 {
            let s = secant.get(i).copied().unwrap_or(0.0);
            if s.abs() < 1e-9 {
                if let Some(slot) = tangent.get_mut(i) {
                    *slot = 0.0;
                }
                if let Some(slot) = tangent.get_mut(i + 1) {
                    *slot = 0.0;
                }
                continue;
            }
            let a = tangent.get(i).copied().unwrap_or(0.0) / s;
            let b = tangent.get(i + 1).copied().unwrap_or(0.0) / s;
            let norm = a.hypot(b);
            if norm > 3.0 {
                let scale = 3.0 / norm;
                if let Some(slot) = tangent.get_mut(i) {
                    *slot = scale * a * s;
                }
                if let Some(slot) = tangent.get_mut(i + 1) {
                    *slot = scale * b * s;
                }
            }
        }

        for (index, slot) in table.iter_mut().enumerate() {
            let x = index as f32 / (Self::ENTRIES - 1) as f32;
            *slot = evaluate(&xs, &ys, &tangent, x);
        }
        Self { table, identity }
    }

    /// True when this curve changes nothing.
    #[must_use]
    pub const fn is_identity(&self) -> bool {
        self.identity
    }

    /// Look up a value in the curve domain, with linear interpolation between entries.
    ///
    /// Values above 1.0 - which exist, because the working space is scene-referred and a
    /// specular highlight is brighter than white - are passed through with the curve's slope
    /// at 1.0, so a curve does not clip a highlight it was never drawn across.
    #[must_use]
    pub fn lookup(&self, x: f32) -> f32 {
        if x <= 0.0 {
            return self.table.first().copied().unwrap_or(0.0);
        }
        if x >= 1.0 {
            let top = self.table.last().copied().unwrap_or(1.0);
            let below = self
                .table
                .get(Self::ENTRIES.saturating_sub(2))
                .copied()
                .unwrap_or(top);
            let slope = (top - below) * (Self::ENTRIES - 1) as f32;
            return top + (x - 1.0) * slope;
        }
        let position = x * (Self::ENTRIES - 1) as f32;
        let index = position.floor() as usize;
        let frac = position - index as f32;
        let a = self.table.get(index).copied().unwrap_or(0.0);
        let b = self.table.get(index + 1).copied().unwrap_or(a);
        a + (b - a) * frac
    }
}

fn evaluate(xs: &[f32], ys: &[f32], tangent: &[f32], x: f32) -> f32 {
    let n = xs.len();
    if x <= xs.first().copied().unwrap_or(0.0) {
        return ys.first().copied().unwrap_or(0.0);
    }
    if x >= xs.last().copied().unwrap_or(1.0) {
        return ys.last().copied().unwrap_or(1.0);
    }
    let mut i = 0usize;
    while i + 1 < n && xs.get(i + 1).copied().unwrap_or(1.0) < x {
        i += 1;
    }
    let x0 = xs.get(i).copied().unwrap_or(0.0);
    let x1 = xs.get(i + 1).copied().unwrap_or(1.0);
    let y0 = ys.get(i).copied().unwrap_or(0.0);
    let y1 = ys.get(i + 1).copied().unwrap_or(1.0);
    let m0 = tangent.get(i).copied().unwrap_or(0.0);
    let m1 = tangent.get(i + 1).copied().unwrap_or(0.0);
    let h = (x1 - x0).max(1e-9);
    let t = (x - x0) / h;
    let t2 = t * t;
    let t3 = t2 * t;
    (2.0 * t3 - 3.0 * t2 + 1.0) * y0
        + (t3 - 2.0 * t2 + t) * h * m0
        + (-2.0 * t3 + 3.0 * t2) * y1
        + (t3 - t2) * h * m1
}

/// Apply a point curve to one pixel, holding its colour.
#[must_use]
pub fn apply_curve(rgb: [f32; 3], lut: &CurveLut) -> [f32; 3] {
    if lut.is_identity() {
        return rgb;
    }
    let l = luma(rgb);
    if l <= 0.0 {
        return rgb;
    }
    let out = curve_domain_decode(lut.lookup(curve_domain_encode(l)));
    set_luma(rgb, out)
}

/// Hue in `0..1`, saturation in `0..1`, and value, from working-space RGB.
#[must_use]
pub fn to_hsv(rgb: [f32; 3]) -> (f32, f32, f32) {
    let r = rgb.first().copied().unwrap_or(0.0);
    let g = rgb.get(1).copied().unwrap_or(0.0);
    let b = rgb.get(2).copied().unwrap_or(0.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    if delta <= 1e-9 || max <= 0.0 {
        return (0.0, 0.0, max);
    }
    let hue = if (max - r).abs() < 1e-9 {
        ((g - b) / delta).rem_euclid(6.0)
    } else if (max - g).abs() < 1e-9 {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    };
    (hue / 6.0, delta / max, max)
}

/// The eight-band weights a hue falls into.
///
/// Bands overlap: a colour halfway between orange and yellow is affected by both, weighted,
/// which is what stops an HSL adjustment producing a visible edge in a gradient. The centres
/// are evenly spaced in hue, which is Adobe's arrangement and the one photographers'
/// muscle memory is built on.
#[must_use]
pub fn band_weights(hue: f32) -> [f32; 8] {
    let mut out = [0.0f32; 8];
    let mut total = 0.0f32;
    for (index, slot) in out.iter_mut().enumerate() {
        let centre = index as f32 / 8.0;
        let mut d = (hue - centre).abs();
        if d > 0.5 {
            d = 1.0 - d;
        }
        let w = (1.0 - d * 8.0).max(0.0);
        *slot = w * w * (3.0 - 2.0 * w);
        total += *slot;
    }
    if total > 1e-6 {
        for slot in &mut out {
            *slot /= total;
        }
    }
    out
}

/// Per-band hue, saturation and luminance shifts.
#[must_use]
pub fn hsl(rgb: [f32; 3], bands: &BTreeMap<String, HslShift>) -> [f32; 3] {
    if bands.is_empty() || bands.values().all(HslShift::is_neutral) {
        return rgb;
    }
    let (hue, sat, _) = to_hsv(rgb);
    if sat <= 1e-4 {
        // A neutral pixel has no hue to band, and shifting one would tint the greys.
        return rgb;
    }
    let weights = band_weights(hue);

    let mut h_shift = 0.0f32;
    let mut s_shift = 0.0f32;
    let mut l_shift = 0.0f32;
    for (index, name) in HSL_BANDS.iter().enumerate() {
        let Some(shift) = bands.get(*name) else {
            continue;
        };
        let w = weights.get(index).copied().unwrap_or(0.0);
        h_shift += w * f32::from(shift.h);
        s_shift += w * f32::from(shift.s);
        l_shift += w * f32::from(shift.l);
    }

    // Hue rotation of at most one eighth of the wheel at full travel, which is the width of
    // one band: a band that could rotate past its neighbour would let one slider undo
    // another's work.
    let rotated = rotate_hue(rgb, h_shift / 100.0 / 8.0);
    let saturated = saturate(rotated, 1.0 + s_shift / 100.0);
    let l = luma(saturated);
    set_luma(saturated, (l * (1.0 + l_shift / 100.0 * 0.5)).max(0.0))
}

/// Rotate a pixel's hue by `turns` of the full wheel, holding luminance and saturation.
#[must_use]
pub fn rotate_hue(rgb: [f32; 3], turns: f32) -> [f32; 3] {
    if turns.abs() < 1e-6 {
        return rgb;
    }
    let l = luma(rgb);
    let (hue, sat, value) = to_hsv(rgb);
    let out = from_hsv((hue + turns).rem_euclid(1.0), sat, value);
    set_luma(out, l)
}

/// Working-space RGB from hue, saturation and value.
#[must_use]
pub fn from_hsv(hue: f32, sat: f32, value: f32) -> [f32; 3] {
    let h = hue.rem_euclid(1.0) * 6.0;
    let sector = h.floor();
    let f = h - sector;
    let p = value * (1.0 - sat);
    let q = value * (1.0 - sat * f);
    let t = value * (1.0 - sat * (1.0 - f));
    match sector as i32 {
        0 => [value, t, p],
        1 => [q, value, p],
        2 => [p, value, t],
        3 => [p, q, value],
        4 => [t, p, value],
        _ => [value, p, q],
    }
}

/// Scale saturation about the pixel's own luminance.
#[must_use]
pub fn saturate(rgb: [f32; 3], factor: f32) -> [f32; 3] {
    let l = luma(rgb);
    [
        l + (rgb.first().copied().unwrap_or(0.0) - l) * factor,
        l + (rgb.get(1).copied().unwrap_or(0.0) - l) * factor,
        l + (rgb.get(2).copied().unwrap_or(0.0) - l) * factor,
    ]
}

/// Vibrance and saturation, in that order.
///
/// Vibrance is saturation weighted **away** from colours that are already saturated, which
/// is what makes it the safe control on skin: a face at 30 % saturation moves and a sari at
/// 90 % barely does. The weight is `(1 - s)^2`, which is the shape every editor uses.
#[must_use]
pub fn vibrance_saturation(rgb: [f32; 3], vibrance: f32, saturation: f32) -> [f32; 3] {
    let mut out = rgb;
    if vibrance != 0.0 {
        let (_, s, _) = to_hsv(out);
        let weight = (1.0 - s).max(0.0);
        out = saturate(out, 1.0 + vibrance * weight * weight);
    }
    if saturation != 0.0 {
        out = saturate(out, 1.0 + saturation);
    }
    out
}

/// Black-and-white conversion with a per-band mix.
///
/// The mix is a *weight* on each band's contribution to luminance rather than a replacement
/// for it: a band at zero renders at its natural luminance, at +100 at 1.5 times it, at -100
/// at half. A mix that could take a band to zero would let one slider erase a red dress.
#[must_use]
pub fn monochrome(rgb: [f32; 3], bw: &Bw) -> [f32; 3] {
    let l = luma(rgb);
    let (hue, sat, _) = to_hsv(rgb);
    if sat <= 1e-4 || bw.mix.is_empty() {
        return [l; 3];
    }
    let weights = band_weights(hue);
    let mut gain = 1.0f32;
    for (index, name) in HSL_BANDS.iter().enumerate() {
        let Some(amount) = bw.mix.get(*name) else {
            continue;
        };
        let w = weights.get(index).copied().unwrap_or(0.0);
        gain += w * f32::from(*amount) / 100.0 * 0.5;
    }
    [(l * gain.max(0.0)); 3]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Element-wise, because comparing float arrays with `==` is a lint and a habit worth
    /// not forming.
    fn same(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() < 1e-6)
    }

    fn approx(a: [f32; 3], b: [f32; 3], tolerance: f32) {
        for (x, y) in a.iter().zip(b.iter()) {
            assert!((x - y).abs() <= tolerance, "{a:?} vs {b:?}");
        }
    }

    #[test]
    fn the_curve_domain_is_invertible() {
        for value in [0.0f32, 0.001, 0.18, 0.5, 1.0, 4.0] {
            let back = curve_domain_decode(curve_domain_encode(value));
            assert!((back - value).abs() < 1e-4, "{value} -> {back}");
        }
    }

    #[test]
    fn the_identity_curve_is_the_identity() {
        let lut = CurveLut::build(&Curve::identity());
        assert!(lut.is_identity());
        let rgb = [0.4, 0.2, 0.1];
        assert!(same(apply_curve(rgb, &lut), rgb));
    }

    #[test]
    fn a_curve_through_its_own_diagonal_changes_almost_nothing() {
        let lut = CurveLut::build(&Curve {
            points: vec![[0, 0], [64, 64], [128, 128], [255, 255]],
        });
        let rgb = [0.4, 0.2, 0.1];
        approx(apply_curve(rgb, &lut), rgb, 2e-3);
    }

    #[test]
    fn a_monotone_curve_never_overshoots() {
        // Fritsch-Carlson's whole reason for being. A cubic spline through these points
        // dips below zero between the first two.
        let lut = CurveLut::build(&Curve {
            points: vec![[0, 0], [32, 2], [200, 250], [255, 255]],
        });
        let mut previous = -1.0f32;
        for i in 0..CurveLut::ENTRIES {
            let x = i as f32 / (CurveLut::ENTRIES - 1) as f32;
            let y = lut.lookup(x);
            assert!(y >= -1e-6, "curve dipped below zero at {x}: {y}");
            assert!(y >= previous - 1e-6, "curve went backwards at {x}");
            previous = y;
        }
    }

    #[test]
    fn exposure_is_exactly_a_power_of_two() {
        let rgb = [0.1, 0.2, 0.3];
        approx(exposure(rgb, 1.0), [0.2, 0.4, 0.6], 1e-6);
        approx(exposure(rgb, -1.0), [0.05, 0.1, 0.15], 1e-6);
        assert!(same(exposure(rgb, 0.0), rgb));
    }

    #[test]
    fn contrast_holds_middle_grey() {
        let grey = [MID_GREY; 3];
        for amount in [-1.0f32, -0.5, 0.5, 1.0] {
            let out = contrast(grey, amount);
            assert!(
                (luma(out) - MID_GREY).abs() < 1e-3,
                "amount {amount} moved the pivot to {}",
                luma(out)
            );
        }
    }

    #[test]
    fn contrast_steepens_and_flattens_in_the_right_directions() {
        let dark = [0.05; 3];
        let bright = [0.6; 3];
        assert!(luma(contrast(dark, 0.5)) < luma(dark));
        assert!(luma(contrast(bright, 0.5)) > luma(bright));
        assert!(luma(contrast(dark, -0.5)) > luma(dark));
    }

    #[test]
    fn shadows_lift_shadows_and_leave_highlights_alone() {
        let t = Tone {
            shadows: 1.0,
            ..Tone::default()
        };
        let dark = [0.02; 3];
        let bright = [1.2; 3];
        assert!(luma(tone(dark, t)) > luma(dark) * 1.05);
        assert!((luma(tone(bright, t)) - luma(bright)).abs() < 1e-3);
    }

    #[test]
    fn highlights_pull_highlights_down_and_leave_shadows_alone() {
        let t = Tone {
            highlights: -1.0,
            ..Tone::default()
        };
        let dark = [0.01; 3];
        let bright = [0.5; 3];
        assert!(luma(tone(bright, t)) < luma(bright) * 0.95);
        assert!((luma(tone(dark, t)) - luma(dark)).abs() < 1e-3);
    }

    #[test]
    fn every_tone_stage_holds_the_colour_ratio() {
        let rgb = [0.4, 0.2, 0.1];
        let ratio = rgb[0] / rgb[2];
        for out in [
            tone(
                rgb,
                Tone {
                    shadows: 1.0,
                    ..Tone::default()
                },
            ),
            contrast(rgb, 0.8),
            apply_curve(
                rgb,
                &CurveLut::build(&Curve {
                    points: vec![[0, 0], [128, 180], [255, 255]],
                }),
            ),
        ] {
            assert!((out[0] / out[2] - ratio).abs() < 1e-3, "{out:?}");
        }
    }

    #[test]
    fn vibrance_moves_a_dull_colour_more_than_a_vivid_one() {
        let dull = [0.30, 0.26, 0.24];
        let vivid = [0.90, 0.05, 0.05];
        let dull_gain = to_hsv(vibrance_saturation(dull, 1.0, 0.0)).1 - to_hsv(dull).1;
        let vivid_gain = to_hsv(vibrance_saturation(vivid, 1.0, 0.0)).1 - to_hsv(vivid).1;
        assert!(
            dull_gain > vivid_gain,
            "dull {dull_gain} should move more than vivid {vivid_gain}"
        );
    }

    #[test]
    fn hsl_leaves_a_neutral_pixel_alone() {
        let mut bands = BTreeMap::new();
        bands.insert(
            "orange".to_string(),
            HslShift {
                h: 50,
                s: 80,
                l: 40,
            },
        );
        let grey = [0.35; 3];
        assert!(same(hsl(grey, &bands), grey));
    }

    #[test]
    fn hsl_bands_sum_to_one_at_every_hue() {
        for i in 0..64 {
            let hue = i as f32 / 64.0;
            let total: f32 = band_weights(hue).iter().sum();
            assert!((total - 1.0).abs() < 1e-4, "hue {hue} summed to {total}");
        }
    }

    #[test]
    fn a_hue_rotation_of_zero_is_the_identity() {
        let rgb = [0.5, 0.2, 0.1];
        assert!(same(rotate_hue(rgb, 0.0), rgb));
    }

    #[test]
    fn monochrome_produces_three_equal_channels() {
        let bw = Bw::default();
        let out = monochrome([0.5, 0.2, 0.1], &bw);
        assert!((out[0] - out[1]).abs() < 1e-6 && (out[1] - out[2]).abs() < 1e-6);
    }
}
