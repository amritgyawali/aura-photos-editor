//! A cull measured from pixels: the frames a photographer rejects on technical grounds
//! alone, decided without any learned model. ADR-0088.
//!
//! Phase 12's culling engine fuses four learned sub-scores, and every one of them still
//! comes from a placeholder head, so an unattended run used to keep every frame and say so.
//! This module is the part of a cull that needs no model at all:
//!
//! 1. **Unusable exposure** - a black frame, or one that is blown out almost everywhere.
//! 2. **Out of focus** - nothing in the frame is sharp, at a scale relative to the frame.
//! 3. **Motion blur** - edges survive in one direction and are smeared in another.
//! 4. **Burst duplicates** - consecutive near-identical frames, of which the sharpest with
//!    the most open eyes is kept.
//!
//! Above those four rules sits a second question - which of the usable frames is the better
//! photograph - answered by the hundred measured checks in [`crate::cull_checks`]:
//!
//! 5. **Weaker shots of one moment** - similar frames taken within seconds, of which those
//!    scoring close to the best are kept and the clearly weaker ones are left out.
//! 6. **Very low scores** - a frame whose hundred checks add up to almost nothing.
//!
//! The best-scoring fifth of what is delivered is marked as top picks. ADR-0185.
//!
//! Frames of one shot are found by their difference hash and, because a darker, brighter or
//! softer copy of a shot has a different hash, by the layout of light and dark in the frame,
//! which exposure and focus do not change. How hard the cull is - whether weaker shots of a
//! moment go at all, how close to the best a shot must be to stay, where the low-score line
//! sits - is one of three [`Strictness`] levels the photographer picks.
//!
//! It only ever errs toward keeping. A frame a person edited by hand is never rejected, a
//! frame that cannot be measured is kept, a run in which the technical rules would discard
//! an implausible share of the wedding withdraws them, and nothing is deleted: a rejected
//! photograph stays in the collection and is simply not delivered by this run.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    // Plane coordinates never exceed the analysis edge, so a signed cast cannot wrap.
    clippy::cast_possible_wrap
)]

use std::collections::BTreeMap;

use aura_preview::contract::service::{PreviewService, Priority};
use aura_vision::portrait::PortraitFace;
use serde::{Deserialize, Serialize};

use crate::commands::IpcResult;
use crate::contract::ipc::ImageRowLite;
use crate::AppState;

/// Long edge every frame is measured at. Blur is judged relative to the frame, so a
/// 45-megapixel file and a web-sized one are held to the same standard.
const ANALYSIS_EDGE: usize = 1024;
/// Strongest edges (99.9th percentile of gradient, over the frame's own contrast) below
/// this mean nothing in the frame is sharp. Calibrated on real photographs: the softest
/// sharp original measured 0.156, a three-pixel Gaussian blur at this scale 0.086.
const DEFOCUS: f32 = 0.10;
/// A frame with a tiny sharp subject and a large soft background still has a few very
/// strong edges; one that is blurred throughout does not.
const DEFOCUS_PEAK: f32 = 0.16;
/// The weakest of four edge directions, for motion blur.
const MOTION: f32 = 0.08;
/// Weakest over strongest direction. Sharp originals measured 0.74 to 0.88.
const MOTION_RATIO: f32 = 0.5;
/// The brightest one percent of a frame, below which it is a lens cap or a misfire.
const BLACK_FRAME: f32 = 0.05;
/// Share of the frame at pure white above which nothing is recoverable.
const BLOWN_FRACTION: f32 = 0.60;
/// Difference-hash bits two consecutive frames may differ by and still be one burst.
const BURST_BITS: u32 = 6;
/// The same, when the camera clock cannot confirm the frames were taken together.
const BURST_BITS_UNTIMED: u32 = 3;
/// Longest gap between two frames of one burst.
const BURST_GAP_MS: i64 = 2500;
/// How many following frames a frame is compared with when looking for its burst.
const BURST_WINDOW: usize = 8;
/// Above this share of technical rejections the rules are judging a style, not a mistake.
const IMPLAUSIBLE_SHARE: f32 = 0.40;
/// Difference-hash bits two frames of one *moment* may differ by: wider than a burst, so a
/// reframed shot of the same people counts, narrower than a change of scene.
const MOMENT_BITS: u32 = 12;
/// The same, when the camera clock cannot confirm the frames were taken together.
const MOMENT_BITS_UNTIMED: u32 = 8;
/// Longest gap between two frames of one moment.
const MOMENT_GAP_MS: i64 = 20_000;
/// How many following frames a frame is compared with when looking for its moment.
const MOMENT_WINDOW: usize = 12;
/// A frame of a moment within this many points of the moment's best is kept beside it.
const MOMENT_MARGIN: f32 = 12.0;
/// Below this score out of 100 a frame's checks say almost nothing in it worked.
const LOW_SCORE: f32 = 25.0;
/// The gentle level's low-score line: a frame this low has failed outright.
const GENTLE_LOW_SCORE: f32 = 15.0;
/// The strict level's low-score line.
const STRICT_LOW_SCORE: f32 = 50.0;
/// The strict level keeps a second shot of a moment only this close to the best.
const STRICT_MOMENT_MARGIN: f32 = 6.0;
/// The share of delivered frames marked as top picks, and the score a top pick must reach.
const TOP_SHARE: f32 = 0.20;
const TOP_FLOOR: f32 = 60.0;
/// Frames either side a frame's exposure and colour are compared with.
const NEIGHBOURS: usize = 5;
/// Frames either side searched for a near-duplicate when measuring uniqueness.
const UNIQUENESS_WINDOW: usize = 25;
/// A gap longer than this, or a picture this different, opens a new scene.
const SCENE_GAP_MS: i64 = 120_000;
const SCENE_BITS: u32 = 24;
/// Side of the grid a frame's shape - its layout of light and dark - is read on.
const SHAPE_GRID: usize = 24;
/// Two frames whose shapes correlate at least this much are one shot, whatever their
/// exposure or focus. On 1,000 distinct wedding photographs the closest two different
/// photographs correlated at 0.90; every copy darkened tenfold or defocused correlated at
/// 0.99 or more with its original.
const SAME_SHOT: f32 = 0.97;

/// How hard the cull is. Every level keeps the hard rules, the burst rule and the
/// photographer's own choices; they differ in how much of the rest is left out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Strictness {
    /// Only clear mistakes and near-identical frames are left out.
    Gentle,
    /// Also the clearly weaker shots of a moment, and very low scores.
    #[default]
    Balanced,
    /// Keeps only the strongest shots of a moment and leaves out weak photographs.
    Strict,
}

/// What one strictness level means in numbers.
#[derive(Debug, Clone, Copy)]
struct Policy {
    /// A frame whose own checks add up to less is left out.
    low_score: f32,
    /// Weaker shots of a moment are left out at all.
    moments: bool,
    /// A shot within this many points of its moment's best is kept beside it...
    moment_margin: f32,
    /// ...up to this share of the moment, rounded up.
    moment_share: f32,
    /// Frames kept from bursts of up to 5, up to 12, and longer.
    burst_keepers: [usize; 3],
}

impl Strictness {
    /// Every level, gentlest first.
    pub const ALL: [Self; 3] = [Self::Gentle, Self::Balanced, Self::Strict];

    /// The stable name, as the wire and the stored review spell it.
    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Self::Gentle => "gentle",
            Self::Balanced => "balanced",
            Self::Strict => "strict",
        }
    }

    /// The level a slug names; `None` for anything else.
    #[must_use]
    pub fn parse(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|level| level.slug() == slug)
    }

    fn policy(self) -> Policy {
        match self {
            Self::Gentle => Policy {
                low_score: GENTLE_LOW_SCORE,
                moments: false,
                moment_margin: 0.0,
                moment_share: 1.0,
                burst_keepers: [1, 2, 3],
            },
            Self::Balanced => Policy {
                low_score: LOW_SCORE,
                moments: true,
                moment_margin: MOMENT_MARGIN,
                moment_share: 0.5,
                burst_keepers: [1, 2, 3],
            },
            Self::Strict => Policy {
                low_score: STRICT_LOW_SCORE,
                moments: true,
                moment_margin: STRICT_MOMENT_MARGIN,
                moment_share: 1.0 / 3.0,
                burst_keepers: [1, 1, 2],
            },
        }
    }
}

/// Why a frame was kept or left out. The slug is stable; the sentence is for people.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    /// Delivered: nothing technical against it.
    Keep,
    /// Delivered: the best frame of its burst.
    BestOfBurst,
    /// Delivered: a person edited it, so it is theirs to judge.
    EditedByHand,
    /// Delivered: it could not be measured, and an unmeasured frame is never rejected.
    Unmeasured,
    /// Delivered: the technical rules were withdrawn for the whole run.
    RulesWithdrawn,
    /// Left out: black, or blown out almost everywhere.
    UnusableExposure,
    /// Left out: nothing in the frame is sharp.
    OutOfFocus,
    /// Left out: smeared in one direction.
    MotionBlur,
    /// Left out: a sharper frame of the same burst is delivered instead.
    BurstDuplicate,
    /// Left out: a clearly better shot of the same moment is delivered instead.
    WeakerOfMoment,
    /// Left out: its hundred checks add up to almost nothing.
    LowScore,
}

impl Reason {
    /// Whether the frame is delivered.
    #[must_use]
    pub fn keeps(self) -> bool {
        matches!(
            self,
            Self::Keep
                | Self::BestOfBurst
                | Self::EditedByHand
                | Self::Unmeasured
                | Self::RulesWithdrawn
        )
    }
}

