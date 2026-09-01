//! Picking a run back up, and deciding what may be trusted.
//!
//! Section 6.1's second half: "resume replays only unfinished units; a hash of stage inputs
//! detects when upstream changes invalidate a checkpoint, forcing a clean re-run of the affected
//! stage only."
//!
//! ## What resume is not
//!
//! It is not a state machine that reconstructs what the last process was doing. There is no
//! in-memory state to rebuild - a run's whole state is its rows, written in the same transactions
//! as the work - so resuming is reading the plan and asking, for each stage, whether it is
//! finished, unfinished but continuable, or unfinished and stale.
//!
//! That is why section 11's 20 s resume budget is met with room to spare: twenty-five checkpoint
//! reads and twenty-five input hashes, against a run whose cheapest stage takes minutes.

use crate::checkpoint;
use crate::contract::autopilot::{Checkpoint, Invalidation, StageId};

/// What a resumed stage should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plan {
    /// It finished last time and nothing it depends on moved.
    AlreadyDone,
    /// Start from unit zero.
    Restart {
        /// Why, when the restart is because something moved rather than because it never ran.
        invalidation: Invalidation,
    },
    /// Continue from this unit.
    Continue {
        /// Units already finished.
        from: u32,
    },
}

impl Plan {
    /// Whether this stage will do any work.
    #[must_use]
    pub const fn runs(self) -> bool {
        !matches!(self, Self::AlreadyDone)
    }
}

/// What to do with one stage on a resume.
///
/// `stored` is its checkpoint from the interrupted run, `current_hash` is what its declared inputs
/// hash to now, and `units` is how many it would have to do.
///
/// A finished stage whose inputs moved is **not** already done - it restarts, and everything
/// downstream of it will find its own hash moved on the next pass. That cascade is the correct
/// behaviour and it is worth being explicit about, because the tempting alternative - trusting a
/// completed stage forever - is how a wedding ends up half graded under one scene profile and half
/// under another.
#[must_use]
pub fn plan(stored: Option<&Checkpoint>, current_hash: &str, units: u32) -> Plan {
    let Some(stored) = stored else {
        return Plan::Restart {
            invalidation: Invalidation::None,
        };
    };

    let invalidation = checkpoint::invalidation(stored, current_hash, units);
    if invalidation.restarts() {
        return Plan::Restart { invalidation };
    }

    // Two outcomes mean the stage is done with: it finished, or it was skipped for a reason that
    // will still be true next time. `partial` and `failed` both mean unfinished work remains, and
    // reading `partial` as done is what the first version of this did - which made a stage
    // interrupted half-way through never run again, on exactly the resume the whole phase exists
    // for. Naming the two that are done is safer than excluding the ones that are not: a future
    // outcome slug nobody updated here would default to "run it", which costs time, rather than to
    // "skip it", which costs a wedding.
    let finished = matches!(stored.outcome.as_deref(), Some("completed" | "skipped"));
    if finished {
        return Plan::AlreadyDone;
    }

    match checkpoint::resume_from(Some(stored), invalidation, units) {
        0 => Plan::Restart {
            invalidation: Invalidation::None,
        },
        from => Plan::Continue { from },
    }
}

/// Whether a stage's checkpoint has to be discarded because a build changed.
///
/// The orchestrator's own version, rather than any stage's. A run planned by a different
/// scheduler describes a plan this one does not have - different stages, different dependencies,
/// possibly a different checkpoint format - and continuing it would be resuming onto somebody
/// else's graph.
#[must_use]
pub const fn orchestrator_moved(stored_ver: i64) -> bool {
    stored_ver != checkpoint::ORCHESTRATOR_VER
}

