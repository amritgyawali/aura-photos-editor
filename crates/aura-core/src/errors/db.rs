//! Catalog error constructors. `aura-catalog` maps every `rusqlite` failure through here
//! so no `SQLite` string ever reaches a photographer.

use crate::contract::error::{AuraError, ErrorCode, Recovery, Severity};

/// The catalog could not be opened, or the file is not an AURA catalog.
pub const DB_OPEN_FAILED: ErrorCode = ErrorCode("AURA-DB-3001");
/// A migration failed and was rolled back.
pub const DB_MIGRATION_FAILED: ErrorCode = ErrorCode("AURA-DB-3002");
/// The catalog failed its integrity check.
pub const DB_INTEGRITY: ErrorCode = ErrorCode("AURA-DB-3003");
/// The catalog was written by a newer build.
pub const DB_SCHEMA_TOO_NEW: ErrorCode = ErrorCode("AURA-DB-3004");
/// Another AURA instance holds the catalog write lock.
pub const DB_SECOND_WRITER: ErrorCode = ErrorCode("AURA-DB-3005");
/// A statement or transaction failed for a reason that is a defect.
pub const DB_STATEMENT_FAILED: ErrorCode = ErrorCode("AURA-DB-3006");
/// A pre-migration backup could not be verified.
pub const DB_BACKUP_UNVERIFIED: ErrorCode = ErrorCode("AURA-DB-3007");
/// The catalog stayed busy past the write timeout.
pub const DB_BUSY: ErrorCode = ErrorCode("AURA-DB-3008");

/// The catalog file could not be opened or is not an AURA catalog.
#[must_use]
pub fn open_failed(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        DB_OPEN_FAILED,
        Severity::Fatal,
        Recovery::AskUser,
        detail,
        "AURA could not open this catalog. Restart the app; if it happens again, contact support.",
    )
}

/// The catalog file could not be opened, with the underlying cause attached.
#[must_use]
pub fn open_failed_with(detail: impl Into<String>, cause: &dyn std::error::Error) -> AuraError {
    open_failed(detail).with_cause(cause)
}

/// A schema migration failed; the transaction was rolled back.
#[must_use]
pub fn migration_failed(version: i64, cause: &dyn std::error::Error) -> AuraError {
    AuraError::new(
        DB_MIGRATION_FAILED,
        Severity::Fatal,
        Recovery::Halt,
        format!("migration {version} failed and was rolled back"),
        "AURA could not upgrade this catalog. Your work is untouched; contact support before \
         importing again.",
    )
    .with_context("schema_version", version.to_string())
    .with_cause(cause)
}

/// The catalog failed `PRAGMA integrity_check` or the foreign-key check.
#[must_use]
pub fn integrity_failed(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        DB_INTEGRITY,
        Severity::Fatal,
        Recovery::AskUser,
        detail,
        "This catalog file is damaged. Restore the most recent backup from the same folder.",
    )
}

/// The integrity check itself could not run.
#[must_use]
pub fn integrity_unreadable(cause: &dyn std::error::Error) -> AuraError {
    integrity_failed("integrity check could not be read").with_cause(cause)
}

/// The catalog belongs to a newer build than this one.
#[must_use]
pub fn schema_too_new(found: i64, supported: i64) -> AuraError {
    AuraError::new(
        DB_SCHEMA_TOO_NEW,
        Severity::Fatal,
        Recovery::AskUser,
        format!("catalog schema {found} is newer than supported {supported}"),
        "This catalog was made by a newer version of AURA. Update AURA and open it again.",
    )
    .with_context("found", found.to_string())
    .with_context("supported", supported.to_string())
}

/// A second instance already owns the write lock file.
#[must_use]
pub fn second_writer(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        DB_SECOND_WRITER,
        Severity::RunBlocking,
        Recovery::AskUser,
        detail,
        "This catalog is already open in another AURA window. Close it and try again.",
    )
}

/// A prepared statement or transaction failed in a way that indicates a defect.
#[must_use]
pub fn statement_failed(detail: impl Into<String>, cause: &dyn std::error::Error) -> AuraError {
    AuraError::new(
        DB_STATEMENT_FAILED,
        Severity::RunBlocking,
        Recovery::Halt,
        detail,
        "AURA hit an internal catalog problem and stopped safely. Nothing already imported is \
         lost.",
    )
    .with_cause(cause)
}

/// The writer thread is gone, so no further writes can be accepted.
#[must_use]
pub fn writer_gone() -> AuraError {
    AuraError::new(
        DB_STATEMENT_FAILED,
        Severity::RunBlocking,
        Recovery::Halt,
        "catalog writer thread is no longer running",
        "AURA hit an internal catalog problem and stopped safely. Nothing already imported is \
         lost.",
    )
}

/// A pre-migration backup could not be proven good, so the migration must not run.
#[must_use]
pub fn backup_unverified(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        DB_BACKUP_UNVERIFIED,
        Severity::Fatal,
        Recovery::Halt,
        detail,
        "AURA could not make a safe backup before upgrading this catalog, so it stopped. Free \
         some disk space and try again.",
    )
}

/// The writer could not acquire the database within the busy timeout.
#[must_use]
pub fn busy(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        DB_BUSY,
        Severity::RunBlocking,
        Recovery::Retry,
        detail,
        "The catalog is busy. AURA will try again in a moment.",
    )
}
