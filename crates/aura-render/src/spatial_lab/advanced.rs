//! Original selection-local operators. Work is bounded to 7x7 statistics, a
//! 49-sample aperture, or a 6x6 interpolation footprint per working-grid pixel.
//! No pretrained data, donor pixels, or unselected statistics are consulted.
use super::{distance, luminance, Grid, Operation, Tool};
use rayon::prelude::*;
use std::f32::consts::PI;

type Sample = (isize, isize, [f32; 3], f32);
const Y: [f32; 3] = [0.2627, 0.678, 0.0593];

fn neighborhood(grid: &Grid, i: usize, radius: isize) -> Vec<Sample> {
    let mut out = Vec::with_capacity(((radius * 2 + 1).pow(2)) as usize);
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let j = grid.neighbor(i, dx, dy);
            if grid.weight[j] > 0.0 {
                out.push((dx, dy, grid.pixels[j], grid.weight[j]));
            }
        }
    }
    out
}
fn mean(samples: &[Sample], fallback: [f32; 3]) -> [f32; 3] {
    let total = samples.iter().map(|s| s.3).sum::<f32>();
    if total <= 1e-8 {
        return fallback;
    }
    [0, 1, 2].map(|c| samples.iter().map(|s| s.2[c] * s.3).sum::<f32>() / total)
}
fn quantile(mut values: Vec<(f32, f32)>, fraction: f32) -> f32 {
    values.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
    let target = values.iter().map(|s| s.1).sum::<f32>() * fraction;
    let mut sum = 0.0;
    for (value, weight) in &values {
        sum += weight;
        if sum >= target {
            return *value;
        }
    }
    values.last().map_or(0.0, |s| s.0)
}
fn median(samples: &[Sample]) -> [f32; 3] {
    [0, 1, 2].map(|c| quantile(samples.iter().map(|s| (s.2[c], s.3)).collect(), 0.5))
}
fn chroma(p: [f32; 3]) -> [f32; 2] {
    let y = luminance(p);
    [p[0] - y, p[2] - y]
}
fn with_chroma(p: [f32; 3], uv: [f32; 2]) -> [f32; 3] {
    let y = luminance(p);
    let residual = [uv[0], -(Y[0] * uv[0] + Y[2] * uv[1]) / Y[1], uv[1]];
    // Reduce chroma uniformly if it would create negative light. This retains Y.
    let scale = residual
        .iter()
        .filter(|v| **v < 0.0)
        .fold(1.0_f32, |a, v| a.min(y.max(0.0) / -v));
    residual.map(|v| (y + v * scale).max(0.0))
}
fn with_luma(p: [f32; 3], value: f32) -> [f32; 3] {
    let old = luminance(p);
    if old > 1e-8 {
        p.map(|v| v * value.max(0.0) / old)
    } else {
        [value.max(0.0); 3]
    }
}

fn adaptive_median(grid: &Grid, i: usize, threshold: f32) -> [f32; 3] {
    let p = grid.pixels[i];
    let yp = luminance(p);
    let mut candidate = p;
    for radius in 1..=3 {
        let samples = neighborhood(grid, i, radius);
        candidate = median(&samples);
        let ym = luminance(candidate);
        let low = samples
            .iter()
            .map(|s| luminance(s.2))
            .fold(f32::INFINITY, f32::min);
        let high = samples
            .iter()
            .map(|s| luminance(s.2))
            .fold(f32::NEG_INFINITY, f32::max);
        if ym > low + 1e-6 && ym < high - 1e-6 {
            return if yp > low + threshold && yp < high - threshold {
                p
            } else {
                candidate
            };
        }
    }
    if (yp - luminance(candidate)).abs() > threshold {
        candidate
    } else {
        p
    }
}

