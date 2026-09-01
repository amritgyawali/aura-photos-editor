//! The escalation queue. PHASE-27 section 2.1.
//!
//! ## Two orderings, and they are deliberately different
//!
//! [`crate::triage::order`] sorts by **root cause first**, because the loop fixing a symptom before
//! its cause wastes one of two rounds.
//!
//! This module sorts by **severity first**, because a photographer clearing a queue is triaging
//! their own attention and wants the worst frames at the top. `QcTicket::queue_order` is that
//! ordering and it lives on the contract, so the SQL view, the IPC surface and this module all agree
//! about it.
//!
//! Having two is not an inconsistency: the loop and the person are answering different questions.
//! What would be an inconsistency is one of them silently using the other's, which is why the two
//! functions live in different modules with this note in both.
//!
//! ## Grouping is what makes forty tickets clearable in minutes
//!
//! Section 2.1: "grouped by category so a photographer can clear 40 tickets in minutes". The reason
//! grouping works is that judgement is expensive and *re-judgement* is cheap: deciding whether AURA
//! is right about skin drift takes a minute the first time and five seconds the twentieth, because
//! the question is the same one. A queue interleaved by category makes every ticket the first one.
//!
//! [`group`] is that, and [`bulk`] is the action it enables.

use std::collections::BTreeMap;

use aura_core::contract::qc::{QcCategory, QcOverride, QcTicket, TicketStatus};

/// One category's tickets, worst first.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    /// Which inspection.
    pub category: QcCategory,
    /// The tickets, worst first.
    pub tickets: Vec<QcTicket>,
    /// The worst severity in the group, for ordering the groups themselves.
    pub worst: f32,
}

/// Every open ticket, worst first, ungrouped.
///
/// What the IPC surface returns and what `v_qc_queue` orders by. Closed and photographer-settled
/// tickets are excluded: a queue containing things somebody has already decided about is a queue
/// nobody trusts.
#[must_use]
pub fn open(tickets: &[QcTicket], limit: usize) -> Vec<QcTicket> {
    let mut ordered: Vec<QcTicket> = tickets
        .iter()
        .filter(|ticket| ticket.status.is_open())
        .cloned()
        .collect();
    ordered.sort_by(QcTicket::queue_order);
    ordered.truncate(limit);
    ordered
}

/// Open tickets grouped by category, worst group first.
///
/// Groups are ordered by their worst member rather than by their size, because a category with one
/// unsafe crop is more urgent than one with forty marginal colour drifts - and a photographer
/// deciding where to spend twenty minutes needs the first thing they open to be the worst thing
/// there is.
#[must_use]
pub fn group(tickets: &[QcTicket]) -> Vec<Group> {
    let mut buckets: BTreeMap<QcCategory, Vec<QcTicket>> = BTreeMap::new();
    for ticket in tickets.iter().filter(|t| t.status.is_open()) {
        buckets
            .entry(ticket.category)
            .or_default()
            .push(ticket.clone());
    }
    let mut groups: Vec<Group> = buckets
        .into_iter()
        .map(|(category, mut tickets)| {
            tickets.sort_by(QcTicket::queue_order);
            let worst = tickets.first().map_or(0.0, QcTicket::severity);
            Group {
                category,
                tickets,
                worst,
            }
        })
        .collect();
    groups.sort_by(|left, right| {
        right
            .worst
            .total_cmp(&left.worst)
            .then_with(|| left.category.cmp(&right.category))
    });
    groups
}

/// One category's tickets, worst first.
#[must_use]
pub fn in_category(tickets: &[QcTicket], category: QcCategory, limit: usize) -> Vec<QcTicket> {
    let mut ordered: Vec<QcTicket> = tickets
        .iter()
        .filter(|ticket| ticket.status.is_open() && ticket.category == category)
        .cloned()
        .collect();
    ordered.sort_by(QcTicket::queue_order);
    ordered.truncate(limit);
    ordered
}

