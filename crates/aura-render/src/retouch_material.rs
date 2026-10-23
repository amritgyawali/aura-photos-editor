//! Explicit material and lighting corrections in linear Rec.2020. ADR-0127.
// Coordinates are clipped to dimension-checked frames before indexing.
#![allow(clippy::indexing_slicing)]

use crate::retouch_mask::Coverage;
use crate::retouch_planes::{luma, Rect, Weighted};
use aura_recipe::retouch_tools::{Edit, Tool};

const TILE: usize = 256;
const BLACK: f32 = 1e-5;
const MAX_TARGET_SAMPLES: usize = 65_536;
const SHADOW_STOPS: f32 = 2.0;
const STAIN_STOPS: f32 = 1.0;

pub(crate) fn supports(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::FabricMoire
            | Tool::FabricStain
            | Tool::ShadowBalance
            | Tool::BackgroundBlur
            | Tool::HighlightGlow
    )
}

struct Source {
    rect: Rect,
    pixels: Vec<[f32; 3]>,
}

impl Source {
    fn capture(rgb: &[f32], width: usize, rect: Rect) -> Self {
        let pixels = rect
            .cells()
            .map(|(x, y)| {
                let i = (y * width + x) * 3;
                [rgb[i], rgb[i + 1], rgb[i + 2]]
            })
            .collect();
        Self { rect, pixels }
    }

    fn at(&self, x: usize, y: usize) -> [f32; 3] {
        self.pixels[(y - self.rect.y0) * self.rect.w + x - self.rect.x0]
    }
}

/// Deterministic upper midtone of the selected region, sampled at bounded cost.
fn shadow_target(source: &Source, coverage: &Coverage, width: usize, height: usize) -> f32 {
    let stride = source.pixels.len().div_ceil(MAX_TARGET_SAMPLES).max(1);
    let mut samples: Vec<(f32, f32)> = source
        .rect
        .cells()
        .enumerate()
        .step_by(stride)
        .map(|(i, (x, y))| (luma(source.pixels[i]), coverage.at(x, y, width, height)))
        .filter(|(value, weight)| *weight > 0.0 && *value > BLACK && value.is_finite())
        .collect();
    if samples.is_empty() {
        return BLACK.log2();
    }
    samples.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
    let target_weight = samples.iter().map(|(_, weight)| weight).sum::<f32>() * (2.0 / 3.0);
    let mut accumulated = 0.0;
    for (value, weight) in samples {
        accumulated += weight;
        if accumulated >= target_weight {
            return value.log2();
        }
    }
    BLACK.log2()
}

/// A small clean-material reference. It is intentionally independent of the repair mask.
fn fabric_reference(rgb: &[f32], width: usize, height: usize, edit: &Edit) -> Option<[f32; 3]> {
    let source = edit.source?;
    let x = ((source[0] * width as f32) as usize).min(width - 1);
    let y = ((source[1] * height as f32) as usize).min(height - 1);
    let radius = ((edit.radius * width.min(height) as f32) as usize).clamp(1, 16);
    let mut channels = [Vec::new(), Vec::new(), Vec::new()];
    for py in y.saturating_sub(radius)..=(y + radius).min(height - 1) {
        for px in x.saturating_sub(radius)..=(x + radius).min(width - 1) {
            for (c, samples) in channels.iter_mut().enumerate() {
                samples.push(rgb[(py * width + px) * 3 + c].max(BLACK).log2());
            }
        }
    }
    Some(channels.map(|mut samples| {
        let rank = samples.len() / 2;
        let (_, value, _) = samples.select_nth_unstable_by(rank, f32::total_cmp);
        *value
    }))
}

pub(crate) fn apply(
    rgb: &mut [f32],
    width: usize,
    height: usize,
    edit: &Edit,
    coverage: &Coverage,
) {
    apply_tiled(rgb, width, height, edit, coverage, TILE);
}

fn apply_tiled(
    rgb: &mut [f32],
    width: usize,
    height: usize,
    edit: &Edit,
    coverage: &Coverage,
    tile: usize,
) {
    let Some(rect) = Rect::around(coverage.bounds, 0, width, height) else {
        return;
    };
    if !supports(edit.tool) || !edit.enabled || edit.amount <= 0.0 {
        return;
    }
    let reference = if edit.tool == Tool::FabricStain {
        fabric_reference(rgb, width, height, edit)
    } else {
        None
    };
    if edit.tool == Tool::FabricStain && reference.is_none() {
        return;
    }
    let source = Source::capture(rgb, width, rect);
    let target = if edit.tool == Tool::ShadowBalance {
        shadow_target(&source, coverage, width, height)
    } else {
        0.0
    };
    let radius = ((edit.radius * width.min(height) as f32).round() as usize).max(1);
    let context = Context {
        source,
        coverage,
        edit,
        width,
        height,
        radius,
        target,
        reference,
    };
    for y in (rect.y0..rect.y0 + rect.h).step_by(tile) {
        for x in (rect.x0..rect.x0 + rect.w).step_by(tile) {
            let core = Rect {
                x0: x,
                y0: y,
                w: tile.min(rect.x0 + rect.w - x),
                h: tile.min(rect.y0 + rect.h - y),
            };
            context.render_tile(rgb, core);
        }
    }
}

struct Context<'a> {
    source: Source,
    coverage: &'a Coverage,
    edit: &'a Edit,
    width: usize,
    height: usize,
    radius: usize,
    target: f32,
    reference: Option<[f32; 3]>,
}

