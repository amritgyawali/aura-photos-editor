//! Local source descriptors and explicit edge connectivity; no synthesized source pixels.
#![allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
use super::{band, byte, check, invalid, seed, Action, CancelToken, Frame, Options, Plane};
use aura_core::AuraResult;
use std::collections::VecDeque;
fn at(plane: &Plane, values: &[f32], x: usize, y: usize, dx: isize, dy: isize) -> f32 {
    values[y.saturating_add_signed(dy).min(plane.height - 1) * plane.width
        + x.saturating_add_signed(dx).min(plane.width - 1)]
}
fn patch(plane: &Plane, x: usize, y: usize, radius: usize) -> impl Iterator<Item = usize> {
    let width = plane.width;
    (y.saturating_sub(radius)..(y + radius + 1).min(plane.height)).flat_map(move |sy| {
        (x.saturating_sub(radius)..(x + radius + 1).min(width)).map(move |sx| sy * width + sx)
    })
}
fn gradients(plane: &Plane, frame: &Frame) -> Vec<[f32; 3]> {
    (0..plane.alpha.len())
        .map(|index| {
            let (x, y) = (index % plane.width, index / plane.width);
            let dx = (at(plane, &frame.luminance, x, y, 1, 0)
                - at(plane, &frame.luminance, x, y, -1, 0))
                * 0.5;
            let dy = (at(plane, &frame.luminance, x, y, 0, 1)
                - at(plane, &frame.luminance, x, y, 0, -1))
                * 0.5;
            [dx, dy, dx.hypot(dy).min(1.0)]
        })
        .collect()
}
fn correlation(first: &[f32], second: &[f32]) -> f32 {
    let count = first.len() as f32;
    let avg_first = first.iter().sum::<f32>() / count;
    let avg_second = second.iter().sum::<f32>() / count;
    let (mut cross, mut first_var, mut second_var) = (0.0, 0.0, 0.0);
    for (a, b) in first.iter().zip(second) {
        let da = a - avg_first;
        let db = b - avg_second;
        cross += da * db;
        first_var += da * da;
        second_var += db * db;
    }
    if first_var * second_var < 1e-12 {
        0.5
    } else {
        (0.5 + 0.5 * cross / (first_var * second_var).sqrt()).clamp(0.0, 1.0)
    }
}
fn descriptor(
    plane: &Plane,
    frame: &Frame,
    index: usize,
    action: Action,
    options: &Options,
) -> f32 {
    let (x, y) = (index % plane.width, index / plane.width);
    let centre = frame.luminance[index];
    let ids: Vec<_> = patch(plane, x, y, options.radius as usize).collect();
    let values: Vec<_> = ids.iter().map(|i| frame.luminance[*i]).collect();
    let count = values.len() as f32;
    let avg = values.iter().sum::<f32>() / count;
    let (dx, dy) = (
        options.angle.to_radians().cos().round() as isize,
        options.angle.to_radians().sin().round() as isize,
    );
    match action {
        Action::MedianAbsoluteDeviation => {
            let mut sorted = values;
            sorted.sort_by(f32::total_cmp);
            let median = sorted[sorted.len() / 2];
            let mut residuals: Vec<_> = sorted.iter().map(|v| (v - median).abs()).collect();
            residuals.sort_by(f32::total_cmp);
            ((centre - median).abs() / (3.0 * residuals[residuals.len() / 2] + 0.001)).min(1.0)
        }
        Action::PatchAutocorrelation | Action::PatchSemivariance => {
            let shifted: Vec<_> = ids
                .iter()
                .map(|i| {
                    at(
                        plane,
                        &frame.luminance,
                        i % plane.width,
                        i / plane.width,
                        dx,
                        dy,
                    )
                })
                .collect();
            if action == Action::PatchAutocorrelation {
                correlation(&values, &shifted)
            } else {
                values
                    .iter()
                    .zip(shifted)
                    .map(|(a, b)| (a - b).powi(2))
                    .sum::<f32>()
                    / count
            }
        }
        Action::LocalMoran => {
            let variance = values.iter().map(|v| (v - avg).powi(2)).sum::<f32>() / count;
            let adjacent = [(-1, 0), (1, 0), (0, -1), (0, 1)]
                .map(|(dx, dy)| at(plane, &frame.luminance, x, y, dx, dy));
            if variance < 1e-10 {
                0.5
            } else {
                (0.5 + (centre - avg) * (adjacent.iter().sum::<f32>() * 0.25 - avg) / variance
                    * 0.125)
                    .clamp(0.0, 1.0)
            }
        }
        Action::LocalHueDispersion => {
            let (mut horizontal, mut vertical, mut mass) = (0.0, 0.0, 0.0);
            for i in ids {
                let rgb = frame.rgb[i];
                let first = rgb[0] - rgb[1];
                let second = (rgb[0] + rgb[1] - 2.0 * rgb[2]) / 3.0_f32.sqrt();
                horizontal += first;
                vertical += second;
                mass += first.hypot(second);
            }
            if mass < 1e-6 {
                0.0
            } else {
                (1.0 - horizontal.hypot(vertical) / mass).clamp(0.0, 1.0)
            }
        }
        _ => 0.0,
    }
}
fn pattern(plane: &Plane, frame: &Frame, index: usize) -> f32 {
    let (x, y) = (index % plane.width, index / plane.width);
    let ring = [
        (-1, -1),
        (0, -1),
        (1, -1),
        (1, 0),
        (1, 1),
        (0, 1),
        (-1, 1),
        (-1, 0),
    ];
    let bits =
        ring.map(|(dx, dy)| at(plane, &frame.luminance, x, y, dx, dy) >= frame.luminance[index]);
    (0..8).filter(|i| bits[*i] != bits[(i + 1) % 8]).count() as f32 / 8.0
}
fn template(plane: &Plane, frame: &Frame, index: usize, target: usize, radius: usize) -> f32 {
    let (x, y) = (index % plane.width, index / plane.width);
    let (tx, ty) = (target % plane.width, target / plane.width);
    let radius = isize::try_from(radius).unwrap_or(4);
    let mut first = Vec::new();
    let mut second = Vec::new();
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            first.push(at(plane, &frame.luminance, x, y, dx, dy));
            second.push(at(plane, &frame.luminance, tx, ty, dx, dy));
        }
    }
    correlation(&first, &second)
}
fn thin_edges(plane: &Plane, gradient: &[[f32; 3]], index: usize) -> f32 {
    let [dx, dy, magnitude] = gradient[index];
    if magnitude < 1e-8 {
        return 0.0;
    }
    let (ox, oy) = if dx.abs() > dy.abs() * 2.0 {
        (1, 0)
    } else if dy.abs() > dx.abs() * 2.0 {
        (0, 1)
    } else {
        (1, if dx * dy >= 0.0 { 1 } else { -1 })
    };
    let (x, y) = (index % plane.width, index / plane.width);
    let get = |ox: isize, oy: isize| {
        gradient[y.saturating_add_signed(oy).min(plane.height - 1) * plane.width
            + x.saturating_add_signed(ox).min(plane.width - 1)][2]
    };
    if magnitude >= get(ox, oy) && magnitude >= get(-ox, -oy) {
        magnitude
    } else {
        0.0
    }
}
fn hysteresis(
    plane: &Plane,
    gradient: &[[f32; 3]],
    options: &Options,
    cancel: &CancelToken,
) -> AuraResult<Vec<u8>> {
    let mut output = vec![0; plane.alpha.len()];
    let mut queue = VecDeque::new();
    for (i, value) in gradient.iter().enumerate() {
        if value[2] > 0.0 && value[2] >= options.range[1] {
            output[i] = 255;
            queue.push_back(i);
        }
    }
    while let Some(index) = queue.pop_front() {
        check(cancel)?;
        let (x, y) = (index % plane.width, index / plane.width);
        for dy in [-1_isize, 0, 1] {
            for dx in [-1_isize, 0, 1] {
                let (Some(sx), Some(sy)) = (x.checked_add_signed(dx), y.checked_add_signed(dy))
                else {
                    continue;
                };
                if sx >= plane.width || sy >= plane.height {
                    continue;
                }
                let next = sy * plane.width + sx;
                if output[next] == 0
                    && gradient[next][2] > 0.0
                    && gradient[next][2] >= options.range[0]
                {
                    output[next] = 255;
                    queue.push_back(next);
                }
            }
        }
    }
    Ok(output)
}
fn prototypes(
    plane: &Plane,
    frame: &Frame,
    options: &Options,
    cancel: &CancelToken,
) -> AuraResult<Vec<u8>> {
    if options.vertices.len() < 2 || options.seed_index >= options.vertices.len() {
        return Err(invalid(
            "Choose at least two colour prototypes and a valid prototype index.",
        ));
    }
    let colours: Vec<_> = options
        .vertices
        .iter()
        .map(|point| frame.rgb[seed(*point, plane)])
        .collect();
    let mut output = vec![0; plane.alpha.len()];
    for (index, pixel) in frame.rgb.iter().enumerate() {
        if index % plane.width == 0 {
            check(cancel)?;
        }
        let closest = colours
            .iter()
            .enumerate()
            .map(|(index, colour)| {
                (
                    index,
                    pixel
                        .iter()
                        .zip(colour)
                        .map(|(a, b)| (a - b).powi(2))
                        .sum::<f32>(),
                )
            })
            .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
        if closest.is_some_and(|(which, _)| which == options.seed_index) {
            output[index] = 255;
        }
    }
    Ok(output)
}
pub(super) fn apply(
    plane: &Plane,
    action: Action,
    options: &Options,
    point: [f32; 2],
    frame: &Frame,
    cancel: &CancelToken,
) -> AuraResult<Vec<u8>> {
    if action == Action::ColourPrototypePartition {
        return prototypes(plane, frame, options, cancel);
    }
    let gradient = gradients(plane, frame);
    if action == Action::EdgeHysteresis {
        return hysteresis(plane, &gradient, options, cancel);
    }
    let target = seed(point, plane);
    let sample_rgb = frame.rgb[target];
    let log = |rgb: [f32; 3]| {
        [
            (rgb[0] + 1.0 / 255.0).ln() - (rgb[1] + 1.0 / 255.0).ln(),
            (rgb[2] + 1.0 / 255.0).ln() - (rgb[1] + 1.0 / 255.0).ln(),
        ]
    };
    let target_log = log(sample_rgb);
    let mut output = vec![0; plane.alpha.len()];
    for (index, value) in output.iter_mut().enumerate() {
        if index % plane.width == 0 {
            check(cancel)?;
        }
        if action == Action::LogChromaDistance {
            let chroma = log(frame.rgb[index]);
            *value = byte(
                1.0 - (chroma[0] - target_log[0]).hypot(chroma[1] - target_log[1])
                    / (options.tolerance * 8.0),
            );
            continue;
        }
        let metric = match action {
            Action::BinaryPatternTransitions => pattern(plane, frame, index),
            Action::EdgeNonmaximum => thin_edges(plane, &gradient, index),
            Action::TemplatePatchCorrelation => {
                template(plane, frame, index, target, options.radius as usize)
            }
            _ => descriptor(plane, frame, index, action, options),
        };
        *value = if action == Action::EdgeNonmaximum && metric <= 0.0 {
            0
        } else {
            byte(band(metric, options))
        };
    }
    Ok(output)
}
