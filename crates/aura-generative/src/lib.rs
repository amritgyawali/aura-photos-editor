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
// exemption phases 14, 19 and 23 took, for the same reason.
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

//! Distraction removal, bounded by a safety engine. PHASE-24.
//!
//! The exit sign glowing over the first dance, the gaffer tape across the aisle, the caterer's
//! crate at the edge of a portrait. Each takes two minutes in Photoshop and none of them is worth
//! two minutes, so they ship. This crate is those two minutes, taken four hundred times, on the
//! part of the problem where the answer is obvious - and a refusal everywhere else.
//!
//! ## Read this before reading the modules
//!
//! **This is the first code in the product that removes something the camera got right.** Phase 22
//! removed noise, which is not information. Phase 23 removed framing. This removes an object that
//! was there, and puts pixels in its place that were not.
//!
//! So the shape of this crate is upside down compared with every phase before it. The usual shape
//! is: find candidates, score them, apply the best. Here it is: find candidates, **prove each one
//! is safe**, discard the ones that cannot be proved, and only then score what is left. The score
//! has no term for safety, because a penalty is a trade and any penalty large enough to be safe
//! across four hundred frames loses on the one frame where the salience term is most confident -
//! which is the frame where the distraction is nearest the subject.
//!
//! [`safety::check`] is therefore the first module written and the one everything else is
//! downstream of. `aura_core::contract::cleanup::CleanupProposal::new` refuses a candidate whose
//! verdict is not `allowed`, so the ordering is a property of the type rather than a convention a
//! later caller could reorder.
//!
//! ## The modules
//!
//! | Module | What it does |
//! |---|---|
//! | [`policy`] | Loads `cleanup_policy.toml`, and refuses a file that widens a bound the contract owns |
//! | [`safety`] | The five checks of section 6.2, run before anything is scored |
//! | [`denylist`] | The intersection against phase 18's masks, where an absent mask blocks |
//! | [`detect`] | Unexplained-salience candidates, ranked and capped |
//! | [`errors`] | This crate's own constructors, ML 5115-5122 |
//!
//! Everything that moves a pixel lives behind one choke point, and
//! `tests/one_choke_point.rs` is a grep that fails the build if it is bypassed.
//!
//! ## What this crate cannot do
//!
//! There is no text input anywhere in it. `docs/generative-policy.md` promises that AURA never
//! generates from a description, and the way that promise is kept is that no type in the frozen
//! contract could carry one - so there is nothing here to plumb a prompt through.
//!
//! It also cannot raise a bound. The contract owns [`AREA_CAP_DEFAULT`] and its two companions;
//! `cleanup_policy.toml` may only lower them, and [`policy::Policy::load_str`] refuses a file that
//! tries the other direction.
//!
//! [`AREA_CAP_DEFAULT`]: aura_core::contract::cleanup::AREA_CAP_DEFAULT

pub mod denylist;
pub mod detect;
pub mod errors;
pub mod policy;
pub mod safety;

pub use policy::{Policy, ScenePolicy};
pub use safety::{check, Candidate, SafeCandidate};

/// Which safety arithmetic judged a stored proposal.
///
/// Bumped when any of the five checks changes how it decides, which invalidates every stored
/// verdict - phase 06's rule about version columns, and the eighth time this product has needed
/// it. A verdict allowed under one version of the denylist intersection is not necessarily
/// allowed under the next, and a comparison across the two returns a plausible number that means
/// nothing.
pub const ANALYSIS_VER: u16 = 1;

/// Which detector produced a stored candidate.
///
/// Separate from [`ANALYSIS_VER`] because it invalidates a different thing: a new detector finds
/// different regions, where new arithmetic re-judges the same ones.
pub const DETECTOR_VER: u16 = 1;

/// Whether a trained distraction detector is available.
///
/// **False, and there is no labelled wedding-distraction vocabulary in this repository.** What
/// runs instead is [`detect::candidates`], which finds unexplained salience - the half of section
/// 6.1 that can be built from measurement rather than from labels. It names nothing: every
/// candidate it produces is `DistractionClass::Unclassified`, which cannot be shown to be
/// story-irrelevant, which is why nothing in this build applies unattended.
///
/// ADR-0049 section 6 has the argument, and it is condition C2 of the phase 24 exit report.
pub const DISTRACTION_HEAD_TRAINED: bool = false;

/// Whether a trained artefact classifier is available.
///
/// **False.** The self-check is three measurements rather than a learned score, and unlike the
/// detector that is not a compromise: a repeated texture, a warped line and a terminated gradient
/// are defined by geometry, not by a label set. What a learned classifier would add is the
/// failures nobody has thought of yet.
pub const ARTEFACT_HEAD_TRAINED: bool = false;

/// Whether a local diffusion inpainting model pack is installed.
///
/// **False, and there is no fallback.** `CleanupMethod::Inpaint` returns
/// `CleanupCode::InpaintUnavailable` on every call rather than quietly running the classical fill
/// underneath it, because the fill was already tried first and calling its output an inpaint would
/// put a false disclosure on a stored row. ADR-0049 section 5.
pub const INPAINT_PACK_INSTALLED: bool = false;
