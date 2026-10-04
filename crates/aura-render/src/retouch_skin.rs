//! Sample-guided portrait processing in linear Rec.2020. ADR-0075.
// Dimensions and coordinates are bounded by the caller and Coverage before indexing.
#![allow(clippy::indexing_slicing)]
use crate::retouch_mask::Coverage;
use aura_recipe::retouch_tools::{Edit, Tool};

const EPSILON: f32 = 1e-6;
const MAX_EXPOSURE_STOPS: f32 = 0.5;
const MAX_COLOR_DELTA: f32 = 0.35;

fn luma(p: [f32; 3]) -> f32 {
    p[0] * 0.2627 + p[1] * 0.6780 + p[2] * 0.0593
}

fn chroma(p: [f32; 3]) -> [f32; 3] {
    let sum = p.iter().map(|v| v.max(0.0)).sum::<f32>().max(EPSILON);
    p.map(|v| v.max(0.0) / sum)
}

fn affinity(p: [f32; 3], reference: [f32; 3], tolerance: f32) -> f32 {
    if luma(p) <= EPSILON || luma(reference) <= EPSILON {
        return 0.0;
    }
    let p = chroma(p);
    let reference = chroma(reference);
    let distance = p
        .iter()
        .zip(reference)
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f32>()
        .sqrt();
    // Full coverage in the inner half; a smooth falloff has a strictly zero exterior.
    let t = ((tolerance - distance) / (tolerance * 0.5)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Radius-independent box mean, clipped at boundaries, with f64 running sums.
fn mean(values: &[f32], w: usize, h: usize, radius: usize) -> Vec<f32> {
    let mut horizontal = vec![0.0; values.len()];
    let mut out = vec![0.0; values.len()];
    for y in 0..h {
        let mut sum: f64 = values[y * w..y * w + (radius + 1).min(w)]
            .iter()
            .map(|v| f64::from(*v))
            .sum();
        for x in 0..w {
            let lo = x.saturating_sub(radius);
            let hi = (x + radius + 1).min(w);
            horizontal[y * w + x] = (sum / (hi - lo) as f64) as f32;
            if x >= radius {
                sum -= f64::from(values[y * w + x - radius]);
            }
            if x + radius + 1 < w {
                sum += f64::from(values[y * w + x + radius + 1]);
            }
        }
    }
    for x in 0..w {
        let mut sum: f64 = (0..(radius + 1).min(h))
            .map(|y| f64::from(horizontal[y * w + x]))
            .sum();
        for y in 0..h {
            let lo = y.saturating_sub(radius);
            let hi = (y + radius + 1).min(h);
            out[y * w + x] = (sum / (hi - lo) as f64) as f32;
            if y >= radius {
                sum -= f64::from(horizontal[(y - radius) * w + x]);
            }
            if y + radius + 1 < h {
                sum += f64::from(horizontal[(y + radius + 1) * w + x]);
            }
        }
    }
    out
}

/// Scalar self-guided filter (He et al.), with local signal-relative regularization.
/// Called independently per channel so a same-luminance color edge is also protected.
fn guided(values: &[f32], w: usize, h: usize, radius: usize, protection: f32) -> Vec<f32> {
    let average = mean(values, w, h, radius);
    let square = values.iter().map(|v| v * v).collect::<Vec<_>>();
    let variance = mean(&square, w, h, radius);
    let mut slope = Vec::with_capacity(values.len());
    let mut offset = Vec::with_capacity(values.len());
    // 3%-50% relative contrast. The reference signal, not a fixed skin brightness,
    // sets the regularizer; darker and lighter samples receive the same treatment.
    let relative = 0.03 + (1.0 - protection) * 0.47;
    for (&m, v) in average.iter().zip(variance) {
        let variance = (v - m * m).max(0.0);
        let regularizer = (relative * m.abs().max(EPSILON)).powi(2).max(1e-16);
        let a = variance / (variance + regularizer);
        slope.push(a);
        offset.push(m * (1.0 - a));
    }
    let slope = mean(&slope, w, h, radius);
    let offset = mean(&offset, w, h, radius);
    values
        .iter()
        .zip(slope)
        .zip(offset)
        .map(|((&p, a), b)| a * p + b)
        .collect()
}

fn reference(rgb: &[f32], w: usize, h: usize, point: [f32; 2]) -> [f32; 3] {
    let cx = ((point[0] * w as f32) as usize).min(w - 1);
    let cy = ((point[1] * h as f32) as usize).min(h - 1);
    let radius = ((w.min(h) as f32 * 0.002).round() as usize).max(1);
    let mut sum = [0.0; 3];
    let mut count = 0.0;
    for y in cy.saturating_sub(radius)..=(cy + radius).min(h - 1) {
        for x in cx.saturating_sub(radius)..=(cx + radius).min(w - 1) {
            for c in 0..3 {
                sum[c] += rgb[(y * w + x) * 3 + c];
            }
            count += 1.0;
        }
    }
    sum.map(|v| v / count)
}

fn correction(
    old: [f32; 3],
    low: [f32; 3],
    broad: [f32; 3],
    sample: [f32; 3],
    edit: &Edit,
) -> [f32; 3] {
    let luminance = luma(old).max(EPSILON);
    let low_luma = luma(low).max(EPSILON);
    match edit.tool {
        Tool::SkinSmooth => std::array::from_fn(|c| {
            old[c] + edit.tone * (broad[c] - low[c]) + (edit.texture - 1.0) * (old[c] - low[c])
        }),
        Tool::SkinUniformity => {
            let sample_luma = luma(sample).max(EPSILON);
            let mut delta: [f32; 3] =
                std::array::from_fn(|c| (sample[c] / sample_luma - low[c] / low_luma) * low_luma);
            // Remove numerical luminance drift before bounding chroma correction.
            let drift = luma(delta);
            for d in &mut delta {
                *d -= drift;
            }
            let peak = delta.iter().fold(0.0_f32, |m, d| m.max(d.abs()));
            let scale = (luminance * MAX_COLOR_DELTA / peak.max(EPSILON)).min(1.0) * edit.tone;
            std::array::from_fn(|c| old[c] + delta[c] * scale)
        }
        Tool::PortraitDodgeBurn => {
            let stops = (luma(broad).max(EPSILON) / low_luma)
                .log2()
                .clamp(-MAX_EXPOSURE_STOPS, MAX_EXPOSURE_STOPS)
                * edit.tone;
            old.map(|v| v * stops.exp2())
        }
        _ => old,
    }
}

/// 1 for skin-bright pixels, falling to 0 for pixels under a third of the sample's brightness.
fn hair_guard(p: [f32; 3], sample: [f32; 3]) -> f32 {
    let ratio = luma(p) / luma(sample).max(EPSILON);
    let t = ((ratio - 0.3) / 0.2).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Whether colour likeness to the sample limits the operation: always for brush and region
/// selections, and for a segmentation matte only when skin settings are given as well.
fn uses_colour(edit: &Edit) -> bool {
    edit.matte.is_none() || edit.skin.is_some()
}

/// Skin connected to the sample point, as a soft 0..1 weight over `coverage.bounds`.
///
/// A flood fill on a coarse grid of cell means: a cell joins when its colour is close to the
/// sample and the step from its neighbour is not a strong edge in brightness or colour.
/// That separates a person from a skin-coloured background, which almost always meets the
/// skin at an edge, and from skin-coloured areas that do not touch the person at all.
/// Returns `None` when the selection is not limited to connected skin.
#[allow(clippy::too_many_lines)]
pub(crate) fn connected_weight(
    rgb: &[f32],
    w: usize,
    h: usize,
    edit: &Edit,
    coverage: &Coverage,
) -> Option<Vec<f32>> {
    let settings = edit.skin?;
    if !settings.connected {
        return None;
    }
    let point = edit.source?;
    let [x0, y0, x1, y1] = coverage.bounds;
    let (bw, bh) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
    if bw == 0 || bh == 0 {
        return Some(Vec::new());
    }
    let sample = reference(rgb, w, h, point);
    let cell = (bw.max(bh) / 240).max(2);
    let (gw, gh) = (bw.div_ceil(cell), bh.div_ceil(cell));
    let mut means = vec![[0.0_f32; 3]; gw * gh];
    let mut usable = vec![false; gw * gh];
    for gy in 0..gh {
        for gx in 0..gw {
            let (cx0, cy0) = (x0 + gx * cell, y0 + gy * cell);
            let (cx1, cy1) = ((cx0 + cell).min(x1), (cy0 + cell).min(y1));
            let mut sum = [0.0_f32; 3];
            let mut n = 0.0_f32;
            for y in cy0..cy1 {
                for x in cx0..cx1 {
                    let i = (y * w + x) * 3;
                    for c in 0..3 {
                        sum[c] += rgb[i + c];
                    }
                    n += 1.0;
                }
            }
            let mean = sum.map(|v| v / n.max(1.0));
            let k = gy * gw + gx;
            means[k] = mean;
            let centre = coverage.at(usize::midpoint(cx0, cx1), usize::midpoint(cy0, cy1), w, h);
            usable[k] = centre > 0.0 && affinity(mean, sample, settings.tolerance) >= 0.3;
        }
    }
    // Seed at the sample, or at the nearest usable cell within a few cells of it.
    let sx = ((point[0] * w as f32) as usize).clamp(x0, x1 - 1);
    let sy = ((point[1] * h as f32) as usize).clamp(y0, y1 - 1);
    let (sgx, sgy) = ((sx - x0) / cell, (sy - y0) / cell);
    let mut seed = None;
    'search: for radius in 0..6_usize {
        for gy in sgy.saturating_sub(radius)..=(sgy + radius).min(gh - 1) {
            for gx in sgx.saturating_sub(radius)..=(sgx + radius).min(gw - 1) {
                if usable[gy * gw + gx] {
                    seed = Some(gy * gw + gx);
                    break 'search;
                }
            }
        }
    }
    let mut reached = vec![false; gw * gh];
    if let Some(start) = seed {
        let mut stack = vec![start];
        reached[start] = true;
        while let Some(k) = stack.pop() {
            let (gx, gy) = (k % gw, k / gw);
            let here = means[k];
            let mut visit = |n: usize| {
                if reached[n] || !usable[n] {
                    return;
                }
                let there = means[n];
                let (la, lb) = (luma(here), luma(there));
                let step = (la - lb).abs() / la.max(lb).max(EPSILON);
                let a = chroma(here);
                let b = chroma(there);
                let hue = a
                    .iter()
                    .zip(b)
                    .map(|(p, q)| (p - q).powi(2))
                    .sum::<f32>()
                    .sqrt();
                if step < 0.22 && hue < settings.tolerance * 0.6 {
                    reached[n] = true;
                    stack.push(n);
                }
            };
            if gx > 0 {
                visit(k - 1);
            }
            if gx + 1 < gw {
                visit(k + 1);
            }
            if gy > 0 {
                visit(k - gw);
            }
            if gy + 1 < gh {
                visit(k + gw);
            }
        }
    }
    // Soften the cell edges: a 3x3 mean of the reached map, sampled bilinearly per pixel.
    let mut soft = vec![0.0_f32; gw * gh];
    for gy in 0..gh {
        for gx in 0..gw {
            let mut sum = 0.0;
            let mut n = 0.0;
            for ny in gy.saturating_sub(1)..=(gy + 1).min(gh - 1) {
                for nx in gx.saturating_sub(1)..=(gx + 1).min(gw - 1) {
                    sum += f32::from(u8::from(reached[ny * gw + nx]));
                    n += 1.0;
                }
            }
            soft[gy * gw + gx] = if reached[gy * gw + gx] {
                (sum / n).max(0.5)
            } else {
                sum / n * 0.5
            };
        }
    }
    let mut out = Vec::with_capacity(bw * bh);
    for y in y0..y1 {
        let fy = ((y - y0) as f32 + 0.5) / cell as f32 - 0.5;
        let gy0 = (fy.floor().max(0.0) as usize).min(gh - 1);
        let gy1 = (gy0 + 1).min(gh - 1);
        let ty = (fy - gy0 as f32).clamp(0.0, 1.0);
        for x in x0..x1 {
            let fx = ((x - x0) as f32 + 0.5) / cell as f32 - 0.5;
            let gx0 = (fx.floor().max(0.0) as usize).min(gw - 1);
            let gx1 = (gx0 + 1).min(gw - 1);
            let tx = (fx - gx0 as f32).clamp(0.0, 1.0);
            let top = soft[gy0 * gw + gx0] * (1.0 - tx) + soft[gy0 * gw + gx1] * tx;
            let bottom = soft[gy1 * gw + gx0] * (1.0 - tx) + soft[gy1 * gw + gx1] * tx;
            let v = top * (1.0 - ty) + bottom * ty;
            out.push((v * 2.0 - 0.5).clamp(0.0, 1.0));
        }
    }
    Some(out)
}

/// What a sampled skin operation will actually change, before strength: the authored
/// coverage times this pixel's skin likeness times the connected-skin weight.
pub(crate) fn selection(
    rgb: &[f32],
    w: usize,
    h: usize,
    edit: &Edit,
    coverage: &Coverage,
) -> Vec<f32> {
    let settings = edit.skin.unwrap_or_default();
    let connected = connected_weight(rgb, w, h, edit, coverage);
    // A segmentation matte already says which pixels are this person's skin; colour likeness
    // to one sample is then only applied when the operation asks for it explicitly.
    let sample = edit
        .source
        .filter(|_| uses_colour(edit))
        .map(|p| reference(rgb, w, h, p));
    let [x0, y0, x1, y1] = coverage.bounds;
    let mut out = vec![0.0; w * h];
    for y in y0..y1 {
        for x in x0..x1 {
            let i = (y * w + x) * 3;
            let p = [rgb[i], rgb[i + 1], rgb[i + 2]];
            let mut v = coverage.at(x, y, w, h);
            if let Some(sample) = sample {
                v *= affinity(p, sample, settings.tolerance);
            }
            if let Some(weights) = &connected {
                v *= weights
                    .get((y - y0) * (x1 - x0) + x - x0)
                    .copied()
                    .unwrap_or(0.0);
            }
            out[y * w + x] = v;
        }
    }
    out
}

pub(crate) fn apply(rgb: &mut [f32], w: usize, h: usize, edit: &Edit, coverage: &Coverage) {
    let Some(point) = edit.source else {
        return;
    }; // Validation requires a photographer's sample.
    let [x0, y0, x1, y1] = coverage.bounds;
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let settings = edit.skin.unwrap_or_default();
    let sample = reference(rgb, w, h, point);
    if luma(sample) <= EPSILON {
        return;
    }
    let radius = (edit.radius * w.min(h) as f32).round().max(1.0) as usize;
    // Guided output has two neighborhood passes; include both, plus the narrow band.
    let margin = radius * 8;
    let bx = x0.saturating_sub(margin);
    let by = y0.saturating_sub(margin);
    let bw = (x1 + margin).min(w) - bx;
    let bh = (y1 + margin).min(h) - by;
    let mut narrow = Vec::with_capacity(3);
    let mut wide = Vec::with_capacity(3);
    for c in 0..3 {
        let plane: Vec<f32> = (by..by + bh)
            .flat_map(|y| (bx..bx + bw).map(move |x| (y * w + x) * 3 + c))
            .map(|i| rgb[i])
            .collect();
        let low = mean(&plane, bw, bh, radius);
        // Uniformity needs only the low-frequency chroma, no second filter.
        let broad = if edit.tool == Tool::SkinUniformity {
            Vec::new()
        } else {
            guided(&low, bw, bh, radius * 3, settings.edge_protection)
        };
        narrow.push(low);
        wide.push(broad);
    }
    let connected = connected_weight(rgb, w, h, edit, coverage);
    let colour = uses_colour(edit);
    for y in y0..y1 {
        for x in x0..x1 {
            let i = (y * w + x) * 3;
            let local = (y - by) * bw + x - bx;
            let old = [rgb[i], rgb[i + 1], rgb[i + 2]];
            let low = std::array::from_fn(|c| narrow[c][local]);
            let reach = connected.as_ref().map_or(1.0, |weights| {
                weights
                    .get((y - y0) * (x1 - x0) + x - x0)
                    .copied()
                    .unwrap_or(0.0)
            });
            // Selection checks the original pixel as well as its neighborhood, so
            // averaging across a lip/hair/background boundary cannot paint over it.
            let likeness = if colour {
                affinity(old, sample, settings.tolerance).min(affinity(
                    low,
                    sample,
                    settings.tolerance,
                ))
            } else {
                // A matte is coarser than a lash, a brow hair or stubble. Pixels far darker
                // than the sampled skin are hair, not skin, at any resolution: never lift them.
                hair_guard(old, sample)
            };
            let alpha = reach * coverage.at(x, y, w, h) * edit.amount * likeness;
            if alpha <= 0.0 {
                continue;
            }
            let broad = if edit.tool == Tool::SkinUniformity {
                low
            } else {
                std::array::from_fn(|c| wide[c][local])
            };
            let value = correction(old, low, broad, sample, edit);
            // Reduce the whole RGB correction together to avoid negative channels
            // without changing the chroma direction or the luminance constraint.
            let mut blend = alpha;
            for c in 0..3 {
                if value[c] < 0.0 && old[c] >= 0.0 {
                    blend = blend.min(old[c] / (old[c] - value[c]).max(EPSILON));
                }
            }
            for c in 0..3 {
                rgb[i + c] = old[c] + blend * (value[c] - old[c]);
            }
        }
    }
}
