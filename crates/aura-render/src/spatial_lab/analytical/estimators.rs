use super::{
    chroma, distance, luminance, mean, quantile, tap, with_chroma, with_luma, Grid, Operation,
    Sample, Tool,
};

fn geometric(values: &[Sample], p: [f32; 3]) -> [f32; 3] {
    let mut centre = mean(values, p);
    for _ in 0..8 {
        let weighted: Vec<_> = values
            .iter()
            .map(|s| (s.0, s.1, s.2, s.3 / distance(s.2, centre).sqrt().max(1e-5)))
            .collect();
        centre = mean(&weighted, centre);
    }
    centre
}
fn mean_shift(values: &[Sample], p: [f32; 3], threshold: f32) -> [f32; 3] {
    let mut centre = p;
    for _ in 0..5 {
        let weighted: Vec<_> = values
            .iter()
            .map(|s| {
                (
                    s.0,
                    s.1,
                    s.2,
                    s.3 * (-distance(s.2, centre) / threshold.powi(2)).exp(),
                )
            })
            .collect();
        centre = mean(&weighted, centre);
    }
    centre
}
fn hodges(values: &[Sample]) -> [f32; 3] {
    [0, 1, 2].map(|c| {
        let mut pairs = Vec::new();
        for (i, a) in values.iter().enumerate() {
            for b in &values[i..] {
                pairs.push(((a.2[c] + b.2[c]) * 0.5, a.3 * b.3));
            }
        }
        quantile(pairs, 0.5)
    })
}

fn geodesic(grid: &Grid, i: usize, threshold: f32) -> [f32; 3] {
    let mut cost = [f32::INFINITY; 25];
    let mut visited = [false; 25];
    cost[12] = 0.0;
    for _ in 0..25 {
        let next = (0..25)
            .filter(|j| !visited[*j])
            .min_by(|a, b| cost[*a].total_cmp(&cost[*b]));
        let Some(index) = next else {
            break;
        };
        if !cost[index].is_finite() {
            break;
        }
        visited[index] = true;
        let (x, y) = ((index % 5) as isize - 2, (index / 5) as isize - 2);
        let current = tap(grid, i, x, y);
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (nx, ny) = (x + dx, y + dy);
            if !(-2..=2).contains(&nx) || !(-2..=2).contains(&ny) {
                continue;
            }
            let j = grid.neighbor(i, nx, ny);
            if grid.weight[j] <= 0.0 {
                continue;
            }
            let k = ((ny + 2) * 5 + nx + 2) as usize;
            cost[k] = cost[k]
                .min(cost[index] + distance(current, grid.pixels[j]).sqrt() + threshold * 0.15);
        }
    }
    let values: Vec<_> = super::samples(grid, i, 2)
        .into_iter()
        .map(|s| {
            let k = ((s.1 + 2) * 5 + s.0 + 2) as usize;
            (s.0, s.1, s.2, s.3 * (-cost[k] / threshold).exp())
        })
        .collect();
    mean(&values, grid.pixels[i])
}

// Fixed tensor loops preserve accumulation order in the regularized RGB inverse.
#[allow(clippy::needless_range_loop)]
fn mahalanobis(values: &[Sample], p: [f32; 3], noise: f32) -> [f32; 3] {
    let centre = mean(values, p);
    let total = values.iter().map(|s| s.3).sum::<f32>().max(1e-8);
    let mut a = [[0.0_f32; 6]; 3];
    for row in 0..3 {
        a[row][row + 3] = 1.0;
        for col in 0..3 {
            a[row][col] = values
                .iter()
                .map(|s| (s.2[row] - centre[row]) * (s.2[col] - centre[col]) * s.3)
                .sum::<f32>()
                / total;
        }
        a[row][row] += noise.powi(2).max(1e-6);
    }
    for pivot in 0..3 {
        let divisor = a[pivot][pivot].max(1e-10);
        for col in 0..6 {
            a[pivot][col] /= divisor;
        }
        for row in 0..3 {
            if row != pivot {
                let f = a[row][pivot];
                for col in 0..6 {
                    a[row][col] -= f * a[pivot][col];
                }
            }
        }
    }
    let weighted: Vec<_> = values
        .iter()
        .map(|s| {
            let d = [0, 1, 2].map(|c| s.2[c] - p[c]);
            let range = (0..3)
                .map(|r| d[r] * (0..3).map(|c| a[r][c + 3] * d[c]).sum::<f32>())
                .sum::<f32>()
                .max(0.0);
            (
                s.0,
                s.1,
                s.2,
                s.3 * (-0.5 * range - (s.0 * s.0 + s.1 * s.1) as f32 / 8.0).exp(),
            )
        })
        .collect();
    mean(&weighted, p)
}

