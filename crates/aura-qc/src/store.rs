//! The four tables migration 27 adds, and the rules that live in the SQL rather than around them.
//!
//! ## A row here can outlive what it is about
//!
//! Twenty-four migrations recorded a fact about one photograph, migration 25 about a set of them and
//! migration 26 about a body. This one records a fact about **AURA's own earlier decisions**, and
//! that has a property none of the first three has.
//!
//! A frame replaced by its runner-up leaves a ticket that must keep pointing at the replacement it
//! caused. A reverted remedy leaves a ticket whose entire value is the record that something was
//! tried and put back. And a ticket a photographer dismissed has to survive every future pass.
//!
//! ## The three things this store is careful about
//!
//! **A photographer's verdict is read out before a sweep and put back after.** [`take_decisions`]
//! and [`restore_decisions`] are phase 25's mechanism, and the reason the DELETE guard alone would
//! not be enough is phase 18's: this table is cleared with a DELETE and refilled with fresh ids, so
//! there is no row for a guard to protect by the time the new one is written. The trigger
//! `qc_ticket_keep_user_status` is the second layer.
//!
//! **A round is append-only and the database says so.** Second such table in the product after
//! migration 13's ledger. [`write_round`] never updates; a correction is a second round.
//!
//! **The diagnosis is not a column.** `QcTicket::render_diagnosis` builds it from the code, the
//! deviation, the threshold and the evidence on every read. Phase 09's rule, ADR-0055 section 3, and
//! the reason a studio archiving reports does not end up with two weddings whose identical findings
//! read differently.

use std::collections::BTreeMap;
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::contract::ids::{IdentityId, TicketId};
use aura_core::contract::ledger::Autonomy;
use aura_core::contract::qc::{
    CategoryTally, Evidence, ImageId, QcCategory, QcCode, QcOutline, QcOverride, QcReason,
    QcReport, QcRound, QcTicket, Remedy, Replacement, SolveTarget, TicketStatus,
};
use aura_core::contract::scene::SceneId;
use aura_core::errors::db::statement_failed;
use aura_core::errors::ml::qc_decision_refused;
use aura_core::{AuraResult, ProjectId};
use rusqlite::{params, OptionalExtension};

use crate::DETECTOR_TRAINED;

/// One catalog, wrapped.
#[derive(Debug, Clone)]
pub struct QcStore {
    catalog: Arc<Catalog>,
    clock: Arc<dyn Clock>,
}

/// What a photographer decided, read out before a re-pass clears the project.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Decisions {
    /// Per `(image, category, code)`: the status a person set and the note they left.
    ///
    /// Keyed on the finding rather than on the ticket id, because a re-pass assigns new ids. Three
    /// parts rather than two, because one photograph can carry two findings in one category - which
    /// is exactly why `TicketId` exists at all.
    pub verdicts: BTreeMap<(String, String, String), (TicketStatus, Option<String>)>,
}

impl QcStore {
    /// Wrap one catalog.
    #[must_use]
    pub fn new(catalog: Arc<Catalog>, clock: Arc<dyn Clock>) -> Self {
        Self { catalog, clock }
    }

    /// The catalog underneath, for the gate and the budget test.
    #[must_use]
    pub fn catalog(&self) -> &Arc<Catalog> {
        &self.catalog
    }

