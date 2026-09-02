//! The six delivery codes. `AURA-DLV-10001` to `10006`, registered in `crates/aura-core/errors.toml`.
//!
//! A new domain rather than more `RENDER` codes, because these fail the way a network and a
//! filesystem fail rather than the way a render does, and a support case that opens "my gallery did
//! not arrive" is read by a different person from one that opens "this photograph looks wrong".
//! ADR-0061 decision 8.
//!
//! **10003 is the one to understand.** A backup copy that reads back differently from its source
//! halts, exactly as `AURA-RENDER-8022` does, and for the same reason with one addition: a backup
//! that silently contains a different file from the original is worse than no backup, because
//! somebody will restore from it.

use aura_core::contract::error::{AuraError, ErrorCode, Recovery, Severity};

/// The provider named is not one this build has.
pub const DLV_UNKNOWN_PROVIDER: ErrorCode = ErrorCode("AURA-DLV-10001");
/// The destination or the provider could not be reached.
pub const DLV_UNREACHABLE: ErrorCode = ErrorCode("AURA-DLV-10002");
/// A backup copy did not match its source.
pub const DLV_BACKUP_DIVERGED: ErrorCode = ErrorCode("AURA-DLV-10003");
/// No credential is saved for this provider.
pub const DLV_NO_CREDENTIAL: ErrorCode = ErrorCode("AURA-DLV-10004");
/// The far end reported a different digest from the one sent.
pub const DLV_UPLOAD_CORRUPT: ErrorCode = ErrorCode("AURA-DLV-10005");
/// A set has nowhere to go at this provider.
pub const DLV_SET_UNMAPPED: ErrorCode = ErrorCode("AURA-DLV-10006");

/// The provider is unknown, or its name is not one a path and a catalog key both accept.
#[must_use]
pub fn unknown_provider(name: &str) -> AuraError {
    AuraError::new(
        DLV_UNKNOWN_PROVIDER,
        Severity::ItemFailed,
        Recovery::AskUser,
        format!("no provider named `{name}` in this build"),
        "AURA does not know that client gallery. Choose one from the list in the delivery settings.",
    )
}

/// The far end is not there. Cheap to retry, because every file's state is stored.
#[must_use]
pub fn unreachable(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        DLV_UNREACHABLE,
        Severity::ItemFailed,
        Recovery::Retry,
        detail,
        "AURA could not reach that place. Everything already sent is remembered, so when it comes \
         back the upload carries on from where it stopped.",
    )
}

/// A backup copy read back differently from its source. **Halts.**
#[must_use]
pub fn backup_diverged(path: &str) -> AuraError {
    AuraError::new(
        DLV_BACKUP_DIVERGED,
        Severity::RunBlocking,
        Recovery::Halt,
        format!("`{path}` in the backup does not match the file it was copied from"),
        "A backup copy came back different from the original, so the backup was stopped. Check \
         that drive before trusting anything on it.",
    )
}

/// No credential in the OS credential store for this provider.
#[must_use]
pub fn no_credential(provider: &str) -> AuraError {
    AuraError::new(
        DLV_NO_CREDENTIAL,
        Severity::ItemFailed,
        Recovery::AskUser,
        format!("no credential stored for `{provider}`"),
        "There is no sign-in saved for that gallery. Add one in the delivery settings and the \
         upload will start.",
    )
}

/// The far end accepted a file and reported a different digest.
#[must_use]
pub fn upload_corrupt(path: &str) -> AuraError {
    AuraError::new(
        DLV_UPLOAD_CORRUPT,
        Severity::ItemFailed,
        Recovery::Retry,
        format!("the provider's digest for `{path}` is not the one that was sent"),
        "The gallery received something different from what AURA sent, so that photograph will be \
         sent again.",
    )
}

/// A set in the delivery has no mapping at this provider.
#[must_use]
pub fn set_unmapped(set: &str) -> AuraError {
    AuraError::new(
        DLV_SET_UNMAPPED,
        Severity::Warning,
        Recovery::Fallback,
        format!("no mapping for set `{set}`"),
        "One of these sets has no place to go in that gallery, so it was left out. Map it in the \
         delivery settings and run the upload again.",
    )
}
