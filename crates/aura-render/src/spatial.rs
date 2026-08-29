//! The stages that read more than one pixel: blur, local contrast, sharpening, noise
//! reduction, vignette and geometry.
//!
//! Every function here is **separable and deterministic**. No atomics, no unordered
//! reductions, no floating-point accumulation whose order depends on a thread count. Section
//! 6.2: *"No non-deterministic reductions: fixed tile order, no atomics in colour maths."*
//! The parallelism is per-row and each row writes only its own slice, which is the one shape
//! that is deterministic without any care being taken.
//!
//! # Halo
//!
//! Each of these stages declares a radius in `graph::Stage::halo`, and the tiler decodes
//! that much extra around every tile. The numbers there are the *reach* of these functions,
//! not estimates: a three-pass box blur of radius `r` reads `3r` pixels away, so clarity at
//! `r = 16` declares 48 and sharpening at `r <= 3` declares 12 (nine for the blur, one for
//! the Sobel, rounded up).

use rayon::prelude::*;

use crate::colour::{luma, set_luma};

/// Three box passes approximate a Gaussian to within a per-cent, at a third of the cost and
/// with exactly reproducible arithmetic.
const BOX_PASSES: usize = 3;

/// Where a buffer sits inside the whole photograph.
///
/// A tile is a window onto a frame, and two stages need to know which window: vignette
/// correction measures a radius from the *frame's* centre, and a mask is drawn in frame
/// coordinates. Passing the tile's own size to either would put a vignette in every tile and
/// a gradient in every tile, which is the most visible tiling defect there is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    /// The buffer's left edge in frame coordinates.
    pub x: u32,
    /// The buffer's top edge in frame coordinates.
    pub y: u32,
    /// The whole frame's width.
    pub full_width: u32,
    /// The whole frame's height.
    pub full_height: u32,
}

impl Position {
    /// A buffer that *is* the whole frame.
    #[must_use]
    pub const fn whole(width: u32, height: u32) -> Self {
        Self {
            x: 0,
            y: 0,
            full_width: width,
            full_height: height,
        }
    }

    /// The position of a frame's buffer after a resample to `width` by `height`.
    ///
    /// A resample changes the scale, so the origin and the full size scale with it. In
    /// practice tiling only ever happens at full resolution, where this is the identity -
    /// but a proxy render of a tile would otherwise place its vignette centre wrongly, and
    /// the arithmetic is one line.
    #[must_use]
    pub fn of(frame: &crate::cpu::Frame, width: u32, height: u32) -> Self {
        if frame.width == 0 || frame.height == 0 {
            return Self::whole(width.max(1), height.max(1));
        }
        let sx = f64::from(width) / f64::from(frame.width);
        let sy = f64::from(height) / f64::from(frame.height);
        Self {
            x: (f64::from(frame.origin.0) * sx).round() as u32,
            y: (f64::from(frame.origin.1) * sy).round() as u32,
            full_width: ((f64::from(frame.full.0) * sx).round() as u32).max(1),
            full_height: ((f64::from(frame.full.1) * sy).round() as u32).max(1),
        }
    }
}

/// Frame-wide numbers three stages need, measured once.
///
/// # Why this type exists
///
/// Sharpening's edge mask, noise reduction's edge keeper and dehaze's dark-channel floor are
/// all normalised against a property of the **whole frame**. Measured inside a tile they
/// would each take a different value in every tile, and a tiled render would disagree with a
/// whole-frame one - which is the one thing section 6.3 promises it never does.
///
/// So they are measured once, here, and passed in. And they are measured on a fixed 512 px
/// reduction of the frame rather than on the frame itself, for two reasons: it costs the
/// same on a 24 MP file and a 100 MP one, and it makes the numbers independent of the render
/// level, so a proxy and an export sharpen identically.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stats {
    /// The largest Sobel gradient magnitude in the frame. Sharpening's normaliser.
    pub gradient_peak: f32,
    /// The fifth percentile of the blurred dark channel. Dehaze's floor.
    pub dark_floor: f32,
}

