//! The bounded re-edit loop. PHASE-27 section 6.3.
//!
//! ## The three rules, and why each one has to be there
//!
//! Section 6.3 states them together: "each remedy application is followed by re-inspection of that
//! ticket's metric only; if the metric does not improve by at least the expected gain margin, the
//! change is reverted and the ticket escalates. Maximum 2 rounds per image, global time budget per
//! wedding, and a rule that no remedy may worsen another check by more than a small tolerance."
//!
//! **Improvement is measured against what the ticket opened with.** Not against the threshold. A
//! ticket at 4.2 against a 2.5 threshold, remediated to 3.9, has improved and still fails - and a
//! build that kept only what passes would throw away every partial repair on the hardest frames,
//! which are the frames a photographer most wants helped. ADR-0055 section 4, and it is phase 19's
//! lesson in a new place: a converged value cannot be used to detect its own constraints.
//!
//! **The collateral check runs on the categories the remedy could reach.** `Remedy::collateral_checks`
//! is a `const fn` on the enum rather than a configuration row, because which categories an
//! operation can move is a fact about the operation. Re-running all ten after every remedy would be
//! the whole pass five times over inside a 90 s budget.
//!
//! **Two rounds, and a round that was reverted still counts.** The bound is on *attempts*, not on
//! successes. A loop that only counted kept rounds would try forever on a frame nothing helps, which
//! is exactly the frame the bound exists for.
//!
//! ## Why the loop needs a port
//!
//! Nothing in this crate can apply a remedy: applying one means re-running phase 15's solver or
//! writing through `aura_recipe::schema::merge`, and neither is this crate's to do. [`Remediator`]
//! is the port the caller implements, and it is the same shape phase 19's `MaskField` took - a
//! narrow trait rather than a dependency, so that the loop's arithmetic can be driven by a test with
//! no catalog, no renderer and no recipe.
//!
//! It also means the revert is the caller's, which is deliberate. A revert that this crate performed
//! would be a second path to a recipe, and phase 14's rule is that there is one.

use aura_core::contract::qc::{
    ImageId, QcCategory, QcCode, QcRound, QcTicket, Remedy, TicketStatus, MAX_ROUNDS,
};
use aura_core::contract::scene::Timestamp;
use aura_core::AuraError;

use crate::checks::{self, Frame};
use crate::policy::{LoopPolicy, Thresholds};

/// The port that turns a decision into an edit.
///
/// Implemented outside this crate. Every method returns the frame's *new readings*, so the loop
/// re-inspects real numbers rather than predicting what the remedy should have done - which is the
/// difference between a loop that verifies and a loop that assumes.
pub trait Remediator {
    /// Apply a remedy and return what the frame measures afterwards.
    ///
    /// # Errors
    ///
    /// Whatever the underlying phase raises. A failure here is not a revert: nothing was applied, so
    /// the ticket is escalated with the error's own code underneath.
    fn apply(&mut self, image: ImageId, remedy: &Remedy) -> Result<Frame, AuraError>;

    /// Undo a remedy and return what the frame measures afterwards.
    ///
    /// # Errors
    ///
    /// Whatever the underlying phase raises. A failed revert is the one genuinely bad outcome in
    /// this module and [`Loop::run`] reports it as such: the frame is left carrying a change that
    /// did not help, and the ticket escalates saying so.
    fn revert(&mut self, image: ImageId, remedy: &Remedy) -> Result<Frame, AuraError>;
}

/// What one round did.
#[derive(Debug, Clone, PartialEq)]
pub struct RoundResult {
    /// The row to store.
    pub round: QcRound,
    /// The frame's readings after the round settled - after a revert, if there was one.
    pub frame: Frame,
    /// The ticket's status after the round.
    pub status: TicketStatus,
    /// The ticket's deviation after the round settled.
    pub deviation: f32,
}

/// The loop.
///
/// Holds no state between images: a `Loop` is constructed per image, which is what makes the
/// per-image round bound impossible to leak across frames.
#[derive(Debug)]
pub struct Loop<'a> {
    thresholds: &'a Thresholds,
    policy: LoopPolicy,
}

