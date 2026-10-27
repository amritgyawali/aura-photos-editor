//! Bounded, selection-aware local estimators in linear working RGB.
use super::{distance, light, luminance, Grid, Operation, Tool};

type Sample = (isize, isize, [f32; 3], f32);

fn neighborhood(grid: &Grid, i: usize) -> Vec<Sample> {
    let mut samples = Vec::with_capacity(25);
    for y in -2..=2 {
        for x in -2..=2 {
            let j = grid.neighbor(i, x, y);
            if grid.weight[j] > 0.0 {
                samples.push((x, y, grid.pixels[j], grid.weight[j]));
            }
        }
    }
    samples
}

fn mean(values: &[Sample]) -> [f32; 3] {
    let total = values.iter().map(|v| v.3).sum::<f32>().max(1e-8);
    [0, 1, 2].map(|c| values.iter().map(|v| v.2[c] * v.3).sum::<f32>() / total)
}

fn location(values: &[Sample], op: &Operation) -> [f32; 3] {
    let mut centre = [0, 1, 2].map(|c| {
        let mut sorted: Vec<_> = values.iter().map(|v| v.2[c]).collect();
        sorted.sort_unstable_by(f32::total_cmp);
        sorted[sorted.len() / 2]
    });
    for _ in 0..4 {
        let mut sum = [0.0; 3];
        let mut total = 0.0;
        for (_, _, p, selected) in values {
            let residual = distance(*p, centre).sqrt() / op.threshold;
            let weight = if op.tool == Tool::HuberLocation {
                1.0 / residual.max(1.0)
            } else {
                (1.0 - residual * residual).max(0.0).powi(2)
            } * selected;
            for c in 0..3 {
                sum[c] += p[c] * weight;
            }
            total += weight;
        }
        if total > 1e-8 {
            centre = sum.map(|v| v / total);
        }
    }
    centre
}

fn plane(values: &[Sample], p: [f32; 3], detail: f32) -> [f32; 3] {
    let mut matrix = [[0.0_f32; 3]; 3];
    let mut rhs = [[0.0_f32; 3]; 3];
    for (x, y, rgb, weight) in values {
        let row = [1.0, *x as f32, *y as f32];
        for a in 0..3 {
            for b in 0..3 {
                matrix[a][b] += row[a] * row[b] * weight;
                rhs[a][b] += row[a] * rgb[b] * weight;
            }
        }
    }
    // Small ridge keeps thin or disconnected selections well-conditioned.
    matrix[1][1] += 1e-5;
    matrix[2][2] += 1e-5;
    for pivot in 0..3 {
        let divisor = matrix[pivot][pivot].max(1e-8);
        for c in 0..3 {
            matrix[pivot][c] /= divisor;
            rhs[pivot][c] /= divisor;
        }
        for row in 0..3 {
            if row != pivot {
                let factor = matrix[row][pivot];
                for c in 0..3 {
                    matrix[row][c] -= matrix[pivot][c] * factor;
                    rhs[row][c] -= rhs[pivot][c] * factor;
                }
            }
        }
    }
    [0, 1, 2].map(|c| p[c] + (p[c] - rhs[0][c]) * detail)
}

fn side_window(grid: &Grid, i: usize) -> [f32; 3] {
    let mut best = grid.pixels[i];
    let mut error = f32::INFINITY;
    for (axis, sign) in [(0, -1), (0, 1), (1, -1), (1, 1)] {
        let values: Vec<_> = neighborhood(grid, i)
            .into_iter()
            .filter(|v| {
                if axis == 0 {
                    v.0 * sign >= 0
                } else {
                    v.1 * sign >= 0
                }
            })
            .collect();
        let candidate = mean(&values);
        let score = distance(candidate, grid.pixels[i]);
        if score < error {
            best = candidate;
            error = score;
        }
    }
    best
}

fn median_means(values: &[Sample]) -> [f32; 3] {
    let groups: Vec<_> = (-2..=2)
        .filter_map(|row| {
            let values: Vec<_> = values.iter().copied().filter(|v| v.1 == row).collect();
            (!values.is_empty()).then(|| mean(&values))
        })
        .collect();
    [0, 1, 2].map(|c| {
        let mut samples: Vec<_> = groups.iter().map(|p| p[c]).collect();
        samples.sort_unstable_by(f32::total_cmp);
        samples[samples.len() / 2]
    })
}