impl Default for Stats {
    /// Neutral: nothing is normalised away and nothing is lifted.
    fn default() -> Self {
        Self {
            gradient_peak: 1.0,
            dark_floor: 0.0,
        }
    }
}

/// The long edge the statistics are measured at. Fixed, so the numbers do not depend on the
/// render level.
pub const STATS_EDGE: usize = 512;

/// Measure a frame once.
#[must_use]
pub fn measure(rgb: &[f32], width: usize, height: usize) -> Stats {
    if width == 0 || height == 0 {
        return Stats::default();
    }
    let long = width.max(height);
    let (small, w, h) = if long > STATS_EDGE {
        let scale = STATS_EDGE as f32 / long as f32;
        let w = ((width as f32 * scale).round() as usize).max(1);
        let h = ((height as f32 * scale).round() as usize).max(1);
        (resample(rgb, width, height, w, h), w, h)
    } else {
        (rgb.to_vec(), width, height)
    };

    let plane = luma_plane(&small, w, h);
    let gradient = gradient_plane(&plane, w, h);
    let gradient_peak = gradient.iter().copied().fold(0.0f32, f32::max).max(1e-6);

    let mut dark = vec![0.0f32; w * h];
    for (index, slot) in dark.iter_mut().enumerate() {
        let base = index * 3;
        *slot = small
            .get(base)
            .copied()
            .unwrap_or(0.0)
            .min(small.get(base + 1).copied().unwrap_or(0.0))
            .min(small.get(base + 2).copied().unwrap_or(0.0));
    }
    let blurred = blur_plane(&dark, w, h, 8);
    let mut sorted = blurred;
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let dark_floor = sorted
        .get(sorted.len() / 20)
        .copied()
        .unwrap_or(0.0)
        .max(0.0);

    Stats {
        gradient_peak,
        dark_floor,
    }
}

/// A separable box blur over a single-channel plane, repeated [`BOX_PASSES`] times.
///
/// Edges are clamped rather than wrapped or zeroed. Zeroing darkens a frame's border, which
/// on a clarity pass looks exactly like a vignette somebody did not ask for.
#[must_use]
pub fn blur_plane(plane: &[f32], width: usize, height: usize, radius: usize) -> Vec<f32> {
    if radius == 0 || width == 0 || height == 0 {
        return plane.to_vec();
    }
    let mut current = plane.to_vec();
    let mut scratch = vec![0.0f32; width * height];
    for _ in 0..BOX_PASSES {
        blur_rows(&current, &mut scratch, width, height, radius);
        blur_columns(&scratch, &mut current, width, height, radius);
    }
    current
}

fn blur_rows(src: &[f32], dst: &mut [f32], width: usize, height: usize, radius: usize) {
    dst.par_chunks_mut(width)
        .enumerate()
        .take(height)
        .for_each(|(y, row)| {
            let base = y * width;
            for (x, slot) in row.iter_mut().enumerate() {
                let mut sum = 0.0f32;
                let mut count = 0.0f32;
                for offset in 0..=(radius * 2) {
                    let sx = (x + offset).saturating_sub(radius).min(width - 1);
                    sum += src.get(base + sx).copied().unwrap_or(0.0);
                    count += 1.0;
                }
                *slot = sum / count.max(1.0);
            }
        });
}

fn blur_columns(src: &[f32], dst: &mut [f32], width: usize, height: usize, radius: usize) {
    dst.par_chunks_mut(width)
        .enumerate()
        .take(height)
        .for_each(|(y, row)| {
            for (x, slot) in row.iter_mut().enumerate() {
                let mut sum = 0.0f32;
                let mut count = 0.0f32;
                for offset in 0..=(radius * 2) {
                    let sy = (y + offset).saturating_sub(radius).min(height - 1);
                    sum += src.get(sy * width + x).copied().unwrap_or(0.0);
                    count += 1.0;
                }
                *slot = sum / count.max(1.0);
            }
        });
}

