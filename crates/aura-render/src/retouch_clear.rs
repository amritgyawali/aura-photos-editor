//! Frequency healing: find compact marks in a selection and rebuild the tone under them.
//!
//! A retoucher separates a face into a tone layer and a texture layer, paints clean skin
//! colour over each mark on the tone layer, and leaves the pores on the texture layer alone.
//! This is that, measured rather than painted:
//!
//! 1. **Find.** At four sizes, a pixel is compared with the selected skin around it - how
//!    much darker it is, and how much redder. Both are ratios, so nothing here depends on
//!    exposure or on what colour anybody's skin is. The thresholds are multiples of the
//!    selection's own robust spread, so rough skin, noise and gentle shading do not read as
//!    marks. Only **compact** groups survive: a crease, a strand of hair or the edge of a
//!    shadow is long and thin, and a long thin group also bars the pieces of itself that a
//!    stricter threshold would otherwise break off and call spots.
//! 2. **Rebuild.** The tone under each mark is replaced by a weighted mean of the unmarked
//!    skin around it, taken at the smallest size that has enough clean skin to average.
//! 3. **Keep.** Detail finer than the mark stays. Only relief far outside the selection's
//!    normal pore contrast - the lit rim of a raised spot - is limited, and by how much is
//!    the operation's `texture`.
//!
//! Nothing is generated: every value written is a mean of this photograph's own pixels.
//! ADR-0090.
// Every plane covers one bounded rectangle and every index is derived from it.
#![allow(clippy::indexing_slicing)]
use crate::retouch_mask::Coverage;
use crate::retouch_planes::{
    blur, centre_and_spread, distance_to, groups, luma, smoothstep, Group, Rect, Weighted,
};
use aura_recipe::retouch_tools::Edit;
use rayon::prelude::*;

/// Neighbourhood sizes, including small marks, in units of the frequency radius.
const SCALES: [f32; 4] = [0.65, 1.0, 2.0, 3.2];
/// Stricter thresholds separate marks that touch at the most sensitive one.
const TIERS: [f32; 3] = [1.0, 1.8, 2.8];
/// A group longer than this many times its width is a line, not a spot.
const MAX_ELONGATION: f32 = 5.5;
/// Directions a candidate's surroundings are read in.
const SECTORS: usize = 8;
/// How many samples the robust spreads are measured from.
const SPREAD_SAMPLES: usize = 60_000;
/// Find-and-rebuild passes one operation makes.
const PASSES: usize = 2;
pub(crate) const VERSION: &str = "enclosed-light-v2";
/// The frequency radius, in samples, that marks are found and tone is rebuilt at.
///
/// Everything that finds a mark and rebuilds the tone under it is a neighbourhood mean at
/// 0.3 to 24 frequency radii, so it describes the skin at the scale of a mark, never a pore.
/// At full resolution a large face has a radius of eighteen pixels or more, and those means
/// are then measured on four to sixteen times as many samples as the scale they describe needs.
/// From twice this radius the selection is reduced by a whole factor until it is about this
/// size, the marks are found and the tone rebuilt there, and the rebuilt tone and the marks'
/// feathered weight are brought back to full resolution. A mark's own tone, the pores put back
/// over the rebuilt tone and the normal relief they are limited by are always measured at full
/// resolution. Below the threshold - most faces on a proxy - nothing changes.
/// `a_large_face_healed_on_the_working_grid_matches_full_resolution` holds the two to each
/// other at four radii.
const WORK_RADIUS: f32 = 4.0;
/// The largest reduction: four is still more than a sample per third of a radius.
const MAX_REDUCTION: usize = 4;
/// Under a mark, relief beyond this many robust spreads of the selection's ordinary pore
/// contrast belongs to the mark - its dark core, its lit rim - rather than to the skin.
const RELIEF_LIMIT: f32 = 1.5;
/// How much of a region's glint strength is added to its darkness threshold.
const GLINT_ALLOWANCE: f32 = 2.0;
/// Luminance above this multiple of the local mean is a glint, not the skin's own tone.
const GLINT_CLIP: f32 = 1.06;
/// Red below this share of the surrounding skin's red is pigment, not inflammation.
/// ADR-0118.
const MOLE: f32 = 0.5;
/// The skin a cell's red is compared with, in frequency radii: about a quarter of the
/// distance between a face's eyes, several times the width of a mole.
const MOLE_SURROUND: f32 = 12.0;
/// Selection coverage at or above this counts as fully selected. A selection's soft edge says
/// where marks are looked for, not how much of one is rebuilt: half a mark is still a mark,
/// and a spot beside a brow or an eye sits in the feather that keeps smoothing off them.
const FULLY_SELECTED: f32 = 0.25;

/// Packing pays only when the packed plane is at most this share of the whole rectangle.
const PACK_WHEN_BELOW: f32 = 0.8;
/// Packed pieces start on multiples of this, so a cell of the working grid never straddles two.
const PACK_ALIGN: usize = MAX_REDUCTION;
/// A cell selected more faintly than this would move by less than one level in 255 if it were
/// rebuilt in full, so it does not join two parts of a selection. A matte's edge refined
/// against the photograph leaves such a haze for a few dozen pixels around the skin, and
/// otherwise every arm would join its neighbour.
const FAINTEST: f32 = 1.0 / 255.0;

/// One part of the frame placed in the plane a pass works on: `frame`'s cells, row for row,
/// starting at column `x` and row `y` of the plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Piece {
    frame: Rect,
    x: usize,
    y: usize,
}

/// The plane a pass works on, and where its cells come from in the frame.
///
/// A face is one patch of skin, and the rectangle around it is mostly that skin. A body is
/// not: a neck, two bare arms and two lower legs inside a body rectangle of five megapixels or
/// more are a sixth of it, and every blur a pass makes - sixty or so - would otherwise run over
/// the dress between them. Every quantity a pass measures is either a mean weighted by the
/// selection, to which an unselected cell adds nothing, or read within fourteen frequency
/// radii of a selected cell. So the separately selected parts of a selection, each with that
/// margin of the frame around it, are packed into one compact plane with an unselected gap
/// between them, and the unchanged pass runs on that. The thresholds are still measured over
/// the whole selection at once, so a hand and a neck are held to the same skin.
/// `separate_patches_heal_as_they_do_in_the_whole_rectangle` holds the two to each other.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Layout {
    w: usize,
    h: usize,
    pieces: Vec<Piece>,
    /// The selection's whole rectangle with its margin. The selection's spreads are sampled
    /// at the cells of it they would be sampled at if it were the plane; see
    /// [`Layout::sampled`].
    whole: Rect,
}

impl Layout {
    /// The selection's whole rectangle with its margin: the plane every pass used before.
    fn whole(edit: &Edit, coverage: &Coverage, w: usize, h: usize) -> Option<Self> {
        let frame = Rect::around(coverage.bounds, margin(edit, w, h), w, h)?;
        Some(Self {
            w: frame.w,
            h: frame.h,
            pieces: vec![Piece { frame, x: 0, y: 0 }],
            whole: frame,
        })
    }

    /// The selected parts only, packed, when that is clearly smaller than [`Layout::whole`].
    /// `None` when nothing is selected.
    fn new(edit: &Edit, coverage: &Coverage, w: usize, h: usize) -> Option<Self> {
        let whole = Self::whole(edit, coverage, w, h)?;
        let margin = margin(edit, w, h);
        let parts = islands(coverage, w, h, margin);
        if parts.is_empty() {
            return None;
        }
        let packed = pack(&parts, margin, whole.whole);
        let share = (packed.w * packed.h) as f32 / (whole.w * whole.h).max(1) as f32;
        Some(if share < PACK_WHEN_BELOW {
            packed
        } else {
            whole
        })
    }

    fn len(&self) -> usize {
        self.w * self.h
    }

    /// Which cells of this plane, reduced by `factor`, the selection's spreads are sampled at.
    ///
    /// A spread is a median over about [`SPREAD_SAMPLES`] cells, taken at every n-th cell of
    /// the whole rectangle (reduced by the same factor). A packed plane is sampled at the very
    /// same cells of the frame, so packing changes where the work is done and not what the
    /// thresholds are measured from - a different set of samples moves a threshold measured on
    /// a few thousand of them by more than packing moves anything else.
    fn sampled(&self, factor: usize) -> Vec<bool> {
        let (cw, ch) = (self.w.div_ceil(factor), self.h.div_ceil(factor));
        let whole_w = self.whole.w.div_ceil(factor);
        let step = (whole_w * self.whole.h.div_ceil(factor) / SPREAD_SAMPLES).max(1);
        let mut out = vec![false; cw * ch];
        out.par_chunks_mut(cw.max(1))
            .enumerate()
            .for_each(|(cy, line)| {
                let y = cy * factor;
                for piece in &self.pieces {
                    if y < piece.y || y >= piece.y + piece.frame.h {
                        continue;
                    }
                    let fy = piece.frame.y0 + y - piece.y;
                    for (cx, slot) in line.iter_mut().enumerate() {
                        let x = cx * factor;
                        if x < piece.x || x >= piece.x + piece.frame.w {
                            continue;
                        }
                        let fx = piece.frame.x0 + x - piece.x;
                        let at =
                            (fy - self.whole.y0) / factor * whole_w + (fx - self.whole.x0) / factor;
                        *slot = at.is_multiple_of(step);
                    }
                }
            });
        out
    }
}