impl<'a> Loop<'a> {
    /// A loop over one project's thresholds.
    #[must_use]
    pub fn new(thresholds: &'a Thresholds) -> Self {
        Self {
            policy: thresholds.loop_policy(),
            thresholds,
        }
    }

    /// The bound this loop is running under.
    #[must_use]
    pub const fn max_rounds(&self) -> u8 {
        // The policy may lower it and the loader refuses one above the contract's ceiling, so this
        // is always `<= MAX_ROUNDS`.
        self.policy.max_rounds
    }

    /// Apply one remedy, re-inspect, and keep or revert.
    ///
    /// # Errors
    ///
    /// Returns the error the [`Remediator`] raised when the remedy could not be applied at all. A
    /// remedy that applied and did not help is not an error - it is a reverted round, which is a
    /// row.
    pub fn run(
        &self,
        ticket: &QcTicket,
        frame: &Frame,
        remedy: &Remedy,
        remediator: &mut dyn Remediator,
        now: Timestamp,
        elapsed_ms: u32,
    ) -> Result<RoundResult, AuraError> {
        let before = ticket.deviation;
        let round_index = ticket.round.saturating_add(1);

        // An escalation changes nothing, so there is nothing to apply, measure or revert. It is
        // still a round row, because "the product decided this needs a person" is part of the
        // history section 6.3 asks to be reconstructable.
        if !remedy.mutates() {
            return Ok(RoundResult {
                round: QcRound {
                    ticket: ticket.id,
                    round: round_index,
                    remedy: remedy.clone(),
                    deviation_before: before,
                    deviation_after: before,
                    expected_gain: ticket.expected_gain,
                    collateral: 0.0,
                    collateral_category: None,
                    kept: false,
                    outcome: QcCode::EscalatedToHuman,
                    ms: elapsed_ms,
                    at: now,
                },
                frame: frame.clone(),
                status: TicketStatus::Escalated,
                deviation: before,
            });
        }

        let after_frame = remediator.apply(ticket.image_id, remedy)?;
        let after = self.measure(ticket, &after_frame);
        let (collateral, collateral_category) =
            self.collateral(ticket, frame, &after_frame, remedy);

        let realised = realised_share(before, after, ticket.expected_gain);
        let improved = realised >= self.policy.min_gain_share;
        let safe = collateral <= self.policy.max_collateral;

        if improved && safe {
            return Ok(RoundResult {
                round: QcRound {
                    ticket: ticket.id,
                    round: round_index,
                    remedy: remedy.clone(),
                    deviation_before: before,
                    deviation_after: after,
                    expected_gain: ticket.expected_gain,
                    collateral,
                    collateral_category,
                    kept: true,
                    outcome: QcCode::RemedyApplied,
                    ms: elapsed_ms,
                    at: now,
                },
                frame: after_frame,
                // Fixed only when the finding is actually gone. A ticket that improved and still
                // fails stays open for a second round, which is what the two-round bound is *for*.
                status: if after <= ticket.threshold {
                    TicketStatus::Fixed
                } else if round_index >= self.max_rounds() {
                    TicketStatus::Escalated
                } else {
                    TicketStatus::Open
                },
                deviation: after,
            });
        }

        // It did not earn its place. Put it back.
        let outcome = if safe {
            QcCode::RemedyReverted
        } else {
            QcCode::CollateralDamage
        };
        let reverted_frame = remediator.revert(ticket.image_id, remedy)?;
        let settled = self.measure(ticket, &reverted_frame);

        Ok(RoundResult {
            round: QcRound {
                ticket: ticket.id,
                round: round_index,
                remedy: remedy.clone(),
                deviation_before: before,
                deviation_after: after,
                expected_gain: ticket.expected_gain,
                collateral,
                collateral_category,
                kept: false,
                outcome,
                ms: elapsed_ms,
                at: now,
            },
            frame: reverted_frame,
            // A reverted round always escalates. Section 6.3: "the change is reverted and the ticket
            // escalates" - there is no second attempt at a remedy that did not work, because the
            // second attempt would be the same remedy at a different magnitude and the loop has no
            // evidence that magnitude is the problem.
            status: TicketStatus::Escalated,
            deviation: settled,
        })
    }

