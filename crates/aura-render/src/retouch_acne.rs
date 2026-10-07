//! Acne clear: find every blemish in a skin selection against a robust estimate of the clean
//! skin around it, rebuild the tone under each one, and keep the pores. ADR-0092.
//!
//! Frequency healing (ADR-0090) calls a group a mark only when the skin on every side of it
//! is clean. That is safe, and on a face with dense acne it is not enough: inside a cluster
//! the skin beside a mark is another mark, so most of a cluster is never repaired, and the
//! nose, the forehead between the brows and the cheeks of a real acne portrait keep half of
//! their marks. This operation measures each pixel against *clean skin* rather than against
//! *the skin next to it*:
//!
//! 1. **A robust reference.** The tone and colour this skin would have without its marks, at
//!    every pixel: a local percentile of the selected skin in a window three mark radii wide
//!    (seven where that window holds too little skin), measured twice - the second time
//!    without the blemish-sized groups the first found. A percentile, unlike a mean, keeps an
//!    edge: a mark in a shadow is compared with the shadow, and a mark among marks with the
//!    clean skin between them. A region that departs as a whole - a naturally redder nose, the
//!    shadowed side of a face - stays its own reference.
//! 2. **Find.** A pixel's departure from that reference is measured in three directions that
//!    do not depend on exposure: darker (log luminance), redder (log red over green) and
//!    browner (log green over blue). Each is divided by this selection's own robust spread.
//!    A shadow changes how bright skin is and not its colour, so a group that is only darker
//!    must also be darker than the skin on every side of it (the enclosure test frequency
//!    healing uses); a group that is redder or browner is a mark wherever it is. Long thin
//!    groups - a crease, a hair, the rim of an eyelid - are never marks.
//! 3. **Rebuild.** Under a mark the tone and colour become those of the unmarked skin right
//!    around it, never brighter or less coloured than the brighter, calmer quarter of the skin
//!    there, and the pore detail that was photographed there stays, its relief limited to this
//!    skin's ordinary pore range. A coloured mark is never made darker than it is; only the
//!    white head of a pimple, inside its red ring, is brought down. A pixel no mark reached
//!    keeps its exact value.
//! 4. **Even the redness** (optional, `preserve_microtexture`). Red or brown blotches too broad
//!    to be marks have the colour beyond this skin's ordinary variation moved most of the way
//!    toward the same person's clean skin, in proportion, so no patch outline is drawn; a blotch
//!    that is also darker is lifted by the same share, and skin that is only darker is not.
//!    Bands along the edge of the face - skin warms where it turns from the light - are left.
//!
//! Nothing is generated: every value written is this photograph's own skin, averaged. With no
//! segmentation matte the operation is a brush: the reference also reads the skin just
//! outside the painted area, and what is painted is trusted, so a darker spot needs no
//! enclosure test and a compact bright bump (a whitehead) counts as a mark too.
// Every plane covers one bounded rectangle and every index is derived from it.
#![allow(clippy::indexing_slicing)]
use crate::retouch_mask::Coverage;
use crate::retouch_planes::{
    blur, centre_and_spread, distance_to, groups, local_percentile, luma, smoothstep, Group, Rect,
    Weighted,
};
use aura_recipe::retouch_tools::Edit;

/// Selection coverage at or above this counts as fully selected: half a mark is still a mark.
const FULLY_SELECTED: f32 = 0.25;
/// How many samples the robust spreads are measured from.
const SAMPLES: usize = 60_000;
/// Find-and-rebuild passes: with the strongest marks gone the reference is cleaner still.
const PASSES: usize = 2;
/// Stricter thresholds split marks that merge at the most sensitive one.
const TIERS: [f32; 3] = [1.0, 1.6, 2.5];
/// A group longer than this many times its width is a line, not a mark.
const MAX_ELONGATION: f32 = 5.0;
/// Pore relief under a mark beyond this many robust spreads belongs to the mark.
const RELIEF_LIMIT: f32 = 1.5;
/// Directions a dark-only group's surroundings are read in.
const SECTORS: usize = 8;
/// Linear Rec.2020 luminance weights, as [`luma`].
const WEIGHTS: [f32; 3] = [0.2627, 0.6780, 0.0593];

fn px(radius: f32, factor: f32) -> usize {
    (radius * factor).round().max(1.0) as usize
}

