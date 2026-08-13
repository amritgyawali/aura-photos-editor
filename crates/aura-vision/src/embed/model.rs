//! The model, and the preprocessing that is part of it.
//!
//! Section 6.1 asks for the backbone, the head and the preprocessing to be
//! exported as one fused graph, "so preprocessing can never drift between
//! training and inference". That is the right requirement and this build cannot
//! satisfy it literally: the deterministic interpreter in `aura-infer` has no
//! `Resize` operator, so a fused graph would not load (ADR-0007, ADR-0011
//! section 4).
//!
//! It is satisfied a different way. Every step between a decoded proxy and the
//! model's input tensor is a constant of this module - not a parameter, not a
//! setting, not an argument - and [`PREPROCESS_VER`] is written to every stored
//! row beside the model version. A change to the crop, the resampler, the channel
//! order or the normalisation is a version bump, and a version bump triggers the
//! same background re-embed a new model does. When a real fused export exists,
//! these steps move into the graph and the version bumps once more.
//!
//! **What the shipped model is.** `wedding_embedding` 1.0.0 is a convolutional
//! placeholder, not the ViT-B/16 plus contrastive head section 6.1 describes.
//! There is no labelled wedding data in this repository and no GPU backend to
//! train one against, so shipping a plausible-looking transformer would mean
//! shipping numbers that describe nothing. Everything around the model - the
//! preprocessing, the batching, the storage, the index, the queries, the eval
//! harness - is real. The model is C10 in the phase 05 exit report.

use std::sync::Arc;

use aura_core::{AuraResult, Priority};
use aura_index::contract::index::{EMBED_DIM, F16};
use aura_infer::contract::infer::{InferRequest, InferService, ModelRef, TensorView, Version};

use crate::embed::descriptors::{box_resize, LumaPlane};

/// Registry name of the embedding model.
pub const EMBED_MODEL: &str = "wedding_embedding";

/// Pinned version of that model.
pub const EMBED_MODEL_VERSION: Version = Version::new(1, 0, 0);

/// The model version stored on every row.
///
/// A single integer rather than the semantic triple, because it is compared, not
/// read: a row is either the current version or it is not. Derived from the
/// pinned version so the two cannot drift - `1.0.0` is 100.
pub const MODEL_VER: u16 =
    EMBED_MODEL_VERSION.major * 100 + EMBED_MODEL_VERSION.minor * 10 + EMBED_MODEL_VERSION.patch;

/// The preprocessing generation. Bumped by any change to the steps in
/// [`EmbeddingModel::preprocess`].
pub const PREPROCESS_VER: u16 = 1;

/// Side of the square tensor the model takes. Section 2.1: 384 px.
pub const EMBED_INPUT_SIDE: usize = 384;

/// The pinned model, as a value a caller can pass around.
#[must_use]
pub fn model_ref() -> ModelRef {
    ModelRef::new(EMBED_MODEL, EMBED_MODEL_VERSION)
}

/// The embedding model, with its preprocessing attached.
#[derive(Debug, Clone)]
pub struct EmbeddingModel {
    infer: Arc<dyn InferService>,
    model: ModelRef,
}

impl EmbeddingModel {
    /// Wrap the one way to run a model.
    #[must_use]
    pub fn new(infer: Arc<dyn InferService>) -> Self {
        Self {
            infer,
            model: model_ref(),
        }
    }

    /// The model this runner is pinned to.
    #[must_use]
    pub const fn model(&self) -> ModelRef {
        self.model
    }

