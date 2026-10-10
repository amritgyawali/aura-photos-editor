//! Texture restore: put the skin's own pores back after healing and smoothing.
//!
//! A retoucher who smooths skin on a frequency-separated layer leaves the high band, the
//! pores, untouched. Smoothing operators that are not separated, and several of them stacked,
//! still cost fine detail, and the face reads as plastic: no pores on the nose, no grain on
//! the cheeks. This operation runs last and restores that detail from the photograph itself:
//!
//! 1. **Own detail, in place.** For every cell, the target is the fine detail the photograph
//!    had at that very cell before any retouch operation ran. Pores come back exactly where
//!    they were, at the strength they had; nothing is moved or repeated.
//! 2. **Limit.** That detail is first compressed where it goes far beyond this skin's own
//!    pore contrast - the bright ridges of an oily highlight, the deepest pits - with a knee,
//!    so ordinary pores pass through unchanged.
//! 3. **Borrow only where a mark was.** Where an earlier operation rebuilt the tone - a healed
//!    blemish - the photograph's own detail there was the blemish's rim, so detail is borrowed
//!    from clean skin in the same selection instead, as overlapping tiles scaled to the level
//!    this skin's smoother skin has.
//!
//! The change multiplies luminance, so it follows the light it lands in and changes no colour.
//! Nothing is synthesised: every pore written here was photographed on this person. ADR-0090.
// Every plane covers one bounded rectangle and every index is derived from it.
#![allow(clippy::indexing_slicing)]
use crate::retouch_mask::Coverage;
use crate::retouch_planes::{centre_and_spread, luma, quantile, smoothstep, Rect, Weighted};
use aura_recipe::retouch_tools::Edit;

/// Bright relief beyond this many robust spreads is a glint rather than a pore.
const GLINT_LIMIT: f32 = 2.0;
/// Dark relief is limited a little later than bright relief: an open pore is darker than
/// a glint is bright, and a stray hair should fade evenly rather than break up.
const PIT_LIMIT: f32 = 2.6;
/// The level a region is asked to have is capped at this share of the selection's own
/// distribution: the smoother third of a face is what its clean skin looks like.
const CLEAN_SHARE: f32 = 0.35;
/// The largest luminance change a graft may make, in natural-log units (about 0.4 stops).
const MAX_CHANGE: f32 = 0.28;
/// A tile is this many pore radii wide.
const TILE: f32 = 12.0;
/// How many donor positions are tried for each tile.
const DONOR_TRIES: u32 = 40;
/// How many samples the robust statistics are measured from.
const SAMPLES: usize = 60_000;
/// The scale, in pore radii, at which a rebuilt mark is told apart from smoothing: a change
/// at this scale that the same change four times wider does not explain.
const HEAL_SCALE: f32 = 3.0;
/// That localized change of tone, in natural-log units, below which the photograph's own
/// detail is restored, and above which detail is borrowed instead.
const HEAL_LOW: f32 = 0.04;
const HEAL_HIGH: f32 = 0.1;
/// How far past a rebuilt mark, in pore radii, detail is still borrowed rather than restored.
const HEAL_RIM: f32 = 2.0;

fn px(radius: f32, factor: f32) -> usize {
    (radius * factor).round().max(1.0) as usize
}

fn radius_of(edit: &Edit, w: usize, h: usize) -> f32 {
    (edit.radius * w.min(h) as f32).max(1.0)
}

fn margin(radius: f32) -> usize {
    px(radius, TILE * 2.0) + 2
}

fn log_luma(rgb: &[f32], w: usize, rect: Rect) -> Vec<f32> {
    rect.cells()
        .map(|(x, y)| {
            let i = (y * w + x) * 3;
            luma([rgb[i], rgb[i + 1], rgb[i + 2]]).max(1e-5).ln()
        })
        .collect()
}

/// The photograph's luminance before any retouch operation ran, over this operation's
/// rectangle: what "the texture this skin had" is measured from.
#[derive(Debug)]
pub(crate) struct Reference {
    rect: Rect,
    log_luma: Vec<f32>,
}

impl Reference {
    pub(crate) fn capture(
        rgb: &[f32],
        w: usize,
        h: usize,
        edit: &Edit,
        bounds: [usize; 4],
    ) -> Option<Self> {
        let rect = Rect::around(bounds, margin(radius_of(edit, w, h)), w, h)?;
        Some(Self {
            rect,
            log_luma: log_luma(rgb, w, rect),
        })
    }

    fn matches(&self, rect: Rect) -> bool {
        self.rect.x0 == rect.x0
            && self.rect.y0 == rect.y0
            && self.rect.w == rect.w
            && self.rect.h == rect.h
    }
}

