//! Independent iterative restoration on the bounded working grid.
use super::{luminance, Grid, Operation, Tool};
use rayon::prelude::*;

fn average(grid: &Grid, pixels: &[[f32; 3]], i: usize) -> [f32; 3] {
    let mut sum = [0.0; 3];
    let mut total = 0.0;
    for (x, y, spatial) in [
        (0, 0, 4.0),
        (-1, 0, 1.0),
        (1, 0, 1.0),
        (0, -1, 1.0),
        (0, 1, 1.0),
    ] {
        let j = grid.neighbor(i, x, y);
        let weight = grid.weight[j] * spatial;
        for c in 0..3 {
            sum[c] += pixels[j][c] * weight;
        }
        total += weight;
    }
    if total > 1e-8 {
        sum.map(|v| v / total)
    } else {
        pixels[i]
    }
}

fn guided(grid: &Grid, epsilon: f32) -> Vec<[f32; 3]> {
    let coefficients: Vec<_> = (0..grid.pixels.len())
        .into_par_iter()
        .map(|i| {
            let mean = grid.selected_mean(i, 2);
            let y_mean = luminance(mean);
            let mut variance = 0.0;
            let mut covariance = [0.0; 3];
            let mut total = 0.0;
            for dy in -2..=2 {
                for dx in -2..=2 {
                    let j = grid.neighbor(i, dx, dy);
                    let weight = grid.weight[j];
                    let residual = luminance(grid.pixels[j]) - y_mean;
                    variance += residual * residual * weight;
                    for c in 0..3 {
                        covariance[c] += residual * (grid.pixels[j][c] - mean[c]) * weight;
                    }
                    total += weight;
                }
            }
            let slope = covariance.map(|v| v / (variance + epsilon.powi(2) * total).max(1e-8));
            (slope, [0, 1, 2].map(|c| mean[c] - slope[c] * y_mean))
        })
        .collect();
    (0..grid.pixels.len())
        .into_par_iter()
        .map(|i| {
            if grid.weight[i] <= 0.0 {
                return grid.pixels[i];
            }
            let mut sum = [0.0; 3];
            let mut total = 0.0;
            for dy in -2..=2 {
                for dx in -2..=2 {
                    let j = grid.neighbor(i, dx, dy);
                    let weight = grid.weight[j];
                    let (a, b) = coefficients[j];
                    for c in 0..3 {
                        sum[c] += (a[c] * luminance(grid.pixels[i]) + b[c]) * weight;
                    }
                    total += weight;
                }
            }
            if total > 1e-8 {
                sum.map(|v| v / total)
            } else {
                grid.pixels[i]
            }
        })
        .collect()
}

fn domain(grid: &Grid, op: &Operation) -> Vec<[f32; 3]> {
    let mut output = grid.pixels.clone();
    let spatial = (op.radius * grid.width.min(grid.height) as f32).max(0.5);
    for axis in [0, 1] {
        for reverse in [false, true] {
            let length = if axis == 0 { grid.width } else { grid.height };
            let lines = if axis == 0 { grid.height } else { grid.width };
            for line in 0..lines {
                for step in 1..length {
                    let coordinate = if reverse { length - 1 - step } else { step };
                    let previous = if reverse {
                        coordinate + 1
                    } else {
                        coordinate - 1
                    };
                    let index = |v| {
                        if axis == 0 {
                            line * grid.width + v
                        } else {
                            v * grid.width + line
                        }
                    };
                    let (i, j) = (index(coordinate), index(previous));
                    if grid.weight[i] <= 0.0 || grid.weight[j] <= 0.0 {
                        continue;
                    }
                    let edge = (luminance(grid.pixels[i]) - luminance(grid.pixels[j])).abs();
                    let a =
                        (-std::f32::consts::SQRT_2 * (1.0 / spatial + edge / op.threshold)).exp();
                    output[i] = [0, 1, 2].map(|c| output[i][c] * (1.0 - a) + output[j][c] * a);
                }
            }
        }
    }
    output
}

fn richardson_lucy(grid: &Grid) -> Vec<[f32; 3]> {
    let mut estimate = grid.pixels.clone();
    for _ in 0..6 {
        let ratio: Vec<_> = (0..estimate.len())
            .into_par_iter()
            .map(|i| {
                let blur = average(grid, &estimate, i);
                [0, 1, 2].map(|c| {
                    if blur[c] > 1e-8 {
                        grid.pixels[i][c] / blur[c]
                    } else {
                        1.0
                    }
                })
            })
            .collect();
        let correction = adjoint_ratio(grid, &ratio);
        estimate = (0..estimate.len())
            .into_par_iter()
            .map(|i| {
                if grid.weight[i] <= 0.0 {
                    return grid.pixels[i];
                }
                [0, 1, 2].map(|c| {
                    (estimate[i][c] * correction[i][c]).clamp(0.0, grid.pixels[i][c].max(1.0) * 4.0)
                })
            })
            .collect();
    }
    estimate
}

