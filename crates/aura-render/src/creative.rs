//! Lightroom's remaining creative panels: parametric and RGB curves, colour grading, camera
//! calibration, the post-crop vignette and film grain. ADR-0065.
//!
//! Every operator here is point-wise in the sense the tiler cares about - none reads a
//! neighbour - and every one is **deterministic in frame coordinates**, so a tiled export and
//! a whole-frame render of the same recipe agree to the bit. The two position-dependent ones
//! (vignette and grain) take a [`Position`] that describes the *cropped* frame, because both are
//! "post-crop" effects: a vignette drawn on the uncropped frame and then cropped is a vignette
//! whose darkest corner is somewhere in the middle of the delivered photograph.
//!
//! Colour operators work in linear Rec.2020 like the rest of the creative half and read
//! tonality in the curve domain, the same invertible gamma-2.2 coordinate the point curve uses,
//! so "shadows" means the same tones in the colour grade as in the parametric curve.

use aura_recipe::{
    Calibration, ChannelCurves, ColourGrade, Grain, ParametricCurve, PostCropVignette,
};

use crate::colour::{luma, set_luma};
use crate::spatial::Position;
use crate::tonemap::{curve_domain_decode, curve_domain_encode, from_hsv, CurveLut};

/// How far a fully saturated grading wheel moves a channel. Held in the shader too.
pub const GRADE_TINT: f32 = 0.60;
/// Stops of brightness a wheel's luminance slider moves at its ends.
pub const GRADE_LUMA_STOPS: f32 = 0.60;
/// Stops of darkening a vignette of amount -100 applies at the corner.
pub const VIGNETTE_STOPS: f32 = 1.50;
/// Largest grain excursion in the curve domain, at amount 100 in the midtones.
pub const GRAIN_STRENGTH: f32 = 0.10;
/// Largest parametric move in the curve domain, at amount 100.
pub const PARAMETRIC_TRAVEL: f32 = 0.22;
/// Largest primary rotation calibration applies, in turns of the hue wheel.
pub const CALIBRATION_TURNS: f32 = 1.0 / 12.0;

fn smoothstep(x: f32, edge0: f32, edge1: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0).max(1e-6)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn hat(x: f32, centre: f32, width: f32) -> f32 {
    let d = ((x - centre) / width.max(1e-6)).abs();
    if d >= 1.0 {
        return 0.0;
    }
    let t = 1.0 - d;
    t * t * (3.0 - 2.0 * t)
}

// ---------------------------------------------------------------------------------------------
// the tone curve: parametric, then point, then per channel
// ---------------------------------------------------------------------------------------------

/// The parametric curve as a function of the curve-domain value.
///
/// Four smooth bumps centred in their regions, each spanning into its neighbours so the
/// result has no kinks at a split, tapered to zero at black and white so the curve never lifts
/// the black point or pulls down white - which is Lightroom's behaviour and the reason the
/// point curve exists for doing that deliberately. Monotone by construction afterwards: a
/// running maximum is applied to the table the caller builds.
#[must_use]
pub fn parametric(x: f32, p: &ParametricCurve) -> f32 {
    if p.is_flat() {
        return x;
    }
    let s1 = f32::from(p.shadow_split) / 100.0;
    let s2 = f32::from(p.midtone_split) / 100.0;
    let s3 = f32::from(p.highlight_split) / 100.0;
    let regions = [
        (f32::from(p.shadows), s1 * 0.5, s1.max(0.05)),
        (f32::from(p.darks), (s1 + s2) * 0.5, (s2 - s1).max(0.05)),
        (f32::from(p.lights), (s2 + s3) * 0.5, (s3 - s2).max(0.05)),
        (
            f32::from(p.highlights),
            (s3 + 1.0) * 0.5,
            (1.0 - s3).max(0.05),
        ),
    ];
    let taper = smoothstep(x, 0.0, 0.06) * smoothstep(1.0 - x, 0.0, 0.06);
    let lift: f32 = regions
        .iter()
        .map(|(amount, centre, width)| amount / 100.0 * hat(x, *centre, width * 1.25))
        .sum();
    (x + lift * PARAMETRIC_TRAVEL * taper).clamp(0.0, 1.0)
}

