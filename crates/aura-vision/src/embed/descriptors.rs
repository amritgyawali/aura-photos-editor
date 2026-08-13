//! The cheap descriptors, and the resampling everything else in the crate uses.
//!
//! Section 6.2 calls these "descriptors that earn their keep", and the argument
//! is about *when* they are computed rather than whether. Every one of them is a
//! single pass over a buffer that is already decoded and already resident. Asking
//! for them later means opening the file again, which is the cost this whole
//! phase exists to avoid.
//!
//! What each one is for, so that a later phase does not recompute it:
//!
//! * the **HSV histogram** is what phases 25 and 26 compare for gallery colour
//!   consistency and multi-camera matching;
//! * the **luminance percentiles** are phase 15's starting point for exposure and
//!   phase 09's cheap clipping check;
//! * the **edge energy** is a free global sharpness prior that phase 09 refines
//!   with a real model;
//! * the **palette** is what phase 29 lays an album out with.
//!
//! All of them are computed on the 8-bit sRGB proxy, not on linear light. That is
//! deliberate and is the one place in the product where it is correct: these are
//! perceptual summaries meant to match what a photographer sees on a screen, and
//! a histogram of linear values puts nine tenths of a normally exposed frame in
//! the bottom two bins. Invariant 8 governs colour *decisions*; this is a
//! description, and it says so.

use aura_index::contract::index::{LumaStats, HSV_BINS};
use aura_index::store::{Descriptors, PALETTE_LEN};

/// Which implementation of this module produced a row. Stored per row; a change
/// here is a version bump and a re-describe, exactly as a model change is.
pub const DESCRIPTOR_VER: u16 = 1;

/// Bins per axis in the HSV histogram. Eight cubed is 512.
pub const HSV_AXIS: usize = 8;

/// Bins per axis when quantising for the dominant-colour palette.
pub const PALETTE_AXIS: usize = 4;

/// Edge of the greyscale plane the edge-energy summary is measured on.
///
/// Measuring gradients at full proxy resolution would make the number a function
/// of how big the sensor is rather than of how sharp the photograph is. A fixed
/// 128 px plane makes two cameras comparable, which is the whole point of a
/// global prior.
pub const EDGE_PLANE: usize = 128;

/// Below this, a pixel counts as crushed.
const BLACK_POINT: f32 = 2.0 / 255.0;

/// Above this, a pixel counts as blown.
const WHITE_POINT: f32 = 253.0 / 255.0;

/// The luminance plane, kept so that the hash and the edge summary do not each
/// convert the image again.
#[derive(Debug, Clone, PartialEq)]
pub struct LumaPlane {
    /// Row-major luminance in `0..1`.
    pub values: Vec<f32>,
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
}

/// Rec.709 luminance of one gamma-encoded sRGB triple.
///
/// Encoded rather than linearised on purpose: see this module's header.
#[must_use]
pub fn luminance(red: u8, green: u8, blue: u8) -> f32 {
    let r = f32::from(red) / 255.0;
    let g = f32::from(green) / 255.0;
    let b = f32::from(blue) / 255.0;
    0.2126f32.mul_add(r, 0.7152f32.mul_add(g, 0.0722 * b))
}

/// Extract the luminance plane from an interleaved 8-bit sRGB buffer.
#[must_use]
pub fn luma_plane(rgb: &[u8], width: usize, height: usize) -> LumaPlane {
    let mut values = Vec::with_capacity(width * height);
    for pixel in rgb.chunks_exact(3).take(width * height) {
        let (Some(red), Some(green), Some(blue)) = (pixel.first(), pixel.get(1), pixel.get(2))
        else {
            continue;
        };
        values.push(luminance(*red, *green, *blue));
    }
    values.resize(width * height, 0.0);
    LumaPlane {
        values,
        width,
        height,
    }
}

/// Box-filter resample of a single-channel plane.
///
/// An area average rather than bilinear sampling, because a wedding proxy going
/// to 384 px is a downscale of six or more and bilinear sampling of a downscale
/// aliases: it reads four source pixels out of thirty-six and calls the answer a
/// resize. Aliasing in an embedding input is not cosmetic - it makes the vector
/// depend on where the subject happened to fall relative to the sampling grid.
///
/// Deterministic: the accumulation order is row-major and fixed.
#[must_use]
pub fn box_resize(
    source: &[f32],
    width: usize,
    height: usize,
    target_width: usize,
    target_height: usize,
) -> Vec<f32> {
    if width == 0 || height == 0 || target_width == 0 || target_height == 0 {
        return vec![0.0; target_width * target_height];
    }
    let mut out = Vec::with_capacity(target_width * target_height);
    for row in 0..target_height {
        let top = row * height / target_height;
        let bottom = (((row + 1) * height) / target_height)
            .max(top + 1)
            .min(height);
        for column in 0..target_width {
            let left = column * width / target_width;
            let right = (((column + 1) * width) / target_width)
                .max(left + 1)
                .min(width);
            let mut total = 0.0f32;
            let mut count = 0u32;
            for y in top..bottom {
                for x in left..right {
                    total += source.get(y * width + x).copied().unwrap_or(0.0);
                    count += 1;
                }
            }
            out.push(if count == 0 {
                0.0
            } else {
                total / count as f32
            });
        }
    }
    out
}

