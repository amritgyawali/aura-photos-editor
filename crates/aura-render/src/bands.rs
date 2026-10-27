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
/// Three passes of a box filter is a very good approximation of a Gaussian. Each pass keeps a
/// running sum, so its cost does not grow with the radius - which matters at full resolution,
/// where the radii are several times larger than on a proxy - and rows are shared out across
/// the processor's cores. Windows are clipped at the frame edge and averaged over the samples
/// they hold.
///
/// Deterministic on every machine: each output sample is computed by one task, in a fixed
/// order, and the way the rows are divided depends only on the frame and the radius - never on
/// how many cores there are - so invariant 4 holds without a seed.
#[must_use]
pub fn blur(values: &[f32], width: usize, height: usize, radius: usize) -> Vec<f32> {
    blur_owned(values.to_vec(), width, height, radius)
}

/// [`blur`], over a buffer the caller no longer needs: it is blurred where it is rather than
/// copied first.
///
/// A box filter along rows and one along columns act on different indices, so they commute:
/// three passes along every row and then three down every column is the same filter as
/// alternating them. Done that way, a row stays in the first-level cache for all three of its
/// passes and a strip of columns in the second-level cache for all three of its, and the frame
/// crosses main memory twice rather than twelve times - which is what a blur over a face at
/// full resolution is limited by. The running sums are the same `f64` sums; only the order of
/// the passes differs, which moves a result by a rounding at most, the same on every machine.
#[must_use]
pub fn blur_owned(mut buffer: Vec<f32>, width: usize, height: usize, radius: usize) -> Vec<f32> {
    use rayon::prelude::*;
    /// Columns one task carries down the frame: a strip of a tall face fits the second-level
    /// cache, and a frame of any width still makes dozens of tasks.
    const STRIP: usize = 32;
    buffer.resize(width * height, 0.0);
    if width == 0 || height == 0 || radius == 0 {
        return buffer;
    }
    buffer.par_chunks_exact_mut(width).for_each_init(
        || (vec![0.0f32; width], vec![0.0f32; width]),
        |(here, there), row| {
            here.copy_from_slice(row);
            for _ in 0..BOX_PASSES {
                box_row(here, there, radius);
                std::mem::swap(here, there);
            }
            row.copy_from_slice(here);
        },
    );
    // Every strip owns its own piece of every row.
    let mut strips: Vec<Vec<&mut [f32]>> = (0..width.div_ceil(STRIP))
        .map(|_| Vec::with_capacity(height))
        .collect();
    for row in buffer.chunks_exact_mut(width) {
        for (pieces, piece) in strips.iter_mut().zip(row.chunks_mut(STRIP)) {
            pieces.push(piece);
        }
    }
    strips.into_par_iter().for_each(|mut rows| {
        let span = rows.first().map_or(0, |row| row.len());
        let mut here: Vec<f32> = Vec::with_capacity(span * height);
        for row in &rows {
            here.extend_from_slice(row);
        }
        let mut there = vec![0.0f32; here.len()];
        for _ in 0..BOX_PASSES {
            box_columns(&here, &mut there, span, height, radius);
            std::mem::swap(&mut here, &mut there);
        }
        for (row, values) in rows.iter_mut().zip(here.chunks_exact(span)) {
            row.copy_from_slice(values);
        }
    });
    buffer
}

/// One box pass along one row: every window's mean, from a running sum in `f64`, clipped at
/// the ends and averaged over the samples it holds.
fn box_row(src: &[f32], dst: &mut [f32], radius: usize) {
    let width = src.len();
    let mut sum: f64 = src.iter().take(radius + 1).map(|v| f64::from(*v)).sum();
    for (x, slot) in dst.iter_mut().enumerate() {
        let lo = x.saturating_sub(radius);
        let hi = (x + radius).min(width - 1);
        *slot = (sum / (hi - lo + 1) as f64) as f32;
        if x >= radius {
            sum -= src.get(x - radius).map_or(0.0, |v| f64::from(*v));
        }
        if let Some(v) = src.get(x + radius + 1) {
            sum += f64::from(*v);
        }
    }
}