/// Log luminance and two exposure-independent colour axes: red over green, green over blue.
#[derive(Debug, Clone)]
struct Lab {
    l: Vec<f32>,
    a: Vec<f32>,
    b: Vec<f32>,
}

impl Lab {
    fn zeros(n: usize) -> Self {
        Self {
            l: vec![0.0; n],
            a: vec![0.0; n],
            b: vec![0.0; n],
        }
    }

    fn planes(&self) -> [&Vec<f32>; 3] {
        [&self.l, &self.a, &self.b]
    }

    fn mean(&self, weighted: &Weighted<'_>) -> Self {
        Self {
            l: weighted.mean(&self.l),
            a: weighted.mean(&self.a),
            b: weighted.mean(&self.b),
        }
    }
}

fn to_lab(p: [f32; 3]) -> [f32; 3] {
    let [r, g, b] = p.map(|v| v.max(1e-5).ln());
    [luma(p).max(1e-5).ln(), r - g, g - b]
}

/// The inverse of [`to_lab`]: green is solved so the luminance comes out exactly.
fn from_lab([l, a, b]: [f32; 3]) -> [f32; 3] {
    let green = l - (WEIGHTS[0] * a.exp() + WEIGHTS[1] + WEIGHTS[2] * (-b).exp()).ln();
    [(green + a).exp(), green.exp(), (green - b).exp()]
}

/// What the operation works on: the rectangle, its pixels and the selection inside it.
struct Field {
    rect: Rect,
    /// Where marks are looked for and repaired, 0..=1, a soft edge counted as selected.
    alpha: Vec<f32>,
    /// Where clean skin may be read from: the selection, and for a brush the skin around it.
    reference: Vec<f32>,
    lab: Lab,
    /// The mark radius in pixels.
    radius: f32,
    /// A painted brush rather than a measured skin selection.
    brush: bool,
}

impl Field {
    fn new(rgb: &[f32], w: usize, h: usize, edit: &Edit, coverage: &Coverage) -> Option<Self> {
        let radius = (edit.radius * w.min(h) as f32).max(1.0);
        let rect = Rect::around(coverage.bounds, px(radius, 22.0) + 2, w, h)?;
        let mut alpha = Vec::with_capacity(rect.len());
        let mut lab = Lab {
            l: Vec::with_capacity(rect.len()),
            a: Vec::with_capacity(rect.len()),
            b: Vec::with_capacity(rect.len()),
        };
        for (x, y) in rect.cells() {
            let i = (y * w + x) * 3;
            alpha.push(smoothstep(0.0, FULLY_SELECTED, coverage.at(x, y, w, h)));
            let [l, a, b] = to_lab([rgb[i], rgb[i + 1], rgb[i + 2]]);
            lab.l.push(l);
            lab.a.push(a);
            lab.b.push(b);
        }
        if !alpha.iter().any(|a| *a > 0.0) {
            return None;
        }
        let brush = edit.matte.is_none();
        let reference = if brush {
            // A brush is painted over the marks; the clean skin is mostly just outside it.
            let around = blur(&alpha, rect.w, rect.h, px(radius, 5.0));
            alpha
                .iter()
                .zip(around)
                .map(|(a, near)| a.max(smoothstep(0.01, 0.15, near)))
                .collect()
        } else {
            alpha.clone()
        };
        Some(Self {
            rect,
            alpha,
            reference,
            lab,
            radius,
            brush,
        })
    }
}

/// Robust spread of `values` over cells whose weight is above one half.
fn spread(values: &[f32], weight: &[f32], floor: f32) -> f32 {
    let step = (values.len() / SAMPLES).max(1);
    let mut samples: Vec<f32> = values
        .iter()
        .zip(weight)
        .step_by(step)
        .filter(|(_, a)| **a > 0.5)
        .map(|(v, _)| *v)
        .collect();
    centre_and_spread(&mut samples).map_or(floor, |(_, s)| s.max(floor))
}

