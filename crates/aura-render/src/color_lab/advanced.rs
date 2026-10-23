//! Independent statistical colour methods. No external weights or training assets.
use super::{
    chroma, dot, from_chroma, length, luma, pixel, quantile, with_luma, Operation, Stats, Tool, EPS,
};
mod measured;
use measured::Measured;

pub(super) const TOOLS: [Tool; 25] = [
    Tool::MedianAnchor,
    Tool::InterquartileContrast,
    Tool::LowerTailLift,
    Tool::UpperTailShoulder,
    Tool::LuminanceSkewCorrect,
    Tool::RmsExposure,
    Tool::HarmonicExposure,
    Tool::EntropyZoneContrast,
    Tool::MinkowskiIlluminant,
    Tool::GrayEdgeIlluminant,
    Tool::TrimmedIlluminant,
    Tool::LogRatioIlluminant,
    Tool::ChromaModeNeutral,
    Tool::TonalCastEqualize,
    Tool::MahalanobisChromaClip,
    Tool::ChromaRadiusEqualize,
    Tool::HueGeodesicPull,
    Tool::PaletteReflection,
    Tool::ColorfulnessNormalize,
    Tool::HighlightHueRestore,
    Tool::LocalChromaCoherence,
    Tool::LocalIlluminationFlatten,
    Tool::LocalLogRatioDenoise,
    Tool::ChromaHueDiffusion,
    Tool::HueSectorCompress,
];

struct Neighborhood {
    first: Vec<f32>,
    second: Vec<f32>,
    variance: Vec<f32>,
}
impl Neighborhood {
    fn new(src: &[f32], weights: &[f32], w: usize, h: usize, op: &Operation) -> Self {
        let radius = (op.radius * w.min(h) as f32).round().max(1.0) as usize;
        let denominator = crate::bands::box_mean(weights, w, h, radius);
        let mean = |values: Vec<f32>| {
            let weighted: Vec<_> = values.iter().zip(weights).map(|(v, a)| v * a).collect();
            crate::bands::box_mean(&weighted, w, h, radius)
                .iter()
                .zip(&denominator)
                .map(|(v, a)| if *a > EPS { v / a } else { 0.0 })
                .collect::<Vec<_>>()
        };
        let values = |axis: usize| {
            (0..weights.len())
                .map(|i| {
                    let p = pixel(src, i);
                    match op.tool {
                        Tool::LocalIlluminationFlatten => luma(p).max(EPS).log2(),
                        Tool::LocalLogRatioDenoise => (p[axis * 2].max(EPS) / p[1].max(EPS)).ln(),
                        Tool::ChromaHueDiffusion => {
                            let c = chroma(p);
                            c[axis] / length(c).max(EPS)
                        }
                        _ => chroma(p)[axis],
                    }
                })
                .collect::<Vec<_>>()
        };
        let first = mean(values(0));
        let second = if op.tool == Tool::LocalIlluminationFlatten {
            Vec::new()
        } else {
            mean(values(1))
        };
        let variance = if op.tool == Tool::LocalChromaCoherence {
            let square = mean(
                (0..weights.len())
                    .map(|i| dot(chroma(pixel(src, i)), chroma(pixel(src, i))))
                    .collect(),
            );
            square
                .iter()
                .zip(first.iter().zip(&second))
                .map(|(v, (a, b))| (v - a * a - b * b).max(0.0))
                .collect()
        } else {
            Vec::new()
        };
        Self {
            first,
            second,
            variance,
        }
    }
}

