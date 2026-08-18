//! One decode, every number the solve needs.
//!
//! PHASE-15 section 8's step 2: "implement RAW statistics extraction, neutral detection and
//! skin-patch sampling". This is the first of the three, and it is the module the 25 ms
//! budget in section 11 lives or dies by - everything below reads the proxy exactly once and
//! writes into one [`FrameStats`].
//!
//! ## Everything here is in linear light
//!
//! Invariant 8. A grey-world average taken over gamma-encoded values is not the average
//! colour of the light; it is the average of a curve applied to it, and the error is largest
//! exactly where this phase is hardest - in the shadows of a dark reception. So the proxy is
//! linearised once, through [`linear_table`], and every statistic below is an average of linear
//! values.
//!
//! The one exception is the *subject luminance* the exposure solve reads, and the band it is
//! compared against,
//! which are **perceptual**. That is deliberate and it is section 6.1's own units: "ceremony
//! faces at 45-55 % linear-to-perceptual". A face at 50 % perceptual is a face that looks
//! half-bright, which is the thing a photographer is actually setting; a face at 50 %
//! linear is a face that is nearly blown.
//!
//! ## Why the sampling is decimated and why that is safe
//!
//! Every statistic here is a mean, a median or a histogram over a whole frame, and sampling
//! one pixel in four changes none of them by more than the number is ever used to a
//! precision of. On a 2048 px proxy that is 260,000 samples rather than 4.2 million, which
//! is most of the difference between this module costing 6 ms and costing 90.
//!
//! The stride is a **constant**, not a function of the frame size, which is what keeps the
//! result deterministic: a statistic whose sample set depended on the proxy's exact
//! dimensions would give two answers for the same photograph decoded at two rungs.

use aura_core::contract::integrity::CropRect;
use aura_core::contract::people::FaceRef;
use aura_raw::colour::illuminant::{linear_srgb_to_uv, uv_distance, D65_UV};
use aura_vision::embed::descriptors::LumaPlane;

/// Sample every Nth pixel in each direction.
///
/// Two rather than four: the chroma histogram behind mixed-light detection needs enough
/// samples per grid cell to have a median worth reading, and a 6x6 grid over a decimated
/// 2048 px proxy at stride four gives 4,700 samples a cell - which sounds like plenty until
/// the cell is mostly a dark suit and only 300 of them are usable.
pub const STRIDE: usize = 2;

/// Above this linear value a sample is treated as clipped and excluded from every mean.
///
/// The linearisation of phase 09's `exposure::WHITE_POINT`, which is 253/255 encoded. Kept
/// as the same *encoded* threshold rather than as a round linear number, because two modules
/// measuring "blown" differently would be a support ticket nobody could answer - and phase
/// 09 already answered it.
pub const CLIP_LINEAR: f32 = 0.976_2;

/// Below this linear value a sample is too dark for its chromaticity to mean anything.
///
/// A ratio of two small numbers is noise. At 1/255 encoded the two chroma channels of an
/// 8-bit proxy carry about two bits between them, and a grey-world average that included
/// those samples would be an average of the sensor's read noise.
pub const NOISE_FLOOR_LINEAR: f32 = 0.003;

/// The side of the grid the frame is divided into for mixed-light detection.
///
/// Six, so thirty-six cells. Fine enough that a window on one side of a room is a different
/// cell from the tungsten lamp on the other; coarse enough that each cell still has
/// thousands of samples after the two exclusions above.
pub const GRID: usize = 6;

/// The side of the square thumbnail the learned white-balance head reads.
pub const WB_THUMB_SIDE: usize = 64;

/// Fraction of the brightest unclipped samples the white-patch estimate averages.
///
/// The top 1 %. Higher and the "white patch" includes the wall; lower and it is three pixels
/// of a specular highlight, which carries the light source's colour rather than a surface's
/// - and a specular highlight on metal is exactly the wrong thing to white-balance on.
pub const WHITE_PATCH_FRACTION: f32 = 0.01;

