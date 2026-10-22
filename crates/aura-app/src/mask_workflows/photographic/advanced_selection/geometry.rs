//! Independent inverse-coordinate maps, landmark interpolation and path geometry.
#![allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
use super::{byte, check, invalid, sample, Action, CancelToken, Options, Plane};
use aura_core::AuraResult;
use std::f32::consts::TAU;
#[derive(Clone, Copy)]
struct Landmark {
    source: [f32; 2],
    target: [f32; 2],
}
fn subtract(first: [f32; 2], second: [f32; 2]) -> [f32; 2] {
    [first[0] - second[0], first[1] - second[1]]
}
fn cross(first: [f32; 2], second: [f32; 2]) -> f32 {
    first[0] * second[1] - first[1] * second[0]
}
fn length(vector: [f32; 2]) -> f32 {
    vector[0].hypot(vector[1])
}
fn strictly_convex(points: &[[f32; 2]]) -> bool {
    let orientation = cross(
        subtract(points[1], points[0]),
        subtract(points[2], points[0]),
    );
    for (index, first) in points.iter().enumerate() {
        let next = (index + 1) % points.len();
        let edge = subtract(points[next], *first);
        for (other, point) in points.iter().enumerate() {
            if other == index || other == next {
                continue;
            }
            let area = cross(edge, subtract(*point, *first));
            if area.abs() < 1e-6 || orientation * area <= 0.0 {
                return false;
            }
        }
    }
    true
}
fn landmarks(options: &Options, convex: bool) -> AuraResult<Vec<Landmark>> {
    if options.vertices.len() < 6
        || options.vertices.len() > 16
        || !options.vertices.len().is_multiple_of(2)
    {
        return Err(invalid(
            "Enter 3-8 interleaved source/destination landmark pairs.",
        ));
    }
    let pairs: Vec<_> = options
        .vertices
        .chunks_exact(2)
        .map(|pair| Landmark {
            source: pair[0],
            target: pair[1],
        })
        .collect();
    for (index, pair) in pairs.iter().enumerate() {
        if pairs[..index]
            .iter()
            .any(|other| length(subtract(pair.target, other.target)) < 1e-5)
        {
            return Err(invalid(
                "Destination landmarks must occupy distinct coordinates.",
            ));
        }
    }
    if convex {
        for target in [false, true] {
            let points: Vec<_> = pairs
                .iter()
                .map(|pair| if target { pair.target } else { pair.source })
                .collect();
            if !strictly_convex(&points) {
                return Err(invalid(
                    "Both cages must have strictly convex cyclic vertex order.",
                ));
            }
        }
    }
    Ok(pairs)
}
fn barycentric(point: [f32; 2], triangle: [[f32; 2]; 3]) -> Option<[f32; 3]> {
    let first = subtract(triangle[1], triangle[0]);
    let second = subtract(triangle[2], triangle[0]);
    let delta = subtract(point, triangle[0]);
    let determinant = cross(first, second);
    if determinant.abs() < 1e-8 {
        return None;
    }
    let second_weight = cross(delta, second) / determinant;
    let third_weight = cross(first, delta) / determinant;
    let weights = [
        1.0 - second_weight - third_weight,
        second_weight,
        third_weight,
    ];
    weights
        .iter()
        .all(|weight| *weight >= -1e-6)
        .then_some(weights)
}
fn fan_map(point: [f32; 2], pairs: &[Landmark]) -> Option<[f32; 2]> {
    for index in 1..pairs.len() - 1 {
        let triangle = [pairs[0], pairs[index], pairs[index + 1]];
        if let Some(weights) = barycentric(point, triangle.map(|pair| pair.target)) {
            return Some([0, 1].map(|axis| {
                triangle
                    .iter()
                    .zip(weights)
                    .map(|(pair, weight)| pair.source[axis] * weight)
                    .sum()
            }));
        }
    }
    None
}
fn mean_value_map(point: [f32; 2], pairs: &[Landmark]) -> Option<[f32; 2]> {
    fan_map(point, pairs)?;
    let vectors: Vec<_> = pairs
        .iter()
        .map(|pair| subtract(pair.target, point))
        .collect();
    let distances: Vec<_> = vectors.iter().copied().map(length).collect();
    for (index, distance) in distances.iter().enumerate() {
        if *distance < 1e-6 {
            return Some(pairs[index].source);
        }
    }
    let mut half_tangents = vec![0.0; pairs.len()];
    for index in 0..pairs.len() {
        let next = (index + 1) % pairs.len();
        let dot = vectors[index][0] * vectors[next][0] + vectors[index][1] * vectors[next][1];
        let area = cross(vectors[index], vectors[next]);
        if area.abs() < 1e-7 && dot < 0.0 {
            let fraction = distances[index] / (distances[index] + distances[next]);
            return Some([0, 1].map(|axis| {
                pairs[index].source[axis] * (1.0 - fraction) + pairs[next].source[axis] * fraction
            }));
        }
        half_tangents[index] = area / (distances[index] * distances[next] + dot);
    }
    let weights: Vec<_> = (0..pairs.len())
        .map(|index| {
            (half_tangents[(index + pairs.len() - 1) % pairs.len()] + half_tangents[index])
                / distances[index]
        })
        .collect();
    let mass = weights.iter().sum::<f32>();
    if !mass.is_finite() || mass.abs() < 1e-8 {
        return None;
    }
    Some([0, 1].map(|axis| {
        pairs
            .iter()
            .zip(&weights)
            .map(|(pair, weight)| pair.source[axis] * weight)
            .sum::<f32>()
            / mass
    }))
}
fn radial(first: [f32; 2], second: [f32; 2]) -> f32 {
    let delta = subtract(first, second);
    let squared = delta[0] * delta[0] + delta[1] * delta[1];
    if squared < 1e-12 {
        0.0
    } else {
        squared * squared.ln()
    }
}
fn solve(mut matrix: Vec<Vec<f32>>, count: usize) -> AuraResult<Vec<[f32; 2]>> {
    for column in 0..count {
        let pivot = (column..count)
            .max_by(|a, b| {
                matrix[*a][column]
                    .abs()
                    .total_cmp(&matrix[*b][column].abs())
            })
            .ok_or_else(|| invalid("Missing deformation pivot."))?;
        if matrix[pivot][column].abs() < 1e-8 {
            return Err(invalid(
                "Landmarks are collinear or the deformation system is singular.",
            ));
        }
        matrix.swap(column, pivot);
        let divisor = matrix[column][column];
        for entry in &mut matrix[column][column..count + 2] {
            *entry /= divisor;
        }
        let pivot_values = matrix[column][column..count + 2].to_vec();
        for (row_index, row) in matrix.iter_mut().enumerate() {
            if row_index == column {
                continue;
            }
            let factor = row[column];
            for (entry, pivot) in row[column..count + 2].iter_mut().zip(&pivot_values) {
                *entry -= factor * pivot;
            }
        }
    }
    Ok(matrix
        .iter()
        .map(|row| [row[count], row[count + 1]])
        .collect())
}
fn thin_plate(pairs: &[Landmark], regularization: f32) -> AuraResult<Vec<[f32; 2]>> {
    let count = pairs.len();
    let mut matrix = vec![vec![0.0; count + 5]; count + 3];
    for (row, pair) in pairs.iter().enumerate() {
        for (column, other) in pairs.iter().enumerate() {
            matrix[row][column] = radial(pair.target, other.target);
        }
        matrix[row][row] += regularization * 0.0001;
        matrix[row][count] = 1.0;
        matrix[row][count + 1] = pair.target[0];
        matrix[row][count + 2] = pair.target[1];
        matrix[count][row] = 1.0;
        matrix[count + 1][row] = pair.target[0];
        matrix[count + 2][row] = pair.target[1];
        matrix[row][count + 3] = pair.source[0];
        matrix[row][count + 4] = pair.source[1];
    }
    solve(matrix, count + 3)
}
fn landmark_warp(
    plane: &Plane,
    action: Action,
    options: &Options,
    cancel: &CancelToken,
) -> AuraResult<Vec<u8>> {
    let pairs = landmarks(options, action != Action::ThinPlateCoverageWarp)?;
    let coefficients = if action == Action::ThinPlateCoverageWarp {
        thin_plate(&pairs, options.corner)?
    } else {
        Vec::new()
    };
    let mut output = vec![0; plane.alpha.len()];
    for y in 0..plane.height {
        check(cancel)?;
        for x in 0..plane.width {
            let point = [
                (x as f32 + 0.5) / plane.width as f32,
                (y as f32 + 0.5) / plane.height as f32,
            ];
            let source = match action {
                Action::ThinPlateCoverageWarp => {
                    let count = pairs.len();
                    Some([0, 1].map(|axis| {
                        coefficients[count][axis]
                            + coefficients[count + 1][axis] * point[0]
                            + coefficients[count + 2][axis] * point[1]
                            + pairs
                                .iter()
                                .zip(&coefficients)
                                .map(|(pair, coefficient)| {
                                    radial(point, pair.target) * coefficient[axis]
                                })
                                .sum::<f32>()
                    }))
                }
                Action::MeanValueCageWarp => mean_value_map(point, &pairs),
                _ => fan_map(point, &pairs),
            }
            .unwrap_or(point);
            if source.iter().any(|value| !value.is_finite()) {
                return Err(invalid("Deformation produced nonfinite coordinates."));
            }
            output[y * plane.width + x] = byte(sample(plane, source[0], source[1]));
        }
    }
    Ok(output)
}
fn mobius(plane: &Plane, options: &Options, cancel: &CancelToken) -> AuraResult<Vec<u8>> {
    let [left, top, right, bottom] = options.bounds;
    let centre = [(left + right) * 0.5, (top + bottom) * 0.5];
    let extent = [(right - left) * 0.5, (bottom - top) * 0.5];
    let displacement = [
        options.corner * 0.8 * options.angle.to_radians().cos(),
        options.corner * 0.8 * options.angle.to_radians().sin(),
    ];
    let mut output = vec![0; plane.alpha.len()];
    for y in 0..plane.height {
        check(cancel)?;
        for x in 0..plane.width {
            let point = [
                (x as f32 + 0.5) / plane.width as f32,
                (y as f32 + 0.5) / plane.height as f32,
            ];
            let local = [
                (point[0] - centre[0]) / extent[0],
                (point[1] - centre[1]) / extent[1],
            ];
            let source = if length(local) >= 1.0 {
                point
            } else {
                let numerator = [local[0] + displacement[0], local[1] + displacement[1]];
                let denominator = [
                    1.0 + displacement[0] * local[0] + displacement[1] * local[1],
                    displacement[0] * local[1] - displacement[1] * local[0],
                ];
                let norm = denominator[0] * denominator[0] + denominator[1] * denominator[1];
                [
                    centre[0]
                        + extent[0]
                            * (numerator[0] * denominator[0] + numerator[1] * denominator[1])
                            / norm,
                    centre[1]
                        + extent[1]
                            * (numerator[1] * denominator[0] - numerator[0] * denominator[1])
                            / norm,
                ]
            };
            output[y * plane.width + x] = byte(sample(plane, source[0], source[1]));
        }
    }
    Ok(output)
}
fn unwrap_path(plane: &Plane, options: &Options, cancel: &CancelToken) -> AuraResult<Vec<u8>> {
    if options.vertices.len() < 2 {
        return Err(invalid("Path unwrap requires at least two vertices."));
    }
    let segments: Vec<_> = options
        .vertices
        .windows(2)
        .filter_map(|pair| {
            let delta = subtract(pair[1], pair[0]);
            let distance = length(delta);
            (distance > 1e-6).then_some((pair[0], delta, distance))
        })
        .collect();
    let total = segments.iter().map(|segment| segment.2).sum::<f32>();
    if total < 1e-6 {
        return Err(invalid("Path vertices must contain a nonzero segment."));
    }
    let mut output = vec![0; plane.alpha.len()];
    for x in 0..plane.width {
        check(cancel)?;
        let mut distance = (x as f32 + 0.5) / plane.width as f32 * total;
        let mut chosen = segments[segments.len() - 1];
        for segment in &segments {
            chosen = *segment;
            if distance <= segment.2 {
                break;
            }
            distance -= segment.2;
        }
        let (start, delta, segment_length) = chosen;
        let fraction = (distance / segment_length).clamp(0.0, 1.0);
        for y in 0..plane.height {
            let offset = ((y as f32 + 0.5) / plane.height as f32 - 0.5) * 2.0 * options.tolerance;
            let source = [
                start[0] + delta[0] * fraction - delta[1] / segment_length * offset,
                start[1] + delta[1] * fraction + delta[0] / segment_length * offset,
            ];
            output[y * plane.width + x] = byte(sample(plane, source[0], source[1]));
        }
    }
    Ok(output)
}
fn seam_carve(plane: &Plane, options: &Options, cancel: &CancelToken) -> AuraResult<Vec<u8>> {
    let mut width = plane.width;
    let mut coverage = plane.alpha.clone();
    for _ in 0..(options.radius as usize).min(width - 1) {
        check(cancel)?;
        let mut cost = vec![0_u32; width * plane.height];
        let mut previous = vec![0; width * plane.height];
        for y in 0..plane.height {
            check(cancel)?;
            for x in 0..width {
                let index = y * width + x;
                cost[index] = u32::from(coverage[index]);
                if y > 0 {
                    let best = (x.saturating_sub(1)..=(x + 1).min(width - 1))
                        .min_by_key(|column| (cost[(y - 1) * width + column], *column))
                        .unwrap_or(x);
                    cost[index] += cost[(y - 1) * width + best];
                    previous[index] = best;
                }
            }
        }
        let mut column = (0..width)
            .min_by_key(|x| (cost[(plane.height - 1) * width + x], *x))
            .unwrap_or(0);
        let mut seam = vec![0; plane.height];
        for y in (0..plane.height).rev() {
            seam[y] = column;
            column = previous[y * width + column];
        }
        let mut next = Vec::with_capacity((width - 1) * plane.height);
        for y in 0..plane.height {
            for x in 0..width {
                if x != seam[y] {
                    next.push(coverage[y * width + x]);
                }
            }
        }
        coverage = next;
        width -= 1;
    }
    let mut output = vec![0; plane.alpha.len()];
    for y in 0..plane.height {
        check(cancel)?;
        for x in 0..plane.width {
            let source = ((x as f32 + 0.5) * width as f32 / plane.width as f32 - 0.5)
                .clamp(0.0, (width - 1) as f32);
            let left = source.floor() as usize;
            let fraction = source - left as f32;
            output[y * plane.width + x] = byte(
                (f32::from(coverage[y * width + left]) * (1.0 - fraction)
                    + f32::from(coverage[y * width + (left + 1).min(width - 1)]) * fraction)
                    / 255.0,
            );
        }
    }
    Ok(output)
}
fn ribbon(plane: &Plane, options: &Options, cancel: &CancelToken) -> AuraResult<Vec<u8>> {
    let [left, top, right, bottom] = options.bounds;
    let centre = [(left + right) * 0.5, (top + bottom) * 0.5];
    let extent = [(right - left) * 0.5, (bottom - top) * 0.5];
    let frequency = options.radius as f32;
    let steps = (options.radius as usize * 64).max(128);
    let point = |step: usize| {
        let phase = step as f32 / steps as f32 * TAU;
        [
            centre[0] + extent[0] * (frequency * phase + options.angle.to_radians()).sin(),
            centre[1] + extent[1] * ((frequency + 1.0) * phase).sin(),
        ]
    };
    let mut output = vec![0; plane.alpha.len()];
    for step in 0..steps {
        check(cancel)?;
        let first = point(step);
        let second = point(step + 1);
        let delta = subtract(second, first);
        let denominator = (delta[0] * delta[0] + delta[1] * delta[1]).max(1e-12);
        let left = ((first[0].min(second[0]) - options.tolerance) * plane.width as f32)
            .floor()
            .max(0.0) as usize;
        let right = (((first[0].max(second[0]) + options.tolerance) * plane.width as f32).ceil()
            as usize)
            .min(plane.width);
        let top = ((first[1].min(second[1]) - options.tolerance) * plane.height as f32)
            .floor()
            .max(0.0) as usize;
        let bottom = (((first[1].max(second[1]) + options.tolerance) * plane.height as f32).ceil()
            as usize)
            .min(plane.height);
        for y in top..bottom {
            for x in left..right {
                let local = [
                    (x as f32 + 0.5) / plane.width as f32 - first[0],
                    (y as f32 + 0.5) / plane.height as f32 - first[1],
                ];
                let fraction =
                    ((local[0] * delta[0] + local[1] * delta[1]) / denominator).clamp(0.0, 1.0);
                let distance =
                    (local[0] - delta[0] * fraction).hypot(local[1] - delta[1] * fraction);
                let index = y * plane.width + x;
                output[index] = output[index].max(byte(1.0 - distance / options.tolerance));
            }
        }
    }
    Ok(output)
}
pub(super) fn apply(
    plane: &Plane,
    action: Action,
    options: &Options,
    cancel: &CancelToken,
) -> AuraResult<Vec<u8>> {
    match action {
        Action::ThinPlateCoverageWarp
        | Action::MeanValueCageWarp
        | Action::TriangulatedCageWarp => landmark_warp(plane, action, options, cancel),
        Action::MobiusCoverageWarp => mobius(plane, options, cancel),
        Action::PathNormalUnwrap => unwrap_path(plane, options, cancel),
        Action::CoverageSeamCarve => seam_carve(plane, options, cancel),
        Action::LissajousRibbon => ribbon(plane, options, cancel),
        _ => Err(invalid("Unknown advanced geometric operator.")),
    }
}