fn adjoint_ratio(grid: &Grid, ratio: &[[f32; 3]]) -> Vec<[f32; 3]> {
    let mut sum = vec![[0.0; 3]; ratio.len()];
    let mut normalization = vec![0.0_f32; ratio.len()];
    // Mask-normalized kernels are asymmetric at boundaries. Scatter the actual
    // transpose rather than reuse the forward blur as an approximate adjoint.
    for (i, value) in ratio.iter().enumerate() {
        if grid.weight[i] <= 0.0 {
            continue;
        }
        let taps = [
            (0, 0, 4.0),
            (-1, 0, 1.0),
            (1, 0, 1.0),
            (0, -1, 1.0),
            (0, 1, 1.0),
        ]
        .map(|(dx, dy, kernel)| {
            let j = grid.neighbor(i, dx, dy);
            (j, kernel * grid.weight[j])
        });
        let total = taps.iter().map(|v| v.1).sum::<f32>().max(1e-8);
        for (j, coefficient) in taps {
            let weight = coefficient / total;
            normalization[j] += weight;
            for c in 0..3 {
                sum[j][c] += value[c] * weight;
            }
        }
    }
    sum.into_iter()
        .zip(normalization)
        .map(|(p, weight)| {
            if weight > 1e-8 {
                p.map(|v| v / weight)
            } else {
                [1.0; 3]
            }
        })
        .collect()
}

fn reconstruct(grid: &Grid, closing: bool) -> Vec<[f32; 3]> {
    let extreme = |pixels: &[[f32; 3]], i, minimum: bool| {
        let mut value = pixels[i];
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let j = grid.neighbor(i, dx, dy);
            if grid.weight[j] > 0.0 {
                for c in 0..3 {
                    value[c] = if minimum {
                        value[c].min(pixels[j][c])
                    } else {
                        value[c].max(pixels[j][c])
                    };
                }
            }
        }
        value
    };
    let mut marker: Vec<_> = (0..grid.pixels.len())
        .map(|i| extreme(&grid.pixels, i, !closing))
        .collect();
    // Fixed 16 geodesic steps: a bounded reconstruction distance, not a convergence claim.
    for _ in 0..16 {
        marker = (0..marker.len())
            .into_par_iter()
            .map(|i| {
                if grid.weight[i] <= 0.0 {
                    return grid.pixels[i];
                }
                let propagated = extreme(&marker, i, closing);
                [0, 1, 2].map(|c| {
                    if closing {
                        propagated[c].max(grid.pixels[i][c])
                    } else {
                        propagated[c].min(grid.pixels[i][c])
                    }
                })
            })
            .collect();
    }
    marker
}

fn evolution(grid: &Grid, op: &Operation) -> Vec<[f32; 3]> {
    let mut current = grid.pixels.clone();
    for _ in 0..4 {
        current = (0..current.len())
            .into_par_iter()
            .map(|i| {
                if grid.weight[i] <= 0.0 {
                    return grid.pixels[i];
                }
                let sample = |dx, dy| {
                    let j = grid.neighbor(i, dx, dy);
                    if grid.weight[j] > 0.0 {
                        current[j]
                    } else {
                        current[i]
                    }
                };
                let (l, r, t, b) = (sample(-1, 0), sample(1, 0), sample(0, -1), sample(0, 1));
                let p = current[i];
                [0, 1, 2].map(|c| {
                    let (gx, gy) = ((r[c] - l[c]) * 0.5, (b[c] - t[c]) * 0.5);
                    let (xx, yy) = (r[c] + l[c] - 2.0 * p[c], b[c] + t[c] - 2.0 * p[c]);
                    let xy = (sample(1, 1)[c] - sample(1, -1)[c] - sample(-1, 1)[c]
                        + sample(-1, -1)[c])
                        * 0.25;
                    let delta = if op.tool == Tool::ShockSharpen {
                        -(xx + yy).signum()
                            * (gx * gx + gy * gy).sqrt().min(op.threshold)
                            * op.detail
                    } else {
                        (gy * gy * xx - 2.0 * gx * gy * xy + gx * gx * yy)
                            / (gx * gx + gy * gy + op.threshold.powi(2))
                    };
                    (p[c] + 0.15 * delta).max(0.0)
                })
            })
            .collect();
    }
    current
}

pub(super) fn evaluate(grid: &Grid, op: &Operation) -> Option<Vec<[f32; 3]>> {
    Some(match op.tool {
        Tool::GuidedLinear => guided(grid, op.threshold),
        Tool::DomainTransform => domain(grid, op),
        Tool::RichardsonLucy => richardson_lucy(grid),
        Tool::ShockSharpen | Tool::CurvatureFlow => evolution(grid, op),
        Tool::OpeningReconstruct => reconstruct(grid, false),
        Tool::ClosingReconstruct => reconstruct(grid, true),
        _ => return None,
    })
}