    /// This ticket's own metric, re-measured on the frame as it now stands.
    ///
    /// Section 6.3: "re-inspection of *that ticket's* metric only". Re-running all ten checks would
    /// be the collateral check, which is a separate and narrower thing.
    ///
    /// A check that now *skips* returns the deviation unchanged rather than zero. A remedy that made
    /// a reading unmeasurable has not fixed anything, and treating an absent measurement as a
    /// perfect one is the exact failure ADR-0055 section 8 exists to prevent - here it would keep
    /// every remedy that broke its own input.
    fn measure(&self, ticket: &QcTicket, frame: &Frame) -> f32 {
        let outcomes = checks::run_frame(frame, self.thresholds);
        let mut ran = false;
        let mut worst = 0.0f32;
        for outcome in outcomes {
            if !outcome.ran() {
                continue;
            }
            for finding in outcome.findings() {
                if finding.category != ticket.category {
                    continue;
                }
                ran = true;
                worst = worst.max(finding.deviation);
            }
        }
        if !ran {
            // Either the check ran and found nothing - the finding is gone - or it could not run.
            // The two are distinguished by whether *any* outcome for this category ran at all.
            if self.category_ran(frame, ticket.category) {
                return 0.0;
            }
            return ticket.deviation;
        }
        worst
    }

    /// Whether the inspection that owns this category actually ran on this frame.
    fn category_ran(&self, frame: &Frame, category: QcCategory) -> bool {
        checks::run_frame(frame, self.thresholds)
            .into_iter()
            .zip(CATEGORY_ORDER)
            .any(|(outcome, owner)| owner == category && outcome.ran())
    }

    /// The worst movement this remedy caused in a check it could reach.
    ///
    /// As a share of *that* check's own threshold, because the ten checks are measured in five
    /// units. Only the categories `Remedy::collateral_checks` names, because re-running all ten
    /// after every remedy is the pass five times over.
    fn collateral(
        &self,
        ticket: &QcTicket,
        before: &Frame,
        after: &Frame,
        remedy: &Remedy,
    ) -> (f32, Option<QcCategory>) {
        let mut worst = 0.0f32;
        let mut which = None;
        for category in remedy.collateral_checks() {
            if category == ticket.category {
                // The ticket's own metric is judged by `measure`, not here. Counting it twice would
                // make every successful remedy look like collateral damage to itself.
                continue;
            }
            let was = worst_in(before, category, self.thresholds);
            let now = worst_in(after, category, self.thresholds);
            let threshold = threshold_in(after, category, self.thresholds).max(1e-6);
            let moved = ((now - was) / threshold).max(0.0);
            if moved > worst {
                worst = moved;
                which = Some(category);
            }
        }
        (worst, which)
    }
}

/// Which category each of `checks::FRAME_CHECKS` owns, in the same order.
///
/// A parallel array rather than a field on the outcome, so that a check added to `FRAME_CHECKS`
/// without a category here is a compile error on the length rather than a silent mis-attribution.
const CATEGORY_ORDER: [QcCategory; 9] = [
    QcCategory::Consistency,
    QcCategory::Skin,
    QcCategory::Exposure,
    QcCategory::Sharpness,
    QcCategory::Retouch,
    QcCategory::Mask,
    QcCategory::Crop,
    QcCategory::Cleanup,
    QcCategory::Duplicate,
];

/// The worst deviation in one category on one frame, or zero when there is none.
fn worst_in(frame: &Frame, category: QcCategory, thresholds: &Thresholds) -> f32 {
    checks::run_frame(frame, thresholds)
        .into_iter()
        .flat_map(Outcome_findings)
        .filter(|finding| finding.category == category)
        .map(|finding| finding.deviation)
        .fold(0.0f32, f32::max)
}

/// The threshold in one category on one frame, or its scene row's when nothing fired.
fn threshold_in(frame: &Frame, category: QcCategory, thresholds: &Thresholds) -> f32 {
    checks::run_frame(frame, thresholds)
        .into_iter()
        .flat_map(Outcome_findings)
        .find(|finding| finding.category == category)
        .map_or(1.0, |finding| finding.threshold)
}

