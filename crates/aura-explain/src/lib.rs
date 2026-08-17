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

//! The record. Everything this product decides can be opened up, argued with, and
//! reproduced a year later.
//!
//! **PHASE-13.** Twelve phases have produced evidence and one has produced a decision.
//! Every one of them carried a confidence and a list of reasons, because invariant 2 says a
//! decision without an explanation is a bug - and that left the product with eight
//! vocabularies, eight confidence scales and no way to answer the only question a support
//! case ever asks, which is *what did it do, and why*.
//!
//! ## What this crate is for, in one paragraph
//!
//! A photographer will not hand a client's wedding to a black box. The thing that converts
//! scepticism into trust is not a score - it is the crop of the soft eye, the frame that
//! nearly won shown beside the one that did, and the exact parameters that were applied.
//! This crate stores those, one row per decision, append-only, with a hash of the question
//! that produced them so that `aura replay` can ask it again.
//!
//! ## The four rules this phase adds, and every later phase inherits
//!
//! * **`ExplainService` is the only way to record what happened.** Phase 27 writes QC
//!   decisions, phase 28 reads the autonomy bands, phase 30's learning loop reads the whole
//!   table. Ninth phase, ninth time. Two ledgers is two answers to "what did the product
//!   do", and a support case cannot survive a product that disagrees with itself about its
//!   own history.
//! * **Reasons are produced by the deciding code, never by the explainer.** [`reason`]
//!   holds every code every phase can emit, assembled from the frozen enums themselves
//!   rather than copied, and a decision carrying a code that is not in it is refused rather
//!   than stored. The summariser may rephrase a reason and may never add one; [`summary`]
//!   checks that every number in a generated sentence appeared in the structured input.
//! * **Calibrated confidence is what autonomy reads, and the raw number is kept.**
//!   [`calibration`] maps one to the other per `(kind, source)`. Storing only the mapped
//!   number would make a re-calibration unfalsifiable; storing only the raw one would make
//!   the band a guess.
//! * **A guarantee that a bundle carries no pixels is a property of the shape.**
//!   `Evidence` has no variant that could hold image bytes, [`bundle`] anonymises every
//!   identifier it exports, and a test scans the output for the things that must not be in
//!   it. Section 10.1's last criterion is checked three ways because it is the one that
//!   leaves the machine.
//!
//! ## The ledger is append-only, and the database enforces it
//!
//! Migration 13 carries a trigger that aborts every `UPDATE` on `decisions`. A correction
//! is a new row whose `supersedes` points backwards. `DELETE` is deliberately still allowed,
//! because compaction and the project cascade need it - and [`ledger::Compaction`] is the
//! only thing in this crate that uses it.
//!
//! ## What this phase ships that is not yet real
//!
//! Said plainly here rather than only in the exit report.
//!
//! * **Every calibration model is the identity map at version 0.** Section 6.1 asks for
//!   isotonic regression fitted on labelled outcomes. The fitter is here, tested, and
//!   measured by `ml/eval/calibration_report.py`; what it has to fit on is user overrides
//!   from real weddings, and there are none in this repository. `AURA-ML-5058` says so once
//!   per run rather than hiding it.
//! * **The ECE gate is therefore measured on synthetic outcomes.** Section 6.1's threshold -
//!   0.05, for any decision type with 500 samples or more - is enforced by the gate against
//!   fixtures whose right answer is known by construction. That proves the estimator, not
//!   the product's calibration.
//! * **Five of the six decision kinds have no producer.** `Edit`, `Retouch`, `Qc`, `Curate`
//!   and `Export` are in the frozen vocabulary and are written by phases 14, 20, 27, 29 and
//!   30. The ledger is designed for them now precisely so that they do not each design one.

pub mod adapt;
pub mod api;
pub mod bundle;
pub mod calibration;
pub mod decision;
pub mod errors;
pub mod fixtures;
pub mod ledger;
pub mod policy;
pub mod reason;
pub mod replay;
pub mod summary;

pub use api::Explain;
pub use bundle::{Bundle, BundleOptions};
pub use calibration::{CalibrationSet, Calibrator, Reliability};
pub use decision::DecisionBuilder;
pub use ledger::{Compaction, Ledger};
pub use policy::{AutonomyPolicy, Bands, Risk};
pub use reason::{Catalog, ReasonSeverity, RegisteredReason};
pub use replay::{ReplayOutcome, ReplaySource, Replayer};
pub use summary::{Grounding, Summariser};

/// Which build's ledger semantics produced a row.
///
/// Bumped when the inputs hash, the canonical output encoding or the compaction policy
/// changes - that is, when a stored decision would no longer replay against a fresh one for
/// a reason that is about this crate rather than about the deciding phase. It is **not**
/// bumped when a band moves in `autonomy_bands.toml`: that file is versioned itself and its
/// version is recorded on every decision it banded.
pub const LEDGER_VER: u16 = 1;

/// The proxy rung this phase reads. It reads none.
///
/// Named and set to zero rather than omitted, because every phase since 02 declares the
/// pixels it opens and the honest answer here is *none*. The Explain panel renders evidence
/// crops by asking the preview service for a region of an already-cached proxy; this crate
/// stores the rectangle and never sees the pixels. That is what makes the support bundle
/// pixel-free by construction.
pub const EXPLAIN_LEVEL: u8 = 0;
