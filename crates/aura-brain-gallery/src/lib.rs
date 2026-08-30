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
    // Every bound in this phase is named `max_d_<axis>` and every one of the five reads that way in
    // `consistency.toml`, in the migration and in the contract. Renaming them to satisfy a lint
    // would put four spellings of the same number in the product.
    clippy::struct_field_names,
    // `Frame` carries four independent booleans - mixed light, intentional light, user edited,
    // enabled - and every one of them is read by a different rule for a different reason. Folding
    // them into a bitflag or a state enum would make `Frame::blocked_by`'s priority order, which is
    // the whole point of the type, an argument about how to decode an integer.
    clippy::struct_excessive_bools,
    clippy::fn_params_excessive_bools
)]
// The panic family and slice indexing are banned in library code and are how a test asserts. An
// inline `#[cfg(test)]` module is not compiled into the library at all, so nothing it does can
// reach a photographer; the lints stay denied everywhere else in the crate. The same exemption
// phases 14, 19, 23 and 24 took, for the same reason.
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
        clippy::uninlined_format_args
    )
)]

//! Gallery intelligence: a wedding matched to itself. PHASE-25.
//!
//! Twenty-four phases decided things about **one photograph at a time**. This one decides about a
//! wedding, and that is a different problem rather than a bigger version of the same one.
//!
//! ## Read this before reading the modules
//!
//! Phase 15 can be inside its own 200 K tolerance on every frame of a ceremony and still produce a
//! ceremony that visibly warms and cools as somebody scrolls, because 200 K of independent error
//! either side of a mean is a 400 K swing between two adjacent frames. **Every per-frame gate in
//! this product can be green while the thing a client actually looks at is wrong.** That is what
//! this crate is for, and it is why nothing in it is a function of a photograph.
//!
//! ## The nine modules, in the order the pass uses them
//!
//! | Module | Question it answers |
//! |---|---|
//! | [`policy`] | how far anything is allowed to move, and what a scene tolerates |
//! | [`tree`] | which photographs should look like each other |
//! | [`changepoint`] | where the light genuinely changed, so a node becomes two |
//! | [`stats`] | what a set of frames looks like, robustly |
//! | [`anchors`] | which frames a node should be judged against |
//! | [`normalise`] | how far every other frame moves toward them |
//! | [`skin_consistency`] | how one person should look across a whole wedding |
//! | [`scene_consistency`] | how a node's contrast and colour character are harmonised |
//! | [`outlier`] | which frames would not come, and by how much |
//!
//! Plus [`store`] (migration 25), [`api`] (the frozen [`GalleryService`] and the resumable walk)
//! and [`fixtures`] (the synthetic galleries every section 10.1 gate is measured against).
//!
//! [`GalleryService`]: aura_core::contract::gallery::GalleryService
//!
//! ## The three properties that decide whether this phase is safe
//!
//! **The delta is measured from the un-normalised world.** [`normalise::solve`] reads phase 15's
//! `ToneEstimate` and phase 16's `ColourDecision` and never reads a `NormalisationDelta`. Its input
//! is immutable with respect to its own output, so a second run computes the same number and
//! writing it again is a no-op. That is what idempotence *is* here; the gate in
//! `tests/eval/consistency_eval.rs` is a regression guard rather than the mechanism. ADR-0051
//! section 2.
//!
//! **A change point splits a node before its anchors are chosen.** Section 2.1's candle-lit vow
//! inside a bright ceremony has exactly two outcomes if it shares a normalisation group with the
//! ceremony - the vow is flattened, or the ceremony is dragged toward it - and no damping factor
//! avoids both. So [`changepoint::split`] runs before [`anchors::select`], and each side of a
//! genuine transition gets its own target. ADR-0051 section 5.
//!
//! **A node the product could not judge is a different row from a node it judged and left alone.**
//! `GalleryCode::NodeUnanchored` and `GalleryCode::AlreadyConsistent` both produce five zeroes, and
//! they mean opposite things. Phase 24's rule - an absent input is ignorance, not permission - in
//! the phase where the two are easiest to confuse.
//!
//! ## What this crate does not do
//!
//! It writes no recipe, opens no file, reaches no provider and keeps no tone solver.
//! `tests/no_recipe_writes.rs` is the grep that keeps it that way - the sixth in the repository,
//! after `colour_discipline.rs`, `aura-brain-photo`'s `no_recipe_writes.rs`,
//! `no_template_writes.rs`, `no_render_calls.rs` and `one_choke_point.rs`.
//!
//! ## What this build cannot claim
//!
//! Every gate is measured against synthetic galleries whose drift was authored and whose lighting
//! transitions are known by construction. There are no weddings in this repository. That is
//! condition C1 of `docs/progress/PHASE-25-EXIT.md`, it is a Sev 2 trigger, and it closes with
//! phase 05's C10 rather than separately - because the anchors this phase ranks are ranked partly
//! on phase 06's face detector, which finds no faces in a photograph.
//!
//! And [`SKIN_FIELD_AVAILABLE`] is false in this build: phase 18's segmentation head is untrained,
//! so no identity-scoped skin region exists to correct inside, and every frame records
//! `GalleryCode::SkinMaskAbsent` rather than a correction. Section 6.3's promise is measured on
//! authored readings and is not a claim about a person. That is condition C2.

pub mod anchors;
pub mod api;
pub mod changepoint;
pub mod errors;
pub mod fixtures;
pub mod normalise;
pub mod outlier;
pub mod policy;
pub mod scene_consistency;
pub mod skin_consistency;
pub mod stats;
pub mod store;
pub mod tree;

pub use policy::Consistency;
pub use skin_consistency::{SkinField, SkinReading};

/// Which build's arithmetic produced a node, an anchor, a target, a delta or an outlier.
///
/// Bump on any change to the tree construction, the change-point detector, the anchor ranking, the
/// robust statistics, the solver or the outlier threshold. A stored row at a different value is
/// re-solved rather than compared against; `AURA-ML-5127` exists so the comparison never happens
/// silently.
///
/// One version column and not two here: the *policy* version lives on
/// [`policy::Consistency::version`] and travels with the file, exactly as phase 24's does.
pub const ANALYSIS_VER: u16 = 1;

/// Whether this build can read a per-frame, per-identity skin region.
///
/// False. Phase 18's `SEG_HEAD_TRAINED` is false, so `MaskService::ensure` produces no
/// identity-scoped `MaskKind::Skin` region on a photograph, and there is nothing for a skin
/// correction to apply inside.
///
/// **It is a constant rather than an inference**, and it is on the wire in `GalleryStatusDto`, for
/// the reason phase 24 put `detector_trained` there: a panel that had to guess would eventually
/// render "everybody's skin is consistent" for a build that cannot look at skin. The skin half of
/// this phase is exercised end to end by [`fixtures`] against authored readings, which proves the
/// arithmetic and says nothing about a photograph.
pub const SKIN_FIELD_AVAILABLE: bool = false;

/// The telemetry stage for a completed consistency pass. Section 11's `gallery.normalised`.
pub const STAGE: &str = "gallery.normalised";

/// The telemetry stage for the skin half. Section 11's `gallery.skin_corrected`.
pub const SKIN_STAGE: &str = "gallery.skin_corrected";

/// The telemetry stage for the outlier report. Section 11's `gallery.outliers`.
pub const OUTLIER_STAGE: &str = "gallery.outliers";

/// The telemetry stage for a pinned anchor. Section 11's `gallery.anchor_pinned`.
pub const ANCHOR_STAGE: &str = "gallery.anchor_pinned";
