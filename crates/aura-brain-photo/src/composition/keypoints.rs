//! Where the bodies are, joint by joint.
//!
//! PHASE-11 section 8's third step: "integrate a body-keypoint model and implement the
//! crop audit". This module is the first half - the model, its input, its decoder and the
//! geometry that turns seventeen points into the questions [`super::crop_audit`] asks.
//!
//! ## The person box is derived, and that is a recorded gap
//!
//! Section 3 draws the data flow as `proxy + person keypoints + faces + scene`, which
//! reads as though a person box arrives from somewhere. It does not.
//! `aura_core::contract::people::ImageSubjects` carries **faces and nothing else** -
//! phase 06 detects bodies internally and its frozen contract does not expose them - so
//! adding a body box to that contract would be a phase 06 contract amendment, an ADR and
//! a re-lock.
//!
//! Rather than do that, [`person_box`] derives a person box from a face box using fixed
//! proportions. It is honest about what that costs:
//!
//! * a seated person, a person at a strong angle and a child all get a box that is too
//!   tall, so their lower joints land below where the body actually is;
//! * a person facing away has no face and therefore no box at all, and is invisible to
//!   the whole crop audit.
//!
//! Both are recorded as condition C3 in the phase 11 exit report, and both fail in the
//! safe direction: a wrong box produces a *missing* violation far more often than an
//! invented one, because the audit only reports a cut when a joint is inside the derived
//! box and near a frame edge.
//!
//! ## Seven joints out of seventeen
//!
//! The head emits the COCO seventeen because that is what every replaceable model emits.
//! `aura_core::contract::composition::Joint` names seven, because those are the seven a
//! photographer complains about. [`Pose::joint_at`] is the mapping, and it takes the
//! *mean* of a left and right point rather than either one: a frame that cuts one wrist
//! and keeps the other is one cut and not two, and the evidence box the panel shows
//! should contain the hand that left.
//!
//! ## What the placeholder head does and does not do
//!
//! The shipped `pose_keypoints` weights are untrained, so the seventeen points it returns
//! over a real photograph are a random projection of that photograph. Every gate that
//! involves them is measured with [`ReferencePose`] against fixtures whose joints are
//! known by construction, which proves the decoder, the mapping, the audit and the
//! severity arithmetic - and says nothing about the weights. Condition C1 of the phase 11
//! exit report.

use std::sync::Arc;

use aura_core::contract::composition::{Box2, Joint};
use aura_core::{AuraResult, Priority};
use aura_infer::contract::infer::{InferService, ModelRef, TensorView, Version};
use aura_infer::onnx::fixtures::{POSE_CROP_SIDE, POSE_JOINTS, POSE_MODEL, POSE_OUTPUTS};
use aura_vision::embed::descriptors::box_resize;

/// Registry name of the keypoint head.
pub const KEYPOINT_MODEL: &str = POSE_MODEL;

/// Pinned version of the keypoint head.
pub const KEYPOINT_MODEL_VERSION: Version = Version::new(1, 0, 0);

/// Registry name of the aesthetic head, pinned beside the keypoint head.
pub const AESTHETIC_MODEL: &str = aura_infer::onnx::fixtures::AESTHETIC_MODEL;

/// Pinned version of the aesthetic head.
pub const AESTHETIC_MODEL_VERSION: Version = Version::new(1, 0, 0);

/// Whether the pinned pose weights have passed the labeled crop-audit gate.
///
/// The registered model is currently a deterministic graph with random weights. Treating
/// those coordinates as observations would invent crop violations, so product analysis
/// withholds the keypoint-dependent claims until exact pinned weights pass the gate.
pub const KEYPOINT_HEAD_TRAINED: bool = false;

/// The version stamped on every `image_composition.model_ver`.
///
/// The two heads ship as one pack and move together, so one number invalidates both.
/// Phase 09's `focus::MODEL_VER` makes the same choice for the same reason: no consumer
/// of a composition judgement cares *which* head moved, only that the numbers are not
/// comparable across the move.
pub const MODEL_VER: u16 = KEYPOINT_MODEL_VERSION.major * 100
    + KEYPOINT_MODEL_VERSION.minor * 10
    + KEYPOINT_MODEL_VERSION.patch;

/// Below this a keypoint is not trusted to be anywhere.
///
/// A joint under this score is treated as absent rather than as located: the crop audit
/// reports nothing about it, which is the direction that under-reports violations instead
/// of inventing them.
pub const POINT_PRESENT_AT: f32 = 0.35;

