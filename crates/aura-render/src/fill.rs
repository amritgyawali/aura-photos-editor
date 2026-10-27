//! Filling a hole in a plane from what surrounds it.
//!
//! Push-pull interpolation: the known samples are averaged down a pyramid, each cell weighted by
//! how much known data it holds, until every cell has support; each level is then blended back
//! up over the bilinearly enlarged level below it. The result keeps every fully known sample
//! exactly, is smooth inside a hole, and continues the colours and the broad lines that meet the
//! hole from every side. It costs a constant number of passes over the plane, whatever the size
//! of the hole. Used for the background behind a reshaped subject and for the tone under a lens
//! reflection.

#![allow(clippy::indexing_slicing)] // Fixed-size pyramid levels, indexed within their bounds.

use rayon::prelude::*;

/// One pyramid level: its size, its values and how much known data each cell holds.
struct Level {
    w: usize,
    h: usize,
    values: Vec<f32>,
    weight: Vec<f32>,
}

/// Bilinear sample of an interleaved plane, clamped at the edge.
fn sample(values: &[f32], channels: usize, w: usize, h: usize, x: f32, y: f32, out: &mut [f32]) {
    let sx = x.clamp(0.0, (w - 1) as f32);
    let sy = y.clamp(0.0, (h - 1) as f32);
    let (x0, y0) = (sx.floor() as usize, sy.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (fx, fy) = (sx - x0 as f32, sy - y0 as f32);
    for (c, o) in out.iter_mut().enumerate().take(channels) {
        let a = values[(y0 * w + x0) * channels + c];
        let b = values[(y0 * w + x1) * channels + c];
        let d = values[(y1 * w + x0) * channels + c];
        let e = values[(y1 * w + x1) * channels + c];
        let top = a + (b - a) * fx;
        let bottom = d + (e - d) * fx;
        *o = top + (bottom - top) * fy;
    }
}

/// Fill `values` (interleaved, `channels` per pixel) wherever `weight` is below one.
///
/// `weight` is how much each sample is known, `0..=1`. A weight of one keeps the sample
/// exactly; zero replaces it entirely; anything between blends the two. With no known sample at
/// all the plane is returned unchanged.
#[must_use]
#[allow(clippy::too_many_lines)]
pub(crate) fn push_pull(
    values: &[f32],
    channels: usize,
    weight: &[f32],
    w: usize,
    h: usize,
) -> Vec<f32> {
    if w == 0
        || h == 0
        || channels == 0
        || values.len() != w * h * channels
        || weight.len() != w * h
        || !weight.iter().any(|k| *k > 0.0)
    {
        return values.to_vec();
    }
    let mut levels = vec![Level {
        w,
        h,
        values: values.to_vec(),
        weight: weight.iter().map(|k| k.clamp(0.0, 1.0)).collect(),
    }];
    // Push: average down until every cell holds enough known data.
    while let Some(last) = levels.last() {
        if (last.w <= 1 && last.h <= 1) || last.weight.iter().all(|k| *k >= 1.0) {
            break;
        }
        let (lw, lh) = (last.w, last.h);
        let (nw, nh) = (lw.div_ceil(2), lh.div_ceil(2));
        let mut next_values = vec![0.0; nw * nh * channels];
        let mut next_weight = vec![0.0; nw * nh];
        next_values
            .par_chunks_mut(nw * channels)
            .zip(next_weight.par_chunks_mut(nw))
            .enumerate()
            .for_each(|(y, (row, row_weight))| {
                let mut sum = vec![0.0_f32; channels];
                for x in 0..nw {
                    sum.fill(0.0);
                    let mut total = 0.0_f32;
                    for (sx, sy) in [
                        (2 * x, 2 * y),
                        (2 * x + 1, 2 * y),
                        (2 * x, 2 * y + 1),
                        (2 * x + 1, 2 * y + 1),
                    ] {
                        if sx >= lw || sy >= lh {
                            continue;
                        }
                        let k = last.weight[sy * lw + sx];
                        if k <= 0.0 {
                            continue;
                        }
                        let i = (sy * lw + sx) * channels;
                        for (c, v) in sum.iter_mut().enumerate() {
                            *v += k * last.values[i + c];
                        }
                        total += k;
                    }
                    if total > 0.0 {
                        for (c, v) in sum.iter().enumerate() {
                            row[x * channels + c] = v / total;
                        }
                    }
                    row_weight[x] = total.min(1.0);
                }
            });
        levels.push(Level {
            w: nw,
            h: nh,
            values: next_values,
            weight: next_weight,
        });
    }
    // Pull: blend each level back up over the enlarged level below it.
    let mut filled = levels.last().map_or_else(Vec::new, |l| l.values.clone());
    for l in (0..levels.len().saturating_sub(1)).rev() {
        let level = &levels[l];
        let (cw, ch) = (levels[l + 1].w, levels[l + 1].h);
        let lw = level.w;
        let mut next = vec![0.0; lw * level.h * channels];
        next.par_chunks_mut(lw * channels)
            .enumerate()
            .for_each(|(y, row)| {
                let mut up = vec![0.0_f32; channels];
                for x in 0..lw {
                    let i = y * lw + x;
                    let k = level.weight[i];
                    let known = &level.values[i * channels..(i + 1) * channels];
                    let out = &mut row[x * channels..(x + 1) * channels];
                    if k >= 1.0 {
                        out.copy_from_slice(known);
                        continue;
                    }
                    // This cell's centre in the coarser level's pixel coordinates.
                    sample(
                        &filled,
                        channels,
                        cw,
                        ch,
                        (x as f32 + 0.5) * 0.5 - 0.5,
                        (y as f32 + 0.5) * 0.5 - 0.5,
                        &mut up,
                    );
                    for c in 0..channels {
                        out[c] = k * known[c] + (1.0 - k) * up[c];
                    }
                }
            });
        filled = next;
    }
    filled
}

/// How far each pixel is from the nearest `true` one, in pixels: a two-pass chamfer distance
/// (steps of one and the square root of two). With no `true` pixel every distance is the
/// plane's width plus its height.
#[must_use]
pub(crate) fn distance_to(mask: &[bool], w: usize, h: usize) -> Vec<f32> {
    const STRAIGHT: f32 = 1.0;
    const DIAGONAL: f32 = std::f32::consts::SQRT_2;
    let far = (w + h) as f32;
    if mask.len() != w * h {
        return vec![far; w * h];
    }
    let mut d: Vec<f32> = mask.iter().map(|m| if *m { 0.0 } else { far }).collect();
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let mut v = d[i];
            if x > 0 {
                v = v.min(d[i - 1] + STRAIGHT);
            }
            if y > 0 {
                v = v.min(d[i - w] + STRAIGHT);
                if x > 0 {
                    v = v.min(d[i - w - 1] + DIAGONAL);
                }
                if x + 1 < w {
                    v = v.min(d[i - w + 1] + DIAGONAL);
                }
            }
            d[i] = v;
        }
    }
    for y in (0..h).rev() {
        for x in (0..w).rev() {
            let i = y * w + x;
            let mut v = d[i];
            if x + 1 < w {
                v = v.min(d[i + 1] + STRAIGHT);
            }
            if y + 1 < h {
                v = v.min(d[i + w] + STRAIGHT);
                if x + 1 < w {
                    v = v.min(d[i + w + 1] + DIAGONAL);
                }
                if x > 0 {
                    v = v.min(d[i + w - 1] + DIAGONAL);
                }
            }
            d[i] = v;
        }
    }
    d
}

