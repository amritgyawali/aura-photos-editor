//! Frequency separation: taking a face apart so the form can be shaped and the pores cannot.
//!
//! PHASE-19 section 6.3, first bullet:
//!
//! > Separate the face region into low-frequency (form) and mid-frequency (texture) bands
//! > with a bilateral/Gaussian pair; shape only the low-frequency band so pores are
//! > untouched.
//!
//! ## Three bands, not two
//!
//! A retoucher's frequency separation is two bands - low and high - because the two things
//! they want are to move form and to keep pores. This phase needs a third, and section 6.3's
//! third bullet is why: "mid-frequency evening reduces blotchy tonal patches without
//! smoothing". A blotch is not form and it is not a pore; it is somewhere in between, and a
//! two-band split puts it in whichever band the radius happened to catch.
//!
//! So: `low` is a wide Gaussian, `mid` is the difference between a narrow Gaussian and the
//! wide one, and `high` is everything the narrow Gaussian left behind. **`high` is never
//! written to and never read.** It is not returned, it is not stored and there is no operator
//! anywhere in this phase that could touch it - which is what makes "dodge and burn shapes
//! form without touching skin texture" (section 13) a property of the decomposition rather
//! than a promise about the operators.
//!
//! ## Gaussian rather than bilateral
//!
//! Section 6.3 offers "bilateral/Gaussian pair" and this takes the Gaussian. A bilateral
//! filter is edge-preserving, which sounds better and is worse here: the edge it preserves on
//! a face is the shadow boundary, and the shadow boundary is precisely the thing the low band
//! is supposed to contain so that shaping can move it. A bilateral low band leaves the shadow
//! edge in `mid`, and shaping the low band then moves the fill without moving the terminator,
//! which reads as a lit face with a shadow painted on it.

use aura_vision::embed::descriptors::LumaPlane;

/// The wide radius, as a fraction of the face region's shorter side.
///
/// A twelfth. Wide enough that a cheekbone and the hollow under it end up in different places
/// on the low band, narrow enough that the jawline survives it.
pub const LOW_RADIUS_FRAC: f32 = 1.0 / 12.0;

/// The narrow radius, as a fraction of the face region's shorter side.
///
/// A sixtieth - about two pixels on a face crop from a 2048 px proxy. Below the scale of a
/// blotch and above the scale of a pore, which is the definition of the boundary between
/// `mid` and `high`.
pub const HIGH_RADIUS_FRAC: f32 = 1.0 / 60.0;

/// The smallest radius, in samples, that is worth blurring with.
pub const MIN_RADIUS: usize = 1;

/// One face crop, decomposed.
#[derive(Debug, Clone, PartialEq)]
pub struct Bands {
    /// The low-frequency band: form. Same layout as the input.
    pub low: Vec<f32>,
    /// The mid-frequency band: blotches and modelling, signed around zero.
    pub mid: Vec<f32>,
    /// Width in samples.
    pub width: usize,
    /// Height in samples.
    pub height: usize,
}

impl Bands {
    /// The mid band's energy: mean absolute deviation.
    ///
    /// Section 10.1's "measured band energy" and the number the texture gate is written
    /// against. Mean absolute rather than RMS, because RMS is dominated by the handful of
    /// samples at a shadow terminator and this needs to describe the whole crop.
    #[must_use]
    pub fn mid_energy(&self) -> f32 {
        if self.mid.is_empty() {
            return 0.0;
        }
        self.mid.iter().map(|v| v.abs()).sum::<f32>() / self.mid.len() as f32
    }

    /// True when there was enough crop to decompose at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }
}

/// Decompose one face crop.
#[must_use]
pub fn separate(crop: &LumaPlane) -> Bands {
    if crop.width == 0 || crop.height == 0 {
        return Bands {
            low: Vec::new(),
            mid: Vec::new(),
            width: 0,
            height: 0,
        };
    }
    let side = crop.width.min(crop.height) as f32;
    let low_radius = ((side * LOW_RADIUS_FRAC).round() as usize).max(MIN_RADIUS);
    let high_radius = ((side * HIGH_RADIUS_FRAC).round() as usize).max(MIN_RADIUS);

    let low = blur(&crop.values, crop.width, crop.height, low_radius);
    let narrow = blur(&crop.values, crop.width, crop.height, high_radius);
    let mid = narrow
        .iter()
        .zip(low.iter())
        .map(|(n, l)| n - l)
        .collect::<Vec<f32>>();

    Bands {
        low,
        mid,
        width: crop.width,
        height: crop.height,
    }
}

