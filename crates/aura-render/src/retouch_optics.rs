//! Selected optical and colour tools, in linear Rec.2020. ADR-0128.
// Frame dimensions and edit parameters are checked at the render/recipe boundaries.
#![allow(clippy::indexing_slicing)]
mod pyramid;

use crate::retouch_mask::Coverage;
use crate::retouch_planes::{luma, Rect};
use aura_recipe::retouch_tools::{Edit, Tool};
use rayon::prelude::*;

const BLACK: f32 = 1e-6;
const MAX_INTERVALS: usize = 64;

pub(crate) fn supports(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::MotionBlur
            | Tool::ZoomBlur
            | Tool::ColorDespill
            | Tool::HighlightCompress
            | Tool::ColorSaturation
    )
}

pub(crate) fn apply(
    rgb: &mut [f32],
    width: usize,
    height: usize,
    edit: &Edit,
    coverage: &Coverage,
) {
    if !supports(edit.tool) || !edit.enabled || edit.amount <= 0.0 {
        return;
    }
    if matches!(
        edit.tool,
        Tool::ColorDespill | Tool::HighlightCompress | Tool::ColorSaturation
    ) {
        apply_colour(rgb, width, height, edit, coverage);
        return;
    }
    let Some(rect) = Rect::around(coverage.bounds, 0, width, height) else {
        return;
    };
    let centre = edit.source.unwrap_or([0.5; 2]);
    let focus = [
        centre[0] * width as f32 - 0.5,
        centre[1] * height as f32 - 0.5,
    ];
    let angle = edit.warmth * std::f32::consts::PI;
    let length = edit.radius * width.min(height) as f32;
    let motion = [angle.cos() * length, angle.sin() * length];
    let max_length = if edit.tool == Tool::ZoomBlur {
        (width as f32).hypot(height as f32) * edit.radius * 10.0
    } else {
        length
    };
    let pyramid = pyramid::Pyramid::capture(
        rgb,
        width,
        height,
        rect,
        coverage,
        max_length / MAX_INTERVALS as f32,
    );
    rgb.par_chunks_mut(width * 3)
        .enumerate()
        .skip(rect.y0)
        .take(rect.h)
        .for_each(|(y, row)| {
            for x in rect.x0..rect.x0 + rect.w {
                let amount = coverage.at(x, y, width, height) * edit.amount;
                if amount <= 0.0 {
                    continue;
                }
                let vector = if edit.tool == Tool::ZoomBlur {
                    [
                        (x as f32 - focus[0]) * edit.radius * 10.0,
                        (y as f32 - focus[1]) * edit.radius * 10.0,
                    ]
                } else {
                    motion
                };
                let distance = vector[0].hypot(vector[1]);
                // A zoom centre is exactly fixed, with no round-trip or filter blur.
                if distance < BLACK {
                    continue;
                }
                let intervals = (distance.ceil() as usize).clamp(2, MAX_INTERVALS);
                let spacing = distance / intervals as f32;
                let level = spacing.max(1.0).log2();
                let mut sum = [0.0; 4];
                for index in 0..=intervals {
                    let position = index as f32 / intervals as f32 - 0.5;
                    let point = [
                        x as f32 + vector[0] * position,
                        y as f32 + vector[1] * position,
                    ];
                    let sample = pyramid.sample(point, level);
                    let shutter = if index == 0 || index == intervals {
                        0.5
                    } else {
                        1.0
                    };
                    for c in 0..4 {
                        sum[c] += sample[c] * shutter;
                    }
                }
                if sum[3] <= BLACK {
                    continue;
                }
                for c in 0..3 {
                    let old = row[x * 3 + c];
                    row[x * 3 + c] = old + amount * (sum[c] / sum[3] - old);
                }
            }
        });
}

fn spill_axis(edit: &Edit) -> Option<[f32; 3]> {
    let chosen = crate::retouch_color::to_working(edit.target_color?);
    let neutral = luma(chosen);
    let chroma = chosen.map(|value| value - neutral);
    let norm = chroma.iter().map(|value| value * value).sum::<f32>().sqrt();
    (norm > BLACK).then(|| chroma.map(|value| value / norm))
}