// Symmetric Jacobi diagonalization, fixed sweep count. Covariance shrinkage
// works in measured principal components rather than independently in RGB.
// Indexed paired rows preserve the explicit symmetric matrix rotation order.
#[allow(clippy::needless_range_loop)]
fn covariance(samples: &[Sample], p: [f32; 3], noise: f32, chroma_only: bool) -> [f32; 3] {
    let converted: Vec<_> = samples
        .iter()
        .map(|s| {
            let uv = chroma(s.2);
            (
                s.0,
                s.1,
                if chroma_only {
                    [uv[0], uv[1], 0.0]
                } else {
                    s.2
                },
                s.3,
            )
        })
        .collect();
    let uv = chroma(p);
    let point = if chroma_only { [uv[0], uv[1], 0.0] } else { p };
    let centre = mean(&converted, point);
    let total = converted.iter().map(|s| s.3).sum::<f32>().max(1e-8);
    let mut matrix = [[0.0; 3]; 3];
    let mut basis = [[0.0; 3]; 3];
    for a in 0..3 {
        basis[a][a] = 1.0;
        for b in 0..3 {
            matrix[a][b] = converted
                .iter()
                .map(|s| (s.2[a] - centre[a]) * (s.2[b] - centre[b]) * s.3)
                .sum::<f32>()
                / total;
        }
    }
    for _ in 0..5 {
        for (a, b) in [(0, 1), (0, 2), (1, 2)] {
            if matrix[a][b].abs() <= 1e-12 {
                continue;
            }
            let angle = 0.5 * (2.0 * matrix[a][b]).atan2(matrix[b][b] - matrix[a][a]);
            let (s, c) = angle.sin_cos();
            for row in &mut matrix {
                let (x, y) = (row[a], row[b]);
                row[a] = c * x - s * y;
                row[b] = s * x + c * y;
            }
            for col in 0..3 {
                let (x, y) = (matrix[a][col], matrix[b][col]);
                matrix[a][col] = c * x - s * y;
                matrix[b][col] = s * x + c * y;
            }
            for row in &mut basis {
                let (x, y) = (row[a], row[b]);
                row[a] = c * x - s * y;
                row[b] = s * x + c * y;
            }
        }
    }
    let mut output = centre;
    for axis in 0..3 {
        let projected = (0..3)
            .map(|c| (point[c] - centre[c]) * basis[c][axis])
            .sum::<f32>();
        let variance = matrix[axis][axis].max(0.0);
        let gain = (variance - noise * noise).max(0.0) / variance.max(1e-10);
        for c in 0..3 {
            output[c] += projected * gain * basis[c][axis];
        }
    }
    if chroma_only {
        with_chroma(p, [output[0], output[1]])
    } else {
        output
    }
}

// Indexed elimination retains its fixed pivot and accumulation order; converting
// paired mutable matrix rows into iterators would obscure that numerical order.
#[allow(clippy::needless_range_loop)]
fn quadratic(samples: &[Sample], p: [f32; 3], detail: f32) -> [f32; 3] {
    let mut a = [[0.0_f32; 9]; 6];
    for (x, y, rgb, weight) in samples {
        let (x, y) = (*x as f32, *y as f32);
        let row = [1.0, x, y, x * x, x * y, y * y];
        for j in 0..6 {
            for k in 0..6 {
                a[j][k] += row[j] * row[k] * weight;
            }
            for c in 0..3 {
                a[j][6 + c] += row[j] * rgb[c] * weight;
            }
        }
    }
    for (j, row) in a.iter_mut().enumerate() {
        row[j] += 1e-6;
    }
    // Partial pivoting also handles single-pixel or line-shaped selections.
    for pivot in 0..6 {
        let best = (pivot..6)
            .max_by(|x, y| a[*x][pivot].abs().total_cmp(&a[*y][pivot].abs()))
            .unwrap_or(pivot);
        a.swap(pivot, best);
        let divisor = a[pivot][pivot];
        if divisor.abs() < 1e-10 {
            continue;
        }
        for k in pivot..9 {
            a[pivot][k] /= divisor;
        }
        for row in 0..6 {
            if row == pivot {
                continue;
            }
            let factor = a[row][pivot];
            for k in pivot..9 {
                a[row][k] -= factor * a[pivot][k];
            }
        }
    }
    [0, 1, 2].map(|c| p[c] + detail * (p[c] - a[0][6 + c]))
}