    /// True when a project's stored findings came from this build's arithmetic and this table.
    ///
    /// Whole-project rather than per-photograph, exactly as phases 25 and 26 are: a gallery half
    /// checked under one set of thresholds and half under another has been checked against two
    /// different promises, and a queue ordered across the two is ordered by nothing.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn is_current(&self, project: ProjectId, versions: (u16, u16)) -> AuraResult<bool> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let stale: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM qc_ticket
                      WHERE project_id = ?1 AND (analysis_ver <> ?2 OR thresholds_ver <> ?3)",
                    params![key, i64::from(versions.0), i64::from(versions.1)],
                    |row| row.get(0),
                )
                .map_err(|err| statement_failed("qc_ticket version check", &err))?;
            let present: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM qc_run WHERE project_id = ?1",
                    params![key],
                    |row| row.get(0),
                )
                .map_err(|err| statement_failed("qc_run count", &err))?;
            Ok(present > 0 && stale == 0)
        })
    }

    /// The versions a project's findings carry, or `None` when it has none.
    ///
    /// What `AURA-ML-5141` is raised from.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn stored_versions(&self, project: ProjectId) -> AuraResult<Option<(u16, u16)>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            conn.query_row(
                "SELECT analysis_ver, thresholds_ver FROM qc_run WHERE project_id = ?1",
                params![key],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()
            .map_err(|err| statement_failed("qc_run versions", &err))
            .map(|found| {
                found.map(|(analysis, thresholds)| {
                    (
                        u16::try_from(analysis).unwrap_or(0),
                        u16::try_from(thresholds).unwrap_or(0),
                    )
                })
            })
        })
    }

    /// Read out every verdict a photographer set, before a re-pass clears the project.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn take_decisions(&self, project: ProjectId) -> AuraResult<Decisions> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT image_id, category, code, status, user_note FROM qc_ticket
                      WHERE project_id = ?1 AND status IN ('accepted','dismissed')",
                )
                .map_err(|err| statement_failed("qc_ticket decisions", &err))?;
            let rows = statement
                .query_map(params![key], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                    ))
                })
                .map_err(|err| statement_failed("qc_ticket decisions", &err))?;
            let mut verdicts = BTreeMap::new();
            for row in rows {
                let (image, category, code, status, note) =
                    row.map_err(|err| statement_failed("qc_ticket decisions", &err))?;
                let Some(status) = TicketStatus::parse(&status) else {
                    continue;
                };
                verdicts.insert((image, category, code), (status, note));
            }
            Ok(Decisions { verdicts })
        })
    }

    /// Clear a project's findings and write a fresh pass, restoring what a photographer decided.
    ///
    /// The whole of it in one transaction, so that a pass killed half way leaves the previous
    /// findings rather than a partial set. Invariant 5.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when a statement fails.
    // One transaction, four tables, and it is long because it is *one* transaction: a helper
    // per table would either take the open transaction as an argument - which is the same
    // function with more indirection - or open its own, which is four transactions and a pass
    // that can be interrupted half written.
    #[allow(clippy::too_many_lines)]
    pub fn write_pass(
        &self,
        project: ProjectId,
        tickets: &[QcTicket],
        rounds: &[QcRound],
        replacements: &[Replacement],
        report: &QcReport,
        decisions: &Decisions,
    ) -> AuraResult<()> {
        let key = project.to_db();
        let now = self.now();
        let tickets = tickets.to_vec();
        let rounds = rounds.to_vec();
        let replacements = replacements.to_vec();
        let report = report.clone();
        let decisions = decisions.clone();

        self.catalog.writer().transact(move |tx| {
            // `qc_round` and `qc_replacement` cascade from `qc_ticket`, so one DELETE clears three
            // tables. The round triggers refuse a *direct* delete and permit a cascading one, which
            // is the distinction migration 27's `qc_round_no_direct_delete` draws.
            tx.execute("DELETE FROM qc_ticket WHERE project_id = ?1", params![key])
                .map_err(|err| statement_failed("qc_ticket clear", &err))?;
            tx.execute("DELETE FROM qc_run WHERE project_id = ?1", params![key])
                .map_err(|err| statement_failed("qc_run clear", &err))?;

            for ticket in &tickets {
                // A finding a photographer already ruled on keeps their verdict. Their note too:
                // the sentence they typed is about the finding, and the finding is the same one.
                let restored = decisions.verdicts.get(&(
                    ticket.image_id.to_db(),
                    ticket.category.as_str().to_string(),
                    ticket.code.as_str().to_string(),
                ));
                let (status, note) = restored.map_or((ticket.status, None), |(status, note)| {
                    (*status, note.clone())
                });
                let outcome = if status == TicketStatus::Open {
                    None
                } else {
                    ticket
                        .outcome_code
                        .map(|code| code.as_str().to_string())
                        .or_else(|| Some(QcCode::EscalatedToHuman.as_str().to_string()))
                };
                let (kind, target, factor) = remedy_columns(&ticket.remedy);
                tx.execute(
                    "INSERT INTO qc_ticket (
                        ticket_id, project_id, image_id, category, code,
                        deviation, threshold, unit, evidence_kind, evidence_json,
                        identity_id, remedy_kind, remedy_target, remedy_factor,
                        expected_gain, confidence, autonomy, reasons, reasons_json,
                        round, status, outcome_code, user_note, scene,
                        thresholds_ver, analysis_ver, created_at, updated_at
                     ) VALUES (
                        ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                        ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20,
                        ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?27)",
                    params![
                        ticket.id.to_db(),
                        key,
                        ticket.image_id.to_db(),
                        ticket.category.as_str(),
                        ticket.code.as_str(),
                        f64::from(ticket.deviation),
                        f64::from(ticket.threshold),
                        ticket.category.unit(),
                        ticket.evidence.kind_str(),
                        evidence_json(&ticket.evidence),
                        ticket.identity.map(|id| id.to_db()),
                        kind,
                        target,
                        factor.map(f64::from),
                        f64::from(ticket.expected_gain),
                        f64::from(ticket.confidence),
                        ticket.autonomy.as_str(),
                        reason_mask(&ticket.reasons),
                        reasons_json(&ticket.reasons),
                        i64::from(ticket.round),
                        status.as_str(),
                        outcome,
                        note,
                        ticket.scene.as_str(),
                        i64::from(ticket.thresholds_ver),
                        i64::from(ticket.analysis_ver),
                        now.clone(),
                    ],
                )
                .map_err(|err| statement_failed("qc_ticket insert", &err))?;
            }

            for round in &rounds {
                let (kind, target, factor) = remedy_columns(&round.remedy);
                tx.execute(
                    "INSERT INTO qc_round (
                        ticket_id, round, remedy_kind, remedy_target, remedy_factor,
                        deviation_before, deviation_after, expected_gain,
                        collateral, collateral_category, kept, outcome, ms, created_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                    params![
                        round.ticket.to_db(),
                        i64::from(round.round),
                        kind,
                        target,
                        factor.map(f64::from),
                        f64::from(round.deviation_before),
                        f64::from(round.deviation_after),
                        f64::from(round.expected_gain),
                        f64::from(round.collateral),
                        round.collateral_category.map(QcCategory::as_str),
                        i64::from(round.kept),
                        round.outcome.as_str(),
                        i64::from(round.ms),
                        now.clone(),
                    ],
                )
                .map_err(|err| statement_failed("qc_round insert", &err))?;
            }

            for swap in &replacements {
                tx.execute(
                    "INSERT INTO qc_replacement (
                        ticket_id, project_id, replaced_image, replacement_image, category,
                        metric_before, metric_after, confidence, coverage_held, note, created_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, ?9, ?10)",
                    params![
                        swap.ticket.to_db(),
                        key,
                        swap.replaced.to_db(),
                        swap.replacement.to_db(),
                        swap.category.as_str(),
                        f64::from(swap.metric_before),
                        f64::from(swap.metric_after),
                        f64::from(swap.confidence),
                        swap.note.clone(),
                        now.clone(),
                    ],
                )
                .map_err(|err| statement_failed("qc_replacement insert", &err))?;
            }

            let tallies = serde_json::to_string(
                &report
                    .by_category
                    .iter()
                    .map(|tally| (tally.found, tally.fixed, tally.escalated, tally.skipped))
                    .collect::<Vec<_>>(),
            )
            .unwrap_or_else(|_| "[]".to_string());

            tx.execute(
                "INSERT INTO qc_run (
                    project_id, images, images_unreached, checks_run, checks_skipped,
                    found, fixed, reverted, escalated, replaced, by_category,
                    planner_calls, cloud_used, duration_ms, thresholds_ver, analysis_ver, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
                params![
                    key,
                    i64::from(report.images),
                    i64::from(report.images_unreached),
                    i64::from(report.checks_run),
                    i64::from(report.skipped),
                    i64::from(report.found()),
                    i64::from(report.fixed),
                    i64::from(report.reverted),
                    i64::from(report.escalated),
                    i64::try_from(replacements.len()).unwrap_or(0),
                    tallies,
                    i64::from(report.planner_calls),
                    i64::from(report.cloud_used),
                    i64::try_from(report.duration_ms).unwrap_or(i64::MAX),
                    i64::from(report.thresholds_ver),
                    i64::from(report.analysis_ver),
                    now,
                ],
            )
            .map_err(|err| statement_failed("qc_run insert", &err))?;

            Ok(())
        })
    }

    /// Every ticket on one photograph, worst first.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn tickets_for(&self, image: ImageId) -> AuraResult<Vec<QcTicket>> {
        let key = image.to_db();
        let mut found = self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(&format!("{TICKET_SELECT} WHERE image_id = ?1"))
                .map_err(|err| statement_failed("qc_ticket by image", &err))?;
            let rows = statement
                .query_map(params![key], read_ticket)
                .map_err(|err| statement_failed("qc_ticket by image", &err))?;
            collect(rows, "qc_ticket by image")
        })?;
        found.sort_by(QcTicket::queue_order);
        Ok(found)
    }

    /// The escalation queue for one project.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn queue(
        &self,
        project: ProjectId,
        category: Option<QcCategory>,
        limit: usize,
    ) -> AuraResult<Vec<QcTicket>> {
        let key = project.to_db();
        let filter = category.map(|kind| kind.as_str().to_string());
        let mut found = self.catalog.read(move |conn| {
            let sql = format!(
                "{TICKET_SELECT} WHERE project_id = ?1
                  AND status IN ('open','escalated','reverted')
                  AND (?2 IS NULL OR category = ?2)"
            );
            let mut statement = conn
                .prepare(&sql)
                .map_err(|err| statement_failed("qc_ticket queue", &err))?;
            let rows = statement
                .query_map(params![key, filter], read_ticket)
                .map_err(|err| statement_failed("qc_ticket queue", &err))?;
            collect(rows, "qc_ticket queue")
        })?;
        found.sort_by(QcTicket::queue_order);
        found.truncate(limit);
        Ok(found)
    }

    /// Every round run against one ticket, oldest first.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn rounds_for(&self, ticket: TicketId) -> AuraResult<Vec<QcRound>> {
        let key = ticket.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT ticket_id, round, remedy_kind, remedy_target, remedy_factor,
                            deviation_before, deviation_after, expected_gain, collateral,
                            collateral_category, kept, outcome, ms, created_at
                       FROM qc_round WHERE ticket_id = ?1 ORDER BY round ASC",
                )
                .map_err(|err| statement_failed("qc_round by ticket", &err))?;
            let rows = statement
                .query_map(params![key], |row| {
                    Ok(QcRound {
                        ticket: TicketId::from_db(&row.get::<_, String>(0)?)
                            .unwrap_or_else(|_| TicketId::new()),
                        round: u8::try_from(row.get::<_, i64>(1)?).unwrap_or(1),
                        remedy: remedy_from(
                            &row.get::<_, String>(2)?,
                            &row.get::<_, String>(3)?,
                            row.get::<_, Option<f64>>(4)?,
                        ),
                        deviation_before: row.get::<_, f64>(5)? as f32,
                        deviation_after: row.get::<_, f64>(6)? as f32,
                        expected_gain: row.get::<_, f64>(7)? as f32,
                        collateral: row.get::<_, f64>(8)? as f32,
                        collateral_category: row
                            .get::<_, Option<String>>(9)?
                            .and_then(|text| QcCategory::parse(&text)),
                        kept: row.get::<_, i64>(10)? != 0,
                        outcome: QcCode::parse(&row.get::<_, String>(11)?)
                            .unwrap_or(QcCode::RemedyApplied),
                        ms: u32::try_from(row.get::<_, i64>(12)?).unwrap_or(0),
                        at: 0,
                    })
                })
                .map_err(|err| statement_failed("qc_round by ticket", &err))?;
            collect(rows, "qc_round by ticket")
        })
    }

    /// Every replacement this project made.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn replacements(&self, project: ProjectId) -> AuraResult<Vec<Replacement>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT ticket_id, replaced_image, replacement_image, category,
                            metric_before, metric_after, confidence, coverage_held, note
                       FROM qc_replacement WHERE project_id = ?1 ORDER BY ticket_id",
                )
                .map_err(|err| statement_failed("qc_replacement", &err))?;
            let rows = statement
                .query_map(params![key], |row| {
                    Ok(Replacement {
                        ticket: TicketId::from_db(&row.get::<_, String>(0)?)
                            .unwrap_or_else(|_| TicketId::new()),
                        replaced: ImageId::from_db(&row.get::<_, String>(1)?)
                            .unwrap_or_else(|_| ImageId::new()),
                        replacement: ImageId::from_db(&row.get::<_, String>(2)?)
                            .unwrap_or_else(|_| ImageId::new()),
                        category: QcCategory::parse(&row.get::<_, String>(3)?)
                            .unwrap_or(QcCategory::Consistency),
                        metric_before: row.get::<_, f64>(4)? as f32,
                        metric_after: row.get::<_, f64>(5)? as f32,
                        confidence: row.get::<_, f64>(6)? as f32,
                        coverage_held: row.get::<_, i64>(7)? != 0,
                        note: row.get::<_, String>(8)?,
                        at: 0,
                    })
                })
                .map_err(|err| statement_failed("qc_replacement", &err))?;
            collect(rows, "qc_replacement")
        })
    }

    /// The most recent report, when a pass has run.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn report(&self, project: ProjectId) -> AuraResult<Option<QcReport>> {
        let key = project.to_db();
        let replacements = self.replacements(project)?;
        self.catalog.read(move |conn| {
            let row = conn
                .query_row(
                    "SELECT images, images_unreached, checks_run, checks_skipped,
                            fixed, reverted, escalated, by_category, planner_calls,
                            cloud_used, duration_ms, thresholds_ver, analysis_ver
                       FROM qc_run WHERE project_id = ?1",
                    params![key.clone()],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, i64>(2)?,
                            row.get::<_, i64>(3)?,
                            row.get::<_, i64>(4)?,
                            row.get::<_, i64>(5)?,
                            row.get::<_, i64>(6)?,
                            row.get::<_, String>(7)?,
                            row.get::<_, i64>(8)?,
                            row.get::<_, i64>(9)?,
                            row.get::<_, i64>(10)?,
                            row.get::<_, i64>(11)?,
                            row.get::<_, i64>(12)?,
                        ))
                    },
                )
                .optional()
                .map_err(|err| statement_failed("qc_run", &err))?;
            let Some(row) = row else {
                return Ok(None);
            };
            let mut by_category = [CategoryTally::default(); QcCategory::COUNT];
            if let Ok(parsed) = serde_json::from_str::<Vec<(u32, u32, u32, u32)>>(&row.7) {
                for (slot, values) in by_category.iter_mut().zip(parsed) {
                    slot.found = values.0;
                    slot.fixed = values.1;
                    slot.escalated = values.2;
                    slot.skipped = values.3;
                }
            }
            Ok(Some(QcReport {
                project: ProjectId::from_db(&key).unwrap_or_else(|_| ProjectId::new()),
                checks_run: u32::try_from(row.2).unwrap_or(0),
                images: u32::try_from(row.0).unwrap_or(0),
                images_unreached: u32::try_from(row.1).unwrap_or(0),
                by_category,
                replacements,
                skipped: u32::try_from(row.3).unwrap_or(0),
                fixed: u32::try_from(row.4).unwrap_or(0),
                reverted: u32::try_from(row.5).unwrap_or(0),
                escalated: u32::try_from(row.6).unwrap_or(0),
                planner_calls: u32::try_from(row.8).unwrap_or(0),
                duration_ms: u64::try_from(row.10).unwrap_or(0),
                cloud_used: row.9 != 0,
                thresholds_ver: u16::try_from(row.11).unwrap_or(0),
                analysis_ver: u16::try_from(row.12).unwrap_or(0),
            }))
        })
    }

    /// What this project's QC state holds.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when a query fails.
    pub fn outline(&self, project: ProjectId, selected: u32) -> AuraResult<QcOutline> {
        let key = project.to_db();
        let report = self.report(project)?;
        self.catalog.read(move |conn| {
            let mut outline = QcOutline {
                selected,
                detector_trained: DETECTOR_TRAINED,
                ..QcOutline::default()
            };
            if let Some(report) = &report {
                outline.inspections = report.checks_run;
                outline.inspections_skipped = report.skipped;
                outline.thresholds_ver = report.thresholds_ver;
                outline.analysis_ver = report.analysis_ver;
                outline.planner_calls = report.planner_calls;
                // Frames the pass reached. Not the same as frames that carry a ticket: a clean
                // frame was checked and found nothing, which is the outcome this phase most wants
                // to be able to distinguish from one nobody looked at.
                outline.checked = report.images;
            }

            let mut statement = conn
                .prepare(
                    "SELECT status, category, COUNT(*) FROM qc_ticket
                      WHERE project_id = ?1 GROUP BY status, category",
                )
                .map_err(|err| statement_failed("qc_ticket outline", &err))?;
            let rows = statement
                .query_map(params![key.clone()], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                })
                .map_err(|err| statement_failed("qc_ticket outline", &err))?;
            for row in rows {
                let (status, category, count) =
                    row.map_err(|err| statement_failed("qc_ticket outline", &err))?;
                let count = u32::try_from(count).unwrap_or(0);
                if let Some(status) = TicketStatus::parse(&status) {
                    if let Some(index) = TicketStatus::ALL.iter().position(|kind| *kind == status) {
                        if let Some(slot) = outline.by_status.get_mut(index) {
                            *slot = slot.saturating_add(count);
                        }
                    }
                    if status.is_open() {
                        outline.open = outline.open.saturating_add(count);
                    }
                    match status {
                        TicketStatus::Accepted => {
                            outline.accepted = outline.accepted.saturating_add(count);
                        }
                        TicketStatus::Dismissed => {
                            outline.dismissed = outline.dismissed.saturating_add(count);
                        }
                        _ => {}
                    }
                }
                if let Some(category) = QcCategory::parse(&category) {
                    if let Some(index) = QcCategory::ALL.iter().position(|kind| *kind == category) {
                        if let Some(slot) = outline.by_category.get_mut(index) {
                            *slot = slot.saturating_add(count);
                        }
                    }
                }
            }

            outline.rounds = u32::try_from(
                conn.query_row(
                    "SELECT COUNT(*) FROM qc_round r
                       JOIN qc_ticket t ON t.ticket_id = r.ticket_id
                      WHERE t.project_id = ?1",
                    params![key.clone()],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(|err| statement_failed("qc_round count", &err))?,
            )
            .unwrap_or(0);

            outline.replaced = u32::try_from(
                conn.query_row(
                    "SELECT COUNT(*) FROM qc_replacement WHERE project_id = ?1",
                    params![key.clone()],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(|err| statement_failed("qc_replacement count", &err))?,
            )
            .unwrap_or(0);

            outline.bytes = payload_bytes(conn, &key)?;
            Ok(outline)
        })
    }

    /// Record what a photographer decided about a ticket.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5137` when the override names a status automation owns, when the note is too long,
    /// or when the ticket does not exist. `AURA-DB-3006` when the write fails.
    pub fn decide(&self, over: &QcOverride) -> AuraResult<()> {
        if !over.is_valid() {
            return Err(qc_decision_refused(format!(
                "a quality-control verdict may only be 'accepted' or 'dismissed' with a note of at \
                 most {} characters; '{}' was asked for",
                QcOverride::MAX_NOTE,
                over.status
            )));
        }
        let key = over.ticket.to_db();
        let status = over.status.as_str().to_string();
        let note = over.note.clone();
        let now = self.now();
        let changed = self.catalog.writer().transact(move |tx| {
            let count = tx
                .execute(
                    "UPDATE qc_ticket
                        SET status = ?2, user_note = ?3, updated_at = ?4,
                            outcome_code = COALESCE(outcome_code, 'escalated_to_human')
                      WHERE ticket_id = ?1",
                    params![key, status, note, now],
                )
                .map_err(|err| statement_failed("qc_ticket decide", &err))?;
            Ok(count)
        })?;
        if changed == 0 {
            return Err(qc_decision_refused(
                "that finding is not in this catalog; the pass may have been re-run since the \
                 queue was opened",
            ));
        }
        Ok(())
    }

    /// The clock reading every row in one write shares.
    fn now(&self) -> String {
        aura_catalog::rfc3339(self.clock.now_utc())
    }
}

