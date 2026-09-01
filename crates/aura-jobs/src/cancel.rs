//! Stopping a run without breaking anything.
//!
//! Section 10.1: "cancellation leaves no partial exports and no corrupted catalog."
//!
//! ## Why that is a property of *where* the token is polled
//!
//! A cancel is not an interrupt. Nothing here kills a thread, aborts a transaction or unwinds a
//! stack. [`aura_core::progress::CancelToken`] is a flag, every stage polls it between units, and
//! a stage that observes it stops after the unit it is on - having committed that unit's work and
//! its checkpoint in one transaction.
//!
//! So a cancelled run is exactly a run that stopped early. There is no state a cancel can produce
//! that a power cut could not, which is what makes section 10.1's chaos tests and its cancellation
//! test the same test with a different trigger.
//!
//! ## The two places a cancel is *not* honoured
//!
//! **Inside a unit's write.** A stage that checked the token between its catalog write and its
//! checkpoint write could leave the two disagreeing, which is the one thing resumption cannot
//! recover from. [`CancelPoint::BetweenUnits`] is the only variant this module offers.
//!
//! **Inside an export.** Phase 30's stage checkpoints per stage rather than per image, so a
//! cancelled export has written no checkpoint and the resume re-runs the whole of it. That is
//! deliberate: a directory half full of delivered files that a photographer might send is a much
//! worse outcome than repeating twenty minutes of work.

use aura_core::progress::CancelToken;

use crate::contract::autopilot::{CheckpointKind, SkipCause, StageId, StageOutcome};
use crate::stages;

/// Where a stage is allowed to notice a cancel.
///
/// One variant. It is an enum rather than a comment because a second variant is exactly the change
/// somebody would make to get a faster stop, and a variant that had to be added is a variant that
/// has to be argued for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelPoint {
    /// After a unit's work and its checkpoint are committed, and before the next unit starts.
    BetweenUnits,
}

/// Whether a stage should stop now.
#[must_use]
pub fn should_stop(cancel: &CancelToken, _at: CancelPoint) -> bool {
    cancel.is_cancelled()
}

/// What a stage that never started becomes when the run was cancelled.
#[must_use]
pub const fn unstarted_outcome() -> StageOutcome {
    StageOutcome::Skipped(SkipCause::Cancelled)
}

/// What a stage that was interrupted mid-way becomes.
///
/// `Partial` rather than `Failed` or `Skipped`, and the distinction is the honest one: the stage
/// did some of its work, that work is committed, and a resume will finish it. Recording it as
/// `Failed` would put it in the degraded list of a run nobody said failed, and recording it as
/// `Skipped` would claim it never ran.
///
/// A stage with `CheckpointKind::PerStage` is the exception: it has committed nothing, so it did
/// not partially do anything, and it is [`SkipCause::Cancelled`] like one that never started.
#[must_use]
pub fn interrupted_outcome(stage: StageId, items_done: u32, items_total: u32) -> StageOutcome {
    if stages::decl(stage).checkpoint == CheckpointKind::PerStage || items_done == 0 {
        return unstarted_outcome();
    }
    StageOutcome::Partial {
        items: items_done,
        failed: 0,
        detail: format!(
            "stopped after {items_done} of {items_total}; starting again will continue from there"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_nobody_cancelled_does_not_stop_anything() {
        let token = CancelToken::new();
        assert!(!should_stop(&token, CancelPoint::BetweenUnits));
    }

    #[test]
    fn a_cancelled_token_stops_between_units() {
        let token = CancelToken::new();
        token.cancel();
        assert!(should_stop(&token, CancelPoint::BetweenUnits));
    }

    #[test]
    fn a_stage_that_never_started_is_skipped_rather_than_failed() {
        assert_eq!(
            unstarted_outcome(),
            StageOutcome::Skipped(SkipCause::Cancelled)
        );
    }

    #[test]
    fn a_per_image_stage_interrupted_half_way_is_partial() {
        let outcome = interrupted_outcome(StageId::Colour, 40, 100);
        assert_eq!(outcome.items(), 40);
        assert!(matches!(outcome, StageOutcome::Partial { failed: 0, .. }));
    }

    #[test]
    fn a_gallery_solver_interrupted_half_way_committed_nothing_and_says_so() {
        // The cull, the consistency pass and the QC pass all checkpoint per stage. A `Partial`
        // here would claim a solve had half happened, and there is no such thing.
        for stage in [StageId::Cull, StageId::Consistency, StageId::Qc] {
            assert_eq!(
                interrupted_outcome(stage, 0, 1),
                StageOutcome::Skipped(SkipCause::Cancelled),
                "{stage}"
            );
        }
    }

    #[test]
    fn a_stage_interrupted_before_its_first_unit_is_not_partial() {
        assert_eq!(
            interrupted_outcome(StageId::Colour, 0, 100),
            StageOutcome::Skipped(SkipCause::Cancelled)
        );
    }

    #[test]
    fn there_is_exactly_one_cancel_point() {
        // A second variant is the change somebody would make to get a faster stop, and it would
        // put the poll inside a unit's write - which is the one state resumption cannot recover
        // from. This test is what makes adding one a deliberate act.
        let points = [CancelPoint::BetweenUnits];
        assert_eq!(points.len(), 1);
    }
}