/// Bins in the histogram that finds the white patch's luminance cut.
///
/// Five hundred and twelve, and a histogram rather than a sorted list: sorting 260,000 values
/// to find a 1 % cut costs more than every other statistic in this module put together.
const BRIGHT_BINS: usize = 512;

/// One encoded 8-bit sRGB value in linear light.
///
/// The sRGB electro-optical transfer function: a short linear toe below 0.04045 and a 2.4
/// power above it. Written out rather than approximated by a 2.2 gamma, because the toe is
/// where a dark reception's shadows live and a 2.2 approximation puts them about 12 % too
/// dark - which becomes a 12 % error in the grey-world estimate of exactly the frames this
/// phase exists for.
#[must_use]
pub fn linearise(encoded: u8) -> f32 {
    let value = f32::from(encoded) / 255.0;
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// The 256-entry linear table for one frame.
///
/// A table rather than a `powf` per sample: the proxy is 8-bit, so the table is *exact*
/// rather than an approximation, and the inner loops below run a quarter of a million times
/// per frame. Built once per frame and passed down by reference; it is a kilobyte and lives
/// on the stack for the duration of one analysis.
#[must_use]
pub fn linear_table() -> [f32; 256] {
    let mut table = [0.0f32; 256];
    for (encoded, slot) in table.iter_mut().enumerate() {
        *slot = linearise(encoded as u8);
    }
    table
}

/// One grid cell's chromaticity summary.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Cell {
    /// Mean linear RGB of the usable samples in this cell.
    pub linear: [f32; 3],
    /// Its chromaticity in CIE 1976 `u'v'`.
    pub uv: [f32; 2],
    /// Mean linear luminance.
    pub luma: f32,
    /// How many samples were usable. Zero means this cell said nothing.
    pub samples: u32,
}

impl Cell {
    /// True when this cell carries enough evidence to vote on an illuminant.
    ///
    /// Two hundred samples. Below that a cell that happens to contain one bright reflection
    /// votes as loudly as a cell containing a whole window, and the mixed-light detector
    /// starts finding two illuminants in every frame with a candle in it.
    pub const MIN_SAMPLES: u32 = 200;

    /// True when this cell voted.
    #[must_use]
    pub const fn is_usable(&self) -> bool {
        self.samples >= Self::MIN_SAMPLES
    }
}

/// Everything one frame's pixels say, measured once.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameStats {
    /// Frame width in pixels, as decoded.
    pub width: usize,
    /// Frame height in pixels.
    pub height: usize,
    /// Mean linear RGB over usable samples. The grey-world hypothesis, before normalisation.
    pub linear_mean: [f32; 3],
    /// Mean linear RGB of the brightest [`WHITE_PATCH_FRACTION`] of usable samples.
    ///
    /// The white-patch hypothesis. Excludes clipped samples, which is what makes it a
    /// *bright surface* estimate rather than a *light source* estimate.
    pub bright_mean: [f32; 3],
    /// Mean linear luminance over usable samples.
    pub linear_luma: f32,
    /// Median perceptual luminance, `0..1`.
    ///
    /// From the median rather than the mean, for phase 09's reason: a wedding frame is
    /// routinely half black suit and the mean says so.
    pub luma_median: f32,
    /// Mean perceptual luminance in the middle ninth of the frame.
    pub centre_luma: f32,
    /// Mean perceptual luminance in the outer ring.
    ///
    /// With [`FrameStats::centre_luma`], the backlit test: a subject in front of a window
    /// has a dark centre and a bright surround, and section 6.1 gives that case its own
    /// rule.
    pub border_luma: f32,
    /// Fraction of the frame at or above the clip point.
    pub clipped: f32,
    /// Fraction of the frame at or below the noise floor.
    pub crushed: f32,
    /// Linear luminance of every counted sample, in 256 bins over `0..1`.
    ///
    /// Kept because it makes [`FrameStats::clipping_after`] **exact** rather than modelled.
    /// Section 6.1 bounds the exposure so that "highlight clipping does not increase beyond
    /// the scene tolerance", and a solver that estimated how much a lift would clip - from a
    /// median and a mean, say - would be wrong in the direction that matters: it would either
    /// refuse a lift the frame could take, or grant one that blows a bride's dress.
    pub luma_histogram: [u32; 256],
    /// How many samples the histogram counted, clipped ones included.
    pub counted: u32,
    /// How far the frame's own average sits from D65, in `u'v'`.
    ///
    /// A first, crude cast measurement, used to decide whether the frame is worth running
    /// the expensive hypotheses on at all.
    pub cast: f32,
    /// The `GRID` x `GRID` chromaticity map. Row-major.
    pub cells: Vec<Cell>,
    /// The learned head's input: `WB_THUMB_SIDE` squared, three planes, linear, NCHW.
    pub thumbnail: Vec<f32>,
}