/// The luminance curve with the parametric curve in front of it, as one table.
#[must_use]
pub fn luminance_lut(curve: &aura_recipe::Curve, p: &ParametricCurve) -> CurveLut {
    let point = CurveLut::build(curve);
    if p.is_flat() {
        return point;
    }
    let mut last = 0.0_f32;
    CurveLut::from_fn(false, |x| {
        let y = point.lookup(parametric(x, p)).max(last);
        last = y;
        y
    })
}

/// Three channel curves, built once per render.
#[derive(Debug)]
pub struct ChannelLuts {
    red: CurveLut,
    green: CurveLut,
    blue: CurveLut,
    identity: bool,
}

impl ChannelLuts {
    /// Build the tables.
    #[must_use]
    pub fn build(curves: &ChannelCurves) -> Self {
        Self {
            red: CurveLut::build(&curves.red),
            green: CurveLut::build(&curves.green),
            blue: CurveLut::build(&curves.blue),
            identity: curves.is_identity(),
        }
    }

    /// Apply each channel's curve to that channel, in the curve domain.
    ///
    /// Per channel rather than luminance-preserving, deliberately: an RGB curve is how a
    /// photographer *changes* colour - lifting the blue shadows is the whole point - and
    /// holding luminance would make it a hue control that cannot move brightness.
    #[must_use]
    pub fn apply(&self, rgb: [f32; 3]) -> [f32; 3] {
        if self.identity {
            return rgb;
        }
        let [r, g, b] = rgb;
        let one = |value: f32, lut: &CurveLut| {
            if value <= 0.0 {
                value
            } else {
                curve_domain_decode(lut.lookup(curve_domain_encode(value)))
            }
        };
        [one(r, &self.red), one(g, &self.green), one(b, &self.blue)]
    }
}

// ---------------------------------------------------------------------------------------------
// colour grading
// ---------------------------------------------------------------------------------------------

/// A wheel's hue as a luminance-normalised tint offset: `(tint / luma(tint)) - 1`.
fn tint_offset(hue_degrees: i16) -> [f32; 3] {
    let tint = from_hsv(f32::from(hue_degrees) / 360.0, 1.0, 1.0);
    let l = luma(tint).max(1e-4);
    let [r, g, b] = tint;
    [r / l - 1.0, g / l - 1.0, b / l - 1.0]
}

/// The grade, prepared once per render.
#[derive(Debug, Clone, Copy)]
pub struct GradePlan {
    wheels: [([f32; 3], f32, f32); 4],
    pivot: f32,
    soft: f32,
}

impl GradePlan {
    /// Prepare a grade.
    #[must_use]
    pub fn new(grade: &ColourGrade) -> Self {
        let wheel = |w: &aura_recipe::GradeWheel| {
            (
                tint_offset(w.hue),
                f32::from(w.saturation) / 100.0,
                f32::from(w.luminance) / 100.0,
            )
        };
        Self {
            wheels: [
                wheel(&grade.shadows),
                wheel(&grade.midtones),
                wheel(&grade.highlights),
                wheel(&grade.global),
            ],
            // Positive balance favours the highlights wheel by moving the pivot down.
            pivot: (0.5 - f32::from(grade.balance) / 100.0 * 0.3).clamp(0.2, 0.8),
            soft: 0.10 + 0.40 * f32::from(grade.blending) / 100.0,
        }
    }

    /// The three regional weights at a curve-domain tone. They sum to one.
    #[must_use]
    pub fn weights(&self, t: f32) -> [f32; 3] {
        let shadows = 1.0 - smoothstep(t, self.pivot - self.soft, self.pivot);
        let highlights = smoothstep(t, self.pivot, self.pivot + self.soft);
        [shadows, (1.0 - shadows - highlights).max(0.0), highlights]
    }

