//! Migration 13's three tables, and the two rules that live in the SQL rather than around
//! it.
//!
//! ## Append-only, and the database says so
//!
//! There is no `update` method in this file, and there could not be one: migration 13
//! carries a trigger that aborts every `UPDATE` on `decisions`. A correction is
//! [`Ledger::append`] with `supersedes` pointing backwards.
//!
//! That is unlike every store since phase 06, all of which upsert. The difference is what is
//! being stored. A measurement is a claim about a photograph and re-measuring it should
//! replace the claim; a decision is a thing that *happened*, and the second time a
//! photographer asks why four hundred frames are missing from a wedding delivered eight
//! months ago, the answer has to be what the product did then rather than what it would do
//! now.
//!
//! ## Compaction is the only thing that deletes, and it is bounded
//!
//! Section 6.3: "compaction keeps the newest decision per subject plus all user overrides".
//! [`Compaction`] is that policy, written once, with the two things it may never remove
//! stated as filters in the SQL rather than as care in the caller: a decision nothing
//! supersedes, and a decision whose source is the photographer.
//!
//! ## Reasons store a code, not a sentence
//!
//! Phase 09's decision, applied a fourth time and against the largest table it has met.
//! `text` is written only when the sentence differs from the registry's template - which is
//! true of the reasons that name a rule, a rival frame, a shutter speed or a parameter, and
//! false of the ordinary ones.

use std::collections::BTreeMap;
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::contract::ids::DecisionId;
use aura_core::contract::ledger::{
    Autonomy, DecisionKind, DecisionSource, DecisionSubject, Evidence, LedgerDecision,
    LedgerOutline, LedgerReason,
};
use aura_core::errors::db::statement_failed;
use aura_core::{AuraResult, Box2, ProjectId};
use rusqlite::{params, OptionalExtension};

use crate::reason::Catalog as ReasonCatalog;

/// Migration 13's tables, as one handle.
#[derive(Debug)]
pub struct Ledger {
    catalog: Arc<Catalog>,
    clock: Arc<dyn Clock>,
    reasons: ReasonCatalog,
}

impl Ledger {
    /// A ledger over one catalog.
    #[must_use]
    pub fn new(catalog: Arc<Catalog>, clock: Arc<dyn Clock>) -> Self {
        Self {
            catalog,
            clock,
            reasons: ReasonCatalog::shipped(),
        }
    }

    /// The reason registry this ledger validates against.
    #[must_use]
    pub const fn registry(&self) -> &ReasonCatalog {
        &self.reasons
    }

