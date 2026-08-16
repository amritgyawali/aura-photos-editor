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

//! The decision. Three thousand frames become a gallery, and every frame on both sides of
//! the line knows why.
//!
//! **PHASE-12.** Eleven phases have written evidence into the catalog and every one of
//! them ended with the same sentence in a different form: a distance is not a duplicate, a
//! score is not a verdict, an ordering is not a shortlist, *phase 12 decides*. This is
//! phase 12.
//!
//! ## What makes this different from a threshold
//!
//! Competitors cull frame by frame against a global score. That is a defensible decision
//! about each frame and an indefensible decision about a wedding: forty near-identical
//! dance frames all clear the bar, the one slightly soft photograph of the ring exchange
//! does not, and the gallery that comes out is simultaneously bloated and missing the
//! rings.
//!
//! So this engine is a constrained optimisation over *moments and chapters* rather than a
//! filter over frames, and the constraints are ordered so that the hard ones cannot be
//! traded away:
//!
//! ```text
//! fusion  ->  moment  ->  chapter  ->  COVERAGE  ->  diversity  ->  sizing  ->  COVERAGE
//! ```
//!
//! The guard appears twice on purpose. Once so a chapter quota cannot starve a must-have,
//! and once at the very end so that shrinking the gallery with the slider cannot break one
//! - section 6.4's "the coverage guard always runs last".
//!
//! ## The four rules this phase adds, and every later phase inherits
//!
//! * **`CullService` is the only way to ask what is being delivered.** Phase 14 edits
//!   survivors, phase 27 swaps in runner-ups, phase 29 builds albums from keepers and
//!   phase 30 uploads them. Two answers to "what is in this gallery" is a delivery that
//!   does not match the album that does not match the invoice. Eighth phase, eighth time,
//!   highest stakes it has had.
//! * **A decision is reversible, and nothing on disk moves.** A rejection is a row. There
//!   is no path column, no file operation and no `deleted` flag anywhere in migration 12,
//!   and invariant 1 is why: this is the phase where "just move the rejects to a folder"
//!   first sounds reasonable.
//! * **A guarantee outranks a preference, in that order, always.** Modes, sliders, quotas
//!   and diversity caps are preferences. Must-haves and identity minimums are guarantees.
//!   [`modes`] has no access to the rule table at all, so section 10.1's "Aggressive mode
//!   still satisfies all coverage rules" is a property of the type system rather than a
//!   test result that could drift.
//! * **Say what the gallery was chosen *from*.** `CullOutline::coverage` is the fraction of
//!   the project that was eligible - that carried a phase 09 verdict - and it is the most
//!   consequential denominator in the product. A cull over 60 % of a wedding is a gallery
//!   with a four-hour hole in it that looks exactly like a gallery with a decision in it.
//!
//! ## The engine is pure, and that is the determinism guarantee
//!
//! [`Engine::select`] takes plain data and returns a [`SelectionResult`]. It has no
//! database, no clock, no network and no filesystem in reach, it iterates only
//! `BTreeMap`s and sorted vectors, and every ordering comparison ends in a tie-break on
//! the photo id. Section 10.1's "two runs on two machines produce byte-identical
//! selections" is therefore a unit test rather than a field report.
//!
//! Everything impure lives on the outside: [`gather`] reads the six frozen services,
//! [`store`] writes migration 12's tables, and [`api::Cull`] is the [`CullService`]
//! implementation that joins them.
//!
//! ## What this phase ships that is not yet real
//!
//! Three things, said plainly here rather than only in the exit report.
//!
//! * **The per-scene calibration is the identity map.** Fitting isotonic regression needs
//!   labelled keeper/reject pairs from real weddings and there are none in this
//!   repository. `KeepScore::calibration_ver` is `0` for the unfitted map so that a fitted
//!   table can never be mistaken for it.
//! * **The gallery-size regression is authored, not trained.** Section 6.4 asks for a fit
//!   over sixty real delivered galleries. [`sizing`] ships the same feature vector with
//!   coefficients chosen by argument, clamped into section 6.4's own 22-38 % band, and
//!   every coefficient carries the reason for its sign.
//! * **Every sub-score underneath the fusion comes from a placeholder head**, because
//!   phases 06, 09, 10 and 11 all ship placeholders. The arithmetic here is real and
//!   measured; the numbers it is measured on are synthetic. No claim in this phase is a
//!   claim about a real wedding's pixels until phase 05's condition C10 closes.

pub mod api;
pub mod chapter_pass;
pub mod coverage;
pub mod diversity;
pub mod engine;
pub mod errors;
pub mod explain;
pub mod fixtures;
pub mod fusion;
pub mod gather;
pub mod input;
pub mod modes;
pub mod moment_pass;
pub mod rules;
pub mod sizing;
pub mod store;
pub mod weights;

pub use api::Cull;
pub use coverage::Guard;
pub use engine::{Engine, PassReport};
pub use gather::Gatherer;
pub use input::{Candidate, CullInput, Framing, IdentityInfo, MomentInfo};
pub use rules::{CoverageRule, RuleTable};
pub use store::CullStore;
pub use weights::{SceneWeights, WeightTable};

/// Which build's passes produced a selection.
///
/// Bumped when the fusion arithmetic, any of the five passes or the sizing model changes.
/// It does **not** move when a weight or a rule in the two TOML tables changes - those are
/// covered by the config digest inside `SelectionResult::deterministic_hash`, because a
/// version number a product manager has to remember to bump is a version number that does
/// not get bumped.
pub const ANALYSIS_VER: u16 = 1;

/// The proxy rung this phase reads. It reads none.
///
/// Named and set to zero rather than omitted, because every phase since 02 has declared
/// the pixels it opens and the honest answer here is *none*. Phase 12 opens no image
/// file, decodes nothing and runs no model: every number it fuses was computed by an
/// earlier phase and stored. That is invariant 3 arriving at its conclusion - "expensive
/// work only on survivors" - and it is why section 11 budgets 1.5 seconds for the
/// selection passes over four thousand frames while budgeting eight minutes for the
/// analysis underneath them.
pub const CULL_LEVEL: u8 = 0;
