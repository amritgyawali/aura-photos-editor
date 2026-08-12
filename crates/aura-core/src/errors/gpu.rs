//! Hardware and execution-provider constructors. Codes 4000-4999, registered in
//! `errors.toml`.
//!
//! Every code in this domain is a *degradation*, never a stop: a machine without
//! a usable accelerator still analyses a wedding, only slower. That is Article X
//! rule R5 written as an error taxonomy - the absence of a GPU has a defined
//! reduced behaviour, and the reduced behaviour is the same code path with a
//! different provider underneath it.

use crate::contract::error::{AuraError, ErrorCode, Recovery, Severity};

/// No accelerated execution provider claimed this machine.
pub const GPU_NO_ACCELERATOR: ErrorCode = ErrorCode("AURA-GPU-4001");
/// A provider failed its correctness or stability check and was set aside.
pub const GPU_PROVIDER_SET_ASIDE: ErrorCode = ErrorCode("AURA-GPU-4002");
/// The accelerator memory budget was reached and the batch shrank.
pub const GPU_MEMORY_DOWNSHIFT: ErrorCode = ErrorCode("AURA-GPU-4003");
/// The hardware probe did not finish inside its ceiling.
pub const GPU_PROBE_FAILED: ErrorCode = ErrorCode("AURA-GPU-4004");
/// A persisted plan could not be read and the machine was measured again.
pub const GPU_PLAN_UNREADABLE: ErrorCode = ErrorCode("AURA-GPU-4005");

/// Nothing but the processor path is available.
#[must_use]
pub fn no_accelerator(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        GPU_NO_ACCELERATOR,
        Severity::Degraded,
        Recovery::Fallback,
        detail,
        "AURA did not find a graphics card it can use for AI, so it will use the processor \
         instead. Everything still works, just more slowly.",
    )
}

/// One provider misbehaved; it is out for this machine until a re-check.
#[must_use]
pub fn provider_set_aside(provider: &str, reason: &str) -> AuraError {
    AuraError::new(
        GPU_PROVIDER_SET_ASIDE,
        Severity::Degraded,
        Recovery::Fallback,
        format!("execution provider {provider} set aside: {reason}"),
        "One graphics path gave wrong or unstable results on this computer, so AURA switched to \
         the next one. You can see which is in use under Settings, then Hardware.",
    )
    .with_context("provider", provider)
    .with_context("reason", reason)
}

/// The ledger refused a batch and the scheduler halved it.
#[must_use]
pub fn memory_downshift(model: &str, from_batch: u16, to_batch: u16) -> AuraError {
    AuraError::new(
        GPU_MEMORY_DOWNSHIFT,
        Severity::Warning,
        Recovery::Retry,
        format!("{model}: batch {from_batch} did not fit, retrying at {to_batch}"),
        "AURA ran short of graphics memory and is working in smaller batches. The job continues \
         and the results are unchanged.",
    )
    .with_context("model", model)
    .with_context("from_batch", from_batch.to_string())
    .with_context("to_batch", to_batch.to_string())
}

/// The probe was abandoned; the conservative plan is in use for this session.
#[must_use]
pub fn probe_failed(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        GPU_PROBE_FAILED,
        Severity::Degraded,
        Recovery::Fallback,
        detail,
        "AURA could not finish measuring this computer, so it chose safe settings. You can \
         re-run the check under Settings, then Hardware.",
    )
}

/// A stored plan was unusable. Measurements are cheap; a stale plan is not.
#[must_use]
pub fn plan_unreadable(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        GPU_PLAN_UNREADABLE,
        Severity::Warning,
        Recovery::Retry,
        detail,
        "The saved hardware settings could not be read, so AURA measured this computer again. \
         Nothing else was affected.",
    )
}
