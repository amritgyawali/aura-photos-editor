//! Bounded selected-only measurements for the advanced colour operators.
use super::super::{chroma, dot, length, luma, pixel, Operation, Tool, EPS, LUMA};
pub(super) type Matrix = [[f64; 3]; 3];
pub(super) fn solve(mut matrix: Matrix, mut rhs: [f64; 3]) -> [f32; 3] {
    for col in 0..3 {
        let mut pivot = col;
        for row in col + 1..3 {
            if matrix[row][col].abs() > matrix[pivot][col].abs() {
                pivot = row;
            }
        }
        if matrix[pivot][col].abs() < 1e-12 {
            return [0.0; 3];
        }
        matrix.swap(col, pivot);
        rhs.swap(col, pivot);
        let scale = matrix[col][col];
        for value in &mut matrix[col] {
            *value /= scale;
        }
        rhs[col] /= scale;
        for row in 0..3 {
            if row == col {
                continue;
            }
            let scale = matrix[row][col];
            let pivot_row = matrix[col];
            for (value, pivot) in matrix[row].iter_mut().zip(pivot_row) {
                *value -= scale * pivot;
            }
            rhs[row] -= scale * rhs[col];
        }
    }
    rhs.map(|value| value as f32)
}
fn outer_add(
    matrix: &mut Matrix,
    rhs: &mut [[f64; 3]],
    basis: [f64; 3],
    values: &[f64],
    weight: f64,
) {
    for row in 0..3 {
        for col in 0..3 {
            matrix[row][col] += weight * basis[row] * basis[col];
        }
        for (target, value) in rhs.iter_mut().zip(values) {
            target[row] += weight * basis[row] * value;
        }
    }
}
fn regularized(mut matrix: Matrix, penalty: f64) -> Matrix {
    for (k, row) in matrix.iter_mut().enumerate() {
        row[k] += penalty;
    }
    matrix
}
#[derive(Clone, Copy, Default)]
pub(super) struct Patch {
    pub(super) valid: bool,
    pub(super) mean: [f32; 3],
    pub(super) center: [f32; 2],
    pub(super) covariance: [[f32; 2]; 2],
    pub(super) rgb_covariance: Matrix,
}
pub(super) fn patch(
    src: &[f32],
    weights: &[f32],
    w: usize,
    h: usize,
    point: [f32; 2],
    radius: usize,
) -> Patch {
    let x = ((point[0] * w as f32) as usize).min(w - 1);
    let y = ((point[1] * h as f32) as usize).min(h - 1);
    let samples = super::super::stencil::selected(src, weights, w, h, radius, y * w + x);
    let total = samples.iter().map(|s| f64::from(s.weight)).sum::<f64>();
    if total <= 0.0 {
        return Patch::default();
    }
    let mut result = Patch {
        valid: true,
        ..Patch::default()
    };
    let mut mean = [0.0; 3];
    let mut center = [0.0; 2];
    for s in samples.iter() {
        for (sum, value) in mean.iter_mut().zip(s.rgb) {
            *sum += f64::from(value) * f64::from(s.weight);
        }
        for (sum, value) in center.iter_mut().zip(chroma(s.rgb)) {
            *sum += f64::from(value) * f64::from(s.weight);
        }
    }
    result.mean = mean.map(|v| (v / total) as f32);
    result.center = center.map(|v| (v / total) as f32);
    for s in samples.iter() {
        let a = f64::from(s.weight) / total;
        let c = chroma(s.rgb);
        for row in 0..3 {
            for col in 0..3 {
                result.rgb_covariance[row][col] += a
                    * f64::from(s.rgb[row] - result.mean[row])
                    * f64::from(s.rgb[col] - result.mean[col]);
            }
        }
        for row in 0..2 {
            for col in 0..2 {
                result.covariance[row][col] +=
                    a as f32 * (c[row] - result.center[row]) * (c[col] - result.center[col]);
            }
        }
    }
    result
}
#[derive(Clone, Copy)]
struct Datum {
    p: [f32; 3],
    a: f64,
    x: f64,
    y: f64,
}
pub(super) struct Measured {
    pub(super) mean: [f32; 3],
    pub(super) mean_y: f32,
    pub(super) axis: [f32; 3],
    pub(super) axis_scale: f32,
    pub(super) plane: [[f32; 3]; 2],
    pub(super) position: [f32; 2],
    pub(super) radial: [f32; 2],
    pub(super) ranks: [Vec<(f32, f64)>; 2],
    pub(super) hue_cdf: [f32; 72],
    pub(super) hue_start: f32,
    pub(super) radius_fit: [f32; 3],
    pub(super) clusters: [[f32; 2]; 3],
    pub(super) recovery: [[f32; 3]; 3],
    pub(super) recovery_valid: [bool; 3],
    pub(super) median: [f32; 2],
    pub(super) noise: f32,
}
impl Measured {
    pub(super) fn new(src: &[f32], weights: &[f32], w: usize, h: usize, op: &Operation) -> Self {
        let selected = weights.iter().filter(|a| **a > 0.0).count();
        let stride = selected.div_ceil(8192).max(1);
        let samples: Vec<_> = weights
            .iter()
            .enumerate()
            .filter(|(_, a)| **a > 0.0)
            .step_by(stride)
            .map(|(i, a)| Datum {
                p: pixel(src, i),
                a: f64::from(*a),
                x: ((i % w) as f64 + 0.5) / w as f64,
                y: ((i / w) as f64 + 0.5) / h as f64,
            })
            .collect();
        let total = samples
            .iter()
            .map(|s| s.a)
            .sum::<f64>()
            .max(f64::MIN_POSITIVE);
        let mean = std::array::from_fn(|k| {
            (samples.iter().map(|s| f64::from(s.p[k]) * s.a).sum::<f64>() / total) as f32
        });
        let mean_y = luma(mean);
        let position = [
            samples.iter().map(|s| s.x * s.a).sum::<f64>() / total,
            samples.iter().map(|s| s.y * s.a).sum::<f64>() / total,
        ]
        .map(|v| v as f32);
        let mut result = Self {
            mean,
            mean_y,
            axis: [1.0; 3],
            axis_scale: 1.0,
            plane: [[0.0; 3]; 2],
            position,
            radial: [0.0; 2],
            ranks: std::array::from_fn(|_| Vec::new()),
            hue_cdf: [0.0; 72],
            hue_start: 0.0,
            radius_fit: [0.0; 3],
            clusters: [[0.0; 2]; 3],
            recovery: [[0.0; 3]; 3],
            recovery_valid: [false; 3],
            median: [0.0; 2],
            noise: 0.0,
        };
        match op.tool {
            Tool::SpatialChromaPlane | Tool::RadialLogIllumination | Tool::HueRadiusBalance => {
                result.regression(&samples, total, op);
            }
            Tool::ChromaCopulaMap | Tool::HueDensityEqualize => {
                result.distributions(&samples, total, op);
            }
            Tool::KmeansPaletteRecolor => result.clustering(&samples),
            Tool::ChannelRegressionRecover => result.channel_recovery(&samples, op),
            Tool::PcaMonochrome => result.principal(&samples, total),
            Tool::ChromaGroupLasso | Tool::GeometricMedianNeutral => result.robust(&samples, total),
            _ => (),
        }
        result
    }
    fn regression(&mut self, samples: &[Datum], total: f64, op: &Operation) {
        let mut matrix = [[0.0; 3]; 3];
        let mut rhs = [[0.0; 3]; 2];
        for s in samples {
            let c = chroma(s.p);
            let hue = f64::from(c[1].atan2(c[0]));
            let radius2 =
                (s.x - f64::from(op.source[0])).powi(2) + (s.y - f64::from(op.source[1])).powi(2);
            let (basis, values) = match op.tool {
                Tool::SpatialChromaPlane => ([1.0, s.x, s.y], [f64::from(c[0]), f64::from(c[1])]),
                Tool::RadialLogIllumination => (
                    [1.0, radius2, 0.0],
                    [f64::from(luma(s.p).max(EPS).ln()), radius2],
                ),
                _ => {
                    if length(c) <= EPS {
                        continue;
                    }
                    (
                        [1.0, hue.cos(), hue.sin()],
                        [f64::from(length(c).max(EPS).ln()), 0.0],
                    )
                }
            };
            outer_add(&mut matrix, &mut rhs, basis, &values, s.a);
        }
        let matrix = regularized(matrix, total * 1e-8);
        let fits = rhs.map(|rhs| solve(matrix, rhs));
        match op.tool {
            Tool::SpatialChromaPlane => self.plane = fits,
            Tool::RadialLogIllumination => {
                self.radial = [
                    fits[0][1],
                    (samples
                        .iter()
                        .map(|s| {
                            s.a * ((s.x - f64::from(op.source[0])).powi(2)
                                + (s.y - f64::from(op.source[1])).powi(2))
                        })
                        .sum::<f64>()
                        / total) as f32,
                ];
            }
            _ => self.radius_fit = fits[0],
        }
    }
    fn distributions(&mut self, samples: &[Datum], total: f64, op: &Operation) {
        if op.tool == Tool::ChromaCopulaMap {
            self.ranks = std::array::from_fn(|k| {
                let mut values: Vec<_> = samples.iter().map(|s| (chroma(s.p)[k], s.a)).collect();
                values.sort_by(|a, b| a.0.total_cmp(&b.0));
                let mut sum = 0.0;
                for (_, a) in &mut values {
                    sum += *a;
                    *a = sum / total;
                }
                values
            });
            return;
        }
        let mut hist = [0.0; 72];
        for s in samples {
            let c = chroma(s.p);
            if length(c) > EPS {
                hist[hue_bin(c[1].atan2(c[0]))] += s.a;
            }
        }
        // Start at the least populated angular bin to place the seam away from dense colours.
        let start = hist
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.total_cmp(b.1))
            .map_or(0, |(k, _)| k);
        self.hue_start = start as f32 / 72.0 * std::f32::consts::TAU;
        let total = hist.iter().sum::<f64>().max(f64::MIN_POSITIVE);
        let mut sum = 0.0;
        self.hue_cdf = std::array::from_fn(|k| {
            let a = hist[(k + start) % 72];
            let midpoint = (sum + a * 0.5) / total;
            sum += a;
            midpoint as f32
        });
    }
    fn clustering(&mut self, samples: &[Datum]) {
        if samples.is_empty() {
            return;
        }
        self.clusters[0] = chroma(samples[0].p);
        for k in 1..3 {
            self.clusters[k] = samples
                .iter()
                .map(|s| chroma(s.p))
                .max_by(|a, b| {
                    let distance = |c: [f32; 2]| {
                        self.clusters[..k]
                            .iter()
                            .map(|m| dot([c[0] - m[0], c[1] - m[1]], [c[0] - m[0], c[1] - m[1]]))
                            .fold(f32::INFINITY, f32::min)
                    };
                    distance(*a).total_cmp(&distance(*b))
                })
                .unwrap_or([0.0; 2]);
        }
        for _ in 0..6 {
            let mut sum = [[0.0_f64; 3]; 3];
            for s in samples {
                let c = chroma(s.p);
                let at = nearest(c, &self.clusters);
                sum[at][0] += s.a * f64::from(c[0]);
                sum[at][1] += s.a * f64::from(c[1]);
                sum[at][2] += s.a;
            }
            for (center, sum) in self.clusters.iter_mut().zip(sum) {
                if sum[2] > 0.0 {
                    *center = [(sum[0] / sum[2]) as f32, (sum[1] / sum[2]) as f32];
                }
            }
        }
    }
    fn channel_recovery(&mut self, samples: &[Datum], op: &Operation) {
        for k in 0..3 {
            let mut matrix = [[0.0; 3]; 3];
            let mut rhs = [[0.0; 3]; 1];
            let mut count = 0;
            for s in samples.iter().filter(|s| s.p.iter().all(|v| *v < op.pivot)) {
                let basis = [
                    1.0,
                    f64::from(s.p[(k + 1) % 3]),
                    f64::from(s.p[(k + 2) % 3]),
                ];
                outer_add(&mut matrix, &mut rhs, basis, &[f64::from(s.p[k])], s.a);
                count += 1;
            }
            if count >= 6 {
                self.recovery[k] = solve(regularized(matrix, 1e-8), rhs[0]);
                self.recovery_valid[k] = true;
            }
        }
    }
    fn principal(&mut self, samples: &[Datum], total: f64) {
        let mut cov = [[0.0; 3]; 3];
        for s in samples {
            for (row, values) in cov.iter_mut().enumerate() {
                for (col, value) in values.iter_mut().enumerate() {
                    *value += s.a
                        * f64::from(s.p[row] - self.mean[row])
                        * f64::from(s.p[col] - self.mean[col])
                        / total;
                }
            }
        }
        let mut axis = [1.0_f64; 3];
        for _ in 0..12 {
            let next = cov.map(|row| row.iter().zip(axis).map(|(a, b)| a * b).sum::<f64>());
            let norm = next.iter().map(|v| v * v).sum::<f64>().sqrt();
            if norm < 1e-12 {
                break;
            }
            axis = next.map(|v| v / norm);
        }
        if axis
            .iter()
            .zip(LUMA)
            .map(|(a, b)| a * f64::from(b))
            .sum::<f64>()
            < 0.0
        {
            axis = axis.map(|v| -v);
        }
        self.axis = axis.map(|v| v as f32);
        let quadratic = |v: [f64; 3]| {
            (0..3)
                .map(|r| (0..3).map(|c| v[r] * cov[r][c] * v[c]).sum::<f64>())
                .sum::<f64>()
        };
        self.axis_scale =
            (quadratic(LUMA.map(f64::from)) / quadratic(axis).max(1e-12)).sqrt() as f32;
    }
    fn robust(&mut self, samples: &[Datum], total: f64) {
        let mut center = [0.0_f64; 2];
        for s in samples {
            let c = chroma(s.p);
            for k in 0..2 {
                center[k] += s.a * f64::from(c[k]) / total;
            }
        }
        for _ in 0..16 {
            let mut sum = [0.0; 2];
            let mut weight = 0.0;
            for s in samples {
                let c = chroma(s.p);
                let a = s.a
                    / ((f64::from(c[0]) - center[0]).powi(2)
                        + (f64::from(c[1]) - center[1]).powi(2))
                    .sqrt()
                    .max(1e-5);
                weight += a;
                for k in 0..2 {
                    sum[k] += a * f64::from(c[k]);
                }
            }
            if weight > 0.0 {
                center = sum.map(|v| v / weight);
            }
        }
        self.median = center.map(|v| v as f32);
        let mut deviations: Vec<_> = samples
            .iter()
            .map(|s| {
                (
                    length({
                        let c = chroma(s.p);
                        [c[0] - self.median[0], c[1] - self.median[1]]
                    }),
                    s.a,
                )
            })
            .collect();
        deviations.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut sum = 0.0;
        for (v, a) in deviations {
            sum += a;
            if sum >= total * 0.5 {
                self.noise = v;
                break;
            }
        }
    }
}
pub(super) fn hue_bin(angle: f32) -> usize {
    ((angle.rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU * 72.0) as usize).min(71)
}
pub(super) fn nearest(c: [f32; 2], centers: &[[f32; 2]; 3]) -> usize {
    (0..3)
        .min_by(|a, b| {
            let distance =
                |k: usize| (c[0] - centers[k][0]).powi(2) + (c[1] - centers[k][1]).powi(2);
            distance(*a).total_cmp(&distance(*b))
        })
        .unwrap_or(0)
}
