//! Original analytical colour operators; no external trained models.
use super::{chroma, from_chroma, length, luma, pixel, with_luma, Operation, Tool, EPS, LUMA};

pub(super) const TOOLS: [Tool; 25] = [
    Tool::SigmoidContrast,
    Tool::ExponentialShoulder,
    Tool::HermiteTone,
    Tool::SchlickBias,
    Tool::SolarizeLuma,
    Tool::ZoneExposure,
    Tool::BlackFloor,
    Tool::DarkChannelDehaze,
    Tool::LocalWienerTone,
    Tool::LocalMidrange,
    Tool::ChromaShear,
    Tool::ChromaSpiral,
    Tool::TonalHueTwist,
    Tool::DuotoneGradient,
    Tool::ChannelCrossfeed,
    Tool::SelectiveDensity,
    Tool::BradfordAdaptation,
    Tool::ChannelRatioLimit,
    Tool::MedianDespill,
    Tool::HarmonicHueSnap,
    Tool::ChromaLumaDetrend,
    Tool::GuidedChroma,
    Tool::MedianChroma,
    Tool::ChannelRankAlign,
    Tool::BilateralChroma,
];

struct LocalConfig {
    w: usize,
    h: usize,
    radius: usize,
    white: [f32; 3],
}

struct Neighborhood {
    y: Vec<f32>,
    variance: Vec<f32>,
    c: [Vec<f32>; 2],
    yc: [Vec<f32>; 2],
}
impl Neighborhood {
    fn new(
        src: &[f32],
        weights: &[f32],
        w: usize,
        h: usize,
        radius: usize,
        chroma_needed: bool,
    ) -> Self {
        let denominator = crate::bands::box_mean(weights, w, h, radius);
        let mean = |values: Vec<f32>| {
            let weighted: Vec<_> = values
                .iter()
                .zip(weights)
                .map(|(v, a)| if *a > 0.0 { v * a } else { 0.0 })
                .collect();
            crate::bands::box_mean(&weighted, w, h, radius)
                .iter()
                .zip(&denominator)
                .map(|(v, a)| if *a > 0.0 { v / a } else { 0.0 })
                .collect::<Vec<_>>()
        };
        let y = mean((0..weights.len()).map(|i| luma(pixel(src, i))).collect());
        let square = mean(
            (0..weights.len())
                .map(|i| luma(pixel(src, i)).powi(2))
                .collect(),
        );
        let variance = square
            .iter()
            .zip(&y)
            .map(|(v, m)| (v - m * m).max(0.0))
            .collect();
        let c = std::array::from_fn(|k| {
            if !chroma_needed {
                return Vec::new();
            }
            mean(
                (0..weights.len())
                    .map(|i| chroma(pixel(src, i))[k])
                    .collect(),
            )
        });
        let yc = std::array::from_fn(|k| {
            if !chroma_needed {
                return Vec::new();
            }
            mean(
                (0..weights.len())
                    .map(|i| luma(pixel(src, i)) * chroma(pixel(src, i))[k])
                    .collect(),
            )
        });
        Self { y, variance, c, yc }
    }
}

