//! Finding faces in a photograph that was not taken for a face detector.
//!
//! A wedding is the hard case, and it is hard in a specific way. The bride at a
//! first look fills a third of the frame; a guest four rows back in a wide
//! ceremony frame is forty pixels tall; the room is dark; half the faces are
//! turned away; and a string of bokeh fairy lights across the back wall looks,
//! to a detector, exactly like a row of small out-of-focus faces. Section 10.1
//! asks for recall at or above 0.97 overall, 0.90 on the small-face subset, and
//! fewer than one per cent false positives on bokeh highlights - three
//! requirements that pull in opposite directions.
//!
//! Four decisions in this module are how they are reconciled.
//!
//! **Letterbox, never centre-crop.** The embedding path in
//! [`crate::embed::model`] centre-crops to a square, and it is right to: an
//! embedding is about what the photograph is *of*, and the long edges of a
//! portrait frame are usually background. A face detector cannot do that. The
//! faces the tiled pass exists to recover are exactly the ones at the left and
//! right edges of a wide ceremony frame, and cropping them off before inference
//! would make the whole multi-scale design pointless. So the frame is scaled to
//! fit and padded with black.
//!
//! **Three strides, one pass.** The detector emits at strides 8, 16 and 32 from
//! one forward pass. A single-scale detector at 640 px cannot regress both a
//! 200 px face and a 40 px one, because the receptive field that sees the first
//! swallows the second whole.
//!
//! **The tiled pass is conditional and measured.** Section 12 names "tiled
//! detection doubles cost" as a failure mode, so [`should_tile`] is a decision
//! with reasons, its outcome is stored per frame in `face_scan.tiled`, and the
//! phase gate reports the ratio. It fires on wide-angle frames with many small
//! detections, and on the case that matters most: a frame where bodies were found
//! and faces were not.
//!
//! **Bokeh is rejected by geometry, not by score.** A blurred highlight is round,
//! has no landmark structure, and produces five landmarks that collapse towards
//! its centre. [`Detection::landmark_spread`] measures that collapse, and
//! [`DetectConfig::min_landmark_spread`] refuses it. Raising the objectness
//! threshold instead would cost small-face recall, which is the trade this
//! product cannot afford.
//!
//! ## What the shipped model is
//!
//! `face_detect` 1.0.0 is a placeholder with the architecture of an SCRFD and none
//! of its training. It exercises every line of this module - the letterbox, the
//! anchor decode, the channel layout, the non-maximum suppression, the tiled pass,
//! the batch ledger - and it finds no faces in a photograph. The deterministic
//! reference detector in [`crate::face::fixtures`] is what the tests and the phase
//! gate measure recall against. This is condition C1 in
//! `docs/progress/PHASE-06-EXIT.md` and it is a Sev 2 trigger.

use std::sync::Arc;

use aura_core::{AuraResult, Priority};
use aura_infer::contract::infer::{InferService, ModelRef, TensorView, Version};
use aura_infer::onnx::fixtures::{
    face_head_side, FACE_DETECT_INPUT_SIDE, FACE_DETECT_MODEL, FACE_DETECT_STRIDES,
    FACE_HEAD_CHANNELS,
};

use crate::embed::descriptors::box_resize;
use crate::face::align::{Landmarks, LANDMARK_COUNT};

/// Registry name of the detector.
pub const DETECT_MODEL: &str = FACE_DETECT_MODEL;

/// Pinned version of the detector.
pub const DETECT_MODEL_VERSION: Version = Version::new(1, 0, 0);

/// The detector version stored on every `faces` and `face_scan` row.
///
/// Derived from the pinned version so the two cannot drift, exactly as
/// `crate::embed::model::MODEL_VER` is: `1.0.0` is 100.
pub const MODEL_VER: u16 =
    DETECT_MODEL_VERSION.major * 100 + DETECT_MODEL_VERSION.minor * 10 + DETECT_MODEL_VERSION.patch;

/// The detector preprocessing generation.
///
/// Bumped by any change to the letterbox, the padding colour, the resampler or the
/// tile layout. A change here re-scans every frame, because a detection found
/// under one preprocessing path is not comparable with one found under another.
pub const PREPROCESS_VER: u16 = 1;

/// Side of the square tensor the detector takes.
pub const INPUT_SIDE: usize = FACE_DETECT_INPUT_SIDE;

