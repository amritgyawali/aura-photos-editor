//! Multi-camera and second-shooter matching: two brands and two people, one visual result. PHASE-26.
//!
//! Twenty-five phases decided things about a photograph or about a wedding. This one decides about
//! a **body** and about the person holding it, and the reason it is a phase of its own is that the
//! problem does not decompose: no amount of getting one frame right makes a Sony frame and a Canon
//! frame of the same ceremony look like one gallery, because what separates them is not a mistake
//! in either.
//!
//! ## Read this before reading the modules
//!
//! Two cameras set to the same numbers do not produce the same photograph. Their demosaics differ,
//! their forward matrices differ off the neutral axis, and - most visibly at a wedding - what each
//! one does to a highlight as it rolls off differs. So **the objective is stated over appearance
//! rather than over parameters**, which is section 6.2's formulation and is the whole reason this
//! phase is measurable at all: "the files match" is an opinion, and
//! [`AppearanceDistance`][d] falling below a number on pairs the solver never saw is a query.
//!
//! [d]: aura_core::contract::camera::AppearanceDistance
//!
//! ## The nine modules, in the order the pass uses them
//!
//! | Module | Question it answers |
//! |---|---|
//! | [`policy`] | how far a body may be moved, and what each scene tolerates |
//! | [`baseline`] | what this build knows about a brand when the wedding says nothing |
//! | [`fingerprint`] | how each body actually rendered this wedding |
//! | [`pairs`] | which photographs prove two bodies were in the same light |
//! | [`solve`] | the smallest correction that makes them agree, bounded on every axis |
//! | [`transform`] | what a correction does to a frame, and what composes with what |
//! | [`shooter`] | how differently a second photographer exposes, and how much of it survives |
//! | [`report`] | what was corrected, on what evidence, in a photographer's own words |
//! | [`store`] | migration 26 |
//!
//! Plus [`api`] (the frozen [`CameraMatchService`][s] and the resumable pass) and [`fixtures`] (the
//! synthetic two- and three-camera weddings every section 10.1 gate is measured against).
//!
//! [s]: aura_core::contract::camera::CameraMatchService
//!
//! ## The four properties that decide whether this phase is safe
//!
//! **The evidence is found, verified and graded before anything is solved.** [`pairs::find`]
//! proposes, [`pairs::verify`] compares **backgrounds** rather than subjects - scoring a pair on
//! the faces in it would be scoring the very thing under test - and only verified pairs reach
//! [`solve::fit`]. Below [`MIN_MATCHED_PAIRS`][m] the answer is blended toward a bundled brand
//! baseline in proportion to how much evidence there was, and the share is on the row.
//!
//! [m]: aura_core::contract::camera::MIN_MATCHED_PAIRS
//!
//! **A fit is checked against pairs it never saw, and a fit that fails the check is thrown away.**
//! [`solve::split_heldout`] takes a quarter of the verified pairs by pair id - deterministically,
//! because a random split makes a transform a function of a seed nobody stored - and
//! [`solve::verify`] measures the appearance distance on them before and after. A transform that
//! does not improve them falls back on the baseline and records
//! [`CameraCode::HeldOutFailed`][hf]. Section 6.2, and it is the difference between a solver and a
//! curve fitted to nine points.
//!
//! [hf]: aura_core::contract::camera::CameraCode::HeldOutFailed
//!
//! **Every axis is bounded, and the tightest bound is on the axis that can break a photograph.**
//! [`MAX_CHANNEL_GAIN`][g] is ten per cent, because a channel gain is the one parameter here that
//! takes every red in the frame with it - the roses, the sari, the exit sign - while satisfying a
//! measurement made on somebody's cheek. Section 6.2's "bounds prevent the solver from making a
//! Canon file look broken to satisfy a metric" is that number.
//!
//! [g]: aura_core::contract::camera::MAX_CHANNEL_GAIN
//!
//! **A shooter is harmonised and never erased.** [`shooter::measure`] takes a median offset per
//! scene class and [`ShooterBias::correction_for`][c] applies sixty per cent of it, capped at a
//! third of a stop. A second shooter who works darker stays darker; the gallery stops looking like
//! two weddings.
//!
//! [c]: aura_core::contract::camera::ShooterBias::correction_for
//!
//! ## Order of operations, which section 6.4 requires and this module enforces
//!
//! Camera transforms are applied **before** phase 25's within-scene normalisation, as a data
//! dependency rather than a convention: [`crate::api::collect_frames`] takes an optional
//! [`transform::Field`] and folds each frame's transform into the `Frame` it builds, so the
//! consistency pass's tree, its change points, its anchors and its targets are computed over
//! already-comparable numbers. Reversing the two produces a gallery in which every node's target
//! is the average of two brands' colour science and every frame is normalised toward a look
//! neither camera can produce. `tests/eval/camera_eval.rs` asserts the ordering, and
//! `tests/camera_no_recipe_writes.rs` asserts that nothing here reaches a pixel.
//!
//! ## What this build cannot claim
//!
//! Every gate is measured against synthetic weddings whose per-brand colour response was authored,
//! applied to authored readings and recovered. There are no multi-camera weddings in this
//! repository and no measured lens or body in it either. That is condition C1 of
//! `docs/progress/PHASE-26-EXIT.md` and it is a Sev 2 trigger. **Every bundled brand baseline in
//! `assets/camera_baselines/` was fabricated** - condition C2, and the first measured baseline
//! reopens this phase's criteria whatever phase is in flight, exactly as the first real camera file
//! reopens phase 02's.

pub mod api;
pub mod baseline;
pub mod errors;
pub mod fingerprint;
pub mod fixtures;
pub mod pairs;
pub mod policy;
pub mod report;
pub mod shooter;
pub mod solve;
pub mod store;
pub mod transform;

pub use api::{CameraMatching, MatchReport, MatchingPass};
pub use fingerprint::{BackgroundStats, CameraFrame};
pub use policy::Matching;
pub use transform::Field;

/// Which build's arithmetic produced a fingerprint, a pair, a transform or a shooter bias.
///
/// Bump on any change to the fingerprint statistics, the pairing rule, the background verification,
/// the appearance metric, the solver, the held-out split, the blend or the shooter measurement. A
/// stored row at a different value is re-solved rather than compared against; `AURA-ML-5132` exists
/// so the comparison never happens silently.
///
/// One version column and not two here: the *policy* version lives on
/// [`policy::Matching::version`] and travels with the file, exactly as phases 24 and 25 do it.
pub const ANALYSIS_VER: u16 = 1;

/// The most verified pairs kept per body, per flash state.
///
/// A cap on evidence rather than on quality, and it is what keeps `camera_pair` from growing with
/// the wedding: a body that shot alongside the reference for six hours would otherwise produce
/// thousands of rows, of which the solver uses the information in the first hundred. The pairs kept
/// are the best-verified ones, so raising this cap makes a fit slower and not better.
pub const MAX_PAIRS_PER_CAMERA: usize = 160;

/// The telemetry stage for a completed fingerprint pass. Section 11's `camera.fingerprinted`.
pub const FINGERPRINT_STAGE: &str = "camera.fingerprinted";

/// The telemetry stage for a solved transform. Section 11's `camera.matched`.
pub const MATCH_STAGE: &str = "camera.matched";

/// The telemetry stage for a body that fell back on a bundled baseline.
/// Section 11's `camera.baseline_fallback`.
pub const FALLBACK_STAGE: &str = "camera.baseline_fallback";
