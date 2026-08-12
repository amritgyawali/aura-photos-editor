//! Cloud gateway constructors. Codes 6000-6999, registered in `errors.toml`.
//!
//! One rule shapes almost every code in this domain: **a cloud failure is a
//! degradation, never a stoppage**. Invariant 6 says the product completes a full
//! wedding with no network at all, so an expired key, a rate limit, an exhausted
//! budget and a malformed response are all the same kind of event as far as the
//! pipeline is concerned - the local fallback answers instead, the decision is
//! marked `local_fallback`, and the gallery is still complete.
//!
//! Three codes break that pattern, and each one is deliberate.
//!
//! `AURA-CLOUD-6002` (the provider rejected the key) is `ask_user`, because
//! silently degrading for the rest of a wedding when the fix is thirty seconds of
//! typing is worse service than saying so.
//!
//! `AURA-CLOUD-6009` (the payload builder refused) is `quarantine`, because a
//! payload we would not send is a property of that one item, not of the network.
//!
//! `AURA-CLOUD-6012` (the OS credential store is unreachable) is `run_blocking`
//! *for the key operation the user just asked for*, not for the pipeline: the user
//! pressed Save and nothing was saved, and they have to be told.

use crate::contract::error::{AuraError, ErrorCode, Recovery, Severity};

/// No API key has been configured, so there is nothing to call with.
pub const CLOUD_NO_KEY: ErrorCode = ErrorCode("AURA-CLOUD-6001");
/// The provider rejected the key: expired, revoked, or wrong provider.
pub const CLOUD_KEY_REJECTED: ErrorCode = ErrorCode("AURA-CLOUD-6002");
/// The provider could not be reached inside the timeout.
pub const CLOUD_UNREACHABLE: ErrorCode = ErrorCode("AURA-CLOUD-6003");
/// The provider asked us to slow down or is overloaded.
pub const CLOUD_RATE_LIMITED: ErrorCode = ErrorCode("AURA-CLOUD-6004");
/// The response did not match the task's schema, even after one repair.
pub const CLOUD_SCHEMA_INVALID: ErrorCode = ErrorCode("AURA-CLOUD-6005");
/// The project or the month reached its spending cap.
pub const CLOUD_BUDGET_REACHED: ErrorCode = ErrorCode("AURA-CLOUD-6006");
/// Cloud AI is switched off for this project, or the studio is offline by choice.
pub const CLOUD_DISABLED: ErrorCode = ErrorCode("AURA-CLOUD-6007");
/// The photographer has not granted egress for this class of data.
pub const CLOUD_CONSENT_MISSING: ErrorCode = ErrorCode("AURA-CLOUD-6008");
/// The payload builder refused to build something it would not send.
pub const CLOUD_PAYLOAD_REFUSED: ErrorCode = ErrorCode("AURA-CLOUD-6009");
/// The provider answered with something we do not recognise.
pub const CLOUD_PROVIDER_ERROR: ErrorCode = ErrorCode("AURA-CLOUD-6010");
/// The agent loop hit its step, token or time limit without concluding.
pub const CLOUD_AGENT_LIMIT: ErrorCode = ErrorCode("AURA-CLOUD-6011");
/// The operating system's credential store could not be read or written.
pub const CLOUD_KEYSTORE_FAILED: ErrorCode = ErrorCode("AURA-CLOUD-6012");
/// The photographer stopped the cloud work.
pub const CLOUD_CANCELLED: ErrorCode = ErrorCode("AURA-CLOUD-6013");
/// One call would cost more than the task is allowed to spend.
pub const CLOUD_COST_CEILING: ErrorCode = ErrorCode("AURA-CLOUD-6014");

/// No key configured. Not an error the photographer did anything to cause.
#[must_use]
pub fn no_key(provider: &str) -> AuraError {
    AuraError::new(
        CLOUD_NO_KEY,
        Severity::Degraded,
        Recovery::Fallback,
        format!("no API key stored for {provider}"),
        "No AI key is set up, so AURA used its own built-in models for this step. Add a key in \
         Settings to turn the extra reasoning on.",
    )
    .with_context("provider", provider)
}

/// The provider says the key is not valid. Worth interrupting the user for.
#[must_use]
pub fn key_rejected(provider: &str, detail: impl Into<String>) -> AuraError {
    AuraError::new(
        CLOUD_KEY_REJECTED,
        Severity::Degraded,
        Recovery::AskUser,
        detail,
        "Your AI provider would not accept the key. Check it in Settings; AURA carried on with \
         its own models in the meantime.",
    )
    .with_context("provider", provider)
}

/// Network, DNS or timeout. The most common cloud failure by a wide margin.
#[must_use]
pub fn unreachable(provider: &str, detail: impl Into<String>) -> AuraError {
    AuraError::new(
        CLOUD_UNREACHABLE,
        Severity::Degraded,
        Recovery::Fallback,
        detail,
        "AURA could not reach the AI service, so it used its own models instead. Nothing was \
         lost and nothing is waiting.",
    )
    .with_context("provider", provider)
}