/// How far around a selected cell a pass reads the frame: the widest ring a candidate's
/// surroundings are read in is about 13.5 frequency radii.
fn margin(edit: &Edit, w: usize, h: usize) -> usize {
    px(radius_of(edit, w, h), 14.0) + 2
}

fn radius_of(edit: &Edit, w: usize, h: usize) -> f32 {
    (edit.radius * w.min(h) as f32).max(1.0)
}

/// The separately selected parts of a selection, each grown by `margin` and clipped to the
/// frame, merged until no two overlap. Found on a grid of `margin`-sized cells, each of which
/// remembers the exact extent of the selected pixels inside it. In scan order. A cell selected
/// below [`FAINTEST`] that no part's margin reaches is left as it is.
fn islands(coverage: &Coverage, w: usize, h: usize, margin: usize) -> Vec<Rect> {
    let [x0, y0, x1, y1] = coverage.bounds;
    let (x1, y1) = (x1.min(w), y1.min(h));
    if x1 <= x0 || y1 <= y0 {
        return Vec::new();
    }
    let g = margin.max(8);
    let (gw, gh) = ((x1 - x0).div_ceil(g), (y1 - y0).div_ceil(g));
    // Each grid row is scanned on its own, so the rows are shared across cores.
    let cells: Vec<Option<[usize; 4]>> = (0..gh)
        .into_par_iter()
        .flat_map_iter(|gy| {
            let mut row: Vec<Option<[usize; 4]>> = vec![None; gw];
            for y in y0 + gy * g..(y0 + (gy + 1) * g).min(y1) {
                for x in x0..x1 {
                    if smoothstep(0.0, FULLY_SELECTED, coverage.at(x, y, w, h)) >= FAINTEST {
                        let cell = &mut row[(x - x0) / g];
                        *cell = Some(cell.map_or([x, y, x + 1, y + 1], |[l, t, r, b]| {
                            [l.min(x), t.min(y), r.max(x + 1), b.max(y + 1)]
                        }));
                    }
                }
            }
            row
        })
        .collect();
    let mut seen = vec![false; cells.len()];
    let mut rects: Vec<Rect> = Vec::new();
    for start in 0..cells.len() {
        if seen[start] || cells[start].is_none() {
            continue;
        }
        seen[start] = true;
        let mut stack = vec![start];
        let mut bounds = [usize::MAX, usize::MAX, 0, 0];
        while let Some(i) = stack.pop() {
            if let Some([l, t, r, b]) = cells[i] {
                bounds = [
                    bounds[0].min(l),
                    bounds[1].min(t),
                    bounds[2].max(r),
                    bounds[3].max(b),
                ];
            }
            let (cx, cy) = (i % gw, i / gw);
            for ny in cy.saturating_sub(1)..=(cy + 1).min(gh - 1) {
                for nx in cx.saturating_sub(1)..=(cx + 1).min(gw - 1) {
                    let j = ny * gw + nx;
                    if !seen[j] && cells[j].is_some() {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
        }
        if let Some(rect) = Rect::around(bounds, margin, w, h) {
            rects.push(rect);
        }
    }
    // Two parts whose margins overlap read each other's pixels: they are one part.
    let overlap = |a: &Rect, b: &Rect| {
        a.x0 < b.x0 + b.w && b.x0 < a.x0 + a.w && a.y0 < b.y0 + b.h && b.y0 < a.y0 + a.h
    };
    let mut merged = true;
    while merged {
        merged = false;
        'outer: for i in 0..rects.len() {
            for j in i + 1..rects.len() {
                if overlap(&rects[i], &rects[j]) {
                    let (a, b) = (rects[i], rects[j]);
                    let (l, t) = (a.x0.min(b.x0), a.y0.min(b.y0));
                    let (r, bottom) = ((a.x0 + a.w).max(b.x0 + b.w), (a.y0 + a.h).max(b.y0 + b.h));
                    rects[i] = Rect {
                        x0: l,
                        y0: t,
                        w: r - l,
                        h: bottom - t,
                    };
                    rects.remove(j);
                    merged = true;
                    break 'outer;
                }
            }
        }
    }
    rects.sort_by_key(|r| (r.y0, r.x0));
    rects
}

/// The smallest of the shelf packings [`shelves`] makes at a few widths: as wide as the
/// widest part, as wide as a square of the same area, and as wide as one to all of the parts
/// side by side divided into that many shelves.
/// `whole` is the selection's whole rectangle, which the layout keeps for sampling.
fn pack(parts: &[Rect], gap: usize, whole: Rect) -> Layout {
    let area: usize = parts.iter().map(|r| (r.w + gap) * (r.h + gap)).sum();
    let widest = parts.iter().map(|r| r.w).max().unwrap_or(0);
    let row: usize = parts.iter().map(|r| r.w + gap + PACK_ALIGN).sum();
    let mut widths = vec![(area as f64).sqrt().ceil() as usize];
    widths.extend((1..=parts.len()).map(|k| row.div_ceil(k)));
    widths
        .into_iter()
        .fold(shelves(parts, gap, widest, whole), |best, target| {
            let next = shelves(parts, gap, target.max(widest), whole);
            if (next.len(), next.w) < (best.len(), best.w) {
                next
            } else {
                best
            }
        })
}

/// Shelves of parts no wider than `target`, tallest first, left to right, `gap` unselected
/// cells apart.
fn shelves(parts: &[Rect], gap: usize, target: usize, whole: Rect) -> Layout {
    let align = |v: usize| v.div_ceil(PACK_ALIGN) * PACK_ALIGN;
    let mut order: Vec<&Rect> = parts.iter().collect();
    order.sort_by_key(|r| (std::cmp::Reverse(r.h), r.y0, r.x0));
    let (mut x, mut y, mut shelf, mut width) = (0, 0, 0, 0);
    let mut pieces = Vec::with_capacity(parts.len());
    for frame in order {
        if x > 0 && x + frame.w > target {
            y = align(y + shelf + gap);
            x = 0;
            shelf = 0;
        }
        pieces.push(Piece {
            frame: *frame,
            x,
            y,
        });
        width = width.max(x + frame.w);
        shelf = shelf.max(frame.h);
        x = align(x + frame.w + gap);
    }
    Layout {
        w: width,
        h: y + shelf,
        pieces,
        whole,
    }
}

/// What the operation works on: the plane, its pixels and the selection inside it.
struct Field {
    /// The plane's size; `x0` and `y0` are zero. [`Layout`] says where its cells come from.
    rect: Rect,
    /// Selection coverage, 0..=1, with the outer part of a soft edge counted as selected.
    alpha: Vec<f32>,
    channels: [Vec<f32>; 3],
    luma: Vec<f32>,
    /// Red share relative to the other two channels; exposure-independent.
    red: Vec<f32>,
    /// The frequency radius in pixels.
    radius: f32,
    /// The cells the selection's spreads are sampled at; see [`Layout::sampled`].
    sampled: Vec<bool>,
}

fn px(radius: f32, factor: f32) -> usize {
    (radius * factor).round().max(1.0) as usize
}

impl Field {
    fn new(
        rgb: &[f32],
        w: usize,
        h: usize,
        edit: &Edit,
        coverage: &Coverage,
        layout: &Layout,
    ) -> Option<Self> {
        let radius = radius_of(edit, w, h);
        let rect = Rect {
            x0: 0,
            y0: 0,
            w: layout.w,
            h: layout.h,
        };
        // Every row is read on its own, so the rows are shared across cores. A cell no piece
        // covers is an unselected gap: nothing weighted by the selection reads it.
        let mut cells = vec![(0.0_f32, [0.0_f32; 3]); layout.len()];
        cells
            .par_chunks_mut(layout.w.max(1))
            .enumerate()
            .for_each(|(row, line)| {
                for piece in &layout.pieces {
                    if row < piece.y || row >= piece.y + piece.frame.h {
                        continue;
                    }
                    let y = piece.frame.y0 + row - piece.y;
                    for column in 0..piece.frame.w {
                        let x = piece.frame.x0 + column;
                        let i = (y * w + x) * 3;
                        line[piece.x + column] = (
                            smoothstep(0.0, FULLY_SELECTED, coverage.at(x, y, w, h)),
                            [rgb[i], rgb[i + 1], rgb[i + 2]],
                        );
                    }
                }
            });
        let alpha: Vec<f32> = cells.par_iter().map(|(a, _)| *a).collect();
        let channels: [Vec<f32>; 3] =
            std::array::from_fn(|c| cells.par_iter().map(|(_, p)| p[c]).collect());
        let lum: Vec<f32> = cells.par_iter().map(|(_, p)| luma(*p)).collect();
        let red: Vec<f32> = cells
            .par_iter()
            .map(|(_, p)| {
                let sum = p[0].max(0.0) + p[1].max(0.0) + p[2].max(0.0);
                (p[0] - (p[1] + p[2]) * 0.5) / (sum + 1e-4)
            })
            .collect();
        alpha.iter().any(|a| *a > 0.0).then_some(Self {
            rect,
            alpha,
            channels,
            luma: lum,
            red,
            radius,
            sampled: layout.sampled(1),
        })
    }
}

/// How many samples, on each side, one cell of the working grid stands for.
fn reduction(radius: f32) -> usize {
    ((radius / WORK_RADIUS).floor() as usize).clamp(1, MAX_REDUCTION)
}

impl Field {
    /// The same selection on a grid `factor` times coarser: every cell the mean of the samples
    /// it covers, colour channels first and the readings derived from them, exactly as
    /// [`Field::new`] derives them from a pixel.
    fn reduced(&self, factor: usize, layout: &Layout) -> Self {
        let Rect { w, h, .. } = self.rect;
        let (cw, ch) = (w.div_ceil(factor), h.div_ceil(factor));
        let mean = |plane: &[f32]| -> Vec<f32> {
            (0..cw * ch)
                .into_par_iter()
                .map(|cell| {
                    let (cx, cy) = (cell % cw, cell / cw);
                    let (x0, y0) = (cx * factor, cy * factor);
                    let (x1, y1) = ((x0 + factor).min(w), (y0 + factor).min(h));
                    let mut sum = 0.0_f32;
                    for y in y0..y1 {
                        for x in x0..x1 {
                            sum += plane[y * w + x];
                        }
                    }
                    sum / ((x1 - x0) * (y1 - y0)) as f32
                })
                .collect()
        };
        let channels: [Vec<f32>; 3] = std::array::from_fn(|c| mean(&self.channels[c]));
        let lum = (0..cw * ch)
            .map(|i| luma([channels[0][i], channels[1][i], channels[2][i]]))
            .collect();
        let red = (0..cw * ch)
            .map(|i| {
                let p = [channels[0][i], channels[1][i], channels[2][i]];
                let sum = p[0].max(0.0) + p[1].max(0.0) + p[2].max(0.0);
                (p[0] - (p[1] + p[2]) * 0.5) / (sum + 1e-4)
            })
            .collect();
        Self {
            rect: Rect {
                x0: 0,
                y0: 0,
                w: cw,
                h: ch,
            },
            alpha: mean(&self.alpha),
            channels,
            luma: lum,
            red,
            radius: self.radius / factor as f32,
            sampled: layout.sampled(factor),
        }
    }
}

/// A smooth plane measured on the working grid, brought back to the selection's own samples
/// bilinearly between cell centres.
fn enlarge(plane: &[f32], [cw, ch]: [usize; 2], factor: usize, [w, h]: [usize; 2]) -> Vec<f32> {
    let at = |x: usize, y: usize| plane[y.min(ch - 1) * cw + x.min(cw - 1)];
    let place = |i: usize, cells: usize| -> (usize, usize, f32) {
        let u = ((i as f32 + 0.5) / factor as f32 - 0.5).clamp(0.0, (cells - 1) as f32);
        let lo = u.floor() as usize;
        (lo, (lo + 1).min(cells - 1), u - lo as f32)
    };
    let columns: Vec<(usize, usize, f32)> = (0..w).map(|x| place(x, cw)).collect();
    (0..w * h)
        .into_par_iter()
        .map(|i| {
            let (y0, y1, ty) = place(i / w, ch);
            let (x0, x1, tx) = columns[i % w];
            let top = at(x0, y0) * (1.0 - tx) + at(x1, y0) * tx;
            let bottom = at(x0, y1) * (1.0 - tx) + at(x1, y1) * tx;
            top * (1.0 - ty) + bottom * ty
        })
        .collect()
}

/// Median luminance of well-selected cells, the scale the darkness ratio is softened by.
fn typical_luma(field: &Field) -> f32 {
    let mut samples = well_selected(&field.luma, &field.alpha, &field.sampled);
    crate::retouch_planes::quantile(&mut samples, 0.5).unwrap_or(0.18)
}

/// Robust spread of `values` over well-selected cells.
fn spread(values: &[f32], alpha: &[f32], sampled: &[bool]) -> f32 {
    let mut samples = well_selected(values, alpha, sampled);
    centre_and_spread(&mut samples).map_or(0.0, |(_, s)| s)
}

/// `values` at the sampled cells whose weight in `alpha` is over a half.
fn well_selected(values: &[f32], alpha: &[f32], sampled: &[bool]) -> Vec<f32> {
    values
        .iter()
        .zip(alpha)
        .zip(sampled)
        .filter(|((_, a), on)| **on && **a > 0.5)
        .map(|((v, _), _)| *v)
        .collect()
}

/// The skin around a candidate, read in eight directions.
struct Surround<'a> {
    luma: &'a [f32],
    red: &'a [f32],
    alpha: &'a [f32],
    w: usize,
    h: usize,
    radius: f32,
}

impl Surround<'_> {
    /// Whether the group is a mark: darker - or redder - than the selected skin on every side
    /// of it.
    ///
    /// A blemish sits in skin. The pocket of shadow beside the inner corner of an eye, the
    /// side of a nose or the edge of a jaw is darker than the skin on one side and no darker
    /// than the skin on the other, and rebuilding it from the bright side paints a blotch.
    /// A ring just outside the group is read in eight sectors; at least six must be skin, and
    /// all but one of those must be clearly brighter (or less red) than the group.
    fn encloses(&self, group: &Group, deepest: f32, reddest: f32) -> bool {
        let Self { w, h, radius, .. } = *self;
        let area = group.cells.len() as f32;
        let own_luma = group.cells.iter().map(|i| self.luma[*i]).sum::<f32>() / area;
        let own_red = group.cells.iter().map(|i| self.red[*i]).sum::<f32>() / area;
        let mut valid = 0;
        let mut darker_than = 0;
        let mut redder_than = 0;
        let mut darkest_ring = f32::INFINITY;
        let mut brightest_ring = 0.0_f32;
        for sector in 0..SECTORS {
            let (mut sum_luma, mut sum_red, mut count) = (0.0_f32, 0.0_f32, 0.0_f32);
            let (mut context_luma, mut context_count) = (0.0_f32, 0.0_f32);
            for step in 0..3 {
                let distance = group.reach + radius * (0.75 + 0.6 * step as f32);
                for offset in [-0.3_f32, 0.0, 0.3] {
                    let angle =
                        (sector as f32 + 0.5 + offset) / SECTORS as f32 * std::f32::consts::TAU;
                    let x = (group.centre[0] + angle.cos() * distance).round();
                    let y = (group.centre[1] + angle.sin() * distance).round();
                    if x < 0.0 || y < 0.0 || x >= w as f32 || y >= h as f32 {
                        continue;
                    }
                    let i = y as usize * w + x as usize;
                    // Excluded facial shadows are still lighting evidence.
                    // Reading only selected skin biases a boundary toward its
                    // bright side and can turn a contour into a false blemish.
                    context_luma += self.luma[i];
                    context_count += 1.0;
                    if self.alpha[i] > 0.5 {
                        sum_luma += self.luma[i];
                        sum_red += self.red[i];
                        count += 1.0;
                    }
                }
            }
            if context_count >= 3.0 {
                let light = context_luma / context_count;
                darkest_ring = darkest_ring.min(light);
                brightest_ring = brightest_ring.max(light);
            }
            if count < 3.0 {
                continue;
            }
            valid += 1;
            let ring_luma = sum_luma / count;
            if (ring_luma - own_luma) / ring_luma.max(1e-6) > deepest * 0.3 {
                darker_than += 1;
            }
            if own_red - sum_red / count > reddest * 0.3 {
                redder_than += 1;
            }
        }

        // Coloured directional light can satisfy the redness test along a nose
        // shadow even though it is not a surrounded mark. Never rebuild a
        // candidate from incompatible light on opposite sides, or from a ring
        // substantially darker than the candidate itself. Leaving an ambiguous
        // mark is preferable to flattening a facial contour.
        // Even mild opposing gradients can be a cheek/nose contour. A mixed
        // ring whose dark side is below the candidate must not be rebuilt from
        // its bright side. A genuinely bright inflamed head is a separate case:
        // it sits above every side of an otherwise consistently lit ring.
        let surrounded_tone = darkest_ring >= own_luma * 0.98;
        let bright_head = own_luma >= brightest_ring * 1.02;
        let consistent_light =
            brightest_ring <= darkest_ring * 1.35 && (surrounded_tone || bright_head);
        valid >= SECTORS - 2
            && consistent_light
            && (darker_than >= valid - 1 || redder_than >= valid - 1)
    }
}