/// Luminance statistics from a plane, using a 256-bin histogram.
///
/// A histogram rather than a sort: sorting four million values to find three
/// percentiles costs more than the decode did, and 8-bit source data has only
/// 256 distinct values anyway, so the histogram is exact rather than approximate.
#[must_use]
pub fn luma_stats(plane: &LumaPlane) -> LumaStats {
    let total = plane.values.len();
    if total == 0 {
        return LumaStats::zero();
    }
    let mut histogram = [0u32; 256];
    let mut sum = 0.0f64;
    let mut clipped_low = 0u32;
    let mut clipped_high = 0u32;
    for value in &plane.values {
        let clamped = value.clamp(0.0, 1.0);
        sum += f64::from(clamped);
        let bin = (clamped * 255.0).round() as usize;
        if let Some(slot) = histogram.get_mut(bin.min(255)) {
            *slot += 1;
        }
        if clamped <= BLACK_POINT {
            clipped_low += 1;
        }
        if clamped >= WHITE_POINT {
            clipped_high += 1;
        }
    }

    let percentile = |fraction: f64| -> f32 {
        let target = (fraction * total as f64).ceil().max(1.0) as u32;
        let mut seen = 0u32;
        for (bin, count) in histogram.iter().enumerate() {
            seen += count;
            if seen >= target {
                return bin as f32 / 255.0;
            }
        }
        1.0
    };

    LumaStats {
        mean: (sum / total as f64) as f32,
        p1: percentile(0.01),
        p50: percentile(0.50),
        p99: percentile(0.99),
        clip_lo: f64::from(clipped_low) as f32 / total as f32,
        clip_hi: f64::from(clipped_high) as f32 / total as f32,
    }
}

/// Mean gradient magnitude and its ninetieth percentile, both in `0..1`.
///
/// A forward difference rather than a Sobel kernel: on a 128 px plane the two
/// agree to within a percent, and the forward difference has no border cases to
/// get subtly wrong. What matters is that the number is comparable between two
/// frames, not that it is a textbook operator.
#[must_use]
pub fn edge_energy(plane: &LumaPlane) -> (f32, f32) {
    let reduced = box_resize(
        &plane.values,
        plane.width,
        plane.height,
        EDGE_PLANE,
        EDGE_PLANE,
    );
    let mut magnitudes = Vec::with_capacity(EDGE_PLANE * EDGE_PLANE);
    for row in 0..EDGE_PLANE {
        for column in 0..EDGE_PLANE {
            let here = reduced
                .get(row * EDGE_PLANE + column)
                .copied()
                .unwrap_or(0.0);
            let right = reduced
                .get(row * EDGE_PLANE + column.min(EDGE_PLANE - 2) + 1)
                .copied()
                .unwrap_or(here);
            let below = reduced
                .get((row.min(EDGE_PLANE - 2) + 1) * EDGE_PLANE + column)
                .copied()
                .unwrap_or(here);
            magnitudes.push(((right - here).abs() + (below - here).abs()).min(1.0));
        }
    }
    if magnitudes.is_empty() {
        return (0.0, 0.0);
    }
    let mean = magnitudes.iter().sum::<f32>() / magnitudes.len() as f32;
    let mut sorted = magnitudes;
    sorted.sort_by(f32::total_cmp);
    let rank = ((sorted.len() as f64 * 0.90).ceil() as usize).min(sorted.len() - 1);
    let p90 = sorted.get(rank).copied().unwrap_or(mean);
    (mean, p90)
}

/// Hue, saturation and value of one sRGB triple, all in `0..1`.
///
/// Hue is scaled to `0..1` rather than degrees so that all three axes bin
/// identically and the histogram has no special case.
#[must_use]
pub fn to_hsv(red: u8, green: u8, blue: u8) -> (f32, f32, f32) {
    let r = f32::from(red) / 255.0;
    let g = f32::from(green) / 255.0;
    let b = f32::from(blue) / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let span = max - min;

    let hue = if span <= f32::EPSILON {
        0.0
    } else if (max - r).abs() <= f32::EPSILON {
        ((g - b) / span).rem_euclid(6.0) / 6.0
    } else if (max - g).abs() <= f32::EPSILON {
        ((b - r) / span + 2.0) / 6.0
    } else {
        ((r - g) / span + 4.0) / 6.0
    };
    let saturation = if max <= f32::EPSILON { 0.0 } else { span / max };
    (hue.clamp(0.0, 1.0), saturation, max)
}