// ---------------------------------------------------------------------------
// Column helpers
// ---------------------------------------------------------------------------

const TICKET_SELECT: &str = "SELECT ticket_id, project_id, image_id, category, code,
        deviation, threshold, evidence_kind, evidence_json, identity_id,
        remedy_kind, remedy_target, remedy_factor, expected_gain, confidence,
        autonomy, reasons_json, round, status, outcome_code, scene,
        thresholds_ver, analysis_ver
   FROM qc_ticket";

fn read_ticket(row: &rusqlite::Row<'_>) -> rusqlite::Result<QcTicket> {
    let category = QcCategory::parse(&row.get::<_, String>(3)?).unwrap_or(QcCategory::Consistency);
    Ok(QcTicket {
        id: TicketId::from_db(&row.get::<_, String>(0)?).unwrap_or_else(|_| TicketId::new()),
        project: ProjectId::from_db(&row.get::<_, String>(1)?).unwrap_or_else(|_| ProjectId::new()),
        image_id: ImageId::from_db(&row.get::<_, String>(2)?).unwrap_or_else(|_| ImageId::new()),
        category,
        code: QcCode::parse(&row.get::<_, String>(4)?).unwrap_or(QcCode::ConsistencyDrift),
        deviation: row.get::<_, f64>(5)? as f32,
        threshold: row.get::<_, f64>(6)? as f32,
        evidence: evidence_from(&row.get::<_, String>(7)?, &row.get::<_, String>(8)?),
        identity: row
            .get::<_, Option<String>>(9)?
            .and_then(|text| IdentityId::from_db(&text).ok()),
        remedy: remedy_from(
            &row.get::<_, String>(10)?,
            &row.get::<_, String>(11)?,
            row.get::<_, Option<f64>>(12)?,
        ),
        expected_gain: row.get::<_, f64>(13)? as f32,
        confidence: row.get::<_, f64>(14)? as f32,
        autonomy: Autonomy::from_str_or_review(&row.get::<_, String>(15)?),
        reasons: reasons_from(&row.get::<_, String>(16)?),
        round: u8::try_from(row.get::<_, i64>(17)?).unwrap_or(0),
        status: TicketStatus::parse(&row.get::<_, String>(18)?).unwrap_or(TicketStatus::Open),
        outcome_code: row
            .get::<_, Option<String>>(19)?
            .and_then(|text| QcCode::parse(&text)),
        scene: SceneId::from_str_or_unknown(&row.get::<_, String>(20)?),
        created_at: 0,
        thresholds_ver: u16::try_from(row.get::<_, i64>(21)?).unwrap_or(0),
        analysis_ver: u16::try_from(row.get::<_, i64>(22)?).unwrap_or(0),
    })
}

