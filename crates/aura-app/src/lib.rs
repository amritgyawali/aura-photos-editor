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

//! The application layer: one typed command surface, used by the Tauri shell and
//! by the CLI, so the UI can never reach into a crate directly.
//!
//! No command may take longer than 50 ms. Anything heavier returns a job handle
//! and streams progress events.

pub mod commands;

/// Frozen contracts. Changing anything in here requires an ADR and a matching
/// regeneration of `ui/src/ipc/types.ts`.
pub mod contract {
    pub mod ipc;
}

pub mod state;

pub use commands::{
    cancel_job, create_project, list_images, list_problems, list_projects, set_camera_label,
    start_ingest,
};
pub use state::AppState;