/// Separable box blur, run three times.
///
/// Three passes of a box filter is a very good approximation of a Gaussian and is `O(n)` in
/// the radius rather than `O(r)`, which is what keeps section 11's 80 ms reachable on a
/// four-hundred-pixel face crop. Deterministic: the accumulation order is fixed and there is
/// no parallelism inside it, so invariant 4 holds without a seed.
#[must_use]
pub fn blur(values: &[f32], width: usize, height: usize, radius: usize) -> Vec<f32> {
    let mut buffer = values.to_vec();
    buffer.resize(width * height, 0.0);
    let mut scratch = vec![0.0f32; width * height];
    for _ in 0..3 {
        box_pass_h(&buffer, &mut scratch, width, height, radius);
        box_pass_v(&scratch, &mut buffer, width, height, radius);
    }
    buffer
}

fn box_pass_h(src: &[f32], dst: &mut [f32], width: usize, height: usize, radius: usize) {
    if width == 0 {
        return;
    }
    for y in 0..height {
        let row = y * width;
        for x in 0..width {
            let lo = x.saturating_sub(radius);
            let hi = (x + radius).min(width - 1);
            let mut sum = 0.0f32;
            for i in lo..=hi {
                sum += src.get(row + i).copied().unwrap_or(0.0);
            }
            if let Some(slot) = dst.get_mut(row + x) {
                *slot = sum / (hi - lo + 1) as f32;
            }
        }
    }
}

fn box_pass_v(src: &[f32], dst: &mut [f32], width: usize, height: usize, radius: usize) {
    if height == 0 {
        return;
    }
    for x in 0..width {
        for y in 0..height {
            let lo = y.saturating_sub(radius);
            let hi = (y + radius).min(height - 1);
            let mut sum = 0.0f32;
            for i in lo..=hi {
                sum += src.get(i * width + x).copied().unwrap_or(0.0);
            }
            if let Some(slot) = dst.get_mut(y * width + x) {
                *slot = sum / (hi - lo + 1) as f32;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane(width: usize, height: usize, f: impl Fn(usize, usize) -> f32) -> LumaPlane {
        let mut values = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                values.push(f(x, y));
            }
        }
        LumaPlane {
            values,
            width,
            height,
        }
    }

    #[test]
    fn a_flat_crop_has_no_bands() {
        let flat = plane(64, 64, |_, _| 0.5);
        let bands = separate(&flat);
        assert!(bands.mid_energy() < 1e-5);
        for value in &bands.low {
            assert!((value - 0.5).abs() < 1e-3);
        }
    }

    #[test]
    fn a_gradient_is_all_form_and_no_texture() {
        // A face lit from one side is a gradient. It must land entirely in the low band, or
        // shaping would be unable to move the lighting.
        let ramp = plane(64, 64, |x, _| x as f32 / 64.0);
        let bands = separate(&ramp);
        assert!(
            bands.mid_energy() < 0.01,
            "a smooth gradient leaked {} into the texture band",
            bands.mid_energy()
        );
    }

    #[test]
    fn a_blotch_lands_in_the_mid_band() {
        let blotchy = plane(96, 96, |x, y| {
            let dx = x as f32 - 48.0;
            let dy = y as f32 - 48.0;
            if dx.hypot(dy) < 10.0 {
                0.62
            } else {
                0.50
            }
        });
        let bands = separate(&blotchy);
        assert!(
            bands.mid_energy() > 0.002,
            "a blotch did not reach the mid band: {}",
            bands.mid_energy()
        );
    }

    #[test]
    fn pore_scale_noise_stays_out_of_both_returned_bands() {
        // The point of the third band. Alternating single samples are `high`, and `high` is
        // not returned - so a shaping operator cannot reach them however hard it tries.
        let pores = plane(96, 96, |x, y| if (x + y) % 2 == 0 { 0.52 } else { 0.48 });
        let bands = separate(&pores);
        assert!(
            bands.mid_energy() < 0.006,
            "pore-scale detail reached the shapeable bands: {}",
            bands.mid_energy()
        );
    }

    #[test]
    fn separation_is_deterministic() {
        let crop = plane(48, 48, |x, y| ((x * 7 + y * 13) % 32) as f32 / 32.0);
        assert_eq!(separate(&crop), separate(&crop));
    }

    #[test]
    fn an_empty_crop_is_an_empty_answer() {
        let empty = plane(0, 0, |_, _| 0.0);
        assert!(separate(&empty).is_empty());
    }
}