    /// Write one decision and its reasons, in one transaction.
    ///
    /// The caller has already validated the reasons; this method writes what it is given and
    /// leaves `text` out whenever the sentence is the registry's own.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when any statement fails. The transaction rolls back, so a decision
    /// whose reasons could not be written is a decision that was not recorded at all - which
    /// is the correct outcome for invariant 2 and the reason the two writes share one
    /// transaction.
    pub fn append(&self, decision: &LedgerDecision) -> AuraResult<DecisionId> {
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let row = Row::from(decision, &self.reasons);
        let id = decision.id;

        self.catalog.writer().transact(move |conn| {
            conn.execute(
                "INSERT INTO decisions (
                     decision_id, project_id, kind, subject_kind, subject_id,
                     inputs_hash, outputs_json,
                     raw_confidence, calibrated_confidence, calibration_ver,
                     autonomy, source, model_versions, config_versions,
                     ms, created_ts, created_at, supersedes, reason_count)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                         ?16, ?17, ?18, ?19)",
                params![
                    row.decision_id,
                    row.project_id,
                    row.kind,
                    row.subject_kind,
                    row.subject_id,
                    row.inputs_hash,
                    row.outputs_json,
                    row.raw_confidence,
                    row.calibrated_confidence,
                    row.calibration_ver,
                    row.autonomy,
                    row.source,
                    row.model_versions,
                    row.config_versions,
                    row.ms,
                    row.created_ts,
                    now,
                    row.supersedes,
                    row.reasons.len(),
                ],
            )
            .map_err(|e| statement_failed("could not record the decision", &e))?;

            let mut statement = conn
                .prepare(
                    "INSERT INTO decision_reasons
                         (decision_id, ix, code, text, weight, evidence_kind, evidence)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                )
                .map_err(|e| statement_failed("could not prepare the reason insert", &e))?;
            for (index, reason) in row.reasons.iter().enumerate() {
                statement
                    .execute(params![
                        row.decision_id,
                        index,
                        reason.code,
                        reason.text,
                        reason.weight,
                        reason.evidence_kind,
                        reason.evidence,
                    ])
                    .map_err(|e| statement_failed("could not record a reason", &e))?;
            }
            Ok(())
        })?;
        Ok(id)
    }

    /// One decision, by id.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the ledger cannot be read.
    pub fn get(&self, id: DecisionId) -> AuraResult<Option<LedgerDecision>> {
        let key = id.to_db();
        let registry = self.reasons.clone();
        self.catalog.read(move |conn| {
            let row: Option<StoredRow> = conn
                .query_row(
                    &format!("SELECT {COLUMNS} FROM decisions WHERE decision_id = ?1"),
                    params![key],
                    read_row,
                )
                .optional()
                .map_err(|e| statement_failed("could not read the decision", &e))?;
            let Some(row) = row else {
                return Ok(None);
            };
            let reasons = read_reasons(conn, &row.decision_id, &registry)?;
            Ok(row.into_decision(reasons))
        })
    }

    /// Every decision about one subject, newest first.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the ledger cannot be read.
    pub fn history(&self, subject: DecisionSubject) -> AuraResult<Vec<LedgerDecision>> {
        let kind = subject.kind_str().to_string();
        let id = subject.id_str();
        let registry = self.reasons.clone();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(&format!(
                    "SELECT {COLUMNS} FROM decisions
                     WHERE subject_kind = ?1 AND subject_id = ?2
                     ORDER BY created_ts DESC, decision_id DESC"
                ))
                .map_err(|e| statement_failed("could not prepare the history read", &e))?;
            let rows: Vec<StoredRow> = statement
                .query_map(params![kind, id], read_row)
                .map_err(|e| statement_failed("could not read the history", &e))?
                .filter_map(Result::ok)
                .collect();

            let mut out = Vec::with_capacity(rows.len());
            for row in rows {
                let reasons = read_reasons(conn, &row.decision_id, &registry)?;
                if let Some(decision) = row.into_decision(reasons) {
                    out.push(decision);
                }
            }
            Ok(out)
        })
    }

    /// Every decision in one project, oldest first. What the support bundle exports.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the ledger cannot be read.
    pub fn project(&self, project: ProjectId) -> AuraResult<Vec<LedgerDecision>> {
        let key = project.to_db();
        let registry = self.reasons.clone();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(&format!(
                    "SELECT {COLUMNS} FROM decisions
                     WHERE project_id = ?1
                     ORDER BY created_ts ASC, decision_id ASC"
                ))
                .map_err(|e| statement_failed("could not prepare the ledger read", &e))?;
            let rows: Vec<StoredRow> = statement
                .query_map(params![key], read_row)
                .map_err(|e| statement_failed("could not read the ledger", &e))?
                .filter_map(Result::ok)
                .collect();

            let mut out = Vec::with_capacity(rows.len());
            for row in rows {
                let reasons = read_reasons(conn, &row.decision_id, &registry)?;
                if let Some(decision) = row.into_decision(reasons) {
                    out.push(decision);
                }
            }
            Ok(out)
        })
    }

    /// The decisions in one project that are waiting for a person, newest first.
    ///
    /// `band` narrows it to one autonomy band; `None` means both bands that need review, in
    /// the order the queue should be worked - the ones AURA did not do, then the ones it did
    /// and is not sure about.
    ///
    /// Superseded decisions are excluded, and that is the one place in this file where a
    /// read hides a row: a review queue is a list of things to do, and a decision that a
    /// later one replaced is not one of them.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the ledger cannot be read.
    pub fn review_queue(
        &self,
        project: ProjectId,
        band: Option<Autonomy>,
        limit: u32,
    ) -> AuraResult<Vec<LedgerDecision>> {
        let key = project.to_db();
        let wanted: Vec<&'static str> = match band {
            Some(one) => vec![one.as_str()],
            None => vec![Autonomy::RequireReview.as_str(), Autonomy::Suggest.as_str()],
        };
        let registry = self.reasons.clone();
        self.catalog.read(move |conn| {
            let mut out = Vec::new();
            for slug in &wanted {
                let mut statement = conn
                    .prepare(&format!(
                        "SELECT {COLUMNS} FROM decisions
                         WHERE project_id = ?1 AND autonomy = ?2
                           AND decision_id NOT IN (SELECT supersedes FROM decisions
                                                   WHERE project_id = ?1
                                                     AND supersedes IS NOT NULL)
                         ORDER BY created_ts DESC, decision_id DESC
                         LIMIT ?3"
                    ))
                    .map_err(|e| statement_failed("could not prepare the review queue", &e))?;
                let rows: Vec<StoredRow> = statement
                    .query_map(params![key, slug, limit], read_row)
                    .map_err(|e| statement_failed("could not read the review queue", &e))?
                    .filter_map(Result::ok)
                    .collect();
                for row in rows {
                    let reasons = read_reasons(conn, &row.decision_id, &registry)?;
                    if let Some(decision) = row.into_decision(reasons) {
                        out.push(decision);
                    }
                }
            }
            out.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
            Ok(out)
        })
    }

    /// What a project's ledger holds.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the ledger cannot be read.
    pub fn outline(&self, project: ProjectId) -> AuraResult<LedgerOutline> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut outline = LedgerOutline::default();

            let mut statement = conn
                .prepare(
                    "SELECT kind, autonomy, source, calibration_ver, reason_count,
                            length(outputs_json)
                     FROM decisions WHERE project_id = ?1",
                )
                .map_err(|e| statement_failed("could not prepare the ledger outline", &e))?;
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read the ledger outline", &e))?;
            let mut payload = 0u64;
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read the ledger outline", &e))?
            {
                let kind: String = row.get(0).unwrap_or_default();
                let autonomy: String = row.get(1).unwrap_or_default();
                let source: String = row.get(2).unwrap_or_default();
                let calibration: i64 = row.get(3).unwrap_or(0);
                let reasons: i64 = row.get(4).unwrap_or(0);
                let bytes: i64 = row.get(5).unwrap_or(0);

                outline.decisions = outline.decisions.saturating_add(1);
                if reasons > 0 {
                    outline.explained = outline.explained.saturating_add(1);
                }
                outline.reasons = outline
                    .reasons
                    .saturating_add(u32::try_from(reasons).unwrap_or(0));
                payload = payload.saturating_add(u64::try_from(bytes).unwrap_or(0));
                if calibration > 0 {
                    outline.calibrated = outline.calibrated.saturating_add(1);
                    outline.calibration_ver = outline
                        .calibration_ver
                        .max(u16::try_from(calibration).unwrap_or(u16::MAX));
                }
                if let Some(slot) = DecisionKind::parse(&kind)
                    .and_then(|parsed| index_of(&DecisionKind::ALL, &parsed))
                    .and_then(|ix| outline.by_kind.get_mut(ix))
                {
                    *slot = slot.saturating_add(1);
                }
                if let Some(slot) =
                    index_of(&Autonomy::ALL, &Autonomy::from_str_or_review(&autonomy))
                        .and_then(|ix| outline.by_autonomy.get_mut(ix))
                {
                    *slot = slot.saturating_add(1);
                }
                if let Some(slot) = index_of(
                    &DecisionSource::ALL,
                    &DecisionSource::from_str_or_local(&source),
                )
                .and_then(|ix| outline.by_source.get_mut(ix))
                {
                    *slot = slot.saturating_add(1);
                }
            }

            let superseded: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM decisions
                     WHERE project_id = ?1
                       AND decision_id IN (SELECT supersedes FROM decisions
                                           WHERE project_id = ?1 AND supersedes IS NOT NULL)",
                    params![key],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            outline.superseded = u32::try_from(superseded).unwrap_or(0);
            outline.current = outline.decisions.saturating_sub(outline.superseded);

            let evidenced: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM decision_reasons r
                     JOIN decisions d ON d.decision_id = r.decision_id
                     WHERE d.project_id = ?1 AND r.evidence_kind <> 'none'",
                    params![key],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            outline.evidenced = u32::try_from(evidenced).unwrap_or(0);

            // The reason payload, measured rather than estimated. Section 11's budget is
            // about what the ledger costs a photographer's disk, so the figure counts the
            // stored text and evidence as well as the decision rows themselves.
            let reason_bytes: i64 = conn
                .query_row(
                    "SELECT COALESCE(SUM(length(r.code)
                                       + COALESCE(length(r.text), 0)
                                       + COALESCE(length(r.evidence), 0)
                                       + 24), 0)
                     FROM decision_reasons r
                     JOIN decisions d ON d.decision_id = r.decision_id
                     WHERE d.project_id = ?1",
                    params![key],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            outline.bytes = payload
                .saturating_add(u64::try_from(reason_bytes).unwrap_or(0))
                .saturating_add(u64::from(outline.decisions).saturating_mul(ROW_OVERHEAD_BYTES));

            Ok(outline)
        })
    }

    /// Apply the compaction policy, returning how many rows went.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the compaction cannot run. It is one transaction, so a failure
    /// leaves the ledger exactly as it was.
    pub fn compact(&self, project: ProjectId, policy: Compaction) -> AuraResult<u32> {
        let key = project.to_db();
        self.catalog.writer().transact(move |conn| {
            // What survives, stated as a filter rather than as care in a loop:
            //
            //   * anything the photographer decided - `source = 'user'`;
            //   * anything nothing has superseded - the newest decision per subject;
            //   * anything newer than the retention window.
            //
            // Everything else is a superseded row that a later row already points past.
            let removed = conn
                .execute(
                    "DELETE FROM decisions
                     WHERE project_id = ?1
                       AND source <> 'user'
                       AND created_ts < ?2
                       AND decision_id IN (SELECT supersedes FROM decisions
                                           WHERE project_id = ?1 AND supersedes IS NOT NULL)",
                    params![key, policy.older_than_ts],
                )
                .map_err(|e| statement_failed("could not compact the ledger", &e))?;
            Ok(u32::try_from(removed).unwrap_or(u32::MAX))
        })
    }

    /// Which project a photograph belongs to.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the catalog cannot be read.
    pub fn project_of_photo(&self, photo: aura_core::PhotoId) -> AuraResult<Option<ProjectId>> {
        let key = photo.to_db();
        self.catalog.read(move |conn| {
            let text: Option<String> = conn
                .query_row(
                    "SELECT project_id FROM photo WHERE photo_id = ?1",
                    params![key],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| statement_failed("could not find the photograph's project", &e))?;
            Ok(text.and_then(|value| ProjectId::from_db(&value).ok()))
        })
    }
}

