//! The six codes phase 13 adds, `AURA-ML-5054` to `AURA-ML-5059`.
//!
//! The block has the shape every ML block since phase 09 has had - a refusal, a config
//! refusal, a per-item failure, a drift warning and a gap warning - with one difference
//! that is worth naming: **the refusal comes first here, and it is the strictest one in the
//! product.**
//!
//! ## `AURA-ML-5054` refuses to store something rather than degrading it
//!
//! Every other refusal in this workspace protects a *result*. This one protects invariant 2
//! itself: a decision with no reason, or with a reason whose code is not in the shipped
//! registry, is not written. Not written degraded, not written with a placeholder sentence -
//! not written.
//!
//! That is severe on purpose. Section 12's first failure mode is explanations drifting from
//! actual behaviour, and every softer treatment of this case ends the same way: a row in the
//! ledger that says a decision happened and cannot say why, which is exactly the row a
//! support case is looking for when it finds nothing.
//!
//! ## `AURA-ML-5057` is about a *replay*, not about a decision
//!
//! It fires when a stored decision no longer reproduces its stored outcome. That is not
//! necessarily a bug: an upgrade that improved a model is *supposed* to decide differently.
//! Which is why the message carries both hashes - the support engineer's first question is
//! whether the question changed or the answer did, and those are opposite problems.
//!
//! ## `AURA-ML-5058` fires once per run and is expected in this build
//!
//! Every shipped calibration model is the identity map. A product that let that pass
//! silently would be a product whose autonomy bands were drawn on uncalibrated numbers
//! without saying so, and section 6.4's whole argument is that the bands are only
//! defensible if 90 % means 90 %.

use aura_core::contract::error::{AuraError, ErrorCode, Recovery, Severity};

/// A decision was refused because it could not explain itself.
pub const ML_DECISION_UNEXPLAINED: ErrorCode = ErrorCode("AURA-ML-5054");
/// The autonomy band table was refused.
pub const ML_BANDS_REFUSED: ErrorCode = ErrorCode("AURA-ML-5055");
/// A decision could not be replayed.
pub const ML_REPLAY_FAILED: ErrorCode = ErrorCode("AURA-ML-5056");
/// A replayed decision no longer reproduces its stored outcome.
pub const ML_REPLAY_DRIFT: ErrorCode = ErrorCode("AURA-ML-5057");
/// A confidence was banded without a fitted calibration.
pub const ML_UNCALIBRATED_CONFIDENCE: ErrorCode = ErrorCode("AURA-ML-5058");
/// A support bundle was refused.
pub const ML_BUNDLE_REFUSED: ErrorCode = ErrorCode("AURA-ML-5059");

/// A decision carried no reason at all.
///
/// `Severity::ItemFailed` and `Recovery::Retry`: the deciding phase's pass keeps going and
/// this one decision is dropped, because a run that halted here would turn a missing
/// sentence into a failed wedding. The row is not written, which is the point.
#[must_use]
pub fn decision_unexplained(kind: &str, subject: &str) -> AuraError {
    AuraError::new(
        ML_DECISION_UNEXPLAINED,
        Severity::ItemFailed,
        Recovery::Retry,
        format!(
            "a {kind} decision about {subject} carried no reason, so it was not recorded; \
             invariant 2 requires every decision to explain itself"
        ),
        "AURA could not record why it did something, so it did not record that it happened \
         either. Nothing has been changed.",
    )
    .with_context("kind", kind)
    .with_context("subject", subject)
}

/// A decision carried a reason code nothing in this build documents.
///
/// The other half of `AURA-ML-5054`, and the half that catches drift. A code that is not in
/// the registry is a code with no documented meaning, no severity and no sentence template -
/// which is to say a code the reason-code reference does not describe and a translator has
/// never seen.
#[must_use]
pub fn unknown_reason_code(code: &str, kind: &str) -> AuraError {
    AuraError::new(
        ML_DECISION_UNEXPLAINED,
        Severity::ItemFailed,
        Recovery::Retry,
        format!(
            "a {kind} decision cited reason code `{code}`, which is not in the shipped \
             registry; docs/reason-codes.md lists every code this build documents"
        ),
        "AURA gave a reason it does not have wording for, so it did not record the decision. \
         Nothing has been changed.",
    )
    .with_context("reason_code", code)
    .with_context("kind", kind)
}