pub(super) struct Creative {
    mean: [f32; 3],
    mean_y: f32,
    slope: [f32; 2],
    rank: [Vec<(f32, f32)>; 3],
    local: Option<Neighborhood>,
    stencil: Option<LocalConfig>,
}
fn rotate(c: [f32; 2], angle: f32) -> [f32; 2] {
    let (s, cosa) = angle.sin_cos();
    [c[0] * cosa - c[1] * s, c[0] * s + c[1] * cosa]
}
fn weighted_rank(values: &[(f32, f32)], value: f32) -> f32 {
    let index = values.partition_point(|(v, _)| *v < value);
    if index == values.len() {
        return 1.0;
    }
    let last = values.partition_point(|(v, _)| *v <= value);
    let low = if index == 0 { 0.0 } else { values[index - 1].1 };
    let high = if last > index {
        values[last - 1].1
    } else {
        values[index].1
    };
    (low + high) * 0.5
}
fn weighted_value(values: &[(f32, f32)], rank: f32) -> f32 {
    values
        .get(
            values
                .partition_point(|(_, q)| *q < rank)
                .min(values.len().saturating_sub(1)),
        )
        .map_or(0.0, |(v, _)| *v)
}
impl Creative {
    pub(super) fn new(src: &[f32], weights: &[f32], w: usize, h: usize, op: &Operation) -> Self {
        let measured = matches!(
            op.tool,
            Tool::BradfordAdaptation
                | Tool::ChromaLumaDetrend
                | Tool::DarkChannelDehaze
                | Tool::ChannelRankAlign
        );
        let stride = weights
            .iter()
            .filter(|v| measured && **v > 0.0)
            .count()
            .div_ceil(65_536)
            .max(1);
        let indices: Vec<_> = weights
            .iter()
            .enumerate()
            .filter(|(_, v)| measured && **v > 0.0)
            .step_by(stride)
            .map(|(i, _)| i)
            .collect();
        let mut sum = [0.0_f64; 9];
        let mut total = 0.0;
        let mut rank: [Vec<(f32, f32)>; 3] =
            std::array::from_fn(|_| Vec::with_capacity(indices.len()));
        for i in indices {
            let p = pixel(src, i);
            let a = f64::from(weights[i]);
            let y = luma(p);
            let c = chroma(p);
            total += a;
            let v = [p[0], p[1], p[2], y, y * y, c[0], c[1], y * c[0], y * c[1]];
            for k in 0..9 {
                sum[k] += a * f64::from(v[k]);
            }
            for k in 0..3 {
                rank[k].push((p[k], weights[i]));
            }
        }
        total = total.max(f64::EPSILON);
        for values in &mut rank {
            values.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut cumulative = 0.0_f64;
            for value in values {
                cumulative += f64::from(value.1);
                value.1 = (cumulative / total) as f32;
            }
        }
        let mean = std::array::from_fn(|k| (sum[k] / total) as f32);
        let mean_y = (sum[3] / total) as f32;
        let var = (sum[4] / total - f64::from(mean_y).powi(2)).max(f64::from(EPS));
        let slope = std::array::from_fn(|k| {
            ((sum[7 + k] / total - f64::from(mean_y) * sum[5 + k] / total) / var).clamp(-8.0, 8.0)
                as f32
        });
        let white = std::array::from_fn(|k| weighted_value(&rank[k], 0.99).max(EPS));
        let radius = (op.radius * w.min(h) as f32).round().max(1.0) as usize;
        let local = matches!(op.tool, Tool::LocalWienerTone | Tool::GuidedChroma)
            .then(|| Neighborhood::new(src, weights, w, h, radius, op.tool == Tool::GuidedChroma));
        let stencil = matches!(
            op.tool,
            Tool::DarkChannelDehaze
                | Tool::LocalMidrange
                | Tool::MedianChroma
                | Tool::BilateralChroma
        )
        .then_some(LocalConfig {
            w,
            h,
            radius,
            white,
        });
        Self {
            mean,
            mean_y,
            slope,
            rank,
            local,
            stencil,
        }
    }
    // Keep the 25 named operator formulas together so their shared luminance contract is reviewable.
    #[allow(clippy::too_many_lines)]
    pub(super) fn target(
        &self,
        i: usize,
        p: [f32; 3],
        op: &Operation,
        src: &[f32],
        weights: &[f32],
    ) -> [f32; 3] {
        let y = luma(p).max(0.0);
        let c = chroma(p);
        let width = op.width;
        let pivot = op.pivot;
        let target_y = match op.tool {
            Tool::SigmoidContrast => {
                // Normalized logistic in bounded y/(y+pivot), preserving exact black.
                let k = 2.0 + width * 8.0;
                let u = y / (y + pivot);
                let sigmoid = |v: f32| 1.0 / (1.0 + (-k * (v - 0.5)).exp());
                let v = ((sigmoid(u) - sigmoid(0.0)) / (sigmoid(1.0) - sigmoid(0.0)))
                    .clamp(0.0, 1.0 - EPS);
                pivot * v / (1.0 - v)
            }
            Tool::ExponentialShoulder => {
                let k = 1.0 + width * 4.0;
                -(-y * k).exp_m1() / (-(-k).exp_m1())
            }
            Tool::HermiteTone => {
                // Continuous monotone Hermite spline inside [0,1], linear HDR extension.
                let u = y.min(1.0);
                let m = 1.0 / (1.0 + width * 4.0);
                (m * u + (3.0 - 3.0 * m) * u * u + (2.0 * m - 2.0) * u * u * u)
                    + (y - 1.0).max(0.0) * m
            }
            Tool::SchlickBias => {
                let b = (0.5 + width * 0.2).min(0.95);
                let a = 1.0 / b - 2.0;
                let u = y.min(1.0);
                u / (a * (1.0 - u) + 1.0) + (y - 1.0).max(0.0) * (a + 1.0)
            }
            Tool::SolarizeLuma => {
                if y <= pivot {
                    y
                } else {
                    pivot + (y - pivot) * (-((y - pivot) / width)).exp()
                }
            }
            Tool::ZoneExposure => {
                let stops = (y.max(EPS) / pivot).log2();
                y * (width * (-0.5 * stops * stops).exp()).exp2()
            }
            Tool::BlackFloor => (y - pivot).max(0.0) / (1.0 - pivot.min(0.95)),
            Tool::LocalWienerTone => {
                let Some(local) = self.local.as_ref() else {
                    return p;
                };
                let gain = local.variance[i] / (local.variance[i] + width * width);
                local.y[i] + gain * (y - local.y[i])
            }
            _ => y,
        };
        if matches!(
            op.tool,
            Tool::SigmoidContrast
                | Tool::ExponentialShoulder
                | Tool::HermiteTone
                | Tool::SchlickBias
                | Tool::SolarizeLuma
                | Tool::ZoneExposure
                | Tool::BlackFloor
                | Tool::LocalWienerTone
        ) {
            return with_luma(p, target_y);
        }
        if let Some(config) = &self.stencil {
            return local_target(src, weights, config, op, i, p);
        }
        let result = match op.tool {
            Tool::ChromaShear => from_chroma(p, [c[0] + width * c[1], c[1]]),
            Tool::ChromaSpiral => from_chroma(p, rotate(c, width * length(c))),
            Tool::TonalHueTwist => {
                from_chroma(p, rotate(c, width * ((y / pivot).max(EPS).ln()).tanh()))
            }
            Tool::DuotoneGradient => {
                let t = y / (y + pivot);
                let target = chroma(op.target);
                from_chroma(
                    p,
                    [target[0] * (2.0 * t - 1.0), target[1] * (2.0 * t - 1.0)],
                )
            }
            Tool::ChannelCrossfeed => with_luma(
                std::array::from_fn(|k| (p[k] + width * p[(k + 1) % 3]) / (1.0 + width)),
                y,
            ),
            Tool::SelectiveDensity => {
                let target = op.target;
                let scale = target.into_iter().fold(EPS, f32::max);
                // Beer-Lambert channel absorption, driven by an explicit density colour.
                with_luma(
                    std::array::from_fn(|k| p[k] * (-width * target[k] / scale).exp()),
                    y,
                )
            }
            Tool::BradfordAdaptation => adapt(p, self.mean, op.target),
            Tool::ChannelRatioLimit => {
                let floor = p.into_iter().fold(f32::INFINITY, f32::min).max(y * 0.05);
                let ceiling = floor * (1.0 + width * 8.0);
                with_luma(p.map(|v| v.min(ceiling)), y)
            }
            Tool::MedianDespill => {
                let mut sorted = p;
                sorted.sort_by(f32::total_cmp);
                let median = sorted[1];
                with_luma(
                    p.map(|v| v - (v - median).max(0.0) * (width / (1.0 + width))),
                    y,
                )
            }
            Tool::HarmonicHueSnap => {
                let hue = c[1].atan2(c[0]);
                let target = chroma(op.target);
                let origin = target[1].atan2(target[0]);
                let delta = hue - origin;
                let angle = hue - (3.0 * delta).sin() * width.min(1.0) / 3.0;
                from_chroma(p, [length(c) * angle.cos(), length(c) * angle.sin()])
            }
            Tool::ChromaLumaDetrend => from_chroma(
                p,
                std::array::from_fn(|k| c[k] - self.slope[k] * (y - self.mean_y)),
            ),
            Tool::GuidedChroma => {
                let Some(local) = self.local.as_ref() else {
                    return p;
                };
                from_chroma(
                    p,
                    std::array::from_fn(|k| {
                        let slope = (local.yc[k][i] - local.y[i] * local.c[k][i])
                            / (local.variance[i] + width * width);
                        local.c[k][i] + slope * (y - local.y[i])
                    }),
                )
            }
            Tool::ChannelRankAlign => {
                let reference = |q| {
                    self.rank
                        .iter()
                        .enumerate()
                        .map(|(k, v)| LUMA[k] * weighted_value(v, q))
                        .sum::<f32>()
                };
                with_luma(
                    std::array::from_fn(|k| reference(weighted_rank(&self.rank[k], p[k]))),
                    y,
                )
            }
            _ => p,
        };
        result
    }
}