/// What was measured in one frame's pixels.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Measure {
    /// Strongest edges over the frame's contrast; higher is sharper.
    pub sharpness: f32,
    /// The very strongest edges, which a small sharp subject still has.
    pub peak: f32,
    /// The weakest of four edge directions.
    pub direction_min: f32,
    /// Weakest over strongest direction; low means smeared one way.
    pub direction_ratio: f32,
    /// Edge strength inside the largest faces, when there are any.
    pub face_sharpness: Option<f32>,
    /// Contrast of the eye regions over their faces; low next to a sibling means a blink.
    pub eye_contrast: Option<f32>,
    /// Faces found.
    pub faces: usize,
    /// The brightest one percent of the frame, `0..1`.
    pub brightest: f32,
    /// Share of the frame at pure white.
    pub blown: f32,
    /// 64-bit difference hash, for near-duplicate frames.
    #[serde(serialize_with = "hex", deserialize_with = "unhex")]
    pub dhash: u64,
    /// The layout of light and dark on a 24x24 grid, mean removed and scaled to fill an
    /// `i8`. Exposure and focus leave it unchanged, so a darker or softer copy of a shot
    /// still matches; empty for a flat frame.
    #[serde(default, skip_serializing_if = "Vec::is_empty", with = "shape_hex")]
    pub shape: Vec<i8>,
    /// The hundred checks' raw values, in [`crate::cull_checks::CHECKS`] order; empty when they
    /// were not measured. The ten collection checks are filled by [`decide`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<Option<f32>>,
}

// The signature is serde's: `serialize_with` hands the field over by reference.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn hex<S: serde::Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&format!("{value:016x}"))
}

fn unhex<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
    let text = String::deserialize(deserializer)?;
    u64::from_str_radix(&text, 16).map_err(serde::de::Error::custom)
}

/// A shape as two hex digits a cell: a third the size of a JSON array of numbers.
mod shape_hex {
    use serde::{Deserialize, Deserializer, Serializer};

    // The signature is serde's.
    #[allow(clippy::ptr_arg)]
    pub(super) fn serialize<S: Serializer>(
        shape: &Vec<i8>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let mut text = String::with_capacity(shape.len() * 2);
        for byte in shape.iter().flat_map(|cell| cell.to_ne_bytes()) {
            for nibble in [byte >> 4, byte & 0x0f] {
                text.push(char::from_digit(u32::from(nibble), 16).unwrap_or('0'));
            }
        }
        serializer.serialize_str(&text)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<i8>, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.as_bytes()
            .chunks(2)
            .map(|pair| {
                std::str::from_utf8(pair)
                    .ok()
                    .and_then(|digits| u8::from_str_radix(digits, 16).ok())
                    .map(|byte| i8::from_ne_bytes([byte]))
                    .ok_or_else(|| serde::de::Error::custom("a shape is pairs of hex digits"))
            })
            .collect()
    }
}

/// A frame's shape: the grid of mean luminance, mean removed, scaled so the largest cell is
/// 127. Empty for a frame with no layout to speak of.
fn shape(values: &[f32], width: usize, height: usize) -> Vec<i8> {
    let cells =
        aura_vision::embed::descriptors::box_resize(values, width, height, SHAPE_GRID, SHAPE_GRID);
    let Some(middle) = mean(&cells) else {
        return Vec::new();
    };
    let peak = cells
        .iter()
        .map(|v| (v - middle).abs())
        .fold(0.0_f32, f32::max);
    if peak < 1e-3 {
        return Vec::new();
    }
    cells
        .iter()
        .map(|v| ((v - middle) / peak * 127.0).round().clamp(-127.0, 127.0) as i8)
        .collect()
}

/// How alike two shapes are: their correlation, `-1..=1`. `None` when either is missing.
fn shape_correlation(a: &[i8], b: &[i8]) -> Option<f32> {
    if a.is_empty() || a.len() != b.len() {
        return None;
    }
    let (mut ab, mut aa, mut bb) = (0_i64, 0_i64, 0_i64);
    for (x, y) in a.iter().zip(b) {
        let (x, y) = (i64::from(*x), i64::from(*y));
        ab += x * y;
        aa += x * x;
        bb += y * y;
    }
    (aa > 0 && bb > 0).then(|| ab as f32 / ((aa as f32).sqrt() * (bb as f32).sqrt()))
}

/// Whether two frames are one shot by their shape alone.
fn same_shot(a: &Measure, b: &Measure) -> bool {
    shape_correlation(&a.shape, &b.shape).is_some_and(|c| c >= SAME_SHOT)
}

/// One frame as the decision sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Frame {
    /// The photograph.
    pub photo_id: String,
    /// Its file name, for the report.
    pub file_name: String,
    /// What its pixels measured; `None` when they could not be read.
    pub measure: Option<Measure>,
    /// Capture time in milliseconds, only when the camera recorded it.
    pub time_ms: Option<i64>,
    /// A person changed this photograph's edit.
    pub edited_by_hand: bool,
}

/// The decision for one frame, as written to `photo-cull.json`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Verdict {
    /// The photograph.
    pub photo_id: String,
    /// Its file name.
    pub file_name: String,
    /// Whether this run delivers it.
    pub keep: bool,
    /// Why.
    pub reason: Reason,
    /// The same, in a sentence with the numbers behind it.
    pub detail: String,
    /// Frames of one burst share a number.
    pub burst: Option<usize>,
    /// For a burst duplicate or a weaker shot of a moment, the frame delivered instead.
    pub kept_instead: Option<String>,
    /// Frames of one moment share a number.
    pub moment: Option<usize>,
    /// The hundred checks' total, `0..=100`, when they were measured.
    pub score: Option<f32>,
    /// One of the best-scoring frames delivered.
    pub top_pick: bool,
    /// Each check's score, `0..=100`, `None` where it abstained.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<Option<u8>>,
    /// Each group's score, in [`crate::cull_checks::Category::ALL`] order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<Option<u8>>,
    /// The measurements.
    pub measure: Option<Measure>,
}

/// Counts for the run report.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    /// Frames considered.
    pub frames: usize,
    /// Frames delivered.
    pub kept: usize,
    /// Left out as black or blown out.
    pub unusable_exposure: usize,
    /// Left out as out of focus.
    pub out_of_focus: usize,
    /// Left out as motion blurred.
    pub motion_blur: usize,
    /// Left out as a duplicate of a sharper burst frame.
    pub burst_duplicates: usize,
    /// Left out as a clearly weaker shot of a moment.
    pub weaker_of_moment: usize,
    /// Left out for a very low score.
    pub low_score: usize,
    /// Bursts found.
    pub bursts: usize,
    /// Moments of several similar shots found.
    pub moments: usize,
    /// Delivered frames marked as top picks.
    pub top_picks: usize,
    /// The technical rules were withdrawn because they rejected too much.
    pub rules_withdrawn: bool,
}

/// The whole cull, as written to `photo-cull.json`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// How hard this cull was.
    pub strictness: Strictness,
    /// What the numbers below were judged against.
    pub thresholds: BTreeMap<&'static str, f32>,
    /// The check each position of `checks` and `measure.checks` holds.
    pub check_ids: Vec<&'static str>,
    /// Totals.
    pub counts: Counts,
    /// One row per frame, in timeline order.
    pub verdicts: Vec<Verdict>,
}

impl Report {
    /// The photographs this run delivers, in timeline order.
    #[must_use]
    pub fn kept(&self) -> Vec<String> {
        self.verdicts
            .iter()
            .filter(|verdict| verdict.keep)
            .map(|verdict| verdict.photo_id.clone())
            .collect()
    }

    /// One sentence for the run notes.
    #[must_use]
    pub fn summary(&self) -> String {
        let c = &self.counts;
        if c.rules_withdrawn {
            return format!(
                "Measured cull: the focus and exposure rules would have rejected more than {:.0}% of this collection, which describes a style rather than mistakes, so they were withdrawn. {} of {} photographs are delivered; {} burst duplicate(s) were left out. See photo-cull.json.",
                IMPLAUSIBLE_SHARE * 100.0,
                c.kept,
                c.frames,
                c.burst_duplicates
            );
        }
        format!(
            "Measured cull: delivering {} of {} photographs, {} of them top picks. Left out {} burst duplicate(s) of a sharper frame, {} weaker shot(s) of a moment, {} with a very low score, {} out of focus, {} motion blurred and {} with unusable exposure. Nothing was deleted; every decision, its hundred checks and its measurements are in photo-cull.json.",
            c.kept,
            c.frames,
            c.top_picks,
            c.burst_duplicates,
            c.weaker_of_moment,
            c.low_score,
            c.out_of_focus,
            c.motion_blur,
            c.unusable_exposure
        )
    }
}

const BINS: usize = 4096;

/// A fixed-range histogram: percentiles without sorting seven hundred thousand values.
struct Histogram {
    bins: Vec<u32>,
    total: u32,
    max: f32,
}

impl Histogram {
    fn new(max: f32) -> Self {
        Self {
            bins: vec![0; BINS],
            total: 0,
            max,
        }
    }

    fn add(&mut self, value: f32) {
        let index = ((value / self.max).clamp(0.0, 1.0) * (BINS - 1) as f32) as usize;
        if let Some(bin) = self.bins.get_mut(index) {
            *bin += 1;
            self.total += 1;
        }
    }

    fn percentile(&self, fraction: f64) -> f32 {
        if self.total == 0 {
            return 0.0;
        }
        let target = ((f64::from(self.total) * fraction).ceil() as u32).max(1);
        let mut seen = 0_u32;
        for (index, count) in self.bins.iter().enumerate() {
            seen += count;
            if seen >= target {
                return index as f32 / (BINS - 1) as f32 * self.max;
            }
        }
        self.max
    }
}

