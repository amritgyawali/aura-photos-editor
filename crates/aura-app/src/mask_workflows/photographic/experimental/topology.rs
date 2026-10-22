//! Grayscale morphology, graph reconstruction, connected attributes and plateau analysis.
#![allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
use super::{band, byte, check, invalid, neighbors, Action, CancelToken, Options, Plane};
use aura_core::AuraResult;
use std::collections::{BinaryHeap, VecDeque};

fn extrema(
    p: &Plane,
    values: &[u8],
    radius: usize,
    dilate: bool,
    cancel: &CancelToken,
) -> AuraResult<Vec<u8>> {
    // A square structuring element decomposes exactly into two one-dimensional passes.
    let mut horizontal = vec![0; values.len()];
    let mut out = horizontal.clone();
    for y in 0..p.height {
        check(cancel)?;
        for x in 0..p.width {
            let mut v = if dilate { 0 } else { 255 };
            for sx in x.saturating_sub(radius)..(x + radius + 1).min(p.width) {
                v = if dilate {
                    v.max(values[y * p.width + sx])
                } else {
                    v.min(values[y * p.width + sx])
                };
            }
            // Outside-frame coverage is transparent, including during erosion.
            if !dilate && (x < radius || x + radius >= p.width) {
                v = 0;
            }
            horizontal[y * p.width + x] = v;
        }
    }
    for y in 0..p.height {
        check(cancel)?;
        for x in 0..p.width {
            let mut v = if dilate { 0 } else { 255 };
            for sy in y.saturating_sub(radius)..(y + radius + 1).min(p.height) {
                v = if dilate {
                    v.max(horizontal[sy * p.width + x])
                } else {
                    v.min(horizontal[sy * p.width + x])
                };
            }
            if !dilate && (y < radius || y + radius >= p.height) {
                v = 0;
            }
            out[y * p.width + x] = v;
        }
    }
    Ok(out)
}
pub(super) fn apply(
    p: &Plane,
    action: Action,
    o: &Options,
    point: [f32; 2],
    cancel: &CancelToken,
) -> AuraResult<Vec<u8>> {
    let radius = o.radius as usize;
    match action {
        Action::CoverageOpening | Action::CoverageClosing => {
            let first = extrema(
                p,
                &p.alpha,
                radius,
                action == Action::CoverageClosing,
                cancel,
            )?;
            extrema(p, &first, radius, action == Action::CoverageOpening, cancel)
        }
        Action::MorphologicalReconstruction => {
            let mut marker = extrema(p, &p.alpha, radius, false, cancel)?;
            let mut heap: BinaryHeap<_> = marker
                .iter()
                .enumerate()
                .filter(|(_, v)| **v > 0)
                .map(|(i, v)| (*v, i))
                .collect();
            while let Some((value, i)) = heap.pop() {
                check(cancel)?;
                if marker[i] != value {
                    continue;
                }
                for j in neighbors(i, p.width, p.height) {
                    let next = value.min(p.alpha[j]);
                    if next > marker[j] {
                        marker[j] = next;
                        heap.push((next, j));
                    }
                }
            }
            Ok(marker)
        }
        Action::GrayscaleAreaOpening => area_open(p, o.min_area.max(1), cancel),
        Action::BorderConnected => {
            let mut out = vec![0; p.alpha.len()];
            for group in p.region_groups(cancel)? {
                if group.iter().any(|i| {
                    let (x, y) = (i % p.width, i / p.width);
                    x == 0 || y == 0 || x + 1 == p.width || y + 1 == p.height
                }) {
                    for i in group {
                        out[i] = p.alpha[i];
                    }
                }
            }
            Ok(out)
        }
        Action::GeodesicRings => {
            let x = ((point[0] * p.width as f32) as usize).min(p.width - 1);
            let y = ((point[1] * p.height as f32) as usize).min(p.height - 1);
            let seed = y * p.width + x;
            if p.alpha[seed] < 128 {
                return Err(invalid("Place the geodesic seed inside selected coverage."));
            }
            let mut distance = vec![usize::MAX; p.alpha.len()];
            distance[seed] = 0;
            let mut queue = VecDeque::from([seed]);
            while let Some(i) = queue.pop_front() {
                check(cancel)?;
                for j in neighbors(i, p.width, p.height) {
                    if p.alpha[j] >= 128 && distance[j] == usize::MAX {
                        distance[j] = distance[i] + 1;
                        queue.push_back(j);
                    }
                }
            }
            Ok(distance
                .iter()
                .enumerate()
                .map(|(i, d)| {
                    if *d == usize::MAX {
                        0
                    } else {
                        byte(f32::from(p.alpha[i]) / 255.0 * band(*d as f32 / o.radius as f32, o))
                    }
                })
                .collect())
        }
        Action::HoleBearingRegions => hole_bearing(p, cancel),
        Action::RegionalMaxima => maxima(p, cancel),
        _ => Err(invalid("Unsupported topology operator.")),
    }
}
fn root(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}
fn area_open(p: &Plane, min_area: usize, cancel: &CancelToken) -> AuraResult<Vec<u8>> {
    let n = p.alpha.len();
    let mut buckets: Vec<Vec<usize>> = (0..256).map(|_| Vec::new()).collect();
    for (i, v) in p.alpha.iter().enumerate() {
        buckets[*v as usize].push(i);
    }
    let mut parent: Vec<_> = (0..n).collect();
    let mut sizes = vec![1; n];
    let mut pending: Vec<Vec<usize>> = (0..n).map(|i| vec![i]).collect();
    let mut active = vec![false; n];
    let mut out = vec![0; n];
    // Descending upper-level components form a max-tree. Each pending cell is emitted once.
    for level in (1..256).rev() {
        check(cancel)?;
        for &i in &buckets[level] {
            active[i] = true;
        }
        for &i in &buckets[level] {
            for j in neighbors(i, p.width, p.height) {
                if active[j] {
                    let (mut a, mut b) = (root(&mut parent, i), root(&mut parent, j));
                    if a != b {
                        if sizes[a] < sizes[b] {
                            std::mem::swap(&mut a, &mut b);
                        }
                        parent[b] = a;
                        sizes[a] += sizes[b];
                        let mut tail = std::mem::take(&mut pending[b]);
                        pending[a].append(&mut tail);
                    }
                }
            }
        }
        for &i in &buckets[level] {
            let r = root(&mut parent, i);
            if sizes[r] >= min_area {
                for j in std::mem::take(&mut pending[r]) {
                    out[j] = level as u8;
                }
            }
        }
    }
    Ok(out)
}
fn maxima(p: &Plane, cancel: &CancelToken) -> AuraResult<Vec<u8>> {
    let mut visited = vec![false; p.alpha.len()];
    let mut out = vec![0; p.alpha.len()];
    for seed in 0..p.alpha.len() {
        check(cancel)?;
        if visited[seed] || p.alpha[seed] == 0 {
            continue;
        }
        visited[seed] = true;
        let mut queue = VecDeque::from([seed]);
        let mut group = vec![];
        let mut higher = false;
        while let Some(i) = queue.pop_front() {
            group.push(i);
            for j in neighbors(i, p.width, p.height) {
                if p.alpha[j] > p.alpha[seed] {
                    higher = true;
                }
                if !visited[j] && p.alpha[j] == p.alpha[seed] {
                    visited[j] = true;
                    queue.push_back(j);
                }
            }
        }
        if !higher {
            for i in group {
                out[i] = p.alpha[i];
            }
        }
    }
    Ok(out)
}
fn hole_bearing(p: &Plane, cancel: &CancelToken) -> AuraResult<Vec<u8>> {
    let mut out = vec![0; p.alpha.len()];
    for group in p.region_groups(cancel)? {
        check(cancel)?;
        let (mut left, mut top, mut right, mut bottom) = (p.width, p.height, 0, 0);
        for &i in &group {
            left = left.min(i % p.width);
            right = right.max(i % p.width);
            top = top.min(i / p.width);
            bottom = bottom.max(i / p.width);
        }
        let (w, h) = (right - left + 3, bottom - top + 3);
        let mut occupied = vec![false; w * h];
        let mut seen = occupied.clone();
        for &i in &group {
            occupied[(i / p.width - top + 1) * w + i % p.width - left + 1] = true;
        }
        let mut queue = VecDeque::from([0]);
        seen[0] = true;
        while let Some(i) = queue.pop_front() {
            for j in neighbors(i, w, h) {
                if !seen[j] && !occupied[j] {
                    seen[j] = true;
                    queue.push_back(j);
                }
            }
        }
        if seen.iter().zip(&occupied).any(|(s, o)| !s && !o) {
            for i in group {
                out[i] = p.alpha[i];
            }
        }
    }
    Ok(out)
}
