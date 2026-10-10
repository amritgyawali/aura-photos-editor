//! Plane arithmetic shared by frequency healing and the texture graft. ADR-0090.
//!
//! Both operations work on a rectangle cut out of the frame, and both need the same four
//! things: a blur whose cost does not grow with its radius, a blur that averages selected
//! skin only, connected groups of flagged pixels with their shape, and a distance to the
//! nearest flagged pixel. They live here so the two operations cannot disagree about any of
//! them.
// Every plane is `width * height` long and every coordinate is bounded before it is used.
#![allow(clippy::indexing_slicing)]

use rayon::prelude::*;

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

/// Three box passes: a close approximation of a Gaussian whose standard deviation is about
/// the radius. The one implementation is [`crate::bands::blur`]: a running sum per pass, so the
/// cost is the same at every radius, rows shared across cores, and the same result on every
/// machine.
pub(crate) fn blur(values: &[f32], w: usize, h: usize, radius: usize) -> Vec<f32> {
    if w == 0 || h == 0 || radius == 0 || values.len() != w * h {
        return values.to_vec();
    }
    crate::bands::blur(values, w, h, radius)
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

/// Bins a [`local_percentile`] resolves a plane's range into.
const PERCENTILE_BINS: usize = 384;

/// A robust local level and how much of its window it was measured from.
#[derive(Debug)]
pub(crate) struct Percentile {
    /// The value `share` of the counted cells in the window around each cell lie below; a
    /// cell whose window counted nothing keeps its own value.
    pub level: Vec<f32>,
    /// The share of each window that was counted, 0..=1.
    pub support: Vec<f32>,
}

/// The bin of every counted cell (`u16::MAX` for the rest), and the value range the bins span.
fn binned(values: &[f32], counted: &[bool]) -> Option<(Vec<u16>, f32, f32)> {
    let sampling = (values.len() / 60_000).max(1);
    let mut samples: Vec<f32> = values
        .iter()
        .zip(counted)
        .step_by(sampling)
        .filter(|(v, on)| **on && v.is_finite())
        .map(|(v, _)| *v)
        .collect();
    let lo = quantile(&mut samples, 0.001)?;
    let hi = quantile(&mut samples, 0.999)?;
    let span = (hi - lo).max(1e-6);
    let top = (PERCENTILE_BINS - 1) as f32;
    let bins = values
        .par_iter()
        .zip(counted.par_iter())
        .map(|(v, on)| {
            if *on && v.is_finite() {
                ((v - lo) / span * top).round().clamp(0.0, top) as u16
            } else {
                u16::MAX
            }
        })
        .collect();
    Some((bins, lo, span))
}

/// A regular grid of samples, `step` cells apart, with the last row and column on the edge.
#[derive(Debug, Clone, Copy)]
struct Grid {
    w: usize,
    h: usize,
    step: usize,
    cols: usize,
    rows: usize,
}

impl Grid {
    fn new(w: usize, h: usize, step: usize) -> Self {
        let step = step.max(1);
        Self {
            w,
            h,
            step,
            cols: (w - 1).div_ceil(step) + 1,
            rows: (h - 1).div_ceil(step) + 1,
        }
    }

    fn at(self, k: usize, size: usize) -> usize {
        (k * self.step).min(size - 1)
    }

    /// The two samples around `v` along an axis of `size` cells and `cells` samples, and how
    /// far toward the second it is.
    fn locate(self, v: usize, size: usize, cells: usize) -> (usize, usize, f32) {
        let k = (v / self.step).min(cells - 1);
        let next = (k + 1).min(cells - 1);
        let (p0, p1) = (self.at(k, size), self.at(next, size));
        let t = if p1 > p0 {
            (v - p0) as f32 / (p1 - p0) as f32
        } else {
            0.0
        };
        (k, next, t)
    }

    /// Bilinear interpolation of the grid's levels and supports to every cell. A cell whose
    /// four samples counted nothing keeps `own`.
    fn spread(self, levels: &[f32], supports: &[f32], own: &[f32]) -> Percentile {
        // Where every column falls between grid samples is the same on every row, so it is
        // worked out once; rows are independent and are written in place in parallel.
        let columns: Vec<(usize, usize, f32)> =
            (0..self.w).map(|x| self.locate(x, self.w, self.cols)).collect();
        let mut level = vec![0.0_f32; self.w * self.h];
        let mut support = vec![0.0_f32; self.w * self.h];
        level
            .par_chunks_mut(self.w)
            .zip(support.par_chunks_mut(self.w))
            .enumerate()
            .for_each(|(y, (level, support))| {
                let own = &own[y * self.w..(y + 1) * self.w];
                self.spread_row(y, &columns, levels, supports, own, level, support);
            });
        Percentile { level, support }
    }

    #[allow(clippy::too_many_arguments)]
    fn spread_row(
        self,
        y: usize,
        columns: &[(usize, usize, f32)],
        levels: &[f32],
        supports: &[f32],
        own: &[f32],
        level: &mut [f32],
        support: &mut [f32],
    ) {
        let (ky, ny, ty) = self.locate(y, self.h, self.rows);
        for (x, &(kx, nx, tx)) in columns.iter().enumerate() {
            let (mut sum, mut weight, mut held) = (0.0_f32, 0.0_f32, 0.0_f32);
            for (gy, wy) in [(ky, 1.0 - ty), (ny, ty)] {
                for (gx, wx) in [(kx, 1.0 - tx), (nx, tx)] {
                    let g = gy * self.cols + gx;
                    let k = wy * wx;
                    held += supports[g] * k;
                    if levels[g].is_finite() && k > 0.0 {
                        sum += levels[g] * k;
                        weight += k;
                    }
                }
            }
            level[x] = if weight > 1e-6 { sum / weight } else { own[x] };
            support[x] = held;
        }
    }
}

/// A local percentile of `values` over the `counted` cells of a square window of half-width
/// `radius` around every cell.
///
/// Unlike a mean, a percentile keeps an edge: next to a shadow it reports the side of the edge
/// the cell is on, while anything covering less than the share of the window that lies beyond
/// the percentile - a blemish - is ignored. It is measured on a grid of `step` cells by sliding
/// one histogram along each grid row, and interpolated bilinearly in between, so its cost
/// grows with the window's width rather than its area. Deterministic: integer counts, fixed
/// order.
pub(crate) fn local_percentile(
    values: &[f32],
    counted: &[bool],
    w: usize,
    h: usize,
    radius: usize,
    step: usize,
    share: f32,
) -> Percentile {
    let unchanged = || Percentile {
        level: values.to_vec(),
        support: vec![0.0; values.len()],
    };
    if w == 0 || h == 0 || values.len() != w * h || counted.len() != w * h {
        return unchanged();
    }
    let Some((bins, lo, span)) = binned(values, counted) else {
        return unchanged();
    };
    let grid = Grid::new(w, h, step);
    let share = share.clamp(0.0, 1.0);
    let (levels, supports) = cpu_grid(&bins, w, h, radius, grid, share, lo, span);
    grid.spread(&levels, &supports, values)
}

/// The percentile grid on the processor: each grid row slides its own histogram.
#[allow(clippy::too_many_arguments)]
fn cpu_grid(
    bins: &[u16],
    w: usize,
    h: usize,
    radius: usize,
    grid: Grid,
    share: f32,
    lo: f32,
    span: f32,
) -> (Vec<f32>, Vec<f32>) {
    let top = (PERCENTILE_BINS - 1) as f32;
    // Each grid row slides its own histogram, so the rows run in parallel and are joined in
    // order: the same answer on every machine.
    let rows: Vec<(Vec<f32>, Vec<f32>)> = (0..grid.rows)
        .into_par_iter()
        .map(|gy| {
            let mut levels = vec![f32::NAN; grid.cols];
            let mut supports = vec![0.0_f32; grid.cols];
            let mut hist = vec![0_u32; PERCENTILE_BINS];
            let y = grid.at(gy, h);
            let rows = y.saturating_sub(radius)..(y + radius + 1).min(h);
            hist.fill(0);
            let mut total = 0_u32;
            let mut columns = 0..0;
            for gx in 0..grid.cols {
                let x = grid.at(gx, w);
                let wanted = x.saturating_sub(radius)..(x + radius + 1).min(w);
                // Columns that left the window on the left, then columns that entered on the right.
                let leaving = (columns.start..wanted.start.min(columns.end)).map(|c| (c, false));
                let entering = (columns.end.max(wanted.start)..wanted.end).map(|c| (c, true));
                for (column, enter) in leaving.chain(entering) {
                    for row in rows.clone() {
                        let b = bins[row * w + column];
                        if b == u16::MAX {
                            continue;
                        }
                        if enter {
                            hist[usize::from(b)] += 1;
                            total += 1;
                        } else {
                            hist[usize::from(b)] -= 1;
                            total -= 1;
                        }
                    }
                }
                columns = wanted;
                let g = gx;
                supports[g] = total as f32 / (rows.len() * columns.len()) as f32;
                if total == 0 {
                    continue;
                }
                let target = (share * total as f32).max(0.5);
                let mut seen = 0_u32;
                for (bin, count) in hist.iter().enumerate() {
                    seen += count;
                    if seen as f32 >= target {
                        levels[g] = lo + bin as f32 / top * span;
                        break;
                    }
                }
            }
            (levels, supports)
        })
        .collect();
    let mut levels = Vec::with_capacity(grid.cols * grid.rows);
    let mut supports = Vec::with_capacity(grid.cols * grid.rows);
    for (l, s) in rows {
        levels.extend(l);
        supports.extend(s);
    }
    (levels, supports)
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
    fn a_local_percentile_keeps_an_edge_and_ignores_a_spot() {
        let (w, h) = (60, 40);
        // A step from 1.0 to 0.5 at x = 30, and a small dark spot on the bright side.
        let mut values: Vec<f32> = (0..w * h)
            .map(|i| if i % w < 30 { 1.0 } else { 0.5 })
            .collect();
        for y in 18..22 {
            for x in 10..14 {
                values[y * w + x] = 0.1;
            }
        }
        let counted = vec![true; w * h];
        let p = local_percentile(&values, &counted, w, h, 8, 2, 0.5);
        // Either side of the edge keeps its own level, three cells from it.
        assert!(
            (p.level[20 * w + 26] - 1.0).abs() < 0.01,
            "{}",
            p.level[20 * w + 26]
        );
        assert!(
            (p.level[20 * w + 34] - 0.5).abs() < 0.01,
            "{}",
            p.level[20 * w + 34]
        );
        // The spot is not its own level.
        assert!(
            (p.level[20 * w + 12] - 1.0).abs() < 0.01,
            "{}",
            p.level[20 * w + 12]
        );
        assert!((p.support[20 * w + 12] - 1.0).abs() < 1e-6);
        // Cells that are not counted are not read, and a window with none keeps its own value.
        let left_only: Vec<bool> = (0..w * h).map(|i| i % w < 30).collect();
        let q = local_percentile(&values, &left_only, w, h, 4, 2, 0.5);
        assert!((q.level[20 * w + 31] - 1.0).abs() < 0.01);
        assert!((q.level[20 * w + 50] - 0.5).abs() < 1e-6);
        assert!(q.support[20 * w + 50] < 1e-6);
    }

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
