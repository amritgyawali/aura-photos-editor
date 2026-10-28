//! Sample-guided portrait processing in linear Rec.2020. ADR-0075.
// Dimensions and coordinates are bounded by the caller and Coverage before indexing.
#![allow(clippy::indexing_slicing)]
use crate::retouch_mask::Coverage;
use aura_recipe::retouch_tools::{Edit, Tool};
use rayon::prelude::*;

mod connectivity;

const EPSILON: f32 = 1e-6;
/// Included in persistent render-cache identities whenever sampled-skin pixels change.
pub(crate) const REFERENCE_VERSION: &str = "robust-patch-v4-band-inside-selection";
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
    crate::bands::box_mean(values, w, h, radius)
}

/// What a neighbourhood average is taken over: the weight of every cell and how much of each
/// cell's neighbourhood carried any.
///
/// A plain box mean beside an eye averages the lashes and the white of the eye into the skin
/// next to them, and the smoothing built on it leaves a grey band there; beside a hairline it
/// darkens the forehead. Averaging what the operation selects, and nothing else, is what lets
/// skin be evened right up to a feature without taking the feature's colour.
struct Selected<'a> {
    weights: &'a [f32],
    support: Vec<f32>,
    w: usize,
    h: usize,
    radius: usize,
}

impl<'a> Selected<'a> {
    fn new(weights: &'a [f32], w: usize, h: usize, radius: usize) -> Self {
        Self {
            weights,
            support: mean(weights, w, h, radius),
            w,
            h,
            radius,
        }
    }

    /// The selected mean of `values`. A cell whose neighbourhood selects nothing keeps its
    /// own value, and one whose neighbourhood is wholly selected gets the plain mean exactly.
    fn mean(&self, values: &[f32]) -> Vec<f32> {
        let scaled: Vec<f32> = values
            .par_iter()
            .zip(self.weights)
            .map(|(v, a)| v * a)
            .collect();
        mean(&scaled, self.w, self.h, self.radius)
            .par_iter()
            .zip(&self.support)
            .zip(values)
            .map(|((sum, support), own)| {
                if *support >= 1.0 - 1e-5 {
                    *sum
                } else if *support > 1e-4 {
                    sum / support
                } else {
                    *own
                }
            })
            .collect()
    }
}

/// Scalar self-guided filter (He et al.), with local signal-relative regularization.
/// Called independently per channel so a same-luminance color edge is also protected.
///
/// `positions` are planes that say where each of `values` was read (cell coordinates as the
/// narrow band saw them). They go through the same weights as the values, so what comes back
/// with the filtered plane is where *it* was read: the lean the wide band has at every cell,
/// which near the selection's edge is neither zero nor that of one plain mean.
fn guided(
    values: &[f32],
    over: &Selected<'_>,
    protection: f32,
    positions: [&[f32]; 2],
) -> (Vec<f32>, [Vec<f32>; 2]) {
    let average = over.mean(values);
    let square = values.par_iter().map(|v| v * v).collect::<Vec<_>>();
    let variance = over.mean(&square);
    // 3%-50% relative contrast. The reference signal, not a fixed skin brightness,
    // sets the regularizer; darker and lighter samples receive the same treatment.
    let relative = 0.03 + (1.0 - protection) * 0.47;
    let (slope, offset): (Vec<f32>, Vec<f32>) = average
        .par_iter()
        .zip(variance)
        .map(|(&m, v)| {
            let variance = (v - m * m).max(0.0);
            let regularizer = (relative * m.abs().max(EPSILON)).powi(2).max(1e-16);
            let a = variance / (variance + regularizer);
            (a, m * (1.0 - a))
        })
        .unzip();
    let read = positions.map(|plane| {
        let kept: Vec<f32> = over
            .mean(plane)
            .par_iter()
            .zip(&slope)
            .map(|(m, a)| m * (1.0 - a))
            .collect();
        (plane, over.mean(&kept))
    });
    let slope = over.mean(&slope);
    let offset = over.mean(&offset);
    let read = read.map(|(plane, kept)| {
        plane
            .par_iter()
            .zip(&slope)
            .zip(kept)
            .map(|((p, a), b)| a * p + b)
            .collect()
    });
    let out = values
        .par_iter()
        .zip(&slope)
        .zip(offset)
        .map(|((&p, a), b)| a * p + b)
        .collect();
    (out, read)
}

