//! Bounded selected-only sample reduction; all accumulation orders are fixed.
use super::super::{bin, chroma, length, luma, pixel, BINS, EPS};

pub(super) struct Measured {
    pub(super) rms: f32,
    pub(super) harmonic: f32,
    pub(super) skew: f32,
    pub(super) entropy: [f32; 4],
    pub(super) minkowski: [f32; 3],
    pub(super) edge: [f32; 3],
    pub(super) trimmed: [f32; 3],
    pub(super) log_ratio: [f32; 2],
    pub(super) mode: [f32; 2],
    pub(super) tonal: [[f32; 2]; 4],
    pub(super) radii: Vec<f32>,
    pub(super) median_radius: f32,
}
impl Measured {
    pub(super) fn new(src: &[f32], weights: &[f32], w: usize, h: usize) -> Self {
        let indices = selected_indices(weights);
        let total = indices
            .iter()
            .map(|i| f64::from(weights[*i]))
            .sum::<f64>()
            .max(f64::EPSILON);
        let samples: Vec<_> = indices
            .iter()
            .map(|i| (pixel(src, *i), f64::from(weights[*i])))
            .collect();
        let lo = samples
            .iter()
            .map(|(p, _)| luma(*p))
            .fold(f32::MAX, f32::min);
        let hi = samples.iter().map(|(p, _)| luma(*p)).fold(0.0, f32::max);
        let mean = samples
            .iter()
            .map(|(p, a)| f64::from(luma(*p)) * a)
            .sum::<f64>()
            / total;
        let mut moments = [0.0; 3];
        let mut minkowski = [0.0; 3];
        let mut log_ratio = [0.0; 2];
        let mut histogram = [0.0_f64; BINS];
        let mut modes = [[0.0_f64; 3]; 256];
        let mut tonal = [[0.0_f64; 3]; 4];
        let mut radii = Vec::with_capacity(samples.len());
        for (p, a) in &samples {
            let y = luma(*p).max(EPS);
            let c = chroma(*p);
            let delta = f64::from(y) - mean;
            moments[0] += a * f64::from(y) * f64::from(y);
            moments[1] += a / f64::from(y);
            moments[2] += a * delta.powi(3);
            for (moment, channel) in minkowski.iter_mut().zip(p) {
                *moment += a * f64::from(channel.max(0.0)).powi(6);
            }
            for (mean, channel) in log_ratio.iter_mut().zip([p[0], p[2]]) {
                *mean += a * f64::from((channel.max(EPS) / p[1].max(EPS)).ln());
            }
            let at = bin(y, lo, hi);
            histogram[at] += a;
            let zone = ((at * 4) / BINS).min(3);
            tonal[zone][0] += a * f64::from(c[0]);
            tonal[zone][1] += a * f64::from(c[1]);
            tonal[zone][2] += a;
            let cell = |v: f32| (((v + 4.0) / 8.0).clamp(0.0, 1.0) * 15.0).round() as usize;
            let mode = &mut modes[cell(c[0]) + 16 * cell(c[1])];
            mode[0] += a * f64::from(c[0]);
            mode[1] += a * f64::from(c[1]);
            mode[2] += a;
            radii.push(length(c));
        }
        radii.sort_by(f32::total_cmp);
        let median_radius = radii.get(radii.len() / 2).copied().unwrap_or(0.0);
        let variance = samples
            .iter()
            .map(|(p, a)| a * (f64::from(luma(*p)) - mean).powi(2))
            .sum::<f64>()
            / total;
        let skew = (moments[2] / total / variance.max(1e-12).powf(1.5)).clamp(-4.0, 4.0) as f32;
        let mut best = [0.0; 3];
        for candidate in modes {
            if candidate[2] > best[2] {
                best = candidate;
            }
        }
        let mode = [best[0], best[1]].map(|v| (v / best[2].max(f64::EPSILON)) as f32);
        // A missing luminance population is unknown, not a measured neutral colour.
        let center: [f32; 2] =
            std::array::from_fn(|k| (tonal.iter().map(|row| row[k]).sum::<f64>() / total) as f32);
        Self {
            rms: (moments[0] / total).sqrt() as f32,
            harmonic: (total / moments[1].max(f64::EPSILON)) as f32,
            skew,
            entropy: entropy(&histogram),
            minkowski: minkowski.map(|v| (v / total).powf(1.0 / 6.0) as f32),
            edge: edge_estimate(src, weights, &indices, w, h),
            trimmed: trimmed(&samples),
            log_ratio: log_ratio.map(|v| (v / total).clamp(-8.0, 8.0) as f32),
            mode,
            tonal: tonal.map(|row| {
                if row[2] > 0.0 {
                    [row[0], row[1]].map(|v| (v / row[2]) as f32)
                } else {
                    center
                }
            }),
            radii,
            median_radius,
        }
    }
}
fn entropy(histogram: &[f64; BINS]) -> [f32; 4] {
    std::array::from_fn(|zone| {
        let values = &histogram[zone * 64..(zone + 1) * 64];
        let total = values.iter().sum::<f64>().max(f64::EPSILON);
        0.05 + values
            .iter()
            .filter(|v| **v > 0.0)
            .map(|v| {
                let probability = v / total;
                (-probability * probability.ln()) as f32
            })
            .sum::<f32>()
    })
}
fn trimmed(samples: &[([f32; 3], f64)]) -> [f32; 3] {
    std::array::from_fn(|channel| {
        let mut values: Vec<_> = samples.iter().map(|(p, a)| (p[channel], *a)).collect();
        values.sort_by(|a, b| a.0.total_cmp(&b.0));
        let trim = values.len() / 10;
        let selected = &values[trim..values.len().saturating_sub(trim)];
        let mass = selected
            .iter()
            .map(|(_, a)| a)
            .sum::<f64>()
            .max(f64::EPSILON);
        (selected.iter().map(|(v, a)| f64::from(*v) * a).sum::<f64>() / mass) as f32
    })
}
fn edge_estimate(src: &[f32], weights: &[f32], indices: &[usize], w: usize, h: usize) -> [f32; 3] {
    let mut sums = [0.0_f64; 3];
    let mut total = 0.0;
    for &i in indices {
        for neighbor in [
            if i % w + 1 < w { Some(i + 1) } else { None },
            if i / w + 1 < h { Some(i + w) } else { None },
        ]
        .into_iter()
        .flatten()
        {
            let weight = f64::from(weights[i].min(weights[neighbor]));
            if weight <= 0.0 {
                continue;
            }
            let p = pixel(src, i);
            let q = pixel(src, neighbor);
            for (sum, (a, b)) in sums.iter_mut().zip(p.into_iter().zip(q)) {
                *sum += weight * f64::from((a - b).abs());
            }
            total += weight;
        }
    }
    if sums.iter().sum::<f64>() <= f64::EPSILON {
        [1.0; 3]
    } else {
        sums.map(|v| (v / total.max(f64::EPSILON)) as f32)
    }
}

fn selected_indices(weights: &[f32]) -> Vec<usize> {
    let stride = weights
        .iter()
        .filter(|a| **a > 0.0)
        .count()
        .div_ceil(65_536)
        .max(1);
    let indices: Vec<_> = weights
        .iter()
        .enumerate()
        .filter(|(_, a)| **a > 0.0)
        .step_by(stride)
        .map(|(i, _)| i)
        .collect();
    indices
}