/// Channel index of the face objectness logit.
const CH_FACE_SCORE: usize = 0;
/// Channel index of the person objectness logit.
const CH_PERSON_SCORE: usize = 1;
/// First channel of the face box regression.
const CH_FACE_BOX: usize = 2;
/// First channel of the person box regression.
const CH_PERSON_BOX: usize = 6;
/// First channel of the landmark regression.
const CH_LANDMARKS: usize = 10;

/// A box in normalised frame coordinates: `x`, `y`, `w`, `h`, all `0..1`.
///
/// Normalised because a box has to survive a change of preview tier. The same face
/// is found on a 384 px thumbnail during the cheap pass and re-measured on a
/// 2048 px proxy when a later phase wants a mask, and a box in pixels would be
/// wrong in one of those two places.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct NormBox {
    /// Left edge, `0..1`.
    pub x: f32,
    /// Top edge, `0..1`.
    pub y: f32,
    /// Width, `0..1`.
    pub w: f32,
    /// Height, `0..1`.
    pub h: f32,
}

impl NormBox {
    /// A box from two corners, ordered and clamped into the frame.
    #[must_use]
    pub fn from_corners(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        let x0 = left.min(right).clamp(0.0, 1.0);
        let y0 = top.min(bottom).clamp(0.0, 1.0);
        let x1 = left.max(right).clamp(0.0, 1.0);
        let y1 = top.max(bottom).clamp(0.0, 1.0);
        Self {
            x: x0,
            y: y0,
            w: x1 - x0,
            h: y1 - y0,
        }
    }

    /// Right edge.
    #[must_use]
    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    /// Bottom edge.
    #[must_use]
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    /// Centre point.
    #[must_use]
    pub fn centre(&self) -> [f32; 2] {
        [self.x + self.w * 0.5, self.y + self.h * 0.5]
    }

    /// Fraction of the frame this box covers.
    #[must_use]
    pub fn area_frac(&self) -> f32 {
        (self.w * self.h).clamp(0.0, 1.0)
    }

    /// Intersection over union with another box.
    #[must_use]
    pub fn iou(&self, other: &Self) -> f32 {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        if right <= left || bottom <= top {
            return 0.0;
        }
        let overlap = (right - left) * (bottom - top);
        let union = self.w * self.h + other.w * other.h - overlap;
        if union <= f32::EPSILON {
            return 0.0;
        }
        (overlap / union).clamp(0.0, 1.0)
    }

    /// How central this box is, `0..1`, one at the frame centre.
    ///
    /// A cosine-tapered radial distance rather than a linear one, because a
    /// photographer's sense of "centred" is flat in the middle of the frame and
    /// falls away at the edges - a subject a tenth of the way off centre does not
    /// read as ten per cent less central. The taper is normalised by the distance
    /// from the centre to a corner, so a portrait and a landscape frame agree.
    #[must_use]
    pub fn centrality(&self) -> f32 {
        let [cx, cy] = self.centre();
        let dx = (cx - 0.5) * 2.0;
        let dy = (cy - 0.5) * 2.0;
        let radius = ((dx * dx + dy * dy).sqrt() / std::f32::consts::SQRT_2).clamp(0.0, 1.0);
        ((radius * std::f32::consts::PI).cos() * 0.5 + 0.5).clamp(0.0, 1.0)
    }
}

/// How a frame, or a tile of one, was fitted into the detector's square input.
///
/// Kept so a detection can be mapped back. The mapping is the only place a
/// coordinate-system mistake can hide, so it is one function used by every path -
/// the full pass and all four tiles - rather than arithmetic repeated per caller.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Letterbox {
    /// Source region left edge, in frame pixels.
    pub region_x: usize,
    /// Source region top edge, in frame pixels.
    pub region_y: usize,
    /// Source region width, in frame pixels.
    pub region_w: usize,
    /// Source region height, in frame pixels.
    pub region_h: usize,
    /// Whole frame width, in pixels.
    pub frame_w: usize,
    /// Whole frame height, in pixels.
    pub frame_h: usize,
    /// Region pixels per detector pixel, inverted: detector = region * scale.
    pub scale: f32,
    /// Horizontal padding, in detector pixels.
    pub pad_x: f32,
    /// Vertical padding, in detector pixels.
    pub pad_y: f32,
}

