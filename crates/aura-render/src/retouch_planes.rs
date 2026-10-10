//! Plane arithmetic shared by frequency healing and the texture graft. ADR-0090.
//!
//! Both operations work on a rectangle cut out of the frame, and both need the same four
//! things: a blur whose cost does not grow with its radius, a blur that averages selected
//! skin only, connected groups of flagged pixels with their shape, and a distance to the
//! nearest flagged pixel. They live here so the two operations cannot disagree about any of
//! them.
// Every plane is `width * height` long and every coordinate is bounded before it is used.
#![allow(clippy::indexing_slicing)]

/// Linear Rec.2020 luminance, the weights every retouch operator in this crate uses.
pub(crate) fn luma(p: [f32; 3]) -> f32 {
    p[0] * 0.2627 + p[1] * 0.6780 + p[2] * 0.0593
}

/// A rectangle of the frame, padded so a blur near the selection's edge sees real pixels.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Rect {
    pub x0: usize,
    pub y0: usize,
    pub w: usize,
    pub h: usize,
}

impl Rect {
    /// `bounds` (left, top, right, bottom; right and bottom exclusive) grown by `margin` and
    /// clipped to the frame. `None` when the bounds are empty.
    pub(crate) fn around(bounds: [usize; 4], margin: usize, w: usize, h: usize) -> Option<Self> {
        let [x0, y0, x1, y1] = bounds;
        if x1 <= x0 || y1 <= y0 || x0 >= w || y0 >= h {
            return None;
        }
        let left = x0.saturating_sub(margin);
        let top = y0.saturating_sub(margin);
        let right = (x1 + margin).min(w);
        let bottom = (y1 + margin).min(h);
        Some(Self {
            x0: left,
            y0: top,
            w: right - left,
            h: bottom - top,
        })
    }

    pub(crate) fn len(self) -> usize {
        self.w * self.h
    }

    /// Frame coordinates of every cell, row-major.
    pub(crate) fn cells(self) -> impl Iterator<Item = (usize, usize)> {
        (self.y0..self.y0 + self.h)
            .flat_map(move |y| (self.x0..self.x0 + self.w).map(move |x| (x, y)))
    }
}

/// One box pass along rows with a running sum, so the cost is the same at every radius.
fn box_rows(src: &[f32], dst: &mut [f32], w: usize, h: usize, radius: usize) {
    for y in 0..h {
        let row = &src[y * w..(y + 1) * w];
        let out = &mut dst[y * w..(y + 1) * w];
        let mut sum: f64 = row.iter().take(radius + 1).map(|v| f64::from(*v)).sum();
        for x in 0..w {
            let lo = x.saturating_sub(radius);
            let hi = (x + radius + 1).min(w);
            out[x] = (sum / (hi - lo) as f64) as f32;
            if x >= radius {
                sum -= f64::from(row[x - radius]);
            }
            if x + radius + 1 < w {
                sum += f64::from(row[x + radius + 1]);
            }
        }
    }
}

/// The same pass along columns.
fn box_columns(src: &[f32], dst: &mut [f32], w: usize, h: usize, radius: usize) {
    for x in 0..w {
        let mut sum: f64 = (0..(radius + 1).min(h))
            .map(|y| f64::from(src[y * w + x]))
            .sum();
        for y in 0..h {
            let lo = y.saturating_sub(radius);
            let hi = (y + radius + 1).min(h);
            dst[y * w + x] = (sum / (hi - lo) as f64) as f32;
            if y >= radius {
                sum -= f64::from(src[(y - radius) * w + x]);
            }
            if y + radius + 1 < h {
                sum += f64::from(src[(y + radius + 1) * w + x]);
            }
        }
    }
}

/// Three box passes: a close approximation of a Gaussian whose standard deviation is about
/// the radius. Fixed order and no parallelism, so the result is the same on every machine.
pub(crate) fn blur(values: &[f32], w: usize, h: usize, radius: usize) -> Vec<f32> {
    let mut buffer = values.to_vec();
    if w == 0 || h == 0 || radius == 0 || buffer.len() != w * h {
        return buffer;
    }
    let mut scratch = vec![0.0_f32; w * h];
    for _ in 0..3 {
        box_rows(&buffer, &mut scratch, w, h, radius);
        box_columns(&scratch, &mut buffer, w, h, radius);
    }
    buffer
}

/// A blur that averages weighted samples only: `blur(v * weight) / blur(weight)`.
///
/// Hair, lips and eyes outside a skin selection therefore never darken the average near its
/// edge, and a flagged mark never colours the estimate of the skin around it.
#[derive(Debug)]
pub(crate) struct Weighted<'a> {
    weights: &'a [f32],
    spread: Vec<f32>,
    w: usize,
    h: usize,
    radius: usize,
}

