//! The interaction head: what the people in a frame are doing with one another.
//!
//! ## The person prior is a plane, not a list of coordinates
//!
//! Section 6.2: "the interaction head operates on the full frame with person boxes as
//! spatial priors". There are two ways to give a convolutional network that prior -
//! append the box coordinates to the flattened feature vector, or paint them into a
//! fourth input channel - and only the second one is a prior in any useful sense.
//!
//! Coordinates appended after the global pool arrive *after* every spatial decision has
//! been made. A plane arrives at the first convolution, so the network can learn "two
//! prior blobs overlapping, low in the frame" as one feature, which is what a hug looks
//! like. It costs a third of the stem's arithmetic and it is the difference between a
//! prior and a footnote.
//!
//! A frame with no face pass gets an all-zero plane. That is a real input meaning
//! "nobody was located", not a missing one - and the interaction reading from such a
//! frame is stored with a lowered confidence rather than withheld, because the three
//! colour channels still show two people embracing.
//!
//! ## Multi-label, and the floor
//!
//! Nine independent sigmoids. A frame can be a hug *and* tears being wiped, and
//! [`DETECTION_FLOOR`] is what keeps the eight that are not happening out of the stored
//! row - which is most of what keeps this phase inside section 11's 900-byte budget.

use std::sync::Arc;

use aura_core::contract::emotion::Interaction;
use aura_core::contract::people::FaceRef;
use aura_core::{AuraResult, Priority};
use aura_infer::contract::infer::{InferService, ModelRef, TensorView, Version};
use aura_infer::onnx::fixtures::{
    INTERACTION_CHANNELS, INTERACTION_CLASSES, INTERACTION_HEAD_MODEL, INTERACTION_SIDE,
};

/// The head this build is pinned to.
const INTERACTION_MODEL_VERSION: Version = Version::new(1, 0, 0);

/// Below this strength an interaction is not stored.
///
/// Section 5's field is a list of what is happening, not a nine-slot vector of mostly
/// nothing. At 0.35 a typical frame stores zero or one entry; every version of this
/// number below 0.20 stored four, which is 60 bytes of JSON per photograph spent
/// recording that nobody was toasting.
///
/// It is a *storage and reporting* floor rather than a decision threshold. Nothing is
/// flagged, culled or promoted at 0.35; the ranker reads the strongest entry and weights
/// it by the scene, so a weak interaction contributes weakly rather than not at all.
pub const DETECTION_FLOOR: f32 = 0.35;

/// The most interactions one frame stores.
///
/// Three. A frame in which the head claims four different things is a frame the head is
/// uncertain about, and the fourth entry has never been the one a photographer cared
/// about. Also a storage bound: three entries is about 45 bytes of JSON.
pub const MAX_STORED: usize = 3;

/// How much the confidence is cut when no person prior was available.
///
/// A frame scored with an all-zero prior plane has been read on colour alone. The head
/// still works - two people embracing look like two people embracing - but the spatial
/// evidence section 6.2 asks for is absent, and a caller reading a `kiss` at 0.8 deserves
/// to know it was decided without knowing where anybody was.
pub const NO_PRIOR_CONFIDENCE_SCALE: f32 = 0.70;

/// The interaction head.
#[derive(Debug, Clone)]
pub struct InteractionHead {
    infer: Arc<dyn InferService>,
    model: ModelRef,
}

