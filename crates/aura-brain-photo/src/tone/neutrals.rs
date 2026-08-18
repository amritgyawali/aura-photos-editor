//! What in this photograph is supposed to be white.
//!
//! PHASE-15 section 6.2's fourth hypothesis generator: "known-neutral regions (dress,
//! tablecloth, paper)". A neutral surface reflects the illuminant unchanged, so a region
//! known to be neutral hands the solve the answer directly - which makes this the strongest
//! evidence this phase has and the reason [`aura_core::ToneCode::NeutralFound`] is a
//! withdrawal rather than a claim.
//!
//! ## What "known" honestly means here
//!
//! It does not mean recognised. There is no dress detector in this build and section 2.2
//! does not ask for one. What this module finds is a region with the **properties a neutral
//! surface has**, and it is explicit about the difference:
//!
//! * **large** - a wedding dress, a tablecloth or a sheet of paper occupies a contiguous
//!   area, and a two-block region that happens to be grey is a shadow;
//! * **bright, and not clipped** - a neutral surface is one of the brightest things in a
//!   frame, and a clipped one has stopped carrying its colour;
//! * **chromatically uniform** - the giveaway. A white dress under one light is one
//!   chromaticity across its whole area; a cream wall with a lamp on it is a gradient, and
//!   a pale pink bouquet is uniform in luminance and not in chroma.
//!
//! Those three are necessary and not sufficient, and this module is wrong about a beige
//! wall, a grey suit in daylight and a silver car. All three failures are in the same
//! direction - a slightly warm or slightly cool answer rather than a wild one - because the
//! candidate has to be nearly achromatic to be a candidate at all.
//!
//! ## The scene decides how much to believe it
//!
//! `SceneTarget::neutral_prior` is what turns this from a guess into evidence. Getting ready
//! has a dress and a window; a dance floor has neither, and the brightest chromatically
//! uniform region in a dance-floor frame is usually an uplighter pointed at a wall. The
//! prior scales the hypothesis's weight rather than gating it, so a genuine neutral in a
//! low-prior scene still competes - it just has to win on the skin term as well.

use aura_core::contract::integrity::CropRect;
use aura_raw::colour::illuminant::{linear_srgb_to_uv, uv_distance};

use crate::tone::stats::{CLIP_LINEAR, NOISE_FLOOR_LINEAR};

/// The side of one analysis block, in pixels of the proxy.
///
/// Thirty-two, so a 2048 px proxy is a 64x48 block map. Small enough that a tablecloth on
/// one table is several blocks; large enough that each block has a thousand pixels and its
/// chroma variance is a measurement rather than noise.
pub const BLOCK: usize = 32;

/// The fewest blocks a candidate region needs.
///
/// Six, which on a 2048 px proxy is about 0.2 % of the frame. Below that the region is a
/// highlight rather than a surface, and a highlight carries the light source's colour rather
/// than a surface's - which is the specific mistake a naive white-patch estimate makes.
pub const MIN_BLOCKS: usize = 6;

/// The most a block's own chromaticity may vary from its region's, in `u'v'`.
///
/// The uniformity test. A dress under one light holds to about 0.008 across its whole area;
/// 0.015 admits a slightly shadowed fold and still refuses a bouquet.
pub const UNIFORMITY: f32 = 0.015;

/// How far from achromatic a candidate block may sit, in `u'v'` from the frame's own
/// brightest consensus.
///
/// Deliberately generous, because under tungsten a genuinely white dress sits a long way
/// from D65 - that is the whole point of measuring it. The test is relative to the frame's
/// own bright consensus rather than to D65, so it asks "is this the same colour as the other
/// bright things here" rather than "is this white".
pub const CHROMA_SPREAD: f32 = 0.030;

/// The luminance band a candidate block sits in, as fractions of the frame's brightest
/// unclipped block.
///
/// Between 45 % and 100 %. Below 45 % the region is not one of the bright surfaces in the
/// frame and its chromaticity is as likely to be a coloured object in shade.
pub const MIN_RELATIVE_LUMA: f32 = 0.45;

/// One region that behaves like a neutral surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NeutralPatch {
    /// Its chromaticity in CIE 1976 `u'v'`, which under a neutral surface is the
    /// illuminant's own.
    pub uv: [f32; 2],
    /// Mean linear RGB.
    pub linear: [f32; 3],
    /// Mean linear luminance.
    pub luma: f32,
    /// Where it is, for the evidence crop the panel shows.
    pub area: CropRect,
    /// Fraction of the frame it covers, `0..1`.
    pub coverage: f32,
    /// How uniform it is, `0..1`. One is perfectly uniform.
    pub uniformity: f32,
}

