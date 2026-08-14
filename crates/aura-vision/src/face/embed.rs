//! The recognition template: 512 numbers that say who, and nothing else.
//!
//! A face template is not a perceptual embedding. The phase 05 vector in
//! [`crate::embed`] answers "does this photograph look like that one", and it is
//! trained to notice the venue, the light and the dress. This one has to answer
//! "is this the same person", across twelve hours in which the light changed four
//! times, the hair came down, the jacket came off and the make-up was redone -
//! and it has to answer *no* for two sisters who look alike. The two vectors are
//! deliberately different models, stored in different tables, sealed differently,
//! and never compared with each other.
//!
//! Three properties are load-bearing.
//!
//! **The template is L2 normalised, so distance is angle.** Everything downstream -
//! the clustering threshold, the mutual-nearest-neighbour test, the sub-centroids -
//! is expressed in cosine distance, and cosine distance is only a metric on the
//! unit sphere. Normalisation happens here, once, in the same place the rounding
//! happens, so a template that has been through the runtime and one that has come
//! back out of the vault agree bit for bit.
//!
//! **The rounding is `aura_index`'s.** [`aura_index::contract::index::F16`] is
//! already the workspace's half-precision rule - round to nearest, ties to even -
//! and phase 05 froze it. Reusing it rather than writing a second rounding means
//! there is exactly one answer in the product to "what does 0.031 25 become in
//! sixteen bits", which is the sort of thing two implementations disagree about
//! once and then produce two identity graphs.
//!
//! **No clothing, no context, no frame.** Section 6.2 forbids clothing features in
//! the identity signal, and the way that is enforced is architectural rather than
//! by discipline: this module only ever sees the 112 px aligned crop produced by
//! [`crate::face::align::warp_crop`]. It has no access to the frame, so it cannot
//! accidentally learn that the person in the blue lehenga at 6 pm is the person in
//! the blue lehenga at 7 pm.
//!
//! ## What the shipped model is
//!
//! `face_embed` 1.0.0 is a placeholder with an `ArcFace`-shaped graph and no `ArcFace`
//! training, so its templates carry no identity information. Every mechanism around
//! it is real. Condition C1 in `docs/progress/PHASE-06-EXIT.md`.

use std::sync::Arc;

use aura_core::{AuraResult, Priority};
use aura_index::contract::index::F16;
use aura_infer::contract::infer::{InferService, ModelRef, TensorView, Version};
use aura_infer::onnx::fixtures::{FACE_CROP_SIDE, FACE_EMBED_DIM, FACE_EMBED_MODEL};

/// Registry name of the recogniser.
pub const EMBED_MODEL: &str = FACE_EMBED_MODEL;

/// Pinned version of the recogniser.
pub const EMBED_MODEL_VERSION: Version = Version::new(1, 0, 0);

/// The version stamped on every `faces.embed_ver`.
pub const EMBED_VER: u16 =
    EMBED_MODEL_VERSION.major * 100 + EMBED_MODEL_VERSION.minor * 10 + EMBED_MODEL_VERSION.patch;

/// Width of a recognition template. Section 2.1: 512-d, `ArcFace` class.
pub const TEMPLATE_DIM: usize = FACE_EMBED_DIM;

/// Bytes one sealed template occupies before the envelope header is added.
pub const TEMPLATE_BYTES: usize = TEMPLATE_DIM * 2;

/// Turn one aligned 112x112 sRGB crop into the model's input tensor.
///
/// Three steps and no parameters, the same discipline as
/// [`crate::embed::model::EmbeddingModel::preprocess`]: NCHW, `0..1`, sRGB, with no
/// mean or standard deviation subtracted because the manifest declares `range:
/// 0..1` and a normalisation the manifest does not describe is drift waiting to
/// happen.
#[must_use]
pub fn preprocess_crop(crop: &[u8]) -> Vec<f32> {
    let side = FACE_CROP_SIDE;
    let mut out = vec![0.0f32; 3 * side * side];
    for channel in 0..3 {
        for row in 0..side {
            for column in 0..side {
                let source = (row * side + column) * 3 + channel;
                let destination = channel * side * side + row * side + column;
                if let Some(slot) = out.get_mut(destination) {
                    *slot = f32::from(crop.get(source).copied().unwrap_or(0)) / 255.0;
                }
            }
        }
    }
    out
}