fn sampled(values: &[f32], alpha: &[f32], least: f32) -> Vec<f32> {
    let step = (values.len() / SAMPLES).max(1);
    values
        .iter()
        .zip(alpha)
        .step_by(step)
        .filter(|(_, a)| **a > least)
        .map(|(v, _)| *v)
        .collect()
}

/// The identity up to 60 % of `limit`, then a smooth approach to `limit`: ordinary pore
/// contrast passes through exactly and only relief beyond it is compressed.
fn knee(v: f32, limit: f32) -> f32 {
    let free = limit * 0.6;
    let size = v.abs();
    if size <= free || limit <= 0.0 {
        return v;
    }
    let room = limit - free;
    (free + room * ((size - free) / room).tanh()).copysign(v)
}

/// A small integer mix; the tiles need positions that look unrelated, not randomness.
fn mix(a: u32, b: u32, c: u32, d: u32) -> u32 {
    let mut v = a
        .wrapping_mul(0x9E37_79B1)
        .wrapping_add(b.wrapping_mul(0x85EB_CA77))
        .wrapping_add(c.wrapping_mul(0xC2B2_AE3D))
        .wrapping_add(d.wrapping_mul(0x27D4_EB2F));
    v ^= v >> 15;
    v = v.wrapping_mul(0x2C1B_3C6D);
    v ^= v >> 12;
    v = v.wrapping_mul(0x297A_2D39);
    v ^ (v >> 15)
}

fn unit(v: u32) -> f32 {
    (v >> 8) as f32 / 16_777_216.0
}

/// Summed-area table of a 0/1 plane, one row and column larger than the plane.
struct Area {
    sums: Vec<u32>,
    w: usize,
}

impl Area {
    fn new(flags: &[bool], w: usize, h: usize) -> Self {
        let mut sums = vec![0_u32; (w + 1) * (h + 1)];
        for y in 0..h {
            let mut row = 0_u32;
            for x in 0..w {
                row += u32::from(flags[y * w + x]);
                sums[(y + 1) * (w + 1) + x + 1] = sums[y * (w + 1) + x + 1] + row;
            }
        }
        Self { sums, w: w + 1 }
    }

    /// How many flagged cells the half-open box holds.
    fn count(&self, x0: usize, y0: usize, x1: usize, y1: usize) -> u32 {
        self.sums[y1 * self.w + x1] + self.sums[y0 * self.w + x0]
            - self.sums[y0 * self.w + x1]
            - self.sums[y1 * self.w + x0]
    }
}

/// Unit-strength pore detail for every cell, assembled from clean donor tiles.
///
/// Four half-offset lattices of raised-cosine tiles sum to one everywhere, and dividing by the
/// root of the summed squared weights keeps the texture's strength where tiles overlap.
fn tiles(
    detail: &[f32],
    clean: &[bool],
    rect: Rect,
    radius: f32,
    manual: Option<[f32; 2]>,
) -> Vec<f32> {
    let Rect { w, h, .. } = rect;
    let half = (px(radius, TILE) / 2).max(4);
    let size = half * 2;
    let area = Area::new(clean, w, h);
    // Nearly all of a donor tile must be clean: a crease or the edge of a nostril crossing
    // one would be copied wherever the tile lands.
    let needed = ((size * size) as f32 * 0.97) as u32;
    let mut sum = vec![0.0_f32; w * h];
    let mut squares = vec![0.0_f32; w * h];
    for lattice in 0..4_u32 {
        let shift = [
            if lattice & 1 == 1 { half } else { 0 },
            if lattice & 2 == 2 { half } else { 0 },
        ];
        let mut row = 0_u32;
        let mut top = shift[1];
        while top < h + size {
            let mut column = 0_u32;
            let mut left = shift[0];
            while left < w + size {
                // The tile covers `left - size .. left`, `top - size .. top`, clipped.
                let centre = [left as f32 - half as f32, top as f32 - half as f32];
                let donor = (0..DONOR_TRIES).find_map(|attempt| {
                    let turn = unit(mix(lattice, column, row, attempt * 2)) * std::f32::consts::TAU;
                    let spread = unit(mix(lattice, column, row, attempt * 2 + 1));
                    let (cx, cy) = if let Some(p) = manual {
                        // A chosen source: stay within a few tiles of it.
                        let reach = size as f32 * 2.5 * spread;
                        (p[0] + turn.cos() * reach, p[1] + turn.sin() * reach)
                    } else {
                        // Otherwise prefer nearby skin, whose pores are the same size and
                        // run the same way, and look farther only when it is not clean.
                        let reach =
                            size as f32 * (1.0 + attempt as f32 * 0.22) * (0.6 + 0.8 * spread);
                        (
                            centre[0] + turn.cos() * reach,
                            centre[1] + turn.sin() * reach,
                        )
                    };
                    let x0 = cx.round() as i64 - half as i64;
                    let y0 = cy.round() as i64 - half as i64;
                    if x0 < 0 || y0 < 0 {
                        return None;
                    }
                    let (x0, y0) = (x0 as usize, y0 as usize);
                    (x0 + size <= w
                        && y0 + size <= h
                        && area.count(x0, y0, x0 + size, y0 + size) >= needed)
                        .then_some([x0, y0])
                });
                if let Some(origin) = donor {
                    for ty in 0..size {
                        let Some(y) = (top + ty).checked_sub(size).filter(|y| *y < h) else {
                            continue;
                        };
                        let wy = ((ty as f32 + 0.5) / size as f32 * std::f32::consts::PI)
                            .sin()
                            .powi(2);
                        for tx in 0..size {
                            let Some(x) = (left + tx).checked_sub(size).filter(|x| *x < w) else {
                                continue;
                            };
                            let wx = ((tx as f32 + 0.5) / size as f32 * std::f32::consts::PI)
                                .sin()
                                .powi(2);
                            let weight = wx * wy;
                            let i = y * w + x;
                            sum[i] += weight * detail[(origin[1] + ty) * w + origin[0] + tx];
                            squares[i] += weight * weight;
                        }
                    }
                }
                left += size;
                column += 1;
            }
            top += size;
            row += 1;
        }
    }
    sum.iter()
        .zip(&squares)
        .map(|(v, s)| if *s > 1e-4 { v / s.sqrt() } else { 0.0 })
        .collect()
}