impl FrameStats {
    /// Measure one decoded 8-bit sRGB frame.
    ///
    /// `plane` is the perceptual luminance plane the caller already built for phase 09 and
    /// phase 11; it is taken rather than recomputed because a third copy of the same
    /// conversion is a third chance for the three to disagree.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn measure(rgb: &[u8], width: usize, height: usize, plane: &LumaPlane) -> Self {
        let table = linear_table();
        let mut totals = [0.0f64; 3];
        let mut luma_total = 0.0f64;
        let mut usable = 0u64;
        let mut clipped = 0u64;
        let mut crushed = 0u64;
        let mut counted = 0u64;

        let mut bright_histogram = [0u32; BRIGHT_BINS];

        let mut cells = vec![Cell::default(); GRID * GRID];
        let mut cell_totals = vec![[0.0f64; 4]; GRID * GRID];
        let mut luma_histogram = [0u32; 256];

        let mut row = 0;
        while row < height {
            let cell_row = (row * GRID / height.max(1)).min(GRID - 1);
            let mut column = 0;
            while column < width {
                let base = (row * width + column) * 3;
                let (Some(red), Some(green), Some(blue)) = (
                    rgb.get(base).and_then(|v| table.get(*v as usize)).copied(),
                    rgb.get(base + 1)
                        .and_then(|v| table.get(*v as usize))
                        .copied(),
                    rgb.get(base + 2)
                        .and_then(|v| table.get(*v as usize))
                        .copied(),
                ) else {
                    column += STRIDE;
                    continue;
                };
                counted += 1;
                let luma = 0.2126 * red + 0.7152 * green + 0.0722 * blue;
                // Every counted sample lands in the histogram, clipped ones included: the
                // question `clipping_after` asks is "what fraction of the frame is at or
                // above the clip point", and a histogram that omitted the pixels already
                // there would answer it about a different photograph.
                let luma_bin = ((luma.clamp(0.0, 1.0) * 255.0) as usize).min(255);
                if let Some(slot) = luma_histogram.get_mut(luma_bin) {
                    *slot += 1;
                }
                if red >= CLIP_LINEAR || green >= CLIP_LINEAR || blue >= CLIP_LINEAR {
                    clipped += 1;
                    column += STRIDE;
                    continue;
                }
                if luma <= NOISE_FLOOR_LINEAR {
                    crushed += 1;
                    column += STRIDE;
                    continue;
                }

                usable += 1;
                totals[0] += f64::from(red);
                totals[1] += f64::from(green);
                totals[2] += f64::from(blue);
                luma_total += f64::from(luma);

                let bin = ((luma * (BRIGHT_BINS - 1) as f32) as usize).min(BRIGHT_BINS - 1);
                if let Some(slot) = bright_histogram.get_mut(bin) {
                    *slot += 1;
                }

                let cell_column = (column * GRID / width.max(1)).min(GRID - 1);
                if let Some(slot) = cell_totals.get_mut(cell_row * GRID + cell_column) {
                    slot[0] += f64::from(red);
                    slot[1] += f64::from(green);
                    slot[2] += f64::from(blue);
                    slot[3] += 1.0;
                }

                column += STRIDE;
            }
            row += STRIDE;
        }

