//! Migration 30's learning half: corrections, updates, snapshots and consent.
//!
//! ## The correction table's foreign key is the guarantee
//!
//! `learn_correction.decision_id` is `NOT NULL REFERENCES decisions(decision_id)`. Everything
//! [`crate::attribute`] checks in code, the schema checks again - which matters here more than in
//! most stores, because the cost of getting it wrong is not a bad row but a profile that has
//! learned from nothing and says it has.
//!
//! ## `adopted` is never written by an INSERT
//!
//! `learn_update_no_self_adopt` refuses one. [`LearnStore::adopt`] is a separate UPDATE, and it is
//! the only statement in this crate that sets the column.

use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::contract::ids::ProfileId;
use aura_core::contract::learn::{
    AbComparison, AbRow, Consent, Correction, CorrectionBucket, CorrectionContext, LearnOutline,
    Learnable, LearningUpdate,
};
use aura_core::contract::ledger::DecisionKind;
use aura_core::contract::scene::SceneId;
use aura_core::errors::db::statement_failed;
use aura_core::{AuraResult, ProjectId};
use rusqlite::{params, OptionalExtension};

use crate::aggregate::Sample;
use crate::errors::rollback_failed;

/// One catalog, wrapped.
#[derive(Debug, Clone)]
pub struct LearnStore {
    catalog: Arc<Catalog>,
    clock: Arc<dyn Clock>,
}

impl LearnStore {
    /// Wrap a catalog.
    #[must_use]
    pub fn new(catalog: Arc<Catalog>) -> Self {
        let clock = Arc::clone(catalog.clock());
        Self { catalog, clock }
    }