/// A luminance plane at the analysis scale.
struct Plane {
    values: Vec<f32>,
    width: usize,
    height: usize,
}

impl Plane {
    /// Clamped read: the border repeats, as the calibration's edge padding did.
    fn at(&self, x: isize, y: isize) -> f32 {
        let x = x.clamp(0, self.width as isize - 1) as usize;
        let y = y.clamp(0, self.height as isize - 1) as usize;
        self.values
            .get(y * self.width + x)
            .copied()
            .unwrap_or_default()
    }

    /// Box-average an sRGB frame down by a whole factor.
    fn from_rgb(rgb: &[u8], width: usize, height: usize) -> Option<Self> {
        if width == 0 || height == 0 || rgb.len() < width * height * 3 {
            return None;
        }
        let factor = width.max(height).div_ceil(ANALYSIS_EDGE).max(1);
        let (plane_width, plane_height) = (width / factor, height / factor);
        if plane_width < 8 || plane_height < 8 {
            return None;
        }
        let mut values = vec![0.0_f32; plane_width * plane_height];
        for (row_index, row) in rgb.chunks_exact(width * 3).take(height).enumerate() {
            let y = row_index / factor;
            if y >= plane_height {
                break;
            }
            for (column, pixel) in row.chunks_exact(3).enumerate() {
                let x = column / factor;
                if x >= plane_width {
                    break;
                }
                let luminance: f32 = pixel
                    .iter()
                    .zip([0.2126, 0.7152, 0.0722])
                    .map(|(value, weight)| f32::from(*value) * weight)
                    .sum();
                if let Some(cell) = values.get_mut(y * plane_width + x) {
                    *cell += luminance;
                }
            }
        }
        let scale = 1.0 / (255.0 * (factor * factor) as f32);
        for value in &mut values {
            *value *= scale;
        }
        Some(Self {
            values,
            width: plane_width,
            height: plane_height,
        })
    }

    /// A 3x3 binomial smooth, so sensor noise is not mistaken for detail.
    fn smoothed(&self) -> Self {
        let mut values = Vec::with_capacity(self.values.len());
        for y in 0..self.height as isize {
            for x in 0..self.width as isize {
                let mut sum = 0.0;
                for (dy, row_weight) in [(-1, 1.0), (0, 2.0), (1, 1.0)] {
                    for (dx, weight) in [(-1, 1.0), (0, 2.0), (1, 1.0)] {
                        sum += self.at(x + dx, y + dy) * row_weight * weight;
                    }
                }
                values.push(sum / 16.0);
            }
        }
        Self {
            values,
            width: self.width,
            height: self.height,
        }
    }

    /// Mean and standard deviation inside a pixel rectangle.
    fn spread(&self, left: usize, top: usize, right: usize, bottom: usize) -> Option<f32> {
        let (right, bottom) = (right.min(self.width), bottom.min(self.height));
        if right <= left + 1 || bottom <= top + 1 {
            return None;
        }
        let (mut sum, mut squares, mut count) = (0.0_f64, 0.0_f64, 0.0_f64);
        for y in top..bottom {
            for x in left..right {
                let value = f64::from(self.at(x as isize, y as isize));
                sum += value;
                squares += value * value;
                count += 1.0;
            }
        }
        let mean = sum / count;
        Some(((squares / count - mean * mean).max(0.0)).sqrt() as f32)
    }
}

/// Measure one frame. `rgb` is 8-bit sRGB, row-major; faces are normalised to the frame.
///
/// Returns `None` for a frame too small or too short to judge.
#[must_use]
pub fn measure_pixels(
    rgb: &[u8],
    width: usize,
    height: usize,
    faces: &[PortraitFace],
) -> Option<Measure> {
    let plane = Plane::from_rgb(rgb, width, height)?;
    let mut levels = Histogram::new(1.0);
    let mut white = 0_usize;
    for value in &plane.values {
        levels.add(*value);
        if *value > 0.98 {
            white += 1;
        }
    }
    let contrast = (levels.percentile(0.99) - levels.percentile(0.01)).max(0.05);
    let smooth = plane.smoothed();
    let diagonal = 2.0 * std::f32::consts::SQRT_2;
    let mut magnitude = Histogram::new(0.75);
    let mut directions = [
        Histogram::new(0.75),
        Histogram::new(0.75),
        Histogram::new(0.75),
        Histogram::new(0.75),
    ];
    let mut strength = vec![0.0_f32; plane.values.len()];
    for y in 1..plane.height.saturating_sub(1) as isize {
        for x in 1..plane.width.saturating_sub(1) as isize {
            let gx = (smooth.at(x + 1, y) - smooth.at(x - 1, y)) / 2.0;
            let gy = (smooth.at(x, y + 1) - smooth.at(x, y - 1)) / 2.0;
            let d1 = (smooth.at(x + 1, y + 1) - smooth.at(x - 1, y - 1)) / diagonal;
            let d2 = (smooth.at(x - 1, y + 1) - smooth.at(x + 1, y - 1)) / diagonal;
            let both = (gx * gx + gy * gy).sqrt();
            magnitude.add(both);
            for (histogram, value) in directions.iter_mut().zip([gx, gy, d1, d2]) {
                histogram.add(value.abs());
            }
            if let Some(cell) = strength.get_mut(y as usize * plane.width + x as usize) {
                *cell = both;
            }
        }
    }
    let by_direction = directions.map(|histogram| histogram.percentile(0.999) / contrast);
    let direction_min = by_direction.iter().copied().fold(f32::INFINITY, f32::min);
    let direction_max = by_direction.iter().copied().fold(0.0_f32, f32::max);
    let (face_sharpness, eye_contrast) = faces_measured(&plane, &strength, contrast, faces);
    Some(Measure {
        sharpness: magnitude.percentile(0.999) / contrast,
        peak: magnitude.percentile(0.9999) / contrast,
        direction_min,
        direction_ratio: if direction_max > 0.0 {
            direction_min / direction_max
        } else {
            1.0
        },
        face_sharpness,
        eye_contrast,
        faces: faces.len(),
        brightest: levels.percentile(0.99),
        blown: white as f32 / plane.values.len().max(1) as f32,
        dhash: aura_vision::embed::hash::dhash(&plane.values, plane.width, plane.height),
        shape: shape(&plane.values, plane.width, plane.height),
        checks: Vec::new(),
    })
}

/// Edge strength and eye contrast inside the four largest faces.
fn faces_measured(
    plane: &Plane,
    strength: &[f32],
    contrast: f32,
    faces: &[PortraitFace],
) -> (Option<f32>, Option<f32>) {
    let mut largest: Vec<&PortraitFace> = faces.iter().collect();
    largest.sort_by(|a, b| area(b).total_cmp(&area(a)));
    let (width, height) = (plane.width as f32, plane.height as f32);
    let (mut sharp, mut eyes) = (Vec::new(), Vec::new());
    for face in largest.into_iter().take(4) {
        let [left, top, right, bottom] = face.bounds;
        let (left, right) = ((left * width).max(0.0) as usize, (right * width) as usize);
        let (top, bottom) = ((top * height).max(0.0) as usize, (bottom * height) as usize);
        let (right, bottom) = (right.min(plane.width), bottom.min(plane.height));
        if right < left + 12 || bottom < top + 12 {
            continue;
        }
        let mut inside = Histogram::new(0.75);
        for y in top..bottom {
            for value in strength
                .get(y * plane.width + left..y * plane.width + right)
                .unwrap_or_default()
            {
                inside.add(*value);
            }
        }
        sharp.push(inside.percentile(0.99) / contrast);
        let Some(skin) = plane.spread(left, top, right, bottom) else {
            continue;
        };
        let (Some(first), Some(second)) = (face.landmarks.first(), face.landmarks.get(1)) else {
            continue;
        };
        let apart = ((first[0] - second[0]) * width).hypot((first[1] - second[1]) * height);
        let (half_width, half_height) = (apart * 0.22, apart * 0.12);
        if half_height < 2.0 {
            continue;
        }
        for eye in [first, second] {
            let (x, y) = (eye[0] * width, eye[1] * height);
            if let Some(spread) = plane.spread(
                (x - half_width).max(0.0) as usize,
                (y - half_height).max(0.0) as usize,
                (x + half_width) as usize,
                (y + half_height) as usize,
            ) {
                eyes.push(spread / (skin + 0.01));
            }
        }
    }
    (mean(&sharp), mean(&eyes))
}

fn area(face: &PortraitFace) -> f32 {
    let [left, top, right, bottom] = face.bounds;
    (right - left).max(0.0) * (bottom - top).max(0.0)
}

fn mean(values: &[f32]) -> Option<f32> {
    (!values.is_empty()).then(|| values.iter().sum::<f32>() / values.len() as f32)
}

/// The technical rules for one frame, before bursts are considered.
fn technical(measure: &Measure) -> Option<(Reason, String)> {
    if measure.brightest < BLACK_FRAME {
        return Some((
            Reason::UnusableExposure,
            format!(
                "Black frame: even its brightest 1% is at {:.0}% (below {:.0}%).",
                measure.brightest * 100.0,
                BLACK_FRAME * 100.0
            ),
        ));
    }
    if measure.blown > BLOWN_FRACTION {
        return Some((
            Reason::UnusableExposure,
            format!(
                "Blown out: {:.0}% of the frame is pure white (above {:.0}%).",
                measure.blown * 100.0,
                BLOWN_FRACTION * 100.0
            ),
        ));
    }
    if measure.sharpness < DEFOCUS && measure.peak < DEFOCUS_PEAK {
        return Some((
            Reason::OutOfFocus,
            format!(
                "Out of focus: its strongest edges measure {:.3} (below {DEFOCUS:.2}) and nothing in the frame is sharper than {:.3}.",
                measure.sharpness, measure.peak
            ),
        ));
    }
    if measure.direction_min < MOTION && measure.direction_ratio < MOTION_RATIO {
        return Some((
            Reason::MotionBlur,
            format!(
                "Motion blur: edges in one direction measure {:.3} (below {MOTION:.2}), {:.0}% of the strongest direction.",
                measure.direction_min,
                measure.direction_ratio * 100.0
            ),
        ));
    }
    None
}

