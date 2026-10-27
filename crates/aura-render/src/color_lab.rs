//! One hundred independent selected colour/tone methods on linear Rec.2020.
//! Stored source coordinates refer to the full frame before each operation.
#![allow(clippy::indexing_slicing)]
use crate::cpu::Frame;
use aura_recipe::{
    color_lab::{self, Operation, Tool},
    Recipe,
};
use rayon::prelude::*;
mod advanced;
mod creative;
mod scientific;
mod stencil;

const BINS: usize = 256;
const EPS: f32 = 1e-6;
const LUMA: [f32; 3] = [0.2627, 0.678, 0.0593];
fn luma(p: [f32; 3]) -> f32 {
    p.into_iter().zip(LUMA).map(|(v, k)| v * k).sum()
}
fn pixel(src: &[f32], i: usize) -> [f32; 3] {
    [src[i * 3], src[i * 3 + 1], src[i * 3 + 2]]
}
fn chroma(p: [f32; 3]) -> [f32; 2] {
    let y = luma(p);
    if y <= EPS {
        return [0.0; 2];
    }
    [
        (p[0] / y - 1.0).clamp(-8.0, 8.0),
        (p[2] / y - 1.0).clamp(-8.0, 8.0),
    ]
}
fn dot(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}
fn length(a: [f32; 2]) -> f32 {
    dot(a, a).sqrt()
}
/// Hold luminance while shortening chroma to prevent newly negative channels.
fn from_chroma(p: [f32; 3], c: [f32; 2]) -> [f32; 3] {
    let y = luma(p).max(0.0);
    let delta = [
        y * c[0],
        -y * (LUMA[0] * c[0] + LUMA[2] * c[1]) / LUMA[1],
        y * c[1],
    ];
    let scale = delta
        .iter()
        .filter(|v| **v < 0.0)
        .fold(1.0_f32, |a, d| a.min(y / -d).min(1.0));
    delta.map(|d| (y + d * scale).max(0.0))
}
fn with_luma(p: [f32; 3], y: f32) -> [f32; 3] {
    let old = luma(p);
    if old > EPS {
        p.map(|v| v * y.max(0.0) / old)
    } else {
        p
    }
}
fn sample(src: &[f32], w: usize, h: usize, point: [f32; 2]) -> [f32; 3] {
    let x = ((point[0] * w as f32) as usize).min(w - 1);
    let y = ((point[1] * h as f32) as usize).min(h - 1);
    let mut sum = [0.0; 3];
    let mut count = 0.0;
    for row in y.saturating_sub(1)..=(y + 1).min(h - 1) {
        for col in x.saturating_sub(1)..=(x + 1).min(w - 1) {
            for (v, q) in sum.iter_mut().zip(pixel(src, row * w + col)) {
                *v += q;
            }
            count += 1.0;
        }
    }
    sum.map(|v| v / count)
}
fn bin(value: f32, lo: f32, hi: f32) -> usize {
    (((value - lo) / (hi - lo).max(EPS)).clamp(0.0, 1.0) * (BINS - 1) as f32).round() as usize
}
fn quantile(hist: &[f64; BINS], q: f64, lo: f32, hi: f32) -> f32 {
    let target = hist.iter().sum::<f64>() * q;
    let mut sum = 0.0;
    for (i, v) in hist.iter().enumerate() {
        sum += v;
        if sum >= target {
            return lo + (hi - lo) * i as f32 / (BINS - 1) as f32;
        }
    }
    hi
}
struct Stats {
    lo: f32,
    hi: f32,
    log_lo: f32,
    log_hi: f32,
    hist: [f64; BINS],
    cdf: [f32; BINS],
    clipped: [f32; BINS],
    log_cdf: [f32; BINS],
    mean_log: f32,
    center: [f32; 2],
    axis: [f32; 2],
    eigen: [f32; 2],
    split: usize,
}
fn cumulative(hist: &[f64; BINS]) -> [f32; BINS] {
    let total = hist.iter().sum::<f64>().max(f64::EPSILON);
    let first = hist.iter().copied().find(|v| *v > 0.0).unwrap_or(0.0);
    let denominator = (total - first).max(f64::EPSILON);
    let mut sum = 0.0;
    std::array::from_fn(|i| {
        sum += hist[i];
        ((sum - first) / denominator).clamp(0.0, 1.0) as f32
    })
}
impl Stats {
    fn otsu(hist: &[f64; BINS], total: f64) -> usize {
        let mut left = 0.0;
        let mut left_sum = 0.0;
        let all_sum = hist
            .iter()
            .enumerate()
            .map(|(i, v)| i as f64 * v)
            .sum::<f64>();
        let mut score = -1.0;
        let mut split = BINS / 2;
        for (i, v) in hist.iter().enumerate().take(BINS - 1) {
            left += v;
            left_sum += i as f64 * v;
            let right = total - left;
            if left <= 0.0 || right <= 0.0 {
                continue;
            }
            let delta = left_sum / left - (all_sum - left_sum) / right;
            let candidate = left * right * delta * delta;
            if candidate > score {
                score = candidate;
                split = i;
            }
        }
        split
    }
    fn new(src: &[f32], weights: &[f32]) -> Self {
        let selected = weights.iter().filter(|weight| **weight > 0.0).count();
        let stride = selected.div_ceil(65_536).max(1);
        let indices: Vec<_> = weights
            .iter()
            .enumerate()
            .filter(|(_, weight)| **weight > 0.0)
            .step_by(stride)
            .map(|(index, _)| index)
            .collect();
        let mut lo = f32::MAX;
        let mut hi = 0.0_f32;
        let mut sum = [0.0_f64; 6];
        let mut total = 0.0;
        for &i in &indices {
            let a = f64::from(weights[i]);
            if a <= 0.0 {
                continue;
            }
            let p = pixel(src, i);
            let y = luma(p).max(EPS);
            let c = chroma(p);
            lo = lo.min(y);
            hi = hi.max(y);
            total += a;
            let values = [
                f64::from(y.log2()),
                f64::from(c[0]),
                f64::from(c[1]),
                f64::from(c[0] * c[0]),
                f64::from(c[1] * c[1]),
                f64::from(c[0] * c[1]),
            ];
            for (v, q) in sum.iter_mut().zip(values) {
                *v += a * q;
            }
        }
        if total <= 0.0 {
            lo = 0.0;
            hi = 0.0;
            total = 1.0;
        }
        let center = [(sum[1] / total) as f32, (sum[2] / total) as f32];
        let xx = (sum[3] / total) as f32 - center[0] * center[0];
        let yy = (sum[4] / total) as f32 - center[1] * center[1];
        let xy = (sum[5] / total) as f32 - center[0] * center[1];
        let angle = 0.5 * (2.0 * xy).atan2(xx - yy);
        let discriminant = ((xx - yy).powi(2) + 4.0 * xy * xy).sqrt();
        let eigen = [
            ((xx + yy + discriminant) * 0.5).max(EPS),
            ((xx + yy - discriminant) * 0.5).max(EPS),
        ];
        let log_lo = lo.max(EPS).log2();
        let log_hi = hi.max(EPS).log2();
        let mut hist = [0.0; BINS];
        let mut log_hist = [0.0; BINS];
        for &i in &indices {
            let y = luma(pixel(src, i)).max(EPS);
            let a = f64::from(weights[i]);
            hist[bin(y, lo, hi)] += a;
            log_hist[bin(y.log2(), log_lo, log_hi)] += a;
        }
        let cap = total / BINS as f64 * 4.0;
        let excess = hist.iter().map(|v| (v - cap).max(0.0)).sum::<f64>() / BINS as f64;
        let clipped = hist.map(|v| v.min(cap) + excess);
        let split = Self::otsu(&hist, total);
        Self {
            lo,
            hi,
            log_lo,
            log_hi,
            cdf: cumulative(&hist),
            clipped: cumulative(&clipped),
            log_cdf: cumulative(&log_hist),
            hist,
            mean_log: (sum[0] / total) as f32,
            center,
            axis: [angle.cos(), angle.sin()],
            eigen,
            split,
        }
    }
    fn rank(&self, y: f32) -> f32 {
        self.cdf[bin(y, self.lo, self.hi)]
    }
    fn landmark(&self, y: f32) -> f32 {
        let points = [
            self.lo,
            quantile(&self.hist, 0.1, self.lo, self.hi),
            quantile(&self.hist, 0.5, self.lo, self.hi),
            quantile(&self.hist, 0.9, self.lo, self.hi),
            self.hi,
        ];
        let targets = [
            self.lo,
            self.lo + (self.hi - self.lo) * 0.2,
            self.lo + (self.hi - self.lo) * 0.5,
            self.lo + (self.hi - self.lo) * 0.8,
            self.hi,
        ];
        for i in 0..4 {
            if y <= points[i + 1] {
                let t = ((y - points[i]) / (points[i + 1] - points[i]).max(EPS)).clamp(0.0, 1.0);
                return targets[i] + t * (targets[i + 1] - targets[i]);
            }
        }
        y
    }
}
struct Local {
    log_y: Vec<f32>,
    chroma: [Vec<f32>; 2],
    variance: [Vec<f32>; 2],
}
impl Local {
    fn new(src: &[f32], weights: &[f32], w: usize, h: usize, radius: usize) -> Self {
        let denominator = crate::bands::box_mean(weights, w, h, radius);
        let mean = |values: Vec<f32>| {
            let values: Vec<_> = values.iter().zip(weights).map(|(v, a)| v * a).collect();
            crate::bands::box_mean(&values, w, h, radius)
                .iter()
                .zip(&denominator)
                .map(|(v, a)| if *a > EPS { v / a } else { 0.0 })
                .collect::<Vec<_>>()
        };
        let log_y = mean(
            src.chunks_exact(3)
                .map(|p| luma([p[0], p[1], p[2]]).max(EPS).log2())
                .collect(),
        );
        let chroma: [Vec<f32>; 2] = std::array::from_fn(|c| {
            mean(
                (0..weights.len())
                    .map(|i| chroma(pixel(src, i))[c])
                    .collect(),
            )
        });
        let variance = std::array::from_fn(|c| {
            let squared = mean(
                (0..weights.len())
                    .map(|i| super_chroma(src, i, c).powi(2))
                    .collect(),
            );
            squared
                .iter()
                .zip(&chroma[c])
                .map(|(v, m)| (v - m * m).max(0.0))
                .collect()
        });
        Self {
            log_y,
            chroma,
            variance,
        }
    }
}
fn super_chroma(src: &[f32], i: usize, c: usize) -> f32 {
    chroma(pixel(src, i))[c]
}

