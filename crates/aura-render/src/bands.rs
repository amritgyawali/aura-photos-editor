//! Frequency separation, three bands, shared by everything that needs one.
//!
//! PHASE-19 put a two-band-of-three separation in `aura_brain_photo::local::freqsep` and
//! deliberately never returned the high band, because nothing in that phase was allowed to
//! touch a pore. PHASE-20 needs the same decomposition and needs the third band as well - not
//! to *edit* it, but to **measure** it: section 6.3's texture guarantee is a ratio of high-band
//! energies before and after a retouch, and a band you cannot measure is a guarantee you can
//! only assert.
//!
//! So the arithmetic moves here, and phase 19's module keeps its own name and its own two-band
//! return by delegating. That is the pattern phase 10 used when the 112 px face warp got its
//! second consumer and phase 16 used when the Fritsch-Carlson interpolation got its third: two
//! implementations of a decomposition are two answers to "is this a pore or a blotch", and the
//! two phases would disagree about the same pixels while both looking correct.
//!
//! ## The three bands
//!
//! | Band | What is in it | Who may move it |
//! |---|---|---|
//! | `low` | form: the shape of the light on a face | phase 19's dodge and burn |
//! | `mid` | blotches, flush, makeup mismatch, the neck/face step | phases 19 and 20 |
//! | `high` | pores, fine lines, hair, grain | **nobody** |
//!
//! `high` is returned here and it is returned for one purpose: [`Bands3::high_energy`]. The
//! whole texture guard is that number measured twice.
//!
//! ## Gaussian rather than bilateral
//!
//! Phase 19's argument, unchanged: a bilateral filter preserves the shadow terminator, which is
//! precisely the edge the low band is supposed to contain so that shaping can move it. A
//! bilateral low band leaves the terminator in `mid`, and shaping the low band then moves the
//! fill without moving the terminator - which reads as a lit face with a shadow painted on it.
//!
//! ## Everything here is linear
//!
//! Invariant 8. There is no transfer function in this module: a band is a difference of blurs
//! of scene-referred values, and `crates/aura-render/tests/colour_discipline.rs` is the grep
//! that keeps it that way.

/// The wide radius, as a fraction of the region's shorter side.
///
/// A twelfth. Wide enough that a cheekbone and the hollow under it land in different places on
/// the low band, narrow enough that a jawline survives it. The same constant
/// `aura_brain_photo::local::freqsep::LOW_RADIUS_FRAC` re-exports and the same one
/// `freq_bands.wgsl` declares.
pub const LOW_RADIUS_FRAC: f32 = 1.0 / 12.0;

/// The narrow radius, as a fraction of the region's shorter side.
///
/// A sixtieth - about two pixels on a face crop from a 2048 px proxy. Below the scale of a
/// blotch and above the scale of a pore, which *is* the definition of the boundary between
/// `mid` and `high`.
pub const HIGH_RADIUS_FRAC: f32 = 1.0 / 60.0;

/// The smallest radius, in samples, that is worth blurring with.
pub const MIN_RADIUS: usize = 1;

/// How many box passes approximate the Gaussian.
///
/// Three. Two is visibly boxy at the radii above and four costs a third more for a difference
/// no measurement in either phase can see.
pub const BOX_PASSES: usize = 3;

/// One region, decomposed into three bands.
#[derive(Debug, Clone, PartialEq)]
pub struct Bands3 {
    /// Form. Same layout as the input.
    pub low: Vec<f32>,
    /// Blotches and modelling, signed around zero.
    pub mid: Vec<f32>,
    /// Pores, fine lines and grain, signed around zero. **Measured, never moved.**
    pub high: Vec<f32>,
    /// Width in samples.
    pub width: usize,
    /// Height in samples.
    pub height: usize,
}

impl Bands3 {
    /// The mid band's energy: mean absolute deviation.
    ///
    /// Mean absolute rather than RMS, because RMS is dominated by the handful of samples at a
    /// shadow terminator and this has to describe the whole region.
    #[must_use]
    pub fn mid_energy(&self) -> f32 {
        mean_abs(&self.mid)
    }

    /// The high band's energy, by the same measure.
    ///
    /// **The number the whole texture guarantee is built on.** PHASE-20 section 6.3: measure it
    /// before the retouch and after it, and refuse a plan whose ratio falls below the preset's
    /// floor.
    #[must_use]
    pub fn high_energy(&self) -> f32 {
        mean_abs(&self.high)
    }

    /// True when there was enough region to decompose at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// The high-band energy over a subset of the samples.
    ///
    /// The texture guard measures over *skin* rather than over a rectangle, because a rectangle
    /// around a face contains hair and a background, and hair has more high-band energy than
    /// anything else in a photograph. A ratio measured over a box would therefore be dominated
    /// by the pixels the retouch never touched, and would pass every time.
    ///
    /// `weights` is parallel to the samples and is the mask coverage at each of them. Returns
    /// the energy and the number of samples that contributed at all, which is what
    /// `TextureReport::measured_on` carries.
    #[must_use]
    pub fn high_energy_masked(&self, weights: &[f32]) -> (f32, u32) {
        let mut total = 0.0f64;
        let mut weight = 0.0f64;
        let mut counted = 0u32;
        for (value, w) in self.high.iter().zip(weights.iter()) {
            if *w <= 0.0 {
                continue;
            }
            let w64 = f64::from(*w);
            total += f64::from(value.abs()) * w64;
            weight += w64;
            counted += 1;
        }
        if weight <= f64::EPSILON {
            return (0.0, 0);
        }
        ((total / weight) as f32, counted)
    }
}