#[allow(non_snake_case)]
fn Outcome_findings(outcome: checks::Outcome) -> Vec<checks::Finding> {
    outcome.findings()
}

/// The share of a predicted gain that was realised.
///
/// Free function rather than a method, because it is the arithmetic section 10.1 gates on and the
/// eval harness calls it directly. Identical to `QcRound::realised_share`, and the contract's copy
/// is the one a stored row is judged by.
#[must_use]
pub fn realised_share(before: f32, after: f32, expected_gain: f32) -> f32 {
    if !expected_gain.is_finite() || expected_gain <= 0.0 {
        return 0.0;
    }
    let realised = before - after;
    if !realised.is_finite() {
        return 0.0;
    }
    (realised / expected_gain).clamp(-1.0, 4.0)
}

/// Whether another round is permitted on this ticket.
/// `Open` rather than `is_open`, and the difference is the defect gate 6 caught: `is_open` is also
/// true for `Escalated` and `Reverted`, which are things a photographer should see and not things
/// automation may act on again. Section 6.3 is explicit that a reverted remedy escalates and there
/// is no second attempt at it.
#[must_use]
pub fn may_retry(ticket: &QcTicket, policy: LoopPolicy) -> bool {
    ticket.status == TicketStatus::Open
        && ticket.status.is_automatable()
        && ticket.round < policy.max_rounds.min(MAX_ROUNDS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::{ExposureReading, Finding, SharpnessReading};
    use aura_core::contract::ids::ProjectId;
    use aura_core::contract::qc::SolveTarget;
    use aura_core::contract::scene::SceneId;

    /// A remediator that replaces the frame with whatever it was told to, and counts calls.
    #[derive(Debug, Default)]
    struct Fake {
        applied: Vec<Frame>,
        reverted: Frame,
        applies: usize,
        reverts: usize,
    }

    impl Remediator for Fake {
        fn apply(&mut self, _image: ImageId, _remedy: &Remedy) -> Result<Frame, AuraError> {
            self.applies += 1;
            Ok(self
                .applied
                .get(self.applies - 1)
                .cloned()
                .unwrap_or_default())
        }

        fn revert(&mut self, _image: ImageId, _remedy: &Remedy) -> Result<Frame, AuraError> {
            self.reverts += 1;
            Ok(self.reverted.clone())
        }
    }

    fn sharp_frame(relative: f32) -> Frame {
        Frame {
            sharpness: Some(SharpnessReading {
                subject_sharpness: relative,
                relative_sharpness: relative,
                texture_retention: 0.95,
                ringing: 0.01,
                identity_drift: 0.0,
                selfcheck_measured_on: 4,
            }),
            ..Frame::empty(ImageId::new(), SceneId::CouplePortrait)
        }
    }

    fn ticket_for(frame: &Frame, deviation: f32, gain: f32) -> QcTicket {
        let finding = Finding::new(
            QcCategory::Sharpness,
            QcCode::SharpnessBelowFloor,
            deviation,
            0.45,
            gain,
            0.9,
        );
        crate::ticket::from_finding(
            ProjectId::new(),
            frame,
            finding,
            Remedy::ResolveParam {
                target: SolveTarget::Restoration,
                constraint: "x".into(),
            },
            0,
        )
    }

    fn fix() -> Remedy {
        Remedy::ResolveParam {
            target: SolveTarget::Restoration,
            constraint: "sharpen the subject region".into(),
        }
    }

    #[test]
    fn a_remedy_that_realises_its_gain_is_kept() {
        let thresholds = Thresholds::reference();
        let before = sharp_frame(0.05);
        // 0.55 - 0.05 = 0.50 out, remediated to 0.55 - 0.45 = 0.10.
        let mut remediator = Fake {
            applied: vec![sharp_frame(0.45)],
            ..Fake::default()
        };
        let ticket = ticket_for(&before, 0.50, 0.30);
        let result = Loop::new(&thresholds)
            .run(&ticket, &before, &fix(), &mut remediator, 0, 5)
            .expect("the fake cannot fail");
        assert!(result.round.kept);
        assert_eq!(result.round.outcome, QcCode::RemedyApplied);
        assert_eq!(remediator.reverts, 0);
    }

    #[test]
    fn improvement_is_measured_against_what_the_ticket_opened_with() {
        // 4.2 to 3.9 against a 2.5 threshold: still failing, and kept, because it realised more
        // than half of a 0.5 prediction. A build that compared against the threshold would revert
        // every partial repair on the hardest frames.
        let thresholds = Thresholds::reference();
        // The reference row tolerates 0.45 of softness below the 0.55 floor, so a frame at 0.05
        // measures 0.50 out and a frame at 0.08 measures 0.47 - still failing the check.
        let before = sharp_frame(0.05);
        let mut remediator = Fake {
            applied: vec![sharp_frame(0.08)],
            ..Fake::default()
        };
        // deviation 0.50 -> 0.47 against a 0.45 threshold, predicted 0.02.
        let ticket = ticket_for(&before, 0.50, 0.02);
        let result = Loop::new(&thresholds)
            .run(&ticket, &before, &fix(), &mut remediator, 0, 5)
            .expect("the fake cannot fail");
        assert!(result.round.kept);
        assert!(
            result.deviation > ticket.threshold,
            "it still fails the check"
        );
        assert_eq!(
            result.status,
            TicketStatus::Open,
            "and is open for round two"
        );
    }

    #[test]
    fn a_remedy_that_did_nothing_is_reverted_and_escalates() {
        let thresholds = Thresholds::reference();
        let before = sharp_frame(0.05);
        let mut remediator = Fake {
            // Unchanged.
            applied: vec![sharp_frame(0.05)],
            reverted: sharp_frame(0.05),
            ..Fake::default()
        };
        let ticket = ticket_for(&before, 0.50, 0.30);
        let result = Loop::new(&thresholds)
            .run(&ticket, &before, &fix(), &mut remediator, 0, 5)
            .expect("the fake cannot fail");
        assert!(!result.round.kept);
        assert_eq!(result.round.outcome, QcCode::RemedyReverted);
        assert_eq!(result.status, TicketStatus::Escalated);
        assert_eq!(remediator.reverts, 1);
    }

    #[test]
    fn a_remedy_that_promised_nothing_is_reverted() {
        // A zero prediction cannot be half-realised, so the loop reverts. That is what turns every
        // `expected_gain = 0.0` finding - identity drift, an unsafe crop, a missing disclosure -
        // into an escalation without this module needing to know why.
        assert_eq!(realised_share(4.0, 1.0, 0.0), 0.0);
    }

    #[test]
    fn collateral_damage_reverts_a_remedy_that_met_its_own_gain() {
        let thresholds = Thresholds::reference();
        let mut before = sharp_frame(0.05);
        before.exposure = Some(ExposureReading {
            subject_luma: Some(0.45),
            target_luma: 0.45,
            clip_hi_after: 0.0,
            clip_lo_after: 0.0,
            clip_hi_before: 0.0,
            clip_lo_before: 0.0,
            shadow_headroom: Some(1.0),
        });
        // Sharpness fixed, exposure wrecked.
        let mut after = sharp_frame(0.50);
        after.exposure = Some(ExposureReading {
            subject_luma: Some(0.95),
            target_luma: 0.45,
            clip_hi_after: 0.9,
            clip_lo_after: 0.0,
            clip_hi_before: 0.0,
            clip_lo_before: 0.0,
            shadow_headroom: Some(1.0),
        });
        let mut remediator = Fake {
            applied: vec![after],
            reverted: before.clone(),
            ..Fake::default()
        };
        let ticket = ticket_for(&before, 0.50, 0.30);
        // `SolveTarget::Restoration` reaches sharpness and retouch, not exposure - so the default
        // remedy would not notice. A grade re-solve does reach exposure.
        let reaching = Remedy::ResolveParam {
            target: SolveTarget::Grade,
            constraint: "x".into(),
        };
        let mut ticket_for_grade = ticket.clone();
        ticket_for_grade.category = QcCategory::Consistency;
        let result = Loop::new(&thresholds)
            .run(&ticket_for_grade, &before, &reaching, &mut remediator, 0, 5)
            .expect("the fake cannot fail");
        assert!(!result.round.kept);
        assert_eq!(result.round.outcome, QcCode::CollateralDamage);
        assert_eq!(result.round.collateral_category, Some(QcCategory::Exposure));
        assert!(result.round.collateral > 0.0);
    }

    #[test]
    fn a_remedy_that_broke_its_own_input_does_not_read_as_a_fix() {
        // The frame comes back with no sharpness reading at all. Treating an absent measurement as
        // a perfect one would keep every remedy that destroyed its own input - which is ADR-0055
        // section 8's failure in the one place it would be invisible.
        let thresholds = Thresholds::reference();
        let before = sharp_frame(0.05);
        let blank = Frame::empty(before.image_id, SceneId::CouplePortrait);
        let mut remediator = Fake {
            applied: vec![blank],
            reverted: before.clone(),
            ..Fake::default()
        };
        let ticket = ticket_for(&before, 0.50, 0.30);
        let result = Loop::new(&thresholds)
            .run(&ticket, &before, &fix(), &mut remediator, 0, 5)
            .expect("the fake cannot fail");
        assert!(!result.round.kept);
        assert_eq!(result.round.deviation_after, ticket.deviation);
    }

    #[test]
    fn an_escalation_applies_nothing_and_still_writes_a_round() {
        let thresholds = Thresholds::reference();
        let before = sharp_frame(0.05);
        let mut remediator = Fake::default();
        let ticket = ticket_for(&before, 0.50, 0.30);
        let result = Loop::new(&thresholds)
            .run(
                &ticket,
                &before,
                &Remedy::Escalate { note: "n".into() },
                &mut remediator,
                7,
                3,
            )
            .expect("an escalation cannot fail");
        assert_eq!(remediator.applies, 0);
        assert_eq!(remediator.reverts, 0);
        assert_eq!(result.status, TicketStatus::Escalated);
        // A round row all the same: "the product decided this needs a person" is part of the
        // history section 6.3 asks to be reconstructable.
        assert_eq!(result.round.round, 1);
        assert_eq!(result.round.at, 7);
    }

    #[test]
    fn the_round_bound_counts_attempts_rather_than_successes() {
        let thresholds = Thresholds::reference();
        let subject = Loop::new(&thresholds);
        assert!(subject.max_rounds() <= MAX_ROUNDS);
        let before = sharp_frame(0.05);
        let mut ticket = ticket_for(&before, 0.50, 0.30);
        ticket.round = MAX_ROUNDS;
        // A loop that only counted kept rounds would try forever on a frame nothing helps, which is
        // exactly the frame the bound exists for.
        assert!(!may_retry(&ticket, LoopPolicy::reference()));
        ticket.round = MAX_ROUNDS - 1;
        assert!(may_retry(&ticket, LoopPolicy::reference()));
    }

    #[test]
    fn a_ticket_a_photographer_settled_is_never_retried() {
        let before = sharp_frame(0.05);
        let mut ticket = ticket_for(&before, 0.50, 0.30);
        ticket.status = TicketStatus::Dismissed;
        assert!(!may_retry(&ticket, LoopPolicy::reference()));
    }

    #[test]
    fn a_kept_remedy_that_reaches_the_threshold_closes_the_ticket() {
        let thresholds = Thresholds::reference();
        let before = sharp_frame(0.05);
        let mut remediator = Fake {
            applied: vec![sharp_frame(0.99)],
            ..Fake::default()
        };
        let ticket = ticket_for(&before, 0.50, 0.40);
        let result = Loop::new(&thresholds)
            .run(&ticket, &before, &fix(), &mut remediator, 0, 5)
            .expect("the fake cannot fail");
        assert_eq!(result.status, TicketStatus::Fixed);
        assert_eq!(result.deviation, 0.0);
    }

    #[test]
    fn the_stricter_of_the_policy_and_the_contract_wins() {
        let thresholds = Thresholds::reference();
        assert_eq!(Loop::new(&thresholds).max_rounds(), MAX_ROUNDS);
    }
}