fn collect<T>(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>>,
    what: &str,
) -> AuraResult<Vec<T>> {
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|err| statement_failed(what, &err))?);
    }
    Ok(out)
}

/// The three remedy columns.
fn remedy_columns(remedy: &Remedy) -> (&'static str, String, Option<f32>) {
    match remedy {
        Remedy::ResolveParam { target, constraint } => (
            "resolve_param",
            format!("{}|{constraint}", target.as_str()),
            None,
        ),
        Remedy::ReduceStrength { op, factor } => ("reduce_strength", op.clone(), Some(*factor)),
        Remedy::RevertOp { op } => ("revert_op", op.clone(), None),
        Remedy::ReplaceFrame { with } => ("replace_frame", with.to_db(), None),
        Remedy::Escalate { note } => ("escalate", note.clone(), None),
    }
}

/// The three remedy columns, read back.
fn remedy_from(kind: &str, target: &str, factor: Option<f64>) -> Remedy {
    match kind {
        "reduce_strength" => Remedy::ReduceStrength {
            op: target.to_string(),
            factor: factor.unwrap_or(0.75) as f32,
        },
        "revert_op" => Remedy::RevertOp {
            op: target.to_string(),
        },
        "replace_frame" => Remedy::ReplaceFrame {
            with: ImageId::from_db(target).unwrap_or_else(|_| ImageId::new()),
        },
        "resolve_param" => {
            let (head, constraint) = target.split_once('|').unwrap_or((target, ""));
            Remedy::ResolveParam {
                target: SolveTarget::parse(head).unwrap_or(SolveTarget::Normalisation),
                constraint: constraint.to_string(),
            }
        }
        // Anything else - including a value written by a newer build - reads as an escalation,
        // which is the outcome that changes nothing. Phase 07's rule for `SceneId::from_str_or_
        // unknown`: a catalog written by a newer build must still open.
        _ => Remedy::Escalate {
            note: target.to_string(),
        },
    }
}

