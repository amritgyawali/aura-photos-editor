//! Clearing stretch marks: evening that flows across a mark and stops at structure. ADR-0118.
//!
//! A stretch mark is long, thin and faint, and an abdomen carries dozens of them. The spot
//! healers are built for compact marks and leave them; a smoothing wide enough to clear them
//! also flattens the hands resting on the abdomen and erases a navel. What separates the two is
//! not contrast - the darkest marks are nearly as strong as the outline of a finger - but
//! shape: a mark is a line with the same skin on both sides of it, so its gradients cancel
//! over a window wider than the mark, while the edge of a finger, a knuckle crease or the rim
//! of a navel is a step, and a step survives that window.
//!
//! So the operation has three parts. It finds structure as steps that survive, kept as whole
//! contours by growing each strong step along the weaker ones it is connected to. It evens the
//! selected skin with a filter that flows freely across everything else and is stopped by
//! structure and by the selection's edge, so a finger keeps its own tone and the abdomen never
//! takes a hand's. And it puts back the finest grain, limited to the level clean skin carries,
//! because the edge of a mark is larger than a pore.
// Every plane is `rect.w * rect.h` long and every coordinate is bounded before it is used.
#![allow(clippy::indexing_slicing)]
use crate::retouch_mask::Coverage;
use crate::retouch_planes::{blur, luma, quantile, smoothstep, Lean, Rect};
use aura_recipe::retouch_tools::Edit;
use rayon::prelude::*;

/// The window structure is measured over, as a share of the evening radius. Wider than a mark
/// and narrower than a finger.
const WINDOW: f32 = 0.18;
/// A step of this size across the window, in natural-log units of luminance, is structure
/// wherever it is found: the rim of a navel, the outline of a finger, a knuckle.
const SURE_STEP: f32 = 0.36;
/// Connected to a sure step, a step this small is still part of the same contour. The darkest
/// stretch marks reach about 0.28 at their strongest point and are never connected to one.
const CONTOUR_STEP: f32 = 0.16;
/// How firmly structure stops the evening, in radii of distance per cell of structure.
const STOP: f32 = 3.0;
/// Grain is limited to this many times the selected skin's own median.
const GRAIN_LIMIT: f32 = 2.5;
/// A sample the evening would move by less than this, in natural-log units of luminance, is
/// a mark and is evened in full: the cores of the deepest stretch marks measured sit between
/// 0.45 and 0.75 from the skin they cross.
const FAINT: f32 = 0.75;
/// And one it would move by this much is a third as bright as the skin around it, or three
/// times: pigment, a shadow, a highlight. Left exactly as it is, with a fade between the two.
/// A mole measured 1.3 and more.
const NOT_A_MARK: f32 = 1.098_612_3;
/// The shading correction beside a contour may be at most this share of the tone it corrects.
const MOST_SHADING: f32 = 0.15;
/// Passes of the recursive filter; three give a close approximation of a Gaussian.
const PASSES: usize = 3;
/// At most this many cells are read for the grain median.
const GRAIN_SAMPLES: usize = 200_000;

/// Steps that survive a window wider than a mark, as whole contours: 1 on structure, 0 on
/// open skin, with a soft cell between.
fn structure(log_luma: &[f32], w: usize, h: usize, window: usize) -> Vec<f32> {
    let at = |x: usize, y: usize| log_luma[y * w + x];
    let mut gx = vec![0.0_f32; w * h];
    let mut gy = vec![0.0_f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let (l, r) = (x.saturating_sub(1), (x + 1).min(w - 1));
            let (t, b) = (y.saturating_sub(1), (y + 1).min(h - 1));
            gx[y * w + x] = (at(r, y) - at(l, y)) / (r - l).max(1) as f32;
            gy[y * w + x] = (at(x, b) - at(x, t)) / (b - t).max(1) as f32;
        }
    }
    // The mean gradient over the window: opposite slopes either side of a line cancel here.
    let (mx, my) = (blur(&gx, w, h, window), blur(&gy, w, h, window));
    let span = (window * 2) as f32;
    let step: Vec<f32> = mx
        .iter()
        .zip(&my)
        .map(|(x, y)| x.hypot(*y) * span)
        .collect();
    let mut kept: Vec<bool> = step.iter().map(|s| *s >= SURE_STEP).collect();
    let mut frontier: Vec<usize> = (0..w * h).filter(|&i| kept[i]).collect();
    while let Some(i) = frontier.pop() {
        let (x, y) = (i % w, i / w);
        for ny in y.saturating_sub(1)..=(y + 1).min(h - 1) {
            for nx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                let n = ny * w + nx;
                if !kept[n] && step[n] >= CONTOUR_STEP {
                    kept[n] = true;
                    frontier.push(n);
                }
            }
        }
    }
    let hard: Vec<f32> = kept.iter().map(|k| f32::from(u8::from(*k))).collect();
    blur(&hard, w, h, 1)
        .iter()
        .map(|v| (v * 1.5).min(1.0))
        .collect()
}

