//! Turning a measurement into something a photographer sees. PHASE-27 section 5.
//!
//! ## Why this is a separate module from the checks
//!
//! A [`Finding`] is a number. A [`QcTicket`] is a number plus an identity, a clock reading, an
//! autonomy band, a remedy and a status - and every one of those four is a decision rather than a
//! measurement.
//!
//! Keeping them apart is what lets the ten checks be pure functions with no id generator, no clock
//! and no policy, which is what makes them testable with literals and runnable in parallel across a
//! gallery. It is also where the two versions get stamped on, and where invariant 2 is enforced
//! before a row can reach the store.
//!
//! ## The autonomy band is assigned here and overwritten by the ledger
//!
//! Phase 13's rule, inherited exactly: `Explain::record` **overwrites** whatever band a caller
//! supplies, because a deciding phase that could set its own band is a deciding phase that could
//! grant itself permission to act. What this module computes is a *proposal*, and the ledger has the
//! last word.
//!
//! The proposal itself is the conjunction of three things and none of them alone is enough: the
//! confidence has to clear the remedy's own floor, the frame must not be one a photographer edited
//! by hand, and a remedy that changes what is delivered rather than how it looks needs more than one
//! that does not.

use aura_core::contract::ids::ProjectId;
use aura_core::contract::ids::TicketId;
use aura_core::contract::ledger::Autonomy;
use aura_core::contract::qc::{QcCode, QcReason, QcTicket, Remedy, TicketStatus};
use aura_core::contract::scene::Timestamp;

use crate::checks::{Finding, Frame};
use crate::policy::THRESHOLDS_VER;
use crate::ANALYSIS_VER;

/// Assign an id, a clock reading, a remedy and a band to one measurement.
///
/// `now` is passed in rather than read, because reading the system clock is banned in library
/// code, and because a pass that reads the clock per ticket is one whose two runs over the same
/// catalog produce different rows. Determinism, invariant 4.
#[must_use]
pub fn from_finding(
    project: ProjectId,
    frame: &Frame,
    finding: Finding,
    remedy: Remedy,
    now: Timestamp,
) -> QcTicket {
    let autonomy = band_for(&finding, &remedy, frame);
    let mut reasons = Vec::with_capacity(1 + finding.extra_reasons.len());
    reasons.push(QcReason {
        code: finding.code,
        weight: 1.0,
        evidence: finding.evidence.clone(),
    });
    for (code, weight) in &finding.extra_reasons {
        if reasons.len() >= QcTicket::MAX_REASONS {
            break;
        }
        reasons.push(QcReason::new(*code, *weight));
    }
    if frame.user_edited && reasons.len() < QcTicket::MAX_REASONS {
        // The finding stands - a photographer is entitled to know their own frame sits outside its
        // group - and the reason set says why nothing will be done about it.
        reasons.push(QcReason::new(QcCode::UserEdited, -1.0));
    }

    QcTicket {
        id: TicketId::new(),
        project,
        image_id: frame.image_id,
        category: finding.category,
        code: finding.code,
        deviation: finding.deviation,
        threshold: finding.threshold,
        evidence: finding.evidence,
        identity: finding.identity,
        remedy,
        expected_gain: finding.expected_gain,
        confidence: finding.confidence.clamp(0.0, 1.0),
        autonomy,
        reasons,
        round: 0,
        status: TicketStatus::Open,
        outcome_code: None,
        scene: frame.scene,
        created_at: now,
        thresholds_ver: THRESHOLDS_VER,
        analysis_ver: ANALYSIS_VER,
    }
}

