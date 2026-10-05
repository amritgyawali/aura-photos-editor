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
//! It only ever errs toward keeping. A frame a person edited by hand is never rejected, a
//! frame that cannot be measured is kept, a run in which the technical rules would discard
//! an implausible share of the wedding withdraws them, and nothing is deleted: a rejected
//! photograph stays in the collection and is simply not delivered by this run.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use std::collections::BTreeMap;

use aura_preview::contract::service::{PreviewService, Priority};
use aura_vision::portrait::PortraitFace;
use serde::Serialize;

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
/// Above this share of technical rejections the rules are judging a style, not a mistake.
const IMPLAUSIBLE_SHARE: f32 = 0.40;

/// Why a frame was kept or left out. The slug is stable; the sentence is for people.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
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
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
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
    #[serde(serialize_with = "hex")]
    pub dhash: u64,
}

fn hex<S: serde::Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&format!("{value:016x}"))
}

/// One frame as the decision sees it.
#[derive(Debug, Clone, PartialEq)]
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
    /// For a burst duplicate, the frame delivered instead.
    pub kept_instead: Option<String>,
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
    /// Bursts found.
    pub bursts: usize,
    /// The technical rules were withdrawn because they rejected too much.
    pub rules_withdrawn: bool,
}

/// The whole cull, as written to `photo-cull.json`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// What the numbers below were judged against.
    pub thresholds: BTreeMap<&'static str, f32>,
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
            "Measured cull: delivering {} of {} photographs. Left out {} burst duplicate(s) of a sharper frame, {} out of focus, {} motion blurred and {} with unusable exposure. Nothing was deleted; every decision and its measurements are in photo-cull.json.",
            c.kept, c.frames, c.burst_duplicates, c.out_of_focus, c.motion_blur, c.unusable_exposure
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
                let mut channels = pixel.iter().map(|value| f32::from(*value));
                let (r, g, b) = (
                    channels.next().unwrap_or_default(),
                    channels.next().unwrap_or_default(),
                    channels.next().unwrap_or_default(),
                );
                if let Some(cell) = values.get_mut(y * plane_width + x) {
                    *cell += 0.2126 * r + 0.7152 * g + 0.0722 * b;
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

fn same_burst(a: &Frame, b: &Frame) -> bool {
    let (Some(first), Some(second)) = (&a.measure, &b.measure) else {
        return false;
    };
    let bits = (first.dhash ^ second.dhash).count_ones();
    match (a.time_ms, b.time_ms) {
        (Some(earlier), Some(later)) => {
            bits <= BURST_BITS && (later - earlier).abs() <= BURST_GAP_MS
        }
        _ => bits <= BURST_BITS_UNTIMED,
    }
}

/// Decide the whole collection. `frames` is in timeline order.
#[must_use]
pub fn decide(frames: &[Frame]) -> Report {
    let mut verdicts: Vec<Verdict> = frames
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
                measure: frame.measure.clone(),
            }
        })
        .collect();

    // A wedding where the focus and exposure rules reject a large share is a deliberate
    // style (soft film scans, a dark reception shot wide open), not a run of mistakes.
    let technical_rejects = verdicts.iter().filter(|v| !v.keep).count();
    let rules_withdrawn =
        !frames.is_empty() && technical_rejects as f32 / frames.len() as f32 > IMPLAUSIBLE_SHARE;
    if rules_withdrawn {
        for verdict in verdicts.iter_mut().filter(|v| !v.keep) {
            verdict.detail = format!("Delivered anyway. Would have been: {}", verdict.detail);
            verdict.reason = Reason::RulesWithdrawn;
            verdict.keep = true;
        }
    }

    // Bursts are runs of consecutive deliverable frames that are near-identical.
    let mut bursts: Vec<Vec<usize>> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    for index in 0..frames.len() {
        let groupable = |i: usize| {
            verdicts
                .get(i)
                .is_some_and(|v| v.keep && v.reason != Reason::Unmeasured)
        };
        let joins = current.last().is_some_and(|last| {
            groupable(index)
                && frames
                    .get(*last)
                    .zip(frames.get(index))
                    .is_some_and(|(a, b)| same_burst(a, b))
        });
        if !joins {
            if current.len() > 1 {
                bursts.push(std::mem::take(&mut current));
            }
            current.clear();
        }
        if groupable(index) {
            current.push(index);
        }
    }
    if current.len() > 1 {
        bursts.push(current);
    }

    for (number, members) in bursts.iter().enumerate() {
        let measures: Vec<(usize, &Measure)> = members
            .iter()
            .filter_map(|i| frames.get(*i)?.measure.as_ref().map(|m| (*i, m)))
            .collect();
        let best_eyes = measures
            .iter()
            .filter_map(|(_, m)| m.eye_contrast)
            .reduce(f32::max);
        let mut ranked: Vec<(usize, f32)> = measures
            .iter()
            .map(|(i, m)| (*i, quality(m, best_eyes)))
            .collect();
        // A hand-edited frame is always delivered, so it leads its burst.
        ranked.sort_by(|a, b| {
            let edited = |i: usize| frames.get(i).is_some_and(|f| f.edited_by_hand);
            edited(b.0)
                .cmp(&edited(a.0))
                .then(b.1.total_cmp(&a.1))
                .then(a.0.cmp(&b.0))
        });
        let keepers = match members.len() {
            0..=5 => 1,
            6..=12 => 2,
            _ => 3,
        };
        let Some(&(best, best_quality)) = ranked.first() else {
            continue;
        };
        let best_name = frames
            .get(best)
            .map(|f| f.file_name.clone())
            .unwrap_or_default();
        let best_id = frames.get(best).map(|f| f.photo_id.clone());
        for (rank, (index, own)) in ranked.iter().enumerate() {
            let edited = frames.get(*index).is_some_and(|f| f.edited_by_hand);
            let Some(verdict) = verdicts.get_mut(*index) else {
                continue;
            };
            verdict.burst = Some(number + 1);
            if rank < keepers || edited {
                if !edited && verdict.reason == Reason::Keep {
                    verdict.reason = Reason::BestOfBurst;
                    verdict.detail = format!(
                        "Best of a burst of {}: quality {own:.3} from sharpness and open eyes.",
                        members.len()
                    );
                }
            } else {
                verdict.keep = false;
                verdict.reason = Reason::BurstDuplicate;
                verdict.kept_instead.clone_from(&best_id);
                verdict.detail = format!(
                    "Near-identical to {best_name}, which is delivered instead: quality {own:.3} against {best_quality:.3}."
                );
            }
        }
    }

    let count = |reason: Reason| verdicts.iter().filter(|v| v.reason == reason).count();
    let counts = Counts {
        frames: frames.len(),
        kept: verdicts.iter().filter(|v| v.keep).count(),
        unusable_exposure: count(Reason::UnusableExposure),
        out_of_focus: count(Reason::OutOfFocus),
        motion_blur: count(Reason::MotionBlur),
        burst_duplicates: count(Reason::BurstDuplicate),
        bursts: bursts.len(),
        rules_withdrawn,
    };
    Report {
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
            ("implausibleShare", IMPLAUSIBLE_SHARE),
        ]),
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

/// Read one photograph's pixels and measure them.
///
/// # Errors
/// An invalid identifier or a preview that cannot be decoded. The caller keeps the frame.
pub fn measure_photo(
    state: &AppState,
    project_id: &str,
    photo: &ImageRowLite,
    times: &BTreeMap<String, i64>,
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
    let measure = detail
        .as_srgb8()
        .and_then(|rgb| measure_pixels(rgb, detail.width as usize, detail.height as usize, &faces));
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