/// One sample of the planes that are evened together: weighted red, green and blue, the
/// weight itself, and the weighted cell coordinates, which come back as where the mean was
/// read from.
type Cell = [f32; 6];

/// A recursive filter (Gastal and Oliveira's domain transform) whose reach is `radius` over
/// open skin and is cut short wherever `stop` is set. Every cell ends as a mean of the cells
/// it can reach without crossing structure, nearer ones counting for more.
fn evened(cells: &mut [Cell], stop: &[f32], w: usize, h: usize, radius: f32) {
    // Distance from each cell to its right-hand and lower neighbour, in cells of open skin.
    let block = STOP * radius;
    let across: Vec<f32> = (0..w * h)
        .map(|i| {
            let next = if (i % w) + 1 < w { i + 1 } else { i };
            1.0 + block * stop[i].max(stop[next])
        })
        .collect();
    let down: Vec<f32> = (0..w * h)
        .map(|i| {
            let next = if i + w < w * h { i + w } else { i };
            1.0 + block * stop[i].max(stop[next])
        })
        .collect();
    let mix =
        |a: Cell, b: Cell, f: f32| -> Cell { std::array::from_fn(|c| a[c] + f * (b[c] - a[c])) };
    for pass in 0..PASSES {
        // The schedule from the paper: each pass half as wide, together one Gaussian.
        let sigma = radius * 3.0_f32.sqrt() * 2.0_f32.powi((PASSES - pass - 1) as i32)
            / (4.0_f32.powi(PASSES as i32) - 1.0).sqrt();
        let decay = -(2.0_f32.sqrt()) / sigma;
        cells
            .par_chunks_exact_mut(w)
            .zip(across.par_chunks_exact(w))
            .for_each(|(row, gap)| {
                for x in 1..w {
                    row[x] = mix(row[x], row[x - 1], (decay * gap[x - 1]).exp());
                }
                for x in (0..w.saturating_sub(1)).rev() {
                    row[x] = mix(row[x], row[x + 1], (decay * gap[x]).exp());
                }
            });
        for y in 1..h {
            let (above, below) = cells.split_at_mut(y * w);
            let (previous, row) = (&above[(y - 1) * w..], &mut below[..w]);
            let gap = &down[(y - 1) * w..y * w];
            for x in 0..w {
                row[x] = mix(row[x], previous[x], (decay * gap[x]).exp());
            }
        }
        for y in (0..h.saturating_sub(1)).rev() {
            let (above, below) = cells.split_at_mut((y + 1) * w);
            let (row, next) = (&mut above[y * w..], &below[..w]);
            let gap = &down[y * w..(y + 1) * w];
            for x in 0..w {
                row[x] = mix(row[x], next[x], (decay * gap[x]).exp());
            }
        }
    }
}

/// The evened colour of every cell, one plane per channel, with the lean taken back out.
///
/// Beside a contour or the selection's edge the mean is read from one side only, and is then
/// the skin a little way off rather than the skin at the cell. The coordinates that went
/// through the filter say where; the photograph's own shading between there and the cell is
/// removed, as it is for every evening in this crate.
fn levelled(cells: &[Cell], rw: usize, rh: usize) -> Vec<Vec<f32>> {
    let read = |axis: usize| -> Vec<f32> {
        cells
            .iter()
            .enumerate()
            .map(|(local, cell)| {
                if cell[3] > 1e-4 {
                    cell[axis] / cell[3]
                } else if axis == 4 {
                    (local % rw) as f32
                } else {
                    (local / rw) as f32
                }
            })
            .collect()
    };
    let own = |axis: fn(usize, usize) -> usize| -> Vec<f32> {
        (0..cells.len()).map(|i| axis(i, rw) as f32).collect()
    };
    let lean = Lean::from_positions(&read(4), &read(5), rw);
    let none = Lean::from_positions(&own(|i, w| i % w), &own(|i, w| i / w), rw);
    (0..3)
        .map(|c| {
            let mut plane: Vec<f32> = cells
                .iter()
                .map(|cell| {
                    if cell[3] > 1e-4 {
                        cell[c] / cell[3]
                    } else {
                        0.0
                    }
                })
                .collect();
            let shading = lean.shading_between(&none, &plane, rw, rh);
            for ((value, leaked), cell) in plane.iter_mut().zip(shading).zip(cells) {
                // A first-order correction: trusted where the mean had real support, and
                // never for more than a modest share of the tone it corrects.
                let bound = *value * MOST_SHADING;
                *value -= leaked.clamp(-bound, bound) * smoothstep(0.1, 0.4, cell[3]);
            }
            plane
        })
        .collect()
}