fn connected_impulse(grid: &Grid, i: usize, threshold: f32) -> [f32; 3] {
    let p = grid.pixels[i];
    let mut seen = [false; 49];
    let mut queue = vec![(0_isize, 0_isize)];
    seen[24] = true;
    let mut size = 0;
    let mut boundary = Vec::new();
    while let Some((x, y)) = queue.pop() {
        size += 1;
        if size > 4 {
            return p;
        }
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (nx, ny) = (x + dx, y + dy);
            if !(-3..=3).contains(&nx) || !(-3..=3).contains(&ny) {
                continue;
            }
            let index = ((ny + 3) * 7 + nx + 3) as usize;
            if seen[index] {
                continue;
            }
            seen[index] = true;
            let j = grid.neighbor(i, nx, ny);
            if grid.weight[j] <= 0.0 {
                continue;
            }
            if distance(grid.pixels[j], p).sqrt() < threshold {
                queue.push((nx, ny));
            } else {
                boundary.push((nx, ny, grid.pixels[j], grid.weight[j]));
            }
        }
    }
    if boundary.len() >= 3 {
        median(&boundary)
    } else {
        p
    }
}

// The dispatch keeps 25 independent, bounded operators next to their shared
// selection-local statistics; each match arm is an individual method.
#[allow(clippy::too_many_lines)]
fn local(grid: &Grid, i: usize, op: &Operation, air: [f32; 3]) -> [f32; 3] {
    let p = grid.pixels[i];
    let samples = neighborhood(grid, i, 2);
    let base = mean(&samples, p);
    let yp = luminance(p);
    let yb = luminance(base);
    let total = samples.iter().map(|s| s.3).sum::<f32>().max(1e-8);
    let selected = |dx, dy| {
        let j = grid.neighbor(i, dx, dy);
        if grid.weight[j] > 0.0 {
            grid.pixels[j]
        } else {
            p
        }
    };
    match op.tool {
        Tool::AdaptiveMedianImpulse => adaptive_median(grid, i, op.threshold),
        Tool::SigmaRangeMean => {
            let keep: Vec<_> = samples
                .into_iter()
                .filter(|s| distance(s.2, p).sqrt() <= op.threshold)
                .collect();
            mean(&keep, p)
        }
        Tool::SpatialWeightedMedian => {
            let weighted: Vec<_> = samples
                .into_iter()
                .map(|(x, y, p, w)| (x, y, p, w / (1.0 + (x * x + y * y) as f32)))
                .collect();
            median(&weighted)
        }
        Tool::LuminanceWinsor => {
            let values: Vec<_> = samples.iter().map(|s| (luminance(s.2), s.3)).collect();
            let low = quantile(values.clone(), 0.1);
            let high = quantile(values, 0.9);
            with_luma(p, yp.clamp(low, high))
        }
        Tool::ChromaMedian => {
            let u = quantile(samples.iter().map(|s| (chroma(s.2)[0], s.3)).collect(), 0.5);
            let v = quantile(samples.iter().map(|s| (chroma(s.2)[1], s.3)).collect(), 0.5);
            with_chroma(p, [u, v])
        }
        Tool::ChromaBilateral => {
            let uv = chroma(p);
            let mut sum = [0.0; 2];
            let mut weight = 0.0;
            for (x, y, q, w) in &samples {
                let quv = chroma(*q);
                let delta = (quv[0] - uv[0]).powi(2) + (quv[1] - uv[1]).powi(2);
                let a = w * (-delta / op.threshold.powi(2) - (x * x + y * y) as f32 / 8.0).exp();
                for c in 0..2 {
                    sum[c] += quv[c] * a;
                }
                weight += a;
            }
            with_chroma(
                p,
                if weight > 1e-8 {
                    sum.map(|v| v / weight)
                } else {
                    uv
                },
            )
        }
        Tool::ChromaCovariance | Tool::RgbCovariance => {
            covariance(&samples, p, op.threshold, op.tool == Tool::ChromaCovariance)
        }
        Tool::EntropyDetail => {
            let mut histogram = [0.0_f32; 8];
            let low = samples
                .iter()
                .map(|s| luminance(s.2))
                .fold(f32::INFINITY, f32::min);
            let high = samples
                .iter()
                .map(|s| luminance(s.2))
                .fold(f32::NEG_INFINITY, f32::max);
            for s in &samples {
                let bin =
                    (((luminance(s.2) - low) / (high - low).max(1e-8) * 7.999) as usize).min(7);
                histogram[bin] += s.3 / total;
            }
            let entropy = -histogram
                .iter()
                .filter(|h| **h > 0.0)
                .map(|h| h * h.log2())
                .sum::<f32>()
                / 3.0;
            with_luma(p, yp + (yp - yb) * op.detail * entropy)
        }
        Tool::RmsContrast => {
            let variance = samples
                .iter()
                .map(|s| (luminance(s.2) - yb).powi(2) * s.3)
                .sum::<f32>()
                / total;
            let gain = (op.threshold / variance.sqrt().max(1e-6)).clamp(0.25, 4.0);
            with_luma(p, yb + (yp - yb) * gain)
        }
        Tool::LogRetinex => {
            let log_base = samples
                .iter()
                .map(|s| luminance(s.2).max(1e-6).ln() * s.3)
                .sum::<f32>()
                / total;
            let gain = ((op.threshold.max(1e-6).ln() - log_base) * op.detail)
                .exp()
                .clamp(0.25, 4.0);
            p.map(|v| v * gain)
        }
        Tool::PeriodicNotch => {
            let angle = op.angle.to_radians();
            let frequency = op.radius;
            let wave = |x: isize, y: isize| {
                let phase =
                    2.0 * PI * frequency * (x as f32 * angle.cos() + y as f32 * angle.sin());
                [phase.cos(), phase.sin()]
            };
            let centre = [0, 1]
                .map(|c| samples.iter().map(|s| wave(s.0, s.1)[c] * s.3).sum::<f32>() / total);
            let mut matrix = [[0.0_f32; 2]; 2];
            let mut rhs = [0.0; 2];
            for s in &samples {
                let q = wave(s.0, s.1);
                let q = [q[0] - centre[0], q[1] - centre[1]];
                for a in 0..2 {
                    rhs[a] += q[a] * (luminance(s.2) - yb) * s.3;
                    for b in 0..2 {
                        matrix[a][b] += q[a] * q[b] * s.3;
                    }
                }
            }
            matrix[0][0] += 1e-5 * total;
            matrix[1][1] += 1e-5 * total;
            let det = (matrix[0][0] * matrix[1][1] - matrix[0][1] * matrix[1][0]).max(1e-8);
            let a = (rhs[0] * matrix[1][1] - rhs[1] * matrix[0][1]) / det;
            // At the current pixel phase=0, so its fitted periodic contribution
            // is the cosine coefficient. Keep the fitted constant illumination;
            // subtracting a centred basis would leave the box-filtered sinusoid.
            let correction = a.clamp(-op.threshold, op.threshold);
            with_luma(p, yp - correction)
        }
        Tool::CrossMedian => median(
            &samples
                .into_iter()
                .filter(|s| s.0 == 0 || s.1 == 0)
                .collect::<Vec<_>>(),
        ),
        Tool::DirectionalMedian => {
            let mut best = p;
            let mut score = f32::INFINITY;
            for (dx, dy) in [(1, 0), (0, 1), (1, 1), (1, -1)] {
                let values: Vec<_> = samples
                    .iter()
                    .copied()
                    .filter(|s| s.0 * dy == s.1 * dx)
                    .collect();
                let centre = median(&values);
                let weight = values.iter().map(|s| s.3).sum::<f32>().max(1e-8);
                let error = values
                    .iter()
                    .map(|s| distance(s.2, centre) * s.3)
                    .sum::<f32>()
                    / weight;
                if error < score {
                    score = error;
                    best = centre;
                }
            }
            best
        }
        Tool::HessianRidge => {
            let xx = luminance(selected(1, 0)) + luminance(selected(-1, 0)) - 2.0 * yp;
            let yy = luminance(selected(0, 1)) + luminance(selected(0, -1)) - 2.0 * yp;
            let xy = (luminance(selected(1, 1))
                - luminance(selected(-1, 1))
                - luminance(selected(1, -1))
                + luminance(selected(-1, -1)))
                * 0.25;
            let root = ((xx - yy).powi(2) + 4.0 * xy * xy).sqrt();
            let a = (xx + yy + root) * 0.5;
            let b = (xx + yy - root) * 0.5;
            let curvature = if a.abs() > b.abs() { a } else { b };
            with_luma(
                p,
                yp - op.detail * curvature.clamp(-op.threshold, op.threshold) * 0.25,
            )
        }
        Tool::CoherenceDetail => {
            let mut tensor = [0.0_f32; 3];
            for (x, y, _, w) in &samples {
                let j = grid.neighbor(i, *x, *y);
                let q = grid.pixels[j];
                let tap = |dx, dy| {
                    let k = grid.neighbor(j, dx, dy);
                    if grid.weight[k] > 0.0 {
                        luminance(grid.pixels[k])
                    } else {
                        luminance(q)
                    }
                };
                let gx = (tap(1, 0) - tap(-1, 0)) * 0.5;
                let gy = (tap(0, 1) - tap(0, -1)) * 0.5;
                tensor[0] += gx * gx * w;
                tensor[1] += gx * gy * w;
                tensor[2] += gy * gy * w;
            }
            let coherence = ((tensor[0] - tensor[2]).powi(2) + 4.0 * tensor[1].powi(2)).sqrt()
                / (tensor[0] + tensor[2]).max(1e-8);
            with_luma(p, yp + op.detail * coherence * (yp - yb))
        }
        Tool::QuadraticResidual => quadratic(&samples, p, op.detail),
        Tool::ConnectedImpulse => connected_impulse(grid, i, op.threshold),
        Tool::BilateralToneCompress => {
            let range: Vec<_> = samples
                .into_iter()
                .map(|(x, y, q, w)| {
                    (
                        x,
                        y,
                        q,
                        w * (-(luminance(q) - yp).powi(2) / op.threshold.powi(2)).exp(),
                    )
                })
                .collect();
            let base = luminance(mean(&range, p));
            let compressed = base / (1.0 + op.detail.max(0.0) * base);
            with_luma(p, compressed + (yp - base))
        }
        Tool::DarkChannelHaze => {
            let dark = samples
                .iter()
                .map(|s| {
                    (0..3)
                        .map(|c| s.2[c] / air[c].max(1e-4))
                        .fold(f32::INFINITY, f32::min)
                })
                .fold(f32::INFINITY, f32::min);
            let transmission = (1.0 - 0.9 * dark).clamp(op.threshold.max(0.05), 1.0);
            [0, 1, 2].map(|c| ((p[c] - air[c]) / transmission + air[c]).max(0.0))
        }
        _ => optical(grid, i, op),
    }
}

