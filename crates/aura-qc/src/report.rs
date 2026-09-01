//! The report a studio keeps. PHASE-27 sections 2.1 and 6.4.
//!
//! ## What a QC report is for, and why it is not a list of failures
//!
//! Section 1: this phase "produces the artefact photographers want most: a short, honest report of
//! what was checked, what was fixed and what needs their eyes - turning fear of automation into an
//! auditable workflow."
//!
//! Two words in that sentence do the work. **Checked**, because a report that only listed problems
//! would make a wedding with two findings and eight hundred skipped inspections look better than one
//! with twenty findings and complete coverage. And **honest**, because the number a photographer is
//! deciding on is not "how many problems were there" but "how much of this did the product actually
//! look at".
//!
//! So `checks_skipped` and `images_unreached` lead the report rather than being a footnote, and
//! [`to_markdown`] prints the completeness figure above the findings table.
//!
//! ## Markdown, and no PDF
//!
//! Section 2.1 asks for "PDF/Markdown". Markdown ships. A PDF writer is a dependency, a font
//! embedding decision and a page-layout engine, and none of those three is a quality-control
//! question - while Markdown is text a studio can archive, diff, paste into an email and convert
//! with any tool they already own. ADR-0055 section 10.
//!
//! It also makes the report **testable**: [`to_markdown`] is a pure function over [`QcReport`], so
//! what a studio reads is asserted rather than eyeballed.

use std::fmt::Write as _;

use aura_core::contract::qc::{
    CategoryTally, QcCategory, QcCode, QcReport, QcTicket, Replacement, TicketStatus,
};

/// Assemble a report from what a pass produced.
///
/// Takes the tickets rather than counting as it goes, because a pass that counted incrementally
/// would have two sources of truth for "how many were fixed" - its own counter and the rows it
/// wrote - and they would disagree the first time a ticket was superseded.
#[must_use]
// Eleven arguments, and they are eleven separate counts rather than a struct because a struct
// here would be `QcReport` with three fields missing - a second shape meaning almost the same
// thing, which is the failure phase 08's rule about two answers exists to prevent.
#[allow(clippy::too_many_arguments)]
pub fn assemble(
    project: aura_core::ProjectId,
    tickets: &[QcTicket],
    replacements: Vec<Replacement>,
    images: u32,
    images_unreached: u32,
    checks_run: u32,
    checks_skipped: u32,
    planner_calls: u32,
    duration_ms: u64,
    thresholds_ver: u16,
    analysis_ver: u16,
) -> QcReport {
    let mut by_category = [CategoryTally::default(); QcCategory::COUNT];
    let mut fixed = 0u32;
    let mut reverted = 0u32;
    let mut escalated = 0u32;

    for ticket in tickets {
        let Some(index) = QcCategory::ALL
            .iter()
            .position(|kind| *kind == ticket.category)
        else {
            continue;
        };
        let Some(tally) = by_category.get_mut(index) else {
            continue;
        };
        tally.found = tally.found.saturating_add(1);
        match ticket.status {
            TicketStatus::Fixed => {
                tally.fixed = tally.fixed.saturating_add(1);
                fixed = fixed.saturating_add(1);
            }
            TicketStatus::Escalated => {
                tally.escalated = tally.escalated.saturating_add(1);
                escalated = escalated.saturating_add(1);
            }
            TicketStatus::Reverted => reverted = reverted.saturating_add(1),
            TicketStatus::Open | TicketStatus::Accepted | TicketStatus::Dismissed => {}
        }
        if ticket.outcome_code == Some(QcCode::RemedyReverted)
            || ticket.outcome_code == Some(QcCode::CollateralDamage)
        {
            reverted = reverted.saturating_add(1);
        }
    }

    // The skipped count is distributed across categories by the pass rather than derived here,
    // because a check that skipped produced no ticket to attribute it from. It arrives as a total
    // and the per-category share is filled in by `api::collect`'s own bookkeeping.
    QcReport {
        project,
        checks_run,
        images,
        images_unreached,
        by_category,
        replacements,
        skipped: checks_skipped,
        fixed,
        reverted,
        escalated,
        planner_calls,
        duration_ms,
        cloud_used: planner_calls > 0,
        thresholds_ver,
        analysis_ver,
    }
}

/// Fill in the per-category skip counts.
///
/// Separate from [`assemble`] because the two come from different places: findings come from
/// tickets and skips come from outcomes that produced none. Merging them into one function would
/// mean passing a parallel array of skips into a function whose other argument is a list of tickets,
/// which is the shape that invites somebody to index one with the other's length.
pub fn with_skips(report: &mut QcReport, skips: [u32; QcCategory::COUNT]) {
    for (tally, skipped) in report.by_category.iter_mut().zip(skips) {
        tally.skipped = skipped;
    }
}