impl NeutralPatch {
    /// How much this patch should be believed, before the scene prior, `0..1`.
    ///
    /// Three factors multiplied rather than added, so a patch that fails any one of them
    /// cannot be rescued by the other two. That is the same geometric fusion phase 09's
    /// technical score and phase 12's keep score use, and it is right for the same reason: a
    /// tiny perfectly uniform region is not evidence, and neither is a huge blotchy one.
    #[must_use]
    pub fn strength(&self) -> f32 {
        let size = (self.coverage / 0.04).clamp(0.0, 1.0);
        let bright = (self.luma / 0.45).clamp(0.0, 1.0);
        (size * self.uniformity.clamp(0.0, 1.0) * bright).clamp(0.0, 1.0)
    }
}

/// One block of the frame, summarised.
#[derive(Debug, Clone, Copy, Default)]
struct Block {
    linear: [f32; 3],
    luma: f32,
    uv: [f32; 2],
    /// Mean absolute deviation of the block's own pixel chromaticities from its mean.
    spread: f32,
    usable: bool,
    x: usize,
    y: usize,
}

/// Find the regions in this frame that behave like neutral surfaces, strongest first.
///
/// At most three are returned. A frame with four candidate regions has either a very
/// cluttered white set - in which case the strongest three say the same thing - or is a
/// frame where this heuristic is wrong, and returning nine of them would make the solve
/// slower and no better.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn find(rgb: &[u8], width: usize, height: usize, table: &[f32; 256]) -> Vec<NeutralPatch> {
    /// The most patches one frame reports.
    const MAX_PATCHES: usize = 3;

    if width < BLOCK || height < BLOCK {
        return Vec::new();
    }
    let columns = width / BLOCK;
    let rows = height / BLOCK;
    let mut blocks: Vec<Block> = Vec::with_capacity(columns * rows);
    let mut brightest = 0.0f32;

    for row in 0..rows {
        for column in 0..columns {
            let mut totals = [0.0f64; 3];
            let mut count = 0u32;
            for y in (row * BLOCK)..((row + 1) * BLOCK) {
                for x in (column * BLOCK)..((column + 1) * BLOCK) {
                    let base = (y * width + x) * 3;
                    let Some(triple) = linear_at(rgb, base, table) else {
                        continue;
                    };
                    let luma = luma_of(triple);
                    if luma >= CLIP_LINEAR || luma <= NOISE_FLOOR_LINEAR {
                        continue;
                    }
                    if triple[0] >= CLIP_LINEAR
                        || triple[1] >= CLIP_LINEAR
                        || triple[2] >= CLIP_LINEAR
                    {
                        continue;
                    }
                    for channel in 0..3 {
                        if let (Some(slot), Some(value)) =
                            (totals.get_mut(channel), triple.get(channel))
                        {
                            *slot += f64::from(*value);
                        }
                    }
                    count += 1;
                }
            }
            // Half the block has to survive the exclusions. A block that is mostly clipped
            // or mostly black has a mean, and the mean is about the quarter of it that was
            // neither - which is not what "this block is a uniform surface" means.
            let minimum = (BLOCK * BLOCK / 2) as u32;
            if count < minimum {
                blocks.push(Block {
                    x: column,
                    y: row,
                    ..Block::default()
                });
                continue;
            }
            let linear = [
                (totals[0] / f64::from(count)) as f32,
                (totals[1] / f64::from(count)) as f32,
                (totals[2] / f64::from(count)) as f32,
            ];
            let uv = linear_srgb_to_uv(linear);
            let luma = luma_of(linear);
            brightest = brightest.max(luma);
            let spread = chroma_spread(rgb, width, table, column, row, uv);
            blocks.push(Block {
                linear,
                luma,
                uv,
                spread,
                usable: true,
                x: column,
                y: row,
            });
        }
    }

    if brightest <= 0.0 {
        return Vec::new();
    }

    // The consensus: the chromaticity of the bright blocks, which under one illuminant is
    // that illuminant. A median of a small set would be better and a mean is what the budget
    // allows; the uniformity filter below is what makes the difference small.
    let mut consensus = [0.0f64; 2];
    let mut consensus_count = 0u32;
    for block in blocks.iter().filter(|b| b.usable) {
        if block.luma >= brightest * MIN_RELATIVE_LUMA {
            consensus[0] += f64::from(block.uv[0]);
            consensus[1] += f64::from(block.uv[1]);
            consensus_count += 1;
        }
    }
    if consensus_count == 0 {
        return Vec::new();
    }
    let consensus = [
        (consensus[0] / f64::from(consensus_count)) as f32,
        (consensus[1] / f64::from(consensus_count)) as f32,
    ];

    // Candidates: bright, uniform, and the same colour as the other bright things.
    let mut candidates: Vec<&Block> = blocks
        .iter()
        .filter(|block| block.usable)
        .filter(|block| block.luma >= brightest * MIN_RELATIVE_LUMA)
        .filter(|block| block.spread <= UNIFORMITY)
        .filter(|block| uv_distance(block.uv, consensus) <= CHROMA_SPREAD)
        .collect();
    if candidates.len() < MIN_BLOCKS {
        return Vec::new();
    }
    // Deterministic order before grouping. The block map is already row-major, but the
    // filter chain above is the kind of thing a refactor reorders, and a grouping whose
    // answer depends on iteration order is a grouping that is not reproducible.
    candidates.sort_by_key(|block| (block.y, block.x));

    // Group by chromaticity, greedily, in that fixed order. Not a clustering algorithm: the
    // candidates have already passed a `CHROMA_SPREAD` filter against one consensus, so at
    // most a handful of groups can exist and the first-fit answer is the same as the optimal
    // one for all but pathological inputs.
    let mut groups: Vec<Vec<&Block>> = Vec::new();
    for block in candidates {
        let mut placed = false;
        for group in &mut groups {
            let Some(first) = group.first() else { continue };
            if uv_distance(block.uv, first.uv) <= UNIFORMITY * 2.0 {
                group.push(block);
                placed = true;
                break;
            }
        }
        if !placed {
            groups.push(vec![block]);
        }
    }

    let total_blocks = (columns * rows).max(1) as f32;
    let mut patches: Vec<NeutralPatch> = groups
        .into_iter()
        .filter(|group| group.len() >= MIN_BLOCKS)
        .filter_map(|group| {
            let count = group.len() as f64;
            let mut linear = [0.0f64; 3];
            let mut luma = 0.0f64;
            let mut spread = 0.0f64;
            let (mut min_x, mut min_y, mut max_x, mut max_y) = (usize::MAX, usize::MAX, 0, 0);
            for block in &group {
                for channel in 0..3 {
                    if let (Some(slot), Some(value)) =
                        (linear.get_mut(channel), block.linear.get(channel))
                    {
                        *slot += f64::from(*value);
                    }
                }
                luma += f64::from(block.luma);
                spread += f64::from(block.spread);
                min_x = min_x.min(block.x);
                min_y = min_y.min(block.y);
                max_x = max_x.max(block.x);
                max_y = max_y.max(block.y);
            }
            if min_x == usize::MAX {
                return None;
            }
            let mean = [
                (linear[0] / count) as f32,
                (linear[1] / count) as f32,
                (linear[2] / count) as f32,
            ];
            let mean_spread = (spread / count) as f32;
            Some(NeutralPatch {
                uv: linear_srgb_to_uv(mean),
                linear: mean,
                luma: (luma / count) as f32,
                area: CropRect {
                    x: (min_x * BLOCK) as f32 / width as f32,
                    y: (min_y * BLOCK) as f32 / height as f32,
                    w: ((max_x + 1 - min_x) * BLOCK) as f32 / width as f32,
                    h: ((max_y + 1 - min_y) * BLOCK) as f32 / height as f32,
                }
                .clamped(),
                coverage: group.len() as f32 / total_blocks,
                uniformity: (1.0 - mean_spread / UNIFORMITY).clamp(0.0, 1.0),
            })
        })
        .collect();

    patches.sort_by(|a, b| b.strength().total_cmp(&a.strength()));
    patches.truncate(MAX_PATCHES);
    patches
}