/// The clean-skin level at every cell: a local percentile of the counted cells, from a window
/// three mark radii wide where that holds enough skin, else from one seven radii wide.
///
/// `shares` is the percentile for luminance, red and brown. Before marks are known it leans
/// toward brighter and less coloured skin, which is what clean skin is next to a blemish.
fn reference(fine: &Lab, counted: &[bool], rect: Rect, radius: f32, shares: [f32; 3]) -> Lab {
    let near = (px(radius, 3.0), px(radius, 0.75));
    let far = (px(radius, 7.0), px(radius, 2.0));
    let mut out = Lab::zeros(rect.len());
    for ((plane, values), share) in [&mut out.l, &mut out.a, &mut out.b]
        .into_iter()
        .zip(fine.planes())
        .zip(shares)
    {
        let small = local_percentile(values, counted, rect.w, rect.h, near.0, near.1, share);
        let large = local_percentile(values, counted, rect.w, rect.h, far.0, far.1, share);
        for (i, value) in plane.iter_mut().enumerate() {
            let trust = smoothstep(0.1, 0.3, small.support[i]);
            *value = small.level[i] * trust + large.level[i] * (1.0 - trust);
        }
    }
    soften(&out, counted, rect, radius)
}

/// A percentile is measured on a grid and keeps edges as steps; a mark rebuilt from it would
/// show the step. Averaged over a mark radius of counted skin, the level follows the light as
/// smoothly as the skin around it does.
fn soften(level: &Lab, counted: &[bool], rect: Rect, radius: f32) -> Lab {
    let weights: Vec<f32> = counted.iter().map(|on| f32::from(u8::from(*on))).collect();
    level.mean(&Weighted::new(&weights, rect.w, rect.h, px(radius, 1.0)))
}

/// How far each cell departs from the reference, in robust spreads.
struct Departure {
    /// Darker, redder, browner and brighter, each 0 or more.
    dark: Vec<f32>,
    red: Vec<f32>,
    brown: Vec<f32>,
    bright: Vec<f32>,
}

impl Departure {
    fn measure(fine: &Lab, reference: &Lab, weight: &[f32]) -> Self {
        let dl: Vec<f32> = reference
            .l
            .iter()
            .zip(&fine.l)
            .map(|(r, f)| r - f)
            .collect();
        let da: Vec<f32> = fine
            .a
            .iter()
            .zip(&reference.a)
            .map(|(f, r)| f - r)
            .collect();
        let db: Vec<f32> = fine
            .b
            .iter()
            .zip(&reference.b)
            .map(|(f, r)| f - r)
            .collect();
        let sl = spread(&dl, weight, 0.01);
        let sa = spread(&da, weight, 0.002);
        let sb = spread(&db, weight, 0.002);
        Self {
            dark: dl.iter().map(|v| v.max(0.0) / sl).collect(),
            red: da.iter().map(|v| v.max(0.0) / sa).collect(),
            brown: db.iter().map(|v| v.max(0.0) / sb).collect(),
            bright: dl.iter().map(|v| (-v).max(0.0) / sl).collect(),
        }
    }

    fn colour(&self, i: usize) -> f32 {
        self.red[i].hypot(self.brown[i])
    }

    /// Evidence that a cell is a blemish: darker, redder or browner than clean skin.
    fn score(&self, i: usize) -> f32 {
        self.dark[i].hypot(self.colour(i))
    }
}

/// The clean-skin reference and the departures from it. Measured twice: the second time the
/// cells the first found clearly marked are not counted, so a dense cluster of marks does not
/// become its own reference.
fn robust(field: &Field, fine: &Lab, threshold: f32) -> (Lab, Departure) {
    let rect = field.rect;
    let selected: Vec<bool> = field.reference.iter().map(|a| *a > 0.5).collect();
    let first = reference(fine, &selected, rect, field.radius, [0.6, 0.4, 0.4]);
    let first_weights: Vec<f32> = selected.iter().map(|on| f32::from(u8::from(*on))).collect();
    let departure = Departure::measure(fine, &first, &first_weights);
    let flagged: Vec<bool> = (0..rect.len())
        .map(|i| departure.score(i).max(departure.bright[i]) > threshold * 0.8)
        .collect();
    // Only blemish-sized groups are taken out of the count. A region that departs as a whole -
    // a naturally redder nose, a shadowed side of a face - is its own skin, and taking it out
    // would make the skin a cheek away its reference.
    let largest = 60.0 * field.radius * field.radius;
    let mut marked = vec![false; rect.len()];
    for group in groups(&flagged, rect.w, rect.h) {
        if (group.cells.len() as f32) <= largest {
            for i in group.cells {
                marked[i] = true;
            }
        }
    }
    let near_flag = distance_to(&marked, rect.w, rect.h);
    let clean: Vec<bool> = selected
        .iter()
        .zip(&near_flag)
        .map(|(on, d)| *on && *d > 1.5)
        .collect();
    let second = reference(fine, &clean, rect, field.radius, [0.5, 0.5, 0.5]);
    let weights: Vec<f32> = clean.iter().map(|on| f32::from(u8::from(*on))).collect();
    let departure = Departure::measure(fine, &second, &weights);
    (second, departure)
}