/// What compaction may remove.
///
/// Section 6.3's policy as two numbers a caller can see. The default keeps everything from
/// the last ninety days, which is longer than any support case takes and shorter than the
/// life of a catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Compaction {
    /// Superseded rows older than this timestamp may go.
    pub older_than_ts: i64,
}

impl Compaction {
    /// Ninety days in milliseconds. The retention window.
    pub const RETENTION_MS: i64 = 90 * 24 * 60 * 60 * 1_000;

    /// The policy at a given now.
    ///
    /// `now` is an argument rather than a clock call for the workspace rule: nothing outside
    /// `main.rs` reads an ambient clock, and a compaction whose cut-off moved between two
    /// test runs would be a compaction no test could pin.
    #[must_use]
    pub const fn at(now_ms: i64) -> Self {
        Self {
            older_than_ts: now_ms.saturating_sub(Self::RETENTION_MS),
        }
    }

    /// Remove every superseded non-user row, whatever its age.
    ///
    /// What the size test uses, and what a support engineer would run on a catalog that had
    /// grown past its budget.
    #[must_use]
    pub const fn everything() -> Self {
        Self {
            older_than_ts: i64::MAX,
        }
    }
}

/// Bytes of `SQLite` overhead attributed to one decision row.
///
/// A row's own columns are measured; this covers the page and index cost that
/// `length(outputs_json)` cannot see. Chosen to match what
/// `crates/aura-perf/tests/ledger_budgets.rs` measures against real page accounting, so
/// that the outline's figure and the budget's figure describe the same thing.
const ROW_OVERHEAD_BYTES: u64 = 96;

