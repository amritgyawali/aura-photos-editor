//! Whether a face is good enough to be evidence about who somebody is.
//!
//! This module is the reason phase 06's clustering can work at all. A twelve-hour
//! wedding produces thousands of faces that are technically detections and
//! practically useless: a cheek at the edge of a dance-floor frame, a profile
//! behind a champagne flute, a thirty-pixel guest in the back row of a ceremony.
//! Feeding those into an identity clustering pass does not produce a slightly
//! worse answer, it produces a *different kind* of wrong answer - chain merges,
//! where two guests who both photographed badly end up joined to each other and
//! then to a third person, and one identity swallows the room.
//!
//! So section 6.1 asks for a gate, and the gate has two numbers:
//! [`MIN_PX_HEIGHT`] and [`MIN_USABLE`]. A face below either one is **detected,
//! stored and displayed** - it still counts towards a people count, it still
//! anchors a mask later, it is still visible in the People panel - and it does not
//! vote on identity. Excluding is not deleting, and the distinction is in
//! `faces.votes` rather than in whether the row exists.
//!
//! ## Measured evidence, and a model head that is not trusted yet
//!
//! Four of the five factors are measured from the aligned crop with arithmetic
//! that has no weights in it: sharpness from a Laplacian, occlusion from local
//! flatness, exposure from clipping, and frontality from the landmark geometry in
//! [`crate::face::align`]. The fifth is `face_quality`, a trained head.
//!
//! [`QUALITY_MODEL_WEIGHT`] is **0.0** in this build, and that is not a bug. The
//! shipped `face_quality` model is a placeholder with no training behind it, so
//! its four outputs are numbers with no meaning, and folding a meaningless number
//! into a gate that decides which faces vote would be the worst kind of silent
//! error - one that looks like it is working. The head is still loaded, still run,
//! still batched and still reported in [`FaceQuality::model_usable`], because the
//! cost, the memory and the wiring are all real and have to be measured. When a
//! trained head lands, the weight becomes a number and this comment becomes
//! history. See condition C2 in `docs/progress/PHASE-06-EXIT.md`.

use std::sync::Arc;

use aura_core::{AuraResult, Priority};
use aura_infer::contract::infer::{InferService, ModelRef, TensorView, Version};
use aura_infer::onnx::fixtures::{FACE_CROP_SIDE, FACE_QUALITY_MODEL, FACE_QUALITY_OUTPUTS};

use crate::embed::descriptors::luminance;
use crate::face::align::Pose;

/// Registry name of the quality head.
pub const QUALITY_MODEL: &str = FACE_QUALITY_MODEL;

/// Pinned version of the quality head.
pub const QUALITY_MODEL_VERSION: Version = Version::new(1, 0, 0);

/// The version stamped on every `faces.quality_ver`.
pub const QUALITY_VER: u16 = QUALITY_MODEL_VERSION.major * 100
    + QUALITY_MODEL_VERSION.minor * 10
    + QUALITY_MODEL_VERSION.patch;

/// Minimum face height in source pixels for a face to vote. Section 6.1: 48 px.
///
/// Source pixels, not preview pixels. A 48 px face on a 384 px thumbnail is a
/// 256 px face on the sensor, and gating on the preview would throw away most of
/// a wedding's guests.
pub const MIN_PX_HEIGHT: u32 = 48;

/// Minimum fused usability for a face to vote. Section 6.1: 0.4.
pub const MIN_USABLE: f32 = 0.40;

/// The most yaw a voting face may have, in degrees.
///
/// Sixty. This is a **hard cut-off rather than a term in the fused score**, and the
/// distinction is the reason it exists at all.
///
/// A fused score is a trade: a very sharp, perfectly exposed, unoccluded face can carry a
/// poor pose term and still pass. For most factors that is right - a slightly soft face
/// with everything else in its favour really is usable. For yaw past a three-quarter turn
/// it is not, because the thing that has gone missing is *an eye*, and no amount of
/// sharpness on the visible half brings it back. A template from a near-profile is not a
/// worse measurement of the same person, it is a measurement of a different projection,
/// and letting one into a cluster is how two people's faces end up adjacent.
///
/// Sixty rather than forty-five because a wedding is candid: a strict limit would exclude
/// most of the dance floor and half the ceremony, and those frames still have to be
/// grouped. Sixty is roughly where the far eye stops being visible.
pub const MAX_VOTING_YAW: f32 = 60.0;

