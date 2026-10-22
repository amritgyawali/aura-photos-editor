//! Original advanced colour transforms: analytical estimates, creative operators and explicit constraints.
use super::{chroma, from_chroma, length, luma, pixel, with_luma, Operation, Tool, EPS, LUMA};
mod math;
mod measured;
use measured::{Measured, Patch};
pub(super) const TOOLS: [Tool; 25] = [
    Tool::SpatialChromaPlane,
    Tool::RadialLogIllumination,
    Tool::GaussianPaletteTransport,
    Tool::PaletteSimplex,
    Tool::HueDensityEqualize,
    Tool::ChromaCopulaMap,
    Tool::HueRadiusBalance,
    Tool::KmeansPaletteRecolor,
    Tool::ChannelRegressionRecover,
    Tool::KubelkaMunkMix,
    Tool::PcaMonochrome,
    Tool::FisherMonochrome,
    Tool::IsoluminantEdgeLight,
    Tool::LuminanceErrorDiffusion,
    Tool::AmplitudeDotscreen,
    Tool::IsoLumaContours,
    Tool::ExposureFusion,
    Tool::PerceptualLightnessLift,
    Tool::UvCastTranslate,
    Tool::ChromaGroupLasso,
    Tool::GeometricMedianNeutral,
    Tool::AitchisonTint,
    Tool::EnergyConservingLightness,
    Tool::DichromaticSpecular,
    Tool::MirrorHueFold,
];
pub(super) struct Scientific {
    measured: Measured,
    first: Patch,
    second: Patch,
    transport: [[f32; 2]; 2],
    fisher: [f32; 3],
    radius: usize,
    shape: [usize; 2],
    available: bool,
}
impl Scientific {
    pub(super) fn new(src: &[f32], weights: &[f32], w: usize, h: usize, op: &Operation) -> Self {
        let radius = (op.radius * w.min(h) as f32).round().max(1.0) as usize;
        let first = measured::patch(src, weights, w, h, op.source, radius);
        let second = measured::patch(src, weights, w, h, op.second, radius);
        let first_required = matches!(
            op.tool,
            Tool::GaussianPaletteTransport
                | Tool::PaletteSimplex
                | Tool::KmeansPaletteRecolor
                | Tool::FisherMonochrome
                | Tool::DichromaticSpecular
        );
        let second_required = matches!(
            op.tool,
            Tool::GaussianPaletteTransport
                | Tool::PaletteSimplex
                | Tool::KmeansPaletteRecolor
                | Tool::FisherMonochrome
        );
        let hue_target_required = matches!(op.tool, Tool::HueDensityEqualize | Tool::MirrorHueFold);
        let available = !(first_required && !first.valid
            || second_required && !second.valid
            || op.tool == Tool::UvCastTranslate && luma(op.target) <= EPS
            || hue_target_required && length(chroma(op.target)) <= EPS);
        let transport = math::transport(
            first.covariance,
            second.covariance,
            op.width.powi(2) * 0.01 + 1e-6,
        );
        let covariance = std::array::from_fn(|r| {
            std::array::from_fn(|c| {
                first.rgb_covariance[r][c]
                    + second.rgb_covariance[r][c]
                    + if r == c {
                        f64::from(op.width.powi(2)) * 0.01 + 1e-6
                    } else {
                        0.0
                    }
            })
        });
        let mut fisher = measured::solve(
            covariance,
            std::array::from_fn(|k| f64::from(second.mean[k] - first.mean[k])),
        );
        let norm = fisher.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > EPS {
            fisher = fisher.map(|v| v / norm);
        } else {
            fisher = LUMA;
        }
        Self {
            measured: Measured::new(src, weights, w, h, op),
            first,
            second,
            transport,
            fisher,
            radius,
            shape: [w, h],
            available,
        }
    }
    pub(super) fn is_available(&self) -> bool {
        self.available
    }
    pub(super) fn target(
        &self,
        i: usize,
        p: [f32; 3],
        op: &Operation,
        src: &[f32],
        weights: &[f32],
    ) -> [f32; 3] {
        let [w, h] = self.shape;
        if !self.available {
            return p;
        }
        match op.tool {
            Tool::RadialLogIllumination
            | Tool::PcaMonochrome
            | Tool::FisherMonochrome
            | Tool::AmplitudeDotscreen
            | Tool::IsoLumaContours
            | Tool::ExposureFusion
            | Tool::PerceptualLightnessLift
            | Tool::EnergyConservingLightness => self.light(i, p, op, w, h),
            Tool::IsoluminantEdgeLight => self.edge(i, p, op, src, weights),
            Tool::ChannelRegressionRecover => self.recover(p, op),
            _ => self.color(i, p, op, w, h),
        }
    }
    fn light(&self, i: usize, p: [f32; 3], op: &Operation, w: usize, h: usize) -> [f32; 3] {
        let y = luma(p);
        let m = &self.measured;
        match op.tool {
            Tool::RadialLogIllumination => {
                let position = [
                    ((i % w) as f32 + 0.5) / w as f32,
                    ((i / w) as f32 + 0.5) / h as f32,
                ];
                let r2 =
                    (position[0] - op.source[0]).powi(2) + (position[1] - op.source[1]).powi(2);
                with_luma(
                    p,
                    y * (-m.radial[0] * (r2 - m.radial[1]) * op.width / (1.0 + op.width))
                        .clamp(-3.0, 3.0)
                        .exp(),
                )
            }
            Tool::PcaMonochrome | Tool::FisherMonochrome => {
                let (axis, scale) = if op.tool == Tool::PcaMonochrome {
                    (m.axis, m.axis_scale)
                } else {
                    (self.fisher, op.width)
                };
                let value = (m.mean_y
                    + (0..3)
                        .map(|k| (p[k] - m.mean[k]) * axis[k] * scale)
                        .sum::<f32>())
                .max(0.0);
                [value; 3]
            }
            Tool::AmplitudeDotscreen => {
                if y >= op.pivot {
                    return with_luma(p, op.pivot);
                }
                let cell = (op.radius * w.min(h) as f32).max(2.0);
                let x = ((i % w) as f32 + 0.5).rem_euclid(cell) / cell - 0.5;
                let yy = ((i / w) as f32 + 0.5).rem_euclid(cell) / cell - 0.5;
                let threshold = (y / op.pivot).clamp(0.0, 1.0) / std::f32::consts::PI;
                let coverage = ((threshold - x * x - yy * yy) * cell + 0.5).clamp(0.0, 1.0);
                with_luma(p, op.pivot * coverage)
            }
            Tool::IsoLumaContours => {
                let phase = (y.max(EPS) / op.pivot).log2() / op.width;
                with_luma(
                    p,
                    y * (0.2 + 0.8 * (std::f32::consts::PI * phase).sin().abs()),
                )
            }
            Tool::ExposureFusion => {
                let mut sum = 0.0;
                let mut total = 0.0;
                for exposure in [0.25, 1.0, 4.0] {
                    let preview = y * exposure / (1.0 + y * exposure);
                    let weight = (-0.5 * ((preview - op.pivot) / (op.width * 0.5 + 0.01)).powi(2))
                        .exp()
                        + 1e-6;
                    sum += weight * (1.0 + exposure) * y / (1.0 + exposure * y);
                    total += weight;
                }
                with_luma(p, sum / total)
            }
            Tool::PerceptualLightnessLift => {
                let mut lab = math::lab(p);
                lab[0] += 20.0 * op.width * (-((lab[0] - 50.0) / 35.0).powi(2)).exp();
                math::from_lab(lab)
            }
            Tool::EnergyConservingLightness => math::energy_projection(p, op.pivot),
            _ => p,
        }
    }
    // Keep the explicit colour-model formulas beside their common luminance-preserving reconstruction.
    #[allow(clippy::too_many_lines)]
    fn color(&self, i: usize, p: [f32; 3], op: &Operation, w: usize, h: usize) -> [f32; 3] {
        let c = chroma(p);
        let radius = length(c);
        let y = luma(p);
        let m = &self.measured;
        let result = match op.tool {
            Tool::SpatialChromaPlane => {
                let x = ((i % w) as f32 + 0.5) / w as f32 - m.position[0];
                let yy = ((i / w) as f32 + 0.5) / h as f32 - m.position[1];
                std::array::from_fn(|k| {
                    c[k] - (m.plane[k][1] * x + m.plane[k][2] * yy) * op.width / (1.0 + op.width)
                })
            }
            Tool::GaussianPaletteTransport => std::array::from_fn(|k| {
                self.second.center[k]
                    + self.transport[k][0] * (c[0] - self.first.center[0])
                    + self.transport[k][1] * (c[1] - self.first.center[1])
            }),
            Tool::PaletteSimplex => math::simplex(
                c,
                [self.first.center, self.second.center, chroma(op.target)],
            ),
            Tool::HueDensityEqualize => {
                let angle = (c[1].atan2(c[0]) - m.hue_start).rem_euclid(std::f32::consts::TAU);
                let rank = m.hue_cdf[measured::hue_bin(angle)];
                let target = chroma(op.target);
                let hue = target[1].atan2(target[0])
                    + (rank - 0.5) * std::f32::consts::TAU * op.width / (1.0 + op.width);
                [radius * hue.cos(), radius * hue.sin()]
            }
            Tool::ChromaCopulaMap => std::array::from_fn(|k| {
                let values = &m.ranks[k];
                let low = values.partition_point(|(v, _)| *v < c[k]);
                let high = values.partition_point(|(v, _)| *v <= c[k]);
                let left = if low == 0 { 0.0 } else { values[low - 1].1 };
                let right = if high == 0 { 0.0 } else { values[high - 1].1 };
                let q = ((left + right) * 0.5).clamp(0.001, 0.999) as f32;
                op.width * (q / (1.0 - q)).ln() / 1.7
            }),
            Tool::HueRadiusBalance => {
                let hue = c[1].atan2(c[0]);
                let trend = m.radius_fit[1] * hue.cos() + m.radius_fit[2] * hue.sin();
                c.map(|v| {
                    v * (-trend * op.width / (1.0 + op.width))
                        .clamp(-2.0, 2.0)
                        .exp()
                })
            }
            Tool::KmeansPaletteRecolor => {
                let at = measured::nearest(c, &m.clusters);
                let anchors = [self.first.center, self.second.center, chroma(op.target)];
                [
                    c[0] + anchors[at][0] - m.clusters[at][0],
                    c[1] + anchors[at][1] - m.clusters[at][1],
                ]
            }
            Tool::ChromaGroupLasso => {
                let lambda = (m.noise + 0.01) * op.width;
                c.map(|v| v * (1.0 - lambda / radius.max(EPS)).max(0.0))
            }
            Tool::GeometricMedianNeutral => [c[0] - m.median[0], c[1] - m.median[1]],
            Tool::MirrorHueFold => {
                let target = chroma(op.target);
                let origin = target[1].atan2(target[0]);
                let width = (op.width * std::f32::consts::PI).clamp(0.05, std::f32::consts::PI);
                let angle = (c[1].atan2(c[0]) - origin + width).rem_euclid(4.0 * width);
                let hue = origin
                    + if angle <= 2.0 * width {
                        angle - width
                    } else {
                        3.0 * width - angle
                    };
                [radius * hue.cos(), radius * hue.sin()]
            }
            _ => {
                return match op.tool {
                    Tool::UvCastTranslate => {
                        let source = math::uv(m.mean);
                        let target = math::uv(op.target);
                        let uv = math::uv(p);
                        math::from_uv(
                            p,
                            [uv[0] + target[0] - source[0], uv[1] + target[1] - source[1]],
                        )
                    }
                    Tool::KubelkaMunkMix => {
                        let mix = op.width / (1.0 + op.width);
                        let target = with_luma(op.target, y);
                        let white = p.into_iter().chain(target).fold(1.0_f32, f32::max);
                        let result = std::array::from_fn(|k| {
                            let ks = |v: f32| {
                                let r = (v / white).clamp(0.001, 1.0);
                                (1.0 - r).powi(2) / (2.0 * r)
                            };
                            let k = ks(p[k]) * (1.0 - mix) + ks(target[k]) * mix;
                            white / (1.0 + k + (k * k + 2.0 * k).sqrt())
                        });
                        with_luma(result, y)
                    }
                    Tool::AitchisonTint => {
                        let t = op.width / (1.0 + op.width);
                        with_luma(
                            std::array::from_fn(|k| {
                                ((1.0 - t) * p[k].max(EPS).ln() + t * op.target[k].max(EPS).ln())
                                    .exp()
                            }),
                            y,
                        )
                    }
                    Tool::DichromaticSpecular => self.specular(p, op),
                    _ => p,
                }
            }
        };
        from_chroma(p, result)
    }
    fn recover(&self, p: [f32; 3], op: &Operation) -> [f32; 3] {
        std::array::from_fn(|k| {
            if p[k] < op.pivot || !self.measured.recovery_valid[k] {
                return p[k];
            }
            let fit = self.measured.recovery[k];
            let predicted = (fit[0] + fit[1] * p[(k + 1) % 3] + fit[2] * p[(k + 2) % 3]).max(0.0);
            p[k] + (predicted.clamp(p[k] * 0.25, p[k] * 4.0) - p[k])
                * ((p[k] - op.pivot) / (op.width + EPS)).clamp(0.0, 1.0)
        })
    }
    fn edge(
        &self,
        i: usize,
        p: [f32; 3],
        op: &Operation,
        src: &[f32],
        weights: &[f32],
    ) -> [f32; 3] {
        let [w, h] = self.shape;
        let samples = super::stencil::selected(src, weights, w, h, self.radius, i);
        let c = chroma(p);
        let mut sum = 0.0;
        let mut total = 0.0;
        for sample in samples.iter() {
            let cc = chroma(sample.rgb);
            let color = length([cc[0] - c[0], cc[1] - c[1]]);
            let contrast = (luma(sample.rgb) - luma(p)).abs() / (luma(p) + op.pivot);
            let a = sample.weight * (-sample.distance).exp();
            sum += a * (color - contrast).max(0.0);
            total += a;
        }
        with_luma(
            p,
            luma(p) * (1.0 + op.width * (sum / total.max(EPS)).tanh()),
        )
    }
    fn specular(&self, p: [f32; 3], op: &Operation) -> [f32; 3] {
        let diffuse = self.first.mean;
        let white = op.target;
        let dot3 = |a: [f32; 3], b: [f32; 3]| a.into_iter().zip(b).map(|(a, b)| a * b).sum::<f32>();
        let aa = dot3(diffuse, diffuse);
        let bb = dot3(white, white);
        let ab = dot3(diffuse, white);
        let det = aa * bb - ab * ab;
        if det <= EPS {
            return p;
        }
        let specular = ((aa * dot3(p, white) - ab * dot3(p, diffuse)) / det).max(0.0);
        let maximum = (0..3)
            .filter(|k| white[*k] > EPS)
            .map(|k| p[k] / white[k])
            .fold(f32::INFINITY, f32::min);
        let removal = specular.min(maximum) * op.width / (1.0 + op.width);
        std::array::from_fn(|k| (p[k] - removal * white[k]).max(0.0))
    }
}
/// Selected-only Floyd-Steinberg diffusion uses two error rows, independent of frame height.
pub(super) fn error_diffusion(
    src: &[f32],
    weights: &[f32],
    w: usize,
    h: usize,
    op: &Operation,
) -> Vec<f32> {
    let mut output = src.to_vec();
    let mut current = vec![0.0_f32; w];
    let mut next = vec![0.0_f32; w];
    let step = op.width;
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if weights[i] <= 0.0 {
                continue;
            }
            let p = pixel(src, i);
            let light = (luma(p) + current[x]).max(0.0);
            let quantized = (light / step).round() * step;
            let candidate = with_luma(p, quantized);
            // Match the generic colour path: extreme HDR arithmetic must never inject infinities.
            let q = if candidate.iter().all(|value| value.is_finite()) {
                candidate
            } else {
                p
            };
            let residual = light - luma(q);
            let error = if residual.is_finite() { residual } else { 0.0 };
            let destinations = [
                (x.wrapping_add(1), y, 7.0),
                (x.wrapping_sub(1), y + 1, 3.0),
                (x, y + 1, 5.0),
                (x.wrapping_add(1), y + 1, 1.0),
            ];
            let denominator = destinations
                .iter()
                .filter(|(xx, yy, _)| *xx < w && *yy < h)
                .map(|(xx, yy, a)| weights[yy * w + xx] * a)
                .sum::<f32>();
            if denominator > 0.0 {
                for (xx, yy, a) in destinations {
                    if xx < w && yy < h {
                        let delta = error * a * weights[yy * w + xx] / denominator;
                        if yy == y {
                            current[xx] += delta;
                        } else {
                            next[xx] += delta;
                        }
                    }
                }
            }
            let alpha = weights[i] * op.amount * op.strength;
            for k in 0..3 {
                let target = p[k] + (q[k] - p[k]) * alpha;
                if target.is_finite() {
                    output[i * 3 + k] = target.max(0.0);
                }
            }
        }
        std::mem::swap(&mut current, &mut next);
        next.fill(0.0);
    }
    output
}
#[cfg(test)]
mod tests;
