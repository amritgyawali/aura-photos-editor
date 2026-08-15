//! The expression head: eight continuous readings per face.
//!
//! ## Sigmoid, not softmax, and it is the whole design
//!
//! Section 2.1 requires the eight outputs to be "all continuous, not one-hot". A face at
//! a wedding can be laughing and crying at the same time, and the faces that are doing
//! both are disproportionately the ones a client buys. A softmax would make the eight
//! channels compete for one unit of probability and would force the model to pick - which
//! is the modelling mistake that produces the technically-perfect, emotionally-flat
//! gallery section 1 names as "the most common complaint about AI culling today".
//!
//! The activation is in the graph rather than in this decoder, by the rule
//! `aura_infer::onnx::fixtures::face_quality` states: an activation belongs in a graph
//! when it applies to every channel of a tensor, and eight independent sigmoids do.
//!
//! ## What this module deliberately does not do
//!
//! **Decide anything.** It turns a crop into eight numbers and a confidence.
//! [`crate::emotion::score`] is where those numbers meet a scene's weights, and
//! [`crate::emotion::gaze`] measures the ninth field of `FaceExpression` from geometry
//! rather than reading it from a head.

use std::sync::Arc;

use aura_core::contract::emotion::{FaceExpression, GazeTarget};
use aura_core::contract::people::FaceRef;
use aura_core::{AuraResult, Priority};
use aura_infer::contract::infer::{InferService, ModelRef, TensorView, Version};
use aura_infer::onnx::fixtures::{
    EXPRESSION_CHANNELS, EXPRESSION_CROP_SIDE, EXPRESSION_HEAD_MODEL,
};
use aura_vision::face::align;

/// The head this build is pinned to.
const EXPRESSION_MODEL_VERSION: Version = Version::new(1, 0, 0);

/// The integer written into `image_interaction.model_ver` and
/// `face_expression.model_ver`.
///
/// One number for **both** heads, exactly as phase 09 uses one for `focus_head` and
/// `eye_state`. They ship, are validated and are rolled back as one pack; two columns
/// would let a catalog hold a frame whose faces came from one pack and whose
/// interactions came from another, which is a state nothing in the product could
/// interpret.
pub const MODEL_VER: u16 = 100;

/// How sure the head must be before a channel is worth writing a reason about.
///
/// Below this the reading is stored - it still feeds the ranker, weighted by its own
/// confidence - but the product does not put a sentence in front of a photographer about
/// it. The reason threshold and the scoring threshold are deliberately different: a weak
/// signal is legitimate evidence in a weighted sum and is not legitimate as a claim.
pub const REASON_CONFIDENCE: f32 = 0.55;

/// The expression head.
#[derive(Debug, Clone)]
pub struct ExpressionHead {
    infer: Arc<dyn InferService>,
    model: ModelRef,
}