const COLUMNS: &str = "decision_id, project_id, kind, subject_kind, subject_id, inputs_hash, \
                       outputs_json, raw_confidence, calibrated_confidence, calibration_ver, \
                       autonomy, source, model_versions, config_versions, ms, created_ts, \
                       supersedes";

/// One decision as the catalog holds it.
struct StoredRow {
    decision_id: String,
    project_id: String,
    kind: String,
    subject_kind: String,
    subject_id: String,
    inputs_hash: i64,
    outputs_json: String,
    raw_confidence: f64,
    calibrated_confidence: f64,
    calibration_ver: i64,
    autonomy: String,
    source: String,
    model_versions: String,
    config_versions: String,
    ms: i64,
    created_ts: i64,
    supersedes: Option<String>,
}

impl StoredRow {
    /// The contract shape, or `None` when the row cannot be understood.
    ///
    /// `None` rather than a default for the ids: a decision whose subject does not parse is
    /// a decision about nothing, and returning it with a fabricated subject would put a row
    /// in a support bundle that points at a photograph that does not exist.
    fn into_decision(self, reasons: Vec<LedgerReason>) -> Option<LedgerDecision> {
        let id = DecisionId::from_db(&self.decision_id).ok()?;
        let project = ProjectId::from_db(&self.project_id).ok()?;
        let kind = DecisionKind::parse(&self.kind)?;
        let subject = DecisionSubject::parse(&self.subject_kind, &self.subject_id)?;
        Some(LedgerDecision {
            id,
            project,
            kind,
            subject,
            inputs_hash: u64::from_ne_bytes(self.inputs_hash.to_ne_bytes()),
            outputs_json: self.outputs_json,
            reasons,
            raw_confidence: self.raw_confidence as f32,
            calibrated_confidence: self.calibrated_confidence as f32,
            autonomy: Autonomy::from_str_or_review(&self.autonomy),
            source: DecisionSource::from_str_or_local(&self.source),
            model_versions: parse_versions(&self.model_versions),
            config_versions: parse_versions(&self.config_versions),
            calibration_ver: u16::try_from(self.calibration_ver).unwrap_or(0),
            ms: u32::try_from(self.ms).unwrap_or(u32::MAX),
            created_at: self.created_ts,
            supersedes: self
                .supersedes
                .as_deref()
                .and_then(|text| DecisionId::from_db(text).ok()),
        })
    }
}