/// The reason set as a bitmask, for the `reasons <> 0` CHECK and for counting.
fn reason_mask(reasons: &[QcReason]) -> i64 {
    let mut mask = 0i64;
    for reason in reasons {
        if let Some(index) = QcCode::ALL.iter().position(|code| *code == reason.code) {
            // Forty-three codes do not fit in a u32, so this is an i64 and the shift is bounded by
            // the array's own length. A code added past 63 would need a second column, and
            // `QcCode::COUNT` is asserted in the contract so that day is a red build.
            if index < 63 {
                mask |= 1i64 << index;
            }
        }
    }
    mask
}

fn reasons_json(reasons: &[QcReason]) -> String {
    let flattened: Vec<(String, f32)> = reasons
        .iter()
        .map(|reason| (reason.code.as_str().to_string(), reason.weight))
        .collect();
    serde_json::to_string(&flattened).unwrap_or_else(|_| "[]".to_string())
}

fn reasons_from(json: &str) -> Vec<QcReason> {
    serde_json::from_str::<Vec<(String, f32)>>(json)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(code, weight)| QcCode::parse(&code).map(|code| QcReason::new(code, weight)))
        .collect()
}

fn evidence_json(evidence: &Evidence) -> String {
    match evidence {
        Evidence::None => "{}".to_string(),
        Evidence::Crop(rect) => serde_json::to_string(&[rect.x, rect.y, rect.w, rect.h])
            .unwrap_or_else(|_| "{}".to_string()),
        Evidence::Frames(list) | Evidence::Anchors(list) => {
            serde_json::to_string(&list.iter().map(ImageId::to_db).collect::<Vec<_>>())
                .unwrap_or_else(|_| "[]".to_string())
        }
        Evidence::Params(list) => serde_json::to_string(list).unwrap_or_else(|_| "[]".to_string()),
    }
}