/// How good a frame is next to its burst siblings.
fn quality(measure: &Measure, best_eyes: Option<f32>) -> f32 {
    let sharp = measure.face_sharpness.unwrap_or(measure.sharpness);
    // Eyes clearly flatter than the best sibling's are a blink; small differences are noise.
    let eyes = match (measure.eye_contrast, best_eyes) {
        (Some(own), Some(best)) if best > 0.0 && own / best < 0.8 => (own / best).max(0.5),
        _ => 1.0,
    };
    sharp * eyes
}

/// How close two frames must be to count as one group.
#[derive(Debug, Clone, Copy)]
struct Grouping {
    bits: u32,
    bits_untimed: u32,
    gap_ms: i64,
    window: usize,
}

/// Near-identical frames shot within a couple of seconds.
const BURST: Grouping = Grouping {
    bits: BURST_BITS,
    bits_untimed: BURST_BITS_UNTIMED,
    gap_ms: BURST_GAP_MS,
    window: BURST_WINDOW,
};

/// Similar frames of the same people within seconds: a burst, or the shot taken again.
const MOMENT: Grouping = Grouping {
    bits: MOMENT_BITS,
    bits_untimed: MOMENT_BITS_UNTIMED,
    gap_ms: MOMENT_GAP_MS,
    window: MOMENT_WINDOW,
};

fn together(a: &Frame, b: &Frame, grouping: Grouping) -> bool {
    let (Some(first), Some(second)) = (&a.measure, &b.measure) else {
        return false;
    };
    let bits = (first.dhash ^ second.dhash).count_ones();
    match (a.time_ms, b.time_ms) {
        (Some(earlier), Some(later)) => {
            (later - earlier).abs() <= grouping.gap_ms
                && (bits <= grouping.bits || same_shot(first, second))
        }
        _ => bits <= grouping.bits_untimed || same_shot(first, second),
    }
}

/// The verdict each frame gets on its own, before bursts are considered.
fn first_verdicts(frames: &[Frame]) -> Vec<Verdict> {
    frames
        .iter()
        .map(|frame| {
            let (reason, detail) = match &frame.measure {
                None => (
                    Reason::Unmeasured,
                    "Could not be measured; an unmeasured photograph is never left out.".into(),
                ),
                Some(_) if frame.edited_by_hand => (
                    Reason::EditedByHand,
                    "You edited this photograph yourself, so it is always delivered.".into(),
                ),
                Some(measure) => technical(measure).unwrap_or_else(|| {
                    (
                        Reason::Keep,
                        format!(
                            "Sharp and usable: edges {:.3}, weakest direction {:.3}.",
                            measure.sharpness, measure.direction_min
                        ),
                    )
                }),
            };
            Verdict {
                photo_id: frame.photo_id.clone(),
                file_name: frame.file_name.clone(),
                keep: reason.keeps(),
                reason,
                detail,
                burst: None,
                kept_instead: None,
                moment: None,
                score: None,
                top_pick: false,
                checks: Vec::new(),
                categories: Vec::new(),
                measure: frame.measure.clone(),
            }
        })
        .collect()
}

/// A wedding where the focus and exposure rules reject a large share is a deliberate style
/// (soft film scans, a dark reception shot wide open), not a run of mistakes. Returns whether
/// the rules were withdrawn.
fn withdraw_if_implausible(verdicts: &mut [Verdict]) -> bool {
    let rejected = verdicts.iter().filter(|v| !v.keep).count();
    if verdicts.is_empty() || rejected as f32 / verdicts.len() as f32 <= IMPLAUSIBLE_SHARE {
        return false;
    }
    for verdict in verdicts.iter_mut().filter(|v| !v.keep) {
        verdict.detail = format!("Delivered anyway. Would have been: {}", verdict.detail);
        verdict.reason = Reason::RulesWithdrawn;
        verdict.keep = true;
    }
    true
}

/// Groups of deliverable frames that are alike and close together. They need not be
/// neighbours - a second camera's frame, or a rejected one, often sits between two frames of
/// the same burst - so each frame is compared with the few that follow it. Frames the camera
/// did not time must be neighbours.
fn find_groups(frames: &[Frame], verdicts: &[Verdict], grouping: Grouping) -> Vec<Vec<usize>> {
    let candidates: Vec<usize> = (0..frames.len())
        .filter(|index| {
            verdicts
                .get(*index)
                .is_some_and(|v| v.keep && v.reason != Reason::Unmeasured)
        })
        .collect();
    let mut labels: Vec<usize> = (0..candidates.len()).collect();
    for first in 0..candidates.len() {
        for second in first + 1..candidates.len().min(first + 1 + grouping.window) {
            let pair = candidates
                .get(first)
                .zip(candidates.get(second))
                .and_then(|(a, b)| frames.get(*a).zip(frames.get(*b)));
            let Some((earlier, later)) = pair else {
                continue;
            };
            let timed = earlier.time_ms.is_some() && later.time_ms.is_some();
            if (!timed && second != first + 1) || !together(earlier, later, grouping) {
                continue;
            }
            join(&mut labels, first, second);
        }
    }
    // A frame without a camera time has no place in the timeline to be near anything, so a
    // copy of it - darker, softer, exported again - can sit anywhere. Those are matched by
    // shape across the whole collection.
    for first in 0..candidates.len() {
        for second in first + 1..candidates.len() {
            let pair = candidates
                .get(first)
                .zip(candidates.get(second))
                .and_then(|(a, b)| frames.get(*a).zip(frames.get(*b)));
            let Some((a, b)) = pair else { continue };
            if a.time_ms.is_some() && b.time_ms.is_some() {
                continue;
            }
            let shot = a
                .measure
                .as_ref()
                .zip(b.measure.as_ref())
                .is_some_and(|(x, y)| same_shot(x, y));
            if shot && labels.get(first) != labels.get(second) {
                join(&mut labels, first, second);
            }
        }
    }
    let mut grouped: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (position, label) in labels.iter().enumerate() {
        if let Some(index) = candidates.get(position) {
            grouped.entry(*label).or_default().push(*index);
        }
    }
    grouped
        .into_values()
        .filter(|members| members.len() > 1)
        .collect()
}

/// Put `second`'s group into `first`'s.
fn join(labels: &mut [usize], first: usize, second: usize) {
    let (keep, merge) = (
        labels.get(first).copied().unwrap_or(first),
        labels.get(second).copied().unwrap_or(second),
    );
    if keep != merge {
        for label in labels.iter_mut() {
            if *label == merge {
                *label = keep;
            }
        }
    }
}

/// Keep the best of one burst and leave the rest out, naming the frame delivered instead.
fn settle_burst(
    frames: &[Frame],
    verdicts: &mut [Verdict],
    number: usize,
    members: &[usize],
    totals: &[Option<f32>],
    policy: Policy,
) {
    let measures: Vec<(usize, &Measure)> = members
        .iter()
        .filter_map(|i| frames.get(*i)?.measure.as_ref().map(|m| (*i, m)))
        .collect();
    let best_eyes = measures
        .iter()
        .filter_map(|(_, m)| m.eye_contrast)
        .reduce(f32::max);
    // The hundred checks decide when every frame has them; sharpness and open eyes otherwise.
    let scored = measures
        .iter()
        .all(|(i, _)| totals.get(*i).copied().flatten().is_some());
    let mut ranked: Vec<(usize, f32)> = measures
        .iter()
        .map(|(i, m)| {
            let total = totals.get(*i).copied().flatten();
            (
                *i,
                if scored {
                    total.unwrap_or_default()
                } else {
                    quality(m, best_eyes)
                },
            )
        })
        .collect();
    let edited = |index: usize| frames.get(index).is_some_and(|f| f.edited_by_hand);
    // A hand-edited frame is always delivered, so it leads its burst.
    ranked.sort_by(|a, b| {
        edited(b.0)
            .cmp(&edited(a.0))
            .then(b.1.total_cmp(&a.1))
            .then(a.0.cmp(&b.0))
    });
    let [short, medium, long] = policy.burst_keepers;
    let keepers = match members.len() {
        0..=5 => short,
        6..=12 => medium,
        _ => long,
    };
    let Some(&(best, best_quality)) = ranked.first() else {
        return;
    };
    let best_name = frames
        .get(best)
        .map(|f| f.file_name.clone())
        .unwrap_or_default();
    let best_id = frames.get(best).map(|f| f.photo_id.clone());
    for (rank, (index, own)) in ranked.iter().enumerate() {
        let Some(verdict) = verdicts.get_mut(*index) else {
            continue;
        };
        verdict.burst = Some(number);
        if rank < keepers || edited(*index) {
            if !edited(*index) && verdict.reason == Reason::Keep {
                verdict.reason = Reason::BestOfBurst;
                verdict.detail = if scored {
                    format!(
                        "Best of a burst of {}: scored {own:.0} of 100 on its hundred checks.",
                        members.len()
                    )
                } else {
                    format!(
                        "Best of a burst of {}: quality {own:.3} from sharpness and open eyes.",
                        members.len()
                    )
                };
            }
        } else {
            verdict.keep = false;
            verdict.reason = Reason::BurstDuplicate;
            verdict.kept_instead.clone_from(&best_id);
            verdict.detail = if scored {
                format!(
                    "Near-identical to {best_name}, which is delivered instead: scored {own:.0} against {best_quality:.0} of 100."
                )
            } else {
                format!(
                    "Near-identical to {best_name}, which is delivered instead: quality {own:.3} against {best_quality:.3}."
                )
            };
        }
    }
}