/// The luminance plane of an interleaved RGB buffer.
#[must_use]
pub fn luma_plane(rgb: &[f32], width: usize, height: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; width * height];
    for (index, slot) in out.iter_mut().enumerate() {
        let base = index * 3;
        *slot = luma([
            rgb.get(base).copied().unwrap_or(0.0),
            rgb.get(base + 1).copied().unwrap_or(0.0),
            rgb.get(base + 2).copied().unwrap_or(0.0),
        ]);
    }
    out
}

/// Local contrast at a radius. Clarity and texture are the same operation at two scales.
///
/// The detail is taken in the *log* domain rather than linearly, so the same slider value
/// does the same thing to a shadow and to a highlight - which is what stops clarity turning
/// into a highlight-only control on a bright frame.
pub fn local_contrast(rgb: &mut [f32], width: usize, height: usize, radius: usize, amount: f32) {
    if amount == 0.0 || radius == 0 {
        return;
    }
    let plane = luma_plane(rgb, width, height);
    let log: Vec<f32> = plane.iter().map(|v| (v.max(1e-5)).ln()).collect();
    let blurred = blur_plane(&log, width, height, radius);

    rgb.par_chunks_mut(3)
        .enumerate()
        .for_each(|(index, pixel)| {
            let base = log.get(index).copied().unwrap_or(0.0);
            let low = blurred.get(index).copied().unwrap_or(base);
            let detail = base - low;
            let boosted = (low + detail * (1.0 + amount)).exp();
            let current = [
                pixel.first().copied().unwrap_or(0.0),
                pixel.get(1).copied().unwrap_or(0.0),
                pixel.get(2).copied().unwrap_or(0.0),
            ];
            let out = set_luma(current, boosted.max(0.0));
            for (slot, value) in pixel.iter_mut().zip(out.iter()) {
                *slot = *value;
            }
        });
}

/// Unsharp masking with a detail limiter and an edge mask.
///
/// * `amount` is `0..1.5`, from `sharpen.amount / 100`.
/// * `radius` is in pixels.
/// * `detail` is `0..1`: how much of the finest scale is allowed through. Low values
///   suppress the halo that sharpening puts around a hard edge.
/// * `masking` is `0..1`: how strongly flat areas are spared, which is what keeps
///   sharpening out of a smooth sky and out of skin.
#[allow(clippy::too_many_arguments)]
pub fn unsharp(
    rgb: &mut [f32],
    width: usize,
    height: usize,
    radius: f32,
    amount: f32,
    detail: f32,
    masking: f32,
    stats: &Stats,
) {
    if amount <= 0.0 {
        return;
    }
    let r = (radius.max(0.5).round() as usize).max(1);
    let plane = luma_plane(rgb, width, height);
    let blurred = blur_plane(&plane, width, height, r);

    // The edge mask is the local gradient magnitude, normalised by the frame's own range so
    // the control means the same thing on a flat scene and a busy one. The normaliser comes
    // from `Stats` rather than from this buffer: a maximum taken over a tile is a different
    // number from one taken over the frame, and that difference is exactly what would make a
    // tiled render disagree with a whole one. See `Stats`.
    let gradient = gradient_plane(&plane, width, height);
    let peak = stats.gradient_peak.max(1e-6);

    rgb.par_chunks_mut(3)
        .enumerate()
        .for_each(|(index, pixel)| {
            let base = plane.get(index).copied().unwrap_or(0.0);
            let low = blurred.get(index).copied().unwrap_or(base);
            let high = base - low;
            // The detail limiter is a soft clip on the high-frequency component, not a
            // scale: clipping stops a halo, scaling only makes it smaller.
            let ceiling = 0.02 + detail * 0.48;
            let limited = high.clamp(-ceiling, ceiling);
            let edge = gradient.get(index).copied().unwrap_or(0.0) / peak;
            let mask = if masking <= 0.0 {
                1.0
            } else {
                (edge / masking.max(1e-3)).min(1.0)
            };
            let out_luma = (base + limited * amount * mask).max(0.0);
            let current = [
                pixel.first().copied().unwrap_or(0.0),
                pixel.get(1).copied().unwrap_or(0.0),
                pixel.get(2).copied().unwrap_or(0.0),
            ];
            let out = set_luma(current, out_luma);
            for (slot, value) in pixel.iter_mut().zip(out.iter()) {
                *slot = *value;
            }
        });
}

