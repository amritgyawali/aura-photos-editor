//! One decoded buffer in, everything phase 05 promises out.
//!
//! The module is arranged the way the phase document names it: [`model`] for the
//! network and its preprocessing, [`descriptors`] for the cheap summaries,
//! [`hash`] for the difference hash, [`batch`] for the project walk. What ties
//! them together is [`run_one`], and its shape is the whole argument of the
//! phase: the pixels are decoded once, and five things are computed from them
//! before the buffer is dropped.

pub mod batch;
pub mod descriptors;
pub mod hash;
pub mod model;

use aura_core::{AuraResult, PhotoId, Priority};
use aura_index::contract::index::ImageEmbedding;
use aura_index::store::StoredEmbedding;
use aura_raw::contract::pixels::{PixelBuffer, PixelLevel, PixelSource};

use crate::embed::model::{EmbeddingModel, MODEL_VER, PREPROCESS_VER};

/// Everything one photograph contributes, before it is written anywhere.
#[derive(Debug, Clone, PartialEq)]
pub struct Analysed {
    /// The row that goes to the store.
    pub stored: StoredEmbedding,
    /// Milliseconds spent inside the runtime for this frame.
    pub infer_ms: f32,
}

impl Analysed {
    /// The vector and descriptors the index consumes.
    #[must_use]
    pub fn embedding(&self) -> &ImageEmbedding {
        &self.stored.embedding
    }
}

/// Embed one photograph and describe it, from one decoded buffer.
///
/// The buffer must be 8-bit sRGB - the tier 1 thumbnail or the tier 2 proxy.
/// A linear or tiled buffer is refused rather than converted: converting here
/// would be a second preprocessing path, and a second preprocessing path is the
/// drift [`model::PREPROCESS_VER`] exists to prevent.
///
/// # Errors
///
/// * `AURA-ML-5007` when the buffer is not 8-bit sRGB or its dimensions disagree
///   with its payload.
/// * `AURA-ML-5013` when the model returns a vector that is all zeros or not
///   finite. The photograph stays in the catalog; it is simply not a candidate
///   for similarity until it is re-embedded.
/// * Whatever `InferService` raised.
pub fn run_one(
    engine: &EmbeddingModel,
    id: PhotoId,
    buffer: &PixelBuffer,
    level: PixelLevel,
    prio: Priority,
) -> AuraResult<Analysed> {
    let Some(rgb) = buffer.as_srgb8() else {
        return Err(aura_core::errors::ml::shape_mismatch(
            "an 8-bit sRGB buffer",
            buffer.data.as_str(),
        ));
    };
    let width = buffer.width as usize;
    let height = buffer.height as usize;
    if width == 0 || height == 0 || rgb.len() < width * height * 3 {
        return Err(aura_core::errors::ml::shape_mismatch(
            &format!("{width}x{height}x3 samples"),
            &format!("{} samples", rgb.len()),
        ));
    }

    // One pass over the pixels, four descriptors, then the model.
    let plane = descriptors::luma_plane(rgb, width, height);
    let described = descriptors::describe(rgb, width, height, &plane);
    let dhash = hash::dhash(&plane.values, width, height);

    let tensor = EmbeddingModel::preprocess(rgb, width, height);
    let vector = engine.run_one(&tensor, prio)?;

    finish(id, level, buffer.source, &vector, dhash, described, 0.0)
}

/// Assemble a stored row, refusing a vector that says nothing.
///
/// # Errors
///
/// `AURA-ML-5013` when the vector is all zeros or contains a value that is not
/// finite. A model that fails and returns zeros would otherwise sit at cosine
/// distance 1.0 from everything and look like a legitimately unusual photograph -
/// which is precisely the silent failure invariant 9 forbids.
pub fn finish(
    id: PhotoId,
    level: PixelLevel,
    source: PixelSource,
    vector: &[f32],
    dhash: u64,
    described: aura_index::store::Descriptors,
    infer_ms: f32,
) -> AuraResult<Analysed> {
    if vector.iter().any(|value| !value.is_finite()) {
        return Err(aura_index::errors::embed_degenerate(
            &id.to_db(),
            "the model returned a value that is not a number",
        ));
    }
    if vector.iter().all(|value| value.abs() < f32::EPSILON) {
        return Err(aura_index::errors::embed_degenerate(
            &id.to_db(),
            "the model returned an all-zero vector",
        ));
    }

    let mut embedding = ImageEmbedding::zeroed(id, MODEL_VER);
    embedding.vec = model::quantise(vector);
    embedding.dhash = dhash;
    embedding.hsv_hist = described.hsv_hist;
    embedding.luma = described.luma;

    Ok(Analysed {
        stored: StoredEmbedding {
            embedding,
            descriptors: described,
            preprocess_ver: PREPROCESS_VER,
            // The tier says which rung of the pyramid was read; the source says
            // whether those pixels were the camera's JPEG or AURA's render. Both,
            // because invariant "pixels carry their provenance" is about the pair:
            // a tier 1 buffer can be either, and a score computed from one is not
            // the same measurement as a score computed from the other.
            pixel_tier: level.tier().clamp(1, 3) as u8,
            pixel_source: source.as_str(),
        },
        infer_ms,
    })
}