/// The change to log luminance for every cell of the rectangle, before strength.
/// Pore detail borrowed from clean skin, and the level it should be scaled to, for the
/// cells where an earlier operation rebuilt the tone and the photograph's own detail there
/// belonged to the mark.
fn borrowed(
    fine_original: &[f32],
    original: &[f32],
    alpha: &[f32],
    rect: Rect,
    radius: f32,
    normal: f32,
    manual: Option<[f32; 2]>,
) -> (Vec<f32>, Vec<f32>) {
    let Rect { w, h, .. } = rect;
    let n = rect.len();
    let local = Weighted::new(alpha, w, h, px(radius, 4.0));
    let squared: Vec<f32> = fine_original.iter().map(|v| v * v).collect();
    let level_original: Vec<f32> = local
        .mean(&squared)
        .iter()
        .map(|v| v.max(0.0).sqrt())
        .collect();
    // The level a rebuilt region is given: the original, capped at the smoother share of
    // this skin and widely averaged, so a blemished patch does not ask for its roughness back.
    let mut levels = sampled(&level_original, alpha, 0.6);
    let typical = quantile(&mut levels, CLEAN_SHARE).unwrap_or(normal);
    let capped: Vec<f32> = level_original.iter().map(|v| v.min(typical)).collect();
    let regional = Weighted::new(alpha, w, h, px(radius, 24.0)).mean(&capped);
    // Clean donors: ordinary texture, no blotch, mark, glint or edge underneath.
    let pore = Weighted::new(alpha, w, h, px(radius, 1.0));
    let broad = Weighted::new(alpha, w, h, px(radius, 5.0)).mean(original);
    let mid: Vec<f32> = pore
        .mean(original)
        .iter()
        .zip(&broad)
        .map(|(a, b)| a - b)
        .collect();
    let mid_limit = centre_and_spread(&mut sampled(&mid, alpha, 0.6))
        .map_or(f32::MAX, |(_, s)| (s * 2.5).max(0.01));
    let clean: Vec<bool> = (0..n)
        .map(|i| {
            alpha[i] > 0.7
                && level_original[i] > typical * 0.5
                && level_original[i] < typical * 1.25
                && mid[i].abs() < mid_limit
                && fine_original[i].abs() < normal * 3.5
        })
        .collect();
    let detail: Vec<f32> = fine_original
        .iter()
        .zip(&level_original)
        .map(|(v, level)| v / level.max(typical * 0.3))
        .collect();
    (tiles(&detail, &clean, rect, radius, manual), regional)
}