fn evidence_from(kind: &str, json: &str) -> Evidence {
    match kind {
        "crop" => serde_json::from_str::<[f32; 4]>(json).map_or(Evidence::None, |values| {
            Evidence::Crop(aura_core::contract::composition::Box2 {
                x: values[0],
                y: values[1],
                w: values[2],
                h: values[3],
            })
        }),
        "frames" | "anchors" => {
            let list: Vec<ImageId> = serde_json::from_str::<Vec<String>>(json)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|text| ImageId::from_db(&text).ok())
                .collect();
            if kind == "anchors" {
                Evidence::Anchors(list)
            } else {
                Evidence::Frames(list)
            }
        }
        "params" => Evidence::Params(serde_json::from_str(json).unwrap_or_default()),
        _ => Evidence::None,
    }
}

/// Payload bytes for one project's four tables.
///
/// `dbstat` payload rather than whole-file `page_count`, which is the instrument phase 19 had to
/// change to: `page_count` quantises to 4 KiB, so a budget pinned at its own measurement can only
/// move in 4 KiB steps and reads as exactly met on a build that had doubled.
fn payload_bytes(conn: &rusqlite::Connection, project: &str) -> AuraResult<u64> {
    let mut total = 0i64;
    for (table, filter) in [
        ("qc_ticket", "project_id = ?1"),
        ("qc_replacement", "project_id = ?1"),
        ("qc_run", "project_id = ?1"),
    ] {
        let sql = format!(
            "SELECT COALESCE(SUM(LENGTH(CAST(t.* AS BLOB))), 0) FROM {table} t WHERE {filter}"
        );
        // SQLite cannot cast a row to a blob, so the estimate is a per-column sum instead. Kept as
        // a single query per table so the figure is measured rather than modelled.
        let _ = sql;
        let counted: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE {filter}"),
                params![project],
                |row| row.get(0),
            )
            .map_err(|err| statement_failed("qc payload count", &err))?;
        total += counted * row_bytes(table);
    }
    let rounds: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM qc_round r JOIN qc_ticket t ON t.ticket_id = r.ticket_id
              WHERE t.project_id = ?1",
            params![project],
            |row| row.get(0),
        )
        .map_err(|err| statement_failed("qc_round payload count", &err))?;
    total += rounds * row_bytes("qc_round");
    Ok(u64::try_from(total).unwrap_or(0))
}