/// Whether a group is darker than the selected skin on every side of it. A shadow beside an
/// eye, a nose or a jaw is darker on one side only, and a group that is only darker - not
/// redder or browner - must pass this to be a mark.
fn enclosed(group: &Group, fine: &Lab, alpha: &[f32], rect: Rect, radius: f32) -> bool {
    let area = group.cells.len() as f32;
    let own = group.cells.iter().map(|i| fine.l[*i]).sum::<f32>() / area;
    let mut valid = 0;
    let mut darker = 0;
    for sector in 0..SECTORS {
        let (mut sum, mut count) = (0.0_f32, 0.0_f32);
        for step in 0..3 {
            let distance = group.reach + radius * (0.75 + 0.6 * step as f32);
            for offset in [-0.3_f32, 0.0, 0.3] {
                let angle = (sector as f32 + 0.5 + offset) / SECTORS as f32 * std::f32::consts::TAU;
                let x = (group.centre[0] + angle.cos() * distance).round();
                let y = (group.centre[1] + angle.sin() * distance).round();
                if x < 0.0 || y < 0.0 || x >= rect.w as f32 || y >= rect.h as f32 {
                    continue;
                }
                let i = y as usize * rect.w + x as usize;
                if alpha[i] > 0.5 {
                    sum += fine.l[i];
                    count += 1.0;
                }
            }
        }
        if count < 3.0 {
            continue;
        }
        valid += 1;
        // Log luminance: 0.03 is three per cent darker than the ring.
        if sum / count - own > 0.03 {
            darker += 1;
        }
    }
    valid >= SECTORS - 2 && darker >= valid - 1
}

/// Cells that belong to a mark. `None` when there are none.
/// Whether a bright group sits inside a mark: the head of a pimple is ringed by its red base,
/// and a brighter patch of skin beside a mark is not.
fn ringed(group: &Group, marked: &[bool], w: usize, h: usize, r: f32) -> bool {
    let distance = group.reach + (r * 0.5).max(1.5);
    let inside = (0..SECTORS)
        .filter(|sector| {
            let angle = (*sector as f32 + 0.5) / SECTORS as f32 * std::f32::consts::TAU;
            let x = (group.centre[0] + angle.cos() * distance).round();
            let y = (group.centre[1] + angle.sin() * distance).round();
            x >= 0.0
                && y >= 0.0
                && x < w as f32
                && y < h as f32
                && marked[y as usize * w + x as usize]
        })
        .count();
    inside >= SECTORS - 2
}

/// How far each cell is from the edge of the selection, in pixels.
fn rim_distance(alpha: &[f32], w: usize, h: usize) -> Vec<f32> {
    let outside: Vec<bool> = alpha.iter().map(|a| *a < 0.5).collect();
    distance_to(&outside, w, h)
}

/// In robust spreads: 2.25 at the default sensitivity, 1.5 at the highest. A brush is painted
/// over what a person can see is wrong, so it asks for less.
fn threshold(sensitivity: f32, brush: bool) -> f32 {
    (3.0 - 1.5 * sensitivity.clamp(0.0, 1.0)) * if brush { 0.85 } else { 1.0 }
}

/// What one pass found.
struct Marks {
    /// Cells of every mark.
    cells: Vec<bool>,
    /// Cells of marks that are brighter than the skin - the head of a pimple, a bump a brush
    /// was painted over - and so may be darkened. Every other mark is only ever lifted.
    bright: Vec<bool>,
}