/// Where a named check sits in the stored arrays.
fn slot(id: &str) -> usize {
    crate::cull_checks::CHECKS
        .iter()
        .position(|check| check.id == id)
        .unwrap_or(0)
}

/// One frame's raw value for a check.
fn raw(frame: &Frame, id: &str) -> Option<f32> {
    frame
        .measure
        .as_ref()?
        .checks
        .get(slot(id))
        .copied()
        .flatten()
}

/// Score the frames that carry checks against each other; `None` for the rest.
fn scored(values: &[Option<Vec<Option<f32>>>]) -> Vec<Option<crate::cull_checks::Scored>> {
    let present: Vec<Vec<Option<f32>>> = values.iter().flatten().cloned().collect();
    let mut scores = crate::cull_checks::score(&present).into_iter();
    values
        .iter()
        .map(|value| value.as_ref().and_then(|_| scores.next()))
        .collect()
}

/// The frames' own ninety checks, before any frame is compared with another.
fn own_values(frames: &[Frame]) -> Vec<Option<Vec<Option<f32>>>> {
    frames
        .iter()
        .map(|frame| {
            frame
                .measure
                .as_ref()
                .filter(|m| !m.checks.is_empty())
                .map(|m| m.checks.clone())
        })
        .collect()
}

/// A frame's place in its group by score: 1 for the best, 0 for the worst.
fn place(index: usize, members: &[usize], scores: &[Option<f32>]) -> Option<f32> {
    let own = scores.get(index).copied().flatten()?;
    let others: Vec<f32> = members
        .iter()
        .filter_map(|i| scores.get(*i).copied().flatten())
        .collect();
    if others.len() < 2 {
        return None;
    }
    let better = others.iter().filter(|v| **v > own).count();
    Some(1.0 - better as f32 / (others.len() - 1) as f32)
}

/// A frame's value for a check against the best of its group.
fn against_best(
    index: usize,
    members: &[usize],
    frames: &[Frame],
    read: &dyn Fn(&Frame) -> Option<f32>,
) -> Option<f32> {
    let own = frames.get(index).and_then(read)?;
    let best = members
        .iter()
        .filter_map(|i| frames.get(*i).and_then(read))
        .fold(0.0_f32, f32::max);
    (best > 1e-6).then(|| own / best)
}

fn median(mut values: Vec<f32>) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f32::total_cmp);
    values.get(values.len() / 2).copied()
}

/// The ten collection checks: each frame against its burst, its moment and its neighbours.
fn with_context(
    frames: &[Frame],
    own: &[Option<Vec<Option<f32>>>],
    pixel: &[Option<f32>],
    bursts: &[Vec<usize>],
    moments: &[Vec<usize>],
) -> Vec<Option<Vec<Option<f32>>>> {
    let group_of =
        |groups: &[Vec<usize>], index: usize| groups.iter().find(|g| g.contains(&index)).cloned();
    let eyes = |f: &Frame| raw(f, "eyes_open");
    let sharp = |f: &Frame| raw(f, "focus_face").or_else(|| raw(f, "focus_strongest_edges"));
    own.iter()
        .enumerate()
        .map(|(index, values)| {
            let mut values = values.clone()?;
            values.resize(crate::cull_checks::COUNT, None);
            let frame = frames.get(index)?;
            let burst = group_of(bursts, index);
            let moment = group_of(moments, index);
            let near: Vec<usize> = (index.saturating_sub(NEIGHBOURS)
                ..(index + NEIGHBOURS + 1).min(frames.len()))
                .filter(|i| *i != index)
                .collect();
            let neighbour_median = |id: &str| {
                median(
                    near.iter()
                        .filter_map(|i| frames.get(*i).and_then(|f| raw(f, id)))
                        .collect(),
                )
            };
            let dhash = frame.measure.as_ref().map(|m| m.dhash);
            let distance = |other: usize| {
                let theirs = frames.get(other)?.measure.as_ref()?.dhash;
                Some((dhash? ^ theirs).count_ones())
            };
            let uniqueness = (index.saturating_sub(UNIQUENESS_WINDOW)
                ..(index + UNIQUENESS_WINDOW + 1).min(frames.len()))
                .filter(|i| *i != index)
                .filter_map(distance)
                .min();
            let previous = index.checked_sub(1);
            let gap_before = previous.and_then(|p| Some(frame.time_ms? - frames.get(p)?.time_ms?));
            let gap_after = frames
                .get(index + 1)
                .and_then(|next| Some(next.time_ms? - frame.time_ms?));
            let new_scene = match previous {
                None => Some(1.0),
                Some(p) => {
                    let far = gap_before.is_some_and(|gap| gap > SCENE_GAP_MS)
                        || distance(p).is_some_and(|bits| bits > SCENE_BITS);
                    far.then_some(1.0)
                }
            };
            let deliberate = match (gap_before, gap_after) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (Some(gap), None) | (None, Some(gap)) => Some(gap),
                (None, None) => None,
            }
            .map(|ms| ms.max(0) as f32 / 1000.0);
            let fit = |id: &str| Some((raw(frame, id)? - neighbour_median(id)?).abs());
            let colour_fit = match (fit("colour_tint"), fit("colour_cast")) {
                (Some(tint), Some(cast)) => Some(tint + cast / 2.0),
                (tint, cast) => tint.or(cast),
            };
            let context = [
                burst.as_deref().and_then(|g| place(index, g, pixel)),
                moment.as_deref().and_then(|g| place(index, g, pixel)),
                moment
                    .as_deref()
                    .and_then(|g| against_best(index, g, frames, &eyes)),
                moment
                    .as_deref()
                    .and_then(|g| against_best(index, g, frames, &sharp)),
                uniqueness.map(|bits| bits as f32),
                new_scene,
                fit("exposure_mid_level"),
                colour_fit,
                deliberate,
                pixel.get(index).copied().flatten(),
            ];
            for (slot, value) in values
                .iter_mut()
                .skip(crate::cull_checks::CONTEXT)
                .zip(context)
            {
                *slot = value;
            }
            Some(values)
        })
        .collect()
}

/// A frame whose own checks add up to almost nothing is left out, unless a person edited it
/// or a harder rule already named it.
fn drop_low_scores(verdicts: &mut [Verdict], pixel: &[Option<f32>], line: f32) {
    for (verdict, score) in verdicts.iter_mut().zip(pixel) {
        let Some(score) = score else { continue };
        if verdict.reason == Reason::Keep && *score < line {
            verdict.keep = false;
            verdict.reason = Reason::LowScore;
            verdict.detail = format!(
                "Low score: its hundred checks add up to {score:.0} of 100 (below {line:.0}). Open its checks to see which groups failed."
            );
        }
    }
}

/// A gentler level than balanced hands back the low scores the balanced line took.
fn restore_above(verdicts: &mut [Verdict], pixel: &[Option<f32>], line: f32) {
    for (verdict, score) in verdicts.iter_mut().zip(pixel) {
        let Some(score) = score else { continue };
        if verdict.reason == Reason::LowScore && *score >= line {
            verdict.keep = true;
            verdict.reason = Reason::Keep;
            verdict.detail = format!(
                "Delivered at this strictness: its hundred checks add up to {score:.0} of 100."
            );
        }
    }
}

/// Of a moment's frames still delivered, keep the best and those close to it; leave out the
/// clearly weaker ones, naming the frame delivered instead.
fn settle_moment(
    frames: &[Frame],
    verdicts: &mut [Verdict],
    number: usize,
    members: &[usize],
    totals: &[Option<f32>],
    policy: Policy,
) {
    let alive: Vec<usize> = members
        .iter()
        .copied()
        .filter(|i| verdicts.get(*i).is_some_and(|v| v.keep))
        .collect();
    for index in members {
        if let Some(verdict) = verdicts.get_mut(*index) {
            verdict.moment = Some(number);
        }
    }
    if !policy.moments
        || alive.len() < 2
        || alive
            .iter()
            .any(|i| totals.get(*i).copied().flatten().is_none())
    {
        return;
    }
    let edited = |index: usize| frames.get(index).is_some_and(|f| f.edited_by_hand);
    let mut ranked: Vec<(usize, f32)> = alive
        .iter()
        .map(|i| (*i, totals.get(*i).copied().flatten().unwrap_or_default()))
        .collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    let Some(&(best, best_score)) = ranked.first() else {
        return;
    };
    let cap = ((alive.len() as f32 * policy.moment_share).ceil() as usize).max(1);
    let best_name = frames
        .get(best)
        .map(|f| f.file_name.clone())
        .unwrap_or_default();
    let best_id = frames.get(best).map(|f| f.photo_id.clone());
    for (rank, (index, own)) in ranked.iter().enumerate() {
        let close = best_score - own <= policy.moment_margin;
        if rank == 0 || edited(*index) || (close && rank < cap) {
            continue;
        }
        let Some(verdict) = verdicts.get_mut(*index) else {
            continue;
        };
        verdict.keep = false;
        verdict.reason = Reason::WeakerOfMoment;
        verdict.kept_instead.clone_from(&best_id);
        verdict.detail = format!(
            "A similar shot of the same moment, {best_name}, is delivered instead: scored {own:.0} against {best_score:.0} of 100."
        );
    }
}