/// How many head heights make a standing body.
///
/// Seven and a half, the classical figure-drawing canon, and it is a *canon* rather than a
/// measurement - real adults run between seven and eight. It is used only to derive a
/// person box from a face box, where being a quarter of a head out changes which side of
/// a frame edge an ankle falls on and nothing else.
pub const HEADS_PER_BODY: f32 = 7.5;

/// How much wider than the face box a body is.
///
/// Shoulders are about three times the width of a face at the same distance. Generous
/// rather than tight: a box that is too wide includes background and a box that is too
/// narrow excludes an arm, and an excluded arm is a missed cut.
pub const SHOULDERS_PER_FACE: f32 = 3.0;

/// The seventeen COCO joints, in the head's output order.
///
/// The order **is** the model's output order and renumbering it relabels a trained model.
/// It is also the order every replaceable open pose model uses, which is why it is copied
/// exactly rather than reduced to the seven this phase names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Joint17 {
    /// Nose.
    Nose,
    /// Left eye.
    LeftEye,
    /// Right eye.
    RightEye,
    /// Left ear.
    LeftEar,
    /// Right ear.
    RightEar,
    /// Left shoulder.
    LeftShoulder,
    /// Right shoulder.
    RightShoulder,
    /// Left elbow.
    LeftElbow,
    /// Right elbow.
    RightElbow,
    /// Left wrist.
    LeftWrist,
    /// Right wrist.
    RightWrist,
    /// Left hip.
    LeftHip,
    /// Right hip.
    RightHip,
    /// Left knee.
    LeftKnee,
    /// Right knee.
    RightKnee,
    /// Left ankle.
    LeftAnkle,
    /// Right ankle.
    RightAnkle,
}

impl Joint17 {
    /// Every joint, in the head's output order.
    pub const ALL: [Self; POSE_JOINTS] = [
        Self::Nose,
        Self::LeftEye,
        Self::RightEye,
        Self::LeftEar,
        Self::RightEar,
        Self::LeftShoulder,
        Self::RightShoulder,
        Self::LeftElbow,
        Self::RightElbow,
        Self::LeftWrist,
        Self::RightWrist,
        Self::LeftHip,
        Self::RightHip,
        Self::LeftKnee,
        Self::RightKnee,
        Self::LeftAnkle,
        Self::RightAnkle,
    ];

    /// The slot this joint occupies in the head's output.
    #[must_use]
    pub fn as_index(self) -> usize {
        Self::ALL
            .iter()
            .position(|candidate| *candidate == self)
            .unwrap_or(0)
    }

    /// The stable slug, for logs and for the eval fixtures.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Nose => "nose",
            Self::LeftEye => "left_eye",
            Self::RightEye => "right_eye",
            Self::LeftEar => "left_ear",
            Self::RightEar => "right_ear",
            Self::LeftShoulder => "left_shoulder",
            Self::RightShoulder => "right_shoulder",
            Self::LeftElbow => "left_elbow",
            Self::RightElbow => "right_elbow",
            Self::LeftWrist => "left_wrist",
            Self::RightWrist => "right_wrist",
            Self::LeftHip => "left_hip",
            Self::RightHip => "right_hip",
            Self::LeftKnee => "left_knee",
            Self::RightKnee => "right_knee",
            Self::LeftAnkle => "left_ankle",
            Self::RightAnkle => "right_ankle",
        }
    }

    /// The left/right pair that makes up one of the seven named joints.
    ///
    /// The neck has no COCO point of its own, so it is the mean of the two shoulders
    /// raised towards the nose - which is where a photographer's "cut at the neck" is.
    #[must_use]
    pub const fn pair_for(joint: Joint) -> (Self, Self) {
        match joint {
            Joint::Neck | Joint::Shoulder => (Self::LeftShoulder, Self::RightShoulder),
            Joint::Elbow => (Self::LeftElbow, Self::RightElbow),
            Joint::Wrist => (Self::LeftWrist, Self::RightWrist),
            Joint::Hip => (Self::LeftHip, Self::RightHip),
            Joint::Knee => (Self::LeftKnee, Self::RightKnee),
            Joint::Ankle => (Self::LeftAnkle, Self::RightAnkle),
        }
    }
}

/// One located point, in frame-normalised coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    /// Horizontal position, `0..1` of frame width. May fall outside when the joint is
    /// off-frame, which is exactly the case the crop audit exists to notice.
    pub x: f32,
    /// Vertical position, `0..1` of frame height.
    pub y: f32,
    /// How sure the head is that this joint is where it says, `0..1`.
    pub score: f32,
}

impl Point {
    /// True when this point is trusted enough to reason about.
    #[must_use]
    pub fn is_present(&self) -> bool {
        self.score >= POINT_PRESENT_AT
    }