/// The pixels `mask` encloses: not in the mask, and not reachable from outside the selection
/// (`cover` zero, or the plane's border) without crossing it. Gaps in the mask narrower than
/// about twice `leak` pixels are sealed first, so a hairline opening (the corner of an eye at
/// the edge of a reflection) does not let the outside in. A notch open to the outside is never
/// enclosed, however deep.
#[must_use]
pub(crate) fn enclosed(mask: &[bool], cover: &[f32], w: usize, h: usize, leak: f32) -> Vec<bool> {
    if mask.len() != w * h || cover.len() != w * h {
        return vec![false; w * h];
    }
    let barrier: Vec<bool> = distance_to(mask, w, h).iter().map(|d| *d <= leak).collect();
    let mut reached = vec![false; w * h];
    let mut queue = std::collections::VecDeque::new();
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let border = x == 0 || y == 0 || x + 1 == w || y + 1 == h;
            if !barrier[i] && (border || cover[i] <= 0.0) {
                reached[i] = true;
                queue.push_back(i);
            }
        }
    }
    while let Some(i) = queue.pop_front() {
        let (x, y) = (i % w, i / w);
        let mut visit = |j: usize| {
            if !barrier[j] && !reached[j] {
                reached[j] = true;
                queue.push_back(j);
            }
        };
        if x > 0 {
            visit(i - 1);
        }
        if x + 1 < w {
            visit(i + 1);
        }
        if y > 0 {
            visit(i - w);
        }
        if y + 1 < h {
            visit(i + w);
        }
    }
    // The seal itself belongs to whatever side it was sealing against.
    let near_outside = distance_to(&reached, w, h);
    (0..w * h)
        .map(|i| !mask[i] && !reached[i] && near_outside[i] > leak)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::float_cmp)] // Known samples must be kept exactly.
    fn known_samples_are_kept_and_a_hole_takes_its_surroundings() {
        let (w, h) = (37, 23);
        let values: Vec<f32> = (0..w * h).map(|i| (i % w) as f32 / w as f32).collect();
        let weight: Vec<f32> = (0..w * h)
            .map(|i| {
                if (10..25).contains(&(i % w)) {
                    0.0
                } else {
                    1.0
                }
            })
            .collect();
        let mut holed = values.clone();
        for (v, k) in holed.iter_mut().zip(&weight) {
            if *k == 0.0 {
                *v = 99.0;
            }
        }
        let filled = push_pull(&holed, 1, &weight, w, h);
        for (i, k) in weight.iter().enumerate() {
            if *k == 1.0 {
                assert_eq!(filled[i], holed[i]);
            } else {
                // Between the two sides of the hole, never the garbage inside it.
                assert!((0.2..0.75).contains(&filled[i]), "{} at {i}", filled[i]);
            }
        }
        // Monotone across the hole along a row: it continues the ramp.
        let row: Vec<f32> = (9..26).map(|x| filled[11 * w + x]).collect();
        assert!(row.windows(2).all(|p| p[1] >= p[0] - 1e-4), "{row:?}");
    }

    #[test]
    #[allow(clippy::float_cmp)] // A distance on the mask is exactly zero.
    fn a_ring_encloses_its_hole_but_a_notch_open_to_the_outside_is_not_enclosed() {
        let (w, h) = (100, 100);
        let c = 50.0;
        // A ring (outer radius 30, hole radius 10) with a notch cut in from the right.
        let mask: Vec<bool> = (0..w * h)
            .map(|i| {
                let (x, y) = ((i % w) as f32 - c, (i / w) as f32 - c);
                let notch = x > 15.0 && y.abs() < 4.0;
                (10.0..30.0).contains(&x.hypot(y)) && !notch
            })
            .collect();
        let cover = vec![1.0; w * h];
        let inside = enclosed(&mask, &cover, w, h, 2.0);
        assert!(inside[50 * w + 50], "the hole is enclosed");
        assert!(
            !inside[50 * w + 50 + 22],
            "the notch is open to the outside"
        );
        assert!(!inside[5 * w + 5], "outside is outside");
        assert!(!inside[70 * w + 50], "the mask itself is not a hole");
        assert_eq!(distance_to(&mask, w, h)[70 * w + 50], 0.0);
    }

    #[test]
    fn nothing_known_returns_the_input() {
        let values = vec![0.5; 12];
        assert_eq!(push_pull(&values, 3, &[0.0; 4], 2, 2), values);
    }
}
