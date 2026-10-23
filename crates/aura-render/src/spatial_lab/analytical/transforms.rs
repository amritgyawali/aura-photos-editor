use super::{luminance, tap, with_luma, Grid, Operation, Tool};
use std::f32::consts::PI;

#[derive(Debug)]
pub(super) struct Context {
    dct: [[f32; 5]; 5],
    fourier: [[(f32, f32); 25]; 25],
}
impl Context {
    pub(super) fn new() -> Self {
        Self {
            dct: std::array::from_fn(|k| {
                std::array::from_fn(|x| {
                    let scale = if k == 0 {
                        (1.0_f32 / 5.0).sqrt()
                    } else {
                        (2.0_f32 / 5.0).sqrt()
                    };
                    scale * (PI * (2 * x + 1) as f32 * k as f32 / 10.0).cos()
                })
            }),
            fourier: std::array::from_fn(|k| {
                std::array::from_fn(|j| {
                    let phase = 2.0 * PI * ((k % 5) * (j % 5) + (k / 5) * (j / 5)) as f32 / 5.0;
                    let (s, c) = phase.sin_cos();
                    (c, s)
                })
            }),
        }
    }
}
fn shrink(v: f32, threshold: f32) -> f32 {
    v.signum() * (v.abs() - threshold).max(0.0)
}
fn dct(grid: &Grid, i: usize, threshold: f32, context: &Context) -> [f32; 3] {
    let p = grid.pixels[i];
    let mut result = 0.0;
    for v in 0..5 {
        for u in 0..5 {
            let mut coefficient = 0.0;
            for y in 0..5 {
                for x in 0..5 {
                    coefficient += luminance(tap(grid, i, x as isize - 2, y as isize - 2))
                        * context.dct[u][x]
                        * context.dct[v][y];
                }
            }
            if u != 0 || v != 0 {
                coefficient = shrink(coefficient, threshold);
            }
            result += coefficient * context.dct[u][2] * context.dct[v][2];
        }
    }
    with_luma(p, result)
}
fn lifting(grid: &Grid, i: usize, threshold: f32) -> [f32; 3] {
    let p = grid.pixels[i];
    let centre = luminance(p);
    let mut result = 0.0;
    for (dx, dy) in [(1, 0), (0, 1)] {
        let a = luminance(tap(grid, i, -2 * dx, -2 * dy));
        let b = luminance(tap(grid, i, -dx, -dy));
        let c = luminance(tap(grid, i, dx, dy));
        let d = luminance(tap(grid, i, 2 * dx, 2 * dy));
        let left = b - (a + centre) * 0.5;
        let right = c - (centre + d) * 0.5;
        let low = centre + (left + right) * 0.25;
        result += low - (shrink(left, threshold) + shrink(right, threshold)) * 0.25;
    }
    with_luma(p, result * 0.5)
}