fn sinc(x: f32) -> f32 {
    if x.abs() < 1e-6 {
        1.0
    } else {
        (PI * x).sin() / (PI * x)
    }
}
fn optical(grid: &Grid, i: usize, op: &Operation) -> [f32; 3] {
    let p = grid.pixels[i];
    let x = (i % grid.width) as f32;
    let y = (i / grid.width) as f32;
    if op.tool == Tool::LanczosShift {
        let tx = x + op.shift[0] * grid.width as f32;
        let ty = y + op.shift[1] * grid.height as f32;
        let mut sum = [0.0; 3];
        let mut total = 0.0;
        let mut low = [f32::INFINITY; 3];
        let mut high = [f32::NEG_INFINITY; 3];
        for dy in -2..=3 {
            for dx in -2..=3 {
                let sx = tx.floor() as isize + dx;
                let sy = ty.floor() as isize + dy;
                let j = grid.index(sx, sy);
                if grid.weight[j] <= 0.0 {
                    continue;
                }
                let kx = tx - sx as f32;
                let ky = ty - sy as f32;
                let w = sinc(kx) * sinc(kx / 3.0) * sinc(ky) * sinc(ky / 3.0) * grid.weight[j];
                for c in 0..3 {
                    sum[c] += grid.pixels[j][c] * w;
                    low[c] = low[c].min(grid.pixels[j][c]);
                    high[c] = high[c].max(grid.pixels[j][c]);
                }
                total += w;
            }
        }
        return if total > 1e-6 {
            [0, 1, 2].map(|c| (sum[c] / total).clamp(low[c], high[c]))
        } else {
            p
        };
    }
    let radius = (op.radius * grid.width.min(grid.height) as f32).max(0.5);
    let (s, c) = op.angle.to_radians().sin_cos();
    let mut sum = [0.0; 3];
    let mut total = 0.0;
    let field = [
        x / grid.width as f32 - op.source[0],
        y / grid.height as f32 - op.source[1],
    ];
    for gy in -3_isize..=3 {
        for gx in -3_isize..=3 {
            let u = gx as f32 / 3.0;
            let v = gy as f32 / 3.0;
            let (dx, dy, weight) = match op.tool {
                Tool::EllipticalAperture => (
                    u * radius,
                    v * radius * 0.35,
                    (-2.0 * (u * u + v * v)).exp(),
                ),
                Tool::HexagonalAperture => {
                    if v.abs() > 0.866 || u.abs() + v.abs() / 1.732 > 1.0 {
                        continue;
                    }
                    (u * radius, v * radius, 1.0)
                }
                Tool::CatEyeAperture => {
                    let offset = (field[0] * field[0] + field[1] * field[1]).sqrt().min(0.8);
                    if u * u + v * v > 1.0 || (u + offset).powi(2) + v * v > 1.0 {
                        continue;
                    }
                    (u * radius, v * radius, 1.0)
                }
                Tool::HighlightStreak => {
                    if gy != 0 || gx < 0 {
                        continue;
                    }
                    (u * radius, 0.0, (-3.0 * u).exp())
                }
                _ => return p,
            };
            let Some(q) = grid.guided_sample(i, x + dx * c - dy * s, y + dx * s + dy * c) else {
                continue;
            };
            if op.tool == Tool::HighlightStreak {
                let energy = (luminance(q) - op.threshold).max(0.0);
                let own = (luminance(p) - op.threshold).max(0.0);
                let difference = (energy - own).max(0.0);
                for channel in 0..3 {
                    sum[channel] += q[channel] * difference / luminance(q).max(1e-8) * weight;
                }
            } else {
                for channel in 0..3 {
                    sum[channel] += q[channel] * weight;
                }
            }
            total += weight;
        }
    }
    if total <= 1e-8 {
        p
    } else if op.tool == Tool::HighlightStreak {
        [0, 1, 2].map(|c| p[c] + sum[c] / total)
    } else {
        sum.map(|v| v / total)
    }
}

