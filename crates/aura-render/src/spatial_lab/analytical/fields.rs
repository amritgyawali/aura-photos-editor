use super::{luminance, mean, quantile, tap, with_luma, Grid, Operation, Sample, Tool};

fn axis(angle: f32) -> (isize, isize) {
    let (s, c) = angle.to_radians().sin_cos();
    (c.round() as isize, s.round() as isize)
}
fn parabolic(grid: &Grid, i: usize, op: &Operation) -> [f32; 3] {
    let p = grid.pixels[i];
    let mut opening = f32::NEG_INFINITY;
    for y in -2..=2 {
        for x in -2..=2 {
            let mut erosion = f32::INFINITY;
            for v in -2..=2 {
                for u in -2..=2 {
                    erosion = erosion.min(
                        luminance(tap(grid, i, x + u, y + v))
                            + op.threshold * (u * u + v * v) as f32,
                    );
                }
            }
            opening = opening.max(erosion - op.threshold * (x * x + y * y) as f32);
        }
    }
    let y = luminance(p);
    with_luma(p, y + op.detail * (y - opening.min(y)))
}
fn scratch(grid: &Grid, i: usize, op: &Operation) -> [f32; 3] {
    let p = grid.pixels[i];
    let (dx, dy) = axis(op.angle);
    let mut opening = f32::NEG_INFINITY;
    let mut closing = f32::INFINITY;
    for a in -2..=2 {
        let mut low = f32::INFINITY;
        let mut high = f32::NEG_INFINITY;
        for b in -2..=2 {
            let y = luminance(tap(grid, i, (a + b) * dx, (a + b) * dy));
            low = low.min(y);
            high = high.max(y);
        }
        opening = opening.max(low);
        closing = closing.min(high);
    }
    let y = luminance(p);
    let correction = closing - y - (y - opening);
    if correction.abs() > op.threshold {
        with_luma(p, y + correction)
    } else {
        p
    }
}
fn minmod(a: f32, b: f32) -> f32 {
    if a * b > 0.0 {
        a.signum() * a.abs().min(b.abs())
    } else {
        0.0
    }
}
fn limiter(grid: &Grid, i: usize, op: &Operation) -> [f32; 3] {
    let p = grid.pixels[i];
    let (dx, dy) = axis(op.angle);
    let a = tap(grid, i, -2 * dx, -2 * dy);
    let b = tap(grid, i, -dx, -dy);
    let c = tap(grid, i, dx, dy);
    let d = tap(grid, i, 2 * dx, 2 * dy);
    [0, 1, 2].map(|k| {
        let slope = (c[k] - b[k]) * 0.5;
        let left = b[k] + minmod(b[k] - a[k], slope);
        let right = c[k] - minmod(d[k] - c[k], slope);
        p[k] + ((left + right) * 0.5 - p[k]).clamp(-op.threshold, op.threshold)
    })
}
fn adaptation(grid: &Grid, i: usize, values: &[Sample], gray_edge: bool) -> [f32; 3] {
    let p = grid.pixels[i];
    let illuminant = if gray_edge {
        let total = values.iter().map(|s| s.3).sum::<f32>().max(1e-8);
        [0, 1, 2].map(|c| {
            (values
                .iter()
                .map(|s| {
                    let j = grid.neighbor(i, s.0, s.1);
                    let gx = (tap(grid, j, 1, 0)[c] - tap(grid, j, -1, 0)[c]) * 0.5;
                    let gy = (tap(grid, j, 0, 1)[c] - tap(grid, j, 0, -1)[c]) * 0.5;
                    (gx * gx + gy * gy) * s.3
                })
                .sum::<f32>()
                / total)
                .sqrt()
        })
    } else {
        [0, 1, 2].map(|c| quantile(values.iter().map(|s| (s.2[c], s.3)).collect(), 0.9))
    };
    if illuminant.iter().any(|v| *v < 1e-7) {
        return p;
    }
    let neutral = (illuminant[0] * illuminant[1] * illuminant[2]).cbrt();
    let corrected = [0, 1, 2].map(|c| p[c] * (neutral / illuminant[c]).clamp(0.5, 2.0));
    with_luma(corrected, luminance(p))
}
fn moments(values: &[Sample], p: [f32; 3], op: &Operation) -> [f32; 3] {
    let centre = luminance(mean(values, p));
    let total = values.iter().map(|s| s.3).sum::<f32>().max(1e-8);
    let moment = |power| {
        values
            .iter()
            .map(|s| (luminance(s.2) - centre).powi(power) * s.3)
            .sum::<f32>()
            / total
    };
    let variance = moment(2);
    if variance < 1e-10 {
        return p;
    }
    let sigma = variance.sqrt();
    let residual = luminance(p) - centre;
    let correction = if op.tool == Tool::LocalSkewnessBalance {
        let skew = (moment(3) / (variance * sigma)).clamp(-4.0, 4.0);
        -op.detail * skew * sigma * ((residual / sigma).powi(2) - 1.0) * 0.1
    } else {
        let kurtosis = (moment(4) / variance.powi(2)).clamp(0.0, 12.0);
        op.detail * residual * (kurtosis - 3.0) / 6.0
    };
    with_luma(
        p,
        luminance(p) + correction.clamp(-op.threshold, op.threshold),
    )
}

pub(super) fn local(grid: &Grid, i: usize, op: &Operation, values: &[Sample]) -> Option<[f32; 3]> {
    let p = grid.pixels[i];
    Some(match op.tool {
        Tool::BiharmonicStencil => {
            let near = [(1, 0), (-1, 0), (0, 1), (0, -1)];
            let diagonal = [(1, 1), (1, -1), (-1, 1), (-1, -1)];
            let far = [(2, 0), (-2, 0), (0, 2), (0, -2)];
            [0, 1, 2].map(|c| {
                let sum = |offsets: &[(isize, isize)]| {
                    offsets
                        .iter()
                        .map(|(x, y)| tap(grid, i, *x, *y)[c])
                        .sum::<f32>()
                };
                let target = (8.0 * sum(&near) - 2.0 * sum(&diagonal) - sum(&far)) / 20.0;
                p[c] + (target - p[c]).clamp(-op.threshold, op.threshold)
            })
        }
        Tool::FractionalLaplacian => {
            let mut residual = [0.0; 3];
            let mut total = 0.0;
            let alpha = 0.25 + op.threshold * 1.75;
            for s in super::samples(grid, i, 3) {
                let radius = ((s.0 * s.0 + s.1 * s.1) as f32).sqrt();
                if radius < 0.5 {
                    continue;
                }
                let weight = s.3 / radius.powf(2.0 + alpha);
                total += weight;
                for c in 0..3 {
                    residual[c] += (p[c] - s.2[c]) * weight;
                }
            }
            if total > 1e-8 {
                [0, 1, 2].map(|c| p[c] + op.detail * residual[c] / total)
            } else {
                p
            }
        }
        Tool::ParabolicBackground => parabolic(grid, i, op),
        Tool::DirectionalScratch => scratch(grid, i, op),
        Tool::WhitePatchAdapt => adaptation(grid, i, values, false),
        Tool::GrayEdgeAdapt => adaptation(grid, i, values, true),
        Tool::LocalSkewnessBalance | Tool::LocalKurtosisDetail => moments(values, p, op),
        Tool::MinmodEdgeLimiter => limiter(grid, i, op),
        _ => return None,
    })
}