impl Letterbox {
    /// Map a point in detector pixels to normalised frame coordinates.
    #[must_use]
    pub fn to_frame(&self, point: [f32; 2]) -> [f32; 2] {
        if self.scale <= f32::EPSILON || self.frame_w == 0 || self.frame_h == 0 {
            return [0.0, 0.0];
        }
        let rx = (point[0] - self.pad_x) / self.scale;
        let ry = (point[1] - self.pad_y) / self.scale;
        [
            (self.region_x as f32 + rx) / self.frame_w as f32,
            (self.region_y as f32 + ry) / self.frame_h as f32,
        ]
    }

    /// Frame pixels per detector pixel. What decides whether a detection is above
    /// the pixel-height gate.
    #[must_use]
    pub fn source_pixels_per_detector_pixel(&self) -> f32 {
        if self.scale <= f32::EPSILON {
            return 1.0;
        }
        1.0 / self.scale
    }
}

/// Where a detection came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoundBy {
    /// The whole-frame pass.
    Full,
    /// Only the 2x2 tiled pass found it.
    Tile,
}

impl FoundBy {
    /// Stable text for the `found_by` column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Tile => "tile",
        }
    }
}

/// One decoded detection, in normalised frame coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Detection {
    /// The face box.
    pub face: NormBox,
    /// The body box predicted by the same anchor, when the person channel fired.
    pub person: Option<NormBox>,
    /// Face objectness, `0..1`.
    pub face_score: f32,
    /// Person objectness, `0..1`.
    pub person_score: f32,
    /// The five landmarks, normalised to the frame.
    pub landmarks: Landmarks,
    /// Which stride produced it. Kept for the telemetry event and for the
    /// small-face analysis in the benchmark report.
    pub stride: usize,
    /// Which pass produced it.
    pub found_by: FoundBy,
}

impl Detection {
    /// How far the landmarks spread, relative to the box.
    ///
    /// The bokeh test. A real face puts two eyes near the top of the box and two
    /// mouth corners near the bottom, so the mean distance from the landmark
    /// centroid is a substantial fraction of the box. A blurred highlight has no
    /// structure for the head to latch onto, and its five landmarks pile up near
    /// the centre - which is what this measures, in units of the box's diagonal.
    #[must_use]
    pub fn landmark_spread(&self) -> f32 {
        let diagonal = (self.face.w * self.face.w + self.face.h * self.face.h).sqrt();
        if diagonal <= f32::EPSILON {
            return 0.0;
        }
        let mut centre = [0.0f32; 2];
        for point in &self.landmarks {
            centre[0] += point[0];
            centre[1] += point[1];
        }
        centre[0] /= LANDMARK_COUNT as f32;
        centre[1] /= LANDMARK_COUNT as f32;

        let mut spread = 0.0f32;
        for point in &self.landmarks {
            let dx = point[0] - centre[0];
            let dy = point[1] - centre[1];
            spread += (dx * dx + dy * dy).sqrt();
        }
        (spread / LANDMARK_COUNT as f32 / diagonal).clamp(0.0, 4.0)
    }

    /// Height in source pixels, for the 48 px gate.
    #[must_use]
    pub fn px_height(&self, frame_height: usize) -> u32 {
        (self.face.h * frame_height as f32).round().max(0.0) as u32
    }
}

/// Everything that governs a detection pass.
///
/// A struct rather than constants at the call site, because section 10.1's three
/// numbers are in tension and QA has to be able to move one and see the other two
/// change. The defaults are the shipped policy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectConfig {
    /// Minimum face objectness to consider a detection at all.
    pub min_face_score: f32,
    /// Minimum person objectness to record a body box.
    pub min_person_score: f32,
    /// Intersection-over-union above which two detections are the same face.
    pub nms_iou: f32,
    /// Minimum landmark spread, in box diagonals. The bokeh gate.
    pub min_landmark_spread: f32,
    /// Faces per frame, after suppression. A hard cap so a pathological frame
    /// cannot allocate without bound.
    pub max_faces: usize,
    /// A detection whose height is below this fraction of the frame counts as
    /// small when [`should_tile`] is deciding.
    pub small_face_fraction: f32,
    /// How many small detections a frame needs before the tiled pass is worth it.
    pub tile_min_small_faces: usize,
    /// A frame whose 35 mm-equivalent focal length is at or below this is wide.
    pub wide_angle_mm: f32,
}

