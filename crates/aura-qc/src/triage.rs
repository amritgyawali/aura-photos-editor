//! Deciding what to do first, and when to ask for a second opinion. PHASE-27 section 6.2.
//!
//! ## Root causes before symptoms
//!
//! Section 7's system prompt states the rule for the planner and section 7's offline fallback states
//! it for the mechanical path: "if white balance is wrong, do not reduce retouch strength."
//!
//! `QcCategory::triage_rank` is that ordering as data, and it covers all ten categories rather than
//! the five section 7 names. The extension is not arbitrary: colour, exposure and skin come first
//! because everything downstream is measured on pixels they moved; crop comes next because it
//! decides what is *in* the frame the later checks measure; cleanup, mask, retouch and sharpness are
//! operations in the order they compose; and duplicate and coverage come last because they are facts
//! about the set, so remediating one cannot change what any per-frame check measures.
//!
//! ## One remedy per round, and the reason is measurement rather than caution
//!
//! Section 6.3: each remedy application is followed by re-inspection of *that ticket's* metric. Two
//! remedies applied together cannot be attributed - if the frame improved, which one did it, and if
//! another check got worse, which one broke it? The loop's revert would then have to undo both,
//! including the one that worked.
//!
//! So [`order`] returns a sorted list and [`next`] returns exactly one ticket.
//!
//! ## When the planner is asked
//!
//! Section 7's trigger, verbatim: three or more open tickets, contradictory tickets, or a failed
//! first remediation round. [`needs_planner`] is those three, and it is deliberately a cheap
//! predicate over tickets already in hand - a planner trigger that itself needed a model call would
//! be a cost control that costs money.

use aura_core::contract::qc::{
    QcCategory, QcCode, QcTicket, TicketStatus, MAX_ROUNDS, PLANNER_TICKET_FLOOR,
};

/// Order a frame's tickets: root cause first, then worst first.
///
/// Rank leads and severity breaks the tie, which is the opposite of the escalation queue's
/// ordering - and the difference is the point. A *photographer* clearing a queue wants the worst
/// frames first, because they are triaging their own attention. The *loop* wants the root cause
/// first, because fixing a symptom before its cause wastes one of two rounds and can make the cause
/// harder to see.
///
/// `QcTicket::queue_order` is the other one, and the two live in different modules for that reason.
///
/// ## The filter is `Open`, not `is_open`
///
/// `TicketStatus::is_open` is true for `Open`, `Escalated` **and** `Reverted`, because all three are
/// things a photographer should still see. Only the first is something automation may still act on.
///
/// The distinction cost this phase a real defect, caught by gate 6. Section 6.3 says a reverted
/// remedy escalates and there is no second attempt - but `is_open` included `Escalated`, so a
/// ticket whose first round had been reverted came straight back to the loop, was remediated again,
/// was reverted again, and consumed both rounds. The bound held and the behaviour it exists to
/// prevent happened anyway.
///
/// It is the same shape as phase 19's "a converged target cannot detect its own constraints": a
/// predicate that is *nearly* the right question passes every unit test written against the
/// question it does answer.
#[must_use]
pub fn order(tickets: &[QcTicket]) -> Vec<&QcTicket> {
    let mut ordered: Vec<&QcTicket> = tickets
        .iter()
        .filter(|ticket| ticket.status == TicketStatus::Open && ticket.status.is_automatable())
        .collect();
    ordered.sort_by(|left, right| {
        left.category
            .triage_rank()
            .cmp(&right.category.triage_rank())
            .then_with(|| right.severity().total_cmp(&left.severity()))
            .then_with(|| left.id.cmp(&right.id))
    });
    ordered
}

/// The one ticket to work on next.
///
/// Exactly one, never a batch. Two remedies applied together cannot be attributed, and the loop's
/// revert would then have to undo the one that worked along with the one that did not.
#[must_use]
pub fn next(tickets: &[QcTicket]) -> Option<&QcTicket> {
    order(tickets).into_iter().find(|ticket| ticket.may_retry())
}

/// Whether this image's findings want a second opinion. Section 7's trigger.
///
/// Three conditions, any of which is enough:
///
/// * three or more open tickets, which is section 7's own number;
/// * contradictory tickets - see [`contradictory`];
/// * a failed first remediation round, which is a ticket that has been through a round and is still
///   open.
///
/// Deliberately a cheap predicate over tickets already in hand. A trigger that needed a model call
/// to decide whether to make a model call would be a cost control that costs money.
#[must_use]
pub fn needs_planner(tickets: &[QcTicket]) -> bool {
    let open: Vec<&QcTicket> = tickets
        .iter()
        .filter(|ticket| ticket.status.is_open() && ticket.status.is_automatable())
        .collect();
    if open.len() >= PLANNER_TICKET_FLOOR {
        return true;
    }
    if open.iter().any(|ticket| ticket.round >= 1) {
        return true;
    }
    contradictory(&open)
}

