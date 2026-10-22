//! Analytic shapes rasterized with four sub-cell samples, and a continuous seed field.
#![allow(clippy::indexing_slicing, clippy::cast_precision_loss)]
use super::{byte, check, invalid, Action, CancelToken, Options, Plane};
use aura_core::AuraResult;
use std::f32::consts::PI;

fn segment_distance(point: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let delta = [b[0] - a[0], b[1] - a[1]];
    let denominator = delta[0].powi(2) + delta[1].powi(2);
    let t = if denominator > 1e-12 {
        ((point[0] - a[0]) * delta[0] + (point[1] - a[1]) * delta[1]) / denominator
    } else {
        0.0
    }
    .clamp(0.0, 1.0);
    (point[0] - a[0] - t * delta[0]).hypot(point[1] - a[1] - t * delta[1])
}
fn bezier(vertices: &[[f32; 2]]) -> Vec<[f32; 2]> {
    (0..=128)
        .map(|i| {
            let t = i as f32 / 128.0;
            let u = 1.0 - t;
            let weights = [u.powi(3), 3.0 * u * u * t, 3.0 * u * t * t, t.powi(3)];
            let mut p = [0.0; 2];
            for (v, weight) in vertices.iter().zip(weights) {
                p[0] += v[0] * weight;
                p[1] += v[1] * weight;
            }
            p
        })
        .collect()
}
fn star(options: &Options) -> Vec<[f32; 2]> {
    (0..10)
        .map(|i| {
            let a = i as f32 * PI / 5.0 + options.angle.to_radians() - PI * 0.5;
            let r = if i % 2 == 0 {
                1.0
            } else {
                options.corner.clamp(0.05, 0.95)
            };
            [a.cos() * r, a.sin() * r]
        })
        .collect()
}
fn inside_star(point: [f32; 2], options: &Options, vertices: &[[f32; 2]]) -> bool {
    // Five tips and five inset vertices form an actual concave polygon.
    let [left, top, right, bottom] = options.bounds;
    let centre = [(left + right) * 0.5, (top + bottom) * 0.5];
    let point = [
        (point[0] - centre[0]) / ((right - left) * 0.5),
        (point[1] - centre[1]) / ((bottom - top) * 0.5),
    ];
    let mut inside = false;
    for i in 0..10 {
        let a = vertices[i];
        let b = vertices[(i + 1) % 10];
        if (a[1] > point[1]) != (b[1] > point[1])
            && point[0] < (b[0] - a[0]) * (point[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            inside = !inside;
        }
    }
    inside
}
fn coverage(point: [f32; 2], action: Action, options: &Options, curve: &[[f32; 2]]) -> f32 {
    match action {
        Action::SuperellipseMarquee => {
            let [left, top, right, bottom] = options.bounds;
            let x = ((point[0] - (left + right) * 0.5) / ((right - left) * 0.5)).abs();
            let y = ((point[1] - (top + bottom) * 0.5) / ((bottom - top) * 0.5)).abs();
            let power = 0.5 + options.corner * 7.5;
            if x.powf(power) + y.powf(power) <= 1.0 {
                1.0
            } else {
                0.0
            }
        }
        Action::BezierRibbon => {
            let distance = curve
                .windows(2)
                .map(|v| segment_distance(point, v[0], v[1]))
                .fold(f32::INFINITY, f32::min);
            if distance <= options.tolerance {
                1.0
            } else {
                0.0
            }
        }
        Action::StarMarquee => {
            if inside_star(point, options, curve) {
                1.0
            } else {
                0.0
            }
        }
        Action::SeedField => {
            let (mut value, mut total) = (0.0, 0.0);
            let nearest = options
                .vertices
                .iter()
                .map(|v| (point[0] - v[0]).hypot(point[1] - v[1]))
                .fold(f32::INFINITY, f32::min)
                .max(1e-6);
            for (index, vertex) in options.vertices.iter().enumerate() {
                let distance = (point[0] - vertex[0]).hypot(point[1] - vertex[1]);
                if distance < 1e-6 {
                    return if index % 2 == 0 { 1.0 } else { 0.0 };
                }
                // Scale every inverse-distance weight by the nearest distance.
                // The ratio stays <= 1 and prevents overflow near a control point.
                let weight = (nearest / distance).powf(1.0 + options.corner * 7.0);
                total += weight;
                if index % 2 == 0 {
                    value += weight;
                }
            }
            value / total.max(1e-12)
        }
        _ => 0.0,
    }
}
pub(super) fn apply(
    plane: &mut Plane,
    action: Action,
    options: &Options,
    cancel: &CancelToken,
) -> AuraResult<()> {
    if action == Action::BezierRibbon && options.vertices.len() != 4 {
        return Err(invalid(
            "A cubic Bezier ribbon requires exactly four control points.",
        ));
    }
    if action == Action::SeedField && options.vertices.len() < 2 {
        return Err(invalid(
            "A seed field requires at least two alternating foreground/background seeds.",
        ));
    }
    let curve = match action {
        Action::BezierRibbon => bezier(&options.vertices),
        Action::StarMarquee => star(options),
        _ => vec![],
    };
    let mut output = vec![0; plane.alpha.len()];
    for y in 0..plane.height {
        check(cancel)?;
        for x in 0..plane.width {
            let mut value = 0.0;
            for oy in [0.25, 0.75] {
                for ox in [0.25, 0.75] {
                    value += coverage(
                        [
                            (x as f32 + ox) / plane.width as f32,
                            (y as f32 + oy) / plane.height as f32,
                        ],
                        action,
                        options,
                        &curve,
                    );
                }
            }
            output[y * plane.width + x] = byte(value * 0.25);
        }
    }
    plane.alpha = output;
    Ok(())
}