        let scale = |total: f64| -> f32 {
            if usable == 0 {
                0.0
            } else {
                (total / usable as f64) as f32
            }
        };
        let linear_mean = [scale(totals[0]), scale(totals[1]), scale(totals[2])];

        // The white patch: walk the histogram down from the top until 1 % of the usable
        // samples are accounted for, then average the pixels in that band on a second cheap
        // pass. A second pass rather than a running set, because keeping the top 2,600
        // samples means a heap, and a heap in this loop costs more than the pass does.
        let want = ((usable as f32 * WHITE_PATCH_FRACTION) as u64).max(1);
        let mut seen = 0u64;
        let mut cut_bin = BRIGHT_BINS - 1;
        for (index, count) in bright_histogram.iter().enumerate().rev() {
            seen += u64::from(*count);
            cut_bin = index;
            if seen >= want {
                break;
            }
        }
        let cut = cut_bin as f32 / (BRIGHT_BINS - 1) as f32;
        let bright_mean = bright_average(rgb, width, height, &table, cut);

        for (cell, total) in cells.iter_mut().zip(cell_totals.iter()) {
            let count = total[3];
            if count <= 0.0 {
                continue;
            }
            let linear = [
                (total[0] / count) as f32,
                (total[1] / count) as f32,
                (total[2] / count) as f32,
            ];
            cell.linear = linear;
            cell.uv = linear_srgb_to_uv(linear);
            cell.luma = 0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2];
            cell.samples = count as u32;
        }

        let (centre_luma, border_luma) = centre_and_border(plane);
        let cast = uv_distance(linear_srgb_to_uv(linear_mean), D65_UV);

        Self {
            width,
            height,
            linear_mean,
            bright_mean,
            linear_luma: scale(luma_total),
            luma_median: median(plane),
            centre_luma,
            border_luma,
            clipped: fraction(clipped, counted),
            crushed: fraction(crushed, counted),
            luma_histogram,
            counted: counted.min(u64::from(u32::MAX)) as u32,
            cast,
            cells,
            thumbnail: thumbnail(rgb, width, height, &table),
        }
    }

    /// What fraction of the frame would be clipped after an exposure move of `ev` stops.
    ///
    /// Exact rather than modelled, from [`FrameStats::luma_histogram`]. A move of `ev` stops
    /// multiplies every linear luminance by `2^ev`, so a sample clips afterwards exactly when
    /// its current luminance is at or above `CLIP_LINEAR / 2^ev`.
    ///
    /// The histogram is over *luminance* and the clip test in `measure` is per *channel*,
    /// which makes this a slight under-estimate for a strongly coloured highlight - a red
    /// gel on a white wall clips its red channel before its luminance reaches the point. The
    /// bias is in the safe direction for the wrong reason and is worth stating: it means the
    /// solver can grant a lift that clips a saturated highlight it did not predict. Section
    /// 6.1's constraint is about *highlight* clipping, and a saturated single-channel clip is
    /// phase 09's `Clipping::red`, which the verdict already carries.
    #[must_use]
    pub fn clipping_after(&self, ev: f32) -> f32 {
        if self.counted == 0 {
            return 0.0;
        }
        let gain = ev.clamp(-8.0, 8.0).exp2();
        let threshold = CLIP_LINEAR / gain.max(1e-6);
        if threshold >= 1.0 {
            // Even the brightest possible sample stays below the clip point after this move.
            return 0.0;
        }
        let first_bin = ((threshold * 255.0).ceil() as usize).min(255);
        let mut above = 0u64;
        for count in self.luma_histogram.iter().skip(first_bin) {
            above += u64::from(*count);
        }
        (above as f64 / f64::from(self.counted)) as f32
    }

    /// How much *new* clipping a move of `ev` stops would add.
    ///
    /// What `ToneEstimate::clipping_added` stores and what a scene's `clip_tolerance` is
    /// compared against. Never negative: a move that clips less than the frame already does
    /// has added nothing, and reporting a negative addition would make the sum over a wedding
    /// meaningless.
    #[must_use]
    pub fn clipping_added(&self, ev: f32) -> f32 {
        (self.clipping_after(ev) - self.clipping_after(0.0)).max(0.0)
    }

    /// The grey-world hypothesis as a chromaticity.
    #[must_use]
    pub fn grey_world_uv(&self) -> [f32; 2] {
        linear_srgb_to_uv(self.linear_mean)
    }

    /// The white-patch hypothesis as a chromaticity.
    #[must_use]
    pub fn white_patch_uv(&self) -> [f32; 2] {
        linear_srgb_to_uv(self.bright_mean)
    }

    /// The cells that carried enough samples to vote.
    #[must_use]
    pub fn voting_cells(&self) -> Vec<&Cell> {
        self.cells.iter().filter(|cell| cell.is_usable()).collect()
    }

    /// True when the frame looks backlit: a dark middle against a bright surround.
    ///
    /// Section 6.1's dedicated rule needs a trigger, and this is it. A ratio rather than a
    /// difference, because the same absolute gap means something different at a night
    /// reception and in a bright church.
    ///
    /// The threshold is deliberately conservative: a frame that is merely *vignetted* by its
    /// lens has a border about 20 % darker than its centre, and this test is the other way
    /// round and asks for 60 % brighter.
    #[must_use]
    pub fn looks_backlit(&self) -> bool {
        self.centre_luma > 0.0 && self.border_luma > self.centre_luma * 1.6
    }
}