    /// True when this point is outside the frame.
    #[must_use]
    pub fn is_off_frame(&self) -> bool {
        !(0.0..=1.0).contains(&self.x) || !(0.0..=1.0).contains(&self.y)
    }
}

/// One person's seventeen points, plus the box they were read from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    /// The derived person box the crop was taken from.
    pub person: Box2,
    /// The face box the person box was derived from.
    pub face: Box2,
    /// Seventeen points in [`Joint17::ALL`] order, in frame coordinates.
    pub points: [Point; POSE_JOINTS],
}

impl Pose {
    /// One of the seven named joints, as the mean of its left/right pair.
    ///
    /// `None` when neither side is present. The mean rather than either side, for the
    /// reason this module's header gives: one cut wrist is one cut, and the box shown
    /// should contain the hand that left the frame.
    #[must_use]
    pub fn joint_at(&self, joint: Joint) -> Option<Point> {
        let (left, right) = Joint17::pair_for(joint);
        let left = self
            .points
            .get(left.as_index())
            .copied()
            .unwrap_or_default();
        let right = self
            .points
            .get(right.as_index())
            .copied()
            .unwrap_or_default();
        let mut point = match (left.is_present(), right.is_present()) {
            (true, true) => Point {
                x: f32::midpoint(left.x, right.x),
                y: f32::midpoint(left.y, right.y),
                score: f32::midpoint(left.score, right.score),
            },
            (true, false) => left,
            (false, true) => right,
            (false, false) => return None,
        };
        // The neck is not a COCO point. It sits a fifth of the way from the shoulder line
        // towards the nose, which is close enough for "did the frame cut here" and is not
        // claimed to be closer than that.
        if matches!(joint, Joint::Neck) {
            if let Some(nose) = self.points.first().copied().filter(Point::is_present) {
                point.y += (nose.y - point.y) * 0.20;
            }
        }
        Some(point)
    }

    /// How many of the seventeen points are present.
    #[must_use]
    pub fn located(&self) -> usize {
        self.points.iter().filter(|p| p.is_present()).count()
    }

    /// The top of the head, when it can be told.
    ///
    /// The nose raised by the distance from the nose to the ear line, which approximates
    /// the crown without a crown keypoint. `None` when the nose is absent, and the caller
    /// falls back to the face box - which is what [`super::placement`] does.
    #[must_use]
    pub fn crown(&self) -> Option<f32> {
        let nose = self.points.first().copied().filter(Point::is_present)?;
        let ears: Vec<f32> = [Joint17::LeftEar, Joint17::RightEar]
            .into_iter()
            .filter_map(|joint| self.points.get(joint.as_index()).copied())
            .filter(Point::is_present)
            .map(|point| point.y)
            .collect();
        if ears.is_empty() {
            return Some(nose.y - self.face.h * 0.55);
        }
        let ear_y = ears.iter().sum::<f32>() / ears.len() as f32;
        Some(nose.y - (ear_y - nose.y).abs().max(self.face.h * 0.30) * 1.8)
    }
}

/// Derive a person box from a face box.
///
/// Seven and a half head heights tall, three face widths wide, anchored on the face. The
/// box is **not** clamped to the frame: the whole point of the crop audit is to notice
/// joints that fall outside, and a clamped box would move every one of them to the edge
/// and call it a cut.
///
/// The crop taken for the model *is* clamped, in [`preprocess_person`], because a model
/// cannot read pixels that are not there.
#[must_use]
pub fn person_box(face: Box2) -> Box2 {
    let head_height = face.h.max(f32::EPSILON);
    let width = face.w * SHOULDERS_PER_FACE;
    let height = head_height * HEADS_PER_BODY;
    let centre_x = face.x + face.w / 2.0;
    Box2 {
        x: centre_x - width / 2.0,
        // A quarter of a head above the face box, because a face box starts at the brow
        // rather than at the crown.
        y: face.y - head_height * 0.25,
        w: width,
        h: height,
    }
}

