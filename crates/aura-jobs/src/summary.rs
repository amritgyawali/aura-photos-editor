//! What a finished run says it did.
//!
//! Section 6 step 8, and section 5's `RunSummary`. The whole of this module is the argument for
//! one function: [`status_of`], which decides whether a wedding finished.
//!
//! ## Why `Completed` is a claim rather than a default
//!
//! Every earlier phase reports a result and a coverage number beside it. A run has to report *one*
//! word, and a photographer reads it as a promise. So `Completed` is reached only when every stage
//! either did its work or was switched off by the person who pressed the button, and everything
//! else is `CompletedDegraded` with a list.
//!
//! The tempting alternative is to treat a skipped optional stage as fine - it is optional, after
//! all. It is not fine: `optional` means "the run does not fail without it", which is a statement
//! about the *scheduler*, and `Completed` is a statement to the photographer. A wedding whose
//! retouch stage never started because a model was missing is a wedding with four hundred
//! unretouched faces, and the word for that is not "finished".
//!
//! Phase 27 made this distinction for an inspection - clean and skipped are different values - and
//! this is the same distinction one level up, where the audience is a person rather than a query.

use std::path::PathBuf;

use aura_core::contract::ids::RunId;
use aura_core::contract::qc::QcReport;

use crate::contract::autopilot::{
    AutopilotCode, AutopilotReason, RunStatus, RunSummary, SkipCause, StageId, StageOutcome,
};

/// One stage's result, as the summary reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finished {
    /// Which stage.
    pub stage: StageId,
    /// What happened.
    pub outcome: StageOutcome,
    /// How long it took.
    pub elapsed_ms: u64,
}

/// What word a run gets.
///
/// `cancelled` and `failed` are the caller's own knowledge - a cancelled run stopped because
/// somebody asked and a failed one because a mandatory stage ran out of attempts - and they
/// outrank the stage list, because a run that stopped at 40 % with every completed stage clean is
/// still not a finished wedding.
#[must_use]
pub fn status_of(finished: &[Finished], cancelled: bool, failed: bool) -> RunStatus {
    if failed {
        return RunStatus::Failed;
    }
    if cancelled {
        return RunStatus::Cancelled;
    }
    if finished.iter().all(|row| row.outcome.is_clean()) {
        RunStatus::Completed
    } else {
        RunStatus::CompletedDegraded
    }
}

/// Every stage that did not do what it was meant to, with a sentence.
///
/// The sentence is built here rather than stored, which is phase 27's rule at its conclusion:
/// migration 28 has no column a sentence could go in, so a run summary archived last year cannot
/// contain copy this year's release has changed.
#[must_use]
pub fn degraded(finished: &[Finished]) -> Vec<(StageId, String)> {
    finished
        .iter()
        .filter(|row| !row.outcome.is_clean())
        .map(|row| {
            let sentence = match &row.outcome {
                StageOutcome::Skipped(cause) => cause.user_text().to_string(),
                StageOutcome::Partial {
                    items,
                    failed,
                    detail,
                } => {
                    if detail.is_empty() {
                        format!("{items} finished, {failed} did not")
                    } else {
                        format!("{items} finished, {failed} did not: {detail}")
                    }
                }
                StageOutcome::Failed { code, detail } => {
                    if detail.is_empty() {
                        format!("could not finish ({code})")
                    } else {
                        format!("could not finish ({code}): {detail}")
                    }
                }
                StageOutcome::Completed { .. } => String::new(),
            };
            (row.stage, sentence)
        })
        .collect()
}

/// The reasons a run's summary carries.
///
/// One for the run itself, plus one per stage that has something to say. A stage that ran cleanly
/// contributes nothing: a reason list containing "the colour stage worked" twenty-two times is a
/// list nobody reads, and the reason a photographer opens this panel is to find the three rows
/// that are not that.
#[must_use]
pub fn reasons(finished: &[Finished], status: RunStatus, calibrated: bool) -> Vec<AutopilotReason> {
    let mut out = Vec::new();

    out.push(AutopilotReason {
        code: match status {
            RunStatus::Completed => AutopilotCode::RunComplete,
            RunStatus::CompletedDegraded | RunStatus::Running => AutopilotCode::RunDegraded,
            RunStatus::Cancelled => AutopilotCode::RunCancelled,
            RunStatus::Failed => AutopilotCode::RunFailed,
        },
        stage: None,
        detail: status.as_str().to_string(),
    });

    if !calibrated {
        out.push(AutopilotReason {
            code: AutopilotCode::UncalibratedHold,
            stage: None,
            detail: "calibration_ver=0".to_string(),
        });
    }

    for row in finished {
        let code = match &row.outcome {
            StageOutcome::Completed { .. } => continue,
            StageOutcome::Partial { .. } => AutopilotCode::StageIsolated,
            StageOutcome::Failed { .. } => AutopilotCode::StageIsolated,
            StageOutcome::Skipped(cause) => match cause {
                SkipCause::TurnedOff => continue,
                SkipCause::PhaseNotBuilt => AutopilotCode::StageUnbuilt,
                SkipCause::ServiceAbsent => AutopilotCode::StageUnavailable,
                SkipCause::ModelUntrained => AutopilotCode::StageUntrained,
                SkipCause::NoInput => AutopilotCode::StageUnavailable,
                SkipCause::AwaitingReview => AutopilotCode::StageHeld,
                SkipCause::ResourceStopped => AutopilotCode::ResourceStopped,
                SkipCause::Cancelled => AutopilotCode::RunCancelled,
            },
        };
        out.push(AutopilotReason {
            code,
            stage: Some(row.stage),
            detail: row.outcome.as_str().to_string(),
        });
    }

    out
}