impl Default for DetectConfig {
    fn default() -> Self {
        Self {
            // Low, deliberately. Section 10.1 wants 0.97 recall, and the false
            // positives that a low threshold admits are removed by the landmark
            // spread gate rather than by refusing to look.
            min_face_score: 0.30,
            min_person_score: 0.40,
            // 0.35 rather than the usual 0.5: wedding group shots put cheeks
            // against cheeks, and a 0.5 threshold merges two guests in a hug into
            // one detection. Faces overlap far less than pedestrians do.
            nms_iou: 0.35,
            // The bokeh gate. The number comes from arithmetic rather than from taste,
            // so it is worth writing out.
            //
            // A face whose landmarks sit on the ArcFace reference proportions has a mean
            // landmark-to-centroid distance of 0.0234 box widths against a box diagonal
            // of 0.172, which is a spread of **0.136**. That is the value a correctly
            // detected face produces, and it is a property of the reference geometry
            // rather than of any particular photograph.
            //
            // A blurred highlight has no structure for the landmark head to latch onto,
            // so its five points pile up near the centre and its spread is **near zero**.
            //
            // 0.08 sits between the two with a factor of 1.7 of margin on the face side,
            // which leaves room for a real detector's landmark noise and for a partly
            // occluded face - and is still far above anything a structureless blob
            // produces. `crates/aura-vision/tests/face_geometry.rs` asserts both ends.
            min_landmark_spread: 0.08,
            // Sixty is past the "60-face frame" the standing test matrix asks
            // about, and small enough that a runaway head cannot exhaust memory.
            max_faces: 64,
            small_face_fraction: 0.06,
            tile_min_small_faces: 3,
            wide_angle_mm: 35.0,
        }
    }
}

/// What the caller knows about the frame that the pixels do not say.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FrameHint {
    /// 35 mm-equivalent focal length, when EXIF gave one.
    pub focal_len_mm: Option<f32>,
    /// Person boxes the full pass found. A frame with bodies and no faces is the
    /// strongest possible argument for tiling.
    pub person_boxes: usize,
}

/// The reasons a tiling decision went the way it did.
///
/// Invariant 2 applies to a decision about *compute* as much as to one about a
/// photograph: "why did this wedding take twice as long as that one" is a support
/// question, and the answer is in this struct.
#[derive(Debug, Clone, PartialEq)]
pub struct TileDecision {
    /// Whether the tiled pass should run.
    pub tile: bool,
    /// Why, in the order the tests read them.
    pub reasons: Vec<String>,
}

/// Decide whether one frame earns the four extra forward passes.
///
/// Two ways to earn it, and they are different arguments:
///
/// 1. **Small faces on a wide frame.** Several detections already sit below
///    `small_face_fraction` of the frame height, and the lens was wide. Those are
///    the frames where a 640 px pass has thrown away the pixels that separate one
///    guest from another, and a tile at the same 640 px gets them back.
/// 2. **Bodies without faces.** The person channel fired and the face channel did
///    not. Either the faces are turned away - in which case tiling changes nothing
///    and costs four passes - or they are too small to see at all, which is the
///    case that produces a wedding with a hundred unattributed guests. Paying four
///    passes to find out is the right trade, and it is why this branch does not
///    also require a wide lens.
#[must_use]
pub fn should_tile(
    detections: &[Detection],
    hint: &FrameHint,
    frame_width: usize,
    frame_height: usize,
    config: &DetectConfig,
) -> TileDecision {
    let mut reasons = Vec::new();

    let small = detections
        .iter()
        .filter(|detection| detection.face.h < config.small_face_fraction)
        .count();
    let wide = hint
        .focal_len_mm
        .is_none_or(|focal| focal <= config.wide_angle_mm);
    let aspect = if frame_height == 0 {
        1.0
    } else {
        frame_width as f32 / frame_height as f32
    };

    if small >= config.tile_min_small_faces && wide {
        reasons.push(format!(
            "{small} detections are shorter than {:.0}% of the frame and the lens was wide",
            config.small_face_fraction * 100.0
        ));
        return TileDecision {
            tile: true,
            reasons,
        };
    }

    if hint.person_boxes >= 2 && detections.is_empty() {
        reasons.push(format!(
            "{} bodies were found and no faces were; the faces may be below the full pass's \
             resolution",
            hint.person_boxes
        ));
        return TileDecision {
            tile: true,
            reasons,
        };
    }

    if small > 0 && !wide {
        reasons.push(format!(
            "{small} small detections, but the lens was {:.0} mm and not wide",
            hint.focal_len_mm.unwrap_or(0.0)
        ));
    } else if small < config.tile_min_small_faces {
        reasons.push(format!(
            "{small} small detections is below the {} needed to pay for four extra passes",
            config.tile_min_small_faces
        ));
    }
    let _ = aspect;
    TileDecision {
        tile: false,
        reasons,
    }
}