/// Turn one person box into the head's input tensor.
///
/// Three channels at [`POSE_CROP_SIDE`], box-resampled - the same resampler phase 05 uses
/// for every downscale in the product, for the reason `focus::preprocess_region` gives.
/// Colour rather than luminance, because the thing that separates an arm from the wall
/// behind it in a dark reception is usually its colour and not its brightness.
///
/// Pixels outside the frame are filled with zero rather than by edge replication. A
/// replicated edge would show the model a body continuing past the frame, which is the
/// exact fact the crop audit is trying to establish.
#[must_use]
pub fn preprocess_person(rgb: &[u8], width: usize, height: usize, person: Box2) -> Vec<f32> {
    let x0 = (person.x * width as f32).floor() as i64;
    let y0 = (person.y * height as f32).floor() as i64;
    let x1 = ((person.x + person.w) * width as f32).ceil() as i64;
    let y1 = ((person.y + person.h) * height as f32).ceil() as i64;
    let crop_w = (x1 - x0).max(1) as usize;
    let crop_h = (y1 - y0).max(1) as usize;

    let mut planes = Vec::with_capacity(3 * crop_w * crop_h);
    for channel in 0..3usize {
        for row in 0..crop_h {
            for column in 0..crop_w {
                let sx = x0 + i64::try_from(column).unwrap_or(i64::MAX);
                let sy = y0 + i64::try_from(row).unwrap_or(i64::MAX);
                let inside = sx >= 0 && sy >= 0 && (sx as usize) < width && (sy as usize) < height;
                let value = if inside {
                    rgb.get((sy as usize * width + sx as usize) * 3 + channel)
                        .copied()
                        .map_or(0.0, |byte| f32::from(byte) / 255.0)
                } else {
                    0.0
                };
                planes.push(value);
            }
        }
    }

    let mut out = Vec::with_capacity(3 * POSE_CROP_SIDE * POSE_CROP_SIDE);
    for channel in 0..3usize {
        let start = channel * crop_w * crop_h;
        let end = start + crop_w * crop_h;
        let Some(plane) = planes.get(start..end) else {
            out.extend(std::iter::repeat_n(0.0, POSE_CROP_SIDE * POSE_CROP_SIDE));
            continue;
        };
        out.extend(box_resize(
            plane,
            crop_w,
            crop_h,
            POSE_CROP_SIDE,
            POSE_CROP_SIDE,
        ));
    }
    out
}

/// Turn one head output into a pose in frame coordinates.
///
/// The head emits `0..1` inside the person crop; this maps each point back through the
/// crop's placement in the frame. A point at `x = 0.0` in the crop is at the crop's left
/// edge, which may itself be outside the frame - so a decoded `x` below zero is a real
/// answer meaning "this joint is off the left of the photograph".
#[must_use]
pub fn decode(raw: &[f32], person: Box2, face: Box2) -> Pose {
    let mut points = [Point::default(); POSE_JOINTS];
    for (index, point) in points.iter_mut().enumerate() {
        let base = index * 3;
        let x = raw.get(base).copied().unwrap_or(0.0);
        let y = raw.get(base + 1).copied().unwrap_or(0.0);
        let score = raw.get(base + 2).copied().unwrap_or(0.0);
        *point = Point {
            x: person.x + x * person.w,
            y: person.y + y * person.h,
            score,
        };
    }
    Pose {
        person,
        face,
        points,
    }
}

/// The keypoint head.
#[derive(Debug, Clone)]
pub struct PoseHead {
    infer: Arc<dyn InferService>,
    model: ModelRef,
}

impl PoseHead {
    /// Wrap the one way to run a model.
    #[must_use]
    pub fn new(infer: Arc<dyn InferService>) -> Self {
        Self {
            infer,
            model: ModelRef::new(KEYPOINT_MODEL, KEYPOINT_MODEL_VERSION),
        }
    }

    /// The model this runner is pinned to.
    #[must_use]
    pub const fn model(&self) -> ModelRef {
        self.model
    }

    /// True only when the pinned coordinates are approved for product judgements.
    #[must_use]
    pub const fn is_trained(&self) -> bool {
        KEYPOINT_HEAD_TRAINED
    }

