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
// The panic family is banned in library code and is how a test asserts. `scripts/check-banned.sh`
// draws the same line by path; this draws it by compilation, which is stricter: an inline
// `#[cfg(test)]` module is not compiled into the library at all, so nothing it does can reach
// a photographer. The lints stay denied everywhere else in the crate.
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
#![allow(clippy::module_name_repetitions)]
// Product names and standards - Lightroom, XMP, Adobe RGB, ProPhoto - read as prose in
// this crate's documentation rather than as identifiers.
#![allow(clippy::doc_markdown)]
// A recipe is a bag of sliders, and converting between the integer form a photographer
// sees and the float form the renderer needs happens on nearly every field. Spelling out
// a `try_from` and an error path for each would bury the arithmetic that matters.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::cast_possible_wrap,
    clippy::similar_names
)]

//! The edit recipe: the one document every AI editing decision writes into.
//!
//! # What is here
//!
//! | Module | What it owns |
//! |---|---|
//! | [`contract::recipe`] | The frozen schema v1 shapes. Changing one needs an ADR. |
//! | [`schema`] | Validation, clamping, and the merge that protects a photographer's edits. |
//! | [`hash`] | Canonical JSON and the recipe hash that makes renders reproducible. |
//! | [`migrate`] | One function per schema step, plus the never-remove-a-field rule. |
//! | [`xmp`] | The Lightroom-compatible subset, out and back. |
//! | [`sidecar`] | The lossless AURA sidecar, and the reconciliation between the two. |
//! | [`history`] | Parameter-level undo, redo, snapshots and the two resets. |
//! | [`store`] | Migration 14's four tables. |
//!
//! # The rule this crate exists to enforce
//!
//! Section 6.4: **a parameter a person touched is never overwritten by an automated pass.**
//! [`schema::merge`] is the only function in the workspace that writes one recipe into
//! another, it consults [`contract::recipe::Provenance::user_edited_fields`] on every path,
//! and it reports what it refused rather than dropping it. See
//! `docs/adr/ADR-0029-render-pipeline.md` section 6.
//!
//! # What is deliberately not here
//!
//! Nothing in this crate opens an image, decides a parameter value, or writes to a RAW.
//! Phases 15 to 17 decide values; `aura-render` executes them; invariant 1 says the
//! original is opened read-only and this crate cannot open it at all.

/// Frozen contracts. Changing anything in here requires an ADR.
pub mod contract {
    pub mod recipe;
}

pub mod errors;
pub mod fixtures;
pub mod hash;
pub mod history;
pub mod migrate;
pub mod schema;
pub mod sidecar;
pub mod store;
pub mod xmp;

pub use contract::recipe::{
    Bw, Curve, EditSource, Geometry, Global, HslShift, ImageRef, Lens, Mask, MaskKind, MaskParams,
    Noise, Perspective, Provenance, Recipe, Restoration, RetouchOp, Sharpen, ENGINE, HSL_BANDS,
    SCHEMA_VERSION,
};
pub use hash::{canonical, recipe_hash};
pub use history::{History, HistoryEntry, Snapshot};
pub use schema::{merge, MergeReport, Validation};
pub use store::RecipeStore;