impl<'a> Weighted<'a> {
    pub(crate) fn new(weights: &'a [f32], w: usize, h: usize, radius: usize) -> Self {
        Self {
            weights,
            spread: blur(weights, w, h, radius),
            w,
            h,
            radius,
        }
    }

    /// The share of the neighbourhood that carried weight, 0..=1.
    pub(crate) fn support(&self) -> &[f32] {
        &self.spread
    }

    /// The weighted mean of `values`. Where nothing nearby carried weight the original value
    /// is returned, so an unsupported cell is never a made-up colour.
    pub(crate) fn mean(&self, values: &[f32]) -> Vec<f32> {
        let scaled: Vec<f32> = values
            .iter()
            .zip(self.weights)
            .map(|(v, a)| v * a)
            .collect();
        blur(&scaled, self.w, self.h, self.radius)
            .iter()
            .zip(&self.spread)
            .zip(values)
            .map(|((sum, weight), old)| if *weight > 1e-4 { sum / weight } else { *old })
            .collect()
    }
}

/// The value below which `share` of the samples fall. `samples` is reordered.
pub(crate) fn quantile(samples: &mut [f32], share: f32) -> Option<f32> {
    if samples.is_empty() {
        return None;
    }
    let at = ((samples.len() - 1) as f32 * share.clamp(0.0, 1.0)).round() as usize;
    let (_, value, _) = samples.select_nth_unstable_by(at, f32::total_cmp);
    Some(*value)
}

/// Median and a robust spread (1.4826 times the median absolute deviation, which equals the
/// standard deviation for a normal distribution and ignores up to half the samples being
/// marks, glints or hair).
pub(crate) fn centre_and_spread(samples: &mut [f32]) -> Option<(f32, f32)> {
    let median = quantile(samples, 0.5)?;
    for value in samples.iter_mut() {
        *value = (*value - median).abs();
    }
    Some((median, quantile(samples, 0.5)? * 1.4826))
}

/// One connected group of flagged cells and the shape measurements a caller filters on.
#[derive(Debug)]
pub(crate) struct Group {
    pub cells: Vec<usize>,
    pub centre: [f32; 2],
    /// Standard deviation along the long and the short axis, in cells.
    pub axes: [f32; 2],
    /// Distance from the centre to the farthest cell.
    pub reach: f32,
}

impl Group {
    /// How many times longer than wide the group is.
    pub(crate) fn elongation(&self) -> f32 {
        self.axes[0] / self.axes[1].max(0.35)
    }
}

/// Four-connected groups of `true` cells, in scan order. Deterministic.
pub(crate) fn groups(flagged: &[bool], w: usize, h: usize) -> Vec<Group> {
    let mut seen = vec![false; flagged.len()];
    let mut out = Vec::new();
    for start in 0..flagged.len() {
        if !flagged[start] || seen[start] {
            continue;
        }
        seen[start] = true;
        let mut cells = vec![start];
        let mut next = 0;
        while next < cells.len() {
            let i = cells[next];
            next += 1;
            let (x, y) = (i % w, i / w);
            for j in [
                (x > 0).then(|| i - 1),
                (x + 1 < w).then_some(i + 1),
                (y > 0).then(|| i - w),
                (y + 1 < h).then_some(i + w),
            ]
            .into_iter()
            .flatten()
            {
                if flagged[j] && !seen[j] {
                    seen[j] = true;
                    cells.push(j);
                }
            }
        }
        let area = cells.len() as f32;
        let cx = cells.iter().map(|i| (i % w) as f32).sum::<f32>() / area;
        let cy = cells.iter().map(|i| (i / w) as f32).sum::<f32>() / area;
        let (mut xx, mut yy, mut xy, mut reach) = (0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32);
        for &i in &cells {
            let dx = (i % w) as f32 - cx;
            let dy = (i / w) as f32 - cy;
            xx += dx * dx;
            yy += dy * dy;
            xy += dx * dy;
            reach = reach.max(dx.hypot(dy));
        }
        let half = (xx + yy) * 0.5 / area;
        let root = (((xx - yy) * 0.5).powi(2) + xy * xy).sqrt() / area;
        out.push(Group {
            cells,
            centre: [cx, cy],
            axes: [(half + root).max(0.0).sqrt(), (half - root).max(0.0).sqrt()],
            reach,
        });
    }
    out
}

