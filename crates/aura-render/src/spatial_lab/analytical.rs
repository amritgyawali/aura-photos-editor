//! Bounded local mathematical methods, independent of the earlier 75 operators.
//! Footprints reach at most four native pixels; every sampled colour and fitted
//! statistic is conditioned on positive selection coverage.
use super::{distance, luminance, Grid, Operation, Tool};
use rayon::prelude::*;
mod estimators;
mod fields;
mod transforms;

type Sample = (isize, isize, [f32; 3], f32);
fn samples(grid: &Grid, i: usize, radius: isize) -> Vec<Sample> {
    let mut out = Vec::new();
    for y in -radius..=radius {
        for x in -radius..=radius {
            let j = grid.neighbor(i, x, y);
            if grid.weight[j] > 0.0 {
                out.push((x, y, grid.pixels[j], grid.weight[j]));
            }
        }
    }
    out
}
fn mean(values: &[Sample], fallback: [f32; 3]) -> [f32; 3] {
    let weight = values.iter().map(|s| s.3).sum::<f32>();
    if weight <= 1e-8 {
        fallback
    } else {
        [0, 1, 2].map(|c| values.iter().map(|s| s.2[c] * s.3).sum::<f32>() / weight)
    }
}
fn quantile(mut values: Vec<(f32, f32)>, fraction: f32) -> f32 {
    values.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
    let target = values.iter().map(|s| s.1).sum::<f32>() * fraction;
    let mut total = 0.0;
    for (v, w) in &values {
        total += w;
        if total >= target {
            return *v;
        }
    }
    values.last().map_or(0.0, |s| s.0)
}
fn chroma(p: [f32; 3]) -> [f32; 2] {
    let y = luminance(p);
    [p[0] - y, p[2] - y]
}
fn with_chroma(p: [f32; 3], uv: [f32; 2]) -> [f32; 3] {
    let y = luminance(p);
    let residual = [uv[0], -(0.2627 * uv[0] + 0.0593 * uv[1]) / 0.678, uv[1]];
    let scale = residual
        .iter()
        .filter(|v| **v < 0.0)
        .fold(1.0_f32, |a, v| a.min(y.max(0.0) / -v));
    residual.map(|v| (y + v * scale).max(0.0))
}
fn with_luma(p: [f32; 3], value: f32) -> [f32; 3] {
    let y = luminance(p);
    if y > 1e-8 {
        p.map(|v| v * value.max(0.0) / y)
    } else {
        [value.max(0.0); 3]
    }
}
fn tap(grid: &Grid, i: usize, dx: isize, dy: isize) -> [f32; 3] {
    let j = grid.neighbor(i, dx, dy);
    if grid.weight[j] > 0.0 {
        grid.pixels[j]
    } else {
        grid.pixels[i]
    }
}

pub(super) fn supports(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::LocalDctShrink
            | Tool::Cdf53Shrink
            | Tool::GeometricMedian
            | Tool::MeanShiftColour
            | Tool::HodgesLehmann
            | Tool::GeodesicRange
            | Tool::PatchLowRank
            | Tool::MahalanobisRange
            | Tool::LeeSpeckle
            | Tool::FrostSpeckle
            | Tool::GammaMapSpeckle
            | Tool::BiharmonicStencil
            | Tool::FractionalLaplacian
            | Tool::ParabolicBackground
            | Tool::DirectionalScratch
            | Tool::PhaseOnlyDetail
            | Tool::SpectralResidualDetail
            | Tool::HampelImpulse
            | Tool::CircularHueSmooth
            | Tool::ChromaMagnitudeMedian
            | Tool::WhitePatchAdapt
            | Tool::GrayEdgeAdapt
            | Tool::LocalSkewnessBalance
            | Tool::LocalKurtosisDetail
            | Tool::MinmodEdgeLimiter
    )
}
pub(super) fn evaluate(grid: &Grid, op: &Operation) -> Option<Vec<[f32; 3]>> {
    if !supports(op.tool) {
        return None;
    }
    let context = transforms::Context::new();
    Some(
        (0..grid.pixels.len())
            .into_par_iter()
            .map(|i| {
                let p = grid.pixels[i];
                if grid.weight[i] <= 0.0 {
                    return p;
                }
                let values = samples(grid, i, 2);
                let out = estimators::local(grid, i, op, &values)
                    .or_else(|| fields::local(grid, i, op, &values))
                    .or_else(|| transforms::local(grid, i, op, &context))
                    .unwrap_or(p);
                [0, 1, 2].map(|c| {
                    if out[c].is_finite() {
                        out[c].max(0.0)
                    } else {
                        p[c]
                    }
                })
            })
            .collect(),
    )
}