/// The report as Markdown, for a studio's records.
///
/// Deterministic: the same report produces the same bytes on every machine, which is what lets a
/// test assert on it and a studio diff two weddings.
#[must_use]
// One document, written top to bottom. Its sections are not reusable and a helper per section
// would hide the ordering, which is the part of this function that carries a decision: what was
// checked comes before what was found.
#[allow(clippy::too_many_lines)]
pub fn to_markdown(report: &QcReport, replacements: &[Replacement]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Quality control report");
    let _ = writeln!(out);

    // Completeness first. A report that led with findings would let a wedding with two findings and
    // eight hundred skipped inspections read better than one that was actually checked.
    let _ = writeln!(out, "## What was checked");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "- Photographs in the delivered gallery inspected: **{}**",
        report.images
    );
    if report.images_unreached > 0 {
        let _ = writeln!(
            out,
            "- Photographs **not reached** before the time budget ran out: **{}**",
            report.images_unreached
        );
    }
    let _ = writeln!(out, "- Inspections that ran: **{}**", report.checks_run);
    if report.skipped > 0 {
        let _ = writeln!(
            out,
            "- Inspections that could **not** run because something they needed was missing: \
             **{}**",
            report.skipped
        );
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "  A check that could not run has made no claim either way. It is not the same as a \
             check that passed."
        );
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "## What was found");
    let _ = writeln!(out);
    let _ = writeln!(out, "| Check | Found | Fixed | Needs you | Not checked |");
    let _ = writeln!(out, "|---|---:|---:|---:|---:|");
    for (category, tally) in QcCategory::ALL.iter().zip(report.by_category.iter()) {
        if tally.found == 0 && tally.skipped == 0 {
            continue;
        }
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} |",
            category, tally.found, tally.fixed, tally.escalated, tally.skipped
        );
    }
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "**{} found, {} corrected and verified, {} put back, {} for you to look at.**",
        report.found(),
        report.fixed,
        report.reverted,
        report.escalated
    );
    let _ = writeln!(out);
    if report.reverted > 0 {
        let _ = writeln!(
            out,
            "A correction that was *put back* is one AURA tried, measured, and found had not \
             helped. The photograph is exactly as it was before it tried."
        );
        let _ = writeln!(out);
    }

    if !replacements.is_empty() {
        let _ = writeln!(out, "## Photographs AURA swapped");
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "Each of these delivers a different frame from the same moment. Every swap was checked \
             against the gallery's coverage rules before it was made."
        );
        let _ = writeln!(out);
        for swap in replacements {
            let _ = writeln!(
                out,
                "- **{}** replaced by **{}** - {}",
                short(&swap.replaced.to_db()),
                short(&swap.replacement.to_db()),
                swap.note
            );
        }
        let _ = writeln!(out);
    }

    let _ = writeln!(out, "## About this run");
    let _ = writeln!(out);
    let _ = writeln!(out, "- Took {} ms", report.duration_ms);
    let _ = writeln!(
        out,
        "- A second opinion was {}",
        if report.cloud_used {
            format!("asked for on {} photographs", report.planner_calls)
        } else {
            "not used".to_string()
        }
    );
    let _ = writeln!(
        out,
        "- Thresholds version {}, analysis version {}",
        report.thresholds_ver, report.analysis_ver
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "AURA checks finished photographs against the rest of the wedding they belong to. It \
         measures rather than guesses: every line above is a number against a threshold, and \
         `docs/how-qc-works.md` says what each check measures and why."
    );

    out
}

/// The last twelve characters of an id, which is enough to tell two apart in a report.
fn short(id: &str) -> &str {
    let start = id.len().saturating_sub(12);
    id.get(start..).unwrap_or(id)
}

