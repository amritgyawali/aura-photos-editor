//! Coverage construction and inverse-coordinate maps with transparent frame boundaries.
#![allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
use super::{byte, check, invalid, neighbors, Action, CancelToken, Frame, Options, Plane};
use aura_core::AuraResult;
use std::{collections::VecDeque, f32::consts::TAU};
fn sample(p: &Plane, x: f32, y: f32) -> f32 {
    if !x.is_finite() || !y.is_finite() {
        return 0.0;
    }
    let (x, y) = (x * p.width as f32 - 0.5, y * p.height as f32 - 0.5);
    let (left, top) = (x.floor() as isize, y.floor() as isize);
    let (fx, fy) = (x - x.floor(), y - y.floor());
    let mut value = 0.0;
    for (ox, wx) in [(0, 1.0 - fx), (1, fx)] {
        for (oy, wy) in [(0, 1.0 - fy), (1, fy)] {
            let (sx, sy) = (left + ox, top + oy);
            if let (Ok(sx), Ok(sy)) = (usize::try_from(sx), usize::try_from(sy)) {
                if sx < p.width && sy < p.height {
                    value += f32::from(p.alpha[sy * p.width + sx]) / 255.0 * wx * wy;
                }
            }
        }
    }
    value
}
#[allow(clippy::too_many_lines)] // The inverse-map dispatch keeps each geometric formula beside the shared checked output traversal.
pub(super) fn apply(
    plane: &Plane,
    action: Action,
    options: &Options,
    frame: Option<&Frame>,
    cancel: &CancelToken,
) -> AuraResult<Vec<u8>> {
    if action == Action::SignedDistanceSmooth {
        return signed_distance(plane, options, cancel);
    }
    let mut out = vec![0; plane.alpha.len()];
    let [left_bound, top_bound, right_bound, bottom_bound] = options.bounds;
    let (cx, cy) = (
        (left_bound + right_bound) * 0.5,
        (top_bound + bottom_bound) * 0.5,
    );
    let (rx, ry) = (
        (right_bound - left_bound) * 0.5,
        (bottom_bound - top_bound) * 0.5,
    );
    let triangle = if action == Action::BarycentricTriangle {
        if options.vertices.len() < 3 {
            return Err(invalid(
                "Triangle coverage requires at least three vertices; the first three are used.",
            ));
        }
        let (a, b, c) = (
            options.vertices[0],
            options.vertices[1],
            options.vertices[2],
        );
        let denominator = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
        if denominator.abs() < 1e-7 {
            return Err(invalid("Triangle vertices must not be collinear."));
        }
        Some((a, b, c, denominator))
    } else {
        None
    };
    for y in 0..plane.height {
        check(cancel)?;
        for x in 0..plane.width {
            let (nx, ny) = (
                (x as f32 + 0.5) / plane.width as f32,
                (y as f32 + 0.5) / plane.height as f32,
            );
            let value = match action {
                Action::SpiralMarquee => {
                    // Find the nearest Archimedean turn in normalized elliptical coordinates.
                    let (dx, dy) = ((nx - cx) / rx, (ny - cy) / ry);
                    let rho = dx.hypot(dy);
                    let phase = (dy.atan2(dx) - options.angle.to_radians()).rem_euclid(TAU) / TAU;
                    let turns = options.radius as f32;
                    let k = (rho * turns - phase).round().clamp(0.0, turns - 1.0);
                    let distance = (rho - (phase + k) / turns).abs();
                    if rho > 1.0 {
                        0.0
                    } else {
                        (1.0 - distance / options.tolerance).clamp(0.0, 1.0)
                    }
                }
                Action::BarycentricTriangle => {
                    let (a, b, c, den) = triangle.ok_or_else(|| invalid("Missing triangle."))?;
                    let wa = ((b[1] - c[1]) * (nx - c[0]) + (c[0] - b[0]) * (ny - c[1])) / den;
                    let wb = ((c[1] - a[1]) * (nx - c[0]) + (a[0] - c[0]) * (ny - c[1])) / den;
                    let wc = 1.0 - wa - wb;
                    if wa >= 0.0 && wb >= 0.0 && wc >= 0.0 {
                        wa.powf(0.25 + options.corner * 3.75)
                    } else {
                        0.0
                    }
                }
                Action::CoverageShear => {
                    let shear = options.angle.to_radians().sin() * 1.5;
                    sample(plane, nx - shear * (ny - cy), ny)
                }
                Action::CoverageTwirl | Action::CoverageSpherize => {
                    let (dx, dy) = ((nx - cx) / rx, (ny - cy) / ry);
                    let rho = dx.hypot(dy);
                    if rho >= 1.0 {
                        sample(plane, nx, ny)
                    } else if action == Action::CoverageTwirl {
                        let a = dy.atan2(dx) - options.angle.to_radians() * (1.0 - rho).powi(2);
                        sample(plane, cx + rx * rho * a.cos(), cy + ry * rho * a.sin())
                    } else if rho < 1e-7 {
                        sample(plane, cx, cy)
                    } else {
                        // Inverse stereographic-style radial projection; corner blends its strength.
                        let mapped = (1.0 - options.corner) * rho
                            + options.corner * (2.0 * (rho * 0.5).atan() / (2.0 * 0.5_f32.atan()));
                        sample(
                            plane,
                            cx + rx * dx * mapped / rho,
                            cy + ry * dy * mapped / rho,
                        )
                    }
                }
                Action::CoverageDisplace => {
                    let f = frame.ok_or_else(|| {
                        invalid("Coverage displacement requires source luminance.")
                    })?;
                    let left = y * plane.width + x.saturating_sub(1);
                    let right = y * plane.width + (x + 1).min(plane.width - 1);
                    let top = y.saturating_sub(1) * plane.width + x;
                    let bottom = (y + 1).min(plane.height - 1) * plane.width + x;
                    let dx = (f.luminance[right] - f.luminance[left]) * options.tolerance;
                    let dy = (f.luminance[bottom] - f.luminance[top]) * options.tolerance;
                    sample(plane, nx - dx, ny - dy)
                }
                _ => return Err(invalid("Unsupported coverage mapping.")),
            };
            out[y * plane.width + x] = byte(value);
        }
    }
    Ok(out)
}
fn signed_distance(p: &Plane, o: &Options, cancel: &CancelToken) -> AuraResult<Vec<u8>> {
    let inside: Vec<_> = p.alpha.iter().map(|v| *v >= 128).collect();
    // Empty and fully selected frames have no internal interface; retain their exact coverage.
    if inside.iter().all(|v| !*v) || inside.iter().all(|v| *v) {
        return Ok(p.alpha.clone());
    }
    let mut distance = vec![usize::MAX; p.alpha.len()];
    let mut queue = VecDeque::new();
    for i in 0..inside.len() {
        if neighbors(i, p.width, p.height).any(|j| inside[j] != inside[i]) {
            distance[i] = 0;
            queue.push_back(i);
        }
    }
    while let Some(i) = queue.pop_front() {
        check(cancel)?;
        for j in neighbors(i, p.width, p.height) {
            if distance[j] == usize::MAX {
                distance[j] = distance[i] + 1;
                queue.push_back(j);
            }
        }
    }
    Ok(distance
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let signed = (*d as f32 + 0.5) * if inside[i] { 1.0 } else { -1.0 };
            byte(1.0 / (1.0 + (-signed / (o.radius as f32 * 0.5)).exp()))
        })
        .collect())
}
