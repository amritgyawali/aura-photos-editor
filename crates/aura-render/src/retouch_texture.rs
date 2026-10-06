//! Texture graft: bring fine skin texture back to an even, natural level with real pores.
//!
//! Healing and smoothing both cost texture, and they cost it unevenly: a repaired spot is
//! flatter than the skin beside it, and the skin beside it still carries every glint it had.
//! The result reads as patches. This operation evens the two out, in that order:
//!
//! 1. **Limit.** Fine relief far above this skin's own normal pore contrast - the bright
//!    ridges of an oily highlight - is compressed toward the normal range. Dark relief is
//!    limited only at the extreme, so a stray hair is never half erased.
//! 2. **Measure.** The texture level a region *should* have is read from the photograph as it
//!    was before any retouch operation ran, capped at the selection's typical level so a patch
//!    of rough or blemished skin does not ask for roughness back, and averaged widely so a
//!    region that was out of focus stays out of focus.
//! 3. **Graft.** Where the current level falls short, pore detail is borrowed from clean skin
//!    elsewhere in the same selection of the same photograph, placed as overlapping tiles so
//!    no seam and no repeat shows, and scaled to supply exactly the missing energy.
//!
//! The graft multiplies luminance, so it follows the light it lands in and changes no colour.
//! Nothing is synthesised: every pore written here was photographed on this person. ADR-0090.
// Every plane covers one bounded rectangle and every index is derived from it.
#![allow(clippy::indexing_slicing)]
use crate::retouch_mask::Coverage;
use crate::retouch_planes::{centre_and_spread, luma, quantile, smoothstep, Rect, Weighted};
use aura_recipe::retouch_tools::Edit;

/// Bright relief beyond this many robust spreads is a glint rather than a pore.
const GLINT_LIMIT: f32 = 2.4;
/// Dark relief is limited a little later than bright relief: an open pore is darker than
/// a glint is bright, and a stray hair should fade evenly rather than break up.
const PIT_LIMIT: f32 = 3.2;
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
#[allow(clippy::too_many_lines)]
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
    let local = Weighted::new(alpha, w, h, px(radius, 4.0));
    let fine_of = |plane: &[f32]| -> Vec<f32> {
        pore.mean(plane)
            .iter()
            .zip(plane)
            .map(|(low, v)| v - low)
            .collect()
    };
    let level_of = |fine: &[f32]| -> Vec<f32> {
        let squared: Vec<f32> = fine.iter().map(|v| v * v).collect();
        local
            .mean(&squared)
            .iter()
            .map(|v| v.max(0.0).sqrt())
            .collect()
    };
    let fine_original = fine_of(original);
    let level_original = level_of(&fine_original);
    let Some((_, normal)) = centre_and_spread(&mut sampled(&fine_original, alpha, 0.6)) else {
        return vec![0.0; n];
    };
    if normal <= 1e-6 {
        return vec![0.0; n];
    }
    // 1. Limit relief the selection's own pores never reach.
    let glints = edit.tone.clamp(0.0, 1.0);
    let fine_now = fine_of(now);
    let limited: Vec<f32> = fine_now
        .iter()
        .map(|v| {
            let limit = if *v > 0.0 { GLINT_LIMIT } else { PIT_LIMIT } * normal;
            v + glints * (knee(*v, limit) - v)
        })
        .collect();
    let level_now = level_of(&limited);
    // 2. The level each region should have: the original, capped and widely averaged.
    let mut levels = sampled(&level_original, alpha, 0.6);
    let typical = quantile(&mut levels, CLEAN_SHARE).unwrap_or(normal);
    let capped: Vec<f32> = level_original.iter().map(|v| v.min(typical)).collect();
    let regional = Weighted::new(alpha, w, h, px(radius, 24.0)).mean(&capped);
    // 3. Clean donors: ordinary texture, no blotch, mark, glint or edge underneath.
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
    let grafted = tiles(&detail, &clean, rect, radius, manual);
    let wanted = edit.texture.clamp(0.0, 2.0);
    (0..n)
        .map(|i| {
            let target = regional[i] * wanted;
            let missing = (target * target - level_now[i] * level_now[i])
                .max(0.0)
                .sqrt();
            // Below a tenth of the target the difference is measurement noise.
            let missing = missing * smoothstep(0.1, 0.3, missing / target.max(1e-6));
            (limited[i] - fine_now[i] + missing * grafted[i]).clamp(-MAX_CHANGE, MAX_CHANGE)
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