/// 429, 503, or an explicit slow-down. Retryable, with backoff, a bounded number
/// of times, and then the local answer.
#[must_use]
pub fn rate_limited(provider: &str, retry_after_ms: u64) -> AuraError {
    AuraError::new(
        CLOUD_RATE_LIMITED,
        Severity::Degraded,
        Recovery::Retry,
        format!("{provider} asked for a {retry_after_ms} ms pause"),
        "The AI service is busy. AURA slowed down and used its own models for the frames it \
         could not wait for.",
    )
    .with_context("provider", provider)
    .with_context("retry_after_ms", retry_after_ms.to_string())
}

/// The response is not the shape the task promised, after the repair attempt.
#[must_use]
pub fn schema_invalid(task: &str, detail: impl Into<String>) -> AuraError {
    AuraError::new(
        CLOUD_SCHEMA_INVALID,
        Severity::Degraded,
        Recovery::Fallback,
        detail,
        "The AI service answered in a form AURA could not use, so AURA used its own models for \
         that step instead.",
    )
    .with_context("task", task)
}

/// The cap is reached. A stop, not a failure: the run continues locally.
#[must_use]
pub fn budget_reached(scope: &str, cap_usd: f64, spent_usd: f64) -> AuraError {
    AuraError::new(
        CLOUD_BUDGET_REACHED,
        Severity::Warning,
        Recovery::Fallback,
        format!("{scope} cap {cap_usd:.2} reached at {spent_usd:.2}"),
        "This job reached the AI spending limit you set. AURA finished the rest with its own \
         models; raise the limit in Settings if you want more.",
    )
    .with_context("scope", scope)
}

/// Switched off, per project or globally. Expected, and reported once.
#[must_use]
pub fn disabled(reason: &'static str) -> AuraError {
    AuraError::new(
        CLOUD_DISABLED,
        Severity::Warning,
        Recovery::Fallback,
        format!("cloud AI is off: {reason}"),
        "Cloud AI is switched off, so AURA used its own models. Everything still runs; some \
         wording and grouping is simpler.",
    )
    .with_context("reason", reason)
}

/// No consent for this data class. A refusal, and never a retry.
#[must_use]
pub fn consent_missing(class: &str) -> AuraError {
    AuraError::new(
        CLOUD_CONSENT_MISSING,
        Severity::Warning,
        Recovery::Fallback,
        format!("no active consent for data class {class}"),
        "This job has not been given permission to send anything to a cloud AI, so AURA used \
         its own models. You can grant permission per job in Settings.",
    )
    .with_context("data_class", class)
}

/// The payload builder would not build it. Always a bug or a hostile input.
#[must_use]
pub fn payload_refused(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        CLOUD_PAYLOAD_REFUSED,
        Severity::ItemFailed,
        Recovery::Quarantine,
        detail,
        "AURA would not send one item to the AI service because it did not pass the privacy \
         check. That item was set aside and the rest continued.",
    )
}

/// An answer we cannot interpret. Distinct from a schema failure: the transport
/// or the envelope is wrong, not the content.
#[must_use]
pub fn provider_error(provider: &str, status: u16, detail: impl Into<String>) -> AuraError {
    AuraError::new(
        CLOUD_PROVIDER_ERROR,
        Severity::Degraded,
        Recovery::Fallback,
        detail,
        "The AI service returned something AURA did not understand, so AURA used its own models \
         for that step.",
    )
    .with_context("provider", provider)
    .with_context("status", status.to_string())
}

/// The agent ran out of steps, tokens or time. Bounded by design.
#[must_use]
pub fn agent_limit(limit: &str, detail: impl Into<String>) -> AuraError {
    AuraError::new(
        CLOUD_AGENT_LIMIT,
        Severity::Degraded,
        Recovery::Fallback,
        detail,
        "An AI reasoning step took more work than it is allowed, so AURA stopped it and used \
         its own models for that decision.",
    )
    .with_context("limit", limit)
}

/// The keychain would not answer. The one code here the user must act on now.
#[must_use]
pub fn keystore_failed(operation: &str, detail: impl Into<String>) -> AuraError {
    AuraError::new(
        CLOUD_KEYSTORE_FAILED,
        Severity::RunBlocking,
        Recovery::AskUser,
        detail,
        "AURA could not use this computer's secure key store, so the key was not saved. Nothing \
         was written anywhere else.",
    )
    .with_context("operation", operation)
}

/// Cancellation is an outcome we record, never an outcome we infer.
#[must_use]
pub fn cancelled(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        CLOUD_CANCELLED,
        Severity::Warning,
        Recovery::Halt,
        detail,
        "The AI step was stopped. Everything finished so far has been saved.",
    )
}

/// One call priced above the task's own ceiling, before it was made.
#[must_use]
pub fn cost_ceiling(task: &str, estimate_usd: f64, ceiling_usd: f64) -> AuraError {
    AuraError::new(
        CLOUD_COST_CEILING,
        Severity::Warning,
        Recovery::Fallback,
        format!("{task}: estimate {estimate_usd:.4} over ceiling {ceiling_usd:.4}"),
        "One AI request would have cost more than that kind of step is allowed to, so AURA used \
         its own models instead.",
    )
    .with_context("task", task)
}