impl Context<'_> {
    fn render_tile(&self, rgb: &mut [f32], core: Rect) {
        // Three box passes have compact support of three radii. Tile boundaries
        // therefore cannot truncate a kernel contributing to a core pixel.
        let margin = self.radius * crate::bands::BOX_PASSES;
        let outer = self.source.rect;
        let x0 = core.x0.saturating_sub(margin).max(outer.x0);
        let y0 = core.y0.saturating_sub(margin).max(outer.y0);
        let rect = Rect {
            x0,
            y0,
            w: (core.x0 + core.w + margin).min(outer.x0 + outer.w) - x0,
            h: (core.y0 + core.h + margin).min(outer.y0 + outer.h) - y0,
        };
        let weights: Vec<f32> = rect
            .cells()
            .map(|(x, y)| self.coverage.at(x, y, self.width, self.height))
            .collect();
        let filter = Weighted::new(&weights, rect.w, rect.h, self.radius);
        let means = self.filtered(&filter, rect);
        for (x, y) in core.cells() {
            let i = (y - rect.y0) * rect.w + x - rect.x0;
            let weight = weights[i] * self.edit.amount;
            if weight <= 0.0 || filter.support()[i] <= 1e-4 {
                continue;
            }
            let old = self.source.at(x, y);
            let base = if self.edit.tool == Tool::ShadowBalance {
                [means[0][i]; 3]
            } else {
                [means[0][i], means[1][i], means[2][i]]
            };
            let corrected = self.correct(old, base, weight);
            let offset = (y * self.width + x) * 3;
            rgb[offset..offset + 3].copy_from_slice(&corrected);
        }
    }

    fn filtered(&self, filter: &Weighted<'_>, rect: Rect) -> [Vec<f32>; 3] {
        let threshold = 0.18 * (self.edit.tone * 4.0).exp2();
        let channels = if self.edit.tool == Tool::ShadowBalance {
            1
        } else {
            3
        };
        let mut means = [Vec::new(), Vec::new(), Vec::new()];
        for (channel, mean) in means.iter_mut().enumerate().take(channels) {
            let plane: Vec<f32> = rect
                .cells()
                .map(|(x, y)| {
                    let p = self.source.at(x, y);
                    let luminance = luma(p).max(BLACK);
                    match self.edit.tool {
                        Tool::FabricMoire => p[channel] / luminance,
                        Tool::FabricStain => p[channel].max(BLACK).log2(),
                        Tool::ShadowBalance => luminance.log2(),
                        Tool::HighlightGlow => p[channel] * (1.0 - threshold / luminance).max(0.0),
                        _ => p[channel],
                    }
                })
                .collect();
            *mean = filter.mean(&plane);
        }
        means
    }

    fn correct(&self, old: [f32; 3], base: [f32; 3], weight: f32) -> [f32; 3] {
        let luminance = luma(old);
        match self.edit.tool {
            Tool::FabricMoire => {
                if luminance <= BLACK {
                    return old;
                }
                // Normalize after filtering: luminance detail, including the weave,
                // is independent of the smoothed chroma, even on saturated fabric.
                let gain = luminance / luma(base).max(BLACK);
                std::array::from_fn(|c| old[c] + weight * (base[c] * gain - old[c]))
            }
            Tool::FabricStain => {
                let Some(reference) = self.reference else {
                    return old;
                };
                std::array::from_fn(|c| {
                    let stops = (reference[c] - base[c]).clamp(-STAIN_STOPS, STAIN_STOPS);
                    old[c] * (stops * weight * self.edit.tone).exp2()
                })
            }
            Tool::ShadowBalance => {
                // Keep black black and taper corrections near the noise floor.
                let confidence = (luminance.max(0.0) / (BLACK * 100.0)).min(1.0);
                let stops = (self.target - base[0]).clamp(0.0, self.edit.tone * SHADOW_STOPS);
                let gain = (stops * weight * confidence).exp2();
                old.map(|v| v * gain)
            }
            Tool::HighlightGlow => std::array::from_fn(|c| old[c] + weight * base[c].max(0.0)),
            Tool::BackgroundBlur => std::array::from_fn(|c| old[c] + weight * (base[c] - old[c])),
            _ => old,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spatial_tiles_agree_with_a_whole_selection() {
        let frame = crate::fixtures::detail_frame(97, 79);
        for tool in [
            Tool::FabricMoire,
            Tool::FabricStain,
            Tool::ShadowBalance,
            Tool::BackgroundBlur,
            Tool::HighlightGlow,
        ] {
            let edit: Edit = serde_json::from_value(serde_json::json!({
                "id":"tile-check", "tool":tool, "enabled":true, "region":[0.5,0.5,1,1],
                "source":[0.2,0.3], "amount":0.8, "feather":0.0, "radius":0.04,
                "texture":1.0, "tone":0.5, "warmth":0.0, "tint":0.0
            }))
            .unwrap();
            let coverage = Coverage::for_edit(&edit, 97, 79, &frame.rgb);
            let mut tiled = frame.rgb.clone();
            let mut whole = frame.rgb.clone();
            apply_tiled(&mut tiled, 97, 79, &edit, &coverage, 19);
            apply_tiled(&mut whole, 97, 79, &edit, &coverage, 512);
            let error = tiled
                .iter()
                .zip(whole)
                .map(|(a, b)| (a - b).abs())
                .fold(0.0_f32, f32::max);
            assert!(error < 2e-5, "{tool:?}: tile error {error}");
        }
    }
}
