//! Confirm coarse connectivity on original pixels. A cell average must never bridge a
//! protected slit or a thin contour into another skin-coloured surface.
#![allow(clippy::indexing_slicing)]

use super::{affinity, chroma, luma, Coverage, Edit, EPSILON};
use std::collections::VecDeque;

#[derive(Clone, Copy)]
pub(super) struct Coarse<'a> {
    pub cell: usize,
    pub width: usize,
    pub reached: &'a [bool],
    pub seed: Option<usize>,
}

pub(super) fn can_cross(a: [f32; 3], b: [f32; 3], tolerance: f32) -> bool {
    let (la, lb) = (luma(a), luma(b));
    let step = (la - lb).abs() / la.max(lb).max(EPSILON);
    let distance = chroma(a)
        .iter()
        .zip(chroma(b))
        .map(|(p, q)| (p - q).powi(2))
        .sum::<f32>()
        .sqrt();
    step < 0.22 && distance < tolerance * 0.6
}

/// Zero is excluded, one is eligible but disconnected, two is reached. Only cells
/// already accepted by the coarse pass are candidates; this check can only remove
/// support. Four-neighbour traversal cannot jump a zero pixel or a diagonal gap.
pub(super) fn refine(
    rgb: &[f32],
    w: usize,
    h: usize,
    edit: &Edit,
    coverage: &Coverage,
    sample: [f32; 3],
    coarse: Coarse<'_>,
) -> Vec<u8> {
    let [x0, y0, x1, y1] = coverage.bounds;
    let (bw, bh) = (x1 - x0, y1 - y0);
    let mut state = vec![0; bw * bh];
    let (Some(seed), Some(point)) = (coarse.seed, edit.source) else {
        return state;
    };
    let tolerance = edit.skin.unwrap_or_default().tolerance;
    let pixel = |x: usize, y: usize| {
        let i = (y * w + x) * 3;
        [rgb[i], rgb[i + 1], rgb[i + 2]]
    };
    for y in y0..y1 {
        for x in x0..x1 {
            let k = ((y - y0) / coarse.cell) * coarse.width + (x - x0) / coarse.cell;
            if !coarse.reached[k] || coverage.at(x, y, w, h) <= 0.0 {
                continue;
            }
            let p = pixel(x, y);
            if p.iter().all(|v| v.is_finite() && *v >= 0.0) && affinity(p, sample, tolerance) > 0.0
            {
                state[(y - y0) * bw + x - x0] = 1;
            }
        }
    }
    // Preserve the coarse pass's bounded seed search, choosing its nearest actual
    // eligible pixel rather than treating a mixed cell as one connected surface.
    let (sx, sy) = (
        (seed % coarse.width) * coarse.cell,
        (seed / coarse.width) * coarse.cell,
    );
    let target = [
        point[0] * w as f32 - x0 as f32,
        point[1] * h as f32 - y0 as f32,
    ];
    let mut start = None;
    let mut nearest = f32::INFINITY;
    for y in sy..(sy + coarse.cell).min(bh) {
        for x in sx..(sx + coarse.cell).min(bw) {
            let distance = (x as f32 - target[0]).powi(2) + (y as f32 - target[1]).powi(2);
            if state[y * bw + x] == 1 && distance < nearest {
                nearest = distance;
                start = Some(y * bw + x);
            }
        }
    }
    let Some(start) = start else {
        return state;
    };
    let mut frontier = VecDeque::from([start]);
    state[start] = 2;
    while let Some(k) = frontier.pop_front() {
        let (x, y) = (k % bw, k / bw);
        let here = pixel(x0 + x, y0 + y);
        let mut visit = |n: usize| {
            if state[n] == 1 && can_cross(here, pixel(x0 + n % bw, y0 + n / bw), tolerance) {
                state[n] = 2;
                frontier.push_back(n);
            }
        };
        if x > 0 {
            visit(k - 1);
        }
        if x + 1 < bw {
            visit(k + 1);
        }
        if y > 0 {
            visit(k - bw);
        }
        if y + 1 < bh {
            visit(k + bw);
        }
    }
    state
}