/// Distance, in cells, from every cell to the nearest flagged one (two-pass chamfer).
/// Cells farther than the plane's own size report that size.
pub(crate) fn distance_to(flagged: &[bool], w: usize, h: usize) -> Vec<f32> {
    const DIAGONAL: f32 = std::f32::consts::SQRT_2;
    let far = (w + h) as f32;
    let mut d: Vec<f32> = flagged
        .iter()
        .map(|on| if *on { 0.0 } else { far })
        .collect();
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let mut best = d[i];
            if x > 0 {
                best = best.min(d[i - 1] + 1.0);
            }
            if y > 0 {
                best = best.min(d[i - w] + 1.0);
                if x > 0 {
                    best = best.min(d[i - w - 1] + DIAGONAL);
                }
                if x + 1 < w {
                    best = best.min(d[i - w + 1] + DIAGONAL);
                }
            }
            d[i] = best;
        }
    }
    for y in (0..h).rev() {
        for x in (0..w).rev() {
            let i = y * w + x;
            let mut best = d[i];
            if x + 1 < w {
                best = best.min(d[i + 1] + 1.0);
            }
            if y + 1 < h {
                best = best.min(d[i + w] + 1.0);
                if x + 1 < w {
                    best = best.min(d[i + w + 1] + DIAGONAL);
                }
                if x > 0 {
                    best = best.min(d[i + w - 1] + DIAGONAL);
                }
            }
            d[i] = best;
        }
    }
    d
}

/// 0 below `low`, 1 above `high`, smooth in between.
pub(crate) fn smoothstep(low: f32, high: f32, v: f32) -> f32 {
    if high <= low {
        return if v >= high { 1.0 } else { 0.0 };
    }
    let t = ((v - low) / (high - low)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_running_blur_matches_the_reference_blur_and_keeps_a_constant() {
        let (w, h) = (37, 23);
        let plane: Vec<f32> = (0..w * h)
            .map(|i| ((i * 7919) % 101) as f32 / 101.0)
            .collect();
        for radius in [1, 3, 9] {
            let fast = blur(&plane, w, h, radius);
            let reference = crate::bands::blur(&plane, w, h, radius);
            for (a, b) in fast.iter().zip(&reference) {
                assert!((a - b).abs() < 1e-5, "radius {radius}: {a} {b}");
            }
        }
        let flat = vec![0.37_f32; w * h];
        assert!(blur(&flat, w, h, 50)
            .iter()
            .all(|v| (v - 0.37).abs() < 1e-6));
    }

    #[test]
    fn a_weighted_mean_ignores_unweighted_cells() {
        let (w, h) = (40, 40);
        let mut values = vec![0.5_f32; w * h];
        let mut weights = vec![1.0_f32; w * h];
        for y in 15..25 {
            for x in 15..25 {
                values[y * w + x] = 0.0;
                weights[y * w + x] = 0.0;
            }
        }
        let mean = Weighted::new(&weights, w, h, 6).mean(&values);
        // The hole is filled from its surroundings, not dragged toward its own value.
        assert!(
            (mean[20 * w + 20] - 0.5).abs() < 1e-4,
            "{}",
            mean[20 * w + 20]
        );
    }

    #[test]
    fn groups_measure_a_line_as_long_and_a_disk_as_round() {
        let (w, h) = (64, 64);
        let mut flagged = vec![false; w * h];
        for x in 5..45 {
            flagged[10 * w + x] = true;
            flagged[11 * w + x] = true;
        }
        for y in 30..50 {
            for x in 30..50 {
                if (x as f32 - 40.0).hypot(y as f32 - 40.0) < 6.0 {
                    flagged[y * w + x] = true;
                }
            }
        }
        let found = groups(&flagged, w, h);
        assert_eq!(found.len(), 2);
        assert!(found[0].elongation() > 8.0, "{}", found[0].elongation());
        assert!(found[1].elongation() < 1.3, "{}", found[1].elongation());
        assert!((found[1].centre[0] - 40.0).abs() < 0.6);
    }

    #[test]
    fn distance_grows_away_from_a_flagged_cell() {
        let (w, h) = (21, 21);
        let mut flagged = vec![false; w * h];
        flagged[10 * w + 10] = true;
        let d = distance_to(&flagged, w, h);
        assert!(d[10 * w + 10].abs() < 1e-6);
        assert!((d[10 * w + 15] - 5.0).abs() < 1e-4);
        assert!((d[13 * w + 13] - 3.0 * std::f32::consts::SQRT_2).abs() < 1e-3);
        assert!(distance_to(&vec![false; w * h], w, h)[0] >= (w + h) as f32 - 0.5);
    }

    #[test]
    fn the_robust_spread_ignores_a_minority_of_outliers() {
        let mut samples: Vec<f32> = (0..1000)
            .map(|i| ((i % 21) as f32 - 10.0) * 0.001)
            .collect();
        for value in samples.iter_mut().take(200) {
            *value = 0.5;
        }
        let (median, spread) = centre_and_spread(&mut samples).unwrap();
        assert!(median.abs() < 0.004 && spread < 0.012, "{median} {spread}");
    }
}