/// Mark the best-scoring delivered frames. Returns how many.
fn mark_top_picks(verdicts: &mut [Verdict]) -> usize {
    let mut kept: Vec<(usize, f32)> = verdicts
        .iter()
        .enumerate()
        .filter(|(_, v)| v.keep)
        .filter_map(|(i, v)| Some((i, v.score?)))
        .collect();
    kept.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    let wanted = (kept.len() as f32 * TOP_SHARE).ceil() as usize;
    let mut marked = 0;
    for (index, score) in kept.into_iter().take(wanted) {
        if score < TOP_FLOOR {
            break;
        }
        if let Some(verdict) = verdicts.get_mut(index) {
            verdict.top_pick = true;
            marked += 1;
        }
    }
    marked
}

/// Decide the whole collection at the balanced level. `frames` is in timeline order.
#[must_use]
pub fn decide(frames: &[Frame]) -> Report {
    decide_with(frames, Strictness::Balanced)
}

/// Decide the whole collection at one strictness. `frames` is in timeline order.
#[must_use]
pub fn decide_with(frames: &[Frame], strictness: Strictness) -> Report {
    let policy = strictness.policy();
    let mut verdicts = first_verdicts(frames);
    let own = own_values(frames);
    let pixel: Vec<Option<f32>> = scored(&own)
        .iter()
        .map(|s| s.as_ref().map(|s| s.overall))
        .collect();
    // Whether the rules describe a style is judged at the balanced line whatever the level,
    // so a stricter level can never withdraw them and end up keeping more.
    drop_low_scores(&mut verdicts, &pixel, LOW_SCORE);
    let rules_withdrawn = withdraw_if_implausible(&mut verdicts);
    if !rules_withdrawn {
        restore_above(&mut verdicts, &pixel, policy.low_score);
        drop_low_scores(&mut verdicts, &pixel, policy.low_score);
    }
    let bursts = find_groups(frames, &verdicts, BURST);
    let moments = find_groups(frames, &verdicts, MOMENT);
    let full = with_context(frames, &own, &pixel, &bursts, &moments);
    let final_scores = scored(&full);
    let totals: Vec<Option<f32>> = final_scores
        .iter()
        .map(|s| s.as_ref().map(|s| s.overall))
        .collect();
    for (number, members) in bursts.iter().enumerate() {
        settle_burst(frames, &mut verdicts, number + 1, members, &totals, policy);
    }
    for (number, members) in moments.iter().enumerate() {
        settle_moment(frames, &mut verdicts, number + 1, members, &totals, policy);
    }
    for (verdict, (scores, values)) in verdicts.iter_mut().zip(final_scores.into_iter().zip(full)) {
        if let Some(scores) = scores {
            verdict.score = Some((scores.overall * 10.0).round() / 10.0);
            verdict.checks = scores.checks;
            verdict.categories = scores.categories;
        }
        if let (Some(measure), Some(values)) = (verdict.measure.as_mut(), values) {
            measure.checks = values;
        }
    }
    let top_picks = mark_top_picks(&mut verdicts);
    let count = |reason: Reason| verdicts.iter().filter(|v| v.reason == reason).count();
    let counts = Counts {
        frames: frames.len(),
        kept: verdicts.iter().filter(|v| v.keep).count(),
        unusable_exposure: count(Reason::UnusableExposure),
        out_of_focus: count(Reason::OutOfFocus),
        motion_blur: count(Reason::MotionBlur),
        burst_duplicates: count(Reason::BurstDuplicate),
        weaker_of_moment: count(Reason::WeakerOfMoment),
        low_score: count(Reason::LowScore),
        bursts: bursts.len(),
        moments: moments.len(),
        top_picks,
        rules_withdrawn,
    };
    Report {
        strictness,
        thresholds: BTreeMap::from([
            ("analysisEdge", ANALYSIS_EDGE as f32),
            ("defocus", DEFOCUS),
            ("defocusPeak", DEFOCUS_PEAK),
            ("motion", MOTION),
            ("motionRatio", MOTION_RATIO),
            ("blackFrame", BLACK_FRAME),
            ("blownFraction", BLOWN_FRACTION),
            ("burstBits", BURST_BITS as f32),
            ("burstBitsUntimed", BURST_BITS_UNTIMED as f32),
            ("burstGapMs", BURST_GAP_MS as f32),
            ("momentBits", MOMENT_BITS as f32),
            ("momentBitsUntimed", MOMENT_BITS_UNTIMED as f32),
            ("momentGapMs", MOMENT_GAP_MS as f32),
            ("momentMargin", policy.moment_margin),
            ("lowScore", policy.low_score),
            ("sameShot", SAME_SHOT),
            ("topShare", TOP_SHARE),
            ("topFloor", TOP_FLOOR),
            ("implausibleShare", IMPLAUSIBLE_SHARE),
        ]),
        check_ids: crate::cull_checks::CHECKS.iter().map(|c| c.id).collect(),
        counts,
        verdicts,
    }
}

/// Capture times the camera itself recorded, in milliseconds. A file's modification time
/// is not evidence that two frames were taken together, so it is left out.
///
/// # Errors
/// The catalog read failing.
pub fn camera_times(state: &AppState, project_id: &str) -> IpcResult<BTreeMap<String, i64>> {
    let project = project_id.to_owned();
    let rows: Vec<(String, Option<String>, Option<i64>)> = state.catalog().read(move |conn| {
        let mut statement = conn
            .prepare(
                "SELECT photo_id, COALESCE(timeline_time, capture_time), sub_sec FROM photo \
                 WHERE project_id = ?1 AND capture_time_source IN ('exif_original','exif_digitized')",
            )
            .map_err(|e| aura_core::errors::db::statement_failed("cull capture times", &e))?;
        let rows = statement
            .query_map([project], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .map_err(|e| aura_core::errors::db::statement_failed("cull capture times", &e))?
            .filter_map(Result::ok)
            .collect();
        Ok(rows)
    })?;
    let mut times = BTreeMap::new();
    for (photo, stamp, sub_sec) in rows {
        let Some(parsed) = stamp.and_then(|text| {
            time::OffsetDateTime::parse(&text, &time::format_description::well_known::Rfc3339).ok()
        }) else {
            continue;
        };
        // EXIF stores whole seconds; the fraction is separate, in hundredths or milliseconds.
        let fraction = match sub_sec.unwrap_or(0).clamp(0, 999) {
            value if value >= 100 => value,
            value => value * 10,
        };
        times.insert(photo, parsed.unix_timestamp() * 1000 + fraction);
    }
    Ok(times)
}

/// The camera settings the hundred checks read, per photograph.
///
/// # Errors
/// The catalog read failing.
pub fn camera_settings(
    state: &AppState,
    project_id: &str,
) -> IpcResult<BTreeMap<String, crate::cull_checks::Settings>> {
    type Row = (
        String,
        Option<i64>,
        Option<i64>,
        Option<f64>,
        Option<i64>,
        Option<i64>,
        Option<i64>,
    );
    let project = project_id.to_owned();
    let rows: Vec<Row> = state.catalog().read(move |conn| {
        let mut statement = conn
            .prepare(
                "SELECT photo_id, iso, shutter_us, focal_len_mm, flash_fired, width_px, height_px                  FROM photo WHERE project_id = ?1",
            )
            .map_err(|e| aura_core::errors::db::statement_failed("cull camera settings", &e))?;
        let rows = statement
            .query_map([project], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            })
            .map_err(|e| aura_core::errors::db::statement_failed("cull camera settings", &e))?
            .filter_map(Result::ok)
            .collect();
        Ok(rows)
    })?;
    Ok(rows
        .into_iter()
        .map(|(photo, iso, shutter, focal, flash, width, height)| {
            let positive = |value: Option<i64>| value.filter(|v| *v > 0).map(|v| v as f32);
            (
                photo,
                crate::cull_checks::Settings {
                    iso: positive(iso),
                    shutter_s: positive(shutter).map(|us| us / 1_000_000.0),
                    focal_mm: focal.filter(|f| *f > 0.0).map(|f| f as f32),
                    flash: flash.map(|f| f == 1),
                    megapixels: positive(width)
                        .zip(positive(height))
                        .map(|(w, h)| w * h / 1_000_000.0),
                },
            )
        })
        .collect())
}

