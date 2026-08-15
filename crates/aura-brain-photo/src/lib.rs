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

//! Technical judgement about one frame: focus, motion, exposure, noise and eyes.
//!
//! PHASE-09. The crate answers five questions about every photograph - is the *right*
//! subject sharp, was the motion a decision, can the exposure be brought back, how noisy
//! is it, and are the important eyes open - and it answers all five from one decode of
//! one 2048 px proxy.
//!
//! ## What it is not
//!
//! It is not a culling engine. Nothing here rejects, ranks or deletes a frame, and
//! nothing in `migration 9` or in the frozen contract would let it: section 2.2 puts
//! every keep-or-reject decision in phase 12. That restraint matters more in this phase
//! than in any before it, because `technical_score = 0.31` *reads* like a rejection and
//! section 12's first failure mode is that false rejections destroy trust instantly.
//!
//! It is also not an expression model (phase 10), a composition model (phase 11) or a
//! repair tool (phase 22). A frame is described here and acted on elsewhere.
//!
//! ## The three placeholders, said once and plainly
//!
//! * The **focus head** and the **eye-state head** ship with untrained weights, for the
//!   reason every model since phase 05 has: there is no labelled wedding data and no
//!   consented face data in this repository. Every gate that involves them is measured
//!   with `Analyser::with_reference_eyes` against fixtures whose answer is known by
//!   construction, which proves the algorithms and says nothing about the weights.
//! * The **camera calibration table**'s twenty rows are derived from published sensor
//!   specifications rather than measured from bodies, because phase 02's first exit
//!   condition - real camera files - is still open.
//! * The **per-scene isotonic calibration** ships as the identity map, because fitting
//!   one needs labelled keeper/reject pairs and there are none here.
//!
//! All three are recorded as conditions in `docs/progress/PHASE-09-EXIT.md`.
//!
//! ## The rule this phase adds
//!
//! **`IntegrityService` is the only way to ask whether a frame worked.** Phase 05 wrote
//! this for `SimilarityIndex`, phase 06 for `PeopleService`, phase 07 for `StoryService`
//! and phase 08 for `MomentService`. Fifth time, same reason: two answers to "is this
//! frame sharp" is two culling decisions that disagree.

pub mod errors;
pub mod fixtures;
pub mod integrity;

pub use integrity::{
    Analyser, Calibration, CalibrationTable, FrameContext, FrameExif, Integrity, IntegrityPass,
    IntegrityStore, PassReport, ANALYSIS_VER, INTEGRITY_LEVEL, MODEL_VER,
};