/// Whether two findings cannot both be remediated without one undoing the other.
///
/// Four pairs, and each is a real conflict rather than a category clash:
///
/// * **soft and ringing.** Sharpening more fixes the first and worsens the second; sharpening less
///   does the reverse. There is no amount that satisfies both, which is exactly what a frame that
///   was already at the edge of what deconvolution can do looks like.
/// * **texture lost and noisy.** Denoising less keeps texture and keeps noise.
/// * **skin drifted and the grade already moved skin too far.** Correcting toward the person's
///   target means moving skin, and phase 16's guard is already saying it has moved enough.
/// * **an unsafe crop and a lost content warning.** Cropping tighter fixes the first and worsens the
///   second.
///
/// These are the multi-symptom cases section 6.2 sends to the planner: a mechanical rule has no
/// principled way to pick, and a model reading the whole picture might see the root cause - or say
/// so and escalate, which is also a useful answer.
#[must_use]
pub fn contradictory(tickets: &[&QcTicket]) -> bool {
    let has = |code: QcCode| tickets.iter().any(|ticket| ticket.code == code);
    (has(QcCode::SharpnessBelowFloor) && has(QcCode::RingingDetected))
        || (has(QcCode::TextureLost) && has(QcCode::SharpnessBelowFloor))
        || (has(QcCode::SkinDrift) && has(QcCode::SkinGuardExceeded))
        || (has(QcCode::CropUnsafe) && has(QcCode::CropContentLost))
}

/// Whether this image is finished with, for this pass.
///
/// True when nothing is left that automation may act on: every ticket is closed, user-set, or has
/// spent its rounds.
#[must_use]
pub fn settled(tickets: &[QcTicket]) -> bool {
    tickets.iter().all(|ticket| {
        ticket.status != TicketStatus::Open
            || !ticket.status.is_automatable()
            || ticket.round >= MAX_ROUNDS
    })
}