/// The most of a crop that may be crushed or blown and still vote, `0..1`.
///
/// Sixty per cent. The same argument as [`MAX_VOTING_YAW`]: past this point there is not
/// enough face left in the crop to identify anybody, and no combination of the other three
/// factors changes that. Below it the smooth exposure term does the work.
pub const MAX_VOTING_CLIPPED: f32 = 0.60;

/// How much the trained quality head contributes to the fused score.
///
/// Zero in this build. See this module's header; this is not a tuning constant.
pub const QUALITY_MODEL_WEIGHT: f32 = 0.0;

/// Laplacian variance at which a crop is judged half sharp.
///
/// Measured on the fixture set rather than chosen: the aligned crops of faces a
/// human calls sharp cluster above 0.006 in this units system (luminance in
/// `0..1`, a 4-neighbour Laplacian), and the ones a human calls soft sit below
/// 0.0015. 0.003 is the half-way point, and the saturating curve around it means
/// the gate has no cliff for a face sitting exactly on the boundary.
pub const BLUR_HALF_VARIANCE: f32 = 0.003;

/// Block side, in crop pixels, for the occlusion estimate.
const OCCLUSION_BLOCK: usize = 14;

/// Local standard deviation below which a block counts as featureless.
///
/// A hand, a microphone, a veil or a blown highlight over part of a face all show
/// up the same way: a region of the crop with no structure in it. 0.02 in
/// luminance units is about five 8-bit levels of spread across a 14 px block,
/// which real skin never is - skin has pores, shadow gradients and specular
/// falloff.
const OCCLUSION_FLAT_SIGMA: f32 = 0.02;

/// Luminance at or below which a crop pixel is crushed.
const CLIP_LOW: f32 = 0.02;
/// Luminance at or above which a crop pixel is blown.
const CLIP_HIGH: f32 = 0.98;

/// Fraction of clipped pixels at which exposure is judged half usable.
const CLIP_HALF_FRACTION: f32 = 0.25;

/// The weights of the fused geometric mean, in the order sharpness, occlusion,
/// pose, exposure.
///
/// A weighted **geometric** mean rather than an arithmetic one, and the choice is
/// the whole design: an arithmetic mean lets three good factors outvote one
/// catastrophic factor, so a perfectly exposed, perfectly frontal, perfectly
/// unoccluded, completely out-of-focus face scores 0.75 and votes on an identity.
/// A geometric mean cannot do that - any factor near zero drags the result to
/// zero, which is the behaviour a gate is supposed to have.
const FUSION_WEIGHTS: [f32; 4] = [0.35, 0.25, 0.25, 0.15];

/// What the pixels say about one aligned crop.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Measured {
    /// Sharpness, `0..1`, from the Laplacian variance.
    pub sharpness: f32,
    /// Fraction of the crop with no structure in it, `0..1`.
    pub occlusion: f32,
    /// Fraction of the crop that is crushed or blown, `0..1`.
    pub clipped: f32,
    /// Mean luminance of the crop, `0..1`. Reported rather than gated on: a dark
    /// face in a candle-lit ceremony is the photograph, not a fault.
    pub mean_luma: f32,
}

/// One face's usability, with the reasons it is or is not usable.
///
/// Invariant 2. The gate is an AI decision like any other, and "why is this face
/// not in the bride's group" is the single most common support question a people
/// feature generates.
#[derive(Debug, Clone, PartialEq)]
pub struct FaceQuality {
    /// The fused score, `0..1`.
    pub usable: f32,
    /// Blur, `0..1`, one being completely soft. `1 - sharpness`.
    pub blur: f32,
    /// Occlusion, `0..1`.
    pub occlusion: f32,
    /// How far from frontal, `0..1`.
    pub pose_penalty: f32,
    /// Height of the face in source pixels.
    pub px_height: u32,
    /// What the trained head said, when one ran. Reported, not trusted; see this
    /// module's header.
    pub model_usable: Option<f32>,
    /// True when this face may vote on identity.
    pub votes: bool,
    /// Why. Always at least one entry.
    pub reasons: Vec<String>,
    /// Which gate produced this verdict.
    pub quality_ver: u16,
}

