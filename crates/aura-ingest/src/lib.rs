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

//! Scan, hash, pair, read metadata, and insert - idempotently.
//!
//! The single hardest requirement in this crate: importing the same folder twice
//! must insert zero rows the second time, even if the first import was killed
//! halfway, even if half the files were renamed, even if the drive letter changed.

pub mod clock_align;

/// Frozen contracts. Changing anything in here requires an ADR.
pub mod contract {
    pub mod ingest;
}

pub mod exif;
pub mod fixtures;
pub mod hash;
pub mod import;
pub mod pair;
pub mod scan;
pub mod tuning;
pub mod util;

pub use contract::ingest::{
    FileKind, FileRole, ImportMode, ImportPlan, ImportReport, ImportState, ImportedFile,
};
pub use import::{align_clocks, run};
