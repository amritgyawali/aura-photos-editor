//! Bounded local methods run on native-resolution stripes. Geometric and iterative
//! methods retain a 256-pixel working grid; unselected pixels remain exact.
#![allow(clippy::indexing_slicing)]
use crate::cpu::Frame;
use aura_recipe::{
    spatial_lab::{self, Operation, Tool},
    Recipe,
};
use rayon::prelude::*;
const EDGE: usize = 256;
mod advanced;
mod analytical;
mod optics;
mod precision;
mod restoration;
mod statistics;
mod transfer;
fn luminance(p: [f32; 3]) -> f32 {
    p[0] * 0.2627 + p[1] * 0.678 + p[2] * 0.0593
}
fn light(p: [f32; 3], y: f32) -> [f32; 3] {
    let old = luminance(p);
    if old > 1e-6 {
        p.map(|v| v * y.max(0.0) / old)
    } else {
        p
    }
}
fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.into_iter()
        .zip(b)
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        / 3.0
}
#[derive(Debug)]
struct Grid {
    pixels: Vec<[f32; 3]>,
    weight: Vec<f32>,
    width: usize,
    height: usize,
    centre: [f32; 2],
}
impl Grid {
    fn index(&self, x: isize, y: isize) -> usize {
        y.clamp(0, self.height as isize - 1) as usize * self.width
            + x.clamp(0, self.width as isize - 1) as usize
    }
    fn sample(&self, x: f32, y: f32) -> [f32; 3] {
        sample(&self.pixels, self.width, self.height, x, y)
    }
    fn neighbor(&self, i: usize, dx: isize, dy: isize) -> usize {
        self.index(
            (i % self.width) as isize + dx,
            (i / self.width) as isize + dy,
        )
    }
    fn donor(&self, i: usize, op: &Operation) -> [f32; 3] {
        self.sample(
            (i % self.width) as f32 + (op.source[0] - self.centre[0]) * self.width as f32,
            (i / self.width) as f32 + (op.source[1] - self.centre[1]) * self.height as f32,
        )
    }
    fn selected_mean(&self, i: usize, radius: isize) -> [f32; 3] {
        let mut sum = [0.0; 3];
        let mut total = 0.0;
        for y in -radius..=radius {
            for x in -radius..=radius {
                let j = self.neighbor(i, x, y);
                let a = self.weight[j];
                for (c, v) in sum.iter_mut().enumerate() {
                    *v += self.pixels[j][c] * a;
                }
                total += a;
            }
        }
        if total > 1e-6 {
            sum.map(|v| v / total)
        } else {
            self.pixels[i]
        }
    }
    fn guided_sample(&self, i: usize, x: f32, y: f32) -> Option<[f32; 3]> {
        if self.weight[i] <= 0.0 {
            return None;
        }
        // Every bilinear tap must obey the selection. Checking only the nearest
        // cell lets an excluded colour leak through the other three taps.
        let x = x.clamp(0.0, (self.width - 1) as f32);
        let y = y.clamp(0.0, (self.height - 1) as f32);
        let (left, top) = (x.floor() as usize, y.floor() as usize);
        let (fx, fy) = (x - left as f32, y - top as f32);
        let mut sum = [0.0; 3];
        let mut total = 0.0;
        for (dx, dy, interpolation) in [
            (0, 0, (1.0 - fx) * (1.0 - fy)),
            (1, 0, fx * (1.0 - fy)),
            (0, 1, (1.0 - fx) * fy),
            (1, 1, fx * fy),
        ] {
            let j = (top + dy).min(self.height - 1) * self.width + (left + dx).min(self.width - 1);
            let weight = interpolation * self.weight[j];
            for (c, value) in sum.iter_mut().enumerate() {
                *value += self.pixels[j][c] * weight;
            }
            total += weight;
        }
        (total > 1e-6).then(|| sum.map(|v| v / total))
    }
}
fn sample(pixels: &[[f32; 3]], w: usize, h: usize, x: f32, y: f32) -> [f32; 3] {
    let x = x.clamp(0.0, (w - 1) as f32);
    let y = y.clamp(0.0, (h - 1) as f32);
    let (left, top) = (x.floor() as usize, y.floor() as usize);
    let (right, bottom) = ((left + 1).min(w - 1), (top + 1).min(h - 1));
    let (fx, fy) = (x - left as f32, y - top as f32);
    [0, 1, 2].map(|c| {
        pixels[top * w + left][c] * (1.0 - fx) * (1.0 - fy)
            + pixels[top * w + right][c] * fx * (1.0 - fy)
            + pixels[bottom * w + left][c] * (1.0 - fx) * fy
            + pixels[bottom * w + right][c] * fx * fy
    })
}
fn iterative(grid: &Grid, op: &Operation) -> Vec<[f32; 3]> {
    let mut current = grid.pixels.clone();
    for _ in 0..8 {
        let next: Vec<_> = current
            .par_iter()
            .enumerate()
            .map(|(i, p)| {
                if grid.weight[i] <= 0.0 {
                    return *p;
                }
                let mut delta = [0.0; 3];
                let mut total = 0.0;
                for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let j = grid.neighbor(i, dx, dy);
                    let d = luminance(current[j]) - luminance(*p);
                    let selected = grid.weight[j];
                    let weight = match op.tool {
                        Tool::AnisotropicDiffusion => {
                            (-(d / op.threshold).powi(2)).exp() * selected
                        }
                        Tool::TotalVariation => {
                            selected / (d * d + op.threshold * op.threshold).sqrt()
                        }
                        _ => 1.0,
                    };
                    let target = if op.tool == Tool::PoissonDonor {
                        let donor = grid.donor(i, op);
                        let neighbor = grid.donor(j, op);
                        [0, 1, 2].map(|c| current[j][c] + donor[c] - neighbor[c])
                    } else {
                        current[j]
                    };
                    for c in 0..3 {
                        delta[c] += (target[c] - p[c]) * weight;
                    }
                    total += weight;
                }
                let rate = if matches!(op.tool, Tool::HarmonicFill | Tool::PoissonDonor) {
                    0.9
                } else {
                    0.35
                };
                [0, 1, 2].map(|c| p[c] + delta[c] / total.max(1e-6) * rate)
            })
            .collect();
        current = next;
    }
    current
}
fn robust(grid: &Grid, i: usize, op: &Operation) -> [f32; 3] {
    let p = grid.pixels[i];
    let mut neighbors = Vec::with_capacity(25);
    for y in -2..=2 {
        for x in -2..=2 {
            let j = grid.neighbor(i, x, y);
            if grid.weight[j] > 0.0 {
                neighbors.push(grid.pixels[j]);
            }
        }
    }
    if neighbors.is_empty() {
        return p;
    }
    if op.tool == Tool::VectorMedoid {
        return neighbors
            .iter()
            .copied()
            .min_by(|a, b| {
                let score = |v| {
                    neighbors
                        .iter()
                        .map(|q| distance(v, *q).sqrt())
                        .sum::<f32>()
                };
                score(*a).total_cmp(&score(*b))
            })
            .unwrap_or(p);
    }
    if op.tool == Tool::TrimmedMean {
        return [0, 1, 2].map(|c| {
            let mut v: Vec<_> = neighbors.iter().map(|p| p[c]).collect();
            v.sort_unstable_by(f32::total_cmp);
            let trim = v.len() / 5;
            let retained = &v[trim..v.len() - trim];
            retained.iter().sum::<f32>() / retained.len() as f32
        });
    }
    let mean = grid.selected_mean(i, 2);
    let variance =
        neighbors.iter().map(|q| distance(*q, mean)).sum::<f32>() / neighbors.len() as f32;
    if op.tool == Tool::AdaptiveWiener {
        let gain = (1.0 - op.threshold.powi(2) / variance.max(1e-8)).clamp(0.0, 1.0);
        return [0, 1, 2].map(|c| mean[c] + (p[c] - mean[c]) * gain);
    }
    if op.tool == Tool::LocalRankContrast {
        let y = luminance(p);
        // Midranks keep tied samples centered: a flat patch must remain flat
        // instead of receiving the darkest possible rank at every pixel.
        let rank = neighbors
            .iter()
            .map(|q| match luminance(*q).total_cmp(&y) {
                std::cmp::Ordering::Less => 1.0,
                std::cmp::Ordering::Equal => 0.5,
                std::cmp::Ordering::Greater => 0.0,
            })
            .sum::<f32>()
            / neighbors.len() as f32;
        return light(p, y + (rank - 0.5) * op.threshold * op.detail);
    }
    let range = neighbors
        .iter()
        .map(|q| luminance(*q))
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), v| {
            (lo.min(v), hi.max(v))
        });
    if op.tool == Tool::MosquitoSuppress {
        let residual = (luminance(p) - luminance(mean)).abs();
        if range.1 - range.0 > op.threshold * 3.0 && residual < op.threshold {
            return [0, 1, 2].map(|c| mean[c] + (p[c] - mean[c]) * 0.3);
        }
    }
    p
}
fn nonlocal(grid: &Grid, i: usize, op: &Operation) -> [f32; 3] {
    let mut total = 0.0;
    let mut sum = [0.0; 3];
    for dy in -3..=3 {
        for dx in -3..=3 {
            let j = grid.neighbor(i, dx, dy);
            if grid.weight[j] <= 0.0 {
                continue;
            }
            let mut error = 0.0;
            let mut count = 0.0;
            for py in -1..=1 {
                for px in -1..=1 {
                    let a = grid.neighbor(i, px, py);
                    let b = grid.neighbor(j, px, py);
                    if grid.weight[a] > 0.0 && grid.weight[b] > 0.0 {
                        error += distance(grid.pixels[a], grid.pixels[b]);
                        count += 1.0;
                    }
                }
            }
            if count <= 0.0 {
                continue;
            }
            let weight = (-error / count / op.threshold.powi(2)).exp() * grid.weight[j];
            for (c, value) in sum.iter_mut().enumerate() {
                *value += grid.pixels[j][c] * weight;
            }
            total += weight;
        }
    }
    if total > 1e-6 {
        sum.map(|v| v / total)
    } else {
        grid.pixels[i]
    }
}
fn haar(grid: &Grid, i: usize, op: &Operation) -> [f32; 3] {
    let x = (i % grid.width) / 2 * 2;
    let y = (i / grid.width) / 2 * 2;
    // Clamping an incomplete block duplicates its last row/column, so the
    // inverse transform no longer conserves the mean of the actual pixels.
    if x + 1 >= grid.width || y + 1 >= grid.height {
        return grid.pixels[i];
    }
    let ids = [
        grid.index(x as isize, y as isize),
        grid.index(x as isize + 1, y as isize),
        grid.index(x as isize, y as isize + 1),
        grid.index(x as isize + 1, y as isize + 1),
    ];
    if ids.iter().any(|j| grid.weight[*j] <= 0.0) {
        return grid.pixels[i];
    }
    let signs = match (i % grid.width % 2, i / grid.width % 2) {
        (0, 0) => [1.0, 1.0, 1.0],
        (1, 0) => [-1.0, 1.0, -1.0],
        (0, 1) => [1.0, -1.0, -1.0],
        _ => [-1.0, -1.0, 1.0],
    };
    [0, 1, 2].map(|c| {
        let [a, b, c0, d] = ids.map(|j| grid.pixels[j][c]);
        let average = (a + b + c0 + d) * 0.25;
        let coefficients = [
            (a - b + c0 - d) * 0.25,
            (a + b - c0 - d) * 0.25,
            (a - b - c0 + d) * 0.25,
        ];
        let residual = coefficients
            .into_iter()
            .zip(signs)
            .map(|(value, sign)| {
                let adjusted = if op.tool == Tool::HaarDenoise {
                    value.signum() * (value.abs() - op.threshold).max(0.0)
                } else {
                    value * (1.0 + op.detail)
                };
                adjusted * sign
            })
            .sum::<f32>();
        average + residual
    })
}
fn kuwahara(grid: &Grid, i: usize) -> [f32; 3] {
    let mut best = grid.pixels[i];
    let mut best_score = f32::INFINITY;
    for (sx, sy) in [(-1, -1), (1, -1), (-1, 1), (1, 1)] {
        let mut values = Vec::with_capacity(9);
        for dy in 0..=2 {
            for dx in 0..=2 {
                let j = grid.neighbor(i, dx * sx, dy * sy);
                if grid.weight[j] > 0.0 {
                    values.push(grid.pixels[j]);
                }
            }
        }
        if values.is_empty() {
            continue;
        }
        let mean =
            [0, 1, 2].map(|c| values.iter().map(|v| v[c]).sum::<f32>() / values.len() as f32);
        let variance = values.iter().map(|v| distance(*v, mean)).sum::<f32>() / values.len() as f32;
        if variance < best_score {
            best = mean;
            best_score = variance;
        }
    }
    best
}
fn flow(grid: &Grid, i: usize, op: &Operation) -> [f32; 3] {
    let mut tensor = [0.0; 3];
    for y in -1..=1 {
        for x in -1..=1 {
            let j = grid.neighbor(i, x, y);
            let gx = luminance(grid.pixels[grid.neighbor(j, 1, 0)])
                - luminance(grid.pixels[grid.neighbor(j, -1, 0)]);
            let gy = luminance(grid.pixels[grid.neighbor(j, 0, 1)])
                - luminance(grid.pixels[grid.neighbor(j, 0, -1)]);
            tensor[0] += gx * gx;
            tensor[1] += gx * gy;
            tensor[2] += gy * gy;
        }
    }
    let angle = 0.5 * (2.0 * tensor[1]).atan2(tensor[0] - tensor[2]) + std::f32::consts::FRAC_PI_2;
    let (sin, cos) = angle.sin_cos();
    let mut sum = [0.0; 3];
    let mut total = 0.0;
    for step in -3..=3 {
        let x = (i % grid.width) as f32 + cos * step as f32;
        let y = (i / grid.width) as f32 + sin * step as f32;
        if let Some(q) = grid.guided_sample(i, x, y) {
            let a = (-distance(q, grid.pixels[i]) / op.threshold.powi(2)).exp();
            for (c, v) in sum.iter_mut().enumerate() {
                *v += q[c] * a;
            }
            total += a;
        }
    }
    if total > 1e-6 {
        sum.map(|v| v / total)
    } else {
        grid.pixels[i]
    }
}
fn optical(grid: &Grid, i: usize, op: &Operation) -> [f32; 3] {
    let (x, y) = ((i % grid.width) as f32, (i / grid.width) as f32);
    let radius = op.radius * grid.width.min(grid.height) as f32;
    let mut sum = [0.0; 3];
    let mut total = 0.0;
    let mut add = |px, py, weight: f32| {
        if let Some(q) = grid.guided_sample(i, px, py) {
            for (c, v) in sum.iter_mut().enumerate() {
                *v += q[c] * weight;
            }
            total += weight;
        }
    };
    match op.tool {
        Tool::ApertureBokeh => {
            add(x, y, 1.0);
            for ring in 1..=2 {
                for n in 0..12 {
                    let angle = n as f32 * std::f32::consts::TAU / 12.0;
                    let (s, c) = angle.sin_cos();
                    add(
                        x + c * radius * ring as f32 / 2.0,
                        y + s * radius * ring as f32 / 2.0,
                        1.0,
                    );
                }
            }
        }
        Tool::RotationBlur => {
            let (cx, cy) = (
                op.source[0] * grid.width as f32 - 0.5,
                op.source[1] * grid.height as f32 - 0.5,
            );
            for n in -6..=6 {
                let angle = n as f32 / 6.0 * op.angle.to_radians() * op.radius;
                let (s, c) = angle.sin_cos();
                add(
                    cx + (x - cx) * c - (y - cy) * s,
                    cy + (x - cx) * s + (y - cy) * c,
                    1.0,
                );
            }
        }
        Tool::TrajectoryBlur => {
            for point in &op.path {
                add(
                    x + point[0] * grid.width as f32,
                    y + point[1] * grid.height as f32,
                    1.0,
                );
            }
        }
        Tool::Starburst => {
            for ray in 0..4 {
                let angle = op.angle.to_radians() + ray as f32 * std::f32::consts::FRAC_PI_4;
                let (s, c) = angle.sin_cos();
                for n in -4..=4 {
                    let px = x + c * radius * n as f32 / 4.0;
                    let py = y + s * radius * n as f32 / 4.0;
                    if let Some(q) = grid.guided_sample(i, px, py) {
                        let weight = ((luminance(q) - op.threshold) / luminance(q).max(1e-6))
                            .clamp(0.0, 1.0);
                        add(px, py, weight / (1.0 + (n as f32).abs()));
                    }
                }
            }
        }
        _ => {}
    }
    if total <= 1e-6 {
        return grid.pixels[i];
    }
    let mean = sum.map(|v| v / total);
    if op.tool == Tool::Starburst {
        [0, 1, 2].map(|c| grid.pixels[i][c] + mean[c] * op.detail.max(0.0) * 0.2)
    } else {
        mean
    }
}
fn morphology(grid: &Grid, i: usize, op: &Operation) -> [f32; 3] {
    let extreme = |index, minimum: bool| {
        let mut found = luminance(grid.pixels[index]);
        for dy in -1..=1 {
            for dx in -1..=1 {
                let j = grid.neighbor(index, dx, dy);
                if grid.weight[j] > 0.0 {
                    let y = luminance(grid.pixels[j]);
                    found = if minimum { found.min(y) } else { found.max(y) };
                }
            }
        }
        found
    };
    let mut opened = f32::NEG_INFINITY;
    let mut closed = f32::INFINITY;
    for dy in -1..=1 {
        for dx in -1..=1 {
            let j = grid.neighbor(i, dx, dy);
            if grid.weight[j] > 0.0 {
                opened = opened.max(extreme(j, true));
                closed = closed.min(extreme(j, false));
            }
        }
    }
    let y = luminance(grid.pixels[i]);
    let bright = (y - opened).max(0.0).min(op.threshold);
    let dark = (closed - y).max(0.0).min(op.threshold);
    light(grid.pixels[i], y + (dark - bright) * op.detail)
}
fn donor(grid: &Grid, i: usize, op: &Operation) -> [f32; 3] {
    let original = grid.pixels[i];
    if op.tool == Tool::DonorBilateral {
        let donor = grid.donor(i, op);
        let mut sum = [0.0; 3];
        let mut total = 0.0;
        for y in -2..=2 {
            for x in -2..=2 {
                let j = grid.neighbor(i, x, y);
                if grid.weight[j] <= 0.0 {
                    continue;
                }
                let q = grid.donor(j, op);
                let a = (-distance(q, donor) / op.threshold.powi(2)).exp();
                for (c, v) in sum.iter_mut().enumerate() {
                    *v += q[c] * a;
                }
                total += a;
            }
        }
        return if total > 1e-6 {
            let mean = sum.map(|v| v / total);
            let own = grid.selected_mean(i, 2);
            [0, 1, 2].map(|c| mean[c] + original[c] - own[c])
        } else {
            original
        };
    }
    // Candidate patches retain the actual donor's texture; a deterministic seam score
    // chooses among nine offset footprints rather than inventing trained detail.
    let mut best = grid.donor(i, op);
    let mut error = f32::INFINITY;
    let (tile_x, tile_y) = ((i % grid.width) / 4 * 4, (i / grid.width) / 4 * 4);
    for oy in -1..=1 {
        for ox in -1..=1 {
            let offset = [ox as f32 * 4.0, oy as f32 * 4.0];
            let mut score = 0.0;
            for n in 0..4 {
                let j = grid.index(tile_x as isize + n, tile_y as isize);
                let q = grid.sample(
                    (j % grid.width) as f32
                        + (op.source[0] - grid.centre[0]) * grid.width as f32
                        + offset[0],
                    (j / grid.width) as f32
                        + (op.source[1] - grid.centre[1]) * grid.height as f32
                        + offset[1],
                );
                score += distance(q, grid.pixels[j]);
            }
            if score < error {
                error = score;
                best = grid.sample(
                    (i % grid.width) as f32
                        + (op.source[0] - grid.centre[0]) * grid.width as f32
                        + offset[0],
                    (i / grid.width) as f32
                        + (op.source[1] - grid.centre[1]) * grid.height as f32
                        + offset[1],
                );
            }
        }
    }
    best
}
fn local(grid: &Grid, i: usize, op: &Operation) -> [f32; 3] {
    let p = grid.pixels[i];
    match op.tool {
        Tool::NonLocalMeans => nonlocal(grid, i, op),
        Tool::VectorMedoid
        | Tool::TrimmedMean
        | Tool::AdaptiveWiener
        | Tool::LocalRankContrast
        | Tool::MosquitoSuppress => robust(grid, i, op),
        Tool::HaarDenoise | Tool::HaarDetail => haar(grid, i, op),
        Tool::Kuwahara => kuwahara(grid, i),
        Tool::TensorFlow => flow(grid, i, op),
        Tool::ApertureBokeh | Tool::Starburst | Tool::RotationBlur | Tool::TrajectoryBlur => {
            optical(grid, i, op)
        }
        Tool::MorphologyClean => morphology(grid, i, op),
        Tool::DonorBilateral | Tool::TextureQuilt => donor(grid, i, op),
        Tool::ChannelRegister => {
            let x = (i % grid.width) as f32;
            let y = (i / grid.width) as f32;
            let shifted = grid
                .guided_sample(
                    i,
                    x + op.shift[0] * grid.width as f32,
                    y + op.shift[1] * grid.height as f32,
                )
                .unwrap_or(p);
            let opposite = grid
                .guided_sample(
                    i,
                    x - op.shift[0] * grid.width as f32,
                    y - op.shift[1] * grid.height as f32,
                )
                .unwrap_or(p);
            [shifted[0], p[1], opposite[2]]
        }
        Tool::LaplacianDetail => {
            let fine = grid.selected_mean(i, 1);
            let broad = grid.selected_mean(i, 3);
            [0, 1, 2].map(|c| p[c] + (fine[c] - broad[c]) * op.detail)
        }
        Tool::BlockDeblock => {
            let edge = if (i % grid.width).is_multiple_of(8) {
                Some((-1, 0))
            } else if (i / grid.width).is_multiple_of(8) {
                Some((0, -1))
            } else {
                None
            };
            if let Some((dx, dy)) = edge {
                let j = grid.neighbor(i, dx, dy);
                if grid.weight[j] > 0.0 && distance(p, grid.pixels[j]) < op.threshold.powi(2) {
                    return [0, 1, 2].map(|c| (p[c] + grid.pixels[j][c]) * 0.5);
                }
            }
            p
        }
        Tool::EdgeDirectedSmooth => {
            let gradient = |dx, dy| {
                (luminance(grid.pixels[grid.neighbor(i, dx, dy)])
                    - luminance(grid.pixels[grid.neighbor(i, -dx, -dy)]))
                .abs()
            };
            let (dx, dy) = if gradient(1, 0) < gradient(0, 1) {
                (1, 0)
            } else {
                (0, 1)
            };
            let a = grid.neighbor(i, dx, dy);
            let b = grid.neighbor(i, -dx, -dy);
            if grid.weight[a] > 0.0 && grid.weight[b] > 0.0 {
                [0, 1, 2].map(|c| (p[c] * 2.0 + grid.pixels[a][c] + grid.pixels[b][c]) * 0.25)
            } else {
                p
            }
        }
        _ => p,
    }
}
fn evaluate(grid: &Grid, op: &Operation) -> Vec<[f32; 3]> {
    if let Some(output) = analytical::evaluate(grid, op) {
        return output;
    }
    if let Some(output) = advanced::evaluate(grid, op) {
        return output;
    }
    if let Some(output) = restoration::evaluate(grid, op) {
        return output;
    }
    if matches!(
        op.tool,
        Tool::AnisotropicDiffusion | Tool::TotalVariation | Tool::HarmonicFill | Tool::PoissonDonor
    ) {
        iterative(grid, op)
    } else {
        (0..grid.pixels.len())
            .into_par_iter()
            .map(|i| {
                if grid.weight[i] > 0.0 {
                    statistics::local(grid, i, op)
                        .or_else(|| optics::local(grid, i, op))
                        .unwrap_or_else(|| local(grid, i, op))
                } else {
                    grid.pixels[i]
                }
            })
            .collect()
    }
}
fn process(rgb: &mut [f32], width: usize, height: usize, weights: &[f32], op: &Operation) {
    if op.amount <= 0.0 {
        return;
    }
    if precision::supports(op.tool) && precision::process(rgb, width, height, weights, op) {
        return;
    }
    let ratio = (EDGE as f32 / width.max(height) as f32).min(1.0);
    let (w, h) = (
        ((width as f32 * ratio).round() as usize).max(1),
        ((height as f32 * ratio).round() as usize).max(1),
    );
    let mut grid = Grid {
        pixels: Vec::with_capacity(w * h),
        weight: Vec::with_capacity(w * h),
        width: w,
        height: h,
        centre: [0.0; 2],
    };
    let mut total = 0.0;
    for y in 0..h {
        for x in 0..w {
            let sx = ((x as f32 + 0.5) * width as f32 / w as f32).floor() as usize;
            let sy = ((y as f32 + 0.5) * height as f32 / h as f32).floor() as usize;
            let i = sy.min(height - 1) * width + sx.min(width - 1);
            let a = weights[i];
            grid.pixels
                .push([rgb[i * 3], rgb[i * 3 + 1], rgb[i * 3 + 2]]);
            grid.weight.push(a);
            grid.centre[0] += (x as f32 + 0.5) / w as f32 * a;
            grid.centre[1] += (y as f32 + 0.5) / h as f32 * a;
            total += a;
        }
    }
    if total <= 1e-6 {
        return;
    }
    grid.centre = grid.centre.map(|v| v / total);
    let output = evaluate(&grid, op);
    let correction: Vec<_> = output
        .iter()
        .zip(&grid.pixels)
        .zip(&grid.weight)
        .map(|((a, b), weight)| {
            if *weight > 0.0 {
                [0, 1, 2].map(|c| a[c] - b[c])
            } else {
                [0.0; 3]
            }
        })
        .collect();
    rgb.par_chunks_mut(3).enumerate().for_each(|(i, p)| {
        let a = weights[i] * op.amount;
        if a <= 0.0 {
            return;
        }
        let x = ((i % width) as f32 + 0.5) * w as f32 / width as f32 - 0.5;
        let y = ((i / width) as f32 + 0.5) * h as f32 / height as f32 - 0.5;
        let delta = transfer::sample(&correction, &grid.weight, w, h, x, y);
        for c in 0..3 {
            let v = p[c] + delta[c] * a;
            if v.is_finite() {
                p[c] = v.max(0.0);
            }
        }
    });
}
pub(crate) fn apply(
    rgb: &mut [f32],
    width: u32,
    height: u32,
    recipe: &Recipe,
    frame: Option<&Frame>,
) {
    if width == 0 || height == 0 || rgb.len() != width as usize * height as usize * 3 {
        return;
    }
    let Ok(ops) = spatial_lab::read(recipe) else {
        return;
    };
    let Ok(masks) = aura_recipe::local_masks::read(recipe) else {
        return;
    };
    for op in ops.iter().filter(|op| op.enabled && op.amount > 0.0) {
        let Some(mask) = masks
            .iter()
            .find(|m| m.id == op.mask_id && m.enabled && m.amount > 0.0)
        else {
            continue;
        };
        let Some(mut weights) =
            crate::local_masks::coverage(rgb, width, height, recipe, frame, &op.mask_id)
        else {
            continue;
        };
        for a in &mut weights {
            *a *= mask.amount;
        }
        process(rgb, width as usize, height as usize, &weights, op);
    }
}

#[cfg(test)]
mod advanced_tests;
#[cfg(test)]
mod analytical_tests;
#[cfg(test)]
mod next_tests;
#[cfg(test)]
mod precision_tests;
#[cfg(test)]
mod tests;