    /// Grade one pixel.
    ///
    /// The tint is applied as a per-channel gain and the pixel is then returned to its own
    /// luminance, so a wheel moves colour and never brightness; only the luminance slider does
    /// that. This is what keeps a split-tone from quietly lifting the shadows it colours.
    #[must_use]
    pub fn apply(&self, rgb: [f32; 3]) -> [f32; 3] {
        let l = luma(rgb);
        if l <= 1e-6 {
            return rgb;
        }
        let t = curve_domain_encode(l).clamp(0.0, 1.0);
        let [ws, wm, wh] = self.weights(t);
        let mut offset = [0.0_f32; 3];
        let mut stops = 0.0_f32;
        for ((tint, sat, lum), weight) in self.wheels.iter().zip([ws, wm, wh, 1.0]) {
            for (slot, channel) in offset.iter_mut().zip(tint.iter()) {
                *slot += weight * sat * channel;
            }
            stops += weight * lum;
        }
        let [r, g, b] = rgb;
        let [or, og, ob] = offset;
        let tinted = [
            (r * (1.0 + GRADE_TINT * or)).max(0.0),
            (g * (1.0 + GRADE_TINT * og)).max(0.0),
            (b * (1.0 + GRADE_TINT * ob)).max(0.0),
        ];
        set_luma(tinted, l * (stops * GRADE_LUMA_STOPS).exp2())
    }
}

// ---------------------------------------------------------------------------------------------
// calibration
// ---------------------------------------------------------------------------------------------

type Mat3 = [[f32; 3]; 3];

fn mul(m: &Mat3, v: [f32; 3]) -> [f32; 3] {
    let [x, y, z] = v;
    let row = |r: &[f32; 3]| r[0] * x + r[1] * y + r[2] * z;
    let [a, b, c] = m;
    [row(a), row(b), row(c)]
}

/// Rotation about the neutral axis `(1, 1, 1)` by `turns` of the hue wheel (Rodrigues).
fn hue_rotation(turns: f32) -> Mat3 {
    let angle = turns * std::f32::consts::TAU;
    let (s, c) = angle.sin_cos();
    let k = (1.0 - c) / 3.0;
    let q = s / 3.0_f32.sqrt();
    [
        [c + k, k - q, k + q],
        [k + q, c + k, k - q],
        [k - q, k + q, c + k],
    ]
}

/// Camera calibration, prepared once per render: a white-preserving 3x3 plus a shadow tint.
#[derive(Debug, Clone, Copy)]
pub struct CalibrationPlan {
    matrix: Mat3,
    shadow_tint: f32,
}

impl CalibrationPlan {
    /// Build the matrix whose columns are the moved primaries.
    ///
    /// Each primary is rotated about the neutral axis and pushed toward or away from the grey
    /// of its own luminance; the rows are then scaled so white maps to white. A calibration
    /// that tinted white would be a white-balance control with a misleading name.
    #[must_use]
    pub fn new(c: &Calibration) -> Self {
        let primaries = [
            ([1.0, 0.0, 0.0], c.red_hue, c.red_saturation),
            ([0.0, 1.0, 0.0], c.green_hue, c.green_saturation),
            ([0.0, 0.0, 1.0], c.blue_hue, c.blue_saturation),
        ];
        let mut columns = [[0.0_f32; 3]; 3];
        for (column, (primary, hue, sat)) in columns.iter_mut().zip(primaries) {
            let rotated = mul(
                &hue_rotation(f32::from(hue) / 100.0 * CALIBRATION_TURNS),
                primary,
            );
            let grey = luma(primary);
            let scale = 1.0 + f32::from(sat) / 100.0;
            *column = rotated.map(|v| grey + (v - grey) * scale);
        }
        let [c0, c1, c2] = columns;
        let mut matrix = [
            [c0[0], c1[0], c2[0]],
            [c0[1], c1[1], c2[1]],
            [c0[2], c1[2], c2[2]],
        ];
        for row in &mut matrix {
            let sum: f32 = row.iter().sum();
            if sum.abs() > 1e-6 {
                for v in row.iter_mut() {
                    *v /= sum;
                }
            }
        }
        Self {
            matrix,
            shadow_tint: f32::from(c.shadows_tint) / 100.0,
        }
    }