/// One ticket as a line for the escalation queue's export.
///
/// The diagnosis is rendered from the row rather than read from it - phase 09's rule and ADR-0055
/// section 3. Two builds with the same row produce the same sentence.
#[must_use]
pub fn ticket_line(ticket: &QcTicket) -> String {
    format!(
        "- [{}] {} ({} of threshold) - {}",
        ticket.category,
        ticket.render_diagnosis(),
        format_args!("{:.1}x", ticket.severity()),
        ticket.status
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::{Finding, Frame};
    use aura_core::contract::ids::ProjectId;
    use aura_core::contract::qc::{ImageId, Remedy, SolveTarget};
    use aura_core::contract::scene::SceneId;

    fn ticket(category: QcCategory, code: QcCode, status: TicketStatus) -> QcTicket {
        let frame = Frame::empty(ImageId::new(), SceneId::Ceremony);
        let finding = Finding::new(category, code, 4.2, 2.5, 1.0, 0.9);
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
        if status != TicketStatus::Open {
            ticket.outcome_code = Some(QcCode::RemedyApplied);
        }
        ticket
    }

    fn report_of(tickets: &[QcTicket]) -> QcReport {
        assemble(
            ProjectId::new(),
            tickets,
            Vec::new(),
            100,
            0,
            900,
            0,
            0,
            1_234,
            1,
            1,
        )
    }

    #[test]
    fn the_tallies_add_up_to_what_was_found() {
        let tickets = vec![
            ticket(QcCategory::Skin, QcCode::SkinDrift, TicketStatus::Fixed),
            ticket(QcCategory::Skin, QcCode::SkinDrift, TicketStatus::Escalated),
            ticket(
                QcCategory::Crop,
                QcCode::CropUnsafe,
                TicketStatus::Escalated,
            ),
        ];
        let report = report_of(&tickets);
        assert_eq!(report.found(), 3);
        assert_eq!(report.fixed, 1);
        assert_eq!(report.escalated, 2);
        assert_eq!(report.tally(QcCategory::Skin).found, 2);
        assert_eq!(report.tally(QcCategory::Crop).escalated, 1);
    }

    #[test]
    fn the_report_leads_with_what_was_checked_rather_than_with_what_was_found() {
        // A report that led with findings would let a wedding with two findings and eight hundred
        // skipped inspections read better than one that was actually checked.
        let markdown = to_markdown(&report_of(&[]), &[]);
        let checked = markdown
            .find("What was checked")
            .expect("a checked section");
        let found = markdown.find("What was found").expect("a found section");
        assert!(checked < found);
    }

    #[test]
    fn a_skipped_inspection_is_stated_as_making_no_claim() {
        let mut report = report_of(&[]);
        report.skipped = 400;
        let markdown = to_markdown(&report, &[]);
        assert!(markdown.contains("could **not** run"));
        assert!(markdown.contains("made no claim either way"));
        assert!(markdown.contains("not the same as a check that passed"));
    }

    #[test]
    fn an_unreached_gallery_is_stated_rather_than_omitted() {
        let mut report = report_of(&[]);
        report.images = 800;
        report.images_unreached = 200;
        let markdown = to_markdown(&report, &[]);
        assert!(markdown.contains("**not reached**"));
        assert!(markdown.contains("200"));
    }

    #[test]
    fn a_reverted_correction_is_explained_rather_than_listed_as_a_failure() {
        let mut report = report_of(&[]);
        report.reverted = 3;
        let markdown = to_markdown(&report, &[]);
        assert!(markdown.contains("exactly as it was before it tried"));
    }

    #[test]
    fn a_swap_is_always_shown_with_its_reasoning() {
        let swap = Replacement {
            ticket: aura_core::contract::ids::TicketId::new(),
            replaced: ImageId::new(),
            replacement: ImageId::new(),
            category: QcCategory::Sharpness,
            metric_before: 0.6,
            metric_after: 0.1,
            confidence: 0.92,
            coverage_held: true,
            note: "the alternative is sharper".into(),
            at: 0,
        };
        let markdown = to_markdown(&report_of(&[]), std::slice::from_ref(&swap));
        assert!(markdown.contains("Photographs AURA swapped"));
        assert!(markdown.contains("the alternative is sharper"));
        assert!(markdown.contains("checked against the gallery's coverage rules"));
    }

    #[test]
    fn the_markdown_is_deterministic() {
        let tickets = vec![ticket(
            QcCategory::Skin,
            QcCode::SkinDrift,
            TicketStatus::Fixed,
        )];
        let report = report_of(&tickets);
        assert_eq!(to_markdown(&report, &[]), to_markdown(&report, &[]));
    }

    #[test]
    fn a_category_with_nothing_to_say_is_left_out_of_the_table() {
        let markdown = to_markdown(&report_of(&[]), &[]);
        assert!(!markdown.contains("| duplicate |"));
    }

    #[test]
    fn a_category_with_only_skips_is_still_in_the_table() {
        // The one that matters. A check that could not run on four hundred frames must not vanish
        // from the report because it produced no findings.
        let mut report = report_of(&[]);
        let mut skips = [0u32; QcCategory::COUNT];
        skips[5] = 400;
        with_skips(&mut report, skips);
        let markdown = to_markdown(&report, &[]);
        assert!(markdown.contains("| mask |"));
        assert!(markdown.contains("400"));
    }

    #[test]
    fn the_report_says_whether_a_second_opinion_was_asked_for() {
        let quiet = to_markdown(&report_of(&[]), &[]);
        assert!(quiet.contains("not used"));
        let mut asked = report_of(&[]);
        asked.planner_calls = 7;
        asked.cloud_used = true;
        assert!(to_markdown(&asked, &[]).contains("7 photographs"));
    }

    #[test]
    fn a_ticket_line_renders_its_diagnosis_rather_than_storing_it() {
        let ticket = ticket(QcCategory::Skin, QcCode::SkinDrift, TicketStatus::Open);
        let line = ticket_line(&ticket);
        assert!(line.contains("4.20 dE00"));
        assert!(line.contains("1.7x"));
    }

    #[test]
    fn cloud_used_follows_the_call_count_rather_than_being_set_separately() {
        // Migration 27 CHECKs the same thing: `cloud_used = 1` with zero calls is impossible.
        assert!(!report_of(&[]).cloud_used);
    }
}