fn read_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredRow> {
    Ok(StoredRow {
        decision_id: row.get(0)?,
        project_id: row.get(1)?,
        kind: row.get(2)?,
        subject_kind: row.get(3)?,
        subject_id: row.get(4)?,
        inputs_hash: row.get(5)?,
        outputs_json: row.get(6)?,
        raw_confidence: row.get(7)?,
        calibrated_confidence: row.get(8)?,
        calibration_ver: row.get(9)?,
        autonomy: row.get(10)?,
        source: row.get(11)?,
        model_versions: row.get(12)?,
        config_versions: row.get(13)?,
        ms: row.get(14)?,
        created_ts: row.get(15)?,
        supersedes: row.get(16)?,
    })
}

/// One decision's reasons, in the order the deciding code ranked them.
fn read_reasons(
    conn: &rusqlite::Connection,
    decision_id: &str,
    registry: &ReasonCatalog,
) -> AuraResult<Vec<LedgerReason>> {
    let mut statement = conn
        .prepare(
            "SELECT code, text, weight, evidence_kind, evidence
             FROM decision_reasons WHERE decision_id = ?1 ORDER BY ix ASC",
        )
        .map_err(|e| statement_failed("could not prepare the reason read", &e))?;
    let mut cursor = statement
        .query(params![decision_id])
        .map_err(|e| statement_failed("could not read the reasons", &e))?;

    let mut out = Vec::new();
    while let Some(row) = cursor
        .next()
        .map_err(|e| statement_failed("could not read the reasons", &e))?
    {
        let code: String = row.get(0).unwrap_or_default();
        let stored_text: Option<String> = row.get(1).ok().flatten();
        let weight: f64 = row.get(2).unwrap_or(0.0);
        let evidence_kind: String = row.get(3).unwrap_or_else(|_| "none".to_string());
        let evidence: Option<String> = row.get(4).ok().flatten();

        // The stored sentence when there is one, and the registry's own template when there
        // is not. This is the line that makes "a release can improve the copy without
        // rewriting a catalog" true rather than aspirational.
        let text = stored_text.unwrap_or_else(|| registry.template(&code).to_string());
        out.push(LedgerReason {
            code,
            text,
            weight: weight as f32,
            evidence: decode_evidence(&evidence_kind, evidence.as_deref()),
        });
    }
    Ok(out)
}

