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
// The panic family and slice indexing are banned in library code and are how a test asserts.
// An inline `#[cfg(test)]` module is not compiled into the library at all, so nothing it does
// can reach a photographer; the lints stay denied everywhere else in the crate. The same
// exemption `aura-app` took in PHASE-14, taken here in PHASE-19 for the same reason: the
// local light modules are arithmetic, their tests are dense in `deltas[0]` and
// `expect("acts")`, and rewriting each of those into a `let ... else` makes the assertion
// harder to read than the property it is checking.
#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::unwrap_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::float_cmp
    )
)]

//! Judgement about one frame: whether it worked, and whether it was made.
//!
//! Two phases live here. **PHASE-09** answers whether a photograph *worked* - focus,
//! motion, exposure, noise and eyes - and **PHASE-11** answers whether it was *made*:
//! where the horizon is, what the frame cut, where the subject sits in it and what is
//! behind them. They share a crate because they share a decode, a proxy rung and a
//! discipline: both describe a photograph and neither acts on one.
//!
//! `composition` adds one rule to the five phase 09 wrote, and it is the same rule in a
//! new place: **`CompositionService` is the only way to ask how a photograph is framed.**
//! Phase 23's smart crop optimises toward this objective and phase 29 ranks albums with
//! it; two answers to "is this well composed" is a crop that fights the gallery it is in.
//!
//! `colour` adds the fourth phase to this crate - PHASE-16, tone curves, HSL and skin
//! protection - and it is the first of the four that produces something other than a
//! judgement. Phases 09 and 11 describe a photograph; phase 15 says what colour the light
//! was; phase 16 says what the photograph should look like. It shares the crate because it
//! shares the decode, the proxy rung and phase 09's own noise estimate, which is the bound on
//! how far its shadows may be opened.
//!
//! The rest of this header is phase 09's.
//!
//! ## PHASE-09. The crate answers five questions about every photograph - is the *right*
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

pub mod colour;
pub mod composition;
pub mod errors;
pub mod fixtures;
pub mod integrity;
pub mod local;
pub mod tone;

pub use colour::{
    Colour, ColourPass, ColourStore, Grade, IntentTable, SceneIntent, ToneParams, COLOUR_LEVEL,
};
pub use composition::{
    Composition, CompositionPass, CompositionStore, RuleTable, SceneRule, COMPOSITION_LEVEL,
};
pub use integrity::{
    Analyser, Calibration, CalibrationTable, FrameContext, FrameExif, Integrity, IntegrityPass,
    IntegrityStore, PassReport, ANALYSIS_VER, INTEGRITY_LEVEL, MODEL_VER,
};
pub use local::{Local, LocalPass, LocalStore, PolicyTable, ScenePolicy, LOCAL_LEVEL, SHAPING_VER};
pub use tone::{
    AsShot, LocusBuilder, Mood, SceneTarget, TargetTable, Tone, TonePass, ToneStore, TONE_LEVEL,
};