pub(super) struct Advanced {
    measured: Measured,
    neighborhood: Option<Neighborhood>,
}
impl Advanced {
    pub(super) fn new(src: &[f32], weights: &[f32], w: usize, h: usize, op: &Operation) -> Self {
        let neighborhood = matches!(
            op.tool,
            Tool::HighlightHueRestore
                | Tool::LocalChromaCoherence
                | Tool::LocalIlluminationFlatten
                | Tool::LocalLogRatioDenoise
                | Tool::ChromaHueDiffusion
        )
        .then(|| Neighborhood::new(src, weights, w, h, op));
        Self {
            measured: Measured::new(src, weights, w, h),
            neighborhood,
        }
    }
    fn tone(&self, p: [f32; 3], op: &Operation, stats: &Stats) -> [f32; 3] {
        let y = luma(p).max(0.0);
        let q = |rank| quantile(&stats.hist, rank, stats.lo, stats.hi);
        let median = q(0.5).max(EPS);
        let width = op.width;
        let target = match op.tool {
            Tool::MedianAnchor => y * (op.pivot / median).clamp(0.125, 8.0),
            Tool::InterquartileContrast => {
                let slope = (width / (q(0.75) - q(0.25)).max(EPS)).clamp(0.125, 8.0);
                (op.pivot + (y - median) * slope).max(0.0)
            }
            Tool::LowerTailLift => {
                let floor = q(0.1).max(EPS);
                y * (1.0 + op.pivot / (y + floor * (1.0 + width)))
            }
            Tool::UpperTailShoulder => {
                let knee = q(0.9);
                let excess = (y - knee).max(0.0);
                y.min(knee) + width * (excess / width).tanh()
            }
            Tool::LuminanceSkewCorrect => {
                let span = (stats.hi - stats.lo).max(EPS);
                let u = ((y - stats.lo) / span).clamp(0.0, 1.0);
                // Positive skew needs a concave lift; negative skew a convex compression.
                let exponent = (-self.measured.skew * width).clamp(-2.0, 2.0).exp();
                stats.lo + span * u.powf(exponent)
            }
            Tool::RmsExposure => y * (op.pivot / self.measured.rms.max(EPS)).clamp(0.125, 8.0),
            Tool::HarmonicExposure => {
                y * (op.pivot / self.measured.harmonic.max(EPS)).clamp(0.125, 8.0)
            }
            Tool::EntropyZoneContrast => {
                let u = ((y - stats.lo) / (stats.hi - stats.lo).max(EPS)).clamp(0.0, 1.0);
                let zone = ((u * 4.0) as usize).min(3);
                let total = self.measured.entropy.iter().sum::<f32>().max(EPS);
                let start = self.measured.entropy.iter().take(zone).sum::<f32>() / total;
                stats.lo
                    + (stats.hi - stats.lo)
                        * (start + (u * 4.0 - zone as f32) * self.measured.entropy[zone] / total)
            }
            _ => y,
        };
        with_luma(p, target)
    }
    fn illuminant(&self, p: [f32; 3], op: &Operation) -> [f32; 3] {
        let m = &self.measured;
        let estimate = match op.tool {
            Tool::MinkowskiIlluminant => m.minkowski,
            Tool::GrayEdgeIlluminant => m.edge,
            Tool::TrimmedIlluminant => m.trimmed,
            Tool::LogRatioIlluminant => [m.log_ratio[0].exp(), 1.0, m.log_ratio[1].exp()],
            _ => [1.0; 3],
        };
        // RGB diagonal correction is explicitly a statistical approximation, retaining Y.
        let pivot = luma(estimate).max(EPS);
        with_luma(
            std::array::from_fn(|k| p[k] * (pivot / estimate[k].max(EPS)).clamp(0.125, 8.0)),
            luma(p),
        )
    }
    fn chromatic(
        &self,
        p: [f32; 3],
        op: &Operation,
        stats: &Stats,
        source: [f32; 3],
        second: [f32; 3],
    ) -> [f32; 3] {
        let c = chroma(p);
        let radius = length(c);
        let m = &self.measured;
        let result = match op.tool {
            Tool::ChromaModeNeutral => [c[0] - m.mode[0], c[1] - m.mode[1]],
            Tool::TonalCastEqualize => {
                let u =
                    ((luma(p) - stats.lo) / (stats.hi - stats.lo).max(EPS) * 3.0).clamp(0.0, 3.0);
                let low = (u as usize).min(2);
                let t = u - low as f32;
                std::array::from_fn(|k| {
                    c[k] - m.tonal[low][k] * (1.0 - t) - m.tonal[low + 1][k] * t + stats.center[k]
                })
            }
            Tool::MahalanobisChromaClip => {
                let d = [c[0] - stats.center[0], c[1] - stats.center[1]];
                let perpendicular = [-stats.axis[1], stats.axis[0]];
                let distance = (dot(d, stats.axis).powi(2) / stats.eigen[0]
                    + dot(d, perpendicular).powi(2) / stats.eigen[1])
                    .sqrt();
                let gain = ((1.0 + op.width * 3.0) / distance.max(EPS)).min(1.0);
                std::array::from_fn(|k| stats.center[k] + d[k] * gain)
            }
            Tool::ChromaRadiusEqualize => {
                let low = m.radii.partition_point(|v| *v < radius);
                let high = m.radii.partition_point(|v| *v <= radius);
                let rank = (low + high) as f32 / (2.0 * m.radii.len().max(1) as f32);
                c.map(|v| v * (op.width * rank / radius.max(EPS)).min(8.0))
            }
            Tool::HueGeodesicPull => {
                let from = chroma(source);
                let to = chroma(op.target);
                if length(from) <= EPS || length(to) <= EPS {
                    return p;
                }
                let angle = |q: [f32; 2]| q[1].atan2(q[0]);
                let delta = wrapped(angle(to) - angle(c));
                let gate = (-0.5 * (wrapped(angle(c) - angle(from)) / op.width).powi(2)).exp();
                let hue = angle(c) + delta * gate;
                [radius * hue.cos(), radius * hue.sin()]
            }
            Tool::PaletteReflection => {
                let a = chroma(source);
                let b = chroma(second);
                let d = [b[0] - a[0], b[1] - a[1]];
                let n = [-d[1], d[0]];
                let scale = 2.0 * dot([c[0] - a[0], c[1] - a[1]], n) / dot(n, n).max(EPS);
                [c[0] - n[0] * scale, c[1] - n[1] * scale]
            }
            Tool::ColorfulnessNormalize => {
                c.map(|v| v * (op.width / m.median_radius.max(EPS)).clamp(0.125, 8.0))
            }
            Tool::HueSectorCompress => {
                let target = chroma(op.target);
                if length(target) <= EPS {
                    return p;
                }
                let angle = wrapped(c[1].atan2(c[0]) - target[1].atan2(target[0]));
                let gate = (-0.5 * (angle / op.width).powi(2)).exp();
                c.map(|v| v / (1.0 + gate * radius * 4.0))
            }
            _ => c,
        };
        from_chroma(p, result)
    }
    fn local(&self, i: usize, p: [f32; 3], op: &Operation, stats: &Stats) -> [f32; 3] {
        let Some(local) = &self.neighborhood else {
            return p;
        };
        let c = chroma(p);
        match op.tool {
            Tool::LocalIlluminationFlatten => {
                let shift = (stats.mean_log - local.first[i]).clamp(-4.0, 4.0)
                    * (op.width / (1.0 + op.width));
                with_luma(p, luma(p) * shift.exp2())
            }
            Tool::LocalLogRatioDenoise => {
                let ratio = [
                    local.first[i].clamp(-8.0, 8.0).exp(),
                    1.0,
                    local.second[i].clamp(-8.0, 8.0).exp(),
                ];
                with_luma(ratio, luma(p))
            }
            Tool::ChromaHueDiffusion => {
                let mean = [local.first[i], local.second[i]];
                if length(mean) <= EPS {
                    return p;
                }
                from_chroma(p, mean.map(|v| v * length(c) / length(mean).max(EPS)))
            }
            Tool::LocalChromaCoherence => {
                let mean = [local.first[i], local.second[i]];
                let gate = op.width.powi(2) / (op.width.powi(2) + local.variance[i]);
                from_chroma(p, std::array::from_fn(|k| c[k] + (mean[k] - c[k]) * gate))
            }
            Tool::HighlightHueRestore => {
                let peak = p.into_iter().fold(0.0, f32::max);
                let gate = ((peak - op.pivot) / op.width).clamp(0.0, 1.0);
                from_chroma(
                    p,
                    std::array::from_fn(|k| {
                        c[k] + ([local.first[i], local.second[i]][k] - c[k]) * gate
                    }),
                )
            }
            _ => p,
        }
    }
    pub(super) fn target(
        &self,
        i: usize,
        p: [f32; 3],
        op: &Operation,
        stats: &Stats,
        source: [f32; 3],
        second: [f32; 3],
    ) -> [f32; 3] {
        match op.tool {
            Tool::MedianAnchor
            | Tool::InterquartileContrast
            | Tool::LowerTailLift
            | Tool::UpperTailShoulder
            | Tool::LuminanceSkewCorrect
            | Tool::RmsExposure
            | Tool::HarmonicExposure
            | Tool::EntropyZoneContrast => self.tone(p, op, stats),
            Tool::MinkowskiIlluminant
            | Tool::GrayEdgeIlluminant
            | Tool::TrimmedIlluminant
            | Tool::LogRatioIlluminant => self.illuminant(p, op),
            Tool::HighlightHueRestore
            | Tool::LocalChromaCoherence
            | Tool::LocalIlluminationFlatten
            | Tool::LocalLogRatioDenoise
            | Tool::ChromaHueDiffusion => self.local(i, p, op, stats),
            _ => self.chromatic(p, op, stats, source, second),
        }
    }
}
fn wrapped(angle: f32) -> f32 {
    (angle + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

#[cfg(test)]
mod tests;
