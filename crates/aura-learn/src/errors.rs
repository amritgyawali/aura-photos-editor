//! The five learning codes. `AURA-LRN-11001` to `11005`, registered in `crates/aura-core/errors.toml`.
//!
//! A domain of their own because these fail *statistically*: a bucket that is too small, a fit that
//! does not improve, a split that cannot be drawn. None of those is a filesystem problem or a
//! render problem, and the person reading the runbook is asking a different question. ADR-0061
//! decision 8.
//!
//! **11004 is the one to understand**, and it is a `warning` rather than a failure. A change a
//! photographer made was **kept** and not learned from, because there is no decision behind it or
//! because the project has not consented. The photograph is exactly as they left it; only the loop
//! declined. Rendering that as an error would teach photographers that correcting AURA is
//! something that goes wrong.

use aura_core::contract::error::{AuraError, ErrorCode, Recovery, Severity};

/// A stored note names a code this build does not have.
pub const LRN_UNKNOWN_REASON: ErrorCode = ErrorCode("AURA-LRN-11001");
/// A stored correction names a value this build does not learn.
pub const LRN_NOT_LEARNABLE: ErrorCode = ErrorCode("AURA-LRN-11002");
/// An update could not be worked out.
pub const LRN_NO_UPDATE: ErrorCode = ErrorCode("AURA-LRN-11003");
/// A change was kept and not learned from.
pub const LRN_NOT_ATTRIBUTED: ErrorCode = ErrorCode("AURA-LRN-11004");
/// A profile could not be rolled back.
pub const LRN_ROLLBACK_FAILED: ErrorCode = ErrorCode("AURA-LRN-11005");

/// A slug this build does not know. Degraded: draw the panel without that note.
#[must_use]
pub fn unknown_reason(slug: &str) -> AuraError {
    AuraError::new(
        LRN_UNKNOWN_REASON,
        Severity::Degraded,
        Recovery::Fallback,
        format!("unknown learning reason code `{slug}`"),
        "AURA found a note about learning that it does not recognise, which usually means this \
         profile was trained by a newer version.",
    )
}

/// A value outside the closed `Learnable` list.
#[must_use]
pub fn not_learnable(slug: &str) -> AuraError {
    AuraError::new(
        LRN_NOT_LEARNABLE,
        Severity::Degraded,
        Recovery::Fallback,
        format!("`{slug}` is not something this build learns"),
        "AURA found a correction about a setting it does not learn from. Nothing is lost; it \
         simply does not count toward your profile.",
    )
}

/// The fit failed. Not "the bucket is too small" - that is `Ok(None)` with a reason.
#[must_use]
pub fn no_update(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        LRN_NO_UPDATE,
        Severity::ItemFailed,
        Recovery::AskUser,
        detail,
        "AURA could not work out an update from your corrections. Nothing about your photographs \
         or your profile has changed.",
    )
}

/// The change was kept; only the loop declined.
#[must_use]
pub fn not_attributed(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        LRN_NOT_ATTRIBUTED,
        Severity::Warning,
        Recovery::Fallback,
        detail,
        "AURA kept your change and did not learn from it, because there was nothing of its own to \
         compare it against or learning is off for this wedding.",
    )
}

/// The previous version could not be put back.
#[must_use]
pub fn rollback_failed(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        LRN_ROLLBACK_FAILED,
        Severity::ItemFailed,
        Recovery::AskUser,
        detail,
        "AURA could not put the earlier version of that profile back. The version you are on is \
         unchanged and still works.",
    )
}
