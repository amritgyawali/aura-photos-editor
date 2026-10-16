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

/// What the operation works on: the rectangle, its pixels and the selection inside it.
struct Field {
    rect: Rect,
    /// Selection coverage, 0..=1, with the outer part of a soft edge counted as selected.
    alpha: Vec<f32>,
    channels: [Vec<f32>; 3],
    luma: Vec<f32>,
    /// Red share relative to the other two channels; exposure-independent.
    red: Vec<f32>,
    /// The frequency radius in pixels.
    radius: f32,
}

fn px(radius: f32, factor: f32) -> usize {
    (radius * factor).round().max(1.0) as usize
}

impl Field {
    fn new(rgb: &[f32], w: usize, h: usize, edit: &Edit, coverage: &Coverage) -> Option<Self> {
        let radius = (edit.radius * w.min(h) as f32).max(1.0);
        let rect = Rect::around(coverage.bounds, px(radius, 14.0) + 2, w, h)?;
        // Every cell is read on its own, so the rows are shared across cores.
        let cells: Vec<(f32, [f32; 3])> = rect
            .par_cells()
            .map(|(x, y)| {
                let i = (y * w + x) * 3;
                (
                    smoothstep(0.0, FULLY_SELECTED, coverage.at(x, y, w, h)),
                    [rgb[i], rgb[i + 1], rgb[i + 2]],
                )
            })
            .collect();
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
    fn reduced(&self, factor: usize) -> Self {
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
    let step = (field.rect.len() / SPREAD_SAMPLES).max(1);
    let mut samples: Vec<f32> = field
        .luma
        .iter()
        .zip(&field.alpha)
        .step_by(step)
        .filter(|(_, a)| **a > 0.5)
        .map(|(v, _)| *v)
        .collect();
    crate::retouch_planes::quantile(&mut samples, 0.5).unwrap_or(0.18)
}

/// Robust spread of `values` over well-selected cells.
fn spread(values: &[f32], alpha: &[f32]) -> f32 {
    let step = (values.len() / SPREAD_SAMPLES).max(1);
    let mut samples: Vec<f32> = values
        .iter()
        .zip(alpha)
        .step_by(step)
        .filter(|(_, a)| **a > 0.5)
        .map(|(v, _)| *v)
        .collect();
    centre_and_spread(&mut samples).map_or(0.0, |(_, s)| s)
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
        let consistent_light =
            darkest_ring >= own_luma * 0.8 && brightest_ring <= darkest_ring * 1.65;
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
        let dark_limit = (spread(&dark, &field.alpha) * k).max(0.03);
        let red_limit = (spread(&redder, &field.alpha) * k).max(0.006);
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

/// One find-and-rebuild pass. Returns whether anything was found. `touched`, when given, is a
/// frame-sized plane that receives the largest weight each pixel was rebuilt with. `reduce`
/// lets a large selection be measured on the working grid; see [`WORK_RADIUS`].
#[allow(clippy::too_many_lines)]
fn pass(
    rgb: &mut [f32],
    w: usize,
    h: usize,
    edit: &Edit,
    coverage: &Coverage,
    mut touched: Option<&mut [f32]>,
    reduce: bool,
) -> bool {
    let Some(field) = Field::new(rgb, w, h, edit, coverage) else {
        return false;
    };
    let rect = field.rect;
    // Marks are found, and the tone under them rebuilt, on the working grid; see WORK_RADIUS.
    let factor = if reduce { reduction(field.radius) } else { 1 };
    let small = (factor > 1).then(|| field.reduced(factor));
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
    let normal = (spread(&relief, &clean) * RELIEF_LIMIT).max(0.01);
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
    let rows: Vec<Vec<(usize, f32, [f32; 3])>> = (0..rect.h)
        .into_par_iter()
        .map(|row| {
            (0..rect.w)
                .filter_map(|column| {
                    let i = row * rect.w + column;
                    healed_at(i).map(|(reach, value)| {
                        ((rect.y0 + row) * w + rect.x0 + column, reach, value)
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
    for _ in 0..usize::from(edit.repair_passes.unwrap_or(PASSES as u8)) {
        if !pass(rgb, w, h, edit, coverage, None, true) {
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
    let mut scratch = rgb.to_vec();
    for _ in 0..usize::from(edit.repair_passes.unwrap_or(PASSES as u8)) {
        if !pass(&mut scratch, w, h, edit, coverage, Some(&mut touched), true) {
            break;
        }
    }
    touched
}

#[cfg(test)]
mod tests {
    use super::{Group, Surround};

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

    /// A large face at full resolution: pores everywhere and red spots on a lattice, and the
    /// operation that heals it, at a frequency radius well above WORK_RADIUS.
    fn large_face(radius: f32) -> (Vec<f32>, usize, aura_recipe::retouch_tools::Edit) {
        use aura_recipe::retouch_tools::Tool;
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
        let edit = aura_recipe::retouch_tools::Edit {
            id: "large".into(),
            tool: Tool::FrequencyHeal,
            enabled: true,
            region: [0.5, 0.5, 0.75, 0.75],
            source: None,
            amount: 1.0,
            feather: 0.0,
            radius: radius / w as f32,
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
        };
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
        let heal = |reduce: bool| {
            let mut out = rgb.clone();
            for _ in 0..super::PASSES {
                if !super::pass(&mut out, w, w, &edit, &coverage, None, reduce) {
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
        let field = super::Field::new(&rgb, w, h, &edit, &coverage).unwrap();
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
