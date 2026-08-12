//! Job-runtime error constructors shared by `aura-jobs` and every stage it drives.

use crate::contract::error::{AuraError, ErrorCode, Recovery, Severity};

/// The run was cancelled by the photographer.
pub const JOB_CANCELLED: ErrorCode = ErrorCode("AURA-JOB-7001");
/// A worker lease expired and the task was reclaimed.
pub const JOB_LEASE_EXPIRED: ErrorCode = ErrorCode("AURA-JOB-7002");
/// A task failed more times than its retry budget allows.
pub const JOB_RETRIES_EXHAUSTED: ErrorCode = ErrorCode("AURA-JOB-7003");

/// Cancellation is a normal outcome, not a defect: it must still be typed.
#[must_use]
pub fn cancelled(stage: &'static str) -> AuraError {
    AuraError::new(
        JOB_CANCELLED,
        Severity::Warning,
        Recovery::Halt,
        format!("stage {stage} observed a cancel token"),
        "The import was stopped. Everything finished so far has been saved.",
    )
    .with_context("stage", stage)
}

/// The worker holding this task stopped renewing its lease.
#[must_use]
pub fn lease_expired(task: impl Into<String>) -> AuraError {
    AuraError::new(
        JOB_LEASE_EXPIRED,
        Severity::Degraded,
        Recovery::Retry,
        "task lease expired and was reclaimed",
        "One step stopped responding and was restarted automatically.",
    )
    .with_context("task", task.into())
}

/// The task exhausted its retry budget.
#[must_use]
pub fn retries_exhausted(task: impl Into<String>, attempts: u32) -> AuraError {
    AuraError::new(
        JOB_RETRIES_EXHAUSTED,
        Severity::ItemFailed,
        Recovery::Quarantine,
        "task exceeded its retry budget",
        "One step kept failing and was set aside so the rest of the import could finish.",
    )
    .with_context("task", task.into())
    .with_context("attempts", attempts.to_string())
}