fn reference(rgb: &[f32], w: usize, h: usize, point: [f32; 2]) -> [f32; 3] {
    let cx = ((point[0] * w as f32) as usize).min(w - 1);
    let cy = ((point[1] * h as f32) as usize).min(h - 1);
    let radius = ((w.min(h) as f32 * 0.002).round() as usize).max(1);
    let mut samples = Vec::new();
    for y in cy.saturating_sub(radius)..=(cy + radius).min(h - 1) {
        for x in cx.saturating_sub(radius)..=(cx + radius).min(w - 1) {
            let i = (y * w + x) * 3;
            let p = [rgb[i], rgb[i + 1], rgb[i + 2]];
            if p.iter().all(|v| v.is_finite()) && luma(p) > EPSILON {
                samples.push(p);
            }
        }
    }
    robust_reference(&samples)
}

/// A sample is a patch, not a single perfect pixel. Reject sparse hair, dark
/// marks and specular glints with robust luminance/chroma distances before
/// averaging. All thresholds are relative to this patch's own signal; there is
/// no preferred complexion or fixed skin-brightness threshold.
fn robust_reference(samples: &[[f32; 3]]) -> [f32; 3] {
    if samples.is_empty() {
        return [0.0; 3];
    }
    let median = |mut values: Vec<f32>| {
        values.sort_by(f32::total_cmp);
        values[values.len() / 2]
    };
    let logs: Vec<f32> = samples.iter().map(|p| luma(*p).max(EPSILON).ln()).collect();
    let centre = median(logs.clone());
    let spread = median(logs.iter().map(|v| (v - centre).abs()).collect());
    let colours: Vec<[f32; 3]> = samples.iter().map(|p| chroma(*p)).collect();
    let colour: [f32; 3] = std::array::from_fn(|c| median(colours.iter().map(|p| p[c]).collect()));
    let distances: Vec<f32> = colours
        .iter()
        .map(|p| {
            p.iter()
                .zip(colour)
                .map(|(a, b)| (a - b).powi(2))
                .sum::<f32>()
                .sqrt()
        })
        .collect();
    let colour_spread = median(distances.clone());
    // Floors keep a clean, slightly textured patch continuous rather than
    // accepting just its median pixel when MAD happens to be zero.
    let light_limit = (4.685 * 1.4826 * spread).max(0.12);
    let colour_limit = (4.685 * 1.4826 * colour_spread).max(0.025);
    let mut sum = [0.0_f64; 3];
    let mut total = 0.0_f64;
    for ((p, log), distance) in samples.iter().zip(logs).zip(distances) {
        let light = ((log - centre).abs() / light_limit).min(1.0);
        let colour = (distance / colour_limit).min(1.0);
        let weight = f64::from((1.0 - light * light).powi(2) * (1.0 - colour * colour).powi(2));
        for c in 0..3 {
            sum[c] += f64::from(p[c]) * weight;
        }
        total += weight;
    }
    if total > 1e-9 {
        sum.map(|v| (v / total) as f32)
    } else {
        // Contradictory neighbourhood: choose an actual median-luminance
        // photographed pixel instead of manufacturing an unsupported colour.
        samples
            .iter()
            .min_by(|a, b| {
                (luma(**a).ln() - centre)
                    .abs()
                    .total_cmp(&(luma(**b).ln() - centre).abs())
            })
            .copied()
            .unwrap_or([0.0; 3])
    }
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
/// A second traversal on original pixels rejects thin contours and protected gaps hidden
/// by cell averages. Neither pass can add support beyond the authored coverage. Identical
/// touching surfaces without a measurable boundary still require a tighter authored mask.
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
                    let weight = coverage.at(x, y, w, h);
                    let i = (y * w + x) * 3;
                    for c in 0..3 {
                        sum[c] += rgb[i + c] * weight;
                    }
                    n += weight;
                }
            }
            let mean = sum.map(|v| v / n.max(EPSILON));
            let k = gy * gw + gx;
            means[k] = mean;
            usable[k] = n > EPSILON && affinity(mean, sample, settings.tolerance) >= 0.3;
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
                if connectivity::can_cross(here, there, settings.tolerance) {
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
    let fine = connectivity::refine(
        rgb,
        w,
        h,
        edit,
        coverage,
        sample,
        connectivity::Coarse {
            cell,
            width: gw,
            reached: &reached,
            seed,
        },
    );
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
            // Feather inward only. Interpolation must not select a cell the flood
            // rejected across a jaw/background boundary.
            let selected = fine[(y - y0) * bw + x - x0] == 2;
            out.push(if selected {
                (v * 2.0 - 0.5).clamp(0.0, 1.0)
            } else {
                0.0
            });
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

/// The narrow band of every channel over `rect` (left, top, width, height), and with
/// `protection` the edge-aware wide band as well. Uniformity needs only the low-frequency
/// chroma and asks for no second filter.
///
/// Both are means over what `weights` selects. At the selection's edge the wide band is read
/// from further inside than the narrow one, and their difference then holds the photograph's
/// own shading as well as the band; that share is taken back out of the wide band here, so
/// evening runs right up to a jaw or a hairline without flattening either.
fn bands(
    rgb: &[f32],
    w: usize,
    [bx, by, bw, bh]: [usize; 4],
    weights: &[f32],
    radius: usize,
    protection: Option<f32>,
) -> (Vec<Vec<f32>>, Vec<Vec<f32>>) {
    let near = Selected::new(weights, bw, bh, radius);
    let far = Selected::new(weights, bw, bh, radius * 3);
    // Where the narrow band read each cell from.
    let cells = |axis: fn(usize, usize) -> usize| -> Vec<f32> {
        let plane: Vec<f32> = (0..bw * bh).map(|i| axis(i, bw) as f32).collect();
        near.mean(&plane)
    };
    let (xs, ys) = (cells(|i, w| i % w), cells(|i, w| i / w));
    let near_lean = crate::retouch_planes::Lean::from_positions(&xs, &ys, bw);
    // The three channels are independent, so they are worked at once.
    (0..3_usize)
        .into_par_iter()
        .map(|c| {
            let plane: Vec<f32> = (by..by + bh)
                .flat_map(|y| (bx..bx + bw).map(move |x| (y * w + x) * 3 + c))
                .map(|i| rgb[i])
                .collect();
            let low = near.mean(&plane);
            let broad = protection.map_or_else(Vec::new, |protection| {
                let (mut broad, [fx, fy]) = guided(&low, &far, protection, [&xs, &ys]);
                let far_lean = crate::retouch_planes::Lean::from_positions(&fx, &fy, bw);
                let shading = far_lean.shading_between(&near_lean, &broad, bw, bh);
                for (value, leaked) in broad.iter_mut().zip(shading) {
                    *value -= leaked;
                }
                broad
            });
            (low, broad)
        })
        .unzip()
}

/// The narrow band's radius in pixels, for a frame whose short side is `short` and a selection
/// whose short side is `span`.
///
/// The wide band reads three radii out. A selection narrower than that is all edge to `bands`,
/// whose edge-shading correction then takes back the whole band, and the tool changed nothing at
/// all - on a cheek of an ordinary group photograph at the default radius. So the wide band is
/// kept inside the selection.
fn band_radius(edit: &Edit, short: usize, span: usize) -> usize {
    let asked = (edit.radius * short as f32).round().max(1.0) as usize;
    asked.min((span / 6).max(1))
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
    let radius = band_radius(edit, w.min(h), (x1 - x0).min(y1 - y0));
    // Guided output has two neighborhood passes; include both, plus the narrow band.
    let margin = radius * 8;
    let bx = x0.saturating_sub(margin);
    let by = y0.saturating_sub(margin);
    let bw = (x1 + margin).min(w) - bx;
    let bh = (y1 + margin).min(h) - by;
    let colour = uses_colour(edit);
    let connected = connected_weight(rgb, w, h, edit, coverage);
    // Only what the operation selects is averaged: its coverage, and within that the pixels
    // that are this skin rather than a lash, a brow hair or the lip beside it.
    let frame: &[f32] = rgb;
    let weights: Vec<f32> = (by..by + bh)
        .into_par_iter()
        .flat_map_iter(|y| (bx..bx + bw).map(move |x| (x, y)))
        .map(|(x, y)| {
            let cover = coverage.at(x, y, w, h);
            if cover <= 0.0 {
                return 0.0;
            }
            let i = (y * w + x) * 3;
            let p = [frame[i], frame[i + 1], frame[i + 2]];
            let reach = connected.as_ref().map_or(1.0, |weights| {
                if x < x0 || x >= x1 || y < y0 || y >= y1 {
                    return 0.0;
                }
                weights[(y - y0) * (x1 - x0) + x - x0]
            });
            cover
                * reach
                * if colour {
                    affinity(p, sample, settings.tolerance)
                } else {
                    hair_guard(p, sample)
                }
        })
        .collect();
    let (narrow, wide) = bands(
        rgb,
        w,
        [bx, by, bw, bh],
        &weights,
        radius,
        (edit.tool != Tool::SkinUniformity).then_some(settings.edge_protection),
    );
    // Every pixel reads only itself and the bands, so the rows are worked at once.
    rgb.par_chunks_exact_mut(w * 3)
        .enumerate()
        .skip(y0)
        .take(y1 - y0)
        .for_each(|(y, row)| {
            for x in x0..x1 {
                let i = x * 3;
                let local = (y - by) * bw + x - bx;
                let old = [row[i], row[i + 1], row[i + 2]];
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
                    row[i + c] = old[c] + blend * (value[c] - old[c]);
                }
            }
        });
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // Empty and unmodified samples must keep exact reference values.
mod reference_tests {
    use super::*;
    #[test]
    fn sparse_hair_glints_and_coloured_spots_do_not_change_the_skin_reference() {
        for exposure in [0.04, 0.2, 1.0, 3.0] {
            let skin = [0.38, 0.23, 0.16].map(|v| v * exposure);
            let mut samples = vec![skin; 75];
            samples.extend(vec![[0.002 * exposure; 3]; 10]);
            samples.extend(vec![[1.5 * exposure; 3]; 10]);
            samples.extend(vec![[0.4 * exposure, 0.03 * exposure, 0.02 * exposure]; 5]);
            let found = robust_reference(&samples);
            for c in 0..3 {
                assert!((found[c] - skin[c]).abs() < 1e-5 * exposure);
            }
        }
    }
    #[test]
    fn continuous_skin_gradient_keeps_its_midpoint_and_finite_hdr_values() {
        let samples: Vec<[f32; 3]> = (0..101)
            .map(|i| {
                let gain = 0.9 + i as f32 * 0.002;
                [2.0 * gain, 1.2 * gain, 0.8 * gain]
            })
            .collect();
        let found = robust_reference(&samples);
        for (a, b) in found.into_iter().zip([2.0, 1.2, 0.8]) {
            assert!((a - b).abs() < 0.006);
        }
        assert_eq!(robust_reference(&[]), [0.0; 3]);
    }
    #[test]
    fn nonfinite_and_black_source_pixels_cannot_poison_a_render() {
        let mut pixels = [0.3, 0.2, 0.1].repeat(9);
        pixels[0] = f32::NAN;
        pixels[4] = f32::INFINITY;
        assert_eq!(reference(&pixels, 3, 3, [0.5, 0.5]), [0.3, 0.2, 0.1]);
        assert_eq!(reference(&[0.0; 27], 3, 3, [0.5, 0.5]), [0.0; 3]);
    }
}