/// The four overlapping tiles of a 2x2 pass.
///
/// The overlap is [`TILE_OVERLAP`] of each axis, and it is not decoration: a face
/// that straddles a tile boundary is cut in half by both tiles, and a detector
/// given half a face either misses it or regresses a box around the half it can
/// see. With the overlap, every point in the frame is whole in at least one tile,
/// and the cross-pass suppression in [`nms`] removes the duplicate.
pub const TILE_OVERLAP: f32 = 0.12;

/// The 2x2 tile regions of a frame, as `(x, y, w, h)` in pixels.
#[must_use]
pub fn tiles(width: usize, height: usize) -> Vec<(usize, usize, usize, usize)> {
    if width < 4 || height < 4 {
        return Vec::new();
    }
    let overlap_x = (width as f32 * TILE_OVERLAP) as usize;
    let overlap_y = (height as f32 * TILE_OVERLAP) as usize;
    let half_w = width / 2;
    let half_h = height / 2;

    let tile_w = (half_w + overlap_x).min(width);
    let tile_h = (half_h + overlap_y).min(height);
    let right_x = width.saturating_sub(tile_w);
    let bottom_y = height.saturating_sub(tile_h);

    vec![
        (0, 0, tile_w, tile_h),
        (right_x, 0, tile_w, tile_h),
        (0, bottom_y, tile_w, tile_h),
        (right_x, bottom_y, tile_w, tile_h),
    ]
}

/// Fit one region of an 8-bit sRGB frame into the detector's square input.
///
/// Returns the NCHW tensor and the mapping needed to bring detections back.
///
/// The resampler is the area average from [`box_resize`], the same one the
/// embedding path uses, for the same reason: a 6000 px frame going to 640 px is a
/// downscale of nine, and a bilinear downscale at that ratio aliases. An aliased
/// input makes a 40 px face appear and disappear depending on where it fell
/// relative to the sampling grid, which is precisely the small-face recall this
/// module is trying to defend.
#[must_use]
pub fn preprocess(
    rgb: &[u8],
    width: usize,
    height: usize,
    region: (usize, usize, usize, usize),
) -> (Vec<f32>, Letterbox) {
    let side = INPUT_SIDE;
    let (region_x, region_y, region_w, region_h) = region;
    let region_w = region_w.min(width.saturating_sub(region_x));
    let region_h = region_h.min(height.saturating_sub(region_y));

    let mut tensor = vec![0.0f32; 3 * side * side];
    if region_w == 0 || region_h == 0 {
        return (
            tensor,
            Letterbox {
                region_x,
                region_y,
                region_w,
                region_h,
                frame_w: width,
                frame_h: height,
                scale: 1.0,
                pad_x: 0.0,
                pad_y: 0.0,
            },
        );
    }

    let scale = (side as f32 / region_w as f32).min(side as f32 / region_h as f32);
    let target_w = ((region_w as f32 * scale).round() as usize).clamp(1, side);
    let target_h = ((region_h as f32 * scale).round() as usize).clamp(1, side);
    let pad_x = ((side - target_w) / 2) as f32;
    let pad_y = ((side - target_h) / 2) as f32;

    for channel in 0..3 {
        let mut plane = Vec::with_capacity(region_w * region_h);
        for row in 0..region_h {
            for column in 0..region_w {
                let index = ((region_y + row) * width + (region_x + column)) * 3 + channel;
                plane.push(f32::from(rgb.get(index).copied().unwrap_or(0)) / 255.0);
            }
        }
        let resized = box_resize(&plane, region_w, region_h, target_w, target_h);
        for row in 0..target_h {
            for column in 0..target_w {
                let destination = channel * side * side
                    + (row + pad_y as usize) * side
                    + (column + pad_x as usize);
                if let (Some(slot), Some(value)) = (
                    tensor.get_mut(destination),
                    resized.get(row * target_w + column),
                ) {
                    *slot = *value;
                }
            }
        }
    }

    (
        tensor,
        Letterbox {
            region_x,
            region_y,
            region_w,
            region_h,
            frame_w: width,
            frame_h: height,
            scale,
            pad_x,
            pad_y,
        },
    )
}