pub(crate) fn apply(rgb: &mut [f32], w: usize, h: usize, edit: &Edit, coverage: &Coverage) {
    let radius = (edit.radius * w.min(h) as f32).round().max(2.0);
    let Some(rect) = Rect::around(coverage.bounds, radius as usize * 3, w, h) else {
        return;
    };
    let (rw, rh) = (rect.w, rect.h);
    let pixel = |x: usize, y: usize| -> [f32; 3] {
        let i = (y * w + x) * 3;
        [rgb[i], rgb[i + 1], rgb[i + 2]]
    };
    let cover: Vec<f32> = rect.cells().map(|(x, y)| coverage.at(x, y, w, h)).collect();
    let light: Vec<f32> = rect.cells().map(|(x, y)| luma(pixel(x, y))).collect();
    let log_luma: Vec<f32> = blur(&light, rw, rh, 1)
        .iter()
        .map(|v| v.max(1e-4).ln())
        .collect();
    let window = ((radius * WINDOW).round() as usize).max(1);
    let structure = structure(&log_luma, rw, rh, window);
    // Structure is neither read nor written; nor is anything outside the selection, and the
    // selection's own edge stops the flow as a contour does.
    let weights: Vec<f32> = cover
        .iter()
        .zip(&structure)
        .map(|(c, s)| c * (1.0 - s))
        .collect();
    let stop: Vec<f32> = cover
        .iter()
        .zip(&structure)
        .map(|(c, s)| s.max(1.0 - smoothstep(0.05, 0.4, *c)))
        .collect();
    let mut cells: Vec<Cell> = rect
        .cells()
        .zip(&weights)
        .enumerate()
        .map(|(local, ((x, y), a))| {
            let p = pixel(x, y);
            let (lx, ly) = ((local % rw) as f32, (local / rw) as f32);
            [p[0] * a, p[1] * a, p[2] * a, *a, lx * a, ly * a]
        })
        .collect();
    evened(&mut cells, &stop, rw, rh, radius);
    let even = levelled(&cells, rw, rh);
    // The finest band of each channel, and how large it is on the skin being evened.
    let grain: Vec<Vec<f32>> = (0..3)
        .map(|c| {
            let plane: Vec<f32> = rect.cells().map(|(x, y)| pixel(x, y)[c]).collect();
            let low = blur(&plane, rw, rh, 1);
            plane.iter().zip(low).map(|(v, l)| v - l).collect()
        })
        .collect();
    let size = |i: usize| luma([grain[0][i], grain[1][i], grain[2][i]]).abs();
    let selected: Vec<usize> = (0..rect.len()).filter(|&i| weights[i] > 0.5).collect();
    let stride = (selected.len() / GRAIN_SAMPLES).max(1);
    let mut sizes: Vec<f32> = selected.iter().step_by(stride).map(|&i| size(i)).collect();
    let limit = quantile(&mut sizes, 0.5).unwrap_or(0.0) * GRAIN_LIMIT;
    for (local, (x, y)) in rect.cells().enumerate() {
        let support = cells[local][3];
        let alpha = weights[local] * smoothstep(0.0, 0.3, support) * edit.amount;
        if alpha <= 0.0 || support <= 1e-4 {
            continue;
        }
        let keep = (limit / size(local).max(1e-9)).min(1.0) * edit.texture;
        let value: [f32; 3] =
            std::array::from_fn(|c| (even[c][local] + grain[c][local] * keep).max(0.0));
        // A stretch mark is faint. A sample the evening would move a long way is something
        // the contours missed - a mole, a hair, the dark between two fingers - and is left.
        let moved = (luma(value).max(1e-6) / light[local].max(1e-6)).ln().abs();
        let alpha = alpha * (1.0 - smoothstep(FAINT, NOT_A_MARK, moved));
        let i = (y * w + x) * 3;
        for c in 0..3 {
            rgb[i + c] += alpha * (value[c] - rgb[i + c]);
        }
    }
}
