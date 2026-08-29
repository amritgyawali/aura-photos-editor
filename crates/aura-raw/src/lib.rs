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
#![allow(clippy::module_name_repetitions)]
// Product names and standards - LibRaw, ColorChecker, Bayer, Rec.2020 - read as
// prose in this crate's documentation, not as identifiers, and wrapping every
// one of them in code font makes the explanations harder to read rather than
// easier.
#![allow(clippy::doc_markdown)]
// Imaging code converts between integer sample codes and floating point on
// every pixel. Spelling out a `try_from` and an error path for each of those
// conversions would bury the arithmetic that actually matters, so the numeric
// cast family is allowed crate-wide and every cast that could genuinely lose
// information is clamped at the point it happens.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::cast_possible_wrap,
    clippy::similar_names
)]

//! RAW containers, the three decode tiers and the colour pipeline.
//!
//! # The pyramid
//!
//! | Tier | What it is | Cost | Who uses it |
//! |---|---|---|---|
//! | 1 | The camera's embedded JPEG, oriented and scaled | ~6 ms | the grid, triage |
//! | 2 | A 2048 px proxy rendered from the mosaic through a documented colour path | ~120 ms | every model |
//! | 3 | Full sensor resolution, tiled | 1-3 s | final render only |
//!
//! Deep analysis on 4,000 full-resolution RAWs is unaffordable, and it is also
//! unnecessary: the camera's own preview is good enough to decide *which* frames
//! deserve attention. Everything in this crate exists to make that three-step
//! economy work without ever lying about which step a pixel came from - see
//! [`contract::pixels::PixelSource`].
//!
//! # No LibRaw
//!
//! The phase 02 plan specifies a LibRaw FFI wrapper. This build decodes in pure
//! Rust instead, and the crate keeps `#![forbid(unsafe_code)]`. The reasoning,
//! the per-format consequences and the route back to LibRaw are recorded in
//! `docs/adr/ADR-0004-raw-decode-backend.md`; the honest per-camera consequences
//! are listed in `docs/camera-support.md`.

pub mod cfa;
pub mod codec;
pub mod codecs;

/// The colour pipeline: matrices, profiles, the working space and the curve.
pub mod colour {
    pub mod curve;
    pub mod de2000;
    pub mod illuminant;
    pub mod lens;
    pub mod matrix;
    pub mod profile;
    pub mod working_space;
}

/// Container parsers. Every byte read here came from an untrusted file.
pub mod container {
    pub mod bmff;
    pub mod jpeg;
    pub mod raf;
    pub mod tiff;
}

/// Frozen contracts. Changing anything in here requires an ADR.
pub mod contract {
    pub mod pixels;
    pub mod sidecar;
}

pub mod demosaic;
pub mod fixtures;
pub mod format;
pub mod full;
pub mod losslessjpeg;
pub mod meta;
pub mod orientation;
pub mod proxy;
pub mod thumb;
pub mod timeout;

pub use contract::pixels::{
    ColourSpace, PixelBuffer, PixelData, PixelLevel, PixelSource, ProxyPair, Tile, TiledImage,
};
pub use contract::sidecar::{
    CameraProfileMeta, ClippingStats, EngineMeta, PreviewSidecar, TierMeta, PIPELINE_VER,
};
pub use format::{sniff, RawFormat};
pub use meta::{read as read_meta, RawMeta};
pub use timeout::DecodeLimits;

/// The decoder's own version string, recorded in every sidecar.
#[must_use]
pub fn decoder_version() -> String {
    format!("aura-raw {}", env!("CARGO_PKG_VERSION"))
}
