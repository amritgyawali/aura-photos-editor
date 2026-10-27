//! Explicit selected graphs, stable coverage evolution and exact Euclidean geometry.
#![allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
use super::{band, byte, check, invalid, neighbors, seed, Action, CancelToken, Options, Plane};
use aura_core::AuraResult;
use std::{
    cmp::Reverse,
    collections::{BinaryHeap, VecDeque},
};
fn articulations(plane: &Plane, cancel: &CancelToken) -> AuraResult<Vec<u8>> {
    let count = plane.alpha.len();
    let mut depth = vec![usize::MAX; count];
    let mut low = depth.clone();
    let mut parent = depth.clone();
    let mut children = vec![0; count];
    let mut output = vec![0; count];
    let mut clock = 0;
    for root in 0..count {
        if plane.alpha[root] < 128 || depth[root] != usize::MAX {
            continue;
        }
        depth[root] = clock;
        low[root] = clock;
        clock += 1;
        let mut stack = vec![(root, 0)];
        while let Some((index, offset)) = stack.last().copied() {
            check(cancel)?;
            if let Some(next) = neighbors(index, plane.width, plane.height).nth(offset) {
                if let Some(last) = stack.last_mut() {
                    last.1 += 1;
                }
                if plane.alpha[next] < 128 {
                    continue;
                }
                if depth[next] == usize::MAX {
                    parent[next] = index;
                    children[index] += 1;
                    depth[next] = clock;
                    low[next] = clock;
                    clock += 1;
                    stack.push((next, 0));
                } else if next != parent[index] {
                    low[index] = low[index].min(depth[next]);
                }
            } else {
                stack.pop();
                let previous = parent[index];
                if previous != usize::MAX {
                    low[previous] = low[previous].min(low[index]);
                    if parent[previous] != usize::MAX && low[index] >= depth[previous] {
                        output[previous] = plane.alpha[previous];
                    }
                } else if children[index] > 1 {
                    output[index] = plane.alpha[index];
                }
            }
        }
    }
    Ok(output)
}
fn core_neighbors(index: usize, width: usize, height: usize) -> impl Iterator<Item = usize> {
    let (x, y) = (index % width, index / width);
    [
        (-1_isize, -1_isize),
        (0, -1),
        (1, -1),
        (-1, 0),
        (1, 0),
        (-1, 1),
        (0, 1),
        (1, 1),
    ]
    .into_iter()
    .filter_map(move |(dx, dy)| {
        let (Some(sx), Some(sy)) = (x.checked_add_signed(dx), y.checked_add_signed(dy)) else {
            return None;
        };
        (sx < width && sy < height).then_some(sy * width + sx)
    })
}
fn graph_core(plane: &Plane, options: &Options, cancel: &CancelToken) -> AuraResult<Vec<u8>> {
    let minimum = 1 + (options.corner * 3.0).round() as usize;
    let mut active: Vec<_> = plane.alpha.iter().map(|v| *v >= 128).collect();
    let mut degrees: Vec<_> = (0..active.len())
        .map(|index| {
            core_neighbors(index, plane.width, plane.height)
                .filter(|next| active[*next])
                .count()
        })
        .collect();
    let mut queue: VecDeque<_> = (0..active.len())
        .filter(|index| active[*index] && degrees[*index] < minimum)
        .collect();
    while let Some(index) = queue.pop_front() {
        check(cancel)?;
        if !active[index] {
            continue;
        }
        active[index] = false;
        for next in core_neighbors(index, plane.width, plane.height) {
            if active[next] {
                degrees[next] = degrees[next].saturating_sub(1);
                if degrees[next] < minimum {
                    queue.push_back(next);
                }
            }
        }
    }
    Ok(active
        .iter()
        .zip(&plane.alpha)
        .map(|(keep, value)| if *keep { *value } else { 0 })
        .collect())
}
fn watershed(plane: &Plane, options: &Options, cancel: &CancelToken) -> AuraResult<Vec<u8>> {
    if options.vertices.len() < 2 || options.seed_index >= options.vertices.len() {
        return Err(invalid(
            "Watershed requires at least two selected seeds and a valid basin index.",
        ));
    }
    let mut best = vec![(u8::MAX, usize::MAX, usize::MAX); plane.alpha.len()];
    let mut queue = BinaryHeap::new();
    let mut seeds = Vec::new();
    for (label, point) in options.vertices.iter().enumerate() {
        let index = seed(*point, plane);
        if plane.alpha[index] < 128 || seeds.contains(&index) {
            return Err(invalid(
                "Watershed seeds must be distinct cells inside selected coverage.",
            ));
        }
        seeds.push(index);
        let cost = 255 - plane.alpha[index];
        best[index] = (cost, 0, label);
        queue.push(Reverse((cost, 0, label, index)));
    }
    while let Some(Reverse((cost, distance, label, index))) = queue.pop() {
        check(cancel)?;
        if best[index] != (cost, distance, label) {
            continue;
        }
        for next in neighbors(index, plane.width, plane.height) {
            if plane.alpha[next] < 128 {
                continue;
            }
            let candidate = (cost.max(255 - plane.alpha[next]), distance + 1, label);
            if candidate < best[next] {
                best[next] = candidate;
                queue.push(Reverse((candidate.0, candidate.1, label, next)));
            }
        }
    }
    Ok(best
        .iter()
        .zip(&plane.alpha)
        .map(|((_, _, label), value)| {
            if *label == options.seed_index {
                *value
            } else {
                0
            }
        })
        .collect())
}
fn evolve(
    plane: &Plane,
    action: Action,
    options: &Options,
    cancel: &CancelToken,
) -> AuraResult<Vec<u8>> {
    let mut values: Vec<_> = plane.alpha.iter().map(|v| f32::from(*v) / 255.0).collect();
    for _ in 0..options.radius * 4 {
        let mut next = values.clone();
        for y in 0..plane.height {
            check(cancel)?;
            for x in 0..plane.width {
                let index = y * plane.width + x;
                let centre = values[index];
                if action == Action::CoverageAnisotropicDiffusion {
                    let change = neighbors(index, plane.width, plane.height)
                        .map(|neighbor| {
                            let delta = values[neighbor] - centre;
                            delta * (-(delta / options.tolerance).powi(2)).exp()
                        })
                        .sum::<f32>();
                    next[index] = (centre + 0.2 * change).clamp(0.0, 1.0);
                } else if x > 0 && y > 0 && x + 1 < plane.width && y + 1 < plane.height {
                    let dx = (values[index + 1] - values[index - 1]) * 0.5;
                    let dy = (values[index + plane.width] - values[index - plane.width]) * 0.5;
                    let dxx = values[index + 1] + values[index - 1] - 2.0 * centre;
                    let dyy =
                        values[index + plane.width] + values[index - plane.width] - 2.0 * centre;
                    let dxy = (values[index + plane.width + 1]
                        - values[index + plane.width - 1]
                        - values[index - plane.width + 1]
                        + values[index - plane.width - 1])
                        * 0.25;
                    let change = (dyy * dx * dx - 2.0 * dx * dy * dxy + dxx * dy * dy)
                        / (dx * dx + dy * dy + 1e-8);
                    next[index] = (centre + 0.1 * change).clamp(0.0, 1.0);
                }
            }
        }
        values = next;
    }
    Ok(values.into_iter().map(byte).collect())
}
fn distance_line(values: &[f32]) -> Vec<f32> {
    let count = values.len();
    let mut vertices = vec![0; count];
    let mut breaks = vec![f32::INFINITY; count + 1];
    breaks[0] = f32::NEG_INFINITY;
    let mut last = 0;
    for point in 1..count {
        let intersection = |previous: usize| {
            (values[point] + (point as f32).powi(2) - values[previous] - (previous as f32).powi(2))
                / (2.0 * (point - previous) as f32)
        };
        let mut crossing = intersection(vertices[last]);
        while crossing <= breaks[last] {
            last -= 1;
            crossing = intersection(vertices[last]);
        }
        last += 1;
        vertices[last] = point;
        breaks[last] = crossing;
        breaks[last + 1] = f32::INFINITY;
    }
    let mut output = vec![0.0; count];
    last = 0;
    for (point, value) in output.iter_mut().enumerate() {
        while breaks[last + 1] < (point as f32) {
            last += 1;
        }
        *value = (point as f32 - vertices[last] as f32).powi(2) + values[vertices[last]];
    }
    output
}
fn inscribed(plane: &Plane, cancel: &CancelToken) -> AuraResult<Vec<u8>> {
    let mut output = vec![0; plane.alpha.len()];
    for group in plane.region_groups(cancel)? {
        let (mut left, mut top, mut right, mut bottom) = (plane.width, plane.height, 0, 0);
        for &index in &group {
            left = left.min(index % plane.width);
            right = right.max(index % plane.width);
            top = top.min(index / plane.width);
            bottom = bottom.max(index / plane.width);
        }
        let (width, height) = (right - left + 3, bottom - top + 3);
        let mut distance = vec![0.0; width * height];
        for &index in &group {
            distance[(index / plane.width - top + 1) * width + index % plane.width - left + 1] =
                1e9;
        }
        for y in 0..height {
            check(cancel)?;
            let row = distance_line(&distance[y * width..(y + 1) * width]);
            distance[y * width..(y + 1) * width].copy_from_slice(&row);
        }
        for x in 0..width {
            check(cancel)?;
            let column: Vec<_> = (0..height).map(|y| distance[y * width + x]).collect();
            let column = distance_line(&column);
            for (y, value) in column.iter().enumerate() {
                distance[y * width + x] = *value;
            }
        }
        let mut centre = group[0];
        let mut maximum = 0.0;
        for &index in &group {
            let value =
                distance[(index / plane.width - top + 1) * width + index % plane.width - left + 1];
            if value > maximum {
                maximum = value;
                centre = index;
            }
        }
        let radius = (maximum.sqrt() - 0.5).max(0.0);
        for index in group {
            let dx = (index % plane.width) as f32 - (centre % plane.width) as f32;
            let dy = (index / plane.width) as f32 - (centre / plane.width) as f32;
            if dx * dx + dy * dy <= radius * radius {
                output[index] = plane.alpha[index];
            }
        }
    }
    Ok(output)
}
fn runs(plane: &Plane, options: &Options, cancel: &CancelToken) -> AuraResult<Vec<u8>> {
    let vertical = options.angle.to_radians().sin().abs() > options.angle.to_radians().cos().abs();
    let (lines, length) = if vertical {
        (plane.width, plane.height)
    } else {
        (plane.height, plane.width)
    };
    let mut output = vec![0; plane.alpha.len()];
    for line in 0..lines {
        check(cancel)?;
        let index = |offset| {
            if vertical {
                offset * plane.width + line
            } else {
                line * plane.width + offset
            }
        };
        let mut offset = 0;
        while offset < length {
            if plane.alpha[index(offset)] < 128 {
                offset += 1;
                continue;
            }
            let start = offset;
            while offset < length && plane.alpha[index(offset)] >= 128 {
                offset += 1;
            }
            let coverage = band((offset - start) as f32 / options.radius as f32, options);
            for position in start..offset {
                let pixel = index(position);
                output[pixel] = byte(f32::from(plane.alpha[pixel]) / 255.0 * coverage);
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
        Action::SelectionArticulations => articulations(plane, cancel),
        Action::SelectionKCore => graph_core(plane, options, cancel),
        Action::CoverageWatershed => watershed(plane, options, cancel),
        Action::CoverageAnisotropicDiffusion | Action::CoverageCurvatureFlow => {
            evolve(plane, action, options, cancel)
        }
        Action::RegionInscribedDisk => inscribed(plane, cancel),
        Action::DirectionalRunWidth => runs(plane, options, cancel),
        _ => Err(invalid("Unknown selection graph operator.")),
    }
}
