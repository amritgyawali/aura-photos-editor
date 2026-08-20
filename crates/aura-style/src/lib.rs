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

//! What one photographer's own look is, learned from weddings they already delivered.
//!
//! PHASE-17. Phases 15 and 16 decide what a photograph *should* look like, and they decide it
//! the same way for everybody. This crate learns the difference between that consensus and
//! what one photographer would have done, from their own finished work, and it learns it
//! **per kind of photograph and per kind of light** rather than once.
//!
//! ## The shape of the thing
//!
//! ```text
//! archive folder
//!      |
//!  pairs::scan      original RAW <-> delivered final, by hash / stem / time / perceptual
//!      |
//!  extract::read    XMP present? read it : fit::optimise the recipe to reproduce the final
//!      |
//!  bucket::of       (scene group, lighting bucket) from phases 07 and 15
//!      |
//!  tree::fit        per bucket: ridge regression on six features, Huber-reweighted,
//!                   James-Stein shrunk toward the parent, archive influence capped
//!      |
//!  diagnostics      held-out re-render, per-bucket dE00, weak buckets, one sentence
//!      |
//!  profile          version, sign, export, import
//!      |
//!  infer::advise    bucket -> group -> global -> factory, bounded, explained
//! ```
//!
//! ## The three rules this crate is built on
//!
//! **A style is a residual and the baseline is never re-derived.** Every number in a
//! [`aura_core::contract::style::StyleDelta`] is added to what phases 15 and 16 already
//! decided. An empty profile, an unpopulated bucket and a cancelled training all produce
//! exactly the baseline, so there is no state of this system in which the feature makes a
//! photograph worse than switching it off would. It is also why three hundred pairs are
//! enough (section 6.2): the hard part of the problem is already solved upstream.
//!
//! **The shift happens before the guards, not after.** [`infer::advise`] is called from inside
//! phase 15's and phase 16's passes, on the solved parameters, and then phase 15's skin-locus
//! constraint and phase 16's clipping guard and skin guard run on the result. A personal style
//! that would move somebody's skin past phase 16's ceilings is a style the guard attenuates or
//! withdraws. ADR-0035 decision 1.
//!
//! **A pair the fitter cannot reproduce is rejected and reported, never down-weighted.** A
//! residual the develop pipeline cannot express is unmodelled work - a dodge, a composite, a
//! crop - and putting it into a global tone delta is how a profile learns to lift every shadow
//! in a wedding because forty frames had a brightened face. `AURA-ML-5072` is the code and
//! [`diagnostics::Report`] is where the count is said out loud.
//!
//! ## What this crate is not
//!
//! It is not a renderer, a grader or a white-balance solver. It produces *offsets* on other
//! phases' answers and it cannot produce an absolute anything - there is no field on
//! `StyleDelta` that could hold one.
//!
//! It is not a learning loop. Phase 30 updates these same profiles from in-app corrections, and
//! section 2.2 puts that out of scope here. Nothing in this crate reads a correction.
//!
//! It is not a network client. There is no cloud dependency in `Cargo.toml` and
//! `tests/no_network.rs` fails the build if one appears, which is what makes section 13's
//! "never uploads imagery" checkable rather than promised.
//!
//! ## What ships untrained, said once and plainly
//!
//! **Nothing here is a model, and that is a statement about the method rather than about the
//! data.** Ridge regression with James-Stein shrinkage and a Huber reweighting has a closed
//! form; there is nothing to sign, nothing to pin and no card to write, and `models.lock` is
//! untouched by this phase. What *is* missing is the input: there are no photographers'
//! archives in this repository, so every number the gates produce is measured against
//! synthetic archives whose style was **chosen, applied through the real renderer and
//! recovered**. That proves the arithmetic and says nothing about a photograph. It is
//! condition C1 in `docs/progress/PHASE-17-EXIT.md`.

/// The frozen `StyleService` and the resumable training pass.
pub mod api;
/// Which leaf of the tree a photograph belongs in, and the six features that condition it.
pub mod bucket;
/// The honest report: held-out evaluation, weak buckets and one sentence about what to add.
pub mod diagnostics;
/// The six typed errors this phase raises.
pub mod errors;
/// Reading a photographer's own parameters, exactly when they exist and by fitting when they
/// do not.
pub mod extract;
/// Fitting a recipe to reproduce a delivered final.
pub mod fit;
/// Synthetic archives whose style is known by construction.
pub mod fixtures;
/// Resolving a profile into one photograph's advice.
pub mod infer;
/// Matching originals to finals.
pub mod pairs;
/// Versioning, canonical form, signing, export and import.
pub mod profile;
/// Migration 17.
pub mod store;
/// The regression, the shrinkage and the archive cap.
pub mod tree;

pub use api::{Style, TrainPass, TrainReport};
pub use bucket::{Features, PairSource};
pub use diagnostics::Report;
pub use store::StyleStore;

/// How many bytes one profile costs in the catalog, at the section 11 budget.
///
/// Section 11 budgets 3 MB for a *bundle*, which is the exported artefact. This is the catalog
/// figure and it is much smaller: one `profiles` row is about 700 bytes and a populated bucket
/// is about 400, so a photographer with twelve taught buckets and eight groups costs roughly
/// 8.7 KB. The number is asserted by the phase gate against a real row rather than estimated
/// here, for the reason phase 09's `BYTES_PER_IMAGE` is.
pub const BYTES_PER_PROFILE: usize = 12 * 1024;