/// The overrides for a bulk accept or dismiss.
///
/// Section 9 gives MFE "bulk accept/reject". This builds the overrides; `QcService::decide` applies
/// them one at a time, which is deliberate - a bulk write that skipped the service would skip
/// `QcOverride::is_valid` and migration 27's trigger with it.
///
/// **`apply_remedy` is false on every override this produces.** A photographer agreeing that forty
/// findings are real is not the same as instructing AURA to act on forty frames unattended, and a
/// bulk action that did both would be the single easiest way to damage a gallery in this product.
/// The per-ticket panel is where a remedy is authorised.
#[must_use]
pub fn bulk(tickets: &[QcTicket], status: TicketStatus) -> Vec<QcOverride> {
    if !status.is_user_set() {
        // Automation owns `fixed`, `reverted`, `escalated` and `open`. A bulk action that could set
        // one of them would let somebody record a measurement they had not made.
        return Vec::new();
    }
    tickets
        .iter()
        .filter(|ticket| ticket.status.is_open())
        .map(|ticket| QcOverride {
            ticket: ticket.id,
            status,
            apply_remedy: false,
            note: None,
        })
        .collect()
}

/// How many tickets a photographer still has to look at, per category.
#[must_use]
pub fn outstanding(tickets: &[QcTicket]) -> [u32; QcCategory::COUNT] {
    let mut counts = [0u32; QcCategory::COUNT];
    for ticket in tickets.iter().filter(|t| t.status.is_open()) {
        if let Some(index) = QcCategory::ALL
            .iter()
            .position(|kind| *kind == ticket.category)
        {
            if let Some(slot) = counts.get_mut(index) {
                *slot = slot.saturating_add(1);
            }
        }
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::{Finding, Frame};
    use aura_core::contract::ids::ProjectId;
    use aura_core::contract::qc::{ImageId, QcCode, Remedy, SolveTarget};
    use aura_core::contract::scene::SceneId;

    fn ticket(category: QcCategory, deviation: f32, status: TicketStatus) -> QcTicket {
        let frame = Frame::empty(ImageId::new(), SceneId::Ceremony);
        let finding = Finding::new(category, code_for(category), deviation, 1.0, 0.5, 0.9);
        let mut ticket = crate::ticket::from_finding(
            ProjectId::new(),
            &frame,
            finding,
            Remedy::ResolveParam {
                target: SolveTarget::Normalisation,
                constraint: "x".into(),
            },
            0,
        );
        ticket.status = status;
        ticket
    }

    fn code_for(category: QcCategory) -> QcCode {
        match category {
            QcCategory::Consistency => QcCode::ConsistencyDrift,
            QcCategory::Skin => QcCode::SkinDrift,
            QcCategory::Exposure => QcCode::ExposureRegression,
            QcCategory::Sharpness => QcCode::SharpnessBelowFloor,
            QcCategory::Retouch => QcCode::TextureFloorMissed,
            QcCategory::Mask => QcCode::MaskEdgeArtefact,
            QcCategory::Crop => QcCode::CropUnsafe,
            QcCategory::Cleanup => QcCode::CleanupArtefact,
            QcCategory::Duplicate => QcCode::DuplicateLeak,
            QcCategory::Coverage => QcCode::CoverageMissing,
        }
    }

    #[test]
    fn the_worst_frame_leads_the_photographers_queue() {
        // The opposite ordering from `triage::order`, and deliberately: a photographer is triaging
        // their own attention, not deciding what to fix first mechanically.
        let mild = ticket(QcCategory::Consistency, 1.1, TicketStatus::Open);
        let bad = ticket(QcCategory::Retouch, 9.0, TicketStatus::Open);
        let queue = open(&[mild, bad], 10);
        assert_eq!(queue[0].category, QcCategory::Retouch);
        // Whereas the loop would work the consistency finding first.
        assert!(
            QcCategory::Consistency.triage_rank() < QcCategory::Retouch.triage_rank(),
            "the two orderings really are different"
        );
    }

    #[test]
    fn a_settled_ticket_is_never_in_the_queue() {
        let accepted = ticket(QcCategory::Skin, 9.0, TicketStatus::Accepted);
        let dismissed = ticket(QcCategory::Skin, 9.0, TicketStatus::Dismissed);
        let fixed = ticket(QcCategory::Skin, 9.0, TicketStatus::Fixed);
        assert!(open(&[accepted, dismissed, fixed], 10).is_empty());
    }

    #[test]
    fn a_reverted_ticket_is_still_in_the_queue() {
        // A correction that was tried and put back is exactly the case a person should see.
        let reverted = ticket(QcCategory::Skin, 9.0, TicketStatus::Reverted);
        assert_eq!(open(&[reverted], 10).len(), 1);
    }

    #[test]
    fn groups_are_ordered_by_their_worst_member_rather_than_by_size() {
        // One unsafe crop beats forty marginal colour drifts. A photographer with twenty minutes
        // needs the first thing they open to be the worst thing there is.
        let mut tickets: Vec<QcTicket> = (0..40)
            .map(|_| ticket(QcCategory::Consistency, 1.05, TicketStatus::Open))
            .collect();
        tickets.push(ticket(QcCategory::Crop, 8.0, TicketStatus::Open));
        let groups = group(&tickets);
        assert_eq!(groups[0].category, QcCategory::Crop);
        assert_eq!(groups[0].tickets.len(), 1);
        assert_eq!(groups[1].tickets.len(), 40);
    }

    #[test]
    fn a_category_filter_returns_only_that_category() {
        let tickets = vec![
            ticket(QcCategory::Skin, 2.0, TicketStatus::Open),
            ticket(QcCategory::Crop, 2.0, TicketStatus::Open),
        ];
        let only = in_category(&tickets, QcCategory::Skin, 10);
        assert_eq!(only.len(), 1);
        assert_eq!(only[0].category, QcCategory::Skin);
    }

    #[test]
    fn a_bulk_action_never_authorises_a_remedy() {
        // The single easiest way to damage a gallery in this product would be a bulk button that
        // both agreed with forty findings and acted on forty frames.
        let tickets = vec![
            ticket(QcCategory::Skin, 2.0, TicketStatus::Open),
            ticket(QcCategory::Crop, 2.0, TicketStatus::Open),
        ];
        let overrides = bulk(&tickets, TicketStatus::Accepted);
        assert_eq!(overrides.len(), 2);
        assert!(overrides.iter().all(|over| !over.apply_remedy));
        assert!(overrides.iter().all(QcOverride::is_valid));
    }

    #[test]
    fn a_bulk_action_cannot_set_a_status_automation_owns() {
        let tickets = vec![ticket(QcCategory::Skin, 2.0, TicketStatus::Open)];
        for status in [
            TicketStatus::Fixed,
            TicketStatus::Reverted,
            TicketStatus::Escalated,
            TicketStatus::Open,
        ] {
            assert!(
                bulk(&tickets, status).is_empty(),
                "{status} is a record of what happened, not an opinion about it"
            );
        }
    }

    #[test]
    fn the_limit_is_respected() {
        let tickets: Vec<QcTicket> = (0..50)
            .map(|_| ticket(QcCategory::Skin, 2.0, TicketStatus::Open))
            .collect();
        assert_eq!(open(&tickets, 10).len(), 10);
    }

    #[test]
    fn the_ordering_is_identical_on_every_machine() {
        // `QcTicket::queue_order` breaks its last tie on the id, so two equally severe findings in
        // one category come out in the same order everywhere. Without that, a support case and a
        // photographer would see two different queues.
        let tickets: Vec<QcTicket> = (0..8)
            .map(|_| ticket(QcCategory::Skin, 2.0, TicketStatus::Open))
            .collect();
        let first: Vec<_> = open(&tickets, 8).into_iter().map(|t| t.id).collect();
        let second: Vec<_> = open(&tickets, 8).into_iter().map(|t| t.id).collect();
        assert_eq!(first, second);
        let mut sorted = first.clone();
        sorted.sort_unstable();
        assert_eq!(first, sorted);
    }

    #[test]
    fn the_outstanding_counts_are_in_category_order() {
        let tickets = vec![
            ticket(QcCategory::Consistency, 2.0, TicketStatus::Open),
            ticket(QcCategory::Consistency, 2.0, TicketStatus::Open),
            ticket(QcCategory::Coverage, 2.0, TicketStatus::Open),
        ];
        let counts = outstanding(&tickets);
        assert_eq!(counts[0], 2);
        assert_eq!(counts[QcCategory::COUNT - 1], 1);
    }
}