fn speckle(values: &[Sample], p: [f32; 3], op: &Operation) -> [f32; 3] {
    let centre = luminance(mean(values, p));
    let y = luminance(p);
    let total = values.iter().map(|s| s.3).sum::<f32>().max(1e-8);
    let variance = values
        .iter()
        .map(|s| (luminance(s.2) - centre).powi(2) * s.3)
        .sum::<f32>()
        / total;
    let cv2 = variance / centre.powi(2).max(1e-10);
    let noise = op.threshold.powi(2);
    match op.tool {
        Tool::LeeSpeckle => {
            let gain = ((cv2 - noise) / (cv2 * (1.0 + noise)).max(1e-10)).clamp(0.0, 1.0);
            with_luma(p, centre + (y - centre) * gain)
        }
        Tool::FrostSpeckle => {
            let damping = cv2 / noise.max(1e-8);
            let weighted: Vec<_> = values
                .iter()
                .map(|s| {
                    (
                        s.0,
                        s.1,
                        s.2,
                        s.3 * (-damping * ((s.0 * s.0 + s.1 * s.1) as f32).sqrt()).exp(),
                    )
                })
                .collect();
            with_luma(p, luminance(mean(&weighted, p)))
        }
        _ => {
            // Gamma-MAP's homogeneous, texture and strong-edge regimes use the
            // measured coefficient of variation and authored speckle noise.
            if cv2 <= noise {
                return with_luma(p, centre);
            }
            if cv2 >= 2.0 * noise {
                return p;
            }
            let looks = 1.0 / noise.max(1e-6);
            let alpha = (1.0 + noise) / (cv2 - noise).max(1e-8);
            let b = alpha - looks - 1.0;
            let estimate = (b * centre
                + ((b * centre).powi(2) + 4.0 * alpha * looks * centre * y)
                    .max(0.0)
                    .sqrt())
                / (2.0 * alpha);
            with_luma(p, estimate)
        }
    }
}

fn hue(values: &[Sample], p: [f32; 3], magnitude_only: bool) -> [f32; 3] {
    let uv = chroma(p);
    let magnitude = (uv[0] * uv[0] + uv[1] * uv[1]).sqrt();
    if magnitude < 1e-8 {
        return p;
    }
    if magnitude_only {
        let target = quantile(
            values
                .iter()
                .map(|s| {
                    let q = chroma(s.2);
                    ((q[0] * q[0] + q[1] * q[1]).sqrt(), s.3)
                })
                .collect(),
            0.5,
        );
        return with_chroma(p, uv.map(|v| v * target / magnitude));
    }
    let mut direction = [0.0; 2];
    for s in values {
        let q = chroma(s.2);
        let norm = (q[0] * q[0] + q[1] * q[1]).sqrt();
        if norm > 1e-8 {
            for c in 0..2 {
                direction[c] += q[c] / norm * s.3;
            }
        }
    }
    let norm = (direction[0] * direction[0] + direction[1] * direction[1]).sqrt();
    if norm > 1e-8 {
        with_chroma(p, direction.map(|v| v / norm * magnitude))
    } else {
        p
    }
}

pub(super) fn local(grid: &Grid, i: usize, op: &Operation, values: &[Sample]) -> Option<[f32; 3]> {
    let p = grid.pixels[i];
    Some(match op.tool {
        Tool::GeometricMedian => geometric(values, p),
        Tool::MeanShiftColour => mean_shift(values, p, op.threshold),
        Tool::HodgesLehmann => hodges(values),
        Tool::GeodesicRange => geodesic(grid, i, op.threshold),
        Tool::MahalanobisRange => mahalanobis(values, p, op.threshold),
        Tool::LeeSpeckle | Tool::FrostSpeckle | Tool::GammaMapSpeckle => speckle(values, p, op),
        Tool::HampelImpulse => {
            let median = quantile(values.iter().map(|s| (luminance(s.2), s.3)).collect(), 0.5);
            let mad = quantile(
                values
                    .iter()
                    .map(|s| ((luminance(s.2) - median).abs(), s.3))
                    .collect(),
                0.5,
            );
            if (luminance(p) - median).abs() > (3.0 * 1.4826 * mad).max(op.threshold) {
                with_luma(p, median)
            } else {
                p
            }
        }
        Tool::CircularHueSmooth => hue(values, p, false),
        Tool::ChromaMagnitudeMedian => hue(values, p, true),
        _ => return None,
    })
}
