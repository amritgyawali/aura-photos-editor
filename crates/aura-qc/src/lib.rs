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
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    // `Frame` carries one optional reading per upstream phase, and a check that is handed a frame
    // with none of them must report a *skip* rather than a pass. Folding thirteen options into a
    // bitmask or a state enum would make "which input was absent" - which is the whole of
    // ADR-0055 section 8 - an argument about how to decode an integer.
    clippy::struct_excessive_bools,
    clippy::fn_params_excessive_bools,
    // Every threshold in this phase is named `max_<what>` and reads that way in
    // `qc_thresholds.toml`, in the migration and in the contract. Renaming them to satisfy a lint
    // would put three spellings of the same number in the product.
    clippy::struct_field_names
)]
// The panic family and slice indexing are banned in library code and are how a test asserts. An
// inline `#[cfg(test)]` module is not compiled into the library at all, so nothing it does can
// reach a photographer; the lints stay denied everywhere else in the crate. The same exemption
// phases 14, 19, 23, 24, 25 and 26 took, for the same reason.
#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::unwrap_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::float_cmp,
        clippy::disallowed_methods,
        clippy::assertions_on_constants,
        clippy::uninlined_format_args,
        clippy::too_many_lines
    )
)]

//! The autonomous inspector. PHASE-27.
//!
//! Twenty-six phases decided things about a photograph, a wedding or a camera body. This one reads
//! **their decisions** and judges them, and it is the first phase in the product permitted to undo
//! another phase's work.
//!
//! ## Read this before reading the modules
//!
//! A QC agent has exactly one interesting failure mode and it is not "misses a problem". A pass
//! that finds nothing leaves the gallery as phases 15 to 26 produced it, which is the gallery the
//! product would have shipped anyway. A pass that is *wrong* makes the gallery worse - silently,
//! one frame at a time, in a direction nobody is looking.
//!
//! Every structural choice in this crate follows from taking that asymmetry seriously.
//!
//! **A finding is a number against a threshold.** [`checks`] produce a [`checks::Finding`] carrying
//! a measured deviation, the threshold it failed, the unit both are in, and a predicted gain. There
//! is no path through this crate that files a ticket without all four, and the contract's
//! `QcTicket::is_well_formed` plus migration 27's CHECK constraints refuse one twice more.
//!
//! **A remedy must prove it worked, against what the ticket opened with.** [`reedit`] applies one
//! remedy, re-inspects that ticket's own metric, and keeps the change only when the realised
//! improvement is at least `MIN_GAIN_SHARE` of the predicted one *and* nothing else moved by more
//! than `MAX_COLLATERAL`. Otherwise the change is reverted and the ticket escalates. Two rounds,
//! hard.
//!
//! **A replacement is filtered, never scored.** [`replace`] re-validates coverage before it
//! compares any metric. A swap that would leave a must-have uncovered is not a worse candidate; it
//! is not a candidate.
//!
//! **The planner cannot act.** [`planner`] returns [`planner::ProposedStep`]s, which are not
//! [`aura_core::contract::qc::Remedy`]s. The only route between the two is [`remedy::validate`],
//! which takes the ticket, the policy and the frame, and returns `None` for anything outside the
//! contract's bounds. An unreachable provider, a spent budget, a malformed answer and a
//! hallucinated parameter all leave the image with its mechanical triage.
//!
//! **An absent input is a skip, never a pass.** Every check returns [`checks::Outcome`], whose
//! `Skipped` variant is a separate value from `Clean`. `QcOutline::inspection_completeness` is how
//! a caller finds out that a wedding at 100 % coverage was checked by two of the ten checks -
//! which, in a build where several upstream heads are untrained, is the common case.
//!
//! ## What this crate does not contain
//!
//! No renderer, no recipe writer, no file handle, no pixel. Every inspection is a comparison
//! between numbers other phases already measured and stored, which is what makes ten checks over a
//! thousand frames affordable inside section 11's budget and what makes each check a pure function
//! a test can drive with a literal. `tests/no_pixel_ops.rs` is the seventh grep-as-a-test in the
//! repository and fails the build if any of that stops being true.

pub mod api;
pub mod checks;
pub mod errors;
pub mod fixtures;
pub mod planner;
pub mod policy;
pub mod queue;
pub mod reedit;
pub mod remedy;
pub mod replace;
pub mod report;
pub mod store;
pub mod ticket;
pub mod triage;

pub use api::{Qc, QcPass};
pub use checks::{Finding, Frame, Outcome};
pub use policy::Thresholds;

/// Which arithmetic produced a stored deviation.
///
/// Bumped on any change to how a check measures. Phase 05's rule, inherited for the fourteenth
/// time: comparing a deviation from one version against a threshold from another returns a
/// plausible number that means nothing, and `AURA-ML-5141` exists so it never happens silently.
pub const ANALYSIS_VER: u16 = 1;

/// True when a trained defect-detection model is available.
///
/// **False in this build, and the constant exists so that is a fact rather than an omission.**
/// Section 9's DATA row asks for a labelled corpus of real defective wedding galleries and there is
/// none, so nothing here is learned: every check is a measurement against another phase's stored
/// number.
///
/// That is not a compromise in the way phases 15, 16 and 18 refused to consult an untrained head
/// and fell back on a reference model. It is the same argument phase 21 made for its glare and lint
/// detectors and phase 22 made for its denoiser: **a measurement's failure mode is finding fewer
/// problems rather than confidently inventing them**, and a QC agent that invents problems is worse
/// than no QC agent at all.
///
/// It is on the wire as `detectorTrained` for the reason phase 24 put its own there: a panel that
/// had to infer it would eventually present a threshold comparison as a learned judgement.
pub const DETECTOR_TRAINED: bool = false;