    /// Write one correction.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3002` when the row cannot be written - including when its `decision_id` names no
    /// decision, which the foreign key refuses.
    pub fn write_correction(
        &self,
        correction: &Correction,
        context: &CorrectionContext,
        subject_close: bool,
    ) -> AuraResult<()> {
        let id = format!(
            "cor_{}_{}",
            correction.decision_id.to_db(),
            context.learnable.as_str()
        );
        let c = correction.clone();
        let ctx = context.clone();
        self.catalog.writer().with(move |conn| {
            conn.execute(
                "INSERT OR REPLACE INTO learn_correction (correction_id, decision_id, project_id,
                     photo_id, kind, learnable, scene, identity_id, subject_close, before_json,
                     after_json, magnitude, held_out, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    id,
                    c.decision_id.to_db(),
                    ctx.project.to_db(),
                    ctx.image.to_db(),
                    c.kind.as_str(),
                    ctx.learnable.as_str(),
                    c.scene.as_str(),
                    c.identity.map(|i| i.to_db()),
                    i64::from(subject_close),
                    c.before_json,
                    c.after_json,
                    f64::from(c.magnitude),
                    i64::from(ctx.held_out),
                    c.created_at
                ],
            )
            .map_err(|e| statement_failed("learn_correction", &e))?;
            Ok(())
        })
    }

    /// Every bucket with at least one correction, and the samples in it.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    pub fn buckets(&self) -> AuraResult<Vec<(CorrectionBucket, Vec<Sample>)>> {
        self.catalog.read(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT kind, learnable, scene, subject_close, decision_id, project_id,
                            magnitude
                     FROM learn_correction
                     ORDER BY learnable, scene, subject_close, correction_id",
                )
                .map_err(|e| statement_failed("learn_correction", &e))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, f64>(6)?,
                    ))
                })
                .map_err(|e| statement_failed("learn_correction", &e))?;

            let mut out: Vec<(CorrectionBucket, Vec<Sample>)> = Vec::new();
            for row in rows.flatten() {
                let (kind, learnable, scene, close, decision, project, magnitude) = row;
                // A slug this build does not know is a correction a newer release wrote. It is
                // kept and does not count, which is `AURA-LRN-11002`'s whole content.
                let (Ok(learnable), Ok(decision)) = (
                    Learnable::parse(&learnable),
                    aura_core::contract::ids::DecisionId::from_db(&decision),
                ) else {
                    continue;
                };
                let bucket = CorrectionBucket {
                    kind: parse_kind(&kind),
                    scene: SceneId::from_str_or_unknown(&scene),
                    learnable,
                    subject_close: close != 0,
                };
                let sample = Sample {
                    decision,
                    project: project_key(&project),
                    magnitude: magnitude as f32,
                };
                match out.iter_mut().find(|(b, _)| *b == bucket) {
                    Some((_, samples)) => samples.push(sample),
                    None => out.push((bucket, vec![sample])),
                }
            }
            Ok(out)
        })
    }

    /// Store a candidate update and its comparison rows.
    ///
    /// Never adopted. `learn_update_no_self_adopt` refuses an INSERT that arrives already adopted,
    /// so this cannot write one even by mistake.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3002` when the rows cannot be written.
    pub fn write_candidate(
        &self,
        update: &LearningUpdate,
        comparison: &AbComparison,
    ) -> AuraResult<String> {
        let update_id = format!("upd_{}_{}", update.profile_id.to_db(), update.to_version);
        let at = aura_catalog::rfc3339(self.clock.now_utc());
        let id = update_id.clone();
        let u = update.clone();
        let c = comparison.clone();
        let held = comparison.held_out;
        let summary =
            serde_json::to_string(&update.diff_summary).unwrap_or_else(|_| "[]".to_owned());

        self.catalog.writer().with(move |conn| {
            let tx = conn
                .transaction()
                .map_err(|e| statement_failed("learn_update", &e))?;
            tx.execute(
                "INSERT OR REPLACE INTO learn_update (update_id, profile_id, from_version,
                     to_version, corrections_used, held_out_used, current_error, candidate_error,
                     expected_improvement, diff_summary, adopted, computed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0, ?11)",
                params![
                    id,
                    u.profile_id.to_db(),
                    u.from_version,
                    u.to_version,
                    u.corrections_used,
                    held,
                    f64::from(c.current_error),
                    f64::from(c.candidate_error),
                    f64::from(u.expected_improvement),
                    summary,
                    at
                ],
            )
            .map_err(|e| statement_failed("learn_update", &e))?;
            tx.execute(
                "DELETE FROM learn_update_row WHERE update_id = ?1",
                params![id],
            )
            .map_err(|e| statement_failed("learn_update_row", &e))?;
            for row in &c.rows {
                tx.execute(
                    "INSERT INTO learn_update_row (update_id, learnable, scene, current_value,
                         candidate_value, corrections, summary)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        id,
                        row.learnable.as_str(),
                        row.scene.as_str(),
                        f64::from(row.current),
                        f64::from(row.candidate),
                        row.corrections,
                        row.summary
                    ],
                )
                .map_err(|e| statement_failed("learn_update_row", &e))?;
            }
            tx.commit()
                .map_err(|e| statement_failed("learn_update", &e))?;
            Ok(())
        })?;
        Ok(update_id)
    }

    /// A profile's current candidate, with its comparison.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    pub fn candidate(
        &self,
        profile: ProfileId,
    ) -> AuraResult<Option<(LearningUpdate, AbComparison)>> {
        let key = profile.to_db();
        self.catalog.read(move |conn| {
            let Some((
                id,
                from,
                to,
                used,
                held,
                current_err,
                cand_err,
                improvement,
                summary,
                adopted,
            )) = conn
                .query_row(
                    "SELECT update_id, from_version, to_version, corrections_used, held_out_used,
                            current_error, candidate_error, expected_improvement, diff_summary,
                            adopted
                     FROM learn_update WHERE profile_id = ?1
                     ORDER BY to_version DESC, computed_at DESC LIMIT 1",
                    params![key],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, i64>(2)?,
                            row.get::<_, i64>(3)?,
                            row.get::<_, i64>(4)?,
                            row.get::<_, f64>(5)?,
                            row.get::<_, f64>(6)?,
                            row.get::<_, f64>(7)?,
                            row.get::<_, String>(8)?,
                            row.get::<_, i64>(9)?,
                        ))
                    },
                )
                .optional()
                .map_err(|e| statement_failed("learn_update", &e))?
            else {
                return Ok(None);
            };

            let mut stmt = conn
                .prepare(
                    "SELECT learnable, scene, current_value, candidate_value, corrections, summary
                     FROM learn_update_row WHERE update_id = ?1
                     ORDER BY corrections DESC, learnable",
                )
                .map_err(|e| statement_failed("learn_update_row", &e))?;
            let rows = stmt
                .query_map(params![id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, f64>(2)?,
                        row.get::<_, f64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, String>(5)?,
                    ))
                })
                .map_err(|e| statement_failed("learn_update_row", &e))?;
            let ab_rows: Vec<AbRow> = rows
                .flatten()
                .filter_map(|(l, s, cur, cand, n, summary)| {
                    Learnable::parse(&l).ok().map(|learnable| AbRow {
                        learnable,
                        scene: SceneId::from_str_or_unknown(&s),
                        current: cur as f32,
                        candidate: cand as f32,
                        corrections: u32::try_from(n).unwrap_or(0),
                        summary,
                    })
                })
                .collect();

            let update = LearningUpdate {
                profile_id: profile,
                from_version: u16::try_from(from).unwrap_or(0),
                to_version: u16::try_from(to).unwrap_or(1),
                corrections_used: u32::try_from(used).unwrap_or(0),
                expected_improvement: improvement as f32,
                diff_summary: serde_json::from_str(&summary).unwrap_or_default(),
                adopted: adopted != 0,
            };
            let comparison = AbComparison {
                profile_id: profile,
                current_version: update.from_version,
                candidate_version: update.to_version,
                current_error: current_err as f32,
                candidate_error: cand_err as f32,
                held_out: u32::try_from(held).unwrap_or(0),
                rows: ab_rows,
            };
            Ok(Some((update, comparison)))
        })
    }

    /// Mark a candidate adopted. **The only statement in this crate that sets the column.**
    ///
    /// # Errors
    ///
    /// `AURA-DB-3002` when the row cannot be written.
    pub fn adopt(&self, profile: ProfileId, to_version: u16) -> AuraResult<LearningUpdate> {
        let at = aura_catalog::rfc3339(self.clock.now_utc());
        let key = profile.to_db();
        self.catalog.writer().with(move |conn| {
            conn.execute(
                "UPDATE learn_update SET adopted = 1, adopted_at = ?3
                 WHERE profile_id = ?1 AND to_version = ?2",
                params![key, to_version, at],
            )
            .map_err(|e| statement_failed("learn_update", &e))?;
            Ok(())
        })?;
        match self.candidate(profile)? {
            Some((update, _)) => Ok(update),
            None => Err(statement_failed(
                "learn_update: the adopted row vanished",
                &std::io::Error::other("missing"),
            )),
        }
    }

    /// Store a profile snapshot for rollback.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3002` when the row cannot be written.
    pub fn write_snapshot(&self, profile: ProfileId, version: u16, body: &str) -> AuraResult<()> {
        let at = aura_catalog::rfc3339(self.clock.now_utc());
        let key = profile.to_db();
        let body = body.to_owned();
        let hash = blake3::hash(body.as_bytes()).to_hex().to_string();
        let depth = i64::from(aura_core::contract::learn::ROLLBACK_DEPTH);
        self.catalog.writer().with(move |conn| {
            conn.execute(
                "INSERT OR REPLACE INTO learn_profile_snapshot (profile_id, version, body,
                     body_hash, taken_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![key, version, body, hash, at],
            )
            .map_err(|e| statement_failed("learn_profile_snapshot", &e))?;
            // The depth is a product decision: a photographer rolls back to the version before the
            // one that went wrong, not to the version from last spring, and an unbounded history
            // grows with usage forever.
            conn.execute(
                "DELETE FROM learn_profile_snapshot WHERE profile_id = ?1 AND version <=
                     (SELECT MAX(version) FROM learn_profile_snapshot WHERE profile_id = ?1) - ?2",
                params![key, depth],
            )
            .map_err(|e| statement_failed("learn_profile_snapshot", &e))?;
            Ok(())
        })
    }

    /// One snapshot's body and digest.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the row cannot be read.
    pub fn snapshot(
        &self,
        profile: ProfileId,
        version: u16,
    ) -> AuraResult<Option<(String, String)>> {
        let key = profile.to_db();
        self.catalog.read(move |conn| {
            conn.query_row(
                "SELECT body, body_hash FROM learn_profile_snapshot
                 WHERE profile_id = ?1 AND version = ?2",
                params![key, version],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(|e| statement_failed("learn_profile_snapshot", &e))
        })
    }

    /// The highest snapshot version a profile has.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    pub fn current_version(&self, profile: ProfileId) -> AuraResult<Option<u16>> {
        let key = profile.to_db();
        self.catalog.read(move |conn| {
            let v: Option<i64> = conn
                .query_row(
                    "SELECT MAX(version) FROM learn_profile_snapshot WHERE profile_id = ?1",
                    params![key],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| statement_failed("learn_profile_snapshot", &e))?
                .flatten();
            Ok(v.and_then(|v| u16::try_from(v).ok()))
        })
    }

    /// Roll a profile back by dropping every snapshot above `version`.
    ///
    /// Dropping rather than marking, because "which version is current" has exactly one answer in
    /// this table - the highest - and a second column saying so would be a second answer that can
    /// disagree with it.
    ///
    /// # Errors
    ///
    /// `AURA-LRN-11005` when the rows cannot be written.
    pub fn set_current_version(&self, profile: ProfileId, version: u16) -> AuraResult<()> {
        let key = profile.to_db();
        self.catalog
            .writer()
            .with(move |conn| {
                conn.execute(
                    "DELETE FROM learn_profile_snapshot WHERE profile_id = ?1 AND version > ?2",
                    params![key, version],
                )
                .map_err(|e| statement_failed("learn_profile_snapshot", &e))?;
                Ok(())
            })
            .map_err(|e| rollback_failed(e.detail))
    }

    /// What a project has consented to.
    ///
    /// A project with no row has consented to nothing, which is the default and is returned rather
    /// than being an error - a wedding nobody has been asked about is the ordinary case.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the row cannot be read.
    pub fn consent(&self, project: ProjectId, app_version: &str) -> AuraResult<Consent> {
        let key = project.to_db();
        let fallback = Consent::none(project, app_version);
        self.catalog.read(move |conn| {
            let row = conn
                .query_row(
                    "SELECT local_learning, dataset_contribution, crash_reports, telemetry,
                            decided_at, app_version
                     FROM learn_consent WHERE project_id = ?1",
                    params![key],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, i64>(2)?,
                            row.get::<_, i64>(3)?,
                            row.get::<_, i64>(4)?,
                            row.get::<_, String>(5)?,
                        ))
                    },
                )
                .optional()
                .map_err(|e| statement_failed("learn_consent", &e))?;
            Ok(match row {
                Some((learning, dataset, crash, telemetry, at, version)) => Consent {
                    project,
                    local_learning: learning != 0,
                    dataset_contribution: dataset != 0,
                    crash_reports: crash != 0,
                    telemetry: telemetry != 0,
                    decided_at: at,
                    app_version: version,
                },
                None => fallback,
            })
        })
    }

    /// Record what a project consents to.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3002` when the row cannot be written.
    pub fn set_consent(&self, consent: &Consent) -> AuraResult<()> {
        let c = consent.clone();
        self.catalog.writer().with(move |conn| {
            conn.execute(
                "INSERT INTO learn_consent (project_id, local_learning, dataset_contribution,
                     crash_reports, telemetry, decided_at, app_version)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(project_id) DO UPDATE SET
                     local_learning = excluded.local_learning,
                     dataset_contribution = excluded.dataset_contribution,
                     crash_reports = excluded.crash_reports,
                     telemetry = excluded.telemetry,
                     decided_at = excluded.decided_at,
                     app_version = excluded.app_version",
                params![
                    c.project.to_db(),
                    i64::from(c.local_learning),
                    i64::from(c.dataset_contribution),
                    i64::from(c.crash_reports),
                    i64::from(c.telemetry),
                    c.decided_at,
                    c.app_version
                ],
            )
            .map_err(|e| statement_failed("learn_consent", &e))?;
            Ok(())
        })
    }

    /// What the loop has seen.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    pub fn outline(&self, unattributed: u32) -> AuraResult<LearnOutline> {
        self.catalog.read(move |conn| {
            let (corrections, projects): (i64, i64) = conn
                .query_row(
                    "SELECT COUNT(*), COUNT(DISTINCT project_id) FROM learn_correction",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(|e| statement_failed("learn_correction", &e))?;
            let buckets: i64 = conn
                .query_row("SELECT COUNT(*) FROM v_learn_buckets", [], |row| row.get(0))
                .unwrap_or(0);
            let actionable: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM v_learn_buckets WHERE corrections >= ?1 AND projects >= ?2",
                    params![
                        aura_core::contract::learn::MIN_CORRECTIONS,
                        aura_core::contract::learn::MIN_PROJECTS
                    ],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            let (updates, adopted): (i64, i64) = conn
                .query_row(
                    "SELECT COUNT(*), COALESCE(SUM(adopted), 0) FROM learn_update",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap_or((0, 0));
            let (consented, contributing): (i64, i64) = conn
                .query_row(
                    "SELECT COALESCE(SUM(local_learning), 0),
                            COALESCE(SUM(dataset_contribution), 0) FROM learn_consent",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap_or((0, 0));

            Ok(LearnOutline {
                corrections: u32::try_from(corrections).unwrap_or(0),
                projects: u32::try_from(projects).unwrap_or(0),
                buckets: u32::try_from(buckets).unwrap_or(0),
                actionable_buckets: u32::try_from(actionable).unwrap_or(0),
                unattributed,
                outliers: 0,
                updates: u32::try_from(updates).unwrap_or(0),
                adopted: u32::try_from(adopted).unwrap_or(0),
                rollbacks: 0,
                consented_projects: u32::try_from(consented).unwrap_or(0),
                contributing_projects: u32::try_from(contributing).unwrap_or(0),
                last_update_ms: 0,
            })
        })
    }
}

fn parse_kind(text: &str) -> DecisionKind {
    DecisionKind::ALL
        .iter()
        .copied()
        .find(|k| k.as_str() == text)
        .unwrap_or(DecisionKind::Edit)
}

/// A stable numeric key for a project id, for the `MIN_PROJECTS` count.
///
/// A hash rather than the id itself, because [`Sample`] carries a `u64` and the count only needs
/// distinctness. Deterministic, so a bucket's project count does not move between runs.
fn project_key(project_db: &str) -> u64 {
    let digest = blake3::hash(project_db.as_bytes());
    let b = digest.as_bytes();
    u64::from_be_bytes([
        *b.first().unwrap_or(&0),
        *b.get(1).unwrap_or(&0),
        *b.get(2).unwrap_or(&0),
        *b.get(3).unwrap_or(&0),
        *b.get(4).unwrap_or(&0),
        *b.get(5).unwrap_or(&0),
        *b.get(6).unwrap_or(&0),
        *b.get(7).unwrap_or(&0),
    ])
}