/// The logistic function, on one logit.
fn logistic(logit: f32) -> f32 {
    if logit >= 0.0 {
        let exponent = (-logit).exp();
        1.0 / (1.0 + exponent)
    } else {
        // Evaluated this way round for a large negative logit: `exp(-logit)` would
        // overflow to infinity and `1/(1+inf)` is zero, which is nearly right but
        // loses every bit of the tail. This form keeps it.
        let exponent = logit.exp();
        exponent / (1.0 + exponent)
    }
}

/// Turn one stride's head tensor into detections.
///
/// `head` is `[1, 20, side, side]` flattened, `side` is `640 / stride`, and the
/// channel layout is the one documented on
/// `aura_infer::onnx::fixtures::FACE_HEAD_CHANNELS`. Nothing here reads a shape
/// from the tensor: the shape is a contract, and a head that disagrees with it
/// produces no detections rather than detections at the wrong scale.
#[must_use]
pub fn decode_stride(
    head: &[f32],
    stride: usize,
    letterbox: &Letterbox,
    config: &DetectConfig,
    found_by: FoundBy,
) -> Vec<Detection> {
    let side = face_head_side(stride);
    if side == 0 || head.len() < FACE_HEAD_CHANNELS * side * side {
        return Vec::new();
    }
    let plane = side * side;
    let at = |channel: usize, row: usize, column: usize| -> f32 {
        head.get(channel * plane + row * side + column)
            .copied()
            .unwrap_or(0.0)
    };

    let mut out = Vec::new();
    for row in 0..side {
        for column in 0..side {
            let face_score = logistic(at(CH_FACE_SCORE, row, column));
            let person_score = logistic(at(CH_PERSON_SCORE, row, column));
            if face_score < config.min_face_score && person_score < config.min_person_score {
                continue;
            }

            // Anchor centre in detector pixels.
            let cx = (column as f32 + 0.5) * stride as f32;
            let cy = (row as f32 + 0.5) * stride as f32;
            let unit = stride as f32;

            // Distances are non-negative by construction. A raw regression can be
            // negative; clamping is what a real SCRFD's integral head does with a
            // softmax, and clamping here is the deterministic equivalent.
            let distance = |channel: usize| at(channel, row, column).max(0.0) * unit;

            let face = decode_box(cx, cy, CH_FACE_BOX, &distance, letterbox);
            let person = if person_score >= config.min_person_score {
                Some(decode_box(cx, cy, CH_PERSON_BOX, &distance, letterbox))
            } else {
                None
            };

            let mut landmarks = [[0.0f32; 2]; LANDMARK_COUNT];
            for (index, slot) in landmarks.iter_mut().enumerate() {
                let ox = at(CH_LANDMARKS + index * 2, row, column) * unit;
                let oy = at(CH_LANDMARKS + index * 2 + 1, row, column) * unit;
                *slot = letterbox.to_frame([cx + ox, cy + oy]);
            }

            out.push(Detection {
                face,
                person,
                face_score,
                person_score,
                landmarks,
                stride,
                found_by,
            });
        }
    }
    out
}

/// One box, from four distances at an anchor centre.
fn decode_box(
    cx: f32,
    cy: f32,
    first_channel: usize,
    distance: &dyn Fn(usize) -> f32,
    letterbox: &Letterbox,
) -> NormBox {
    let left = cx - distance(first_channel);
    let top = cy - distance(first_channel + 1);
    let right = cx + distance(first_channel + 2);
    let bottom = cy + distance(first_channel + 3);
    let [x0, y0] = letterbox.to_frame([left, top]);
    let [x1, y1] = letterbox.to_frame([right, bottom]);
    NormBox::from_corners(x0, y0, x1, y1)
}

/// Greedy non-maximum suppression over detections from any number of passes.
///
/// Deterministic in two places that a naive implementation is not.
///
/// The **sort** is by score descending and then by the box's own geometry, so two
/// anchors that produced the same score - which happens constantly with a
/// quantised head - are always ordered the same way. Rust's sort is stable, but
/// the input order across a tiled pass depends on which tile is decoded first, so
/// stability alone is not enough.
///
/// The **keep test** compares against every kept box rather than only the last, so
/// the outcome does not depend on the order suppressions happened to fire in.
#[must_use]
pub fn nms(mut detections: Vec<Detection>, config: &DetectConfig) -> Vec<Detection> {
    detections.retain(|detection| {
        detection.face_score >= config.min_face_score
            && detection.face.w > 0.0
            && detection.face.h > 0.0
            && detection.landmark_spread() >= config.min_landmark_spread
    });

    detections.sort_by(|a, b| {
        b.face_score
            .partial_cmp(&a.face_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| order_key(a).cmp(&order_key(b)))
    });

    let mut kept: Vec<Detection> = Vec::with_capacity(detections.len().min(config.max_faces));
    for candidate in detections {
        if kept.len() >= config.max_faces {
            break;
        }
        let overlaps = kept
            .iter()
            .any(|keep| keep.face.iou(&candidate.face) > config.nms_iou);
        if !overlaps {
            kept.push(candidate);
        }
    }
    kept
}

