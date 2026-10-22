//! Explicit optical integration and coordinate corrections, with no learned assets.
use super::{Grid, Operation, Tool};

fn integrate(grid: &Grid, i: usize, op: &Operation) -> [f32; 3] {
    let (x, y) = ((i % grid.width) as f32, (i / grid.width) as f32);
    let (cx, cy) = (
        op.source[0] * grid.width as f32 - 0.5,
        op.source[1] * grid.height as f32 - 0.5,
    );
    let reach = op.radius * grid.width.min(grid.height) as f32;
    let mut sum = [0.0; 3];
    let mut total = 0.0;
    let mut add = |px, py, weight| {
        if let Some(p) = grid.guided_sample(i, px, py) {
            for c in 0..3 {
                sum[c] += p[c] * weight;
            }
            total += weight;
        }
    };
    match op.tool {
        Tool::AnnularBokeh | Tool::AiryDiffraction => {
            if op.tool == Tool::AiryDiffraction {
                add(x, y, 1.0);
            }
            for ring in 1..=3 {
                let radius = if op.tool == Tool::AnnularBokeh {
                    0.6 + ring as f32 * 0.1333
                } else {
                    ring as f32 / 3.0
                };
                // Squared sinc is an explicit radial diffraction approximation,
                // not an exact measured lens point-spread function.
                let argument = radius * std::f32::consts::PI * 2.5;
                let weight = if op.tool == Tool::AnnularBokeh {
                    1.0
                } else {
                    (argument.sin() / argument).powi(2)
                };
                for n in 0..16 {
                    let (sin, cos) = (n as f32 * std::f32::consts::TAU / 16.0).sin_cos();
                    add(x + cos * radius * reach, y + sin * radius * reach, weight);
                }
            }
        }
        Tool::ZoomIntegration => {
            for step in -8..=8 {
                let scale = 1.0 + step as f32 / 8.0 * op.radius;
                add(cx + (x - cx) * scale, cy + (y - cy) * scale, 1.0);
            }
        }
        Tool::TiltShiftIntegration => {
            let (sin, cos) = op.angle.to_radians().sin_cos();
            let across =
                ((x - cx) * (-sin) + (y - cy) * cos).abs() / grid.width.min(grid.height) as f32;
            let distance = ((across - op.threshold).max(0.0) * 4.0).min(1.0) * reach;
            for step in -8..=8 {
                add(
                    x + cos * distance * step as f32 / 8.0,
                    y + sin * distance * step as f32 / 8.0,
                    1.0,
                );
            }
        }
        _ => {}
    }
    if total > 1e-8 {
        sum.map(|v| v / total)
    } else {
        grid.pixels[i]
    }
}

fn warp(grid: &Grid, i: usize, op: &Operation) -> [f32; 3] {
    let p = grid.pixels[i];
    let (x, y) = ((i % grid.width) as f32, (i / grid.width) as f32);
    let (cx, cy) = (
        op.source[0] * grid.width as f32 - 0.5,
        op.source[1] * grid.height as f32 - 0.5,
    );
    let (dx, dy) = ((x - cx) / grid.width as f32, (y - cy) / grid.height as f32);
    if op.tool == Tool::RadialChromatic {
        let r2 = dx * dx + dy * dy;
        let red = grid
            .guided_sample(
                i,
                x + dx * r2 * op.shift[0] * grid.width as f32,
                y + dy * r2 * op.shift[0] * grid.height as f32,
            )
            .unwrap_or(p);
        let blue = grid
            .guided_sample(
                i,
                x + dx * r2 * op.shift[1] * grid.width as f32,
                y + dy * r2 * op.shift[1] * grid.height as f32,
            )
            .unwrap_or(p);
        return [red[0], p[1], blue[2]];
    }
    let (sx, sy) = if op.tool == Tool::PerspectivePlane {
        let denominator = (1.0 + dx * op.shift[0] * 4.0 + dy * op.shift[1] * 4.0).max(0.2);
        (cx + (x - cx) / denominator, cy + (y - cy) / denominator)
    } else {
        let r2 = (dx * dx + dy * dy) / op.radius.powi(2);
        // A compact Gaussian-damped thin-plate radial basis displacement.
        let basis = r2 * (r2 + 1e-6).ln() * (-r2 * 0.5).exp();
        (
            x + op.shift[0] * grid.width as f32 * basis,
            y + op.shift[1] * grid.height as f32 * basis,
        )
    };
    grid.guided_sample(i, sx, sy).unwrap_or(p)
}

pub(super) fn local(grid: &Grid, i: usize, op: &Operation) -> Option<[f32; 3]> {
    match op.tool {
        Tool::AiryDiffraction
        | Tool::AnnularBokeh
        | Tool::TiltShiftIntegration
        | Tool::ZoomIntegration => Some(integrate(grid, i, op)),
        Tool::RadialChromatic | Tool::PerspectivePlane | Tool::ThinPlateWarp => {
            Some(warp(grid, i, op))
        }
        _ => None,
    }
}