fn mean_abs(values: &[f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let sum: f64 = values.iter().map(|v| f64::from(v.abs())).sum();
    (sum / values.len() as f64) as f32
}

/// Decompose one region into three bands.
///
/// The radii are fractions of the region's shorter side, so a face crop and a whole frame are
/// decomposed at the same *perceptual* scale rather than at the same pixel scale - which is
/// what makes a band ratio measured on a proxy comparable with one measured at full resolution.
#[must_use]
pub fn separate(values: &[f32], width: usize, height: usize) -> Bands3 {
    if width == 0 || height == 0 {
        return Bands3 {
            low: Vec::new(),
            mid: Vec::new(),
            high: Vec::new(),
            width: 0,
            height: 0,
        };
    }
    let side = width.min(height) as f32;
    let low_radius = radius(side, LOW_RADIUS_FRAC);
    let high_radius = radius(side, HIGH_RADIUS_FRAC);

    let low = blur(values, width, height, low_radius);
    let narrow = blur(values, width, height, high_radius);

    let mut mid = Vec::with_capacity(low.len());
    let mut high = Vec::with_capacity(low.len());
    for index in 0..low.len() {
        let n = narrow.get(index).copied().unwrap_or(0.0);
        let l = low.get(index).copied().unwrap_or(0.0);
        let source = values.get(index).copied().unwrap_or(0.0);
        mid.push(n - l);
        high.push(source - n);
    }

    Bands3 {
        low,
        mid,
        high,
        width,
        height,
    }
}

/// The blur radius for one band, in samples.
#[must_use]
pub fn radius(side: f32, fraction: f32) -> usize {
    ((side * fraction).round() as usize).max(MIN_RADIUS)
}

/// Separable box blur, run [`BOX_PASSES`] times.
///
/// Three passes of a box filter is a very good approximation of a Gaussian and is `O(n)` in the
/// radius rather than `O(r)`. Deterministic: the accumulation order is fixed and there is no
/// parallelism inside it, so invariant 4 holds without a seed.
#[must_use]
pub fn blur(values: &[f32], width: usize, height: usize, radius: usize) -> Vec<f32> {
    let mut buffer = values.to_vec();
    buffer.resize(width * height, 0.0);
    let mut scratch = vec![0.0f32; width * height];
    for _ in 0..BOX_PASSES {
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

    fn plane(width: usize, height: usize, f: impl Fn(usize, usize) -> f32) -> Vec<f32> {
        let mut values = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                values.push(f(x, y));
            }
        }
        values
    }

    #[test]
    fn the_three_bands_sum_back_to_the_input() {
        // The property that makes a transplant possible: retouching the mid band and putting
        // the original high band back must reproduce every untouched pixel exactly.
        let values = plane(64, 64, |x, y| ((x * 5 + y * 11) % 23) as f32 / 23.0);
        let bands = separate(&values, 64, 64);
        for (index, value) in values.iter().enumerate() {
            let sum = bands.low[index] + bands.mid[index] + bands.high[index];
            assert!(
                (sum - value).abs() < 1e-5,
                "band sum {sum} != input {value}"
            );
        }
    }

    #[test]
    fn pore_scale_detail_lands_in_the_high_band() {
        let pores = plane(96, 96, |x, y| if (x + y) % 2 == 0 { 0.52 } else { 0.48 });
        let bands = separate(&pores, 96, 96);
        assert!(
            bands.high_energy() > 0.015,
            "pores did not reach the high band: {}",
            bands.high_energy()
        );
        assert!(
            bands.mid_energy() < 0.006,
            "pores leaked into the mid band: {}",
            bands.mid_energy()
        );
    }

    #[test]
    fn a_blotch_lands_in_the_mid_band_and_not_in_the_high_one() {
        let blotchy = plane(96, 96, |x, y| {
            let dx = x as f32 - 48.0;
            let dy = y as f32 - 48.0;
            if dx.hypot(dy) < 10.0 {
                0.62
            } else {
                0.50
            }
        });
        let bands = separate(&blotchy, 96, 96);
        assert!(bands.mid_energy() > 0.002, "{}", bands.mid_energy());
        assert!(bands.high_energy() < bands.mid_energy());
    }

    #[test]
    fn a_gradient_is_all_form() {
        let ramp = plane(64, 64, |x, _| x as f32 / 64.0);
        let bands = separate(&ramp, 64, 64);
        assert!(bands.mid_energy() < 0.01);
        assert!(bands.high_energy() < 0.005);
    }

    #[test]
    fn masked_energy_ignores_the_pixels_outside_the_mask() {
        // Half pores, half flat. Measured over the flat half the energy must be near zero,
        // which is the reason the guard measures over skin rather than over a box.
        let values = plane(64, 64, |x, y| {
            if x < 32 {
                if (x + y) % 2 == 0 {
                    0.55
                } else {
                    0.45
                }
            } else {
                0.5
            }
        });
        let bands = separate(&values, 64, 64);
        let mut weights = vec![0.0f32; 64 * 64];
        for y in 0..64 {
            for x in 32..64 {
                weights[y * 64 + x] = 1.0;
            }
        }
        let (energy, counted) = bands.high_energy_masked(&weights);
        assert_eq!(counted, 64 * 32);
        assert!(energy < 0.01, "flat half measured {energy}");
    }

    #[test]
    fn separation_is_deterministic() {
        let values = plane(48, 48, |x, y| ((x * 7 + y * 13) % 32) as f32 / 32.0);
        assert_eq!(separate(&values, 48, 48), separate(&values, 48, 48));
    }

    #[test]
    fn an_empty_region_is_an_empty_answer() {
        assert!(separate(&[], 0, 0).is_empty());
    }
}