/// A total order over a detection's geometry, for tie-breaking.
///
/// Quantised to a ten-thousandth of the frame and turned into integers, because
/// `f32` has no total order and a comparator that returns `Equal` for two
/// different boxes reintroduces the non-determinism this exists to remove.
fn order_key(detection: &Detection) -> (i32, i32, i32, i32, u16) {
    let quantise = |value: f32| (value * 10_000.0).round() as i32;
    (
        quantise(detection.face.y),
        quantise(detection.face.x),
        quantise(detection.face.h),
        quantise(detection.face.w),
        detection.stride as u16,
    )
}

/// Everything one frame's detection pass produced.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameDetections {
    /// Faces, strongest first, after suppression across every pass.
    pub faces: Vec<Detection>,
    /// Body boxes, including the ones with no face.
    pub persons: Vec<NormBox>,
    /// True when the tiled pass ran.
    pub tiled: bool,
    /// Why the tiling decision went the way it did.
    pub tile_reasons: Vec<String>,
    /// Forward passes this frame cost.
    pub passes: usize,
}

/// The detector, with its preprocessing and its decode attached.
#[derive(Debug, Clone)]
pub struct FaceDetector {
    infer: Arc<dyn InferService>,
    model: ModelRef,
    config: DetectConfig,
}

impl FaceDetector {
    /// Wrap the one way to run a model.
    #[must_use]
    pub fn new(infer: Arc<dyn InferService>) -> Self {
        Self {
            infer,
            model: ModelRef::new(DETECT_MODEL, DETECT_MODEL_VERSION),
            config: DetectConfig::default(),
        }
    }

    /// Override the detection policy. Used by QA's threshold sweeps.
    #[must_use]
    pub const fn with_config(mut self, config: DetectConfig) -> Self {
        self.config = config;
        self
    }

    /// The policy in force.
    #[must_use]
    pub const fn config(&self) -> &DetectConfig {
        &self.config
    }

    /// The model this detector is pinned to.
    #[must_use]
    pub const fn model(&self) -> ModelRef {
        self.model
    }