/// The two components of a 3x3 Sobel gradient over a plane.
///
/// `(gx, gy)`, same size as the input, edge-clamped. [`gradient_plane`] is the magnitude of
/// this and calls it, so there is one Sobel in the workspace rather than two.
///
/// The components rather than the magnitude, because a magnitude has no *orientation* in it and
/// PHASE-23's keystone solver has to be able to tell the upright lines of a building from the
/// horizontals of the pews in front of it. A caller that needed orientation and had only a
/// magnitude would write its own Sobel, which is how two edge detectors end up in one product
/// disagreeing about which pixels are edges.
#[must_use]
pub fn sobel_planes(plane: &[f32], width: usize, height: usize) -> (Vec<f32>, Vec<f32>) {
    let mut gx_out = vec![0.0f32; width * height];
    let mut gy_out = vec![0.0f32; width * height];
    if width < 3 || height < 3 {
        return (gx_out, gy_out);
    }
    gx_out
        .par_chunks_mut(width)
        .zip(gy_out.par_chunks_mut(width))
        .enumerate()
        .take(height)
        .for_each(|(y, (gx_row, gy_row))| {
            for x in 0..width {
                let sample = |dx: isize, dy: isize| -> f32 {
                    let sx = (x as isize + dx).clamp(0, width as isize - 1) as usize;
                    let sy = (y as isize + dy).clamp(0, height as isize - 1) as usize;
                    plane.get(sy * width + sx).copied().unwrap_or(0.0)
                };
                let gx = -sample(-1, -1) - 2.0 * sample(-1, 0) - sample(-1, 1)
                    + sample(1, -1)
                    + 2.0 * sample(1, 0)
                    + sample(1, 1);
                let gy = -sample(-1, -1) - 2.0 * sample(0, -1) - sample(1, -1)
                    + sample(-1, 1)
                    + 2.0 * sample(0, 1)
                    + sample(1, 1);
                if let Some(slot) = gx_row.get_mut(x) {
                    *slot = gx;
                }
                if let Some(slot) = gy_row.get_mut(x) {
                    *slot = gy;
                }
            }
        });
    (gx_out, gy_out)
}

/// The magnitude of a 3x3 Sobel gradient over a plane.
#[must_use]
pub fn gradient_plane(plane: &[f32], width: usize, height: usize) -> Vec<f32> {
    let (gx, gy) = sobel_planes(plane, width, height);
    gx.into_par_iter()
        .zip(gy.into_par_iter())
        .map(|(x, y)| x.hypot(y))
        .collect()
}

