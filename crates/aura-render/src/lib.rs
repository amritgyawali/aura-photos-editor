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
// The panic family is banned in library code and is how a test asserts. `scripts/check-banned.sh`
// draws the same line by path; this draws it by compilation, which is stricter: an inline
// `#[cfg(test)]` module is not compiled into the library at all, so nothing it does can reach
// a photographer. The lints stay denied everywhere else in the crate.
#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::unwrap_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::float_cmp,
        clippy::disallowed_methods,
        clippy::uninlined_format_args
    )
)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
// Product names and standards - Lightroom, Rec.2020, Adobe RGB, Display P3, Bayer, wgpu,
// WGSL - read as prose in this crate's documentation rather than as identifiers.
#![allow(clippy::doc_markdown)]
// Imaging code converts between integer sample codes and floating point on every pixel.
// Spelling out a `try_from` and an error path for each of those conversions would bury the
// arithmetic that actually matters; every cast that could genuinely lose information is
// clamped at the point it happens.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::cast_possible_wrap,
    clippy::similar_names,
    clippy::many_single_char_names
)]

//! The develop engine: the deterministic renderer every AI editing decision writes into.
//!
//! # What is here
//!
//! | Module | What it owns |
//! |---|---|
//! | [`contract::render`] | The frozen render API. Changing one shape needs an ADR. |
//! | [`colour`] | Highlight recovery, white balance, the camera matrix. |
//! | [`profiles`] | Which matrix renders which body, and what happens when none does. |
//! | [`tonemap`] | Exposure, tone, contrast, curve, HSL, saturation, the mono mix. |
//! | [`spatial`] | The stages that read a neighbourhood, and the frame-wide statistics. |
//! | [`output`] | The output transform. **The only place tone is baked.** |
//! | [`graph`] | Which stages run, in what order, and what a changed parameter costs. |
//! | [`cpu`] | The reference pipeline. The ground truth. |
//! | [`tiles`] | Streaming, so a 100 MP export never has to fit in memory. |
//! | [`gpu`] | The accelerator port. This build ships none. |
//! | [`parity`] | The harness that compares an accelerator with the reference. |
//! | [`shaders`] | The WGSL sources, checked against the reference by a test. |
//! | [`golden`] | Digests and dE2000, for the regression gate. |
//!
//! # The three rules this crate exists to keep
//!
//! **Colour discipline.** One linear scene-referred working space, entered once and left
//! once. Invariant 8, and `crates/aura-render/tests/colour_discipline.rs` is the grep that
//! keeps [`output`] the only module that bakes a transfer function.
//!
//! **Determinism.** The same recipe over the same pixels through the same engine produces
//! the same bytes, on any machine, tiled or whole. Invariant 4. Every reduction is ordered,
//! the dither is positional rather than sequential, and the frame-wide statistics three
//! stages need are measured once and passed in rather than reduced inside a tile.
//!
//! **The RAW is never touched.** Nothing in this crate can open a file. Pixels arrive
//! through [`cpu::FrameSource`] and leave as a buffer; there is no path type in scope.
//! Invariant 1 as a property of the dependency list.
//!
//! # What this build does not have
//!
//! No `wgpu` backend. `docs/adr/ADR-0029-render-pipeline.md` section 4 has the argument, the
//! WGSL sources ship and are gated anyway, and `AURA-RENDER-8001` is raised once per session
//! so the absence is visible rather than silent. The section 11 GPU budgets are waived with
//! an expiry rather than relabelled as processor numbers.

/// Frozen contracts. Changing anything in here requires an ADR.
pub mod contract {
    pub mod render;
}

pub mod colour;
pub mod cpu;
pub mod errors;
pub mod fixtures;
pub mod golden;
pub mod gpu;
pub mod graph;
pub mod output;
pub mod parity;
pub mod profiles;
pub mod shaders;
pub mod spatial;
pub mod tiles;
pub mod tonemap;

pub use contract::render::{
    Backend, OutputColour, OutputSpec, RenderCaps, RenderLevel, RenderNote, RenderPurpose,
    RenderRequest, RenderService, RenderedData, RenderedImage, SkipReason,
};
pub use cpu::{CpuEngine, Frame, FrameSource};
pub use graph::{engine_string, render_hash, Capabilities, InputKind, Plan, Stage};
pub use parity::TOLERANCE as PARITY_TOLERANCE;