/// Assemble the summary.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn build(
    run_id: RunId,
    finished: &[Finished],
    cancelled: bool,
    failed: bool,
    selected: u32,
    exported: u32,
    needs_review: u32,
    qc: Option<QcReport>,
    spend_usd: f32,
    output_path: PathBuf,
) -> RunSummary {
    RunSummary {
        run_id,
        status: status_of(finished, cancelled, failed),
        selected,
        exported,
        needs_review,
        qc,
        stage_timings: finished
            .iter()
            .map(|row| (row.stage, row.elapsed_ms))
            .collect(),
        spend_usd,
        output_path,
        degraded_stages: degraded(finished),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn done(stage: StageId) -> Finished {
        Finished {
            stage,
            outcome: StageOutcome::Completed { items: 10 },
            elapsed_ms: 100,
        }
    }

    fn skipped(stage: StageId, cause: SkipCause) -> Finished {
        Finished {
            stage,
            outcome: StageOutcome::Skipped(cause),
            elapsed_ms: 0,
        }
    }

    #[test]
    fn a_run_where_everything_worked_is_completed() {
        let finished = [done(StageId::Ingest), done(StageId::Cull)];
        assert_eq!(status_of(&finished, false, false), RunStatus::Completed);
        assert!(degraded(&finished).is_empty());
    }

    #[test]
    fn a_stage_the_photographer_turned_off_does_not_degrade_the_run() {
        let finished = [
            done(StageId::Ingest),
            skipped(StageId::Micro, SkipCause::TurnedOff),
        ];
        assert_eq!(status_of(&finished, false, false), RunStatus::Completed);
        assert!(degraded(&finished).is_empty());
    }

    #[test]
    fn a_stage_that_could_not_run_degrades_the_run_even_though_it_is_optional() {
        // The distinction this module exists for. `optional` is a statement about the scheduler;
        // `Completed` is a statement to the photographer.
        let finished = [
            done(StageId::Ingest),
            skipped(StageId::Retouch, SkipCause::ModelUntrained),
        ];
        assert_eq!(
            status_of(&finished, false, false),
            RunStatus::CompletedDegraded
        );
        let rows = degraded(&finished);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, StageId::Retouch);
        assert!(!rows[0].1.is_empty());
    }

    #[test]
    fn cancelling_outranks_a_clean_stage_list() {
        let finished = [done(StageId::Ingest)];
        assert_eq!(status_of(&finished, true, false), RunStatus::Cancelled);
    }

    #[test]
    fn a_failed_mandatory_stage_outranks_everything() {
        let finished = [done(StageId::Ingest)];
        assert_eq!(status_of(&finished, true, true), RunStatus::Failed);
    }

    #[test]
    fn every_degraded_stage_gets_a_sentence() {
        let finished = [
            skipped(StageId::Curation, SkipCause::PhaseNotBuilt),
            Finished {
                stage: StageId::Cleanup,
                outcome: StageOutcome::Failed {
                    code: "AURA-JOB-7005".into(),
                    detail: "device lost".into(),
                },
                elapsed_ms: 5,
            },
            Finished {
                stage: StageId::Colour,
                outcome: StageOutcome::Partial {
                    items: 90,
                    failed: 10,
                    detail: String::new(),
                },
                elapsed_ms: 5,
            },
        ];
        for (stage, sentence) in degraded(&finished) {
            assert!(!sentence.trim().is_empty(), "{stage} says nothing");
        }
    }

    #[test]
    fn a_clean_stage_contributes_no_reason() {
        let finished = [done(StageId::Ingest), done(StageId::Cull)];
        let reasons = reasons(&finished, RunStatus::Completed, true);
        assert_eq!(reasons.len(), 1);
        assert_eq!(reasons[0].code, AutopilotCode::RunComplete);
    }

    #[test]
    fn an_uncalibrated_build_always_says_so() {
        let finished = [done(StageId::Ingest)];
        let reasons = reasons(&finished, RunStatus::Completed, false);
        assert!(reasons
            .iter()
            .any(|reason| reason.code == AutopilotCode::UncalibratedHold));
    }

    #[test]
    fn a_held_stage_is_reported_as_held_rather_than_as_unavailable() {
        let finished = [skipped(StageId::Retouch, SkipCause::AwaitingReview)];
        let reasons = reasons(&finished, RunStatus::CompletedDegraded, false);
        assert!(reasons
            .iter()
            .any(|reason| reason.code == AutopilotCode::StageHeld
                && reason.stage == Some(StageId::Retouch)));
    }

    #[test]
    fn the_summary_totals_the_stage_timings() {
        let finished = [done(StageId::Ingest), done(StageId::Previews)];
        let summary = build(
            RunId::new(),
            &finished,
            false,
            false,
            400,
            0,
            0,
            None,
            0.0,
            PathBuf::from("."),
        );
        assert_eq!(summary.total_ms(), 200);
        assert_eq!(summary.status, RunStatus::Completed);
        assert_eq!(summary.stage_timings.len(), 2);
    }
}