    /// Which execution provider the hardware plan selected.
    ///
    /// Section 11's `embed.batch` event carries it, because "the same wedding took
    /// four times as long on this laptop" is a question whose answer is usually
    /// which provider answered.
    #[must_use]
    pub fn ep(&self) -> &'static str {
        self.infer.plan().selected_ep().as_str()
    }

    /// Load the model before a photographer is waiting on it.
    ///
    /// # Errors
    ///
    /// Whatever the loader raised. A warmup failure is a real failure: it is how
    /// the product learns a model pack is broken before a wedding depends on it.
    pub fn warmup(&self) -> AuraResult<()> {
        self.infer.warmup(&[self.model])
    }

    /// Turn a decoded 8-bit sRGB buffer into the model's input tensor.
    ///
    /// Three steps, all of them constants of this crate:
    ///
    /// 1. **Centre crop to a square.** A wedding mixes portrait and landscape
    ///    frames of the same moment; squashing them to a square would make the
    ///    two look different to the model for a reason that has nothing to do
    ///    with what is in them. Cropping loses the long edges, which is the
    ///    lesser harm and is what every open image encoder does.
    /// 2. **Box resize to 384.** An area average, because a proxy going to
    ///    384 px is a downscale of five or more and a bilinear downscale aliases;
    ///    an aliased input makes the vector depend on where the subject fell
    ///    relative to the sampling grid.
    /// 3. **NCHW, `0..1`, sRGB.** The layout, range and colour the manifest
    ///    declares. No mean or standard deviation is subtracted: the manifest
    ///    says `range: 0..1`, and a normalisation the manifest does not describe
    ///    is exactly the drift this design exists to prevent.
    #[must_use]
    pub fn preprocess(rgb: &[u8], width: usize, height: usize) -> Vec<f32> {
        let side = width.min(height);
        let left = (width - side) / 2;
        let top = (height - side) / 2;

        // Split the interleaved buffer into three planes over the cropped square,
        // then resample each one. Planar first because the resize is per channel
        // and the model wants NCHW anyway.
        let mut out = Vec::with_capacity(3 * EMBED_INPUT_SIDE * EMBED_INPUT_SIDE);
        for channel in 0..3 {
            let mut plane = Vec::with_capacity(side * side);
            for row in 0..side {
                for column in 0..side {
                    let index = ((top + row) * width + (left + column)) * 3 + channel;
                    let value = rgb.get(index).copied().unwrap_or(0);
                    plane.push(f32::from(value) / 255.0);
                }
            }
            out.extend(box_resize(
                &plane,
                side,
                side,
                EMBED_INPUT_SIDE,
                EMBED_INPUT_SIDE,
            ));
        }
        out
    }

    /// The same crop and resize, on a luminance plane.
    ///
    /// Used by nothing in the pipeline and by the parity test, which needs to
    /// prove that the crop the model sees is the crop the descriptors describe.
    #[must_use]
    pub fn preprocess_luma(plane: &LumaPlane) -> Vec<f32> {
        let side = plane.width.min(plane.height);
        let left = (plane.width - side) / 2;
        let top = (plane.height - side) / 2;
        let mut cropped = Vec::with_capacity(side * side);
        for row in 0..side {
            for column in 0..side {
                cropped.push(
                    plane
                        .values
                        .get((top + row) * plane.width + (left + column))
                        .copied()
                        .unwrap_or(0.0),
                );
            }
        }
        box_resize(&cropped, side, side, EMBED_INPUT_SIDE, EMBED_INPUT_SIDE)
    }

    /// Run one batch of preprocessed tensors and return normalised vectors.
    ///
    /// `batch` is `count` tensors, each `3 * 384 * 384` samples, concatenated.
    /// One call rather than `count` calls: the scheduler in `aura-infer` sizes
    /// batches against a memory ledger and halves them rather than failing, and
    /// handing it one image at a time would waste all of that.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5007` when the tensor shape disagrees with the manifest,
    /// `AURA-ML-5011` when the work was cancelled, and whatever the loader raised
    /// the first time a model is used.
    pub fn run_batch(&self, batch: &[Vec<f32>], prio: Priority) -> AuraResult<Vec<Vec<f32>>> {
        if batch.is_empty() {
            return Ok(Vec::new());
        }
        let mut inputs = Vec::with_capacity(batch.len());
        for tensor in batch {
            inputs.push(vec![TensorView::new(
                vec![1, 3, EMBED_INPUT_SIDE, EMBED_INPUT_SIDE],
                tensor,
            )?]);
        }
        let results = self.infer.run_batch(self.model, inputs, prio)?;

        let mut out = Vec::with_capacity(results.len());
        for result in results {
            let Some(tensor) = result.outputs.first() else {
                return Err(aura_core::errors::ml::shape_mismatch(
                    "one output tensor",
                    "no outputs",
                ));
            };
            let mut vector: Vec<f32> = tensor.data.iter().take(EMBED_DIM).copied().collect();
            vector.resize(EMBED_DIM, 0.0);
            aura_index::query::normalise(&mut vector);
            out.push(vector);
        }
        Ok(out)
    }

    /// Run one image. A convenience over [`EmbeddingModel::run_batch`] for the
    /// interactive path, where there is exactly one frame and somebody waiting.
    ///
    /// # Errors
    ///
    /// As [`EmbeddingModel::run_batch`].
    pub fn run_one(&self, tensor: &[f32], prio: Priority) -> AuraResult<Vec<f32>> {
        let input = vec![TensorView::new(
            vec![1, 3, EMBED_INPUT_SIDE, EMBED_INPUT_SIDE],
            tensor,
        )?];
        let result = self.infer.run(InferRequest::new(self.model, input, prio))?;
        let Some(output) = result.outputs.first() else {
            return Err(aura_core::errors::ml::shape_mismatch(
                "one output tensor",
                "no outputs",
            ));
        };
        let mut vector: Vec<f32> = output.data.iter().take(EMBED_DIM).copied().collect();
        vector.resize(EMBED_DIM, 0.0);
        aura_index::query::normalise(&mut vector);
        Ok(vector)
    }
}

/// Quantise a normalised vector into the stored half-precision array.
///
/// The rounding is the contract's, so a vector that has been through the runtime
/// and one that has been through the store agree bit for bit.
#[must_use]
pub fn quantise(vector: &[f32]) -> [F16; EMBED_DIM] {
    let mut out = [F16::ZERO; EMBED_DIM];
    for (slot, value) in out.iter_mut().zip(vector) {
        *slot = F16::from_f32(*value);
    }
    out
}