/// The change to log luminance for every cell of the rectangle, before strength.
///
/// The target for every cell is the fine detail the photograph had **at that cell** before
/// any retouch operation ran, with glints and pits limited to this skin's own pore range.
/// Healing and smoothing that removed pores therefore get exactly those pores back, in the
/// same place, and skin that was left alone only has its glints limited. Only where an
/// earlier operation rebuilt the tone - a healed blemish, whose own detail was the mark's
/// rim - is detail borrowed from clean skin instead.
fn plan(
    now: &[f32],
    original: &[f32],
    alpha: &[f32],
    rect: Rect,
    radius: f32,
    edit: &Edit,
    manual: Option<[f32; 2]>,
) -> Vec<f32> {
    let Rect { w, h, .. } = rect;
    let n = rect.len();
    let pore = Weighted::new(alpha, w, h, px(radius, 1.0));
    let fine_of = |plane: &[f32]| -> Vec<f32> {
        pore.mean(plane)
            .iter()
            .zip(plane)
            .map(|(low, v)| v - low)
            .collect()
    };
    let fine_original = fine_of(original);
    let Some((_, normal)) = centre_and_spread(&mut sampled(&fine_original, alpha, 0.6)) else {
        return vec![0.0; n];
    };
    if normal <= 1e-6 {
        return vec![0.0; n];
    }
    let fine_now = fine_of(now);
    let glints = edit.tone.clamp(0.0, 1.0);
    let limit = |v: f32| {
        let bound = if v > 0.0 { GLINT_LIMIT } else { PIT_LIMIT } * normal;
        v + glints * (knee(v, bound) - v)
    };
    // Where an earlier operation rebuilt a mark, the tone moved a lot in a small place. Dodge
    // and burn, tone and light evening move it too, but across a whole region: the change is
    // band-passed so only a change about the size of a mark counts.
    let moved: Vec<f32> = now.iter().zip(original).map(|(a, b)| a - b).collect();
    let near = Weighted::new(alpha, w, h, px(radius, HEAL_SCALE)).mean(&moved);
    let wide = Weighted::new(alpha, w, h, px(radius, HEAL_SCALE * 4.0)).mean(&moved);
    let found: Vec<f32> = near
        .iter()
        .zip(&wide)
        .map(|(a, b)| smoothstep(HEAL_LOW, HEAL_HIGH, (a - b).abs()))
        .collect();
    // The rim of a rebuilt mark - a raised ring, the edge of a pustule - is where the tone
    // changed least and the photograph's own detail is most the mark's. Borrow across it too.
    let healed: Vec<f32> = Weighted::new(alpha, w, h, px(radius, HEAL_RIM))
        .mean(&found)
        .iter()
        .zip(&found)
        .map(|(around, own)| own.max((around * 2.0).min(1.0)))
        .collect();
    // After acne clear (ADR-0092) a healed mark already kept this skin's own pores, so nothing
    // is borrowed: the mark is left as it was healed and only skin that smoothing flattened
    // gets its own detail back.
    let keep_healed = edit.preserve_microtexture;
    let donors = (!keep_healed && healed.iter().any(|v| *v > 0.0)).then(|| {
        borrowed(
            &fine_original,
            original,
            alpha,
            rect,
            radius,
            normal,
            manual,
        )
    });
    let wanted = edit.texture.clamp(0.0, 2.0);
    (0..n)
        .map(|i| {
            let own = limit(fine_original[i]) * wanted;
            let target = match &donors {
                Some((grafted, regional)) if healed[i] > 0.0 => {
                    let lent = limit(grafted[i] * regional[i]) * wanted;
                    own + healed[i] * (lent - own)
                }
                None if keep_healed => own + healed[i] * (fine_now[i] - own),
                _ => own,
            };
            (target - fine_now[i]).clamp(-MAX_CHANGE, MAX_CHANGE)
        })
        .collect()
}

pub(crate) fn apply(
    rgb: &mut [f32],
    w: usize,
    h: usize,
    edit: &Edit,
    coverage: &Coverage,
    reference: Option<&Reference>,
) {
    let radius = radius_of(edit, w, h);
    let Some(rect) = Rect::around(coverage.bounds, margin(radius), w, h) else {
        return;
    };
    let alpha: Vec<f32> = rect.cells().map(|(x, y)| coverage.at(x, y, w, h)).collect();
    if !alpha.iter().any(|a| *a > 0.0) {
        return;
    }
    let now = log_luma(rgb, w, rect);
    // Without the photograph as it was, the current frame is its own reference: glints are
    // still limited, and only a region flatter than its surroundings receives a graft.
    let original = reference
        .filter(|r| r.matches(rect))
        .map_or(now.as_slice(), |r| r.log_luma.as_slice());
    let manual = edit.source.map(|p| {
        [
            p[0] * w as f32 - rect.x0 as f32,
            p[1] * h as f32 - rect.y0 as f32,
        ]
    });
    let change = plan(&now, original, &alpha, rect, radius, edit, manual);
    for (((x, y), delta), a) in rect.cells().zip(change).zip(&alpha) {
        let weight = a * edit.amount;
        if weight <= 0.0 || delta.abs() < 1e-7 {
            continue;
        }
        let gain = (weight * delta).exp();
        let i = (y * w + x) * 3;
        for value in &mut rgb[i..i + 3] {
            *value *= gain;
        }
    }
}