/// Quantise a normalised template into the stored half-precision array.
#[must_use]
pub fn quantise(template: &[f32]) -> [F16; TEMPLATE_DIM] {
    let mut out = [F16::ZERO; TEMPLATE_DIM];
    for (slot, value) in out.iter_mut().zip(template) {
        *slot = F16::from_f32(*value);
    }
    out
}

/// 512 halves to 1,024 little-endian bytes, ready for the vault.
#[must_use]
pub fn encode(template: &[F16; TEMPLATE_DIM]) -> Vec<u8> {
    let mut out = Vec::with_capacity(TEMPLATE_BYTES);
    for half in template {
        out.extend_from_slice(&half.to_bits().to_le_bytes());
    }
    out
}

/// 1,024 little-endian bytes back to a single-precision template.
///
/// `None` on any other length. A template of the wrong width is a schema change
/// nobody recorded, and padding or truncating one would produce a vector that
/// compares plausibly against every other vector and means nothing.
#[must_use]
pub fn decode(bytes: &[u8]) -> Option<Vec<f32>> {
    if bytes.len() != TEMPLATE_BYTES {
        return None;
    }
    let mut out = Vec::with_capacity(TEMPLATE_DIM);
    for pair in bytes.chunks_exact(2) {
        let fixed: [u8; 2] = pair.try_into().ok()?;
        out.push(F16::from_bits(u16::from_le_bytes(fixed)).to_f32());
    }
    Some(out)
}

/// Cosine distance between two templates, `0..2`.
///
/// The frozen phase 05 implementation, reused rather than reimplemented: its
/// accumulation order is fixed at eight lanes, which is what lets the clustering
/// tests assert exact equality instead of a tolerance.
#[must_use]
pub fn distance(a: &[f32], b: &[f32]) -> f32 {
    aura_index::contract::index::cosine_distance(a, b)
}

/// True when a template says nothing.
///
/// An all-zero or non-finite template sits at distance 1.0 from everything, which
/// looks exactly like a person nobody else at the wedding resembles - so it would
/// quietly become its own identity and then its own guest. Refused instead.
#[must_use]
pub fn is_degenerate(template: &[f32]) -> bool {
    template.iter().any(|value| !value.is_finite())
        || template.iter().all(|value| value.abs() < f32::EPSILON)
}

/// The recogniser.
#[derive(Debug, Clone)]
pub struct FaceEmbedder {
    infer: Arc<dyn InferService>,
    model: ModelRef,
}

impl FaceEmbedder {
    /// Wrap the one way to run a model.
    #[must_use]
    pub fn new(infer: Arc<dyn InferService>) -> Self {
        Self {
            infer,
            model: ModelRef::new(EMBED_MODEL, EMBED_MODEL_VERSION),
        }
    }

    /// The model this runner is pinned to.
    #[must_use]
    pub const fn model(&self) -> ModelRef {
        self.model
    }

    /// Which execution provider the hardware plan selected.
    #[must_use]
    pub fn ep(&self) -> &'static str {
        self.infer.plan().selected_ep().as_str()
    }

    /// Load the recogniser before a photographer is waiting on it.
    ///
    /// # Errors
    ///
    /// Whatever the loader raised.
    pub fn warmup(&self) -> AuraResult<()> {
        self.infer.warmup(&[self.model])
    }

    /// Embed a batch of preprocessed crops, returning normalised templates.
    ///
    /// One call rather than one per face: a wedding frame has two or three faces
    /// and a group photograph has thirty, and handing those to the runtime one at a
    /// time would throw away the memory ledger and the batch scheduler that phase
    /// 03 built.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5007` when the tensor shape disagrees with the manifest,
    /// `AURA-ML-5011` when the work was cancelled, and whatever the loader raised.
    pub fn run_batch(&self, crops: &[Vec<f32>], prio: Priority) -> AuraResult<Vec<Vec<f32>>> {
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
                    "one template tensor",
                    "no outputs",
                ));
            };
            let mut template: Vec<f32> = tensor.data.iter().take(TEMPLATE_DIM).copied().collect();
            template.resize(TEMPLATE_DIM, 0.0);
            aura_index::query::normalise(&mut template);
            out.push(template);
        }
        Ok(out)
    }
}
