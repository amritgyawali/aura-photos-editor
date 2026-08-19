#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::disallowed_methods,
    clippy::disallowed_types,
    missing_debug_implementations,
    unreachable_pub,
    rust_2018_idioms
)]
// The panic family is banned in library code and is how a test asserts. An inline
// `#[cfg(test)]` module is not compiled into the library at all, so nothing it does can
// reach a photographer; the lints stay denied everywhere else in the crate. PHASE-18.
#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::unwrap_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::float_cmp,
        clippy::disallowed_methods,
        clippy::uninlined_format_args
    )
)]
#![warn(clippy::pedantic)]
#![allow(
    clippy::module_name_repetitions,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

//! One pass over the pixels, five things computed from them.
//!
//! Phase 05's whole argument is that a wedding is read once. Opening a 2048 px
//! proxy costs milliseconds and 12 MB of resident memory; doing it five times
//! because five stages each wanted one number from it is what turns an
//! eight-minute pipeline into an hour. So the embedding, the difference hash, the
//! colour histogram, the luminance statistics and the edge summary are all
//! produced from the same decoded buffer, in [`embed::run_one`], and then never
//! recomputed.
//!
//! The crate is deliberately small and owns nothing durable:
//!
//! * [`embed::model`] holds the preprocessing - centre crop, box resize, channel
//!   order, normalisation - and the one call into `InferService`;
//! * [`embed::descriptors`] holds the cheap descriptors;
//! * [`embed::hash`] holds the difference hash;
//! * [`embed::batch`] walks a project, sized by the hardware plan, resumable at
//!   any point, and writes through `aura_index::EmbeddingStore`.
//!
//! The vectors themselves belong to `aura-index`, which owns the contract, the
//! storage and the queries. This crate produces them and forgets them.
//!
//! **PHASE-06 adds a second pass with the same discipline.** [`face`] finds faces,
//! aligns them, measures them, embeds them and finds bodies - all from one decoded
//! buffer, in [`face::FacePipeline::analyse`], with two batched calls into the
//! runtime rather than two per face. It runs on the 2048 px proxy rather than the
//! 384 px thumbnail, and the reason is written down at [`face::FACE_LEVEL`]: a
//! guest's face is eleven pixels tall on a thumbnail.
//!
//! The face pass owns no storage. Templates are biometric data, and everything
//! durable about them - the envelope, the key, the project scoping, the erasure -
//! lives in `aura-people`. This crate has no catalog dependency, so it *cannot*
//! write one; the separation is structural rather than a rule people remember.
//!
//! **Preprocessing is a version, not a parameter.** Section 6.1 asks for a fused
//! export so preprocessing cannot drift between training and inference. The
//! interpreter in `aura-infer` has no resize operator, so the fusion is achieved
//! the other way round: the steps are constants of this crate, every stored row
//! carries [`embed::model::PREPROCESS_VER`], and changing any of them is a version
//! bump that triggers a re-embed. See
//! `docs/adr/ADR-0011-embeddings-and-similarity-index.md` section 4.

/// Frozen contracts. Changing anything in here requires an ADR and a re-lock.
pub mod contract {
    pub mod mask;
}

pub mod embed;
pub mod face;
pub mod mask;

pub use embed::batch::{EmbedProgress, EmbedReport, EmbeddingRunner};
pub use embed::model::{EmbeddingModel, EMBED_INPUT_SIDE, MODEL_VER, PREPROCESS_VER};
pub use embed::{run_one, Analysed};
pub use face::detect::{DetectConfig, Detection, FaceDetector, FrameHint, NormBox};
pub use face::{FaceObservation, FacePipeline, FramePeople, FACE_LEVEL};

pub use contract::mask::{
    EdgeQuality, GpuMask, Mask, MaskKind, MaskOp, MaskOutline, MaskPayload, MaskReason,
    MaskService, Storage, ALL_KINDS,
};
// Aliased on the way out: `embed::model::MODEL_VER` is the embedding's and this is the mask
// set's, and two constants called `MODEL_VER` in one crate root is how a store ends up
// stamping a mask row with the embedder's version.
pub use mask::{
    MaskPipeline, MaskSet, ANALYSIS_VER as MASK_ANALYSIS_VER, MASK_LEVEL,
    MODEL_VER as MASK_MODEL_VER,
};
