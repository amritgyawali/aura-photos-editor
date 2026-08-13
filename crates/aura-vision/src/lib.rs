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
//! **Preprocessing is a version, not a parameter.** Section 6.1 asks for a fused
//! export so preprocessing cannot drift between training and inference. The
//! interpreter in `aura-infer` has no resize operator, so the fusion is achieved
//! the other way round: the steps are constants of this crate, every stored row
//! carries [`embed::model::PREPROCESS_VER`], and changing any of them is a version
//! bump that triggers a re-embed. See
//! `docs/adr/ADR-0011-embeddings-and-similarity-index.md` section 4.

pub mod embed;

pub use embed::batch::{EmbedProgress, EmbedReport, EmbeddingRunner};
pub use embed::model::{EmbeddingModel, EMBED_INPUT_SIDE, MODEL_VER, PREPROCESS_VER};
pub use embed::{run_one, Analysed};