/// Cells on or right beside a mole or a beauty mark.
///
/// Redness cannot tell a mole from a spot: a brown mole has a larger share of red than the
/// skin around it, exactly as an inflamed spot has. What differs is the red channel itself.
/// Blood absorbs green and blue and passes red, so inflammation, however angry, leaves the
/// red channel nearly where it was. Melanin absorbs everything, and takes red down with the
/// rest.
///
/// Dark in red is still not enough: the mark an old spot leaves on dark skin is as dark as a
/// mole. It is also diffuse, fading into the skin over several frequency radii, where a mole
/// has an edge. So a mole is a connected patch whose red is under half that of the selected
/// skin around it, with a step of `edge` at its border. With `edge` at zero every such patch
/// counts, which is how a body's navel is kept.
fn near_a_mole(field: &Field, edge: f32) -> Vec<bool> {
    let Rect { w, h, .. } = field.rect;
    let r = field.radius;
    let red = &field.channels[0];
    // Each cell's own red is read as it is. Averaged with its neighbours, a mole three cells
    // wide or the hem of a top in a selection's soft edge is outvoted by the skin beside it.
    // The skin it is compared with is wide enough that the mole is a small part of it.
    let around = Weighted::new(&field.alpha, w, h, px(r, MOLE_SURROUND)).mean(red);
    let dark: Vec<bool> = red
        .iter()
        .zip(&around)
        .zip(&field.alpha)
        .map(|((own, skin), a)| *a > 0.0 && *own < skin * MOLE)
        .collect();
    // The largest step from a cell to a neighbour, as a share of the skin's red and carried
    // across one frequency radius.
    let step = |i: usize| -> f32 {
        let (x, y) = (i % w, i / w);
        let beside = [
            (x > 0).then(|| i - 1),
            (x + 1 < w).then(|| i + 1),
            (y > 0).then(|| i - w),
            (y + 1 < h).then(|| i + w),
        ];
        beside
            .into_iter()
            .flatten()
            .map(|n| (red[i] - red[n]).abs())
            .fold(0.0, f32::max)
            / around[i].max(1e-6)
            * r
    };
    let mut moles = vec![false; field.rect.len()];
    for group in groups(&dark, w, h) {
        if edge <= 0.0 || group.cells.iter().any(|i| step(*i) >= edge) {
            for i in group.cells {
                moles[i] = true;
            }
        }
    }
    // A mark that touches one is its edge, not a blemish beside it.
    distance_to(&moles, w, h)
        .iter()
        .map(|d| *d <= r * 0.5 + 1.0)
        .collect()
}