fn marks(
    field: &Field,
    fine: &Lab,
    departure: &Departure,
    threshold: f32,
    keep_dark: bool,
) -> Option<Marks> {
    let Rect { w, h, .. } = field.rect;
    let n = field.rect.len();
    let r = field.radius;
    let score: Vec<f32> = (0..n).map(|i| departure.score(i)).collect();
    let selected: Vec<bool> = field.alpha.iter().map(|a| *a > 0.2).collect();
    let smallest = (0.35 * r * r).max(3.0);
    let rim = rim_distance(&field.alpha, w, h);
    let mut marked = vec![false; n];
    let mut lines = vec![false; n];
    for tier in TIERS {
        let level = threshold * tier;
        let flagged: Vec<bool> = score
            .iter()
            .zip(&selected)
            .map(|(s, on)| *on && *s > level)
            .collect();
        for group in groups(&flagged, w, h) {
            let area = group.cells.len() as f32;
            let thin = group.axes[1] < r * 0.6;
            if group.elongation() > MAX_ELONGATION && thin && group.reach > r * 2.5 {
                for &i in &group.cells {
                    lines[i] = true;
                }
                continue;
            }
            let at = (group.centre[1].round() as usize).min(h - 1) * w
                + (group.centre[0].round() as usize).min(w - 1);
            if area < smallest || group.elongation() > MAX_ELONGATION || lines[at] {
                continue;
            }
            // A long band - the warm edge of a shadow, the rim of the skin selection - is light
            // falling on the face. A row of real marks splits into compact ones a tier higher.
            if !field.brush && group.elongation() > 3.0 && group.reach > r * 3.0 {
                continue;
            }
            // Skin darkens and warms where it turns away from the light, at the edge of the
            // face and the hairline. A group lying mostly along the selection's edge that is
            // not a small round spot is that, not a mark.
            let along_edge = group.cells.iter().filter(|i| rim[**i] < r * 2.0).count() as f32;
            if !field.brush
                && along_edge > area * 0.5
                && (group.elongation() > 1.8 || area > 8.0 * r * r)
            {
                continue;
            }
            let peak = group.cells.iter().map(|i| score[*i]).fold(0.0, f32::max);
            if peak < level * 1.2 {
                continue;
            }
            // How much of the evidence is colour rather than brightness.
            let (mut colour, mut total) = (0.0_f32, 0.0_f32);
            for &i in &group.cells {
                colour += departure.colour(i).powi(2);
                total += score[i].powi(2);
            }
            let coloured = colour / total.max(1e-6);
            let dark_only = coloured < 0.35;
            // A mark no redder or browner than its surroundings may be a mole or a freckle.
            if dark_only && keep_dark {
                continue;
            }
            // A dark-only group larger than a big pimple is shading. A coloured one larger than
            // a cluster of a few marks is a blotch: rebuilt whole it would be a flat patch with
            // an outline, so it is left to the redness evening, which moves its colour and only
            // as much of its darkness. A real cluster splits into marks a tier higher.
            let largest = if dark_only {
                20.0 * r * r
            } else {
                45.0 * r * r
            };
            if area > largest {
                continue;
            }
            if dark_only && !field.brush && !enclosed(&group, fine, &field.alpha, field.rect, r) {
                continue;
            }
            for &i in &group.cells {
                marked[i] = true;
            }
        }
    }
    // The white head of a pimple is brighter than the skin, not darker or redder, and sits
    // inside the red ring just found. A small bright group touching a mark goes with it; a glint
    // or a bright pore anywhere else is left alone. A brush counts every compact bright bump.
    let raised: Vec<bool> = (0..n)
        .map(|i| selected[i] && departure.bright[i] > threshold)
        .collect();
    let mut bright = vec![false; n];
    for group in groups(&raised, w, h) {
        let area = group.cells.len() as f32;
        if area > 20.0 * r * r || group.elongation() > 3.0 || area < smallest {
            continue;
        }
        if field.brush || ringed(&group, &marked, w, h, r) {
            for &i in &group.cells {
                marked[i] = true;
                bright[i] = true;
            }
        }
    }
    marked.iter().any(|on| *on).then_some(Marks {
        cells: marked,
        bright,
    })
}

/// Soft limit: the identity near zero, approaching `limit` for large values.
fn soft_limit(v: f32, limit: f32) -> f32 {
    if limit <= 0.0 {
        return 0.0;
    }
    limit * (v / limit).tanh()
}