/// The 8x8x8 HSV histogram, each bin scaled so the fullest bin is 255.
///
/// Scaled by the maximum rather than by the total because the interesting
/// comparison is the *shape* of a frame's colour distribution: two photographs of
/// the same room at different exposures should match, and normalising by the
/// total would make one of them a scaled copy of the other in every bin at once.
#[must_use]
pub fn hsv_histogram(rgb: &[u8], width: usize, height: usize) -> [u8; HSV_BINS] {
    let mut counts = [0u32; HSV_BINS];
    for pixel in rgb.chunks_exact(3).take(width * height) {
        let (Some(red), Some(green), Some(blue)) = (pixel.first(), pixel.get(1), pixel.get(2))
        else {
            continue;
        };
        let (hue, saturation, value) = to_hsv(*red, *green, *blue);
        let axis = |component: f32| -> usize {
            ((component * HSV_AXIS as f32) as usize).min(HSV_AXIS - 1)
        };
        let bin = axis(hue) * HSV_AXIS * HSV_AXIS + axis(saturation) * HSV_AXIS + axis(value);
        if let Some(slot) = counts.get_mut(bin) {
            *slot += 1;
        }
    }
    let peak = counts.iter().copied().max().unwrap_or(0);
    let mut out = [0u8; HSV_BINS];
    if peak == 0 {
        return out;
    }
    for (slot, count) in out.iter_mut().zip(counts) {
        *slot = ((f64::from(count) * 255.0 / f64::from(peak)).round() as u32).min(255) as u8;
    }
    out
}

/// The five most common colours, most frequent first, as bin centres.
///
/// A 4x4x4 quantisation rather than a clustering pass: sixty-four buckets is
/// enough to say "this frame is mostly warm skin and dark suit", it costs one
/// pass, and unlike k-means it produces the same answer every time without
/// anybody having to seed a generator.
#[must_use]
pub fn palette(rgb: &[u8], width: usize, height: usize) -> [[u8; 3]; PALETTE_LEN] {
    const BUCKETS: usize = PALETTE_AXIS * PALETTE_AXIS * PALETTE_AXIS;
    let mut counts = [0u32; BUCKETS];
    for pixel in rgb.chunks_exact(3).take(width * height) {
        let (Some(red), Some(green), Some(blue)) = (pixel.first(), pixel.get(1), pixel.get(2))
        else {
            continue;
        };
        let axis = |component: u8| -> usize {
            (usize::from(component) * PALETTE_AXIS / 256).min(PALETTE_AXIS - 1)
        };
        let bucket =
            axis(*red) * PALETTE_AXIS * PALETTE_AXIS + axis(*green) * PALETTE_AXIS + axis(*blue);
        if let Some(slot) = counts.get_mut(bucket) {
            *slot += 1;
        }
    }

    let mut ranked: Vec<(u32, usize)> = counts
        .iter()
        .copied()
        .enumerate()
        .map(|(bucket, count)| (count, bucket))
        .collect();
    // Descending by count, then ascending by bucket, so ties are stable.
    ranked.sort_by(|left, right| right.0.cmp(&left.0).then(left.1.cmp(&right.1)));

    let step = 256 / PALETTE_AXIS;
    let centre = |axis_index: usize| -> u8 { (axis_index * step + step / 2).min(255) as u8 };

    let mut out = [[0u8; 3]; PALETTE_LEN];
    for (slot, (count, bucket)) in out.iter_mut().zip(ranked) {
        if count == 0 {
            break;
        }
        let red = bucket / (PALETTE_AXIS * PALETTE_AXIS);
        let green = (bucket / PALETTE_AXIS) % PALETTE_AXIS;
        let blue = bucket % PALETTE_AXIS;
        *slot = [centre(red), centre(green), centre(blue)];
    }
    out
}

/// Every cheap descriptor, from one buffer, in one pass each.
#[must_use]
pub fn describe(rgb: &[u8], width: usize, height: usize, plane: &LumaPlane) -> Descriptors {
    let (edge_mean, edge_p90) = edge_energy(plane);
    Descriptors {
        hsv_hist: hsv_histogram(rgb, width, height),
        luma: luma_stats(plane),
        edge_energy: edge_mean,
        edge_p90,
        palette: palette(rgb, width, height),
        descriptor_ver: DESCRIPTOR_VER,
    }
}