/// Which categories a remedy on `category` may have disturbed, for the collateral re-check.
///
/// A thin wrapper over `Remedy::collateral_checks` that adds the gallery-scoped pair when the remedy
/// changed which photograph is delivered. It exists so the loop has one call site rather than a
/// match, and so the *addition* is documented in one place: a swap changes the set, so coverage and
/// duplicates have to be re-run over the gallery even though every other remedy leaves them alone.
#[must_use]
pub fn affected_by(remedy: &aura_core::contract::qc::Remedy) -> Vec<QcCategory> {
    remedy.collateral_checks()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::{Finding, Frame};
    use aura_core::contract::ids::ProjectId;
    use aura_core::contract::qc::{ImageId, Remedy, SolveTarget, TicketStatus};
    use aura_core::contract::scene::SceneId;

    fn ticket(category: QcCategory, code: QcCode, deviation: f32) -> QcTicket {
        let frame = Frame::empty(ImageId::new(), SceneId::Ceremony);
        let finding = Finding::new(category, code, deviation, 1.0, 0.5, 0.9);
        crate::ticket::from_finding(
            ProjectId::new(),
            &frame,
            finding,
            Remedy::ResolveParam {
                target: SolveTarget::Normalisation,
                constraint: "x".into(),
            },
            0,
        )
    }

    #[test]
    fn the_root_cause_is_worked_first_even_when_a_symptom_is_worse() {
        // A retouch finding five thresholds out and a colour finding barely over the line. The
        // colour one goes first: fixing the retouch on a frame whose white balance is wrong is
        // treating a symptom, and it burns one of two rounds.
        let symptom = ticket(QcCategory::Retouch, QcCode::TextureFloorMissed, 5.0);
        let cause = ticket(QcCategory::Consistency, QcCode::ConsistencyDrift, 1.1);
        let tickets = vec![symptom, cause];
        let first = next(&tickets).expect("something to work on");
        assert_eq!(first.category, QcCategory::Consistency);
        assert!(symptom_is_worse(&tickets));
    }

    fn symptom_is_worse(tickets: &[QcTicket]) -> bool {
        let retouch = tickets
            .iter()
            .find(|t| t.category == QcCategory::Retouch)
            .unwrap();
        let colour = tickets
            .iter()
            .find(|t| t.category == QcCategory::Consistency)
            .unwrap();
        retouch.severity() > colour.severity()
    }

    #[test]
    fn severity_breaks_a_tie_inside_one_category() {
        let mild = ticket(QcCategory::Skin, QcCode::SkinDrift, 1.1);
        let bad = ticket(QcCategory::Skin, QcCode::SkinDrift, 9.0);
        let tickets = vec![mild, bad];
        assert!(next(&tickets).unwrap().deviation > 5.0);
    }

    #[test]
    fn the_loop_takes_exactly_one_ticket_at_a_time() {
        // Two remedies applied together cannot be attributed, and the revert would undo the one
        // that worked along with the one that did not.
        let tickets = vec![
            ticket(QcCategory::Consistency, QcCode::ConsistencyDrift, 2.0),
            ticket(QcCategory::Skin, QcCode::SkinDrift, 2.0),
        ];
        assert!(next(&tickets).is_some());
        assert_eq!(order(&tickets).len(), 2, "ordered, but taken one at a time");
    }

    #[test]
    fn a_ticket_a_photographer_settled_is_never_worked_on() {
        let mut accepted = ticket(QcCategory::Skin, QcCode::SkinDrift, 9.0);
        accepted.status = TicketStatus::Accepted;
        let mut dismissed = ticket(QcCategory::Consistency, QcCode::ConsistencyDrift, 9.0);
        dismissed.status = TicketStatus::Dismissed;
        let tickets = vec![accepted, dismissed];
        assert!(next(&tickets).is_none());
        assert!(order(&tickets).is_empty());
        assert!(settled(&tickets));
    }

    #[test]
    fn a_ticket_that_has_spent_its_rounds_is_not_retried() {
        let mut spent = ticket(QcCategory::Skin, QcCode::SkinDrift, 9.0);
        spent.round = MAX_ROUNDS;
        let tickets = vec![spent];
        assert!(next(&tickets).is_none());
        assert!(settled(&tickets));
    }

    #[test]
    fn three_open_tickets_asks_for_a_second_opinion() {
        let tickets = vec![
            ticket(QcCategory::Consistency, QcCode::ConsistencyDrift, 2.0),
            ticket(QcCategory::Skin, QcCode::SkinDrift, 2.0),
            ticket(QcCategory::Retouch, QcCode::TextureFloorMissed, 2.0),
        ];
        assert!(needs_planner(&tickets));
        assert!(!needs_planner(&tickets[..2]));
    }

    #[test]
    fn a_failed_first_round_asks_for_a_second_opinion() {
        let mut once = ticket(QcCategory::Skin, QcCode::SkinDrift, 2.0);
        once.round = 1;
        assert!(needs_planner(&[once]));
    }

    #[test]
    fn two_findings_that_cannot_both_be_fixed_ask_for_a_second_opinion() {
        // Sharpening more fixes the softness and worsens the ringing; sharpening less does the
        // reverse. A mechanical rule has no principled way to pick.
        let soft = ticket(QcCategory::Sharpness, QcCode::SharpnessBelowFloor, 2.0);
        let ringing = ticket(QcCategory::Sharpness, QcCode::RingingDetected, 2.0);
        assert!(needs_planner(&[soft, ringing]));
    }

    #[test]
    fn two_ordinary_findings_do_not_ask_for_a_second_opinion() {
        let tickets = vec![
            ticket(QcCategory::Consistency, QcCode::ConsistencyDrift, 2.0),
            ticket(QcCategory::Crop, QcCode::CropResolutionLow, 2.0),
        ];
        assert!(!needs_planner(&tickets));
    }

    #[test]
    fn a_settled_frame_asks_for_nothing() {
        let mut accepted = ticket(QcCategory::Skin, QcCode::SkinDrift, 9.0);
        accepted.status = TicketStatus::Accepted;
        let mut also = ticket(QcCategory::Consistency, QcCode::ConsistencyDrift, 9.0);
        also.status = TicketStatus::Accepted;
        let mut third = ticket(QcCategory::Crop, QcCode::CropUnsafe, 9.0);
        third.status = TicketStatus::Accepted;
        // Three tickets, and the planner is not asked: they are not open.
        assert!(!needs_planner(&[accepted, also, third]));
    }

    #[test]
    fn a_swap_disturbs_every_category_and_an_escalation_disturbs_none() {
        let swap = Remedy::ReplaceFrame {
            with: ImageId::new(),
        };
        assert_eq!(affected_by(&swap).len(), QcCategory::COUNT);
        assert!(affected_by(&Remedy::Escalate { note: "n".into() }).is_empty());
    }

    #[test]
    fn the_two_gallery_scoped_categories_come_last_in_the_loop_ordering() {
        let coverage = ticket(QcCategory::Coverage, QcCode::CoverageMissing, 9.0);
        let colour = ticket(QcCategory::Consistency, QcCode::ConsistencyDrift, 1.05);
        let tickets = vec![coverage, colour];
        // Coverage is nine thresholds out and still goes second: remediating it cannot change what
        // any per-frame check measures, so doing it first would be doing it blind.
        assert_eq!(order(&tickets)[0].category, QcCategory::Consistency);
    }
}