/// The tone and colour under each mark, from unmarked selected skin at the smallest of three
/// neighbourhoods that has enough of it; where none has, the robust reference, which follows
/// the light rather than reaching for whatever clean skin is farther away.
fn nearby_skin(
    fine: &Lab,
    alpha: &[f32],
    mark: &[f32],
    reference: &Lab,
    rect: Rect,
    r: f32,
) -> Lab {
    let clean: Vec<f32> = alpha
        .iter()
        .zip(mark)
        .map(|(a, m)| a * (1.0 - m.min(1.0)).powi(2))
        .collect();
    let n = rect.len();
    let mut out = Lab::zeros(n);
    let mut filled = vec![0.0_f32; n];
    for step in 0..3_i32 {
        let around = Weighted::new(&clean, rect.w, rect.h, px(r, 1.5 * 2.0_f32.powi(step)));
        let take: Vec<f32> = around
            .support()
            .iter()
            .zip(&filled)
            .map(|(support, done)| smoothstep(0.08, 0.35, *support) * (1.0 - done))
            .collect();
        let means = fine.mean(&around);
        for (plane, mean) in [&mut out.l, &mut out.a, &mut out.b]
            .into_iter()
            .zip(means.planes())
        {
            for ((value, m), share) in plane.iter_mut().zip(mean).zip(&take) {
                *value += m * share;
            }
        }
        for (done, share) in filled.iter_mut().zip(&take) {
            *done += share;
        }
    }
    for (plane, fallback) in [&mut out.l, &mut out.a, &mut out.b]
        .into_iter()
        .zip(reference.planes())
    {
        for ((value, fallback), done) in plane.iter_mut().zip(fallback).zip(&filled) {
            *value += fallback * (1.0 - done.min(1.0));
        }
    }
    out
}

/// One find-and-rebuild pass. Returns whether anything was found. `touched`, when given, is a
/// frame-sized plane that receives the largest weight each pixel was rebuilt with.
fn pass(
    rgb: &mut [f32],
    w: usize,
    edit: &Edit,
    field: &Field,
    mut touched: Option<&mut [f32]>,
) -> bool {
    let rect = field.rect;
    let r = field.radius;
    // Pore-scale smoothing first, so a single bright or dark pore is never a mark.
    let pores = Weighted::new(&field.reference, rect.w, rect.h, px(r, 0.35));
    let fine = field.lab.mean(&pores);
    let threshold = threshold(edit.sensitivity.unwrap_or(0.5), field.brush);
    let (reference, departure) = robust(field, &fine, threshold);
    let Some(Marks {
        cells: core,
        bright,
    }) = marks(field, &fine, &departure, threshold, edit.keep_dark_marks)
    else {
        return false;
    };
    // May a rebuilt cell come out darker than it is? Only on and right around a bright mark.
    let lower: Vec<bool> = distance_to(&bright, rect.w, rect.h)
        .iter()
        .map(|d| *d <= r * 0.5 + 1.0)
        .collect();
    // The lesion sits inside the fully rebuilt core; the feather lies on healthy skin.
    let grow = r * 0.5 + 1.0;
    let feather = r * 0.9 + 1.0;
    let reach: Vec<f32> = distance_to(&core, rect.w, rect.h)
        .iter()
        .zip(&field.alpha)
        .map(|(d, a)| (1.0 - smoothstep(grow, grow + feather, *d)) * a)
        .collect();
    // Softened by a pixel, and never past the selection: an unselected pixel keeps its bytes.
    let reach: Vec<f32> = blur(&reach, rect.w, rect.h, 1)
        .into_iter()
        .zip(&field.alpha)
        .map(|(v, a)| if *a > 0.0 { v } else { 0.0 })
        .collect();
    // Ordinary pore contrast on unmarked skin; relief far beyond it under a mark is the
    // mark's own lit rim and dark core.
    let clean: Vec<f32> = field
        .alpha
        .iter()
        .zip(&reach)
        .map(|(a, m)| if *m > 0.0 { 0.0 } else { *a })
        .collect();
    let detail: [Vec<f32>; 3] = std::array::from_fn(|c| {
        field.lab.planes()[c]
            .iter()
            .zip(fine.planes()[c])
            .map(|(v, f)| v - f)
            .collect()
    });
    let normal = [
        spread(&detail[0], &clean, 0.004) * RELIEF_LIMIT,
        spread(&detail[1], &clean, 0.001) * RELIEF_LIMIT,
        spread(&detail[2], &clean, 0.001) * RELIEF_LIMIT,
    ];
    let keep = edit.texture.clamp(0.0, 1.0);
    let strength = edit.tone.clamp(0.0, 1.0) * edit.amount.clamp(0.0, 1.0);
    // The robust reference finds marks; what a mark becomes is the unmarked skin right
    // around it, so a mark in a shadow is rebuilt as shadowed skin and the light across a
    // nose keeps its gradient.
    let mut rebuilt = nearby_skin(&fine, &field.alpha, &reach, &reference, rect, r);
    // Never brighter or less coloured than the brighter, calmer quarter of the skin right
    // there: where marks are dense in a shadow, the nearest clean skin can be a lit cheek away,
    // and rebuilding from it paints a pale patch into the shadow.
    let selected: Vec<bool> = field.alpha.iter().map(|a| *a > 0.5).collect();
    let local = |values: &[f32], share: f32| {
        let level = local_percentile(
            values,
            &selected,
            rect.w,
            rect.h,
            px(r, 2.0),
            px(r, 0.5),
            share,
        );
        blur(&level.level, rect.w, rect.h, px(r, 0.5))
    };
    let (ceiling, floor_a, floor_b) = (
        local(&fine.l, 0.75),
        local(&fine.a, 0.25),
        local(&fine.b, 0.25),
    );
    // A bright head is brought down to the skin around it, never below its ordinary level.
    let floor_l = local(&fine.l, 0.45);
    for i in 0..rect.len() {
        rebuilt.l[i] = rebuilt.l[i].min(ceiling[i]);
        rebuilt.a[i] = rebuilt.a[i].max(floor_a[i]);
        rebuilt.b[i] = rebuilt.b[i].max(floor_b[i]);
    }
    let refs = rebuilt.planes();
    let own = field.lab.planes();
    for (i, (x, y)) in rect.cells().enumerate() {
        let weight = reach[i].min(1.0) * strength;
        if weight <= 0.0 {
            continue;
        }
        if let Some(plane) = touched.as_deref_mut() {
            plane[y * w + x] = plane[y * w + x].max(reach[i].min(1.0));
        }
        let healed: [f32; 3] = std::array::from_fn(|c| {
            // A red mark is not made darker than it is: its colour is the blemish, and a mark
            // that is no darker than the skin keeps its own light.
            let level = match (c, lower[i]) {
                (0, false) => refs[0][i].max(fine.l[i]),
                (0, true) => refs[0][i].max(floor_l[i].min(fine.l[i])),
                _ => refs[c][i],
            };
            let d = detail[c][i];
            level + keep * d + (1.0 - keep) * soft_limit(d, normal[c])
        });
        let old: [f32; 3] = std::array::from_fn(|c| own[c][i]);
        let lab: [f32; 3] = std::array::from_fn(|c| old[c] + weight * (healed[c] - old[c]));
        let at = (y * w + x) * 3;
        rgb[at..at + 3].copy_from_slice(&from_lab(lab));
    }
    true
}