/// Mean absolute chromaticity deviation inside one block.
///
/// Sampled every fourth pixel: the answer is a spread over a thousand pixels and a quarter
/// of them says the same thing to three decimal places.
fn chroma_spread(
    rgb: &[u8],
    width: usize,
    table: &[f32; 256],
    column: usize,
    row: usize,
    mean: [f32; 2],
) -> f32 {
    let mut total = 0.0f64;
    let mut count = 0u32;
    for y in ((row * BLOCK)..((row + 1) * BLOCK)).step_by(4) {
        for x in ((column * BLOCK)..((column + 1) * BLOCK)).step_by(4) {
            let base = (y * width + x) * 3;
            let Some(triple) = linear_at(rgb, base, table) else {
                continue;
            };
            let luma = luma_of(triple);
            if luma >= CLIP_LINEAR || luma <= NOISE_FLOOR_LINEAR {
                continue;
            }
            total += f64::from(uv_distance(linear_srgb_to_uv(triple), mean));
            count += 1;
        }
    }
    if count == 0 {
        return f32::MAX;
    }
    (total / f64::from(count)) as f32
}

/// Linear RGB of one pixel, or `None` when the index is out of range.
fn linear_at(rgb: &[u8], base: usize, table: &[f32; 256]) -> Option<[f32; 3]> {
    Some([
        table.get(*rgb.get(base)? as usize).copied()?,
        table.get(*rgb.get(base + 1)? as usize).copied()?,
        table.get(*rgb.get(base + 2)? as usize).copied()?,
    ])
}

/// Rec.709 luminance of a linear triple.
fn luma_of(rgb: [f32; 3]) -> f32 {
    0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
}