fn chroma_regression(values: &[Sample], p: [f32; 3], epsilon: f32) -> [f32; 3] {
    let average = mean(values);
    let total = values.iter().map(|v| v.3).sum::<f32>();
    let y_mean = luminance(average);
    let variance = values
        .iter()
        .map(|v| (luminance(v.2) - y_mean).powi(2) * v.3)
        .sum::<f32>()
        / total;
    let candidate = [0, 1, 2].map(|c| {
        let covariance = values
            .iter()
            .map(|v| (luminance(v.2) - y_mean) * (v.2[c] - average[c]) * v.3)
            .sum::<f32>()
            / total;
        average[c] + covariance / (variance + epsilon.powi(2)) * (luminance(p) - y_mean)
    });
    light(candidate, luminance(p))
}

fn toggle(values: &[Sample], p: [f32; 3]) -> [f32; 3] {
    let low = values
        .iter()
        .min_by(|a, b| luminance(a.2).total_cmp(&luminance(b.2)))
        .map_or(p, |v| v.2);
    let high = values
        .iter()
        .max_by(|a, b| luminance(a.2).total_cmp(&luminance(b.2)))
        .map_or(p, |v| v.2);
    if distance(p, low) < distance(p, high) {
        low
    } else {
        high
    }
}
fn power_mean(values: &[Sample], op: &Operation) -> [f32; 3] {
    let total = values.iter().map(|v| v.3).sum::<f32>();
    [0, 1, 2].map(|c| {
        if op.tool == Tool::GeometricMean {
            return (values
                .iter()
                .map(|v| v.2[c].max(1e-8).ln() * v.3)
                .sum::<f32>()
                / total)
                .exp();
        }
        let q = op.detail.clamp(-1.0, 2.0);
        let denominator = values
            .iter()
            .map(|v| v.2[c].max(1e-6).powf(q) * v.3)
            .sum::<f32>();
        values
            .iter()
            .map(|v| v.2[c].max(1e-6).powf(q + 1.0) * v.3)
            .sum::<f32>()
            / denominator.max(1e-8)
    })
}

pub(super) fn local(grid: &Grid, i: usize, op: &Operation) -> Option<[f32; 3]> {
    if !matches!(
        op.tool,
        Tool::ToggleMapping
            | Tool::ContraharmonicMean
            | Tool::GeometricMean
            | Tool::HuberLocation
            | Tool::TukeyLocation
            | Tool::LocalPlaneResidual
            | Tool::ChromaGuidedRegression
            | Tool::AnscombeDenoise
            | Tool::SideWindowMean
            | Tool::MedianOfMeans
            | Tool::BilateralSharpen
    ) {
        return None;
    }
    let values = neighborhood(grid, i);
    let p = grid.pixels[i];
    Some(match op.tool {
        Tool::ToggleMapping => toggle(&values, p),
        Tool::ContraharmonicMean | Tool::GeometricMean => power_mean(&values, op),
        Tool::HuberLocation | Tool::TukeyLocation => location(&values, op),
        Tool::LocalPlaneResidual => plane(&values, p, op.detail),
        Tool::ChromaGuidedRegression => chroma_regression(&values, p, op.threshold),
        Tool::AnscombeDenoise => {
            let scale = op.threshold.max(1e-4);
            let total = values.iter().map(|v| v.3).sum::<f32>();
            [0, 1, 2].map(|c| {
                let stabilized = values
                    .iter()
                    .map(|v| 2.0 * (v.2[c].max(0.0) / scale + 0.375).sqrt() * v.3)
                    .sum::<f32>()
                    / total;
                (stabilized * stabilized * 0.25 - 0.375).max(0.0) * scale
            })
        }
        Tool::SideWindowMean => side_window(grid, i),
        Tool::MedianOfMeans => median_means(&values),
        Tool::BilateralSharpen => {
            let mut sum = [0.0; 3];
            let mut total = 0.0;
            for (x, y, q, selected) in &values {
                let weight = (-distance(*q, p) / op.threshold.powi(2)
                    - (*x * *x + *y * *y) as f32 / 8.0)
                    .exp()
                    * selected;
                for c in 0..3 {
                    sum[c] += q[c] * weight;
                }
                total += weight;
            }
            [0, 1, 2].map(|c| p[c] + (p[c] - sum[c] / total.max(1e-8)) * op.detail)
        }
        _ => p,
    })
}