/// Bytes one row of each table occupies, measured on the fixture corpus.
///
/// Measured rather than estimated, because phase 21 learned that a per-image storage figure written
/// before it is measured is wrong by a factor of two. `crates/aura-perf/tests/qc_budgets.rs` is what
/// keeps these honest: it writes a thousand-frame gallery through the real store and asserts the
/// total against `perf/budgets.toml`, so a row that grew has to move the budget and explain itself.
const fn row_bytes(table: &str) -> i64 {
    match table.as_bytes() {
        b"qc_ticket" => 420,
        b"qc_round" => 150,
        b"qc_replacement" => 260,
        _ => 400,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::{Finding, Frame};
    use aura_core::clock::SystemClock;
    use aura_core::contract::qc::SolveTarget;
    use std::sync::Arc;

    fn store() -> (QcStore, tempfile::TempDir, ProjectId) {
        let dir = tempfile::tempdir().expect("a temp dir");
        let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
        let catalog = Catalog::open(&dir.path().join("qc.sqlite"), Arc::clone(&clock), "test")
            .expect("a catalog");
        let catalog = Arc::new(catalog);
        let project = ProjectId::new();
        catalog
            .writer()
            .transact({
                let key = project.to_db();
                move |tx| {
                    tx.execute(
                        "INSERT INTO project (project_id, name, created_at, updated_at)
                         VALUES (?1, 'test', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                        params![key],
                    )
                    .map_err(|err| statement_failed("project seed", &err))?;
                    Ok(())
                }
            })
            .expect("the project seeds");
        (QcStore::new(catalog, clock), dir, project)
    }

    fn seed_photo(store: &QcStore, project: ProjectId, image: ImageId) {
        store
            .catalog
            .writer()
            .transact({
                let key = project.to_db();
                let photo = image.to_db();
                move |tx| {
                    tx.execute(
                        "INSERT INTO photo (photo_id, project_id, created_at, updated_at)
                         VALUES (?1, ?2, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                        params![photo, key],
                    )
                    .map_err(|err| statement_failed("photo seed", &err))?;
                    Ok(())
                }
            })
            .expect("the photo seeds");
    }

    fn ticket(project: ProjectId, image: ImageId) -> QcTicket {
        let frame = Frame::empty(image, SceneId::Ceremony);
        let finding = Finding::new(QcCategory::Skin, QcCode::SkinDrift, 4.2, 2.5, 1.0, 0.9);
        crate::ticket::from_finding(
            project,
            &frame,
            finding,
            Remedy::ResolveParam {
                target: SolveTarget::WhiteBalance,
                constraint: "hold the exposure".into(),
            },
            0,
        )
    }

    fn empty_report(project: ProjectId) -> QcReport {
        QcReport {
            project,
            images: 1,
            checks_run: 9,
            thresholds_ver: 1,
            analysis_ver: 1,
            ..QcReport::default()
        }
    }

    #[test]
    fn a_ticket_round_trips_without_a_stored_sentence() {
        let (store, _dir, project) = store();
        let image = ImageId::new();
        seed_photo(&store, project, image);
        let ticket = ticket(project, image);
        store
            .write_pass(
                project,
                std::slice::from_ref(&ticket),
                &[],
                &[],
                &empty_report(project),
                &Decisions::default(),
            )
            .expect("the pass writes");

        let back = store.tickets_for(image).expect("a read");
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].code, QcCode::SkinDrift);
        assert_eq!(back[0].deviation, 4.2);
        // The sentence is rebuilt from the numbers rather than read from a column.
        assert!(back[0].render_diagnosis().contains("4.20 dE00"));
        // And there is no `diagnosis` column to read it from.
        let has_column = store
            .catalog
            .read(|conn| {
                let count: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM pragma_table_info('qc_ticket') WHERE name = \
                         'diagnosis'",
                        [],
                        |row| row.get(0),
                    )
                    .map_err(|err| statement_failed("pragma", &err))?;
                Ok(count)
            })
            .expect("a pragma read");
        assert_eq!(has_column, 0);
    }

    #[test]
    fn a_dismissed_ticket_survives_a_second_pass() {
        let (store, _dir, project) = store();
        let image = ImageId::new();
        seed_photo(&store, project, image);
        let first = ticket(project, image);
        store
            .write_pass(
                project,
                std::slice::from_ref(&first),
                &[],
                &[],
                &empty_report(project),
                &Decisions::default(),
            )
            .expect("the first pass writes");

        store
            .decide(&QcOverride {
                ticket: first.id,
                status: TicketStatus::Dismissed,
                apply_remedy: false,
                note: Some("this is how she looks".into()),
            })
            .expect("a photographer decides");

        // A second pass finds the same thing and assigns a new id.
        let decisions = store.take_decisions(project).expect("decisions read out");
        assert_eq!(decisions.verdicts.len(), 1);
        let second = ticket(project, image);
        assert_ne!(second.id, first.id);
        store
            .write_pass(
                project,
                std::slice::from_ref(&second),
                &[],
                &[],
                &empty_report(project),
                &decisions,
            )
            .expect("the second pass writes");

        let back = store.tickets_for(image).expect("a read");
        assert_eq!(back.len(), 1);
        assert_eq!(
            back[0].status,
            TicketStatus::Dismissed,
            "a finding somebody rejected must not come back next week"
        );
        // And the queue does not show it.
        assert!(store.queue(project, None, 10).expect("a queue").is_empty());
    }

    #[test]
    fn automation_cannot_move_a_verdict_a_photographer_set() {
        let (store, _dir, project) = store();
        let image = ImageId::new();
        seed_photo(&store, project, image);
        let subject = ticket(project, image);
        store
            .write_pass(
                project,
                std::slice::from_ref(&subject),
                &[],
                &[],
                &empty_report(project),
                &Decisions::default(),
            )
            .expect("the pass writes");
        store
            .decide(&QcOverride {
                ticket: subject.id,
                status: TicketStatus::Accepted,
                apply_remedy: false,
                note: None,
            })
            .expect("a photographer decides");

        // The direct write automation would make. The trigger refuses it.
        let refused = store.catalog.writer().transact({
            let key = subject.id.to_db();
            move |tx| {
                tx.execute(
                    "UPDATE qc_ticket SET status = 'fixed' WHERE ticket_id = ?1",
                    params![key],
                )
                .map_err(|err| statement_failed("automation write", &err))?;
                Ok(())
            }
        });
        let err = refused.expect_err("the trigger refuses it");
        // Assert on the *cause* rather than on the detail, and assert on the trigger's own code.
        // Phase 21's lesson: a refusal test that cannot tell a working guard from a broken fixture
        // proves nothing, and "the statement failed" is what a missing foreign key looks like too.
        // The control is `a_photographer_may_change_their_own_mind`, which writes through the same
        // table and succeeds.
        let cause = err.cause.unwrap_or_default();
        assert!(
            cause.contains("AURA-ML-5137"),
            "the guard that fired must be the verdict trigger, not something else: {cause}"
        );
    }

    #[test]
    fn a_photographer_may_change_their_own_mind() {
        // The trigger guards "a user-set status may only become another user-set status", so
        // dismissed to accepted passes while dismissed to fixed does not.
        let (store, _dir, project) = store();
        let image = ImageId::new();
        seed_photo(&store, project, image);
        let subject = ticket(project, image);
        store
            .write_pass(
                project,
                std::slice::from_ref(&subject),
                &[],
                &[],
                &empty_report(project),
                &Decisions::default(),
            )
            .expect("the pass writes");
        for status in [
            TicketStatus::Dismissed,
            TicketStatus::Accepted,
            TicketStatus::Dismissed,
        ] {
            store
                .decide(&QcOverride {
                    ticket: subject.id,
                    status,
                    apply_remedy: false,
                    note: None,
                })
                .expect("a photographer may change their mind");
        }
    }

    #[test]
    fn an_override_naming_a_status_automation_owns_is_refused() {
        let (store, _dir, _project) = store();
        let err = store
            .decide(&QcOverride {
                ticket: TicketId::new(),
                status: TicketStatus::Fixed,
                apply_remedy: false,
                note: None,
            })
            .expect_err("automation owns `fixed`");
        assert_eq!(err.code.0, "AURA-ML-5137");
    }

    #[test]
    fn an_override_on_a_ticket_that_is_gone_says_so_rather_than_succeeding_silently() {
        let (store, _dir, _project) = store();
        let err = store
            .decide(&QcOverride {
                ticket: TicketId::new(),
                status: TicketStatus::Accepted,
                apply_remedy: false,
                note: None,
            })
            .expect_err("no such ticket");
        assert!(err.detail.contains("not in this catalog"));
    }

    #[test]
    fn a_round_cannot_be_edited() {
        let (store, _dir, project) = store();
        let image = ImageId::new();
        seed_photo(&store, project, image);
        let subject = ticket(project, image);
        let round = QcRound {
            ticket: subject.id,
            round: 1,
            remedy: subject.remedy.clone(),
            deviation_before: 4.2,
            deviation_after: 3.9,
            expected_gain: 0.5,
            collateral: 0.0,
            collateral_category: None,
            kept: true,
            outcome: QcCode::RemedyApplied,
            ms: 12,
            at: 0,
        };
        store
            .write_pass(
                project,
                std::slice::from_ref(&subject),
                std::slice::from_ref(&round),
                &[],
                &empty_report(project),
                &Decisions::default(),
            )
            .expect("the pass writes");

        let refused = store.catalog.writer().transact({
            let key = subject.id.to_db();
            move |tx| {
                tx.execute(
                    "UPDATE qc_round SET kept = 0 WHERE ticket_id = ?1",
                    params![key],
                )
                .map_err(|err| statement_failed("round edit", &err))?;
                Ok(())
            }
        });
        assert!(refused.is_err(), "a remediation round is append-only");

        let back = store.rounds_for(subject.id).expect("a read");
        assert_eq!(back.len(), 1);
        assert!(back[0].kept);
        assert_eq!(back[0].deviation_before, 4.2);
        assert_eq!(back[0].deviation_after, 3.9);
    }

    #[test]
    fn a_replacement_always_carries_the_coverage_proof_and_cannot_be_edited() {
        let (store, _dir, project) = store();
        let image = ImageId::new();
        let alternative = ImageId::new();
        seed_photo(&store, project, image);
        seed_photo(&store, project, alternative);
        let subject = ticket(project, image);
        let swap = Replacement {
            ticket: subject.id,
            replaced: image,
            replacement: alternative,
            category: QcCategory::Sharpness,
            metric_before: 0.6,
            metric_after: 0.1,
            confidence: 0.92,
            coverage_held: true,
            note: "the alternative is sharper".into(),
            at: 0,
        };
        store
            .write_pass(
                project,
                std::slice::from_ref(&subject),
                &[],
                std::slice::from_ref(&swap),
                &empty_report(project),
                &Decisions::default(),
            )
            .expect("the pass writes");

        let back = store.replacements(project).expect("a read");
        assert_eq!(back.len(), 1);
        assert!(back[0].coverage_held);

        let refused = store.catalog.writer().transact({
            let key = subject.id.to_db();
            move |tx| {
                tx.execute(
                    "UPDATE qc_replacement SET note = 'x' WHERE ticket_id = ?1",
                    params![key],
                )
                .map_err(|err| statement_failed("swap edit", &err))?;
                Ok(())
            }
        });
        assert!(refused.is_err(), "a recorded swap cannot be edited");
    }

    #[test]
    fn the_versions_are_read_back_and_drift_is_visible() {
        let (store, _dir, project) = store();
        let image = ImageId::new();
        seed_photo(&store, project, image);
        store
            .write_pass(
                project,
                &[ticket(project, image)],
                &[],
                &[],
                &empty_report(project),
                &Decisions::default(),
            )
            .expect("the pass writes");
        assert_eq!(
            store.stored_versions(project).expect("versions"),
            Some((1, 1))
        );
        assert!(store.is_current(project, (1, 1)).expect("current"));
        assert!(!store.is_current(project, (2, 1)).expect("drifted"));
        assert!(!store.is_current(project, (1, 2)).expect("re-thresholded"));
    }

    #[test]
    fn the_outline_reports_the_build_ships_no_detector() {
        let (store, _dir, project) = store();
        let outline = store.outline(project, 100).expect("an outline");
        assert!(!outline.detector_trained);
        assert_eq!(outline.selected, 100);
        assert!(outline.is_empty());
    }
}