/// One box pass down every column of a strip `span` columns wide, row-major, one running sum
/// per column carried the whole height.
fn box_columns(src: &[f32], dst: &mut [f32], span: usize, height: usize, radius: usize) {
    let row = |y: usize| src.get(y * span..(y + 1) * span).unwrap_or(&[]);
    let mut sums = vec![0.0f64; span];
    for y in 0..(radius + 1).min(height) {
        for (sum, v) in sums.iter_mut().zip(row(y)) {
            *sum += f64::from(*v);
        }
    }
    for (y, line) in dst.chunks_exact_mut(span).enumerate() {
        let lo = y.saturating_sub(radius);
        let hi = (y + radius).min(height - 1);
        let count = (hi - lo + 1) as f64;
        for (slot, sum) in line.iter_mut().zip(sums.iter()) {
            *slot = (sum / count) as f32;
        }
        if y >= radius {
            for (sum, v) in sums.iter_mut().zip(row(y - radius)) {
                *sum -= f64::from(*v);
            }
        }
        if y + radius + 1 < height {
            for (sum, v) in sums.iter_mut().zip(row(y + radius + 1)) {
                *sum += f64::from(*v);
            }
        }
    }
}

/// One box pass each way: the mean of the `(2r+1)` square window around every sample,
/// clipped at the frame edge. The same running sums and the same sharing of rows across cores
/// as [`blur`], which is three of these.
#[must_use]
pub(crate) fn box_mean(values: &[f32], width: usize, height: usize, radius: usize) -> Vec<f32> {
    let mut out = values.to_vec();
    out.resize(width * height, 0.0);
    if width == 0 || height == 0 || radius == 0 {
        return out;
    }
    let mut scratch = vec![0.0f32; width * height];
    box_pass_h(&out, &mut scratch, width, radius);
    box_pass_v(&scratch, &mut out, width, height, radius);
    out
}

/// One horizontal pass: every row's window mean, from a running sum in `f64`.
fn box_pass_h(src: &[f32], dst: &mut [f32], width: usize, radius: usize) {
    use rayon::prelude::*;
    dst.par_chunks_exact_mut(width)
        .zip(src.par_chunks_exact(width))
        .for_each(|(out, row)| {
            let mut sum: f64 = row.iter().take(radius + 1).map(|v| f64::from(*v)).sum();
            for (x, slot) in out.iter_mut().enumerate() {
                let lo = x.saturating_sub(radius);
                let hi = (x + radius).min(width - 1);
                *slot = (sum / (hi - lo + 1) as f64) as f32;
                // Slide the window one sample to the right.
                if x >= radius {
                    sum -= row.get(x - radius).map_or(0.0, |v| f64::from(*v));
                }
                if let Some(v) = row.get(x + radius + 1) {
                    sum += f64::from(*v);
                }
            }
        });
}

/// One vertical pass. The frame is cut into bands of rows whose height depends only on the
/// radius; each band starts its running column sums afresh and slides them down, so memory is
/// read in order. When a radius is so wide that there are only a few bands, each band is cut
/// again into strips of columns, every strip its own task writing its own pieces of the rows:
/// a band as tall as a face is then not summed by one core. Each column's sums are the same
/// additions in the same order however it is cut, so the result does not depend on the
/// strips or the cores.
/// The columns `x0..x1` of one row, empty when the row is shorter.
fn span(row: &[f32], x0: usize, x1: usize) -> &[f32] {
    row.get(x0..x1).unwrap_or(&[])
}