/// What one pass found.
struct Marks {
    /// 0..=1 for every cell: 1 over a mark and a margin of healthy skin around it, then a
    /// feathered rim.
    weight: Vec<f32>,
    /// The cells of the marks themselves, before the margin.
    core: Vec<bool>,
}

/// Cells that belong to a compact mark. `None` when there are none.
#[allow(clippy::too_many_lines)]
fn marks(
    field: &Field,
    sensitivity: f32,
    keep_dark: bool,
    mole_edge: Option<f32>,
) -> Option<Marks> {
    let Rect { w, h, .. } = field.rect;
    let n = field.rect.len();
    let r = field.radius;
    let midpoint = typical_luma(field);
    let floor = midpoint * 0.03;
    // Pore-scale smoothing first, so a single bright or dark pore is never a mark.
    let fine = Weighted::new(&field.alpha, w, h, px(r, 0.3));
    let fine_luma = fine.mean(&field.luma);
    let fine_red = fine.mean(&field.red);
    // Multiples of the selection's own spread: 3.0 at the default, 1.8 at most sensitive.
    let k = 4.2 - 2.4 * sensitivity.clamp(0.0, 1.0);
    let selected: Vec<bool> = field.alpha.iter().map(|a| *a > 0.2).collect();
    let moles = mole_edge.map(|edge| near_a_mole(field, edge));
    let mut marked = vec![false; n];
    // Cells of long thin structures; a stricter threshold must not split them into spots.
    let mut lines = vec![false; n];
    // How strongly a region glints is read over the same neighbourhood at every scale.
    let glint_reach = Weighted::new(&field.alpha, w, h, px(r, 6.0));
    for scale in SCALES {
        let around = Weighted::new(&field.alpha, w, h, px(r, scale));
        // Glints lift a plain mean, and the ordinary skin between them then measures as
        // dark. Clip them just above the first estimate and average again.
        let first = around.mean(&field.luma);
        let calm: Vec<f32> = field
            .luma
            .par_iter()
            .zip(&first)
            .map(|(v, mean)| v.min(mean * GLINT_CLIP))
            .collect();
        let base_luma = around.mean(&calm);
        let base_red = around.mean(&field.red);
        let redder: Vec<f32> = fine_red
            .par_iter()
            .zip(&base_red)
            .map(|(a, b)| a - b)
            .collect();
        let dark: Vec<f32> = base_luma
            .par_iter()
            .zip(&fine_luma)
            .zip(&redder)
            .map(|((base, fine), red)| {
                crate::retouch_analysis::darkness(*base, *fine, *red, midpoint, floor)
            })
            .collect();
        let dark_limit = (spread(&dark, &field.alpha, &field.sampled) * k).max(0.03);
        let red_limit = (spread(&redder, &field.alpha, &field.sampled) * k).max(0.006);
        // Where skin glints, the skin between the glints is darker than they are without
        // being a mark. How strongly a region glints raises what counts as dark there.
        let glint: Vec<f32> = dark.par_iter().map(|d| (-d).max(0.0).powi(2)).collect();
        let glinting = glint_reach.mean(&glint);
        // In units of the threshold. A bright head counts only as far as it is also redder
        // than its surroundings, so a specular pore never spends a repair.
        let score: Vec<f32> = dark
            .par_iter()
            .zip(&redder)
            .zip(&glinting)
            .map(|((d, red), glinting)| {
                let limit = dark_limit + GLINT_ALLOWANCE * glinting.max(0.0).sqrt();
                let red = red.max(0.0) / red_limit;
                let darker = d.max(0.0) / limit;
                let brighter = (-d).max(0.0) / limit;
                darker + brighter.min(red) + red
            })
            .collect();
        let smallest = (0.36 * (r * scale.min(1.0)).powi(2)).max(3.0);
        let largest = 18.0 * scale * scale * r * r;
        for tier in TIERS {
            let flagged: Vec<bool> = score
                .iter()
                .zip(&selected)
                .map(|(s, on)| *on && *s > tier)
                .collect();
            for group in groups(&flagged, w, h) {
                let area = group.cells.len() as f32;
                let thin = group.axes[1] < r * 0.5;
                if group.elongation() > MAX_ELONGATION && thin && group.reach > r * 2.5 {
                    for &i in &group.cells {
                        lines[i] = true;
                    }
                    continue;
                }
                let at = (group.centre[1].round() as usize).min(h - 1) * w
                    + (group.centre[0].round() as usize).min(w - 1);
                if area < smallest
                    || area > largest
                    || group.elongation() > MAX_ELONGATION
                    || group.reach > 3.6 * scale * r
                    || lines[at]
                {
                    continue;
                }
                let peak = group.cells.iter().map(|i| score[*i]).fold(0.0, f32::max);
                let reddest = group
                    .cells
                    .iter()
                    .map(|i| redder[*i])
                    .fold(f32::MIN, f32::max)
                    / red_limit;
                // The dark ring a bright pore casts in the blurred estimate is not a mark.
                let bright_pore = fine_luma[at] > base_luma[at] && reddest < 0.3;
                // A mark no redder than its surroundings may be a mole or a freckle.
                let dark_only = keep_dark && reddest < 0.6;
                let mole = moles
                    .as_ref()
                    .is_some_and(|near| group.cells.iter().any(|i| near[*i]));
                if peak < tier * 1.25 || bright_pore || dark_only || mole {
                    continue;
                }
                let deepest = group.cells.iter().map(|i| dark[*i]).fold(0.0, f32::max);
                let reddest_raw = group.cells.iter().map(|i| redder[*i]).fold(0.0, f32::max);
                let shape = Surround {
                    // The selection-weighted estimate is undefined in excluded
                    // cells. Read the photograph itself for lighting context;
                    // each sector already averages several ring samples.
                    luma: &field.luma,
                    red: &fine_red,
                    alpha: &field.alpha,
                    w,
                    h,
                    radius: r,
                };
                if !shape.encloses(&group, deepest, reddest_raw) {
                    continue;
                }
                for &i in &group.cells {
                    marked[i] = true;
                }
            }
        }
    }
    if !marked.iter().any(|on| *on) {
        return None;
    }
    // The lesion sits inside the fully rebuilt core; the feather lies on healthy skin.
    let grow = r * 0.5 + 1.0;
    let feather = r * 0.9 + 1.0;
    let weight = distance_to(&marked, w, h)
        .iter()
        .zip(&field.alpha)
        .map(|(d, a)| (1.0 - smoothstep(grow, grow + feather, *d)) * smoothstep(0.0, 0.2, *a))
        .collect();
    Some(Marks {
        weight,
        core: marked,
    })
}