struct Kernel<'a> {
    src: &'a [f32],
    weights: &'a [f32],
    op: &'a Operation,
    w: usize,
    h: usize,
    stats: Stats,
    local: Option<Local>,
    source: [f32; 3],
    second: [f32; 3],
    advanced: Option<advanced::Advanced>,
    creative: Option<creative::Creative>,
    scientific: Option<scientific::Scientific>,
}
impl<'a> Kernel<'a> {
    fn new(src: &'a [f32], weights: &'a [f32], w: usize, h: usize, op: &'a Operation) -> Self {
        let radius = (op.radius * w.min(h) as f32).round().max(1.0) as usize;
        let local = matches!(
            op.tool,
            Tool::LocalLogContrast
                | Tool::RetinexRatio
                | Tool::ChromaEdgeProtect
                | Tool::ChromaWinsorize
                | Tool::OpponentContrast
        )
        .then(|| Local::new(src, weights, w, h, radius));
        Self {
            src,
            weights,
            op,
            w,
            h,
            stats: Stats::new(src, weights),
            local,
            source: sample(src, w, h, op.source),
            second: sample(src, w, h, op.second),
            scientific: scientific::TOOLS
                .contains(&op.tool)
                .then(|| scientific::Scientific::new(src, weights, w, h, op)),
            creative: creative::TOOLS
                .contains(&op.tool)
                .then(|| creative::Creative::new(src, weights, w, h, op)),
            advanced: advanced::TOOLS
                .contains(&op.tool)
                .then(|| advanced::Advanced::new(src, weights, w, h, op)),
        }
    }
    fn bridge(&self, i: usize, p: [f32; 3]) -> [f32; 3] {
        let a = chroma(self.source);
        let b = chroma(self.second);
        let t = if self.op.tool == Tool::SpatialNeutralBridge {
            let delta = [
                self.op.second[0] - self.op.source[0],
                self.op.second[1] - self.op.source[1],
            ];
            let at = [
                ((i % self.w) as f32 + 0.5) / self.w as f32 - self.op.source[0],
                ((i / self.w) as f32 + 0.5) / self.h as f32 - self.op.source[1],
            ];
            if dot(delta, delta) < EPS {
                0.5
            } else {
                (dot(at, delta) / dot(delta, delta)).clamp(0.0, 1.0)
            }
        } else {
            let lo = luma(self.source);
            let hi = luma(self.second);
            if (hi - lo).abs() < EPS {
                0.5
            } else {
                ((luma(p) - lo) / (hi - lo)).clamp(0.0, 1.0)
            }
        };
        let neutral = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
        let y = luma(p);
        let gains = [
            1.0 / (1.0 + neutral[0]).max(0.05),
            1.0 / (1.0 - (LUMA[0] * neutral[0] + LUMA[2] * neutral[1]) / LUMA[1]).max(0.05),
            1.0 / (1.0 + neutral[1]).max(0.05),
        ];
        with_luma([0, 1, 2].map(|c| p[c] * gains[c]), y)
    }
    fn projected(&self, p: [f32; 3]) -> [f32; 3] {
        let c = chroma(p);
        let a = chroma(self.source);
        let b = chroma(self.second);
        let target = match self.op.tool {
            Tool::ChromaReject => {
                let denom = dot(a, a).max(EPS);
                let projection = dot(c, a) / denom;
                [c[0] - a[0] * projection, c[1] - a[1] * projection]
            }
            Tool::PaletteAxis => {
                let d = [b[0] - a[0], b[1] - a[1]];
                let at = [c[0] - a[0], c[1] - a[1]];
                let t = dot(at, d) / dot(d, d).max(EPS);
                [a[0] + d[0] * t, a[1] + d[1] * t]
            }
            Tool::SampledReplace => {
                let distance = length([c[0] - a[0], c[1] - a[1]]);
                let gate = (-(distance / self.op.width).powi(2) * 0.5).exp();
                let target = chroma(self.op.target);
                [
                    c[0] + (target[0] - a[0]) * gate,
                    c[1] + (target[1] - a[1]) * gate,
                ]
            }
            _ => c,
        };
        from_chroma(p, target)
    }
    fn covariance(&self, p: [f32; 3]) -> [f32; 3] {
        let c = chroma(p);
        let s = &self.stats;
        let d = [c[0] - s.center[0], c[1] - s.center[1]];
        let orthogonal = [-s.axis[1], s.axis[0]];
        let mut along = dot(d, s.axis);
        let mut across = dot(d, orthogonal);
        if self.op.tool == Tool::DecorrelationStretch {
            let target = self.op.width;
            along *= (target / s.eigen[0].sqrt()).clamp(0.25, 4.0);
            across *= (target / s.eigen[1].sqrt()).clamp(0.25, 4.0);
        } else {
            along *= 0.1;
        }
        from_chroma(
            p,
            [
                s.center[0] + along * s.axis[0] + across * orthogonal[0],
                s.center[1] + along * s.axis[1] + across * orthogonal[1],
            ],
        )
    }
    fn compression(&self, p: [f32; 3]) -> [f32; 3] {
        let y = luma(p);
        let c = chroma(p);
        let radius = length(c);
        let gain = match self.op.tool {
            Tool::SoftGamut => {
                let max = p.into_iter().fold(y, f32::max);
                let ceiling = y.max(1.0);
                let distance = (max - y).max(0.0);
                let limit = (ceiling - y).max(self.op.width);
                if distance > EPS {
                    limit * (distance / limit).tanh() / distance
                } else {
                    1.0
                }
            }
            Tool::LumaChromaCompress => 1.0 / (1.0 + radius * y / (self.op.pivot + self.op.width)),
            Tool::ChromaCusp => {
                let ratios = from_chroma([1.0; 3], c);
                let peak = ratios.into_iter().fold(1.0, f32::max);
                let cusp = self.op.width / (peak - 1.0).max(EPS);
                if radius > self.op.width {
                    cusp.min(1.0) + (1.0 - cusp.min(1.0)) / (1.0 + radius / self.op.width)
                } else {
                    1.0
                }
            }
            Tool::NeutralExclusion => {
                radius * radius / (radius * radius + self.op.width * self.op.width)
            }
            _ => 1.0,
        };
        from_chroma(p, c.map(|v| v * gain))
    }
    fn histogram(&self, p: [f32; 3]) -> [f32; 3] {
        let y = luma(p);
        let s = &self.stats;
        if s.hi - s.lo <= EPS {
            return p;
        }
        let rank = s.rank(y);
        let span = s.hi - s.lo;
        let target = match self.op.tool {
            Tool::HistogramEqualize => s.lo + span * rank,
            Tool::ClippedEqualize => s.lo + span * s.clipped[bin(y, s.lo, s.hi)],
            Tool::LogEqualize => (s.log_lo
                + (s.log_hi - s.log_lo) * s.log_cdf[bin(y.max(EPS).log2(), s.log_lo, s.log_hi)])
            .exp2(),
            Tool::RankGaussian => {
                let u = rank.clamp(0.001, 0.999);
                let normal = (u / (1.0 - u)).ln() / 1.7;
                s.lo + span * (0.5 + normal * self.op.width).clamp(0.0, 1.0)
            }
            Tool::BimodalSeparate => {
                let split = s.lo + span * s.split as f32 / (BINS - 1) as f32;
                let rank_split = s.cdf[s.split];
                if y <= split {
                    s.lo + span * 0.45 * rank / rank_split.max(EPS)
                } else {
                    s.lo + span * (0.55 + 0.45 * (rank - rank_split) / (1.0 - rank_split).max(EPS))
                }
            }
            Tool::PercentileRemap => s.landmark(y),
            _ => y,
        };
        with_luma(p, target)
    }
    fn tone_map(&self, p: [f32; 3]) -> [f32; 3] {
        let y = luma(p);
        let anchor = self.op.pivot;
        let target = match self.op.tool {
            Tool::PhotographicMap => {
                let x = y * anchor / self.stats.mean_log.exp2().max(EPS);
                x / (1.0 + x)
            }
            Tool::AsinhMap => {
                let scale = 1.0 + self.op.width * 16.0;
                (y * scale).asinh() / scale.asinh()
            }
            Tool::LogToeShoulder => {
                let k = self.op.width * 16.0;
                let x = (1.0 + k * y.max(0.0)).ln() / (1.0 + k).ln();
                let toe = x * x / (x + anchor).max(EPS);
                toe * (1.0 + anchor)
            }
            _ => y,
        };
        with_luma(p, target)
    }
    fn local(&self, i: usize, p: [f32; 3]) -> [f32; 3] {
        let Some(local) = &self.local else {
            return p;
        };
        let y = luma(p);
        let c = chroma(p);
        let log_y = y.max(EPS).log2();
        let residual = log_y - local.log_y[i];
        match self.op.tool {
            Tool::LocalLogContrast => with_luma(
                p,
                (log_y + residual.clamp(-2.0, 2.0) * self.op.width).exp2(),
            ),
            Tool::RetinexRatio => {
                let ratio = (y / (local.log_y[i].exp2() + self.op.pivot)).min(8.0);
                with_luma(p, ratio * self.op.pivot * 2.0)
            }
            Tool::ChromaEdgeProtect => {
                let gate = (-(residual / self.op.width).powi(2)).exp();
                from_chroma(p, c.map(|v| v * (1.0 + gate)))
            }
            Tool::ChromaWinsorize => {
                let corrected = std::array::from_fn(|k| {
                    let mean = local.chroma[k][i];
                    let reach = local.variance[k][i].sqrt().max(self.op.width);
                    c[k].clamp(mean - reach, mean + reach)
                });
                from_chroma(p, corrected)
            }
            Tool::OpponentContrast => from_chroma(
                p,
                std::array::from_fn(|k| c[k] + (c[k] - local.chroma[k][i]) * self.op.width),
            ),
            _ => p,
        }
    }
    fn target(&self, i: usize, p: [f32; 3]) -> [f32; 3] {
        if let Some(scientific) = &self.scientific {
            return scientific.target(i, p, self.op, self.src, self.weights);
        }
        if let Some(creative) = &self.creative {
            return creative.target(i, p, self.op, self.src, self.weights);
        }
        match self.op.tool {
            Tool::SpatialNeutralBridge | Tool::TonalNeutralBridge => self.bridge(i, p),
            Tool::ChromaReject | Tool::PaletteAxis | Tool::SampledReplace => self.projected(p),
            Tool::DecorrelationStretch | Tool::PrincipalChromaSuppress => self.covariance(p),
            Tool::SoftGamut
            | Tool::LumaChromaCompress
            | Tool::ChromaCusp
            | Tool::NeutralExclusion => self.compression(p),
            Tool::HistogramEqualize
            | Tool::ClippedEqualize
            | Tool::LogEqualize
            | Tool::RankGaussian
            | Tool::BimodalSeparate
            | Tool::PercentileRemap => self.histogram(p),
            Tool::PhotographicMap | Tool::AsinhMap | Tool::LogToeShoulder => self.tone_map(p),
            _ => self.advanced.as_ref().map_or_else(
                || self.local(i, p),
                |advanced| advanced.target(i, p, self.op, &self.stats, self.source, self.second),
            ),
        }
    }
}
fn filtered(src: &[f32], weights: &[f32], w: usize, h: usize, op: &Operation) -> Vec<f32> {
    if op.amount <= 0.0 || op.strength <= 0.0 || !weights.iter().any(|weight| *weight > 0.0) {
        return src.to_vec();
    }
    if op.tool == Tool::LuminanceErrorDiffusion {
        return scientific::error_diffusion(src, weights, w, h, op);
    }
    let kernel = Kernel::new(src, weights, w, h, op);
    // An unavailable reference is an exact no-op, including signed working-space values.
    if kernel
        .scientific
        .as_ref()
        .is_some_and(|method| !method.is_available())
    {
        return src.to_vec();
    }
    let mut output = src.to_vec();
    output
        .par_chunks_exact_mut(3)
        .enumerate()
        .for_each(|(i, dst)| {
            let alpha = weights[i] * op.amount * op.strength;
            if alpha <= 0.0 {
                return;
            }
            let p = pixel(src, i);
            let q = kernel.target(i, p);
            for (c, value) in dst.iter_mut().enumerate() {
                let target = p[c] + (q[c] - p[c]) * alpha;
                if target.is_finite() {
                    *value = target.max(0.0);
                }
            }
        });
    output
}
/// Selected masks honor enabled state and opacity. Invalid/missing masks stay neutral.
pub(crate) fn apply(rgb: &mut [f32], w: u32, h: u32, recipe: &Recipe, frame: Option<&Frame>) {
    if w == 0 || h == 0 || rgb.len() != w as usize * h as usize * 3 {
        return;
    }
    let Ok(operations) = color_lab::read(recipe) else {
        return;
    };
    let Ok(masks) = aura_recipe::local_masks::read(recipe) else {
        return;
    };
    for op in operations
        .iter()
        .filter(|op| op.enabled && op.amount > 0.0 && op.strength > 0.0)
    {
        let Some(mask) = masks
            .iter()
            .find(|m| m.id == op.mask_id && m.enabled && m.amount > 0.0)
        else {
            continue;
        };
        let Some(mut weights) = crate::local_masks::coverage(rgb, w, h, recipe, frame, &op.mask_id)
        else {
            continue;
        };
        for weight in &mut weights {
            *weight *= mask.amount;
        }
        let next = filtered(rgb, &weights, w as usize, h as usize, op);
        rgb.copy_from_slice(&next);
    }
}

#[cfg(test)]
mod tests;
