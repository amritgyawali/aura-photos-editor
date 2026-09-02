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
// reach a photographer; the lints stay denied everywhere else in the crate. PHASE-14.
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
    // Every command on this surface takes its input DTO **by value**, because that is what the
    // Tauri handler has: it deserialises the payload off the wire and owns it. Taking a reference
    // would make every handler in `ui/src-tauri/src/main.rs` hold a binding alive across an
    // `await` for no reason, and it would make the CLI construct a value only to lend it.
    //
    // Added in phase 25, and it applies to the whole surface: phase 24's cleanup commands are what
    // clippy names, but `set_framing`, `plan_geometry` and a dozen others have the same shape and
    // are only silent because their DTOs happen to be moved into a call.
    clippy::needless_pass_by_value
)]

//! The application layer: one typed command surface, used by the Tauri shell and
//! by the CLI, so the UI can never reach into a crate directly.
//!
//! No command may take longer than 50 ms. Anything heavier returns a job handle
//! and streams progress events.

pub mod autopilot_commands;
pub mod biometric_keys;
pub mod camera_commands;
pub mod cleanup_commands;
pub mod cloud_commands;
pub mod colour_commands;
pub mod commands;
pub mod composition_commands;
pub mod cull_commands;
pub mod curate_commands;
pub mod delivery_commands;
pub mod develop_commands;
pub mod gallery_commands;
pub mod learn_commands;

/// Frozen contracts. Changing anything in here requires an ADR and a matching
/// regeneration of `ui/src/ipc/types.ts`.
pub mod contract {
    pub mod ipc;
}

pub mod emotion_commands;
pub mod explain_commands;
pub mod geometry_commands;
pub mod index_commands;
pub mod infer_commands;
pub mod integrity_commands;
pub mod local_commands;
pub mod mask_commands;
pub mod micro_commands;
pub mod moment_commands;
pub mod people_commands;
pub mod preview_commands;
pub mod qc_commands;
pub mod restore_commands;
pub mod retouch_commands;
pub mod state;
pub mod story_commands;
pub mod style_commands;
pub mod tone_commands;