/// Read one photograph's pixels and measure them: the four hard rules and the hundred checks.
///
/// # Errors
/// An invalid identifier or a preview that cannot be decoded. The caller keeps the frame.
pub fn measure_photo(
    state: &AppState,
    project_id: &str,
    photo: &ImageRowLite,
    times: &BTreeMap<String, i64>,
    settings: &BTreeMap<String, crate::cull_checks::Settings>,
) -> IpcResult<Frame> {
    let id = aura_core::PhotoId::from_db(&photo.id)
        .map_err(|_| aura_core::errors::render::recipe_invalid("photo", "invalid identifier"))?;
    let previews = state.previews(project_id)?;
    let thumb = previews.get(id, aura_raw::PixelLevel::Thumb(512), Priority::Interactive)?;
    // Focus cannot be judged from a thumbnail; the proxy is what the edit reads next anyway.
    let proxy = previews
        .get(id, aura_raw::PixelLevel::Proxy2048, Priority::Interactive)
        .ok();
    let faces = thumb
        .as_srgb8()
        .and_then(|rgb| aura_vision::portrait::detect(rgb, thumb.width, thumb.height).ok())
        .unwrap_or_default();
    let detail = proxy.as_ref().unwrap_or(&thumb);
    let (width, height) = (detail.width as usize, detail.height as usize);
    let measure = detail.as_srgb8().and_then(|rgb| {
        let mut measure = measure_pixels(rgb, width, height, &faces)?;
        measure.checks = crate::cull_checks::measure(&crate::cull_checks::Sample {
            rgb,
            width,
            height,
            faces: &faces,
            settings: settings.get(&photo.id).copied().unwrap_or_default(),
        })
        .unwrap_or_default();
        Some(measure)
    });
    let edited_by_hand = !crate::develop_commands::load_or_neutral(state, id)?
        .provenance
        .user_edited_fields
        .is_empty();
    Ok(Frame {
        photo_id: photo.id.clone(),
        file_name: photo.file_name.clone(),
        measure,
        time_ms: times.get(&photo.id).copied(),
        edited_by_hand,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame of sharp random blocks, optionally box-blurred along x and y. The blocks are
    /// large, as the objects in a photograph are: blur softens their edges, not their contrast.
    fn scene(seed: u32, blur_x: usize, blur_y: usize) -> Vec<u8> {
        let (width, height) = (640_usize, 480_usize);
        let mut luma = vec![0.0_f32; width * height];
        for y in 0..height {
            for x in 0..width {
                let cell = (x / 64) as u32 * 31 + (y / 64) as u32 * 17 + seed * 7919;
                let hashed = cell.wrapping_mul(2_654_435_761).rotate_left(13) % 200;
                luma[y * width + x] = 30.0 + hashed as f32;
            }
        }
        let pass = |plane: &[f32], radius: usize, horizontal: bool| -> Vec<f32> {
            if radius == 0 {
                return plane.to_vec();
            }
            let mut out = vec![0.0; plane.len()];
            for y in 0..height {
                for x in 0..width {
                    let (mut sum, mut count) = (0.0, 0.0);
                    for offset in 0..=2 * radius {
                        let (sx, sy) = if horizontal {
                            ((x + offset).saturating_sub(radius).min(width - 1), y)
                        } else {
                            (x, (y + offset).saturating_sub(radius).min(height - 1))
                        };
                        sum += plane[sy * width + sx];
                        count += 1.0;
                    }
                    out[y * width + x] = sum / count;
                }
            }
            out
        };
        let blurred = pass(&pass(&luma, blur_x, true), blur_y, false);
        blurred.iter().flat_map(|value| [*value as u8; 3]).collect()
    }

    fn frame(name: &str, rgb: &[u8], time_ms: Option<i64>) -> Frame {
        Frame {
            photo_id: name.into(),
            file_name: format!("{name}.jpg"),
            measure: measure_pixels(rgb, 640, 480, &[]),
            time_ms,
            edited_by_hand: false,
        }
    }

    #[test]
    fn a_sharp_frame_is_kept_and_blurred_ones_are_named_for_what_they_are() {
        let sharp = measure_pixels(&scene(1, 0, 0), 640, 480, &[]).expect("measured");
        assert!(technical(&sharp).is_none(), "{sharp:?}");
        let soft = measure_pixels(&scene(1, 12, 12), 640, 480, &[]).expect("measured");
        assert_eq!(technical(&soft).expect("rejected").0, Reason::OutOfFocus);
        assert!(soft.sharpness < sharp.sharpness / 2.0);
        let smeared = measure_pixels(&scene(1, 20, 0), 640, 480, &[]).expect("measured");
        assert_eq!(technical(&smeared).expect("rejected").0, Reason::MotionBlur);
        assert!(smeared.direction_ratio < MOTION_RATIO, "{smeared:?}");
    }

    #[test]
    fn black_and_blown_frames_are_unusable_and_a_tiny_frame_is_not_judged() {
        let black = measure_pixels(&vec![3_u8; 640 * 480 * 3], 640, 480, &[]).expect("measured");
        assert_eq!(
            technical(&black).expect("rejected").0,
            Reason::UnusableExposure
        );
        let white = measure_pixels(&vec![255_u8; 640 * 480 * 3], 640, 480, &[]).expect("measured");
        assert_eq!(
            technical(&white).expect("rejected").0,
            Reason::UnusableExposure
        );
        assert!(measure_pixels(&[0; 27], 3, 3, &[]).is_none());
        assert!(measure_pixels(&[0; 10], 640, 480, &[]).is_none());
    }

    #[test]
    fn a_burst_keeps_its_sharpest_frame_and_names_it() {
        let frames = [
            frame("a", &scene(1, 1, 1), Some(1_000)),
            frame("b", &scene(1, 0, 0), Some(1_400)),
            frame("c", &scene(1, 2, 2), Some(1_900)),
            frame("other", &scene(2, 0, 0), Some(2_300)),
        ];
        let report = decide(&frames);
        assert_eq!(report.kept(), ["b", "other"]);
        assert_eq!(report.counts.bursts, 1);
        assert_eq!(report.counts.burst_duplicates, 2);
        let a = &report.verdicts[0];
        assert_eq!(a.reason, Reason::BurstDuplicate);
        assert_eq!(a.kept_instead.as_deref(), Some("b"));
        assert_eq!(report.verdicts[1].reason, Reason::BestOfBurst);
        assert_eq!(report.verdicts[3].burst, None);
    }

    #[test]
    fn a_burst_survives_another_camera_or_a_rejected_frame_in_the_middle() {
        let frames = [
            frame("a", &scene(1, 1, 1), Some(1_000)),
            frame("second-camera", &scene(7, 0, 0), Some(1_200)),
            frame("b", &scene(1, 2, 2), Some(1_400)),
            frame("blurred", &scene(8, 12, 12), Some(1_600)),
            frame("c", &scene(1, 0, 0), Some(1_900)),
        ];
        let report = decide(&frames);
        assert_eq!(report.kept(), ["second-camera", "c"]);
        assert_eq!(report.counts.bursts, 1);
        assert_eq!(report.counts.burst_duplicates, 2);
        assert_eq!(report.counts.out_of_focus, 1);
        // Frames the camera did not time have no place in the timeline, so one shot is found
        // anywhere by its shape - and a different photograph in between is left alone.
        let untimed = [
            frame("a", &scene(1, 0, 0), None),
            frame("between", &scene(7, 0, 0), None),
            frame("b", &scene(1, 1, 1), None),
        ];
        let report = decide(&untimed);
        assert_eq!(report.counts.bursts, 1);
        assert_eq!(report.verdicts[1].burst, None);
        assert!(report.verdicts[1].keep);
    }

    /// A frame with its hundred checks, as the cull measures one.
    fn checked(name: &str, rgb: &[u8], time_ms: Option<i64>) -> Frame {
        let mut frame = frame(name, rgb, time_ms);
        if let Some(measure) = frame.measure.as_mut() {
            measure.checks = crate::cull_checks::measure(&crate::cull_checks::Sample {
                rgb,
                width: 640,
                height: 480,
                faces: &[],
                settings: crate::cull_checks::Settings::default(),
            })
            .expect("checked");
        }
        frame
    }

    #[test]
    fn a_darker_or_softer_copy_is_the_same_shot_wherever_it_sits() {
        let dark: Vec<u8> = scene(1, 0, 0).iter().map(|v| v / 10).collect();
        let original = measure_pixels(&scene(1, 0, 0), 640, 480, &[]).expect("measured");
        let copy = measure_pixels(&dark, 640, 480, &[]).expect("measured");
        let soft = measure_pixels(&scene(1, 3, 3), 640, 480, &[]).expect("measured");
        let other = measure_pixels(&scene(2, 0, 0), 640, 480, &[]).expect("measured");
        // On real photographs a tenfold darker copy moves the difference hash by up to 22
        // bits; its shape does not move.
        assert!(
            same_shot(&original, &copy),
            "{:?}",
            shape_correlation(&original.shape, &copy.shape)
        );
        assert!(same_shot(&original, &soft));
        assert!(
            !same_shot(&original, &other),
            "{:?}",
            shape_correlation(&original.shape, &other.shape)
        );
        // A camera original and an untimed dark copy far down the collection: one shot, and
        // the copy is the one left out.
        let frames = [
            checked("original", &scene(1, 0, 0), Some(1_000)),
            checked("next", &scene(2, 0, 0), Some(90_000)),
            checked("later", &scene(3, 0, 0), Some(400_000)),
            checked("copy", &dark, None),
        ];
        let report = decide(&frames);
        let copy = &report.verdicts[3];
        assert!(!copy.keep, "{copy:?}");
        assert_eq!(copy.kept_instead.as_deref(), Some("original"));
        assert!(report.verdicts[..3].iter().all(|v| v.keep));
    }

    #[test]
    fn a_shape_survives_the_stored_review() {
        let measure = measure_pixels(&scene(1, 0, 0), 640, 480, &[]).expect("measured");
        assert_eq!(measure.shape.len(), SHAPE_GRID * SHAPE_GRID);
        let text = serde_json::to_string(&measure).expect("json");
        let back: Measure = serde_json::from_str(&text).expect("read back");
        assert_eq!(back, measure);
        // A flat frame has no shape and matches nothing.
        let flat = measure_pixels(&vec![128_u8; 640 * 480 * 3], 640, 480, &[]).expect("measured");
        assert!(flat.shape.is_empty());
        assert!(!same_shot(&flat, &flat));
    }

    #[test]
    fn a_stricter_level_never_delivers_more() {
        let frames = [
            checked("a", &scene(1, 0, 0), Some(0)),
            checked("a2", &scene(1, 1, 1), Some(4_000)),
            checked("a3", &scene(1, 2, 2), Some(8_000)),
            checked("b", &scene(2, 0, 0), Some(90_000)),
            checked("soft", &scene(3, 6, 6), Some(200_000)),
            checked("c", &scene(4, 0, 0), Some(400_000)),
        ];
        let kept: Vec<usize> = Strictness::ALL
            .iter()
            .map(|level| decide_with(&frames, *level).counts.kept)
            .collect();
        assert!(kept[0] >= kept[1] && kept[1] >= kept[2], "{kept:?}");
        assert_eq!(decide(&frames), decide_with(&frames, Strictness::Balanced));
        assert_eq!(
            decide_with(&frames, Strictness::Strict).strictness,
            Strictness::Strict
        );
        for level in Strictness::ALL {
            assert_eq!(Strictness::parse(level.slug()), Some(level));
        }
        assert_eq!(Strictness::parse("harsh"), None);
    }

    #[test]
    fn each_level_reads_a_moment_its_own_way() {
        let frames: Vec<Frame> = (0..6).map(|i| plain(&i.to_string(), false)).collect();
        let totals: Vec<Option<f32>> = [80.0, 77.0, 74.0, 71.0, 60.0, 50.0]
            .iter()
            .map(|v| Some(*v))
            .collect();
        let kept = |level: Strictness| {
            let mut verdicts: Vec<Verdict> = (0..6).map(|i| kept(&i.to_string())).collect();
            settle_moment(
                &frames,
                &mut verdicts,
                1,
                &[0, 1, 2, 3, 4, 5],
                &totals,
                level.policy(),
            );
            verdicts.iter().filter(|v| v.keep).count()
        };
        // Gentle leaves a moment alone; balanced keeps those within 12 points, up to half;
        // strict those within 6, up to a third.
        assert_eq!(kept(Strictness::Gentle), 6);
        assert_eq!(kept(Strictness::Balanced), 3);
        assert_eq!(kept(Strictness::Strict), 2);
    }

    #[test]
    fn the_same_scene_minutes_apart_is_two_photographs_not_a_burst() {
        let frames = [
            frame("first", &scene(1, 0, 0), Some(0)),
            frame("later", &scene(1, 1, 1), Some(600_000)),
        ];
        let report = decide(&frames);
        assert_eq!(report.kept(), ["first", "later"]);
        assert_eq!(report.counts.bursts, 0);
    }

    #[test]
    fn a_hand_edited_or_unmeasured_frame_is_always_delivered() {
        let mut edited = frame("edited", &scene(1, 12, 12), Some(0));
        edited.edited_by_hand = true;
        let mut sibling = frame("sibling", &scene(3, 0, 0), Some(100));
        sibling.measure = None;
        let frames = [
            edited,
            sibling,
            frame("x", &scene(4, 0, 0), None),
            frame("y", &scene(5, 0, 0), None),
            frame("z", &scene(6, 0, 0), None),
        ];
        let report = decide(&frames);
        assert_eq!(report.counts.kept, 5);
        assert_eq!(report.verdicts[0].reason, Reason::EditedByHand);
        assert_eq!(report.verdicts[1].reason, Reason::Unmeasured);
    }

    #[test]
    fn rules_that_reject_most_of_a_collection_are_withdrawn_and_say_so() {
        let frames = [
            frame("a", &scene(1, 12, 12), None),
            frame("b", &scene(2, 12, 12), None),
            frame("c", &scene(3, 0, 0), None),
        ];
        let report = decide(&frames);
        assert!(report.counts.rules_withdrawn);
        assert_eq!(report.counts.kept, 3);
        assert_eq!(report.verdicts[0].reason, Reason::RulesWithdrawn);
        assert!(report.summary().contains("withdrawn"));
        assert!(decide(&[]).kept().is_empty());
    }

    fn kept(name: &str) -> Verdict {
        Verdict {
            photo_id: name.into(),
            file_name: format!("{name}.jpg"),
            keep: true,
            reason: Reason::Keep,
            detail: String::new(),
            burst: None,
            kept_instead: None,
            moment: None,
            score: None,
            top_pick: false,
            checks: Vec::new(),
            categories: Vec::new(),
            measure: None,
        }
    }

    fn plain(name: &str, edited_by_hand: bool) -> Frame {
        Frame {
            photo_id: name.into(),
            file_name: format!("{name}.jpg"),
            measure: Some(Measure::default()),
            time_ms: None,
            edited_by_hand,
        }
    }

    #[test]
    fn a_moment_keeps_its_best_and_close_shots_and_leaves_out_the_weak_ones() {
        let frames = [
            plain("a", false),
            plain("b", false),
            plain("c", false),
            plain("d", true),
        ];
        let mut verdicts: Vec<Verdict> = ["a", "b", "c", "d"].iter().map(|n| kept(n)).collect();
        let totals = [Some(80.0), Some(75.0), Some(50.0), Some(30.0)];
        let balanced = Strictness::Balanced.policy();
        settle_moment(&frames, &mut verdicts, 1, &[0, 1, 2, 3], &totals, balanced);
        let keeps: Vec<bool> = verdicts.iter().map(|v| v.keep).collect();
        // b is within the margin; c is not; d is weak but a person edited it.
        assert_eq!(keeps, [true, true, false, true]);
        assert_eq!(verdicts[2].reason, Reason::WeakerOfMoment);
        assert_eq!(verdicts[2].kept_instead.as_deref(), Some("a"));
        assert!(verdicts.iter().all(|v| v.moment == Some(1)));
        // Without scores for every frame nothing is decided by score.
        let mut unscored: Vec<Verdict> = ["a", "b"].iter().map(|n| kept(n)).collect();
        settle_moment(
            &frames,
            &mut unscored,
            2,
            &[0, 1],
            &[Some(90.0), None],
            balanced,
        );
        assert!(unscored.iter().all(|v| v.keep));
    }

    #[test]
    fn a_moment_never_keeps_more_than_half_of_its_close_shots_beside_the_best() {
        let frames: Vec<Frame> = (0..6).map(|i| plain(&i.to_string(), false)).collect();
        let mut verdicts: Vec<Verdict> = (0..6).map(|i| kept(&i.to_string())).collect();
        let totals: Vec<Option<f32>> = (0..6).map(|i| Some(80.0 - i as f32)).collect();
        let balanced = Strictness::Balanced.policy();
        settle_moment(
            &frames,
            &mut verdicts,
            1,
            &[0, 1, 2, 3, 4, 5],
            &totals,
            balanced,
        );
        assert_eq!(verdicts.iter().filter(|v| v.keep).count(), 3);
    }

    #[test]
    fn a_very_low_score_is_left_out_but_never_a_hand_edited_frame() {
        let mut verdicts = vec![kept("a"), kept("b"), kept("c")];
        verdicts[1].reason = Reason::EditedByHand;
        let scores = [Some(10.0), Some(10.0), Some(70.0)];
        drop_low_scores(&mut verdicts, &scores, LOW_SCORE);
        assert_eq!(verdicts[0].reason, Reason::LowScore);
        assert!(!verdicts[0].keep);
        assert!(verdicts[1].keep && verdicts[2].keep);
        // A gentler line hands it back.
        restore_above(&mut verdicts, &scores, 5.0);
        assert!(verdicts[0].keep);
        assert_eq!(verdicts[0].reason, Reason::Keep);
    }

    #[test]
    fn top_picks_are_the_best_fifth_and_must_score_well() {
        let mut verdicts: Vec<Verdict> = (0..10).map(|i| kept(&i.to_string())).collect();
        for (i, verdict) in verdicts.iter_mut().enumerate() {
            verdict.score = Some(10.0 + 10.0 * i as f32);
        }
        assert_eq!(mark_top_picks(&mut verdicts), 2);
        assert!(verdicts[9].top_pick && verdicts[8].top_pick && !verdicts[7].top_pick);
        let mut weak: Vec<Verdict> = (0..5).map(|i| kept(&i.to_string())).collect();
        for verdict in &mut weak {
            verdict.score = Some(40.0);
        }
        assert_eq!(mark_top_picks(&mut weak), 0);
    }

    #[test]
    fn every_checked_frame_gets_a_hundred_scores_and_its_neighbours_are_read() {
        let frames = [
            checked("a", &scene(1, 0, 0), Some(0)),
            checked("b", &scene(2, 0, 0), Some(60_000)),
            checked("c", &scene(3, 0, 0), Some(400_000)),
        ];
        let report = decide(&frames);
        assert_eq!(report.check_ids.len(), crate::cull_checks::COUNT);
        for verdict in &report.verdicts {
            assert!(verdict.score.is_some(), "{verdict:?}");
            assert_eq!(verdict.checks.len(), crate::cull_checks::COUNT);
            let values = &verdict.measure.as_ref().expect("measured").checks;
            assert!(values[slot("context_uniqueness")].is_some());
            assert!(values[slot("context_collection_rank")].is_some());
        }
        // The first frame, and one after a long gap, open a scene.
        let new_scene = slot("context_new_scene");
        let scene_of = |i: usize| report.verdicts[i].measure.as_ref().unwrap().checks[new_scene];
        assert_eq!(scene_of(0), Some(1.0));
        assert_eq!(scene_of(2), Some(1.0));
        assert!(report.summary().contains("top picks"));
    }

    #[test]
    fn a_blink_loses_to_open_eyes_in_the_same_burst() {
        let open = Measure {
            sharpness: 0.2,
            face_sharpness: Some(0.2),
            eye_contrast: Some(1.0),
            ..Measure::default()
        };
        let blink = Measure {
            face_sharpness: Some(0.21),
            eye_contrast: Some(0.5),
            ..open.clone()
        };
        assert!(quality(&open, Some(1.0)) > quality(&blink, Some(1.0)));
        // Without eyes to compare, the sharper frame wins.
        assert!(quality(&blink, None) > quality(&open, None));
    }
}