impl FaceQuality {
    /// A face nothing could be measured from: refused, with a reason.
    #[must_use]
    pub fn unmeasurable(px_height: u32, reason: &str) -> Self {
        Self {
            usable: 0.0,
            blur: 1.0,
            occlusion: 1.0,
            pose_penalty: 1.0,
            px_height,
            model_usable: None,
            votes: false,
            reasons: vec![reason.to_string()],
            quality_ver: QUALITY_VER,
        }
    }
}

/// The four numbers that decide whether a face votes.
///
/// Two smooth and two hard. `min_px_height` and `min_usable` are section 6.1's gate;
/// `max_yaw` and `max_clipped` are cut-offs, and the reason a gate needs both shapes is on
/// [`MAX_VOTING_YAW`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QualityGate {
    /// Minimum height in source pixels.
    pub min_px_height: u32,
    /// Minimum fused usability.
    pub min_usable: f32,
    /// Maximum yaw in degrees. A hard cut-off; see [`MAX_VOTING_YAW`].
    pub max_yaw: f32,
    /// Maximum crushed-or-blown fraction. A hard cut-off; see [`MAX_VOTING_CLIPPED`].
    pub max_clipped: f32,
}

impl Default for QualityGate {
    fn default() -> Self {
        Self {
            min_px_height: MIN_PX_HEIGHT,
            min_usable: MIN_USABLE,
            max_yaw: MAX_VOTING_YAW,
            max_clipped: MAX_VOTING_CLIPPED,
        }
    }
}

/// Measure one aligned 112x112 sRGB crop.
///
/// Everything here runs on the crop rather than on the frame, and that is
/// deliberate: the crop is normalised for scale and rotation, so the same face at
/// two distances produces the same measurements. Measuring sharpness on the frame
/// would make every distant face read as soft, which is a statement about the lens
/// and not about the photograph.
#[must_use]
pub fn measure(crop: &[u8]) -> Measured {
    let side = FACE_CROP_SIDE;
    if crop.len() < side * side * 3 {
        return Measured::default();
    }

    let mut luma = Vec::with_capacity(side * side);
    for pixel in crop.chunks_exact(3).take(side * side) {
        let (Some(red), Some(green), Some(blue)) = (pixel.first(), pixel.get(1), pixel.get(2))
        else {
            continue;
        };
        luma.push(luminance(*red, *green, *blue));
    }
    if luma.len() < side * side {
        return Measured::default();
    }

    let at = |row: usize, column: usize| -> f32 {
        luma.get(row * side + column).copied().unwrap_or(0.0)
    };

    // Four-neighbour Laplacian, interior only, so the crop's black padding does
    // not read as an edge. The variance of the response, not its mean: a mean
    // Laplacian is near zero for any image, which is why the textbook sharpness
    // measure is the second moment.
    let mut sum = 0.0f64;
    let mut sum_squares = 0.0f64;
    let mut count = 0u32;
    for row in 1..side - 1 {
        for column in 1..side - 1 {
            let response = 4.0f32.mul_add(
                at(row, column),
                -(at(row - 1, column)
                    + at(row + 1, column)
                    + at(row, column - 1)
                    + at(row, column + 1)),
            );
            sum += f64::from(response);
            sum_squares += f64::from(response) * f64::from(response);
            count += 1;
        }
    }
    let variance = if count == 0 {
        0.0f32
    } else {
        let n = f64::from(count);
        let mean = sum / n;
        ((sum_squares / n) - mean * mean).max(0.0) as f32
    };
    let sharpness = (variance / (variance + BLUR_HALF_VARIANCE)).clamp(0.0, 1.0);

    // Occlusion: the fraction of blocks with no structure. Blocks rather than
    // pixels, because a hand over a mouth is a region and a single flat pixel is
    // noise.
    let mut flat = 0u32;
    let mut blocks = 0u32;
    let mut row_start = 0usize;
    while row_start + OCCLUSION_BLOCK <= side {
        let mut column_start = 0usize;
        while column_start + OCCLUSION_BLOCK <= side {
            let mut block_sum = 0.0f32;
            let mut block_squares = 0.0f32;
            for row in row_start..row_start + OCCLUSION_BLOCK {
                for column in column_start..column_start + OCCLUSION_BLOCK {
                    let value = at(row, column);
                    block_sum += value;
                    block_squares += value * value;
                }
            }
            let n = (OCCLUSION_BLOCK * OCCLUSION_BLOCK) as f32;
            let mean = block_sum / n;
            let sigma = ((block_squares / n) - mean * mean).max(0.0).sqrt();
            if sigma < OCCLUSION_FLAT_SIGMA {
                flat += 1;
            }
            blocks += 1;
            column_start += OCCLUSION_BLOCK;
        }
        row_start += OCCLUSION_BLOCK;
    }
    let occlusion = if blocks == 0 {
        0.0
    } else {
        f64::from(flat) as f32 / f64::from(blocks) as f32
    };

    let mut clipped = 0u32;
    let mut luma_sum = 0.0f64;
    for value in &luma {
        if *value <= CLIP_LOW || *value >= CLIP_HIGH {
            clipped += 1;
        }
        luma_sum += f64::from(*value);
    }
    let total = luma.len() as f32;

    Measured {
        sharpness,
        occlusion: occlusion.clamp(0.0, 1.0),
        clipped: (f64::from(clipped) as f32 / total).clamp(0.0, 1.0),
        mean_luma: (luma_sum / f64::from(total)) as f32,
    }
}