    /// Calibrate one pixel.
    #[must_use]
    pub fn apply(&self, rgb: [f32; 3]) -> [f32; 3] {
        let [r, g, b] = mul(&self.matrix, rgb);
        if self.shadow_tint.abs() < 1e-6 {
            return [r, g, b];
        }
        let l = luma([r, g, b]);
        let shadow = 1.0 - smoothstep(curve_domain_encode(l.max(0.0)), 0.0, 0.5);
        // Positive tint is magenta, as in Lightroom: less green.
        let gain = 1.0 - 0.25 * self.shadow_tint * shadow;
        set_luma([r, (g * gain).max(0.0), b], l)
    }
}

// ---------------------------------------------------------------------------------------------
// the post-crop effects
// ---------------------------------------------------------------------------------------------

/// Where a pixel sits in the cropped frame, as `u, v` in `-1..1` from the centre.
fn centred(index: usize, width: usize, position: Position) -> (f32, f32, f32) {
    let x = (index % width.max(1)) as f32 + position.x as f32 + 0.5;
    let y = (index / width.max(1)) as f32 + position.y as f32 + 0.5;
    let fw = position.full_width.max(1) as f32;
    let fh = position.full_height.max(1) as f32;
    (x / fw * 2.0 - 1.0, y / fh * 2.0 - 1.0, fw / fh)
}

/// Apply the post-crop vignette to a buffer that is (part of) the cropped frame.
pub fn vignette(rgb: &mut [f32], width: usize, position: Position, settings: &PostCropVignette) {
    use rayon::prelude::*;
    let amount = f32::from(settings.amount) / 100.0;
    if amount.abs() < 1e-6 {
        return;
    }
    let midpoint = f32::from(settings.midpoint) / 100.0;
    let feather = (f32::from(settings.feather) / 100.0).max(0.02);
    let roundness = f32::from(settings.roundness) / 100.0;
    let highlights = f32::from(settings.highlights) / 100.0;
    let lo = midpoint * (1.0 - feather);
    let hi = (midpoint + (1.0 - midpoint) * feather).max(lo + 0.02);
    rgb.par_chunks_mut(3)
        .enumerate()
        .for_each(|(index, pixel)| {
            let (u, v, aspect) = centred(index, width, position);
            // Roundness +1 is a circle in the frame's own pixels; 0 is an ellipse that follows the
            // frame; -1 squares it toward the rectangle with a higher superellipse exponent.
            // A circle in pixels divides the short axis by the aspect ratio.
            let (su, sv) = if roundness > 0.0 {
                let (cu, cv) = if aspect >= 1.0 {
                    (1.0, aspect)
                } else {
                    (1.0 / aspect, 1.0)
                };
                (1.0 + (cu - 1.0) * roundness, 1.0 + (cv - 1.0) * roundness)
            } else {
                (1.0, 1.0)
            };
            let power = 2.0 + 4.0 * (-roundness).max(0.0);
            let d = ((u / su).abs().powf(power) + (v / sv).abs().powf(power)).powf(1.0 / power)
                / 2.0_f32.powf(1.0 / power);
            let mut weight = smoothstep(d, lo, hi);
            let [r, g, b] = [
                pixel.first().copied().unwrap_or(0.0),
                pixel.get(1).copied().unwrap_or(0.0),
                pixel.get(2).copied().unwrap_or(0.0),
            ];
            if highlights > 0.0 && amount < 0.0 {
                let t = curve_domain_encode(luma([r, g, b]).max(0.0));
                weight *= 1.0 - highlights * smoothstep(t, 0.55, 0.95);
            }
            let gain = (amount * VIGNETTE_STOPS * weight).exp2();
            for (slot, value) in pixel.iter_mut().zip([r, g, b]) {
                *slot = value * gain;
            }
        });
}

