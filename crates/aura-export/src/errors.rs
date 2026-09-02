//! The six export codes, constructed in one place. Codes 8020-8025, registered in
//! `crates/aura-core/errors.toml`, one runbook each.
//!
//! Phase 14 renamed the reserved-but-empty `EXPORT` block to `RENDER` with a note saying phase 30
//! would want codes in the same domain for the same subject. An export is a render written to a
//! file, so these live there rather than in a domain of their own.
//!
//! **8022 is the one to understand.** It is `run_blocking` on what is structurally a per-item
//! failure, which almost nothing else in this product is. ADR-0061 decision 3 has the argument, and
//! it is worth restating here because the temptation to soften it will come from somebody looking
//! at a wedding that failed on its 3,000th frame: a gallery missing one photograph is a phone call,
//! a gallery containing one corrupt photograph is a photograph nobody notices until the couple
//! opens it, and a verification failure is almost never about the file - it is about the volume.

use aura_core::contract::error::{AuraError, ErrorCode, Recovery, Severity};

/// A stored delivery note names a code this build does not have.
pub const EXPORT_UNKNOWN_REASON: ErrorCode = ErrorCode("AURA-RENDER-8020");
/// The job was refused before anything was written.
pub const EXPORT_JOB_REFUSED: ErrorCode = ErrorCode("AURA-RENDER-8021");
/// A written file did not read back the same.
pub const EXPORT_VERIFY_FAILED: ErrorCode = ErrorCode("AURA-RENDER-8022");
/// The destination is full or cannot be written to.
pub const EXPORT_DESTINATION_BAD: ErrorCode = ErrorCode("AURA-RENDER-8023");
/// One photograph could not be rendered.
pub const EXPORT_RENDER_FAILED: ErrorCode = ErrorCode("AURA-RENDER-8024");
/// A file name could not be made unique.
pub const EXPORT_NAME_EXHAUSTED: ErrorCode = ErrorCode("AURA-RENDER-8025");

/// A stored slug this build does not know. Degraded: draw the panel without that note.
#[must_use]
pub fn unknown_reason(slug: &str) -> AuraError {
    AuraError::new(
        EXPORT_UNKNOWN_REASON,
        Severity::Degraded,
        Recovery::Fallback,
        format!("unknown delivery reason code `{slug}`"),
        "AURA found a delivery note it does not recognise, which usually means this wedding was \
         delivered by a newer version.",
    )
}

/// The job does not validate. Raised **before** a frame is rendered, so nothing is written.
#[must_use]
pub fn job_refused(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        EXPORT_JOB_REFUSED,
        Severity::ItemFailed,
        Recovery::AskUser,
        detail,
        "AURA cannot run this export as it is set up. Nothing has been written. Check the sets, \
         sizes and file names, then try again.",
    )
}

/// What was read back is not what was written. **Halts the job.**
#[must_use]
pub fn verify_failed(path: &str, wrote: u64, read: u64) -> AuraError {
    AuraError::new(
        EXPORT_VERIFY_FAILED,
        Severity::RunBlocking,
        Recovery::Halt,
        format!("`{path}` was written as {wrote} bytes and read back as {read}, or its digest moved"),
        "One file AURA wrote came back different from what it sent, so the whole delivery has been \
         stopped. Check the drive before sending any of this to a client.",
    )
}

/// The destination cannot take the files.
#[must_use]
pub fn destination_bad(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        EXPORT_DESTINATION_BAD,
        Severity::RunBlocking,
        Recovery::AskUser,
        detail,
        "AURA cannot write where this export was going. Free up space or choose somewhere else; \
         what has already been written is listed and unharmed.",
    )
}

/// One photograph could not be rendered. Item-level: the other 699 frames still deliver.
#[must_use]
pub fn render_failed(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        EXPORT_RENDER_FAILED,
        Severity::ItemFailed,
        Recovery::Retry,
        detail,
        "One photograph could not be prepared, so it is not in this export. Everything else was \
         written and checked, and the summary names it.",
    )
}

/// The naming plan ran out of suffixes.
#[must_use]
pub fn name_exhausted(base: &str) -> AuraError {
    AuraError::new(
        EXPORT_NAME_EXHAUSTED,
        Severity::ItemFailed,
        Recovery::AskUser,
        format!("no free name for `{base}` within the suffix bound"),
        "AURA ran out of ways to give this photograph a name of its own. Change the file-naming \
         template so it includes a number or the original name.",
    )
}