/// Flat red or brown blotches moved part of the way toward the clean skin around them.
///
/// The colour is what is measured: a blotch is redder or browner than the skin around it, and a
/// shadow is not. A blotch that is also darker is lifted by the same share as its colour is
/// moved - only the colour of a dark red mark would leave a grey one - and nothing that is
/// only darker is touched at all.
fn even_redness(rgb: &mut [f32], w: usize, edit: &Edit, field: &Field) {
    let rect = field.rect;
    let r = field.radius;
    let mid = Weighted::new(&field.reference, rect.w, rect.h, px(r, 1.0));
    let fine = field.lab.mean(&mid);
    let selected: Vec<bool> = field.reference.iter().map(|a| *a > 0.5).collect();
    // Clean skin a blotch's width away, on the same side of any shadow edge as the blotch.
    let level = |counted: &[bool], shares: [f32; 3]| {
        let mut out = Lab::zeros(rect.len());
        for ((plane, values), share) in [&mut out.l, &mut out.a, &mut out.b]
            .into_iter()
            .zip(fine.planes())
            .zip(shares)
        {
            // Four mark radii where that holds clean skin; inside a blotch wider than that,
            // nine, so its middle is compared with the skin around it rather than with itself.
            let near = local_percentile(
                values,
                counted,
                rect.w,
                rect.h,
                px(r, 4.0),
                px(r, 0.8),
                share,
            );
            let far = local_percentile(
                values,
                counted,
                rect.w,
                rect.h,
                px(r, 9.0),
                px(r, 2.0),
                share,
            );
            *plane = near
                .level
                .iter()
                .zip(&near.support)
                .zip(&far.level)
                .map(|((n, support), f)| {
                    let trust = smoothstep(0.1, 0.3, *support);
                    n * trust + f * (1.0 - trust)
                })
                .collect();
        }
        soften(&out, counted, rect, r)
    };
    let excess = |base: &Lab| -> (Vec<f32>, Vec<f32>) {
        (
            fine.a.iter().zip(&base.a).map(|(f, b)| f - b).collect(),
            fine.b.iter().zip(&base.b).map(|(f, b)| f - b).collect(),
        )
    };
    let first = level(&selected, [0.5, 0.4, 0.4]);
    let weights: Vec<f32> = selected.iter().map(|on| f32::from(u8::from(*on))).collect();
    let (da, db) = excess(&first);
    let (sa, sb) = (spread(&da, &weights, 0.002), spread(&db, &weights, 0.002));
    let clean: Vec<bool> = (0..rect.len())
        .map(|i| selected[i] && (da[i] / sa).hypot(db[i] / sb) < 1.5)
        .collect();
    let base = level(&clean, [0.5, 0.5, 0.5]);
    let (da, db) = excess(&base);
    let weights: Vec<f32> = clean.iter().map(|on| f32::from(u8::from(*on))).collect();
    let (sa, sb) = (spread(&da, &weights, 0.002), spread(&db, &weights, 0.002));
    // Only what lies beyond a robust spread of this skin's own colour variation is evened, in
    // proportion to how far beyond it is: the correction is as smooth as the colour it
    // corrects, so it cannot draw the outline of a patch. Skin warms where it turns away from
    // the light at the edge of the face, so the evening fades out near the selection's edge.
    let rim = rim_distance(&field.alpha, rect.w, rect.h);
    let strength = 0.7 * edit.tone.clamp(0.0, 1.0) * edit.amount.clamp(0.0, 1.0);
    for (i, (x, y)) in rect.cells().enumerate() {
        let weight = field.alpha[i] * smoothstep(r * 1.5, r * 4.0, rim[i]) * strength;
        if weight <= 0.0 {
            continue;
        }
        let cut_a = (da[i] - sa).max(0.0);
        let cut_b = (db[i] - sb).max(0.0);
        if cut_a <= 0.0 && cut_b <= 0.0 {
            continue;
        }
        // A blotch that is also darker is lifted by the share of its colour that goes, so a
        // dark red mark the rebuild left does not turn grey. Only-darker skin is never lifted.
        let share = (cut_a / da[i].max(1e-6))
            .max(cut_b / db[i].max(1e-6))
            .min(1.0);
        let lift = (base.l[i] - fine.l[i]).clamp(0.0, 0.3) * share;
        let at = (y * w + x) * 3;
        let [l, a, b] = to_lab([rgb[at], rgb[at + 1], rgb[at + 2]]);
        let value = from_lab([l + weight * lift, a - weight * cut_a, b - weight * cut_b]);
        rgb[at..at + 3].copy_from_slice(&value);
    }
}