pub(super) fn evaluate(grid: &Grid, op: &Operation) -> Option<Vec<[f32; 3]>> {
    if !matches!(
        op.tool,
        Tool::AdaptiveMedianImpulse
            | Tool::SigmaRangeMean
            | Tool::SpatialWeightedMedian
            | Tool::LuminanceWinsor
            | Tool::ChromaBilateral
            | Tool::ChromaCovariance
            | Tool::RgbCovariance
            | Tool::EntropyDetail
            | Tool::RmsContrast
            | Tool::LogRetinex
            | Tool::PeriodicNotch
            | Tool::CrossMedian
            | Tool::HessianRidge
            | Tool::CoherenceDetail
            | Tool::DirectionalMedian
            | Tool::QuadraticResidual
            | Tool::LanczosShift
            | Tool::EllipticalAperture
            | Tool::HexagonalAperture
            | Tool::CatEyeAperture
            | Tool::HighlightStreak
            | Tool::BilateralToneCompress
            | Tool::DarkChannelHaze
            | Tool::ConnectedImpulse
            | Tool::ChromaMedian
    ) {
        return None;
    }
    // Airlight is estimated only from selected pixels and shared for the frame.
    let air = grid
        .pixels
        .iter()
        .zip(&grid.weight)
        .filter(|(_, w)| **w > 0.0)
        .max_by(|(a, _), (b, _)| luminance(**a).total_cmp(&luminance(**b)))
        .map_or([1.0; 3], |(p, _)| *p);
    let bound = grid
        .pixels
        .iter()
        .zip(&grid.weight)
        .filter(|(_, w)| **w > 0.0)
        .flat_map(|(p, _)| p.iter().copied())
        .fold(1.0_f32, f32::max)
        .min(f32::MAX / 16.0)
        * 8.0;
    Some(
        (0..grid.pixels.len())
            .into_par_iter()
            .map(|i| {
                if grid.weight[i] <= 0.0 {
                    return grid.pixels[i];
                }
                let output = local(grid, i, op, air);
                // Bounds are scene-relative, retain HDR, and prevent extreme fitted
                // residuals from poisoning subsequent operations.
                [0, 1, 2].map(|c| {
                    if output[c].is_finite() {
                        output[c].clamp(0.0, bound)
                    } else {
                        grid.pixels[i][c]
                    }
                })
            })
            .collect(),
    )
}