/// Fraction of `total` that `part` is, guarding the empty frame.
fn fraction(part: u64, total: u64) -> f32 {
    if total == 0 {
        return 0.0;
    }
    (part as f64 / total as f64) as f32
}

/// Median of the perceptual luminance plane, from a 256-bin histogram.
///
/// Exact rather than approximate: the plane came from 8-bit data and has 256 distinct
/// values.
fn median(plane: &LumaPlane) -> f32 {
    let mut histogram = [0u32; 256];
    let mut count = 0u32;
    for value in &plane.values {
        let bin = ((value.clamp(0.0, 1.0) * 255.0) as usize).min(255);
        if let Some(slot) = histogram.get_mut(bin) {
            *slot += 1;
        }
        count += 1;
    }
    if count == 0 {
        return 0.0;
    }
    let target = count / 2;
    let mut seen = 0u32;
    for (bin, hits) in histogram.iter().enumerate() {
        seen += *hits;
        if seen > target {
            return bin as f32 / 255.0;
        }
    }
    1.0
}

/// Mean perceptual luminance of the middle ninth, and of everything outside it.
fn centre_and_border(plane: &LumaPlane) -> (f32, f32) {
    if plane.width == 0 || plane.height == 0 {
        return (0.0, 0.0);
    }
    let x0 = plane.width / 3;
    let x1 = plane.width * 2 / 3;
    let y0 = plane.height / 3;
    let y1 = plane.height * 2 / 3;
    let mut centre = (0.0f64, 0u32);
    let mut border = (0.0f64, 0u32);
    for y in (0..plane.height).step_by(STRIDE) {
        for x in (0..plane.width).step_by(STRIDE) {
            let value = plane
                .values
                .get(y * plane.width + x)
                .copied()
                .unwrap_or(0.0);
            if x >= x0 && x < x1 && y >= y0 && y < y1 {
                centre.0 += f64::from(value);
                centre.1 += 1;
            } else {
                border.0 += f64::from(value);
                border.1 += 1;
            }
        }
    }
    let mean = |pair: (f64, u32)| -> f32 {
        if pair.1 == 0 {
            0.0
        } else {
            (pair.0 / f64::from(pair.1)) as f32
        }
    };
    (mean(centre), mean(border))
}