    /// Which execution provider the hardware plan selected.
    #[must_use]
    pub fn ep(&self) -> &'static str {
        self.infer.plan().selected_ep().as_str()
    }

    /// Load the detector before a photographer is waiting on it.
    ///
    /// # Errors
    ///
    /// Whatever the loader raised.
    pub fn warmup(&self) -> AuraResult<()> {
        self.infer.warmup(&[self.model])
    }

    /// Find every face in one frame, tiling when the frame earns it.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5007` when the head tensors disagree with the declared shape,
    /// `AURA-ML-5011` on cancellation, and whatever the loader raised on first use.
    pub fn detect(
        &self,
        rgb: &[u8],
        width: usize,
        height: usize,
        hint: &FrameHint,
        prio: Priority,
    ) -> AuraResult<FrameDetections> {
        let (tensor, letterbox) = preprocess(rgb, width, height, (0, 0, width, height));
        let heads = self.run_one(&tensor, prio)?;
        let mut faces = self.decode_all(&heads, &letterbox, FoundBy::Full);
        let persons = collect_persons(&faces, &self.config);

        let decision = should_tile(
            &nms(faces.clone(), &self.config),
            &FrameHint {
                focal_len_mm: hint.focal_len_mm,
                person_boxes: persons.len(),
            },
            width,
            height,
            &self.config,
        );

        let mut passes = 1usize;
        if decision.tile {
            let regions = tiles(width, height);
            if !regions.is_empty() {
                let mut tensors = Vec::with_capacity(regions.len());
                let mut boxes = Vec::with_capacity(regions.len());
                for region in regions {
                    let (tile_tensor, tile_box) = preprocess(rgb, width, height, region);
                    tensors.push(tile_tensor);
                    boxes.push(tile_box);
                }
                // One `run_batch` rather than four `run` calls: the scheduler in
                // `aura-infer` sizes batches against a memory ledger and halves
                // them rather than failing, and four separate calls would waste
                // all of that on the frame that needs it most.
                let results = self.run_batch(&tensors, prio)?;
                passes += results.len();
                for (result, tile_box) in results.iter().zip(boxes.iter()) {
                    faces.extend(self.decode_all(result, tile_box, FoundBy::Tile));
                }
            }
        }

        let faces = nms(faces, &self.config);
        let persons = collect_persons(&faces, &self.config);

        // Section 11: `face.detect` {images, faces, small_faces, ms, tiled_pass_used}.
        // The per-frame half of it; the batch runner sums these.
        tracing::debug!(
            target: "face.detect",
            faces = faces.len(),
            small_faces = faces
                .iter()
                .filter(|face| face.face.h < self.config.small_face_fraction)
                .count(),
            tiled_pass_used = decision.tile,
            passes,
            "frame scanned"
        );

        Ok(FrameDetections {
            faces,
            persons,
            tiled: decision.tile,
            tile_reasons: decision.reasons,
            passes,
        })
    }

    /// Decode all three strides of one pass.
    fn decode_all(
        &self,
        heads: &[Vec<f32>],
        letterbox: &Letterbox,
        found_by: FoundBy,
    ) -> Vec<Detection> {
        let mut out = Vec::new();
        for (index, stride) in FACE_DETECT_STRIDES.iter().enumerate() {
            let Some(head) = heads.get(index) else {
                continue;
            };
            out.extend(decode_stride(
                head,
                *stride,
                letterbox,
                &self.config,
                found_by,
            ));
        }
        out
    }

    /// One forward pass, returning the three head tensors.
    fn run_one(&self, tensor: &[f32], prio: Priority) -> AuraResult<Vec<Vec<f32>>> {
        let batch = self.run_batch(std::slice::from_ref(&tensor.to_vec()), prio)?;
        batch.into_iter().next().ok_or_else(|| {
            aura_core::errors::ml::shape_mismatch("three head tensors", "no outputs")
        })
    }

    /// A batch of forward passes, each returning its three head tensors.
    fn run_batch(&self, tensors: &[Vec<f32>], prio: Priority) -> AuraResult<Vec<Vec<Vec<f32>>>> {
        if tensors.is_empty() {
            return Ok(Vec::new());
        }
        let mut inputs = Vec::with_capacity(tensors.len());
        for tensor in tensors {
            inputs.push(vec![TensorView::new(
                vec![1, 3, INPUT_SIDE, INPUT_SIDE],
                tensor,
            )?]);
        }
        let results = self.infer.run_batch(self.model, inputs, prio)?;

        let mut out = Vec::with_capacity(results.len());
        for result in results {
            if result.outputs.len() < FACE_DETECT_STRIDES.len() {
                return Err(aura_core::errors::ml::shape_mismatch(
                    &format!("{} head tensors", FACE_DETECT_STRIDES.len()),
                    &format!("{} outputs", result.outputs.len()),
                ));
            }
            out.push(
                result
                    .outputs
                    .into_iter()
                    .map(|tensor| tensor.data)
                    .collect(),
            );
        }
        Ok(out)
    }
}

/// Body boxes from the detections that carried one, suppressed against each other.
///
/// Separate suppression from the faces, with a looser threshold: bodies in a
/// wedding group photograph overlap enormously - a row of guests is a row of
/// occluded torsos - and a face-tight 0.35 would delete most of them.
fn collect_persons(detections: &[Detection], config: &DetectConfig) -> Vec<NormBox> {
    let mut candidates: Vec<(f32, NormBox)> = detections
        .iter()
        .filter_map(|detection| {
            detection
                .person
                .filter(|_| detection.person_score >= config.min_person_score)
                .map(|person| (detection.person_score, person))
        })
        .collect();

    candidates.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                let key = |value: &NormBox| {
                    (
                        (value.y * 10_000.0).round() as i32,
                        (value.x * 10_000.0).round() as i32,
                    )
                };
                key(&a.1).cmp(&key(&b.1))
            })
    });

    let mut kept: Vec<NormBox> = Vec::new();
    for (_, candidate) in candidates {
        if kept.len() >= config.max_faces {
            break;
        }
        if !kept.iter().any(|keep| keep.iou(&candidate) > 0.65) {
            kept.push(candidate);
        }
    }
    kept
}