/// A deterministic value in `-1..1` for an integer lattice point.
fn lattice(x: i64, y: i64, seed: u64) -> f32 {
    let mut h = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ seed;
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    h ^= h >> 33;
    h = h.wrapping_mul(0xC4CE_B9FE_1A85_EC53);
    h ^= h >> 33;
    (h >> 40) as f32 / (1_u64 << 23) as f32 - 1.0
}

/// Smooth value noise at a frame coordinate, with a lattice spacing of `cell` pixels.
fn value_noise(x: f32, y: f32, cell: f32, seed: u64) -> f32 {
    let gx = x / cell;
    let gy = y / cell;
    let (ix, iy) = (gx.floor(), gy.floor());
    let (fx, fy) = (gx - ix, gy - iy);
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let (ix, iy) = (ix as i64, iy as i64);
    let a = lattice(ix, iy, seed);
    let b = lattice(ix + 1, iy, seed);
    let c = lattice(ix, iy + 1, seed);
    let d = lattice(ix + 1, iy + 1, seed);
    let top = a + (b - a) * sx;
    let bottom = c + (d - c) * sx;
    top + (bottom - top) * sy
}

/// Apply film grain to a buffer that is (part of) the cropped frame.
///
/// Monochromatic, strongest in the midtones and absent in pure black and white, and sized
/// against the frame's long edge so a proxy and an export carry the same grain. The pattern is
/// a function of the frame coordinate alone, so every tile of a streamed export draws exactly
/// the grain the whole-frame render drew there.
pub fn grain(rgb: &mut [f32], width: usize, position: Position, g: &Grain) {
    use rayon::prelude::*;
    let amount = f32::from(g.amount) / 100.0;
    if amount <= 0.0 {
        return;
    }
    let long = position.full_width.max(position.full_height).max(1) as f32;
    let cell = ((0.6 + f32::from(g.size) / 100.0 * 3.4) * long / 2400.0).max(0.5);
    let rough = f32::from(g.roughness) / 100.0;
    rgb.par_chunks_mut(3)
        .enumerate()
        .for_each(|(index, pixel)| {
            let x = (index % width.max(1)) as f32 + position.x as f32;
            let y = (index / width.max(1)) as f32 + position.y as f32;
            let fine = value_noise(x, y, cell, 0x6752_4149_4e21);
            let coarse = value_noise(x, y, cell * 2.3, 0x4155_5241_0001);
            let n = fine * (1.0 - 0.5 * rough) + coarse * 0.5 * rough;
            let value = [
                pixel.first().copied().unwrap_or(0.0),
                pixel.get(1).copied().unwrap_or(0.0),
                pixel.get(2).copied().unwrap_or(0.0),
            ];
            let l = luma(value);
            if l <= 1e-6 {
                return;
            }
            let t = curve_domain_encode(l).clamp(0.0, 1.0);
            let midtones = 4.0 * t * (1.0 - t);
            let shifted = (t + n * amount * GRAIN_STRENGTH * midtones).clamp(0.0, 1.0);
            let out = set_luma(value, curve_domain_decode(shifted));
            for (slot, v) in pixel.iter_mut().zip(out) {
                *slot = v;
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_recipe::{Curve, GradeWheel};

    #[test]
    fn a_neutral_parametric_curve_is_the_identity_and_a_lift_is_monotone() {
        let flat = ParametricCurve::default();
        for i in 0..=20 {
            let x = i as f32 / 20.0;
            assert!((parametric(x, &flat) - x).abs() < 1e-6);
        }
        let lifted = ParametricCurve {
            shadows: 100,
            darks: 60,
            ..ParametricCurve::default()
        };
        let lut = luminance_lut(&Curve::identity(), &lifted);
        let mut last = -1.0;
        for i in 0..=100 {
            let y = lut.lookup(i as f32 / 100.0);
            assert!(y >= last - 1e-6, "not monotone at {i}");
            last = y;
        }
        assert!(
            lut.lookup(0.2) > 0.2 + 0.05,
            "shadows +100 lifts the shadows"
        );
        assert!(lut.lookup(0.0) < 0.01, "the black point does not move");
        assert!(
            (lut.lookup(0.9) - 0.9).abs() < 0.03,
            "highlights barely move"
        );
    }

    #[test]
    fn grade_weights_are_a_partition_of_unity() {
        let plan = GradePlan::new(&ColourGrade::default());
        for i in 0..=20 {
            let w = plan.weights(i as f32 / 20.0);
            assert!((w.iter().sum::<f32>() - 1.0).abs() < 1e-5);
        }
        assert!(plan.weights(0.02)[0] > 0.95);
        assert!(plan.weights(0.98)[2] > 0.95);
    }

    #[test]
    fn a_split_tone_colours_shadows_and_highlights_without_moving_brightness() {
        let grade = ColourGrade {
            shadows: GradeWheel {
                hue: 200,
                saturation: 40,
                luminance: 0,
            },
            highlights: GradeWheel {
                hue: 40,
                saturation: 40,
                luminance: 0,
            },
            ..ColourGrade::default()
        };
        let plan = GradePlan::new(&grade);
        let shadow = plan.apply([0.01, 0.01, 0.01]);
        let highlight = plan.apply([0.7, 0.7, 0.7]);
        assert!(shadow[2] > shadow[0], "teal shadows: {shadow:?}");
        assert!(
            highlight[0] > highlight[2],
            "orange highlights: {highlight:?}"
        );
        assert!((luma(shadow) - 0.01).abs() < 1e-4);
        assert!((luma(highlight) - 0.7).abs() < 1e-3);
    }

    #[test]
    fn calibration_keeps_white_white_and_moves_colour() {
        let plan = CalibrationPlan::new(&Calibration {
            red_hue: 60,
            blue_saturation: 50,
            ..Calibration::default()
        });
        let white = plan.apply([0.5, 0.5, 0.5]);
        for v in white {
            assert!((v - 0.5).abs() < 1e-4, "white moved: {white:?}");
        }
        let red = plan.apply([0.6, 0.1, 0.1]);
        assert!(
            (red[1] - 0.1).abs() > 0.005,
            "a red hue shift moves red: {red:?}"
        );
    }

    #[test]
    fn the_vignette_darkens_corners_and_leaves_the_centre() {
        let (w, h) = (40_usize, 30_usize);
        let mut rgb = vec![0.2_f32; w * h * 3];
        let v = PostCropVignette {
            amount: -80,
            ..PostCropVignette::default()
        };
        vignette(&mut rgb, w, Position::whole(w as u32, h as u32), &v);
        let centre = rgb[(15 * w + 20) * 3];
        let corner = rgb[0];
        assert!((centre - 0.2).abs() < 0.01, "centre {centre}");
        assert!(corner < 0.12, "corner {corner}");
    }

    #[test]
    fn grain_is_the_same_in_a_tile_as_in_the_whole_frame() {
        let (w, h) = (64_usize, 48_usize);
        let make = || -> Vec<f32> {
            (0..w * h)
                .flat_map(|i| {
                    let v = 0.05 + (i % w) as f32 / w as f32 * 0.5;
                    [v, v * 0.9, v * 0.8]
                })
                .collect()
        };
        let g = Grain {
            amount: 60,
            ..Grain::default()
        };
        let mut whole = make();
        grain(&mut whole, w, Position::whole(w as u32, h as u32), &g);
        assert!(whole.iter().zip(make()).any(|(a, b)| (a - b).abs() > 1e-4));
        // The right half, rendered as its own tile.
        let half = w / 2;
        let mut tile: Vec<f32> = make()
            .chunks(w * 3)
            .flat_map(|row| row[half * 3..].to_vec())
            .collect();
        grain(
            &mut tile,
            half,
            Position {
                x: half as u32,
                y: 0,
                full_width: w as u32,
                full_height: h as u32,
            },
            &g,
        );
        for (row, expected) in whole.chunks(w * 3).enumerate() {
            let got = &tile[row * half * 3..(row + 1) * half * 3];
            for (a, b) in got.iter().zip(&expected[half * 3..]) {
                assert!((a - b).abs() < 1e-6);
            }
        }
    }
}
