//! Frequency healing: find compact marks in a selection and rebuild the tone under them.
//!
//! A retoucher separates a face into a tone layer and a texture layer, paints clean skin
//! colour over each mark on the tone layer, and leaves the pores on the texture layer alone.
//! This is that, measured rather than painted:
//!
//! 1. **Find.** At three sizes, a pixel is compared with the selected skin around it - how
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

/// The three neighbourhood sizes a mark is measured against, in units of the frequency radius.
const SCALES: [f32; 3] = [1.0, 2.0, 3.2];
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
/// Under a mark, relief beyond this many robust spreads of the selection's ordinary pore
/// contrast belongs to the mark - its dark core, its lit rim - rather than to the skin.
const RELIEF_LIMIT: f32 = 1.5;
/// How much of a region's glint strength is added to its darkness threshold.
const GLINT_ALLOWANCE: f32 = 2.0;
/// Luminance above this multiple of the local mean is a glint, not the skin's own tone.
const GLINT_CLIP: f32 = 1.06;
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
        let mut alpha = Vec::with_capacity(rect.len());
        let mut channels = [
            Vec::with_capacity(rect.len()),
            Vec::with_capacity(rect.len()),
            Vec::with_capacity(rect.len()),
        ];
        let mut lum = Vec::with_capacity(rect.len());
        let mut red = Vec::with_capacity(rect.len());
        for (x, y) in rect.cells() {
            let i = (y * w + x) * 3;
            let p = [rgb[i], rgb[i + 1], rgb[i + 2]];
            alpha.push(smoothstep(0.0, FULLY_SELECTED, coverage.at(x, y, w, h)));
            for (channel, value) in channels.iter_mut().zip(p) {
                channel.push(value);
            }
            lum.push(luma(p));
            let sum = p[0].max(0.0) + p[1].max(0.0) + p[2].max(0.0);
            red.push((p[0] - (p[1] + p[2]) * 0.5) / (sum + 1e-4));
        }
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
        for sector in 0..SECTORS {
            let (mut sum_luma, mut sum_red, mut count) = (0.0_f32, 0.0_f32, 0.0_f32);
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
                    if self.alpha[i] > 0.5 {
                        sum_luma += self.luma[i];
                        sum_red += self.red[i];
                        count += 1.0;
                    }
                }
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

        valid >= SECTORS - 2 && (darker_than >= valid - 1 || redder_than >= valid - 1)
    }
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
fn marks(field: &Field, sensitivity: f32, keep_dark: bool) -> Option<Marks> {
    let Rect { w, h, .. } = field.rect;
    let n = field.rect.len();
    let r = field.radius;
    let floor = typical_luma(field) * 0.03;
    // Pore-scale smoothing first, so a single bright or dark pore is never a mark.
    let fine = Weighted::new(&field.alpha, w, h, px(r, 0.3));
    let fine_luma = fine.mean(&field.luma);
    let fine_red = fine.mean(&field.red);
    // Multiples of the selection's own spread: 3.0 at the default, 1.8 at most sensitive.
    let k = 4.2 - 2.4 * sensitivity.clamp(0.0, 1.0);
    let selected: Vec<bool> = field.alpha.iter().map(|a| *a > 0.2).collect();
    let mut marked = vec![false; n];
    // Cells of long thin structures; a stricter threshold must not split them into spots.
    let mut lines = vec![false; n];
    for scale in SCALES {
        let around = Weighted::new(&field.alpha, w, h, px(r, scale));
        // Glints lift a plain mean, and the ordinary skin between them then measures as
        // dark. Clip them just above the first estimate and average again.
        let first = around.mean(&field.luma);
        let calm: Vec<f32> = field
            .luma
            .iter()
            .zip(&first)
            .map(|(v, mean)| v.min(mean * GLINT_CLIP))
            .collect();
        let base_luma = around.mean(&calm);
        let base_red = around.mean(&field.red);
        let dark: Vec<f32> = base_luma
            .iter()
            .zip(&fine_luma)
            .map(|(base, fine)| (base - fine) / (base + floor))
            .collect();
        let redder: Vec<f32> = fine_red.iter().zip(&base_red).map(|(a, b)| a - b).collect();
        let dark_limit = (spread(&dark, &field.alpha) * k).max(0.03);
        let red_limit = (spread(&redder, &field.alpha) * k).max(0.006);
        // Where skin glints, the skin between the glints is darker than they are without
        // being a mark. How strongly a region glints raises what counts as dark there.
        let glint: Vec<f32> = dark.iter().map(|d| (-d).max(0.0).powi(2)).collect();
        let glinting = Weighted::new(&field.alpha, w, h, px(r, 6.0)).mean(&glint);
        // In units of the threshold. A bright head counts only as far as it is also redder
        // than its surroundings, so a specular pore never spends a repair.
        let score: Vec<f32> = dark
            .iter()
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
        let smallest = (0.36 * r * r).max(3.0);
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
                if peak < tier * 1.25 || bright_pore || dark_only {
                    continue;
                }
                let deepest = group.cells.iter().map(|i| dark[*i]).fold(0.0, f32::max);
                let reddest_raw = group.cells.iter().map(|i| redder[*i]).fold(0.0, f32::max);
                let shape = Surround {
                    luma: &fine_luma,
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
            .iter()
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
        for (plane, channel) in out.iter_mut().zip(&field.channels) {
            for ((value, mean), share) in plane.iter_mut().zip(reach.mean(channel)).zip(&take) {
                *value += mean * share;
            }
        }
        for (done, share) in filled.iter_mut().zip(&take) {
            *done += share;
        }
    }
    // Cells nothing reached keep their own tone rather than a made-up one.
    let tone = Weighted::new(&field.alpha, w, h, px(field.radius, 1.0));
    for (plane, channel) in out.iter_mut().zip(&field.channels) {
        for ((value, own), done) in plane.iter_mut().zip(tone.mean(channel)).zip(&filled) {
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
/// frame-sized plane that receives the largest weight each pixel was rebuilt with.
fn pass(
    rgb: &mut [f32],
    w: usize,
    h: usize,
    edit: &Edit,
    coverage: &Coverage,
    mut touched: Option<&mut [f32]>,
) -> bool {
    let Some(field) = Field::new(rgb, w, h, edit, coverage) else {
        return false;
    };
    let rect = field.rect;
    let Some(Marks { weight: mark, core }) = marks(
        &field,
        edit.sensitivity.unwrap_or(0.5),
        edit.keep_dark_marks,
    ) else {
        return false;
    };
    let rebuilt = rebuilt_tone(&field, &mark);
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
    let low: [Vec<f32>; 3] = std::array::from_fn(|c| {
        let within = own.mean(&field.channels[c]);
        around
            .mean(&field.channels[c])
            .iter()
            .zip(within)
            .zip(&lesion)
            .map(|((skin, within), share)| skin + share.min(1.0) * (within - skin))
            .collect()
    });
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
    for (i, (x, y)) in rect.cells().enumerate() {
        let reach = smooth_mark[i].min(1.0) * field.alpha[i];
        let weight = reach * edit.amount;
        if weight <= 0.0 {
            continue;
        }
        if let Some(plane) = touched.as_deref_mut() {
            plane[y * w + x] = plane[y * w + x].max(reach);
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
        let at = (y * w + x) * 3;
        for c in 0..3 {
            let healed = (new_low[c] + (old[c] - old_low[c]) * gain).max(0.0);
            rgb[at + c] = old[c] + weight * (healed - old[c]);
        }
    }
    true
}

/// Find and rebuild, then once more on the result: with the strongest marks gone, the skin
/// between them is a truer reference, and fainter marks beside them measure as marks.
pub(crate) fn apply(rgb: &mut [f32], w: usize, h: usize, edit: &Edit, coverage: &Coverage) {
    for _ in 0..PASSES {
        if !pass(rgb, w, h, edit, coverage, None) {
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
    for _ in 0..PASSES {
        if !pass(&mut scratch, w, h, edit, coverage, Some(&mut touched)) {
            break;
        }
    }
    touched
}