impl InteractionHead {
    /// Wrap the one way to run a model.
    #[must_use]
    pub fn new(infer: Arc<dyn InferService>) -> Self {
        Self {
            infer,
            model: ModelRef::new(INTERACTION_HEAD_MODEL, INTERACTION_MODEL_VERSION),
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

    /// Read one frame.
    ///
    /// Not batched, unlike the expression head, and the asymmetry is deliberate: there is
    /// one frame per frame and thirty faces per group photograph, so batching buys
    /// nothing here and would hold a second 160x160x4 tensor - 400 KB - to save a call.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5007` when the tensor shape disagrees with the manifest,
    /// `AURA-ML-5011` when the work was cancelled, and whatever the loader raised.
    pub fn run(&self, frame: &[f32], prio: Priority) -> AuraResult<[f32; INTERACTION_CLASSES]> {
        let input = vec![TensorView::new(
            vec![1, INTERACTION_CHANNELS, INTERACTION_SIDE, INTERACTION_SIDE],
            frame,
        )?];
        let results = self.infer.run_batch(self.model, vec![input], prio)?;
        let Some(tensor) = results.first().and_then(|result| result.outputs.first()) else {
            return Err(aura_core::errors::ml::shape_mismatch(
                "one interaction tensor",
                "no outputs",
            ));
        };
        Ok(decode(&tensor.data))
    }
}

/// Turn the head's nine sigmoid outputs into an array, in `Interaction::ALL` order.
///
/// A short tensor reads as all zeroes: nothing detected, which is the reading that
/// claims least and the one a manifest disagreement should produce.
#[must_use]
pub fn decode(values: &[f32]) -> [f32; INTERACTION_CLASSES] {
    let mut out = [0.0f32; INTERACTION_CLASSES];
    if values.len() < INTERACTION_CLASSES {
        return out;
    }
    for (slot, value) in out.iter_mut().zip(values.iter()) {
        *slot = value.clamp(0.0, 1.0);
    }
    out
}

/// The interactions worth storing, strongest first.
///
/// Deterministic on ties: equal strengths fall back to `Interaction::ALL` order, which is
/// the head's output order, so two runs of the same wedding produce byte-identical rows.
/// Invariant 4.
#[must_use]
pub fn detected(strengths: &[f32; INTERACTION_CLASSES]) -> Vec<(Interaction, f32)> {
    let mut out: Vec<(Interaction, f32)> = strengths
        .iter()
        .enumerate()
        .filter(|(_, strength)| **strength >= DETECTION_FLOOR)
        .filter_map(|(index, strength)| {
            Interaction::from_index(index).map(|kind| (kind, *strength))
        })
        .collect();
    out.sort_by(|left, right| {
        right
            .1
            .total_cmp(&left.1)
            .then_with(|| left.0.as_index().cmp(&right.0.as_index()))
    });
    out.truncate(MAX_STORED);
    out
}

/// Build the head's four-plane input from a decoded proxy and phase 06's face boxes.
///
/// Three channels of the frame resampled to 160 px by area averaging, and a fourth plane
/// painted from the face boxes grown into rough person boxes.
///
/// **Area averaging rather than the bilinear sample the face crops use.** The direction of
/// the resample is the whole reason: a 2048 px proxy going to 160 px is a 13x
/// *downscale*, and a bilinear sample of a 13x downscale reads one pixel in every 169 and
/// aliases everything else. The face crops go the other way - a 90 px face upscaled to
/// 112 - which is why they use bilinear and this does not.
#[must_use]
pub fn frame_tensor(rgb: &[u8], width: usize, height: usize, faces: &[FaceRef]) -> Vec<f32> {
    let side = INTERACTION_SIDE;
    let mut out = vec![0.0f32; INTERACTION_CHANNELS * side * side];
    if width == 0 || height == 0 {
        return out;
    }

    // The three colour planes, by area average over the source block each output pixel
    // covers.
    for row in 0..side {
        let y0 = row * height / side;
        let y1 = (((row + 1) * height) / side).max(y0 + 1).min(height);
        for column in 0..side {
            let x0 = column * width / side;
            let x1 = (((column + 1) * width) / side).max(x0 + 1).min(width);
            let mut totals = [0.0f32; 3];
            let mut count = 0.0f32;
            for y in y0..y1 {
                for x in x0..x1 {
                    let base = (y * width + x) * 3;
                    for (channel, total) in totals.iter_mut().enumerate() {
                        *total += f32::from(rgb.get(base + channel).copied().unwrap_or(0));
                    }
                    count += 1.0;
                }
            }
            if count <= 0.0 {
                continue;
            }
            for (channel, total) in totals.into_iter().enumerate() {
                let index = channel * side * side + row * side + column;
                if let Some(slot) = out.get_mut(index) {
                    *slot = (total / count / 255.0).clamp(0.0, 1.0);
                }
            }
        }
    }

    paint_prior(&mut out, faces);
    out
}

/// How far below a face box the person prior extends, as a multiple of the box height.
///
/// Phase 06 detects faces and bodies from one anchor but `FaceRef` only carries the face
/// box, so the prior is a face box grown downwards into a torso. Four heights is roughly
/// head to waist on a standing adult, which is the part of a person an interaction
/// involves: hands, arms and shoulders.
pub const TORSO_HEIGHTS: f32 = 4.0;

/// How far either side of a face box the person prior extends, as a multiple of the box
/// width.
///
/// Shoulders are wider than a head. One box width either side is about right and it errs
/// wide on purpose: a prior that is slightly too large blurs the boundary between two
/// people, and a prior that is too narrow says two people who are touching are not.
pub const SHOULDER_WIDTHS: f32 = 1.0;

/// Paint the fourth plane from the face boxes.
///
/// Written into an already-built tensor rather than returned, so the 400 KB buffer is
/// allocated once per frame rather than twice.
fn paint_prior(tensor: &mut [f32], faces: &[FaceRef]) {
    let side = INTERACTION_SIDE;
    let base = 3 * side * side;
    for face in faces {
        let box_ = face.bbox;
        if box_.w <= f32::EPSILON || box_.h <= f32::EPSILON {
            continue;
        }
        let x0 = ((box_.x - box_.w * SHOULDER_WIDTHS) * side as f32).floor();
        let x1 = ((box_.x + box_.w * (1.0 + SHOULDER_WIDTHS)) * side as f32).ceil();
        let y0 = (box_.y * side as f32).floor();
        let y1 = ((box_.y + box_.h * TORSO_HEIGHTS) * side as f32).ceil();
        let x0 = x0.max(0.0) as usize;
        let y0 = y0.max(0.0) as usize;
        let x1 = (x1.max(0.0) as usize).min(side);
        let y1 = (y1.max(0.0) as usize).min(side);
        for row in y0..y1 {
            for column in x0..x1 {
                if let Some(slot) = tensor.get_mut(base + row * side + column) {
                    *slot = 1.0;
                }
            }
        }
    }
}

/// How sure the head's reading is, given whether a prior was available.
///
/// The strongest detected strength, scaled down when the prior plane was empty. Not a
/// mean over nine channels: eight of them being near zero is the head being *right*, and
/// averaging that in would report every clean frame as an uncertain one.
#[must_use]
pub fn confidence(strengths: &[f32; INTERACTION_CLASSES], had_prior: bool) -> f32 {
    let strongest = strengths.iter().copied().fold(0.0f32, f32::max);
    let scale = if had_prior {
        1.0
    } else {
        NO_PRIOR_CONFIDENCE_SCALE
    };
    (strongest * scale).clamp(0.0, 1.0)
}