/// Fuse the measurements, the pose and the model head into one verdict.
///
/// # Panics
///
/// Never. The geometric mean is taken over four factors that are all clamped into
/// `0..1` and floored away from zero, so the logarithm is always defined.
#[must_use]
pub fn fuse(
    measured: &Measured,
    pose: Pose,
    px_height: u32,
    model_usable: Option<f32>,
    gate: QualityGate,
) -> FaceQuality {
    let pose_penalty = pose.frontality_penalty();
    let exposure =
        1.0 - (measured.clipped / (measured.clipped + CLIP_HALF_FRACTION)).clamp(0.0, 1.0);

    // Floored at a thousandth rather than at zero: a single catastrophic factor
    // should drive the result to nearly nothing, not to exactly nothing, so that
    // two hopeless faces can still be ordered against each other in a debug panel.
    let floor = 0.001f32;
    let factors = [
        measured.sharpness.max(floor),
        (1.0 - measured.occlusion).max(floor),
        (1.0 - pose_penalty).max(floor),
        exposure.max(floor),
    ];

    let mut weighted_log = 0.0f32;
    let mut weight_total = 0.0f32;
    for (factor, weight) in factors.iter().zip(FUSION_WEIGHTS.iter()) {
        weighted_log += weight * factor.ln();
        weight_total += weight;
    }
    let mut usable = if weight_total <= f32::EPSILON {
        0.0
    } else {
        (weighted_log / weight_total).exp().clamp(0.0, 1.0)
    };

    // The head's contribution, at whatever weight this build trusts it. A linear
    // blend rather than another geometric factor: an untrained head returning 0.03
    // must not be able to zero a face the pixels say is perfect, and a weight of
    // zero must be exactly a no-op.
    if let Some(head) = model_usable {
        if QUALITY_MODEL_WEIGHT > 0.0 {
            usable =
                (1.0 - QUALITY_MODEL_WEIGHT) * usable + QUALITY_MODEL_WEIGHT * head.clamp(0.0, 1.0);
        }
    }

    let big_enough = px_height >= gate.min_px_height;
    let good_enough = usable >= gate.min_usable;
    let frontal_enough = pose.yaw.abs() <= gate.max_yaw;
    let exposed_enough = measured.clipped <= gate.max_clipped;
    let votes = big_enough && good_enough && frontal_enough && exposed_enough;

    let mut reasons = Vec::new();
    if votes {
        reasons.push(format!(
            "{px_height} px tall and {usable:.2} usable; votes on identity"
        ));
    } else {
        if !big_enough {
            reasons.push(format!(
                "{px_height} px tall, below the {} px identity gate",
                gate.min_px_height
            ));
        }
        if !good_enough {
            reasons.push(format!(
                "usability {usable:.2} is below the {:.2} identity gate",
                gate.min_usable
            ));
        }
        if !frontal_enough {
            reasons.push(format!(
                "turned {:.0} degrees away, past the {:.0} degree limit; the far eye is not                  visible and no amount of sharpness replaces it",
                pose.yaw.abs(),
                gate.max_yaw
            ));
        }
        if !exposed_enough {
            reasons.push(format!(
                "{:.0}% of the crop is crushed or blown, past the {:.0}% limit; there is not                  enough face left to identify anybody",
                measured.clipped * 100.0,
                gate.max_clipped * 100.0
            ));
        }
    }
    // The dominant cause, so a panel can say something more useful than "low
    // quality". Ordered by how often each one is the real answer at a wedding.
    if measured.sharpness < 0.35 {
        reasons.push(format!("soft: sharpness {:.2}", measured.sharpness));
    }
    if pose_penalty > 0.45 {
        reasons.push(format!(
            "turned away: yaw {:.0} degrees, pitch {:.0} degrees",
            pose.yaw, pose.pitch
        ));
    }
    if measured.occlusion > 0.25 {
        reasons.push(format!(
            "{:.0}% of the crop has no facial structure in it",
            measured.occlusion * 100.0
        ));
    }
    if measured.clipped > 0.20 {
        reasons.push(format!(
            "{:.0}% of the crop is crushed or blown",
            measured.clipped * 100.0
        ));
    }

    FaceQuality {
        usable,
        blur: (1.0 - measured.sharpness).clamp(0.0, 1.0),
        occlusion: measured.occlusion,
        pose_penalty,
        px_height,
        model_usable,
        votes,
        reasons,
        quality_ver: QUALITY_VER,
    }
}