/// A one-line description for the run summary.
#[must_use]
pub fn describe(stage: StageId, plan: Plan) -> String {
    match plan {
        Plan::AlreadyDone => format!("{stage} was already finished"),
        Plan::Restart { invalidation } if invalidation.restarts() => {
            format!("{stage} started again: {}", invalidation.as_str())
        }
        Plan::Restart { .. } => format!("{stage} started from the beginning"),
        Plan::Continue { from } => format!("{stage} continued from unit {from}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::ids::RunId;

    fn stored(done: u32, total: u32, hash: &str, outcome: Option<&str>) -> Checkpoint {
        Checkpoint {
            run_id: RunId::new(),
            stage: StageId::Previews,
            items_done: done,
            items_total: total,
            inputs_hash: hash.to_string(),
            attempts: 1,
            outcome: outcome.map(ToString::to_string),
            elapsed_ms: 0,
        }
    }

    #[test]
    fn a_stage_with_no_checkpoint_starts_at_the_beginning() {
        assert_eq!(
            plan(None, "abc", 100),
            Plan::Restart {
                invalidation: Invalidation::None
            }
        );
    }

    #[test]
    fn an_unfinished_stage_continues_from_where_it_stopped() {
        let checkpoint = stored(40, 100, "abc", None);
        assert_eq!(
            plan(Some(&checkpoint), "abc", 100),
            Plan::Continue { from: 40 }
        );
    }

    #[test]
    fn a_finished_stage_is_not_done_again() {
        let checkpoint = stored(100, 100, "abc", Some("completed"));
        assert_eq!(plan(Some(&checkpoint), "abc", 100), Plan::AlreadyDone);
        assert!(!plan(Some(&checkpoint), "abc", 100).runs());
    }

    #[test]
    fn a_finished_stage_whose_inputs_moved_runs_again() {
        // The one that matters. Trusting a completed stage forever is how a wedding ends up half
        // graded under one scene profile and half under another.
        let checkpoint = stored(100, 100, "abc", Some("completed"));
        assert_eq!(
            plan(Some(&checkpoint), "moved", 100),
            Plan::Restart {
                invalidation: Invalidation::InputsMoved
            }
        );
    }

    #[test]
    fn a_failed_stage_is_not_already_done() {
        let checkpoint = stored(0, 100, "abc", Some("failed"));
        assert!(plan(Some(&checkpoint), "abc", 100).runs());
    }

    #[test]
    fn a_stage_interrupted_half_way_is_not_already_done() {
        // The defect this file shipped first. A cancel writes `partial` with the units that
        // finished; reading that as "done" makes the stage never run again, on exactly the resume
        // the phase exists for.
        let checkpoint = stored(40, 100, "abc", Some("partial"));
        assert_eq!(
            plan(Some(&checkpoint), "abc", 100),
            Plan::Continue { from: 40 }
        );
    }

    #[test]
    fn a_skipped_stage_is_already_done() {
        // A stage the photographer turned off must not run on a resume just because it produced
        // nothing the first time.
        let checkpoint = stored(0, 0, "abc", Some("skipped"));
        assert_eq!(plan(Some(&checkpoint), "abc", 0), Plan::AlreadyDone);
    }

    #[test]
    fn importing_more_photographs_restarts_the_stage() {
        let checkpoint = stored(100, 100, "abc", Some("completed"));
        assert_eq!(
            plan(Some(&checkpoint), "abc", 250),
            Plan::Restart {
                invalidation: Invalidation::ScopeChanged
            }
        );
    }

    #[test]
    fn a_run_planned_by_a_different_scheduler_is_not_continued() {
        assert!(!orchestrator_moved(checkpoint::ORCHESTRATOR_VER));
        assert!(orchestrator_moved(checkpoint::ORCHESTRATOR_VER + 1));
        assert!(orchestrator_moved(0));
    }

    #[test]
    fn every_plan_describes_itself() {
        for plan in [
            Plan::AlreadyDone,
            Plan::Restart {
                invalidation: Invalidation::InputsMoved,
            },
            Plan::Restart {
                invalidation: Invalidation::None,
            },
            Plan::Continue { from: 12 },
        ] {
            assert!(!describe(StageId::Colour, plan).is_empty());
        }
    }
}