fn apply_colour(rgb: &mut [f32], width: usize, height: usize, edit: &Edit, coverage: &Coverage) {
    let [x0, y0, x1, y1] = coverage.bounds;
    let axis = spill_axis(edit);
    rgb.par_chunks_mut(width * 3)
        .enumerate()
        .skip(y0)
        .take(y1.saturating_sub(y0))
        .for_each(|(y, row)| {
            for x in x0..x1 {
                let amount = coverage.at(x, y, width, height) * edit.amount;
                if amount <= 0.0 {
                    continue;
                }
                let old = [row[x * 3], row[x * 3 + 1], row[x * 3 + 2]];
                let corrected = if edit.tool == Tool::ColorDespill {
                    axis.map_or(old, |axis| despill(old, axis, edit.tone, amount))
                } else if edit.tool == Tool::ColorSaturation {
                    saturate(old, edit, amount)
                } else {
                    compress(old, edit, amount)
                };
                row[x * 3..x * 3 + 3].copy_from_slice(&corrected);
            }
        });
}

fn despill(old: [f32; 3], axis: [f32; 3], selectivity: f32, amount: f32) -> [f32; 3] {
    let luminance = luma(old);
    if luminance <= BLACK {
        return old;
    }
    let chroma = old.map(|value| value - luminance);
    let magnitude = chroma.iter().map(|value| value * value).sum::<f32>().sqrt();
    if magnitude <= BLACK {
        return old;
    }
    let projection = chroma
        .iter()
        .zip(axis)
        .map(|(value, direction)| value * direction)
        .sum::<f32>()
        .max(0.0);
    let alignment = projection / magnitude;
    let threshold = selectivity * 0.95;
    let ramp = ((alignment - threshold) / (1.0 - threshold)).clamp(0.0, 1.0);
    let strength = ramp * ramp * (3.0 - 2.0 * ramp) * amount;
    let delta = axis.map(|value| -value * projection * strength);
    let mut gamut = 1.0_f32;
    for c in 0..3 {
        // Preserve existing negative working-space values but create none from
        // positive source channels. Compress the whole correction, never clip RGB.
        if delta[c] < 0.0 {
            gamut = gamut.min(old[c].max(0.0) / -delta[c]);
        }
    }
    std::array::from_fn(|c| old[c] + gamut * delta[c])
}

fn compress(old: [f32; 3], edit: &Edit, amount: f32) -> [f32; 3] {
    let luminance = luma(old);
    let threshold = 0.18 * (edit.tone * 4.0).exp2();
    if luminance <= threshold || edit.texture <= 0.0 {
        return old;
    }
    let excess = luminance - threshold;
    let target = threshold + excess / (1.0 + excess / threshold * edit.texture * 0.5);
    let gain = 1.0 + amount * (target / luminance - 1.0);
    old.map(|value| value * gain)
}

fn saturate(old: [f32; 3], edit: &Edit, amount: f32) -> [f32; 3] {
    if edit.warmth == 0.0 && edit.tint == 0.0 {
        return old;
    }
    let luminance = luma(old);
    if luminance <= BLACK {
        return old;
    }
    let chroma = old.map(|value| value - luminance);
    let maximum = old.into_iter().fold(f32::NEG_INFINITY, f32::max);
    let minimum = old.into_iter().fold(f32::INFINITY, f32::min);
    let saturation = ((maximum - minimum) / maximum.max(BLACK)).clamp(0.0, 1.0);
    let gain = (1.0 + edit.warmth) * (1.0 + edit.tint * (1.0 - saturation));
    let mut gain = 1.0 + amount * (gain - 1.0);
    // Uniformly limit the whole chroma vector, preserving luminance and hue.
    for c in 0..3 {
        if chroma[c] < 0.0 {
            gain = gain.min((luminance - old[c].min(0.0)) / -chroma[c]);
        }
    }
    if (gain - 1.0).abs() <= f32::EPSILON {
        return old;
    }
    std::array::from_fn(|c| luminance + gain * chroma[c])
}
