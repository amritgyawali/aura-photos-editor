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
#![allow(clippy::module_name_repetitions, clippy::cast_precision_loss)]

//! The preview cache: content-addressed, budgeted, self-healing.
//!
//! A photographer opens the same wedding twenty times. Re-decoding 4,000 RAWs on
//! each of those openings would cost an hour of their week, so caching is not an
//! optimisation in this product - it is the feature. What makes it safe to lean
//! on is that nothing here is authoritative: every entry is derivable from the
//! original file, so the cache can be deleted, corrupted, filled or moved and
//! the worst outcome is that some work happens twice.
//!
//! Keys are `content_hash` plus `pipeline_ver`, so:
//!
//! * the same frame copied onto two cards shares one entry;
//! * renaming or moving a file keeps its previews;
//! * bumping the rendering pipeline invalidates every proxy at once, cleanly,
//!   without a migration or a manual delete.

pub mod budget;
pub mod lru;
pub mod paths;
pub mod store;

pub use budget::{CacheBudget, CacheStats, DEFAULT_BUDGET_BYTES, MIN_BUDGET_BYTES};
pub use paths::{decode_linear, encode_linear, Artefact, CacheKey, LinearHeader};
pub use store::CacheStore;
