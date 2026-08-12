//! Callers never assemble severity and recovery by hand; they call a named
//! constructor. That is how the registry and the code stay in sync.

use std::path::Path;

use crate::contract::error::{AuraError, ErrorCode, Recovery, Severity};
use crate::redact::redact_path;

/// Source folder does not exist.
pub const IO_NOT_FOUND: ErrorCode = ErrorCode("AURA-IO-1001");
/// Permission denied reading the source folder.
pub const IO_PERMISSION: ErrorCode = ErrorCode("AURA-IO-1002");
/// Volume disconnected during a run.
pub const IO_DISCONNECTED: ErrorCode = ErrorCode("AURA-IO-1003");
/// Disk full.
pub const IO_DISK_FULL: ErrorCode = ErrorCode("AURA-IO-1004");
/// Path exceeds the operating system limit.
pub const IO_PATH_TOO_LONG: ErrorCode = ErrorCode("AURA-IO-1005");
/// File unreadable or truncated.
pub const IO_UNREADABLE: ErrorCode = ErrorCode("AURA-IO-1006");
/// File changed while being read.
pub const IO_CHANGED: ErrorCode = ErrorCode("AURA-IO-1007");
/// Catalog location is inside a cloud-synced folder.
pub const IO_CLOUD_SYNCED: ErrorCode = ErrorCode("AURA-IO-1008");
/// Preview cache entry could not be written or read back.
pub const IO_CACHE_UNWRITABLE: ErrorCode = ErrorCode("AURA-IO-1009");
/// Zero-byte file.
pub const IO_ZERO_BYTE: ErrorCode = ErrorCode("AURA-IO-1010");

/// The folder or file is gone.
#[must_use]
pub fn not_found(path: &Path) -> AuraError {
    AuraError::new(
        IO_NOT_FOUND,
        Severity::RunBlocking,
        Recovery::AskUser,
        format!("path does not exist: {}", path.display()),
        "That folder is not there any more. Reconnect the drive or choose the folder again.",
    )
    .with_context("path", redact_path(path))
}

/// One file could not be read; quarantine it and keep importing.
#[must_use]
pub fn unreadable(path: &Path, cause: &dyn std::error::Error) -> AuraError {
    AuraError::new(
        IO_UNREADABLE,
        Severity::ItemFailed,
        Recovery::Quarantine,
        format!("unreadable file: {}", path.display()),
        "One file could not be read and was set aside. Check the card before formatting it.",
    )
    .with_context("path", redact_path(path))
    .with_cause(cause)
}

/// The file has zero bytes, which almost always means a failed card transfer.
#[must_use]
pub fn zero_byte(path: &Path) -> AuraError {
    AuraError::new(
        IO_ZERO_BYTE,
        Severity::ItemFailed,
        Recovery::Quarantine,
        format!("zero-byte file: {}", path.display()),
        "One file is empty, which usually means a failed card transfer. It has been set aside.",
    )
    .with_context("path", redact_path(path))
}

/// The path is longer than the platform can open.
#[must_use]
pub fn path_too_long(path: &Path) -> AuraError {
    AuraError::new(
        IO_PATH_TOO_LONG,
        Severity::ItemFailed,
        Recovery::Quarantine,
        format!(
            "path length {} exceeds the platform limit",
            path.as_os_str().len()
        ),
        "One file is nested too deeply for Windows to open. It has been set aside; the rest \
         imported normally.",
    )
    .with_context("path", redact_path(path))
}

/// The file changed size or mtime while it was being read.
#[must_use]
pub fn changed_while_reading(path: &Path) -> AuraError {
    AuraError::new(
        IO_CHANGED,
        Severity::ItemFailed,
        Recovery::Retry,
        format!("file changed during read: {}", path.display()),
        "A file was still being written while AURA read it. It will be retried.",
    )
    .with_context("path", redact_path(path))
}

/// The volume vanished mid-run.
#[must_use]
pub fn disconnected(path: &Path) -> AuraError {
    AuraError::new(
        IO_DISCONNECTED,
        Severity::RunBlocking,
        Recovery::Retry,
        format!("volume disappeared: {}", path.display()),
        "The drive was disconnected. Plug it back in and AURA will carry on from where it \
         stopped.",
    )
    .with_context("path", redact_path(path))
}

/// A cache entry could not be written, or read back with the digest it claims.
///
/// A cache is never truth, so this is a warning with a retry: the caller already
/// holds the pixels in memory and only the on-disk copy was lost.
#[must_use]
pub fn cache_unwritable(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        IO_CACHE_UNWRITABLE,
        Severity::Warning,
        Recovery::Retry,
        detail,
        "AURA could not store a preview on disk, so it will be built again when needed. Check \
         free space on the cache drive.",
    )
}

/// Map a `std::io::Error` to the right code. This is the only place in the
/// codebase permitted to inspect `io::ErrorKind`.
#[must_use]
pub fn from_io(err: &std::io::Error, path: &Path) -> AuraError {
    use std::io::ErrorKind as K;
    match err.kind() {
        K::NotFound => not_found(path),
        K::PermissionDenied => AuraError::new(
            IO_PERMISSION,
            Severity::RunBlocking,
            Recovery::AskUser,
            format!("permission denied: {}", path.display()),
            "AURA is not allowed to read that folder. Grant access in system settings, then \
             try again.",
        )
        .with_context("path", redact_path(path)),
        K::StorageFull => AuraError::new(
            IO_DISK_FULL,
            Severity::RunBlocking,
            Recovery::Halt,
            "disk full",
            "The disk is full. Free up space and AURA will resume; nothing done so far is lost.",
        ),
        _ => unreadable(path, err),
    }
}
