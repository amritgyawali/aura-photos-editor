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

pub mod biometric_keys;
pub mod cloud_commands;
pub mod commands;

/// Frozen contracts. Changing anything in here requires an ADR and a matching
/// regeneration of `ui/src/ipc/types.ts`.
pub mod contract {
    pub mod ipc;
}

pub mod index_commands;
pub mod infer_commands;
pub mod people_commands;
pub mod preview_commands;
pub mod state;

pub use cloud_commands::{
    check_ai_key, clear_ai_key, cloud_cache_stats, cloud_calls, cloud_spend, cloud_status,
    purge_cloud_cache, set_ai_key, set_cloud_budget, set_cloud_privacy,
};
pub use commands::{
    cancel_job, create_project, list_images, list_problems, list_projects, set_camera_label,
    start_ingest,
};
pub use index_commands::{
    build_index, embed_project, find_similar, image_descriptors, index_status,
};
pub use infer_commands::{
    hardware_plan, infer_stats, list_models, recheck_hardware, set_execution_provider,
    warmup_models,
};
pub use people_commands::{
    erase_biometrics, group_people, identity_cover, identity_timelines, list_identities,
    merge_identities, people_status, rename_identity, scan_faces, set_identity_importance,
    set_identity_role, split_identity,
};
pub use preview_commands::base64;
pub use preview_commands::{
    cancel_previews, get_preview, prefetch_previews, preview_problems, preview_stats, purge_cache,
    set_cache_budget,
};
pub use state::AppState;