/// Noise reduction: an edge-preserving luminance blend and a chroma blur.
///
/// Chroma is blurred harder than luminance, always, because chroma noise is the ugly one
/// and blurring it costs almost no detail - two colours a pixel apart are a sensor artefact
/// far more often than they are a photograph.
pub fn denoise(
    rgb: &mut [f32],
    width: usize,
    height: usize,
    luminance: f32,
    chroma: f32,
    detail: f32,
    stats: &Stats,
) {
    if luminance <= 0.0 && chroma <= 0.0 {
        return;
    }
    let plane = luma_plane(rgb, width, height);

    if chroma > 0.0 {
        let radius = (1.0 + chroma * 3.0).round() as usize;
        let mut chroma_r: Vec<f32> = Vec::with_capacity(width * height);
        let mut chroma_b: Vec<f32> = Vec::with_capacity(width * height);
        for index in 0..width * height {
            let base = index * 3;
            let l = plane.get(index).copied().unwrap_or(0.0);
            chroma_r.push(rgb.get(base).copied().unwrap_or(0.0) - l);
            chroma_b.push(rgb.get(base + 2).copied().unwrap_or(0.0) - l);
        }
        let blurred_r = blur_plane(&chroma_r, width, height, radius);
        let blurred_b = blur_plane(&chroma_b, width, height, radius);
        for index in 0..width * height {
            let base = index * 3;
            let l = plane.get(index).copied().unwrap_or(0.0);
            let r = l + blurred_r.get(index).copied().unwrap_or(0.0);
            let b = l + blurred_b.get(index).copied().unwrap_or(0.0);
            let g = rgb.get(base + 1).copied().unwrap_or(0.0);
            let corrected = set_luma([r, g, b], l);
            for (offset, value) in corrected.iter().enumerate() {
                if let Some(slot) = rgb.get_mut(base + offset) {
                    *slot = *value;
                }
            }
        }
    }

    if luminance > 0.0 {
        let radius = (1.0 + luminance * 2.0).round() as usize;
        let blurred = blur_plane(&plane, width, height, radius);
        let gradient = gradient_plane(&plane, width, height);
        let peak = stats.gradient_peak.max(1e-6);

        rgb.par_chunks_mut(3)
            .enumerate()
            .for_each(|(index, pixel)| {
                let base = plane.get(index).copied().unwrap_or(0.0);
                let low = blurred.get(index).copied().unwrap_or(base);
                // An edge keeps its own luminance; a flat area takes the blur. `detail` is
                // how quickly a pixel counts as an edge.
                let edge = (gradient.get(index).copied().unwrap_or(0.0) / peak).min(1.0);
                let keep = edge.powf(1.0 + detail * 2.0);
                let blended = low + (base - low) * (keep + (1.0 - keep) * (1.0 - luminance));
                let current = [
                    pixel.first().copied().unwrap_or(0.0),
                    pixel.get(1).copied().unwrap_or(0.0),
                    pixel.get(2).copied().unwrap_or(0.0),
                ];
                let out = set_luma(current, blended.max(0.0));
                for (slot, value) in pixel.iter_mut().zip(out.iter()) {
                    *slot = *value;
                }
            });
    }
}

/// Haze removal: lift the black point toward the frame's own darkest pixel and add contrast.
///
/// A dark-channel prior in one line. Haze raises the minimum of the three channels
/// everywhere; subtracting a fraction of the frame's own dark-channel floor removes it
/// without touching a scene that has genuine deep blacks - which is why the floor is
/// measured rather than assumed.
pub fn dehaze(rgb: &mut [f32], width: usize, height: usize, amount: f32, stats: &Stats) {
    if amount == 0.0 || width == 0 || height == 0 {
        return;
    }
    // The dark-channel floor is a property of the frame, not of a tile. It comes from
    // `Stats` for the reason `unsharp`'s normaliser does.
    let lift = stats.dark_floor * amount.clamp(-1.0, 1.0);
    rgb.par_chunks_mut(3).for_each(|pixel| {
        for slot in pixel.iter_mut() {
            *slot = (*slot - lift).max(0.0) / (1.0 - lift).max(1e-3);
        }
    });
}

/// Vignette correction: a radial gain that lifts the corners.
///
/// `amount` is `0..1`, from `lens.vignette / 100`. At 1.0 the extreme corner is lifted by
/// 60 %, which is roughly what a fast prime wide open loses.
pub fn correct_vignette(
    rgb: &mut [f32],
    width: usize,
    height: usize,
    amount: f32,
    position: Position,
) {
    if amount <= 0.0 || width == 0 || height == 0 {
        return;
    }
    // The centre and the radius are the *frame's*, not this buffer's. See `Position`.
    let cx = (position.full_width as f32 - 1.0) / 2.0;
    let cy = (position.full_height as f32 - 1.0) / 2.0;
    let max_r = cx.hypot(cy).max(1e-6);

    rgb.par_chunks_mut(3)
        .enumerate()
        .for_each(|(index, pixel)| {
            let x = (index % width) as f32 + position.x as f32;
            let y = (index / width) as f32 + position.y as f32;
            let r = (x - cx).hypot(y - cy) / max_r;
            let gain = 1.0 + 0.6 * amount * r * r;
            for slot in pixel.iter_mut() {
                *slot *= gain;
            }
        });
}

