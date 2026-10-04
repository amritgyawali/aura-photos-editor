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
// The panic family is banned in library code and is how a test asserts. An inline
// `#[cfg(test)]` module is not compiled into the library at all, so nothing it does can
// reach a photographer; the lints stay denied everywhere else in the crate.
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
#![allow(
    clippy::module_name_repetitions,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::suboptimal_flops
)]

//! Measured portrait parsing: where the face, the eyes, the teeth and the hair are, in a
//! real photograph, without a trained network.
//!
//! # Why this crate exists
//!
//! Every retouch phase from 18 to 22 was built against an input port - `MaskField`,
//! `with_masks`, `with_regions` - and on a real photograph nothing filled it, because the face
//! detector phase 06 ships is a placeholder that finds no faces. So the retouch stages were
//! correct, tested and gated to zero on every frame a photographer would ever open.
//!
//! This crate is the measurement that fills the port. It finds faces with a pure-Rust
//! Viola-Jones evaluator running OpenCV's published, BSD-licensed Haar cascades - real trained
//! weights, a few hundred kilobytes of text, no runtime - and then *measures* everything else
//! from the pixels relative to the face it found:
//!
//! ```text
//! linear frame --> analysis canvas (sRGB-encoded, Lab, grey)
//!       |
//!       +--> cascade (frontal + mirrored profile) --> grouped detections
//!                     |
//!                     +--> eye cascade + darkness --> eye centres --> roll
//!                     +--> mouth map (Cr^2, Cr/Cb) --> mouth centre and width
//!                     |
//!                     +--> per-person skin model (their own cheeks, never a constant)
//!                                   |
//!   skin . face . eyes . iris . sclera . brows . under-eyes . nose . lips . teeth . mouth
//!   neck . facial hair . hair . body skin . clothing . body . background . sky
//! ```
//!
//! # A skin model is measured from the person, never assumed
//!
//! The broad prior in [`skin`] only decides which pixels of a *detected face* are sampled.
//! Every skin plane is then a distance from that person's own cheeks and forehead, so the
//! model a dark-skinned guest is segmented against is the colour of their own face. That is
//! the same rule phase 15 wrote for white balance - a skin target is measured, never assumed -
//! applied to segmentation.
//!
//! # Nothing here moves a pixel
//!
//! A parse is a set of soft planes on an analysis grid. `aura-render` resolves them onto
//! the render buffer and applies the operators; this crate never sees an edit.

pub mod canvas;
pub mod cascade;
pub mod face;
pub mod fixtures;
pub mod parse;
pub mod plane;
pub mod readings;
pub mod regions;
pub mod skin;

pub use canvas::Canvas;
pub use cascade::{Cascade, Detection};
pub use face::{FaceGeometry, FaceHint, FaceSource};
pub use parse::{analyse, analyse_linear, PortraitMap, RegionStat, ANALYSIS_EDGE, PARSE_VER};
pub use plane::Plane;
pub use readings::PortraitReadings;
pub use regions::{Region, ALL_REGIONS};
