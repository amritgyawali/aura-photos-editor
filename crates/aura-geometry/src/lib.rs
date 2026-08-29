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
// exemption `aura-app` took in PHASE-14 and `aura-brain-photo` in PHASE-19, taken here for the
// same reason: these modules are geometry, their tests are dense in `crops[0]` and
// `expect("a plan")`, and rewriting each of those into a `let ... else` makes the assertion
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

//! Finishing the frame. PHASE-23.
//!
//! Three jobs that share a resample and a discipline: correct what the optics did, level what
//! the camera was not, and crop with judgement. They are one crate because they are one
//! transform - a lens correction, a rotation, a keystone and a crop compose into a single
//! sampling of the source image, and applying them separately is four interpolations of a
//! photograph that only needed one. Section 12's last failure mode is exactly that:
//! "resampling softens images - geometry applied once in the render graph rather than
//! repeatedly."
//!
//! ## The five rules this phase inherits, and the two it adds
//!
//! **`GeometryService` is the only way to ask how a photograph's frame was finished.**
//! Sixteenth service of its kind. Phase 27 checks these crops, phase 29 lays albums out of the
//! variants and phase 30 exports them; two answers to "what is this photograph's frame" is an
//! album page cropped from a rectangle the gallery never delivered.
//!
//! **A crop that cannot be proven safe is not a candidate.** The safety filter runs before the
//! composition objective, never after it as a penalty. Phase 12's rule - a guarantee outranks
//! a preference - applied where the preference is a score somebody tuned and the guarantee is
//! a bride's hands.
//!
//! ## What is not here
//!
//! **No renderer.** This crate decides a rectangle, an angle and a set of coefficients;
//! `aura-render` applies them, from `edit_recipes` through `aura_recipe::schema::merge` only.
//! `tests/no_render_calls.rs` is a grep that fails the build if this crate ever reaches a
//! pixel directly - the fourth grep-as-a-test in the repository, after `colour_discipline.rs`,
//! `no_recipe_writes.rs` and `no_template_writes.rs`.
//!
//! **No face detector, no pose model and no saliency fallback.** [`ProtectedRegion`] is the
//! input port phase 06 and phase 11 fill. A second answer to "where is her face" is a crop
//! that cuts one this product elsewhere insists it can see, and the failure is silent: a
//! wrongly refused crop looks like restraint and a wrongly accepted one looks fine until it
//! is printed.
//!
//! **No content-aware fill.** A keystone opens two corners and a rotation opens four; they are
//! cropped away, never filled. Section 2.2 puts filling in phase 24.

pub mod api;
pub mod crop;
pub mod errors;
pub mod fixtures;
pub mod guard;
pub mod keystone;
pub mod lens;
pub mod plan;
pub mod profiles;
pub mod rules;
pub mod safety;
pub mod store;
pub mod straighten;
pub mod variants;

pub use api::Geometry;
pub use aura_core::contract::geometry::{
    Aspect, CropPurpose, CropSafetyReport, CropVariant, GeometryCode, GeometryOutline,
    GeometryOverride, GeometryPlan, GeometryReason, GeometryService, Keystone, LensCorrection,
    LensSource, ProtectedKind, ProtectedRegion,
};
pub use plan::{GeometryInput, Planner, ANALYSIS_VER};
pub use profiles::{LensProfile, ProfileTable, PROFILE_VER};
pub use rules::{CropRules, SceneRule, RULES_VER};