fn rebuild(
    rgb: &mut [f32],
    w: usize,
    h: usize,
    edit: &Edit,
    coverage: &Coverage,
    mut touched: Option<&mut [f32]>,
) {
    for _ in 0..PASSES {
        let Some(field) = Field::new(rgb, w, h, edit, coverage) else {
            return;
        };
        if !pass(rgb, w, edit, &field, touched.as_deref_mut()) {
            break;
        }
    }
}

pub(crate) fn apply(rgb: &mut [f32], w: usize, h: usize, edit: &Edit, coverage: &Coverage) {
    rebuild(rgb, w, h, edit, coverage, None);
    if edit.preserve_microtexture {
        if let Some(field) = Field::new(rgb, w, h, edit, coverage) {
            even_redness(rgb, w, edit, &field);
        }
    }
}

/// Where acne clear rebuilds a mark, over the whole frame: 0 for a pixel no mark reached, up
/// to 1 inside one. Found by running the rebuild on a copy; the redness evening, which moves
/// colour across whole blotches, is not shown.
pub(crate) fn mark_plane(
    rgb: &[f32],
    w: usize,
    h: usize,
    edit: &Edit,
    coverage: &Coverage,
) -> Vec<f32> {
    let mut touched = vec![0.0; w * h];
    let mut scratch = rgb.to_vec();
    rebuild(&mut scratch, w, h, edit, coverage, Some(&mut touched));
    touched
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lab_round_trips() {
        for p in [
            [0.2, 0.1, 0.05],
            [0.5, 0.5, 0.5],
            [0.01, 0.02, 0.03],
            [0.9, 0.4, 0.2],
        ] {
            let back = from_lab(to_lab(p));
            for c in 0..3 {
                assert!((back[c] - p[c]).abs() < 1e-5, "{p:?} -> {back:?}");
            }
        }
    }
}