/// The autonomy band table would not load.
///
/// Halting, exactly as `AURA-ML-5051` and `AURA-ML-5052` are, and for a sharper reason than
/// either: without bands there is no mapping from a confidence to what the product is
/// allowed to do, and a build that guessed one would be a build that granted itself
/// autonomy from a missing file.
#[must_use]
pub fn bands_refused(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        ML_BANDS_REFUSED,
        Severity::RunBlocking,
        Recovery::Halt,
        format!("the autonomy band table was refused: {}", detail.into()),
        "AURA could not load the settings that decide how much it is allowed to do on its \
         own, so it has stopped rather than guessing. Restore the file or reinstall.",
    )
}

/// A decision could not be re-run at all.
#[must_use]
pub fn replay_failed(id: &str, detail: impl Into<String>) -> AuraError {
    AuraError::new(
        ML_REPLAY_FAILED,
        Severity::ItemFailed,
        Recovery::AskUser,
        format!("decision {id} could not be replayed: {}", detail.into()),
        "AURA could not work out that decision again from its record. The decision itself is \
         unchanged.",
    )
    .with_context("decision_id", id)
}

/// A decision was re-run and came out differently.
///
/// Degraded rather than failed: the stored decision is still what the gallery was built
/// from, and the drift is a fact about the difference between then and now. The message
/// carries both hashes because the first question is whether the *question* moved.
#[must_use]
pub fn replay_drift(id: &str, stored_hash: u64, fresh_hash: u64, what: &str) -> AuraError {
    let question = if stored_hash == fresh_hash {
        "the inputs are identical, so this is a determinism failure"
    } else {
        "the inputs have changed, so this is an upgrade rather than a determinism failure"
    };
    AuraError::new(
        ML_REPLAY_DRIFT,
        Severity::Degraded,
        Recovery::Fallback,
        format!(
            "decision {id} replayed differently in {what}: stored inputs hash \
             {stored_hash:016x}, fresh {fresh_hash:016x} - {question}"
        ),
        "AURA would make a different decision today than the one on record. The record is \
         unchanged and shows what was actually done.",
    )
    .with_context("decision_id", id)
    .with_context("stored_inputs_hash", format!("{stored_hash:016x}"))
    .with_context("fresh_inputs_hash", format!("{fresh_hash:016x}"))
    .with_context("differs_in", what)
}

/// Confidence was banded through an unfitted calibration.
///
/// Once per run, never per decision. An unfitted map is a property of the build.
#[must_use]
pub fn uncalibrated_confidence(kinds: &[&str]) -> AuraError {
    AuraError::new(
        ML_UNCALIBRATED_CONFIDENCE,
        Severity::Warning,
        Recovery::Fallback,
        format!(
            "no fitted calibration exists for {}, so confidence is reported exactly as the \
             deciding code produced it and the autonomy bands are drawn on uncalibrated \
             numbers",
            kinds.join(", ")
        ),
        "AURA has not yet learned how often it is right about this kind of decision, so \
         treat its confidence as a rough guide rather than a percentage.",
    )
    .with_context("kinds", kinds.join(","))
}

/// A support bundle was refused before anything was written.
///
/// `Recovery::AskUser`, because the two things that trigger it - a project that does not
/// exist and an option asking for data the bundle may not carry - are both answered by a
/// person rather than by a retry.
#[must_use]
pub fn bundle_refused(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        ML_BUNDLE_REFUSED,
        Severity::ItemFailed,
        Recovery::AskUser,
        format!("the support bundle was not written: {}", detail.into()),
        "AURA did not create the support file. Nothing has been sent anywhere.",
    )
}