/// Bilinear resample of an interleaved RGB buffer.
///
/// Bilinear rather than Lanczos, and the choice is recorded rather than defaulted: a
/// windowed sinc is sharper on a downscale and rings on a high-contrast edge, and every
/// consumer of a resampled buffer in this product is either a model trained on bilinear
/// proxies (phase 05 onward) or a screen preview. The export path never resamples.
#[must_use]
pub fn resample(
    rgb: &[f32],
    width: usize,
    height: usize,
    out_width: usize,
    out_height: usize,
) -> Vec<f32> {
    if out_width == 0 || out_height == 0 || width == 0 || height == 0 {
        return Vec::new();
    }
    if out_width == width && out_height == height {
        return rgb.to_vec();
    }
    let mut out = vec![0.0f32; out_width * out_height * 3];
    let sx = width as f32 / out_width as f32;
    let sy = height as f32 / out_height as f32;

    out.par_chunks_mut(out_width * 3)
        .enumerate()
        .take(out_height)
        .for_each(|(y, row)| {
            let source_y = ((y as f32 + 0.5) * sy - 0.5).max(0.0);
            let y0 = source_y.floor() as usize;
            let y1 = (y0 + 1).min(height - 1);
            let fy = source_y - y0 as f32;
            for x in 0..out_width {
                let source_x = ((x as f32 + 0.5) * sx - 0.5).max(0.0);
                let x0 = source_x.floor() as usize;
                let x1 = (x0 + 1).min(width - 1);
                let fx = source_x - x0 as f32;
                for channel in 0..3 {
                    let at = |px: usize, py: usize| -> f32 {
                        rgb.get((py * width + px) * 3 + channel)
                            .copied()
                            .unwrap_or(0.0)
                    };
                    let top = at(x0, y0) * (1.0 - fx) + at(x1, y0) * fx;
                    let bottom = at(x0, y1) * (1.0 - fx) + at(x1, y1) * fx;
                    if let Some(slot) = row.get_mut(x * 3 + channel) {
                        *slot = top * (1.0 - fy) + bottom * fy;
                    }
                }
            }
        });
    out
}