/// Bradford cone response adaptation. Convert linear Rec.2020 to XYZ and back.
fn adapt(p: [f32; 3], source: [f32; 3], target: [f32; 3]) -> [f32; 3] {
    let mul = |m: [[f32; 3]; 3], p: [f32; 3]| {
        m.map(|r| r.into_iter().zip(p).map(|(a, b)| a * b).sum::<f32>())
    };
    let to_xyz = [
        [0.636_958, 0.144_617, 0.168_881],
        [0.2627, 0.677_998, 0.059_302],
        [0.0, 0.028_073, 1.060_985],
    ];
    let from_xyz = [
        [1.716_651, -0.355_671, -0.253_366],
        [-0.666_684, 1.616_481, 0.015_769],
        [0.01764, -0.042_771, 0.942_103],
    ];
    let bradford = [
        [0.8951, 0.2664, -0.1614],
        [-0.7502, 1.7135, 0.0367],
        [0.0389, -0.0685, 1.0296],
    ];
    let inverse = [
        [0.986_993, -0.147_054, 0.159_963],
        [0.432_305, 0.51836, 0.049_291],
        [-0.008_529, 0.040_043, 0.968_487],
    ];
    if luma(source) <= EPS || luma(target) <= EPS {
        return p;
    }
    let a = mul(bradford, mul(to_xyz, source));
    let b = mul(bradford, mul(to_xyz, target));
    let v = mul(bradford, mul(to_xyz, p));
    let transformed = mul(
        from_xyz,
        mul(
            inverse,
            std::array::from_fn(|k| v[k] * (b[k] / a[k].max(EPS)).clamp(0.125, 8.0)),
        ),
    );
    with_luma(transformed.map(|v| v.max(0.0)), luma(p))
}

