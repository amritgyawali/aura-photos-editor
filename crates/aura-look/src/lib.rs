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

//! Matching a look somebody else published.
//!
//! PHASE-31. A photographer points at a page they admire - most often an Instagram account -
//! and at a folder of their own photographs, and AURA makes the second look like the first.
//!
//! ## The shape of the thing
//!
//! ```text
//! a page address                 a folder of reference photographs
//!      |                                      |
//!  ReferenceOrigin::parse            source::walk
//!  (provenance, resolves nothing)    (decode, hash, refuse what is not an image)
//!      |                                      |
//!      +------------------+-------------------+
//!                         |
//!                    measure::read      one decode, eleven readings: seven tone
//!                         |             landmarks, three zone tints, eight hue bands
//!                         |
//!                    light::bucket      which of ten lights, from the pixels alone
//!                         |
//!                  aggregate::fold      median and MAD per lighting bucket
//!                         |
//!      the photographer's own frames, measured the same way, after phases 15 and 16
//!                         |
//!                    solve::delta       reference aggregate - baseline aggregate,
//!                         |             shrunk toward the global lean, then bounded
//!                         |
//!                  materialise::into_style   one lighting-conditioned delta, written
//!                         |                  into every scene group with a reason
//!                         |
//!                    verify::measure    render the photographer's own frames with it
//!                                       and measure how far they actually moved
//! ```
//!
//! ## What this crate does not do
//!
//! **It does not fetch anything.** There is no HTTP client here, no socket, and no dependency
//! that carries one. `scripts/check-banned.sh` refuses an outbound socket outside `aura-cloud`
//! and `tests/no_network.rs` refuses the dependency; ADR-0063 section 4 has the argument, and
//! `MediaSource::PublicUrl` is the refusal a photographer reads rather than a variant nobody
//! can see the shape of.
//!
//! **It does not write a recipe.** A look is a `StyleDelta`, phases 15 and 16 add it to what
//! they already solved, and every guard in both phases runs afterwards on the result.
//! `tests/no_recipe_writes.rs` is the grep that keeps that true - the eleventh in this
//! repository.
//!
//! **It does not move a pixel of its own.** The only renders in this crate are in
//! [`verify`], and they exist to *measure* a match rather than to produce one.
//!
//! **It does not learn anything about skin.** See `aura_core::contract::look`'s header, third
//! thing, and `docs/skin-fairness.md`.

/// The appearance vocabulary: one decode, eleven readings.
pub mod measure;

/// Which of ten lights a reference photograph was made in, from its pixels alone.
pub mod light;

/// Where reference photographs come from, and the one source this build cannot use.
pub mod source;

/// The robust middle of many readings.
pub mod aggregate;

/// Reference minus baseline, shrunk and bounded.
pub mod solve;

/// A look, written into phase 17's frozen profile shape.
pub mod materialise;

/// What a look actually did, measured through the real renderer.
pub mod verify;

/// Migration 31 and the rows.
pub mod store;

/// The frozen `LookService` and the measuring pass.
pub mod api;

/// Synthetic reference galleries with a known answer.
pub mod fixtures;

/// Which build's measurer and solver produced a look.
///
/// Bumped on **any** change to [`measure`], [`light`], [`aggregate`] or [`solve`], because all
/// four decide what a stored number means. A look measured at one version and compared against
/// a baseline measured at another is the comparison `AURA-ML-5148` exists to prevent - the
/// eighth version column in the product, and the rule phases 05 to 10 each wrote: a version
/// counts *measurements*, not commits.
pub const ANALYSIS_VER: u16 = 1;

pub use api::{Look, MeasurePass, MeasureReport};
pub use measure::read;
pub use source::{Reference, ReferenceFile};