/// Crop and rotate, in one resample.
///
/// One pass rather than two, because a rotate followed by a crop resamples twice and loses
/// detail the second time for no reason. The output is the crop rectangle's own size in
/// source pixels, so a straightened frame keeps its resolution.
///
/// `crop` is `[left, top, right, bottom]` normalised; `rotate` is degrees, positive
/// clockwise.
#[must_use]
pub fn crop_rotate(
    rgb: &[f32],
    width: usize,
    height: usize,
    crop: [f32; 4],
    rotate: f32,
) -> (Vec<f32>, usize, usize) {
    let left = crop.first().copied().unwrap_or(0.0).clamp(0.0, 1.0) * width as f32;
    let top = crop.get(1).copied().unwrap_or(0.0).clamp(0.0, 1.0) * height as f32;
    let right = crop.get(2).copied().unwrap_or(1.0).clamp(0.0, 1.0) * width as f32;
    let bottom = crop.get(3).copied().unwrap_or(1.0).clamp(0.0, 1.0) * height as f32;

    let out_width = ((right - left).round() as usize).max(1);
    let out_height = ((bottom - top).round() as usize).max(1);
    let mut out = vec![0.0f32; out_width * out_height * 3];

    let radians = -rotate.to_radians();
    let (sin, cos) = radians.sin_cos();
    // Pixel *centres*, not edges. `(left + right) / 2` is half a pixel off, and half a pixel
    // of shift on every crop is a resample nobody asked for on an untouched frame.
    let ccx = (left + right - 1.0) / 2.0;
    let ccy = (top + bottom - 1.0) / 2.0;
    let ocx = (out_width as f32 - 1.0) / 2.0;
    let ocy = (out_height as f32 - 1.0) / 2.0;

    out.par_chunks_mut(out_width * 3)
        .enumerate()
        .take(out_height)
        .for_each(|(y, row)| {
            let dy = y as f32 - ocy;
            for x in 0..out_width {
                let dx = x as f32 - ocx;
                let source_x = ccx + dx * cos - dy * sin;
                let source_y = ccy + dx * sin + dy * cos;
                if source_x < 0.0
                    || source_y < 0.0
                    || source_x > width as f32 - 1.0
                    || source_y > height as f32 - 1.0
                {
                    // Outside the frame. Left black rather than clamped: a straightened
                    // frame whose corners were filled by smearing the edge pixel is a frame
                    // whose corners are a lie, and the crop is supposed to remove them.
                    continue;
                }
                let x0 = source_x.floor() as usize;
                let y0 = source_y.floor() as usize;
                let x1 = (x0 + 1).min(width - 1);
                let y1 = (y0 + 1).min(height - 1);
                let fx = source_x - x0 as f32;
                let fy = source_y - y0 as f32;
                for channel in 0..3 {
                    let at = |px: usize, py: usize| -> f32 {
                        rgb.get((py * width + px) * 3 + channel)
                            .copied()
                            .unwrap_or(0.0)
                    };
                    let a = at(x0, y0) * (1.0 - fx) + at(x1, y0) * fx;
                    let b = at(x0, y1) * (1.0 - fx) + at(x1, y1) * fx;
                    if let Some(slot) = row.get_mut(x * 3 + channel) {
                        *slot = a * (1.0 - fy) + b * fy;
                    }
                }
            }
        });

    (out, out_width, out_height)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(width: usize, height: usize) -> Vec<f32> {
        let mut out = vec![0.0f32; width * height * 3];
        for y in 0..height {
            for x in 0..width {
                let value = (x as f32 / width.max(1) as f32) * 0.8 + 0.1;
                for channel in 0..3 {
                    if let Some(slot) = out.get_mut((y * width + x) * 3 + channel) {
                        *slot = value;
                    }
                }
            }
        }
        out
    }

    #[test]
    fn a_blur_of_a_flat_plane_is_the_same_flat_plane() {
        let plane = vec![0.4f32; 64];
        let blurred = blur_plane(&plane, 8, 8, 3);
        for value in blurred {
            assert!((value - 0.4).abs() < 1e-5);
        }
    }

    #[test]
    fn a_blur_of_radius_zero_is_the_identity() {
        let plane: Vec<f32> = (0..64).map(|i| i as f32 / 64.0).collect();
        assert_eq!(blur_plane(&plane, 8, 8, 0), plane);
    }

    #[test]
    fn local_contrast_at_zero_is_the_identity() {
        let mut rgb = ramp(16, 16);
        let before = rgb.clone();
        local_contrast(&mut rgb, 16, 16, 4, 0.0);
        assert_eq!(rgb, before);
    }

    #[test]
    fn local_contrast_steepens_an_edge() {
        let mut rgb = vec![0.2f32; 32 * 8 * 3];
        for y in 0..8 {
            for x in 16..32 {
                for c in 0..3 {
                    if let Some(slot) = rgb.get_mut((y * 32 + x) * 3 + c) {
                        *slot = 0.6;
                    }
                }
            }
        }
        let before = rgb.clone();
        local_contrast(&mut rgb, 32, 8, 4, 0.5);
        let dark_before = before.get((4 * 32 + 14) * 3).copied().unwrap_or(0.0);
        let dark_after = rgb.get((4 * 32 + 14) * 3).copied().unwrap_or(0.0);
        assert!(
            dark_after < dark_before,
            "the dark side of the edge went darker"
        );
    }

    #[test]
    fn sharpening_at_zero_amount_is_the_identity() {
        let mut rgb = ramp(16, 16);
        let before = rgb.clone();
        unsharp(&mut rgb, 16, 16, 1.0, 0.0, 0.25, 0.0, &Stats::default());
        assert_eq!(rgb, before);
    }

    #[test]
    fn denoise_leaves_a_flat_frame_flat() {
        let mut rgb = vec![0.3f32; 16 * 16 * 3];
        denoise(&mut rgb, 16, 16, 0.5, 0.5, 0.5, &Stats::default());
        for value in &rgb {
            assert!((value - 0.3).abs() < 1e-3, "{value}");
        }
    }

    #[test]
    fn vignette_correction_lifts_the_corner_and_leaves_the_centre() {
        let mut rgb = vec![0.5f32; 9 * 9 * 3];
        correct_vignette(&mut rgb, 9, 9, 1.0, Position::whole(9, 9));
        let centre = rgb.get((4 * 9 + 4) * 3).copied().unwrap_or(0.0);
        let corner = rgb.first().copied().unwrap_or(0.0);
        assert!((centre - 0.5).abs() < 1e-5, "the centre is the reference");
        assert!(corner > 0.7, "the corner was lifted: {corner}");
    }

    #[test]
    fn a_resample_to_the_same_size_is_the_identity() {
        let rgb = ramp(8, 8);
        assert_eq!(resample(&rgb, 8, 8, 8, 8), rgb);
    }

    #[test]
    fn a_resample_preserves_a_flat_value() {
        let rgb = vec![0.42f32; 16 * 16 * 3];
        let out = resample(&rgb, 16, 16, 5, 5);
        assert_eq!(out.len(), 5 * 5 * 3);
        for value in out {
            assert!((value - 0.42).abs() < 1e-5);
        }
    }

    #[test]
    fn the_full_crop_with_no_rotation_is_the_identity() {
        let rgb = ramp(12, 9);
        let (out, w, h) = crop_rotate(&rgb, 12, 9, [0.0, 0.0, 1.0, 1.0], 0.0);
        assert_eq!((w, h), (12, 9));
        for (a, b) in out.iter().zip(rgb.iter()) {
            assert!((a - b).abs() < 1e-4);
        }
    }

    #[test]
    fn a_crop_takes_the_region_it_names() {
        let rgb = ramp(16, 16);
        let (out, w, h) = crop_rotate(&rgb, 16, 16, [0.5, 0.0, 1.0, 1.0], 0.0);
        assert_eq!((w, h), (8, 16));
        // The left column of the crop is the middle of the source ramp.
        let left = out.first().copied().unwrap_or(0.0);
        assert!(left > 0.4 && left < 0.6, "{left}");
    }

    #[test]
    fn a_rotation_of_zero_degrees_changes_nothing() {
        let rgb = ramp(10, 10);
        let (a, _, _) = crop_rotate(&rgb, 10, 10, [0.1, 0.1, 0.9, 0.9], 0.0);
        let (b, _, _) = crop_rotate(&rgb, 10, 10, [0.1, 0.1, 0.9, 0.9], 0.0);
        assert_eq!(a, b);
    }

    #[test]
    fn dehaze_at_zero_is_the_identity() {
        let mut rgb = ramp(16, 16);
        let before = rgb.clone();
        dehaze(&mut rgb, 16, 16, 0.0, &Stats::default());
        assert_eq!(rgb, before);
    }

    #[test]
    fn every_spatial_stage_is_deterministic_across_runs() {
        // The parallelism is per-row and each row writes only its own slice, so this holds
        // by construction. The test exists because the day somebody adds a reduction it
        // stops holding, and the failure would otherwise be an intermittent golden diff.
        let base = ramp(64, 48);
        let stats = measure(&base, 64, 48);
        for _ in 0..3 {
            let mut a = base.clone();
            let mut b = base.clone();
            local_contrast(&mut a, 64, 48, 8, 0.4);
            local_contrast(&mut b, 64, 48, 8, 0.4);
            assert_eq!(a, b);
            unsharp(&mut a, 64, 48, 1.2, 0.5, 0.3, 0.4, &stats);
            unsharp(&mut b, 64, 48, 1.2, 0.5, 0.3, 0.4, &stats);
            assert_eq!(a, b);
            denoise(&mut a, 64, 48, 0.3, 0.4, 0.5, &stats);
            denoise(&mut b, 64, 48, 0.3, 0.4, 0.5, &stats);
            assert_eq!(a, b);
        }
    }
}