/// Evaluate the existing bounded stencil during final blending, without a retained RGB plane.
fn local_target(
    src: &[f32],
    weights: &[f32],
    config: &LocalConfig,
    op: &Operation,
    i: usize,
    p: [f32; 3],
) -> [f32; 3] {
    let samples = super::stencil::selected(src, weights, config.w, config.h, config.radius, i);
    match op.tool {
        Tool::DarkChannelDehaze => {
            let dark = samples
                .iter()
                .flat_map(|s| (0..3).map(move |k| s.rgb[k] / config.white[k]))
                .fold(f32::INFINITY, f32::min);
            let transmission = (1.0 - (op.width / (1.0 + op.width)) * dark).clamp(0.15, 1.0);
            std::array::from_fn(|k| {
                ((p[k] - config.white[k]) / transmission + config.white[k]).max(0.0)
            })
        }
        Tool::LocalMidrange => {
            let lo = samples
                .iter()
                .map(|s| luma(s.rgb))
                .fold(f32::INFINITY, f32::min);
            let hi = samples.iter().map(|s| luma(s.rgb)).fold(0.0, f32::max);
            let middle = (lo + hi) * 0.5;
            with_luma(p, middle + (luma(p) - middle) / (1.0 + op.width))
        }
        Tool::MedianChroma => {
            let total = samples.iter().map(|s| s.weight).sum::<f32>();
            let medians = std::array::from_fn(|k| {
                let mut values = [(0.0_f32, 0.0_f32); 49];
                let mut count = 0;
                for sample in samples.iter() {
                    values[count] = (chroma(sample.rgb)[k], sample.weight);
                    count += 1;
                }
                values[..count].sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
                let mut cumulative = 0.0;
                for (value, weight) in &values[..count] {
                    cumulative += weight;
                    if cumulative >= total * 0.5 {
                        return *value;
                    }
                }
                0.0
            });
            from_chroma(p, medians)
        }
        Tool::BilateralChroma => {
            let c = chroma(p);
            let mut sum = [0.0; 2];
            let mut total = 0.0;
            for sample in samples.iter() {
                let cc = chroma(sample.rgb);
                let difference = (cc[0] - c[0]).powi(2) + (cc[1] - c[1]).powi(2);
                let weight = sample.weight
                    * (-0.5 * (sample.distance + difference / (op.width * op.width))).exp();
                total += weight;
                for k in 0..2 {
                    sum[k] += cc[k] * weight;
                }
            }
            if total > 0.0 {
                from_chroma(p, sum.map(|v| v / total))
            } else {
                p
            }
        }
        _ => p,
    }
}
#[cfg(test)]
mod tests;