/// One reason as the catalog holds it.
struct ReasonRow {
    code: String,
    text: Option<String>,
    weight: f32,
    evidence_kind: &'static str,
    evidence: Option<String>,
}

/// One decision, encoded for the two inserts.
struct Row {
    decision_id: String,
    project_id: String,
    kind: &'static str,
    subject_kind: &'static str,
    subject_id: String,
    inputs_hash: i64,
    outputs_json: String,
    raw_confidence: f64,
    calibrated_confidence: f64,
    calibration_ver: i64,
    autonomy: &'static str,
    source: &'static str,
    model_versions: String,
    config_versions: String,
    ms: i64,
    created_ts: i64,
    supersedes: Option<String>,
    reasons: Vec<ReasonRow>,
}

impl Row {
    fn from(decision: &LedgerDecision, registry: &ReasonCatalog) -> Self {
        Self {
            decision_id: decision.id.to_db(),
            project_id: decision.project.to_db(),
            kind: decision.kind.as_str(),
            subject_kind: decision.subject.kind_str(),
            subject_id: decision.subject.id_str(),
            inputs_hash: i64::from_ne_bytes(decision.inputs_hash.to_ne_bytes()),
            outputs_json: decision.outputs_json.clone(),
            raw_confidence: f64::from(decision.raw_confidence),
            calibrated_confidence: f64::from(decision.calibrated_confidence),
            calibration_ver: i64::from(decision.calibration_ver),
            autonomy: decision.autonomy.as_str(),
            source: decision.source.as_str(),
            model_versions: encode_versions(&decision.model_versions),
            config_versions: encode_versions(&decision.config_versions),
            ms: i64::from(decision.ms),
            created_ts: decision.created_at,
            supersedes: decision.supersedes.map(|id| id.to_db()),
            reasons: decision
                .reasons
                .iter()
                .map(|reason| ReasonRow {
                    code: reason.code.clone(),
                    text: if reason.text == registry.template(&reason.code) {
                        None
                    } else {
                        Some(reason.text.clone())
                    },
                    weight: reason.weight,
                    evidence_kind: reason.evidence.kind_str(),
                    evidence: encode_evidence(&reason.evidence),
                })
                .collect(),
        }
    }
}