/// The trained quality head.
#[derive(Debug, Clone)]
pub struct FaceQualityModel {
    infer: Arc<dyn InferService>,
    model: ModelRef,
}

impl FaceQualityModel {
    /// Wrap the one way to run a model.
    #[must_use]
    pub fn new(infer: Arc<dyn InferService>) -> Self {
        Self {
            infer,
            model: ModelRef::new(QUALITY_MODEL, QUALITY_MODEL_VERSION),
        }
    }

    /// The model this runner is pinned to.
    #[must_use]
    pub const fn model(&self) -> ModelRef {
        self.model
    }

    /// Load the head before a photographer is waiting on it.
    ///
    /// # Errors
    ///
    /// Whatever the loader raised.
    pub fn warmup(&self) -> AuraResult<()> {
        self.infer.warmup(&[self.model])
    }

    /// Score a batch of aligned crops.
    ///
    /// Each entry is `[usability, blur, occlusion, pose]`, already squashed by the
    /// sigmoid in the graph.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5007` when the output width disagrees with the manifest, and
    /// whatever the loader raised on first use.
    pub fn run_batch(
        &self,
        crops: &[Vec<f32>],
        prio: Priority,
    ) -> AuraResult<Vec<[f32; FACE_QUALITY_OUTPUTS]>> {
        if crops.is_empty() {
            return Ok(Vec::new());
        }
        let mut inputs = Vec::with_capacity(crops.len());
        for crop in crops {
            inputs.push(vec![TensorView::new(
                vec![1, 3, FACE_CROP_SIDE, FACE_CROP_SIDE],
                crop,
            )?]);
        }
        let results = self.infer.run_batch(self.model, inputs, prio)?;

        let mut out = Vec::with_capacity(results.len());
        for result in results {
            let Some(tensor) = result.outputs.first() else {
                return Err(aura_core::errors::ml::shape_mismatch(
                    "one quality tensor",
                    "no outputs",
                ));
            };
            let mut row = [0.0f32; FACE_QUALITY_OUTPUTS];
            for (slot, value) in row.iter_mut().zip(tensor.data.iter()) {
                *slot = value.clamp(0.0, 1.0);
            }
            out.push(row);
        }
        Ok(out)
    }
}