    /// Which execution provider the hardware plan selected.
    #[must_use]
    pub fn ep(&self) -> &'static str {
        self.infer.plan().selected_ep().as_str()
    }

    /// Load the head before a photographer is waiting on it.
    ///
    /// # Errors
    ///
    /// Whatever the loader raised.
    pub fn warmup(&self) -> AuraResult<()> {
        self.infer.warmup(&[self.model])
    }

    /// Locate a batch of people.
    ///
    /// One call for every person in one frame, never one call per person. Section 9 gives
    /// PERF "share keypoint inference with Phase 06", and this is the shape that makes
    /// that possible: a group photograph with thirty faces is thirty crops in one batch,
    /// which is what phase 03's memory ledger and batch scheduler were built for.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5007` when the tensor shape disagrees with the manifest, `AURA-ML-5011`
    /// when the work was cancelled, and whatever the loader raised.
    pub fn run_batch(&self, crops: &[Vec<f32>], prio: Priority) -> AuraResult<Vec<Vec<f32>>> {
        if crops.is_empty() {
            return Ok(Vec::new());
        }
        let mut inputs = Vec::with_capacity(crops.len());
        for crop in crops {
            inputs.push(vec![TensorView::new(
                vec![1, 3, POSE_CROP_SIDE, POSE_CROP_SIDE],
                crop,
            )?]);
        }
        let results = self.infer.run_batch(self.model, inputs, prio)?;

        let mut out = Vec::with_capacity(results.len());
        for result in results {
            let Some(tensor) = result.outputs.first() else {
                return Err(aura_core::errors::ml::shape_mismatch(
                    "one keypoint tensor",
                    "no outputs",
                ));
            };
            if tensor.data.len() < POSE_OUTPUTS {
                return Err(aura_core::errors::ml::shape_mismatch(
                    &format!("{POSE_OUTPUTS} keypoint values"),
                    &format!("{} values", tensor.data.len()),
                ));
            }
            out.push(tensor.data.clone());
        }
        Ok(out)
    }
}

/// Read a pose out of a fixture instead of out of the head.
///
/// **This is the only way to state a limb-crop F1 at all** while the shipped head is a
/// placeholder, and it is the same device phase 06 used for detection recall and phase 09
/// for blink accuracy. What it measures is everything around the head - the person box,
/// the decoder, the joint mapping, the edge tests, the severity arithmetic and the scene
/// weighting - against an answer that is known by construction. What it does not measure
/// is the weights, and the phase 11 exit report says so where the number is reported.
///
/// The convention is deliberately crude and deliberately visible: the fixture paints a
/// small bright marker at each joint it wants located, and this reads the markers back.
/// A marker convention that could be produced by an ordinary photograph would make the
/// gates meaningless, so the marker is a value no real proxy pixel carries - full white
/// in one channel and zero in the other two.
#[derive(Debug, Clone, Copy)]
pub struct ReferencePose;

impl ReferencePose {
    /// The red value a joint marker carries.
    pub const MARKER_R: u8 = 255;
    /// The green and blue values a joint marker carries.
    pub const MARKER_GB: u8 = 0;
    /// How near two marker pixels have to be to belong to the same joint.
    pub const MARKER_RADIUS: f32 = 0.02;

    /// Read the markers inside one person box, in [`Joint17::ALL`] order.
    ///
    /// Markers are matched to joints by their vertical order down the body and then by
    /// their horizontal order, which is enough for the fixtures the eval harness builds
    /// and is documented as exactly that: a fixture reader, not a detector.
    #[must_use]
    pub fn read(rgb: &[u8], width: usize, height: usize, person: Box2, face: Box2) -> Pose {
        let mut found: Vec<(f32, f32)> = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let base = (y * width + x) * 3;
                let red = rgb.get(base).copied().unwrap_or(0);
                let green = rgb.get(base + 1).copied().unwrap_or(0);
                let blue = rgb.get(base + 2).copied().unwrap_or(0);
                if red == Self::MARKER_R && green == Self::MARKER_GB && blue == Self::MARKER_GB {
                    found.push((x as f32 / width as f32, y as f32 / height as f32));
                }
            }
        }
        // Cluster adjacent marker pixels: a marker is drawn as a small disc, and one disc
        // is one joint rather than nine. Single-link against the running centroid, which
        // is enough because the fixtures place markers far further apart than
        // `MARKER_RADIUS`.
        let mut clusters: Vec<(f32, f32, u32)> = Vec::new();
        for (x, y) in found {
            let hit = clusters.iter_mut().find(|(sum_x, sum_y, count)| {
                let centre_x = sum_x / *count as f32;
                let centre_y = sum_y / *count as f32;
                (x - centre_x).hypot(y - centre_y) < Self::MARKER_RADIUS
            });
            if let Some(slot) = hit {
                slot.0 += x;
                slot.1 += y;
                slot.2 += 1;
            } else {
                clusters.push((x, y, 1));
            }
        }
        let mut centres: Vec<(f32, f32)> = clusters
            .into_iter()
            .map(|(sum_x, sum_y, count)| (sum_x / count as f32, sum_y / count as f32))
            .collect();
        centres.sort_by(|left, right| left.1.total_cmp(&right.1).then(left.0.total_cmp(&right.0)));

        let mut points = [Point::default(); POSE_JOINTS];
        for (slot, centre) in points.iter_mut().zip(centres) {
            *slot = Point {
                x: centre.0,
                y: centre.1,
                score: 1.0,
            };
        }
        Pose {
            person,
            face,
            points,
        }
    }
}