fn box_pass_v(src: &[f32], dst: &mut [f32], width: usize, height: usize, radius: usize) {
    use rayon::prelude::*;
    /// Columns one task sums when bands alone are too few: whole cache lines, and a dozen
    /// tasks across a face-sized band.
    const STRIP: usize = 128;
    let band = (radius * 4).max(64);
    let rows: Vec<&[f32]> = src.chunks_exact(width).collect();
    let strip = if height.div_ceil(band) >= 2 * rayon::current_num_threads() {
        width
    } else {
        STRIP.min(width)
    };
    dst.par_chunks_mut(width * band)
        .enumerate()
        .for_each(|(index, out)| {
            let first = index * band;
            // Every strip owns its own piece of every row of the band.
            let mut columns: Vec<Vec<&mut [f32]>> =
                (0..width.div_ceil(strip)).map(|_| Vec::new()).collect();
            for line in out.chunks_exact_mut(width) {
                for (pieces, piece) in columns.iter_mut().zip(line.chunks_mut(strip)) {
                    pieces.push(piece);
                }
            }
            columns
                .into_par_iter()
                .enumerate()
                .for_each(|(number, mut lines)| {
                    let x0 = number * strip;
                    let x1 = (x0 + strip).min(width);
                    let mut sums = vec![0.0f64; x1 - x0];
                    for row in rows
                        .iter()
                        .take((first + radius + 1).min(height))
                        .skip(first.saturating_sub(radius))
                    {
                        for (sum, v) in sums.iter_mut().zip(span(row, x0, x1)) {
                            *sum += f64::from(*v);
                        }
                    }
                    for (offset, line) in lines.iter_mut().enumerate() {
                        let y = first + offset;
                        let lo = y.saturating_sub(radius);
                        let hi = (y + radius).min(height - 1);
                        let count = (hi - lo + 1) as f64;
                        for (slot, sum) in line.iter_mut().zip(sums.iter()) {
                            *slot = (sum / count) as f32;
                        }
                        if y >= radius {
                            if let Some(leaving) = rows.get(y - radius) {
                                for (sum, v) in sums.iter_mut().zip(span(leaving, x0, x1)) {
                                    *sum -= f64::from(*v);
                                }
                            }
                        }
                        if let Some(entering) = rows.get(y + radius + 1) {
                            for (sum, v) in sums.iter_mut().zip(span(entering, x0, x1)) {
                                *sum += f64::from(*v);
                            }
                        }
                    }
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Three passes along the rows and then three down the columns is the alternating filter,
    /// to within a rounding, on frames narrower and wider than a strip and at every radius.
    #[test]
    fn rows_then_columns_is_the_alternating_blur() {
        for (w, h) in [(1, 7), (31, 40), (33, 65), (97, 130), (300, 41)] {
            let values: Vec<f32> = (0..w * h)
                .map(|i| ((i * 7919 + 13) % 1009) as f32 / 1009.0 + (i % 3) as f32 * 0.25)
                .collect();
            for radius in [1, 2, 9, 40, 400] {
                let mut alternating = values.clone();
                let mut scratch = vec![0.0f32; w * h];
                for _ in 0..BOX_PASSES {
                    box_pass_h(&alternating, &mut scratch, w, radius);
                    box_pass_v(&scratch, &mut alternating, w, h, radius);
                }
                let fused = blur(&values, w, h, radius);
                for (a, b) in fused.iter().zip(&alternating) {
                    assert!((a - b).abs() <= 2e-6, "{w}x{h} r {radius}: {a} vs {b}");
                }
            }
        }
    }

    /// The vertical pass as it was before the bands were cut into strips: one task a band.
    #[allow(clippy::indexing_slicing)]
    fn banded_reference(src: &[f32], width: usize, height: usize, radius: usize) -> Vec<f32> {
        let band = (radius * 4).max(64);
        let rows: Vec<&[f32]> = src.chunks_exact(width).collect();
        let mut dst = vec![0.0f32; width * height];
        for (index, out) in dst.chunks_mut(width * band).enumerate() {
            let first = index * band;
            let mut sums = vec![0.0f64; width];
            for row in rows
                .iter()
                .take((first + radius + 1).min(height))
                .skip(first.saturating_sub(radius))
            {
                for (sum, v) in sums.iter_mut().zip(row.iter()) {
                    *sum += f64::from(*v);
                }
            }
            for (offset, line) in out.chunks_exact_mut(width).enumerate() {
                let y = first + offset;
                let count = ((y + radius).min(height - 1) - y.saturating_sub(radius) + 1) as f64;
                for (slot, sum) in line.iter_mut().zip(sums.iter()) {
                    *slot = (sum / count) as f32;
                }
                if y >= radius {
                    for (sum, v) in sums.iter_mut().zip(rows[y - radius]) {
                        *sum -= f64::from(*v);
                    }
                }
                if let Some(entering) = rows.get(y + radius + 1) {
                    for (sum, v) in sums.iter_mut().zip(entering.iter()) {
                        *sum += f64::from(*v);
                    }
                }
            }
        }
        dst
    }

    /// Cutting bands into column strips changes which core sums a column, never the sums.
    #[test]
    fn column_strips_give_exactly_the_banded_result() {
        for (w, h) in [(1, 9), (127, 300), (128, 70), (129, 65), (411, 523)] {
            let values: Vec<f32> = (0..w * h)
                .map(|i| ((i * 7919 + 13) % 1009) as f32 / 1009.0 * 1e-3 + (i % 5) as f32)
                .collect();
            for radius in [1, 3, 17, 64, 240] {
                let mut fast = vec![0.0f32; w * h];
                box_pass_v(&values, &mut fast, w, h, radius);
                let slow = banded_reference(&values, w, h, radius);
                assert!(
                    fast.iter()
                        .zip(&slow)
                        .all(|(a, b)| a.to_bits() == b.to_bits()),
                    "{w}x{h} radius {radius}"
                );
            }
        }
    }

    /// The running-sum passes give the same windows as summing every sample of each one.
    #[test]
    fn running_sums_match_the_window_means_they_replace() {
        let (w, h) = (37, 23);
        let values: Vec<f32> = (0..w * h)
            .map(|i| ((i * 7919) % 101) as f32 / 101.0)
            .collect();
        for radius in [0, 1, 4, 30] {
            let mut naive = values.clone();
            for _ in 0..BOX_PASSES {
                let mut next = vec![0.0f32; w * h];
                for y in 0..h {
                    for x in 0..w {
                        let (lo, hi) = (x.saturating_sub(radius), (x + radius).min(w - 1));
                        next[y * w + x] =
                            (lo..=hi).map(|i| naive[y * w + i]).sum::<f32>() / (hi - lo + 1) as f32;
                    }
                }
                for x in 0..w {
                    for y in 0..h {
                        let (lo, hi) = (y.saturating_sub(radius), (y + radius).min(h - 1));
                        naive[y * w + x] =
                            (lo..=hi).map(|i| next[i * w + x]).sum::<f32>() / (hi - lo + 1) as f32;
                    }
                }
            }
            let fast = blur(&values, w, h, radius);
            for (a, b) in fast.iter().zip(&naive) {
                assert!((a - b).abs() < 1e-5, "radius {radius}: {a} vs {b}");
            }
        }
    }

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

    #[test]
    #[ignore = "timing, by hand"]
    #[allow(clippy::disallowed_methods, clippy::print_stdout)]
    fn pass_timing() {
        let (w, h) = (1400_usize, 1600_usize);
        let values: Vec<f32> = (0..w * h)
            .map(|i| ((i * 7919) % 1009) as f32 / 1009.0)
            .collect();
        let mut out = vec![0.0f32; w * h];
        for radius in [5_usize, 18, 60, 216, 432] {
            let t = std::time::Instant::now();
            for _ in 0..10 {
                box_pass_h(&values, &mut out, w, radius);
            }
            let horizontal = t.elapsed() / 10;
            let t = std::time::Instant::now();
            for _ in 0..10 {
                box_pass_v(&values, &mut out, w, h, radius);
            }
            let vertical = t.elapsed() / 10;
            let t = std::time::Instant::now();
            for _ in 0..10 {
                let _ = blur(&values, w, h, radius);
            }
            let blurred = t.elapsed() / 10;
            println!("r {radius}: h {horizontal:?} v {vertical:?} blur {blurred:?}");
        }
    }
}