/// The tone under each mark, rebuilt from unmarked selected skin at the smallest
/// neighbourhood that has enough of it. One plane per channel.
fn rebuilt_tone(field: &Field, mark: &[f32]) -> [Vec<f32>; 3] {
    let Rect { w, h, .. } = field.rect;
    let clean: Vec<f32> = field
        .alpha
        .iter()
        .zip(mark)
        .map(|(a, m)| a * (1.0 - m) * (1.0 - m))
        .collect();
    let mut out = [
        vec![0.0_f32; field.rect.len()],
        vec![0.0_f32; field.rect.len()],
        vec![0.0_f32; field.rect.len()],
    ];
    let mut filled = vec![0.0_f32; field.rect.len()];
    for step in 0..5_i32 {
        let reach = Weighted::new(&clean, w, h, px(field.radius, 1.5 * 2.0_f32.powi(step)));
        // The widest neighbourhood takes whatever is left, however little clean skin it saw.
        let last = step == 4;
        let take: Vec<f32> = reach
            .support()
            .par_iter()
            .zip(&filled)
            .map(|(support, done)| {
                let trust = if last {
                    f32::from(u8::from(*support > 1e-4))
                } else {
                    smoothstep(0.08, 0.35, *support)
                };
                trust * (1.0 - done)
            })
            .collect();
        let means: Vec<Vec<f32>> = field.channels.par_iter().map(|c| reach.mean(c)).collect();
        for (plane, mean) in out.iter_mut().zip(means) {
            plane
                .par_iter_mut()
                .zip(mean)
                .zip(&take)
                .for_each(|((value, mean), share)| *value += mean * share);
        }
        filled
            .par_iter_mut()
            .zip(&take)
            .for_each(|(done, share)| *done += share);
    }
    // Cells nothing reached keep their own tone rather than a made-up one.
    let tone = Weighted::new(&field.alpha, w, h, px(field.radius, 1.0));
    let owns: Vec<Vec<f32>> = field.channels.par_iter().map(|c| tone.mean(c)).collect();
    for (plane, own) in out.iter_mut().zip(owns) {
        for ((value, own), done) in plane.iter_mut().zip(own).zip(&filled) {
            *value += own * (1.0 - done);
        }
    }
    out
}

/// Soft limit: the identity near zero, approaching `limit` for large values.
fn soft_limit(v: f32, limit: f32) -> f32 {
    if limit <= 0.0 {
        return 0.0;
    }
    limit * (v / limit).tanh()
}

/// One find-and-rebuild pass over the plane `layout` describes. Returns whether anything was
/// found. `touched`, when given, is a frame-sized plane that receives the largest weight each
/// pixel was rebuilt with. `reduce` lets a large selection be measured on the working grid;
/// see [`WORK_RADIUS`].
#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
fn pass(
    rgb: &mut [f32],
    w: usize,
    h: usize,
    edit: &Edit,
    coverage: &Coverage,
    layout: &Layout,
    mut touched: Option<&mut [f32]>,
    reduce: bool,
) -> bool {
    let Some(field) = Field::new(rgb, w, h, edit, coverage, layout) else {
        return false;
    };
    let rect = field.rect;
    // Marks are found, and the tone under them rebuilt, on the working grid; see WORK_RADIUS.
    let factor = if reduce { reduction(field.radius) } else { 1 };
    let small = (factor > 1).then(|| field.reduced(factor, layout));
    let work = small.as_ref().unwrap_or(&field);
    let Some(Marks { weight: mark, core }) = marks(
        work,
        edit.sensitivity.unwrap_or(0.5),
        edit.keep_dark_marks,
        edit.mole_edge,
    ) else {
        return false;
    };
    let rebuilt = rebuilt_tone(work, &mark);
    // The marks' own cells, on the selection's samples: the tone a mark is separated from its
    // pores by is measured at full resolution, so a spot's sharp edge stays with the spot.
    let core: Vec<bool> = if factor > 1 {
        let cw = work.rect.w;
        (0..rect.len())
            .map(|i| core[(i / rect.w / factor) * cw + (i % rect.w) / factor])
            .collect()
    } else {
        core
    };
    // A mark's own tone is measured over the mark, and the skin's over the skin. Averaged
    // together, a small mark's tone is mostly the skin around it, and the mark itself is
    // then left over as "detail" that survives its own repair as a faint dark core.
    let inside: Vec<f32> = core.iter().map(|on| f32::from(u8::from(*on))).collect();
    let outside: Vec<f32> = field
        .alpha
        .iter()
        .zip(&inside)
        .map(|(a, on)| a * (1.0 - on))
        .collect();
    let own = Weighted::new(&inside, rect.w, rect.h, px(field.radius, 1.0));
    let around = Weighted::new(&outside, rect.w, rect.h, px(field.radius, 1.0));
    let lesion = blur(&inside, rect.w, rect.h, 1);
    let lows: Vec<Vec<f32>> = field
        .channels
        .par_iter()
        .map(|channel| {
            let within = own.mean(channel);
            around
                .mean(channel)
                .iter()
                .zip(within)
                .zip(&lesion)
                .map(|((skin, within), share)| skin + share.min(1.0) * (within - skin))
                .collect()
        })
        .collect();
    let Ok(low) = <[Vec<f32>; 3]>::try_from(lows) else {
        return false;
    };
    let mut rebuilt = rebuilt;
    let mut mark = mark;
    if factor > 1 {
        let grid = [work.rect.w, work.rect.h];
        let full = [rect.w, rect.h];
        for plane in &mut rebuilt {
            *plane = enlarge(plane, grid, factor, full);
        }
        mark = enlarge(&mark, grid, factor, full);
    }
    // Normal pore contrast for this selection, as a ratio of fine detail to tone, measured
    // on unmarked skin only.
    let relief: Vec<f32> = (0..rect.len())
        .map(|i| {
            let base = luma([low[0][i], low[1][i], low[2][i]]).max(1e-5);
            (field.luma[i] - base) / base
        })
        .collect();
    let clean: Vec<f32> = field
        .alpha
        .iter()
        .zip(&mark)
        .map(|(a, m)| if *m > 0.0 { 0.0 } else { *a })
        .collect();
    let normal = (spread(&relief, &clean, &field.sampled) * RELIEF_LIMIT).max(0.01);
    let keep = edit.texture.clamp(0.0, 1.0);
    let smooth_mark = blur(&mark, rect.w, rect.h, 1);
    // What each rebuilt cell becomes, from the planes alone: rows are worked at once and
    // written afterwards, so the result is the same on every machine.
    let healed_at = |i: usize| -> Option<(f32, [f32; 3])> {
        let reach = smooth_mark[i].min(1.0) * field.alpha[i];
        let weight = reach * edit.amount;
        if weight <= 0.0 {
            return None;
        }
        let old = [
            field.channels[0][i],
            field.channels[1][i],
            field.channels[2][i],
        ];
        let old_low = [low[0][i], low[1][i], low[2][i]];
        let new_low: [f32; 3] =
            std::array::from_fn(|c| old_low[c] + edit.tone * (rebuilt[c][i] - old_low[c]));
        let old_base = luma(old_low).max(1e-5);
        // Relief beyond the normal pore range is the mark's own lit rim and shadow.
        let limited = if relief[i].abs() > 1e-6 {
            soft_limit(relief[i], normal) / relief[i]
        } else {
            1.0
        };
        let detail = keep + (1.0 - keep) * limited;
        // Pore contrast is relative: the same pores are fainter on darker skin.
        let gain = detail * luma(new_low).max(0.0) / old_base;
        Some((
            reach,
            std::array::from_fn(|c| {
                let healed = (new_low[c] + (old[c] - old_low[c]) * gain).max(0.0);
                old[c] + weight * (healed - old[c])
            }),
        ))
    };
    let piece_rows: Vec<(&Piece, usize)> = layout
        .pieces
        .iter()
        .flat_map(|piece| (0..piece.frame.h).map(move |row| (piece, row)))
        .collect();
    let rows: Vec<Vec<(usize, f32, [f32; 3])>> = piece_rows
        .into_par_iter()
        .map(|(piece, row)| {
            (0..piece.frame.w)
                .filter_map(|column| {
                    let i = (piece.y + row) * rect.w + piece.x + column;
                    healed_at(i).map(|(reach, value)| {
                        (
                            (piece.frame.y0 + row) * w + piece.frame.x0 + column,
                            reach,
                            value,
                        )
                    })
                })
                .collect()
        })
        .collect();
    for (at, reach, value) in rows.into_iter().flatten() {
        if let Some(plane) = touched.as_deref_mut() {
            plane[at] = plane[at].max(reach);
        }
        rgb[at * 3..at * 3 + 3].copy_from_slice(&value);
    }
    true
}