impl ExpressionHead {
    /// Wrap the one way to run a model.
    #[must_use]
    pub fn new(infer: Arc<dyn InferService>) -> Self {
        Self {
            infer,
            model: ModelRef::new(EXPRESSION_HEAD_MODEL, EXPRESSION_MODEL_VERSION),
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

    /// Read a batch of aligned crops.
    ///
    /// One call rather than one per face, for the reason `FaceEmbedder::run_batch` and
    /// `EyeStateModel::run_batch` both give: a group photograph has thirty faces, and
    /// thirty separate calls would throw away phase 03's memory ledger and batch
    /// scheduler.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5007` when the tensor shape disagrees with the manifest,
    /// `AURA-ML-5011` when the work was cancelled, and whatever the loader raised.
    pub fn run_batch(
        &self,
        crops: &[Vec<f32>],
        prio: Priority,
    ) -> AuraResult<Vec<[f32; EXPRESSION_CHANNELS]>> {
        if crops.is_empty() {
            return Ok(Vec::new());
        }
        let mut inputs = Vec::with_capacity(crops.len());
        for crop in crops {
            inputs.push(vec![TensorView::new(
                vec![1, 3, EXPRESSION_CROP_SIDE, EXPRESSION_CROP_SIDE],
                crop,
            )?]);
        }
        let results = self.infer.run_batch(self.model, inputs, prio)?;

        let mut out = Vec::with_capacity(results.len());
        for result in results {
            let Some(tensor) = result.outputs.first() else {
                return Err(aura_core::errors::ml::shape_mismatch(
                    "one expression tensor",
                    "no outputs",
                ));
            };
            out.push(decode(&tensor.data));
        }
        Ok(out)
    }
}

/// Turn the head's eight sigmoid outputs into an array, in channel order.
///
/// A short tensor reads as all zeroes rather than as a partial answer. That is the
/// direction that claims least: every channel at zero with a confidence of zero is an
/// unread face, which is exactly what a manifest disagreement has produced.
#[must_use]
pub fn decode(values: &[f32]) -> [f32; EXPRESSION_CHANNELS] {
    let mut out = [0.0f32; EXPRESSION_CHANNELS];
    if values.len() < EXPRESSION_CHANNELS {
        return out;
    }
    for (slot, value) in out.iter_mut().zip(values.iter()) {
        *slot = value.clamp(0.0, 1.0);
    }
    out
}

/// How sure the head is about one face's eight readings.
///
/// **Not a maximum and not a mean.** Eight independent sigmoids that all sit near 0.5 are
/// a head saying nothing at all, and eight that are all near 0 or 1 are a head that has
/// committed. So the confidence is the mean *distance from a half*, doubled into `0..1`:
/// a face read as `[0.9, 0.1, 0.05, 0.02, ...]` is confident and a face read as
/// `[0.51, 0.49, 0.5, ...]` is not.
///
/// A maximum would call the second of those confident because of one channel, and a mean
/// of the raw values would call an all-zero reading *un*confident when it is the head
/// being very sure that nothing is happening.
#[must_use]
pub fn confidence(channels: &[f32; EXPRESSION_CHANNELS]) -> f32 {
    let total: f32 = channels
        .iter()
        .map(|value| (value.clamp(0.0, 1.0) - 0.5).abs())
        .sum();
    ((total / EXPRESSION_CHANNELS as f32) * 2.0).clamp(0.0, 1.0)
}

/// Assemble one face's reading from the head's output and phase 06's geometry.
///
/// The gaze arrives separately, from [`crate::emotion::gaze`], because it is measured
/// rather than predicted.
#[must_use]
pub fn assemble(
    face: &FaceRef,
    channels: [f32; EXPRESSION_CHANNELS],
    gaze: GazeTarget,
) -> FaceExpression {
    FaceExpression {
        face_id: face.face_id,
        identity: face.identity_id,
        smile: channels.first().copied().unwrap_or(0.0),
        genuineness: channels.get(1).copied().unwrap_or(0.0),
        laughter: channels.get(2).copied().unwrap_or(0.0),
        tears: channels.get(3).copied().unwrap_or(0.0),
        surprise: channels.get(4).copied().unwrap_or(0.0),
        tenderness: channels.get(5).copied().unwrap_or(0.0),
        composed: channels.get(6).copied().unwrap_or(0.0),
        discomfort: channels.get(7).copied().unwrap_or(0.0),
        gaze,
        confidence: confidence(&channels),
        // The crop the Emotion card shows: the face box with a fifth of its size in
        // context around it. A face box at 100 % zoom with no jaw in it is unreadable,
        // which is the same argument `CropRect::expanded` was written for in phase 09.
        crop: face.bbox.expanded(0.20),
    }
}

/// Warp one face into the crop the head reads.
///
/// The shared two-point warp in `aura_vision::face::align`, which phase 09's eye head
/// reads too. One warp per face per frame, and phases 06, 09 and 10 all take this crop -
/// so a wedding that has been scanned for faces pays for the geometry once.
///
/// Returns `None` for a face with no usable landmarks. Skipping rather than guessing is
/// phase 09's rule and it matters more here: a crop built from collapsed landmarks is a
/// rectangle of somebody's shoulder, and eight confident readings about a shoulder is
/// worse than no reading at all.
#[must_use]
pub fn crop_for(rgb: &[u8], width: usize, height: usize, face: &FaceRef) -> Option<Vec<f32>> {
    if width == 0 || height == 0 || !face.has_eyes() {
        return None;
    }
    let crop = align::warp_crop_from_eyes(
        rgb,
        width,
        height,
        [
            face.eyes[0][0] * width as f32,
            face.eyes[0][1] * height as f32,
        ],
        [
            face.eyes[1][0] * width as f32,
            face.eyes[1][1] * height as f32,
        ],
    );
    Some(align::preprocess_face_crop(&crop))
}