// The nine-dimensional covariance describes spatial patches, unlike the earlier
// RGB covariance operator. A fixed power iteration estimates its leading mode.
#[allow(clippy::needless_range_loop)]
fn patch_rank(grid: &Grid, i: usize, threshold: f32) -> [f32; 3] {
    let p = grid.pixels[i];
    let mut patches = Vec::new();
    for cy in -1..=1 {
        for cx in -1..=1 {
            let mut patch = [0.0_f32; 9];
            let mut weight = 1.0_f32;
            for y in -1..=1 {
                for x in -1..=1 {
                    let j = grid.neighbor(i, cx + x, cy + y);
                    weight = weight.min(grid.weight[j]);
                    if grid.weight[j] > 0.0 {
                        patch[((y + 1) * 3 + x + 1) as usize] = luminance(grid.pixels[j]);
                    }
                }
            }
            if weight > 0.0 {
                patches.push((patch, weight));
            }
        }
    }
    if patches.len() < 2 {
        return p;
    }
    let total = patches.iter().map(|s| s.1).sum::<f32>().max(1e-8);
    let centre: [f32; 9] =
        std::array::from_fn(|c| patches.iter().map(|s| s.0[c] * s.1).sum::<f32>() / total);
    let matrix: [[f32; 9]; 9] = std::array::from_fn(|a| {
        std::array::from_fn(|b| {
            patches
                .iter()
                .map(|s| (s.0[a] - centre[a]) * (s.0[b] - centre[b]) * s.1)
                .sum::<f32>()
                / total
        })
    });
    let diagonal = (0..9)
        .max_by(|a, b| matrix[*a][*a].total_cmp(&matrix[*b][*b]))
        .unwrap_or(4);
    let mut axis = [0.0; 9];
    axis[diagonal] = 1.0;
    for _ in 0..8 {
        let next: [f32; 9] =
            std::array::from_fn(|a| (0..9).map(|b| matrix[a][b] * axis[b]).sum::<f32>());
        let norm = next.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm < 1e-10 {
            break;
        }
        axis = next.map(|v| v / norm);
    }
    let variance = (0..9)
        .map(|a| axis[a] * (0..9).map(|b| matrix[a][b] * axis[b]).sum::<f32>())
        .sum::<f32>()
        .max(0.0);
    let gain = (variance - threshold.powi(2)).max(0.0) / variance.max(1e-10);
    let mut projected = 0.0;
    for y in -1..=1 {
        for x in -1..=1 {
            let c = ((y + 1) * 3 + x + 1) as usize;
            projected += (luminance(tap(grid, i, x, y)) - centre[c]) * axis[c];
        }
    }
    with_luma(p, centre[4] + gain * projected * axis[4])
}
fn phase(grid: &Grid, i: usize, op: &Operation) -> [f32; 3] {
    let p = grid.pixels[i];
    let centre = luminance(p);
    let mut odd = [0.0; 2];
    let mut even = 0.0;
    let mut total = 0.0;
    for s in super::samples(grid, i, 3) {
        let r2 = (s.0 * s.0 + s.1 * s.1) as f32;
        if r2 < 0.5 {
            continue;
        }
        let w = s.3 / ((1.0 + r2) * r2.sqrt());
        let delta = luminance(s.2) - centre;
        odd[0] += delta * s.0 as f32 * w;
        odd[1] += delta * s.1 as f32 * w;
        even += delta * w;
        total += w;
    }
    if total < 1e-8 {
        return p;
    }
    let residual = -even / total;
    let amplitude = (residual.powi(2) + (odd[0] / total).powi(2) + (odd[1] / total).powi(2)).sqrt();
    with_luma(
        p,
        centre + op.detail * op.threshold * residual / (amplitude + op.threshold),
    )
}
fn spectral(grid: &Grid, i: usize, op: &Operation, context: &Context) -> [f32; 3] {
    let p = grid.pixels[i];
    let patch: [f32; 25] = std::array::from_fn(|j| {
        luminance(tap(grid, i, (j % 5) as isize - 2, (j / 5) as isize - 2))
    });
    let frequency: [(f32, f32); 25] = std::array::from_fn(|k| {
        let real = (0..25)
            .map(|j| patch[j] * context.fourier[k][j].0)
            .sum::<f32>();
        let imaginary = -(0..25)
            .map(|j| patch[j] * context.fourier[k][j].1)
            .sum::<f32>();
        (real, imaginary)
    });
    let log_amplitude = frequency.map(|(r, m)| (r * r + m * m).sqrt().max(1e-6).ln());
    let mut inverse = [0.0; 2];
    let mut energy = 0.0;
    for k in 0..25 {
        let mut average = 0.0;
        for dy in -1..=1 {
            for dx in -1..=1 {
                let x = ((k % 5) as isize + dx).rem_euclid(5) as usize;
                let y = ((k / 5) as isize + dy).rem_euclid(5) as usize;
                average += log_amplitude[y * 5 + x] / 9.0;
            }
        }
        let amplitude = (log_amplitude[k] - average).clamp(-4.0, 4.0).exp();
        let (r, m) = frequency[k];
        let norm = (r * r + m * m).sqrt().max(1e-6);
        let (c, s) = context.fourier[k][12];
        inverse[0] += amplitude * (r * c - m * s) / norm;
        inverse[1] += amplitude * (r * s + m * c) / norm;
        energy += amplitude * amplitude;
    }
    let saliency =
        ((inverse[0].powi(2) + inverse[1].powi(2)) / (energy * 25.0).max(1e-8)).clamp(0.0, 1.0);
    let base = patch.iter().sum::<f32>() / 25.0;
    with_luma(
        p,
        luminance(p) + op.detail * saliency * (luminance(p) - base),
    )
}
pub(super) fn local(grid: &Grid, i: usize, op: &Operation, context: &Context) -> Option<[f32; 3]> {
    Some(match op.tool {
        Tool::LocalDctShrink => dct(grid, i, op.threshold, context),
        Tool::Cdf53Shrink => lifting(grid, i, op.threshold),
        Tool::PatchLowRank => patch_rank(grid, i, op.threshold),
        Tool::PhaseOnlyDetail => phase(grid, i, op),
        Tool::SpectralResidualDetail => spectral(grid, i, op, context),
        _ => return None,
    })
}
