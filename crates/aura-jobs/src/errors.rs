//! The phase 28 error constructors.
//!
//! Six codes, `AURA-JOB-7004` to `AURA-JOB-7009`. The `JOB` domain rather than `ML`, which every
//! phase since 15 has used, because nothing in this phase is a model: these are the ways a *run*
//! goes wrong, and they sit beside phase 01's cancellation, lease and retry codes in the same
//! namespace a photographer's support bundle already groups them under.

use aura_core::contract::error::{AuraError, ErrorCode, Recovery, Severity};

use crate::contract::autopilot::StageId;

/// The pre-flight refused to start a run.
pub const AUTOPILOT_PREFLIGHT_BLOCKED: ErrorCode = ErrorCode("AURA-JOB-7004");
/// A mandatory stage could not finish.
pub const AUTOPILOT_STAGE_FAILED: ErrorCode = ErrorCode("AURA-JOB-7005");
/// The machine ran out of something the run could not continue without.
pub const AUTOPILOT_RESOURCE_STOP: ErrorCode = ErrorCode("AURA-JOB-7006");
/// A checkpoint could not be resumed and the stage started again.
pub const AUTOPILOT_CHECKPOINT_STALE: ErrorCode = ErrorCode("AURA-JOB-7007");
/// The autopilot policy file was refused.
pub const AUTOPILOT_POLICY_REFUSED: ErrorCode = ErrorCode("AURA-JOB-7008");
/// A run is already going for this project.
pub const AUTOPILOT_RUN_IN_FLIGHT: ErrorCode = ErrorCode("AURA-JOB-7009");

/// The pre-flight found something that stops a two-hour run before it starts.
///
/// `AskUser` rather than `Halt`: every blocking pre-flight row is something a person can fix -
/// free some disk, plug the laptop in, install a model pack - and a recovery of `Halt` would send
/// the shell to a runbook instead of to the dialog that says which.
#[must_use]
pub fn preflight_blocked(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        AUTOPILOT_PREFLIGHT_BLOCKED,
        Severity::RunBlocking,
        Recovery::AskUser,
        detail.into(),
        "AURA has not started, because something needs your attention first. The details are \
         above each item that needs fixing.",
    )
}

/// A stage the wedding cannot be delivered without could not finish.
#[must_use]
pub fn stage_failed(stage: StageId, detail: impl Into<String>) -> AuraError {
    AuraError::new(
        AUTOPILOT_STAGE_FAILED,
        Severity::RunBlocking,
        Recovery::Retry,
        detail.into(),
        "One step AURA cannot skip did not finish. Everything up to that point is saved, and \
         starting again will pick up where it left off.",
    )
    .with_context("stage", stage.as_str())
}

/// The governor ended the run.
///
/// `Degraded` rather than `RunBlocking`, and that is the honest severity: the run stopped at a
/// checkpoint with everything before it committed, which is a run that can be continued rather
/// than a run that failed.
#[must_use]
pub fn resource_stop(kind: &'static str, detail: impl Into<String>) -> AuraError {
    AuraError::new(
        AUTOPILOT_RESOURCE_STOP,
        Severity::Degraded,
        Recovery::AskUser,
        detail.into(),
        "AURA stopped so your machine would keep working. Everything finished so far is saved, \
         and starting again will carry on from there.",
    )
    .with_context("resource", kind)
}

/// A stored checkpoint no longer describes work this build would do.
///
/// `Fallback` rather than `Retry`: the fallback *is* re-running the stage, which is the correct
/// outcome and not a failure. It is an error at all so the run summary can say a stage was
/// re-planned rather than a photographer wondering why a resumed run took as long as a fresh one.
#[must_use]
pub fn checkpoint_stale(stage: StageId, why: &'static str) -> AuraError {
    AuraError::new(
        AUTOPILOT_CHECKPOINT_STALE,
        Severity::Degraded,
        Recovery::Fallback,
        format!("checkpoint for {stage} invalidated: {why}"),
        "Something this step depends on changed since last time, so AURA is doing that step \
         again. Nothing else has to be repeated.",
    )
    .with_context("stage", stage.as_str())
    .with_context("invalidation", why)
}

/// The autopilot policy file could not be used.
///
/// `Halt` and `RunBlocking`, matching phase 27's `AURA-ML-5140` and every policy loader since
/// phase 15: a table that cannot be trusted is not a table to fall back from, because the fallback
/// would be a set of defaults nobody chose applied to somebody's wedding.
#[must_use]
pub fn policy_refused(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        AUTOPILOT_POLICY_REFUSED,
        Severity::RunBlocking,
        Recovery::Halt,
        detail.into(),
        "AURA could not load the settings that decide what runs on its own and how hard it may \
         push your machine, so it has not started anything. Restore the file or reinstall.",
    )
}

/// A run is already going for this project.
#[must_use]
pub fn run_in_flight(run_id: impl Into<String>) -> AuraError {
    AuraError::new(
        AUTOPILOT_RUN_IN_FLIGHT,
        Severity::ItemFailed,
        Recovery::AskUser,
        "a run is already in flight for this project",
        "This wedding is already being worked on. Stop that run first, or wait for it to finish.",
    )
    .with_context("run", run_id.into())
}