/// Average the unclipped samples brighter than `cut` in linear luminance.
fn bright_average(
    rgb: &[u8],
    width: usize,
    height: usize,
    table: &[f32; 256],
    cut: f32,
) -> [f32; 3] {
    let mut totals = [0.0f64; 3];
    let mut count = 0u64;
    let mut row = 0;
    while row < height {
        let mut column = 0;
        while column < width {
            let base = (row * width + column) * 3;
            let (Some(red), Some(green), Some(blue)) = (
                rgb.get(base).and_then(|v| table.get(*v as usize)).copied(),
                rgb.get(base + 1)
                    .and_then(|v| table.get(*v as usize))
                    .copied(),
                rgb.get(base + 2)
                    .and_then(|v| table.get(*v as usize))
                    .copied(),
            ) else {
                column += STRIDE;
                continue;
            };
            if red < CLIP_LINEAR && green < CLIP_LINEAR && blue < CLIP_LINEAR {
                let luma = 0.2126 * red + 0.7152 * green + 0.0722 * blue;
                if luma >= cut {
                    totals[0] += f64::from(red);
                    totals[1] += f64::from(green);
                    totals[2] += f64::from(blue);
                    count += 1;
                }
            }
            column += STRIDE;
        }
        row += STRIDE;
    }
    if count == 0 {
        return [0.0; 3];
    }
    [
        (totals[0] / count as f64) as f32,
        (totals[1] / count as f64) as f32,
        (totals[2] / count as f64) as f32,
    ]
}