/// Find and rebuild, then once more on the result: with the strongest marks gone, the skin
/// between them is a truer reference, and fainter marks beside them measure as marks.
pub(crate) fn apply(rgb: &mut [f32], w: usize, h: usize, edit: &Edit, coverage: &Coverage) {
    let Some(layout) = Layout::new(edit, coverage, w, h) else {
        return;
    };
    for _ in 0..usize::from(edit.repair_passes.unwrap_or(PASSES as u8)) {
        if !pass(rgb, w, h, edit, coverage, &layout, None, true) {
            break;
        }
    }
}

/// Where frequency healing rebuilds tone, over the whole frame: 0 for a pixel it leaves
/// exactly as it is, up to 1 inside a mark. Found by running the operation on a copy, so it
/// is what rendering does rather than a prediction of it.
pub(crate) fn mark_plane(
    rgb: &[f32],
    w: usize,
    h: usize,
    edit: &Edit,
    coverage: &Coverage,
) -> Vec<f32> {
    let mut touched = vec![0.0; w * h];
    let Some(layout) = Layout::new(edit, coverage, w, h) else {
        return touched;
    };
    let mut scratch = rgb.to_vec();
    for _ in 0..usize::from(edit.repair_passes.unwrap_or(PASSES as u8)) {
        if !pass(
            &mut scratch,
            w,
            h,
            edit,
            coverage,
            &layout,
            Some(&mut touched),
            true,
        ) {
            break;
        }
    }
    touched
}

#[cfg(test)]
mod tests {
    use super::{Group, Surround};

    #[test]
    fn mild_coloured_face_shading_is_not_a_mark_but_an_enclosed_bright_head_is() {
        let w = 41;
        let mut light = vec![0.25; w * w];
        let mut red = vec![0.05; w * w];
        let mut cells = Vec::new();
        for y in 0..w {
            for x in 0..w {
                let i = y * w + x;
                light[i] = 0.25 + (x as f32 - 20.0) * 0.0015;
                if (x as f32 - 20.0).hypot(y as f32 - 20.0) <= 3.0 {
                    light[i] = 0.25;
                    red[i] = 0.2;
                    cells.push(i);
                }
            }
        }
        let alpha = vec![1.0; w * w];
        let group = Group {
            cells,
            centre: [20., 20.],
            axes: [2., 2.],
            reach: 3.,
        };
        let shape = Surround {
            luma: &light,
            red: &red,
            alpha: &alpha,
            w,
            h: w,
            radius: 3.,
        };
        assert!(
            !shape.encloses(&group, 0.1, 0.15),
            "mild lighting cannot be replaced as a red blemish"
        );
        let mut light = vec![0.25; w * w];
        for i in &group.cells {
            light[*i] = 0.29;
        }
        let shape = Surround {
            luma: &light,
            ..shape
        };
        assert!(
            shape.encloses(&group, 0.1, 0.15),
            "bright inflamed heads remain repairable"
        );
    }

    #[test]
    fn redness_cannot_override_incompatible_light_around_a_candidate() {
        let w = 41;
        let mut luma = Vec::new();
        let mut red = Vec::new();
        let mut cells = Vec::new();
        for y in 0..w {
            for x in 0..w {
                let inside = (x as f32 - 20.0).hypot(y as f32 - 20.0) <= 3.0;
                luma.push(if inside {
                    0.25
                } else if x < 20 {
                    0.4
                } else {
                    0.08
                });
                red.push(if inside { 0.2 } else { 0.05 });
                if inside {
                    cells.push(y * w + x);
                }
            }
        }
        let alpha = vec![1.0; w * w];
        let group = Group {
            cells,
            centre: [20.0, 20.0],
            axes: [2.0, 2.0],
            reach: 3.0,
        };
        let shape = Surround {
            luma: &luma,
            red: &red,
            alpha: &alpha,
            w,
            h: w,
            radius: 3.0,
        };
        assert!(!shape.encloses(&group, 0.2, 0.15));
        // The same redness surrounded by consistently lit skin is a valid mark.
        let even = vec![0.4; w * w];
        let shape = Surround {
            luma: &even,
            ..shape
        };
        assert!(shape.encloses(&group, 0.2, 0.15));
    }

    #[test]
    fn excluded_shadow_still_informs_surrounding_light() {
        let w = 41;
        let mut luma = vec![0.4; w * w];
        let mut red = vec![0.05; w * w];
        let mut alpha = vec![1.0; w * w];
        let mut cells = Vec::new();
        for y in 0..w {
            for x in 0..w {
                let at = y * w + x;
                if (x as f32 - 20.0).hypot(y as f32 - 20.0) <= 3.0 {
                    luma[at] = 0.25;
                    red[at] = 0.2;
                    cells.push(at);
                } else if x > 20 && y > 20 {
                    luma[at] = 0.08;
                    alpha[at] = 0.0;
                }
            }
        }
        let group = Group {
            cells,
            centre: [20.0, 20.0],
            axes: [2.0, 2.0],
            reach: 3.0,
        };
        let shape = Surround {
            luma: &luma,
            red: &red,
            alpha: &alpha,
            w,
            h: w,
            radius: 3.0,
        };
        assert!(
            !shape.encloses(&group, 0.2, 0.15),
            "masking a shadow must not make a contour look like a surrounded blemish"
        );
        // A partial selection alone is not a reason to reject a real lesion.
        let even = vec![0.4; w * w];
        assert!(Surround {
            luma: &even,
            ..shape
        }
        .encloses(&group, 0.2, 0.15));
    }

    /// A frequency heal over the region, at full amount and tone.
    fn heal_edit() -> aura_recipe::retouch_tools::Edit {
        use aura_recipe::retouch_tools::Tool;
        aura_recipe::retouch_tools::Edit {
            id: "large".into(),
            tool: Tool::FrequencyHeal,
            enabled: true,
            region: [0.5, 0.5, 0.75, 0.75],
            source: None,
            amount: 1.0,
            feather: 0.0,
            radius: 0.01,
            source_scale: 1.0,
            preserve_microtexture: false,
            texture_heal: false,
            clean_ring_fit: false,
            curved_heal: false,
            heal_samples: Vec::new(),
            texture_sources: Vec::new(),
            repair_passes: None,
            target_color: None,
            sensitivity: None,
            keep_dark_marks: false,
            mole_edge: None,
            texture: 0.25,
            tone: 1.0,
            warmth: 0.0,
            tint: 0.0,
            selection: None,
            matte: None,
            mask: None,
            skin: None,
        }
    }