/// `[[name, version], ...]`, sorted by name.
fn encode_versions(versions: &[(String, u16)]) -> String {
    let ordered: BTreeMap<&str, u16> = versions
        .iter()
        .map(|(name, version)| (name.as_str(), *version))
        .collect();
    let parts: Vec<String> = ordered
        .into_iter()
        .map(|(name, version)| format!("[{},{version}]", crate::decision::json_string(name)))
        .collect();
    format!("[{}]", parts.join(","))
}

/// The inverse, tolerant of anything it cannot understand.
///
/// A version list that will not parse costs a support engineer a line in a report; refusing
/// the whole decision over it would cost them the decision.
fn parse_versions(text: &str) -> Vec<(String, u16)> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    let Some(entries) = value.as_array() else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| {
            let pair = entry.as_array()?;
            let name = pair.first()?.as_str()?.to_string();
            let version = u16::try_from(pair.get(1)?.as_u64()?).ok()?;
            Some((name, version))
        })
        .collect()
}

/// Evidence as the catalog holds it, or `None` for the `none` kind.
fn encode_evidence(evidence: &Evidence) -> Option<String> {
    match evidence {
        Evidence::None => None,
        Evidence::Crop(area) => Some(format!(
            "{{\"x\":{:.4},\"y\":{:.4},\"w\":{:.4},\"h\":{:.4}}}",
            area.x, area.y, area.w, area.h
        )),
        Evidence::Frames(ids) => {
            let parts: Vec<String> = ids
                .iter()
                .map(|id| crate::decision::json_string(&id.to_db()))
                .collect();
            Some(format!("[{}]", parts.join(",")))
        }
        Evidence::Params(pairs) => {
            let parts: Vec<String> = pairs
                .iter()
                .map(|(name, value)| {
                    format!("[{},{:.4}]", crate::decision::json_string(name), value)
                })
                .collect();
            Some(format!("[{}]", parts.join(",")))
        }
    }
}

/// The inverse. Anything unreadable becomes [`Evidence::None`], because an empty rectangle
/// drawn over a photograph and called proof is worse than no rectangle.
fn decode_evidence(kind: &str, payload: Option<&str>) -> Evidence {
    let Some(text) = payload else {
        return Evidence::None;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return Evidence::None;
    };
    match kind {
        "crop" => {
            let get = |key: &str| value.get(key).and_then(serde_json::Value::as_f64);
            match (get("x"), get("y"), get("w"), get("h")) {
                (Some(x), Some(y), Some(w), Some(h)) => Evidence::Crop(
                    Box2 {
                        x: x as f32,
                        y: y as f32,
                        w: w as f32,
                        h: h as f32,
                    }
                    .clamped(),
                ),
                _ => Evidence::None,
            }
        }
        "frames" => {
            let Some(entries) = value.as_array() else {
                return Evidence::None;
            };
            let ids: Vec<aura_core::PhotoId> = entries
                .iter()
                .filter_map(|entry| aura_core::PhotoId::from_db(entry.as_str()?).ok())
                .collect();
            if ids.is_empty() {
                Evidence::None
            } else {
                Evidence::Frames(ids)
            }
        }
        "params" => {
            let Some(entries) = value.as_array() else {
                return Evidence::None;
            };
            let pairs: Vec<(String, f32)> = entries
                .iter()
                .filter_map(|entry| {
                    let pair = entry.as_array()?;
                    Some((
                        pair.first()?.as_str()?.to_string(),
                        pair.get(1)?.as_f64()? as f32,
                    ))
                })
                .collect();
            if pairs.is_empty() {
                Evidence::None
            } else {
                Evidence::Params(pairs)
            }
        }
        _ => Evidence::None,
    }
}

/// Where a value sits in a fixed vocabulary.
fn index_of<T: PartialEq, const N: usize>(all: &[T; N], value: &T) -> Option<usize> {
    all.iter().position(|candidate| candidate == value)
}