/// The learned head's input: a linear box-filtered thumbnail, NCHW.
///
/// Box-filtered rather than nearest, for `box_resize`'s own reason: aliasing in a model
/// input makes the answer depend on where the subject happened to fall on the sampling grid,
/// and a white-balance prediction that changes when the camera moved two pixels is a
/// prediction nobody can reproduce.
fn thumbnail(rgb: &[u8], width: usize, height: usize, table: &[f32; 256]) -> Vec<f32> {
    let side = WB_THUMB_SIDE;
    let mut out = vec![0.0f32; 3 * side * side];
    if width == 0 || height == 0 {
        return out;
    }
    for row in 0..side {
        let top = row * height / side;
        let bottom = (((row + 1) * height) / side).max(top + 1).min(height);
        for column in 0..side {
            let left = column * width / side;
            let right = (((column + 1) * width) / side).max(left + 1).min(width);
            let mut totals = [0.0f64; 3];
            let mut count = 0u32;
            for y in top..bottom {
                for x in left..right {
                    let base = (y * width + x) * 3;
                    for channel in 0..3 {
                        let value = rgb
                            .get(base + channel)
                            .and_then(|v| table.get(*v as usize))
                            .copied()
                            .unwrap_or(0.0);
                        if let Some(slot) = totals.get_mut(channel) {
                            *slot += f64::from(value);
                        }
                    }
                    count += 1;
                }
            }
            if count == 0 {
                continue;
            }
            for channel in 0..3 {
                let mean = totals.get(channel).copied().unwrap_or(0.0) / f64::from(count);
                if let Some(slot) = out.get_mut(channel * side * side + row * side + column) {
                    *slot = mean as f32;
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Skin patches
// ---------------------------------------------------------------------------

/// One person's skin, as it appears in one frame.
///
/// Section 8's step 2, third part: "skin-patch sampling". The patch is a *measurement of the
/// pixels*, not of a person: it is a mean linear RGB and a chromaticity, and there is no
/// field here or anywhere downstream that says anything about who they are.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkinPatch {
    /// Mean linear RGB of the sampled pixels.
    pub linear: [f32; 3],
    /// Its chromaticity in CIE 1976 `u'v'`, as observed under whatever light was there.
    pub uv: [f32; 2],
    /// Mean linear luminance.
    pub luma: f32,
    /// Where the sample was taken, for the evidence crop.
    pub area: CropRect,
    /// How many pixels contributed.
    pub samples: u32,
    /// The face's own quality from phase 06, carried so the locus can weight by it.
    pub quality: f32,
}

/// How much of a face box the skin sample takes, as a fraction of each side.
///
/// The middle 50 %. A face box includes hair, a collar and background at its corners, and
/// all three are chromatically nothing like skin. The middle half is cheek, nose and
/// forehead - and it is the region that survives being a little wrong about where the box
/// is.
pub const SKIN_INSET: f32 = 0.50;

/// The most extreme luminances a skin sample keeps, as fractions of the patch's own range.
///
/// The darkest and brightest 20 % of the patch are dropped. That removes a nostril, a
/// shadowed eye socket and a specular highlight on a forehead, all three of which are inside
/// the middle half of a face box and none of which is the colour of skin.
pub const SKIN_TRIM: f32 = 0.20;

/// Sample one face's skin from a decoded frame.
///
/// Returns `None` when the face is too small to sample, when the box falls outside the
/// frame, or when too few pixels survived the trims. `None` rather than a low-confidence
/// patch, for the reason `SkinLocus` gives about weak loci: a patch measured from eleven
/// pixels looks exactly like evidence and is not.
#[must_use]
pub fn sample_skin(
    rgb: &[u8],
    width: usize,
    height: usize,
    table: &[f32; 256],
    face: &FaceRef,
) -> Option<SkinPatch> {
    /// The fewest pixels a usable patch has after trimming.
    const MIN_SAMPLES: u32 = 48;

    let area = face.bbox.expanded(-SKIN_INSET).clamped();
    if area.is_empty() {
        return None;
    }
    let x0 = ((area.x * width as f32) as usize).min(width.saturating_sub(1));
    let y0 = ((area.y * height as f32) as usize).min(height.saturating_sub(1));
    let x1 = (((area.x + area.w) * width as f32) as usize).min(width);
    let y1 = (((area.y + area.h) * height as f32) as usize).min(height);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }

    // Two passes: one to learn the patch's own luminance range, one to average the middle of
    // it. The alternative - collecting the pixels and sorting - allocates per face, and a
    // group photograph has thirty of them.
    let mut low = f32::MAX;
    let mut high = f32::MIN;
    let mut seen = 0u32;
    for y in y0..y1 {
        for x in x0..x1 {
            let base = (y * width + x) * 3;
            let Some(luma) = linear_luma_at(rgb, base, table) else {
                continue;
            };
            if luma >= CLIP_LINEAR || luma <= NOISE_FLOOR_LINEAR {
                continue;
            }
            low = low.min(luma);
            high = high.max(luma);
            seen += 1;
        }
    }
    if seen < MIN_SAMPLES || high - low <= f32::EPSILON {
        return None;
    }
    let span = high - low;
    let keep_low = low + span * SKIN_TRIM;
    let keep_high = high - span * SKIN_TRIM;

    let mut totals = [0.0f64; 3];
    let mut luma_total = 0.0f64;
    let mut count = 0u32;
    for y in y0..y1 {
        for x in x0..x1 {
            let base = (y * width + x) * 3;
            let Some(luma) = linear_luma_at(rgb, base, table) else {
                continue;
            };
            if luma < keep_low || luma > keep_high {
                continue;
            }
            for channel in 0..3 {
                let value = rgb
                    .get(base + channel)
                    .and_then(|v| table.get(*v as usize))
                    .copied()
                    .unwrap_or(0.0);
                if let Some(slot) = totals.get_mut(channel) {
                    *slot += f64::from(value);
                }
            }
            luma_total += f64::from(luma);
            count += 1;
        }
    }
    if count < MIN_SAMPLES {
        return None;
    }
    let linear = [
        (totals[0] / f64::from(count)) as f32,
        (totals[1] / f64::from(count)) as f32,
        (totals[2] / f64::from(count)) as f32,
    ];
    Some(SkinPatch {
        linear,
        uv: linear_srgb_to_uv(linear),
        luma: (luma_total / f64::from(count)) as f32,
        area,
        samples: count,
        quality: face.quality,
    })
}

/// Linear luminance of one pixel, or `None` when the index is out of range.
fn linear_luma_at(rgb: &[u8], base: usize, table: &[f32; 256]) -> Option<f32> {
    let r = table.get(*rgb.get(base)? as usize).copied()?;
    let g = table.get(*rgb.get(base + 1)? as usize).copied()?;
    let b = table.get(*rgb.get(base + 2)? as usize).copied()?;
    Some(0.2126 * r + 0.7152 * g + 0.0722 * b)
}