    /// A large face at full resolution: pores everywhere and red spots on a lattice, and the
    /// operation that heals it, at a frequency radius well above WORK_RADIUS.
    fn large_face(radius: f32) -> (Vec<f32>, usize, aura_recipe::retouch_tools::Edit) {
        let w = 720_usize;
        let mut rgb = Vec::with_capacity(w * w * 3);
        for y in 0..w {
            for x in 0..w {
                let mut v =
                    (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA77);
                v ^= v >> 13;
                v = v.wrapping_mul(0xC2B2_AE3D);
                v ^= v >> 16;
                let pore = 1.0 + ((v >> 8) as f32 / 16_777_216.0 - 0.5) * 0.05;
                let d = ((x % 120) as f32 - 60.0).hypot((y % 120) as f32 - 60.0);
                // An inflamed spot: redder, a little darker, fading over its edge.
                let spot = (1.0 - (d - 8.0).max(0.0) / 4.0).clamp(0.0, 1.0);
                rgb.extend([
                    0.40 * pore * (1.0 + 0.05 * spot),
                    0.29 * pore * (1.0 - 0.22 * spot),
                    0.23 * pore * (1.0 - 0.22 * spot),
                ]);
            }
        }
        let mut edit = heal_edit();
        edit.radius = radius / w as f32;
        (rgb, w, edit)
    }

    /// Mean redness share of the spot cores and of the skin halfway between them.
    fn spot_and_skin_red(rgb: &[f32], w: usize) -> (f32, f32) {
        let red = |x: usize, y: usize| {
            let i = (y * w + x) * 3;
            let p = [rgb[i], rgb[i + 1], rgb[i + 2]];
            (p[0] - (p[1] + p[2]) * 0.5) / (p[0] + p[1] + p[2])
        };
        let (mut spot, mut skin, mut n) = (0.0, 0.0, 0.0);
        for cy in (60..w - 60).step_by(120) {
            for cx in (60..w - 60).step_by(120) {
                for dy in 0..5 {
                    for dx in 0..5 {
                        spot += red(cx + dx - 2, cy + dy - 2);
                        skin += red(cx + 60 + dx - 2, cy + 60 + dy - 2);
                        n += 1.0;
                    }
                }
            }
        }
        (spot / n, skin / n)
    }

    /// Healing a large face on the working grid heals what healing it sample by sample heals,
    /// keeps the same pores, and differs from it by a small fraction of the skin's own tone.
    #[test]
    #[allow(clippy::print_stdout)]
    fn a_large_face_healed_on_the_working_grid_matches_full_resolution() {
        for radius in [9.0, 13.0, 18.0, 27.0] {
            grid_matches_full_resolution(radius);
        }
    }

    #[allow(clippy::print_stdout)]
    fn grid_matches_full_resolution(radius: f32) {
        let (rgb, w, edit) = large_face(radius);
        let coverage = crate::retouch_mask::Coverage::new(&edit, w, w);
        assert!(
            super::reduction(edit.radius * w as f32) > 1,
            "the test must use the grid"
        );
        let layout = super::Layout::new(&edit, &coverage, w, w).unwrap();
        let heal = |reduce: bool| {
            let mut out = rgb.clone();
            for _ in 0..super::PASSES {
                if !super::pass(&mut out, w, w, &edit, &coverage, &layout, None, reduce) {
                    break;
                }
            }
            out
        };
        let exact = heal(false);
        let quick = heal(true);
        let (spot_before, skin_before) = spot_and_skin_red(&rgb, w);
        let (spot_exact, skin_exact) = spot_and_skin_red(&exact, w);
        let (spot_quick, skin_quick) = spot_and_skin_red(&quick, w);
        let before = spot_before - skin_before;
        let left_exact = (spot_exact - skin_exact) / before;
        let left_quick = (spot_quick - skin_quick) / before;
        // Both heal the spots, and the working grid leaves no more of them behind.
        assert!(
            left_exact < 0.35,
            "full resolution left {left_exact:.3} of the redness"
        );
        assert!(
            left_quick <= left_exact + 0.02,
            "working grid left {left_quick:.3}, full resolution {left_exact:.3}"
        );
        // The two results differ by a small share of the skin's tone, on average and anywhere.
        let mut total = 0.0_f64;
        let mut worst = 0.0_f32;
        for (a, b) in exact.iter().zip(&quick) {
            let d = (a - b).abs() / a.abs().max(0.05);
            total += f64::from(d);
            worst = worst.max(d);
        }
        let mean = total / exact.len() as f64;
        assert!(mean < 0.002, "mean relative difference {mean:.5}");
        assert!(worst < 0.06, "largest relative difference {worst:.4}");
        // The pores are the photograph's own, at the same strength.
        let pores = |plane: &[f32]| -> f32 {
            let lum: Vec<f32> = plane
                .chunks_exact(3)
                .map(|p| super::luma([p[0], p[1], p[2]]))
                .collect();
            let soft = crate::retouch_planes::blur(&lum, w, w, 2);
            lum.iter()
                .zip(&soft)
                .map(|(a, b)| (a - b).abs())
                .sum::<f32>()
                / lum.len() as f32
        };
        let ratio = pores(&quick) / pores(&exact);
        println!(
            "radius {radius}: redness left: full {left_exact:.3}, grid {left_quick:.3}; difference mean {mean:.5},              largest {worst:.4}; pore energy ratio {ratio:.4}"
        );
        assert!((ratio - 1.0).abs() < 0.025, "pore energy ratio {ratio:.4}");
    }