pub use autopilot_commands::{
    autopilot_cancel, autopilot_events, autopilot_preflight, autopilot_progress,
    autopilot_set_settings, autopilot_stages, autopilot_start, autopilot_status, autopilot_summary,
};
pub use camera_commands::{
    camera_fingerprints, camera_pairs, camera_pass, camera_reason_codes, camera_reports,
    camera_shooter_bias, camera_status, camera_transforms, disable_camera, set_camera_override,
    set_camera_reference,
};
pub use cleanup_commands::{
    cleanup_blocked, cleanup_disclosures, cleanup_pass, cleanup_reason_codes, cleanup_status,
    decide_cleanup, disable_cleanup, image_cleanup, manual_remove,
};
pub use cloud_commands::{
    check_ai_key, clear_ai_key, cloud_cache_stats, cloud_calls, cloud_spend, cloud_status,
    purge_cloud_cache, set_ai_key, set_cloud_budget, set_cloud_privacy,
};
pub use colour_commands::{
    accept_colour, colour_review_queue, colour_status, estimate_colour, image_colour,
    select_colour_variant, set_colour_override,
};
pub use commands::{
    cancel_job, create_project, list_images, list_problems, list_projects, set_camera_label,
    start_ingest,
};
pub use composition_commands::{
    analyse_composition, composition_status, dismiss_composition_flag, flagged_composition,
    image_composition,
};
pub use cull_commands::{
    cull_project, cull_status, gallery, image_decision, override_decision, resize_gallery,
    set_cull_mode,
};
pub use curate_commands::{
    curate_album, curate_bw, curate_decide, curate_export, curate_heroes, curate_project,
    curate_set_order, curate_social, curate_spread, curate_status, curate_teaser,
};
pub use delivery_commands::{
    delivery_backup, delivery_items, delivery_providers, delivery_status, delivery_upload,
    export_files, export_manifest, export_presets, export_preview_names, export_run, export_status,
};
pub use develop_commands::{
    develop_status, history_step, image_history, image_recipe, render_caps, render_image,
    set_param, snapshot,
};
pub use emotion_commands::{
    emotion_status, image_emotion, moment_peak, prefer_frame, ranked_by_emotion, reactions_of,
    score_emotion, set_moment_peak,
};
pub use explain_commands::{
    compact_ledger, decision_by_id, decision_history, explain_image, export_support_bundle,
    ledger_status, record_decisions, review_queue,
};
pub use gallery_commands::{
    disable_gallery, gallery_node_strip, gallery_nodes, gallery_outliers, gallery_pass,
    gallery_reason_codes, gallery_status, image_gallery, pin_gallery_anchor, set_gallery_override,
};
pub use geometry_commands::{
    accept_geometry, geometry_review_queue, geometry_status, image_geometry, plan_geometry,
    set_framing,
};
pub use index_commands::{
    build_index, embed_project, find_similar, image_descriptors, index_status,
};
pub use infer_commands::{
    hardware_plan, infer_stats, list_models, recheck_hardware, set_execution_provider,
    warmup_models,
};
pub use integrity_commands::{
    analyse_integrity, dismiss_flag, flagged_images, image_integrity, integrity_status,
    within_moment,
};
pub use learn_commands::{
    diagnostics_report, learn_adopt, learn_buckets, learn_compare, learn_consent, learn_roll_back,
    learn_set_consent, learn_status,
};
pub use local_commands::{
    accept_local, image_local, local_review_queue, local_status, sculpt_local, set_local_strength,
};
pub use mask_commands::{
    edit_mask, ensure_masks, image_masks, mask_allowance, mask_kinds, mask_overlay, mask_status,
    regenerate_mask,
};
pub use micro_commands::{
    accept_micro, image_micro, micro_composites, micro_matrix, micro_pass, micro_reason_codes,
    micro_review_queue, micro_status, set_micro_matrix,
};
pub use moment_commands::{
    group_moments, list_moments, lock_moment, merge_moments, moment_duplicates, moment_of_image,
    moment_status, set_keep_hint, split_moment, undo_moment_edit,
};
pub use people_commands::{
    erase_biometrics, group_people, identity_cover, identity_timelines, image_subjects,
    list_identities, merge_identities, people_status, rename_identity, scan_faces,
    set_identity_importance, set_identity_role, split_identity,
};
pub use preview_commands::base64;
pub use preview_commands::{
    cancel_previews, get_preview, prefetch_previews, preview_problems, preview_stats, purge_cache,
    set_cache_budget,
};
pub use qc_commands::{
    qc_decide, qc_decide_bulk, qc_queue, qc_queue_grouped, qc_report, qc_report_markdown,
    qc_rounds, qc_run, qc_status, qc_tickets,
};
pub use restore_commands::{
    accept_restore, image_restore, restore_identity_refusals, restore_pass, restore_reason_codes,
    restore_review_queue, restore_status, set_restore_override,
};
pub use retouch_commands::{
    accept_retouch, image_retouch, protected_features, retouch_pass, retouch_review_queue,
    retouch_status, set_protection, set_retouch,
};
pub use state::AppState;
pub use story_commands::{
    classify_scenes, image_scene, merge_chapters, move_chapter_boundary, scene_profiles,
    segment_story, set_chapter, split_chapter, story_outline, story_status,
};
pub use style_commands::{
    adopt_profile, compare_profiles, export_profile, import_profile, list_profiles, profile_pairs,
    profile_report, scan_archive, set_project_profile, style_status, train_profile,
};
pub use tone_commands::{
    accept_tone, estimate_tone, image_tone, reference_frames, set_tone_override, tone_review_queue,
    tone_status,
};