/// What the product may do about this finding, before the ledger has its say.
///
/// Three conditions, all of which must hold for anything to happen unattended:
///
/// * the confidence clears the remedy's own floor - 0.60 for a parameter fix, 0.85 for a swap;
/// * the frame is not one a photographer set by hand;
/// * the remedy actually changes something, because an escalation is not an action.
///
/// A frame a photographer edited is [`Autonomy::Suggest`] rather than [`Autonomy::RequireReview`],
/// and the difference matters: `RequireReview` says the product is not sure, and `Suggest` says the
/// product is sure and is not allowed to act. Collapsing them would tell a photographer their own
/// deliberate edit was a low-confidence guess.
#[must_use]
pub fn band_for(finding: &Finding, remedy: &Remedy, frame: &Frame) -> Autonomy {
    if !remedy.mutates() {
        // Escalation is what happens when nothing mechanical will help.
        return Autonomy::RequireReview;
    }
    if frame.user_edited {
        return Autonomy::Suggest;
    }
    if finding.confidence < remedy.confidence_floor() {
        return Autonomy::Suggest;
    }
    match remedy {
        // A swap changes which photograph a client receives. Even at high confidence it is
        // `Auto` rather than `AutoZeroTouch`: phase 28 reads the distinction, and a frame nobody
        // chose reaching a delivery with nobody in the loop is the outcome section 6.4 exists to
        // make deliberate.
        Remedy::ReplaceFrame { .. } => Autonomy::Auto,
        _ if finding.confidence >= 0.90 => Autonomy::AutoZeroTouch,
        _ => Autonomy::Auto,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::Finding;
    use aura_core::contract::qc::{Evidence, ImageId, QcCategory, SolveTarget};
    use aura_core::contract::scene::SceneId;

    fn finding(confidence: f32) -> Finding {
        Finding::new(
            QcCategory::Skin,
            QcCode::SkinDrift,
            4.2,
            2.5,
            1.2,
            confidence,
        )
    }

    fn frame() -> Frame {
        Frame::empty(ImageId::new(), SceneId::Ceremony)
    }

    fn fix() -> Remedy {
        Remedy::ResolveParam {
            target: SolveTarget::Normalisation,
            constraint: "hold the exposure".into(),
        }
    }

    #[test]
    fn a_ticket_is_always_well_formed_and_carries_both_versions() {
        let ticket = from_finding(
            ProjectId::new(),
            &frame(),
            finding(0.8),
            fix(),
            1_700_000_000,
        );
        assert!(ticket.is_well_formed());
        assert_eq!(ticket.analysis_ver, ANALYSIS_VER);
        assert_eq!(ticket.thresholds_ver, THRESHOLDS_VER);
        assert_eq!(ticket.status, TicketStatus::Open);
        assert_eq!(ticket.outcome_code, None);
        assert_eq!(ticket.round, 0);
    }

    #[test]
    fn the_finding_code_is_always_the_first_reason() {
        let ticket = from_finding(ProjectId::new(), &frame(), finding(0.8), fix(), 0);
        assert_eq!(ticket.reasons[0].code, QcCode::SkinDrift);
        assert_eq!(ticket.reasons[0].weight, 1.0);
    }

    #[test]
    fn the_evidence_travels_from_the_finding_to_the_first_reason() {
        let anchors = vec![ImageId::new(), ImageId::new()];
        let with_evidence = finding(0.8).with_evidence(Evidence::Anchors(anchors.clone()));
        let ticket = from_finding(ProjectId::new(), &frame(), with_evidence, fix(), 0);
        assert!(matches!(ticket.evidence, Evidence::Anchors(ref a) if a.len() == 2));
        assert!(matches!(ticket.reasons[0].evidence, Evidence::Anchors(_)));
    }

    #[test]
    fn reasons_are_capped_at_the_contracts_maximum() {
        let mut noisy = finding(0.8);
        for _ in 0..10 {
            noisy = noisy.because(QcCode::EscalatedToHuman, 0.1);
        }
        let ticket = from_finding(ProjectId::new(), &frame(), noisy, fix(), 0);
        assert!(ticket.reasons.len() <= QcTicket::MAX_REASONS);
        assert!(ticket.is_well_formed());
    }

    #[test]
    fn a_confident_parameter_fix_may_run_unattended() {
        let ticket = from_finding(ProjectId::new(), &frame(), finding(0.95), fix(), 0);
        assert_eq!(ticket.autonomy, Autonomy::AutoZeroTouch);
        assert!(ticket.may_act_unattended());
    }

    #[test]
    fn a_finding_below_the_remedys_floor_only_suggests() {
        let ticket = from_finding(ProjectId::new(), &frame(), finding(0.4), fix(), 0);
        assert_eq!(ticket.autonomy, Autonomy::Suggest);
        assert!(!ticket.may_act_unattended());
    }

    #[test]
    fn a_replacement_needs_more_confidence_than_a_parameter_fix_for_the_same_band() {
        let swap = Remedy::ReplaceFrame {
            with: ImageId::new(),
        };
        // 0.80 clears a parameter fix's floor and not a swap's.
        let as_fix = from_finding(ProjectId::new(), &frame(), finding(0.80), fix(), 0);
        let as_swap = from_finding(ProjectId::new(), &frame(), finding(0.80), swap, 0);
        assert!(as_fix.may_act_unattended());
        assert!(!as_swap.may_act_unattended());
    }

    #[test]
    fn a_replacement_is_never_zero_touch_however_confident_it_is() {
        let swap = Remedy::ReplaceFrame {
            with: ImageId::new(),
        };
        let ticket = from_finding(ProjectId::new(), &frame(), finding(0.99), swap, 0);
        // A frame nobody chose reaching a delivery with nobody in the loop is the outcome section
        // 6.4 exists to make deliberate.
        assert_eq!(ticket.autonomy, Autonomy::Auto);
        assert_ne!(ticket.autonomy, Autonomy::AutoZeroTouch);
    }

    #[test]
    fn a_frame_the_photographer_edited_is_suggest_and_says_so() {
        let mut edited = frame();
        edited.user_edited = true;
        let ticket = from_finding(ProjectId::new(), &edited, finding(0.99), fix(), 0);
        // `Suggest` rather than `RequireReview`: the product is sure and is not allowed to act,
        // which is a different statement from the product being unsure.
        assert_eq!(ticket.autonomy, Autonomy::Suggest);
        assert!(ticket
            .reasons
            .iter()
            .any(|reason| reason.code == QcCode::UserEdited));
    }

    #[test]
    fn an_escalation_always_requires_review() {
        let escalate = Remedy::Escalate {
            note: "look at this".into(),
        };
        let ticket = from_finding(ProjectId::new(), &frame(), finding(1.0), escalate, 0);
        assert_eq!(ticket.autonomy, Autonomy::RequireReview);
    }

    #[test]
    fn the_clock_is_passed_in_so_two_passes_over_one_catalog_agree() {
        // Invariant 4. `SystemTime::now` is banned in library code, and a pass that read the clock
        // per ticket would produce different rows on every run over identical inputs.
        let a = from_finding(ProjectId::new(), &frame(), finding(0.8), fix(), 42);
        let b = from_finding(ProjectId::new(), &frame(), finding(0.8), fix(), 42);
        assert_eq!(a.created_at, b.created_at);
    }
}