    /// A standing figure's body skin: five separate patches (chest, two arms, two legs) inside a
    /// large body rectangle, each with inflamed spots on a lattice, and a dress around them.
    /// Returns the pixels, the heal and its selection.
    fn patches(
        w: usize,
        h: usize,
    ) -> (
        Vec<f32>,
        aura_recipe::retouch_tools::Edit,
        crate::retouch_mask::Coverage,
    ) {
        use aura_recipe::retouch_tools::{Matte, Tool};
        const PATCHES: [[f32; 4]; 5] = [
            [0.40, 0.10, 0.60, 0.20],
            [0.05, 0.22, 0.15, 0.60],
            [0.85, 0.22, 0.95, 0.60],
            [0.30, 0.76, 0.42, 0.98],
            [0.58, 0.76, 0.70, 0.98],
        ];
        let inside = |x: usize, y: usize| {
            let (u, v) = ((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
            PATCHES
                .iter()
                .any(|[l, t, r, b]| u >= *l && u < *r && v >= *t && v < *b)
        };
        let mut rgb = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for x in 0..w {
                let mut v =
                    (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA77);
                v ^= v >> 13;
                v = v.wrapping_mul(0xC2B2_AE3D);
                v ^= v >> 16;
                let grain = 1.0 + ((v >> 8) as f32 / 16_777_216.0 - 0.5) * 0.05;
                if inside(x, y) {
                    let d = ((x % 41) as f32 - 20.0).hypot((y % 37) as f32 - 18.0);
                    let spot = (1.0 - (d - 3.0).max(0.0) / 2.0).clamp(0.0, 1.0);
                    rgb.extend([
                        0.40 * grain * (1.0 + 0.05 * spot),
                        0.29 * grain * (1.0 - 0.22 * spot),
                        0.23 * grain * (1.0 - 0.22 * spot),
                    ]);
                } else {
                    rgb.extend([0.05 * grain, 0.07 * grain, 0.20 * grain]);
                }
            }
        }
        // The segmenter's grid is coarse: four pixels a cell here.
        let (mw, mh) = (w / 4, h / 4);
        let alpha: Vec<u8> = (0..mw * mh)
            .map(|i| {
                if inside((i % mw) * 4 + 2, (i / mw) * 4 + 2) {
                    255
                } else {
                    0
                }
            })
            .collect();
        let mut matte = Matte::encode([0.0, 0.0, 1.0, 1.0], mw as u32, mh as u32, &alpha);
        matte.refine_edges = false;
        let mut edit = heal_edit();
        edit.tool = Tool::FrequencyHeal;
        edit.region = [0.5, 0.5, 0.5, 0.5];
        edit.feather = 0.0;
        edit.radius = 2.0 / w.min(h) as f32;
        edit.sensitivity = Some(0.5);
        edit.mole_edge = Some(0.0);
        edit.texture = 0.65;
        let plane = crate::retouch_matte::MattePlane::render(&matte, &rgb, w, h).unwrap();
        let coverage = crate::retouch_mask::Coverage::new(&edit, w, h).with_matte(&plane, w, h);
        (rgb, edit, coverage)
    }

    /// Separate patches of body skin are packed into a plane a fraction of the body
    /// rectangle's size, nothing selected is left out of it, and no two pieces overlap.
    #[test]
    fn separate_patches_are_packed_and_cover_the_whole_selection() {
        let (w, h) = (800, 1200);
        let (_, edit, coverage) = patches(w, h);
        let whole = super::Layout::whole(&edit, &coverage, w, h).unwrap();
        let packed = super::Layout::new(&edit, &coverage, w, h).unwrap();
        assert_eq!(packed.pieces.len(), 5, "{packed:?}");
        assert!(
            packed.len() * 2 < whole.len(),
            "packed {} of {} cells",
            packed.len(),
            whole.len()
        );
        let mut in_frame = vec![0_u8; w * h];
        let mut in_plane = vec![0_u8; packed.len()];
        for piece in &packed.pieces {
            assert_eq!(piece.x % super::PACK_ALIGN, 0);
            assert_eq!(piece.y % super::PACK_ALIGN, 0);
            for row in 0..piece.frame.h {
                for column in 0..piece.frame.w {
                    in_frame[(piece.frame.y0 + row) * w + piece.frame.x0 + column] += 1;
                    in_plane[(piece.y + row) * packed.w + piece.x + column] += 1;
                }
            }
        }
        assert!(
            in_frame.iter().all(|n| *n <= 1),
            "pieces overlap in the frame"
        );
        assert!(
            in_plane.iter().all(|n| *n <= 1),
            "pieces overlap in the plane"
        );
        for y in 0..h {
            for x in 0..w {
                if coverage.at(x, y, w, h) > 0.0 {
                    assert_eq!(in_frame[y * w + x], 1, "selected pixel {x},{y} left out");
                }
            }
        }
        // One patch is its own rectangle and is not packed.
        let mut one = coverage.clone();
        one.bounds = [0, 0, w, (h as f32 * 0.21) as usize];
        let single = super::Layout::new(&edit, &one, w, h).unwrap();
        assert_eq!(single.pieces.len(), 1);
    }

    /// Healing the packed patches heals what healing the whole body rectangle heals, leaves
    /// everything unselected exactly as it was, and differs from it only by rounding.
    #[test]
    fn separate_patches_heal_as_they_do_in_the_whole_rectangle() {
        let (w, h) = (800, 1200);
        let (rgb, edit, coverage) = patches(w, h);
        let heal = |layout: &super::Layout| {
            let mut out = rgb.clone();
            for _ in 0..super::PASSES {
                if !super::pass(&mut out, w, h, &edit, &coverage, layout, None, true) {
                    break;
                }
            }
            out
        };
        let whole = heal(&super::Layout::whole(&edit, &coverage, w, h).unwrap());
        let packed = heal(&super::Layout::new(&edit, &coverage, w, h).unwrap());
        let red = |plane: &[f32], x: usize, y: usize| {
            let i = (y * w + x) * 3;
            (plane[i] - (plane[i + 1] + plane[i + 2]) * 0.5)
                / (plane[i] + plane[i + 1] + plane[i + 2])
        };
        let (mut spots, mut before, mut left_whole, mut left_packed) = (0, 0.0, 0.0, 0.0);
        for y in 0..h {
            for x in 0..w {
                let selected = coverage.at(x, y, w, h) > 0.0;
                if !selected {
                    let i = (y * w + x) * 3;
                    assert_eq!(&whole[i..i + 3], &rgb[i..i + 3]);
                    assert_eq!(&packed[i..i + 3], &rgb[i..i + 3]);
                    continue;
                }
                // Spot centres well inside a patch.
                let inner = (2..=3).all(|k| {
                    coverage.at(x.saturating_sub(k * 4), y, w, h) > 0.0
                        && coverage.at((x + k * 4).min(w - 1), y, w, h) > 0.0
                        && coverage.at(x, y.saturating_sub(k * 4), w, h) > 0.0
                        && coverage.at(x, (y + k * 4).min(h - 1), w, h) > 0.0
                });
                if x % 41 == 20 && y % 37 == 18 && inner {
                    spots += 1;
                    before += red(&rgb, x, y);
                    left_whole += red(&whole, x, y);
                    left_packed += red(&packed, x, y);
                }
            }
        }
        assert!(spots > 50, "{spots} spots measured");
        // The skin's own redness share, from the colour it was painted with.
        let skin = (0.40 - (0.29 + 0.23) * 0.5) / (0.40 + 0.29 + 0.23);
        let share = |left: f32| (left / spots as f32 - skin) / (before / spots as f32 - skin);
        assert!(
            share(left_whole) < 0.4,
            "whole left {:.3}",
            share(left_whole)
        );
        assert!(
            share(left_packed) <= share(left_whole) + 0.02,
            "packed left {:.3}, whole {:.3}",
            share(left_packed),
            share(left_whole)
        );
        let mut total = 0.0_f64;
        let mut worst = 0.0_f32;
        for (a, b) in whole.iter().zip(&packed) {
            let d = (a - b).abs() / a.abs().max(0.05);
            total += f64::from(d);
            worst = worst.max(d);
        }
        let mean = total / whole.len() as f64;
        // The same pixels, the same thresholds and the same marks: only the order a blur's
        // running sums meet the gaps in differs, which is rounding.
        assert!(mean < 1e-5, "mean relative difference {mean:.7}");
        assert!(worst < 1e-3, "largest relative difference {worst:.6}");
    }

    /// Where one frequency heal over a face-sized selection spends its time. By hand:
    /// `cargo test --release -p aura-render --lib -- retouch_clear::tests::timing --ignored --nocapture`.
    #[test]
    #[ignore = "timing, by hand"]
    #[allow(clippy::disallowed_methods, clippy::print_stdout)]
    fn timing() {
        use aura_recipe::retouch_tools::Tool;
        let (w, h) = (1400_usize, 1600_usize);
        let mut rgb = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for x in 0..w {
                let mut v =
                    (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA77);
                v ^= v >> 13;
                v = v.wrapping_mul(0xC2B2_AE3D);
                let pore = 1.0 + ((v >> 8) as f32 / 16_777_216.0 - 0.5) * 0.06;
                let spot = ((x % 173) as f32 - 86.0).hypot((y % 151) as f32 - 75.0) < 7.0;
                let k = if spot { 0.8 } else { 1.0 };
                rgb.extend([0.42 * pore, 0.3 * pore * k, 0.24 * pore * k]);
            }
        }
        let edit = aura_recipe::retouch_tools::Edit {
            id: "t".into(),
            tool: Tool::FrequencyHeal,
            enabled: true,
            region: [0.5, 0.5, 0.45, 0.45],
            source: None,
            amount: 1.0,
            feather: 0.1,
            radius: 18.0 / w as f32,
            source_scale: 1.0,
            preserve_microtexture: false,
            texture_heal: false,
            clean_ring_fit: false,
            curved_heal: false,
            heal_samples: Vec::new(),
            texture_sources: Vec::new(),
            repair_passes: None,
            target_color: None,
            sensitivity: None,
            keep_dark_marks: false,
            mole_edge: Some(1.0),
            texture: 0.25,
            tone: 1.0,
            warmth: 0.0,
            tint: 0.0,
            selection: None,
            matte: None,
            mask: None,
            skin: None,
        };
        let coverage = crate::retouch_mask::Coverage::new(&edit, w, h);
        let t = std::time::Instant::now();
        let layout = super::Layout::new(&edit, &coverage, w, h).unwrap();
        let field = super::Field::new(&rgb, w, h, &edit, &coverage, &layout).unwrap();
        println!(
            "field {:?} ({}x{})",
            t.elapsed(),
            field.rect.w,
            field.rect.h
        );
        let t = std::time::Instant::now();
        let moles = super::near_a_mole(&field, 1.0);
        println!("near_a_mole {:?} ({})", t.elapsed(), moles.len());
        let t = std::time::Instant::now();
        let marks = super::marks(&field, 0.5, false, Some(1.0)).unwrap();
        println!("marks {:?}", t.elapsed());
        let t = std::time::Instant::now();
        let _ = super::rebuilt_tone(&field, &marks.weight);
        println!("rebuilt_tone {:?}", t.elapsed());
        let t = std::time::Instant::now();
        let flags: Vec<bool> = marks.core.clone();
        let _ = crate::retouch_planes::distance_to(&flags, field.rect.w, field.rect.h);
        println!("distance_to {:?}", t.elapsed());
        let t = std::time::Instant::now();
        let _ = crate::retouch_planes::groups(&flags, field.rect.w, field.rect.h);
        println!("groups {:?}", t.elapsed());
        let t = std::time::Instant::now();
        let _ = crate::retouch_planes::blur(&field.luma, field.rect.w, field.rect.h, 400);
        println!("one blur {:?}", t.elapsed());
        let mut out = rgb.clone();
        let t = std::time::Instant::now();
        super::apply(&mut out, w, h, &edit, &coverage);
        println!("apply {:?}", t.elapsed());
    }
}
