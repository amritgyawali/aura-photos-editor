//! Migration 12's five tables, and the two rules that live in the SQL rather than around
//! it.
//!
//! ## A re-selection rebuilds everything except the overrides
//!
//! `selection`, `rejections` and `coverage_report` are deleted and rewritten inside one
//! transaction. That is unlike every store since phase 06, which upsert row by row, and the
//! difference is not laziness: a selection is a *set*, and a frame that moved from the
//! gallery to the rejected pile has to leave one table and enter the other atomically. An
//! upsert per row would leave a window in which a photograph was in both.
//!
//! `cull_override` is untouched by that rebuild, which is why it is a table of its own.
//! Phases 06 to 11 all put the photographer's decision in a column on the row it qualifies,
//! because a re-analysis *updates* those rows and the check goes inside the update. Here the
//! rows do not survive, so the column could not either. ADR-0025 section 8.
//!
//! ## Reasons store a code, not a sentence
//!
//! Phase 09's decision, applied a third time. `text` is written only when the sentence
//! differs from `CullCode::user_text` - which is true of the reasons that name a rule, a
//! rival frame or a count, and false of the ordinary ones. On a four-thousand-frame wedding
//! that is the difference between about 90 KB of stored English and about 20 KB.
//!
//! ## What is deliberately not here
//!
//! There is no `delete`, no `move`, no path column and no file operation. A rejection is a
//! row. Invariant 1 says never mutate a RAW, and this is the phase where "just move the
//! rejects to a folder" first sounds reasonable - so the store has nowhere to put the
//! answer.

use std::collections::BTreeMap;
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::contract::cull::ImageId;
use aura_core::errors::db::statement_failed;
use aura_core::{
    AuraResult, ChapterId, Coverage, CoverageReport, CullCode, CullMode, CullOutline, CullReason,
    Decision, IdentityId, MomentId, MustHave, PhotoId, ProjectId, Rejected, Selected,
    SelectionResult,
};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::engine::PassReport;

/// What a photograph was, beyond what the decision carries.
///
/// The four sub-scores, the scene, the chapter and the timeline stamp are stored on every
/// row and are not on `Selected` or `Rejected`. That is deliberate: they are how the panel
/// explains a decision and how the chapter view groups it, and putting four floats and two
/// slugs onto a frozen contract to save one lookup would be a storage decision inside a
/// shape six later phases have to live with.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RowContext {
    /// The scene slug the weights came from.
    pub scene: String,
    /// The chapter slug the quota counted it against.
    pub chapter: String,
    /// Timeline milliseconds. What both tables are ordered by.
    pub timeline_ts: i64,
    /// Technical, emotion, composition and prominence, in `KeepScore` order.
    pub sub_scores: [f32; 4],
}

/// Everything a run needs to record beyond the `SelectionResult` itself.
#[derive(Debug, Clone, PartialEq)]
pub struct Provenance {
    /// The digest of both configuration tables.
    pub config_digest: String,
    /// How many candidates each rule had. The number behind `Missing`.
    pub candidates: BTreeMap<MustHave, u32>,
    /// How many frames each rule ended with.
    pub found: BTreeMap<MustHave, u32>,
    /// The required minimum per rule.
    pub required: BTreeMap<MustHave, u32>,
    /// The warning each rule carries, when it carries one.
    pub warnings: BTreeMap<MustHave, String>,
    /// The run's counters.
    pub report: PassReport,
}

/// Read and write the selection.
#[derive(Debug)]
pub struct CullStore {
    catalog: Arc<Catalog>,
    clock: Arc<dyn Clock>,
}

impl CullStore {
    /// A store over one catalog.
    #[must_use]
    pub fn new(catalog: Arc<Catalog>, clock: Arc<dyn Clock>) -> Self {
        Self { catalog, clock }
    }

    /// Replace a project's selection.
    ///
    /// One transaction. The overrides table is not touched.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when any statement fails; the transaction rolls back and the previous
    /// gallery is still there.
    #[allow(clippy::too_many_lines)]
    pub fn put(
        &self,
        project: &ProjectId,
        result: &SelectionResult,
        provenance: &Provenance,
        context: &BTreeMap<ImageId, RowContext>,
    ) -> AuraResult<()> {
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let key = project.to_db();
        let rows = OwnedRun::build(result, provenance, context);

        self.catalog.writer().transact(move |conn| {
            conn.execute("DELETE FROM selection WHERE project_id = ?1", params![key])
                .map_err(|e| statement_failed("could not clear the previous gallery", &e))?;
            conn.execute("DELETE FROM rejections WHERE project_id = ?1", params![key])
                .map_err(|e| statement_failed("could not clear the previous rejections", &e))?;
            conn.execute(
                "DELETE FROM coverage_report WHERE project_id = ?1",
                params![key],
            )
            .map_err(|e| statement_failed("could not clear the previous coverage report", &e))?;

            conn.execute(
                "INSERT INTO cull_run (
                     project_id, mode, target_count, actual_count,
                     photos, eligible, emotion_aware, composition_aware, grouped,
                     deterministic_hash, config_digest,
                     model_ver, analysis_ver, calibration_ver, elapsed_ms,
                     identity_coverage, chapter_counts, warnings,
                     created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                         ?16, ?17, ?18, ?19, ?19)
                 ON CONFLICT(project_id) DO UPDATE SET
                     mode              = excluded.mode,
                     target_count      = excluded.target_count,
                     actual_count      = excluded.actual_count,
                     photos            = excluded.photos,
                     eligible          = excluded.eligible,
                     emotion_aware     = excluded.emotion_aware,
                     composition_aware = excluded.composition_aware,
                     grouped           = excluded.grouped,
                     deterministic_hash= excluded.deterministic_hash,
                     config_digest     = excluded.config_digest,
                     model_ver         = excluded.model_ver,
                     analysis_ver      = excluded.analysis_ver,
                     calibration_ver   = excluded.calibration_ver,
                     elapsed_ms        = excluded.elapsed_ms,
                     identity_coverage = excluded.identity_coverage,
                     chapter_counts    = excluded.chapter_counts,
                     warnings          = excluded.warnings,
                     updated_at        = excluded.updated_at",
                params![
                    key,
                    rows.mode,
                    rows.target_count,
                    rows.actual_count,
                    rows.photos,
                    rows.eligible,
                    rows.emotion_aware,
                    rows.composition_aware,
                    rows.grouped,
                    rows.hash,
                    rows.config_digest,
                    rows.model_ver,
                    rows.analysis_ver,
                    rows.calibration_ver,
                    rows.elapsed_ms,
                    rows.identity_coverage,
                    rows.chapter_counts,
                    rows.warnings,
                    now,
                ],
            )
            .map_err(|e| statement_failed("could not record the cull run", &e))?;

            {
                let mut statement = conn
                    .prepare(
                        "INSERT INTO selection (
                             photo_id, project_id, moment_id,
                             keep_score, technical, emotion, composition, prominence,
                             scene, chapter, timeline_ts,
                             confidence, reasons, runner_up, coverage_role, created_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                                 ?15, ?16)",
                    )
                    .map_err(|e| statement_failed("could not prepare the gallery insert", &e))?;
                for row in &rows.selected {
                    statement
                        .execute(params![
                            row.photo_id,
                            key,
                            row.moment_id,
                            row.keep_score,
                            row.technical,
                            row.emotion,
                            row.composition,
                            row.prominence,
                            row.scene,
                            row.chapter,
                            row.timeline_ts,
                            row.confidence,
                            row.reasons,
                            row.runner_up,
                            row.coverage_role,
                            now,
                        ])
                        .map_err(|e| statement_failed("could not store a keeper", &e))?;
                }
            }

            {
                let mut statement = conn
                    .prepare(
                        "INSERT INTO rejections (
                             photo_id, project_id, moment_id, keep_score,
                             scene, chapter, timeline_ts, reasons, kept_instead, was_peak,
                             created_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    )
                    .map_err(|e| statement_failed("could not prepare the rejection insert", &e))?;
                for row in &rows.rejected {
                    statement
                        .execute(params![
                            row.photo_id,
                            key,
                            row.moment_id,
                            row.keep_score,
                            row.scene,
                            row.chapter,
                            row.timeline_ts,
                            row.reasons,
                            row.kept_instead,
                            row.was_peak,
                            now,
                        ])
                        .map_err(|e| statement_failed("could not store a rejection", &e))?;
                }
            }

            {
                let mut statement = conn
                    .prepare(
                        "INSERT INTO coverage_report (
                             project_id, rule, state, found, required, candidates, warning,
                             created_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    )
                    .map_err(|e| statement_failed("could not prepare the coverage insert", &e))?;
                for row in &rows.coverage {
                    statement
                        .execute(params![
                            key,
                            row.rule,
                            row.state,
                            row.found,
                            row.required,
                            row.candidates,
                            row.warning,
                            now,
                        ])
                        .map_err(|e| statement_failed("could not store a coverage row", &e))?;
                }
            }
            Ok(())
        })
    }

    /// What a project's cull covered and guaranteed.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the selection cannot be read.
    pub fn outline(&self, project: ProjectId) -> AuraResult<CullOutline> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let row = conn
                .query_row(
                    "SELECT photos, selected, user_kept, user_rejected,
                            COALESCE(eligible, 0), COALESCE(emotion_aware, 0),
                            COALESCE(composition_aware, 0), COALESCE(grouped, 0),
                            COALESCE(mode, 'balanced'), COALESCE(deterministic_hash, 0),
                            COALESCE(model_ver, 0), COALESCE(analysis_ver, 0),
                            COALESCE(calibration_ver, 0)
                       FROM v_cull_coverage WHERE project_id = ?1",
                    params![key],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, i64>(2)?,
                            row.get::<_, i64>(3)?,
                            row.get::<_, i64>(4)?,
                            row.get::<_, i64>(5)?,
                            row.get::<_, i64>(6)?,
                            row.get::<_, i64>(7)?,
                            row.get::<_, String>(8)?,
                            row.get::<_, i64>(9)?,
                            row.get::<_, i64>(10)?,
                            row.get::<_, i64>(11)?,
                            row.get::<_, i64>(12)?,
                        ))
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not read the cull coverage", &e))?;
            let Some(row) = row else {
                return Ok(CullOutline::default());
            };

            let mut states = [0u32; 3];
            let mut statement = conn
                .prepare("SELECT state, COUNT(*) FROM coverage_report WHERE project_id = ?1 GROUP BY state")
                .map_err(|e| statement_failed("could not read the coverage report", &e))?;
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read the coverage report", &e))?;
            while let Some(entry) = cursor
                .next()
                .map_err(|e| statement_failed("could not read the coverage report", &e))?
            {
                let state: String = entry.get(0).unwrap_or_default();
                let count: i64 = entry.get(1).unwrap_or(0);
                let index = match Coverage::from_str_or_missing(&state) {
                    Coverage::Covered => 0,
                    Coverage::CoveredWeak => 1,
                    Coverage::Missing => 2,
                };
                if let Some(slot) = states.get_mut(index) {
                    *slot = u32::try_from(count).unwrap_or(0);
                }
            }

            let eligible = clamp_u32(row.4);
            let selected = clamp_u32(row.1);
            Ok(CullOutline {
                photos: clamp_u32(row.0),
                eligible,
                selected,
                coverage: ratio(eligible, clamp_u32(row.0)),
                emotion_aware: ratio(clamp_u32(row.5), eligible),
                composition_aware: ratio(clamp_u32(row.6), eligible),
                grouped: ratio(clamp_u32(row.7), eligible),
                covered: states.first().copied().unwrap_or(0),
                covered_weak: states.get(1).copied().unwrap_or(0),
                missing: states.get(2).copied().unwrap_or(0),
                user_kept: clamp_u32(row.2),
                user_rejected: clamp_u32(row.3),
                mode: CullMode::from_str_or_balanced(&row.8),
                deterministic_hash: row.9 as u64,
                model_ver: clamp_u16(row.10),
                analysis_ver: clamp_u16(row.11),
                calibration_ver: clamp_u16(row.12),
            })
        })
    }

    /// The stored selection, or `None` when the project has never been culled.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the selection cannot be read.
    #[allow(clippy::too_many_lines)]
    pub fn selection(&self, project: ProjectId) -> AuraResult<Option<SelectionResult>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let run = conn
                .query_row(
                    "SELECT mode, target_count, actual_count, deterministic_hash,
                            model_ver, analysis_ver, calibration_ver,
                            identity_coverage, chapter_counts, warnings
                       FROM cull_run WHERE project_id = ?1",
                    params![key],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, i64>(2)?,
                            row.get::<_, i64>(3)?,
                            row.get::<_, i64>(4)?,
                            row.get::<_, i64>(5)?,
                            row.get::<_, i64>(6)?,
                            row.get::<_, String>(7)?,
                            row.get::<_, String>(8)?,
                            row.get::<_, String>(9)?,
                        ))
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not read the cull run", &e))?;
            let Some(run) = run else {
                return Ok(None);
            };

            let mut selected = Vec::new();
            {
                let mut statement = conn
                    .prepare(
                        "SELECT photo_id, moment_id, keep_score, confidence, reasons,
                                runner_up, coverage_role
                           FROM selection WHERE project_id = ?1 ORDER BY timeline_ts, photo_id",
                    )
                    .map_err(|e| statement_failed("could not read the gallery", &e))?;
                let mut cursor = statement
                    .query(params![key])
                    .map_err(|e| statement_failed("could not read the gallery", &e))?;
                while let Some(row) = cursor
                    .next()
                    .map_err(|e| statement_failed("could not read the gallery", &e))?
                {
                    let Some(image) = photo(&row.get::<_, String>(0).unwrap_or_default()) else {
                        continue;
                    };
                    selected.push(Selected {
                        image_id: image,
                        moment_id: moment(row.get::<_, Option<String>>(1).unwrap_or_default()),
                        keep_score: row.get::<_, f64>(2).unwrap_or(0.0) as f32,
                        confidence: row.get::<_, f64>(3).unwrap_or(0.0) as f32,
                        reasons: decode_reasons(&row.get::<_, String>(4).unwrap_or_default()),
                        runner_up: photo_opt(
                            row.get::<_, Option<String>>(5)
                                .unwrap_or_default()
                                .as_deref(),
                        ),
                        coverage_role: row
                            .get::<_, Option<String>>(6)
                            .unwrap_or_default()
                            .as_deref()
                            .and_then(MustHave::parse),
                    });
                }
            }

            let mut rejected = Vec::new();
            {
                let mut statement = conn
                    .prepare(
                        "SELECT photo_id, moment_id, keep_score, reasons, kept_instead
                           FROM rejections WHERE project_id = ?1 ORDER BY timeline_ts, photo_id",
                    )
                    .map_err(|e| statement_failed("could not read the rejections", &e))?;
                let mut cursor = statement
                    .query(params![key])
                    .map_err(|e| statement_failed("could not read the rejections", &e))?;
                while let Some(row) = cursor
                    .next()
                    .map_err(|e| statement_failed("could not read the rejections", &e))?
                {
                    let Some(image) = photo(&row.get::<_, String>(0).unwrap_or_default()) else {
                        continue;
                    };
                    rejected.push(Rejected {
                        image_id: image,
                        moment_id: moment(row.get::<_, Option<String>>(1).unwrap_or_default()),
                        keep_score: row.get::<_, f64>(2).unwrap_or(0.0) as f32,
                        reasons: decode_reasons(&row.get::<_, String>(3).unwrap_or_default()),
                        kept_instead: photo_opt(
                            row.get::<_, Option<String>>(4)
                                .unwrap_or_default()
                                .as_deref(),
                        ),
                    });
                }
            }

            let mut must_haves = Vec::new();
            {
                let mut statement = conn
                    .prepare("SELECT rule, state FROM coverage_report WHERE project_id = ?1")
                    .map_err(|e| statement_failed("could not read the coverage report", &e))?;
                let mut cursor = statement
                    .query(params![key])
                    .map_err(|e| statement_failed("could not read the coverage report", &e))?;
                while let Some(row) = cursor
                    .next()
                    .map_err(|e| statement_failed("could not read the coverage report", &e))?
                {
                    let slug: String = row.get(0).unwrap_or_default();
                    let state: String = row.get(1).unwrap_or_default();
                    if let Some(rule) = MustHave::parse(&slug) {
                        must_haves.push((rule, Coverage::from_str_or_missing(&state)));
                    }
                }
            }
            must_haves.sort_by_key(|(rule, _)| {
                MustHave::ALL
                    .iter()
                    .position(|candidate| candidate == rule)
                    .unwrap_or(MustHave::COUNT)
            });

            Ok(Some(SelectionResult {
                actual_count: clamp_u32(run.2),
                selected,
                rejected,
                coverage: CoverageReport {
                    must_haves,
                    identity_coverage: decode_identity(&run.7),
                    chapter_counts: decode_chapters(&run.8),
                    warnings: serde_json::from_str(&run.9).unwrap_or_default(),
                },
                target_count: clamp_u32(run.1),
                mode: CullMode::from_str_or_balanced(&run.0),
                deterministic_hash: run.3 as u64,
                model_ver: clamp_u16(run.4),
                analysis_ver: clamp_u16(run.5),
                calibration_ver: clamp_u16(run.6),
            }))
        })
    }

    /// What was decided about one photograph.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the decision cannot be read.
    pub fn decision(&self, image: ImageId) -> AuraResult<Option<Decision>> {
        let key = image.to_db();
        self.catalog.read(move |conn| {
            let keep = conn
                .query_row(
                    "SELECT moment_id, keep_score, confidence, reasons, runner_up, coverage_role
                       FROM selection WHERE photo_id = ?1",
                    params![key],
                    |row| {
                        Ok((
                            row.get::<_, Option<String>>(0)?,
                            row.get::<_, f64>(1)?,
                            row.get::<_, f64>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, Option<String>>(4)?,
                            row.get::<_, Option<String>>(5)?,
                        ))
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not read the decision", &e))?;
            if let Some(keep) = keep {
                let Some(id) = photo(&key) else {
                    return Ok(None);
                };
                return Ok(Some(Decision::Keep(Box::new(Selected {
                    image_id: id,
                    moment_id: moment(keep.0),
                    keep_score: keep.1 as f32,
                    confidence: keep.2 as f32,
                    reasons: decode_reasons(&keep.3),
                    runner_up: photo_opt(keep.4.as_deref()),
                    coverage_role: keep.5.as_deref().and_then(MustHave::parse),
                }))));
            }

            let drop = conn
                .query_row(
                    "SELECT moment_id, keep_score, reasons, kept_instead
                       FROM rejections WHERE photo_id = ?1",
                    params![key],
                    |row| {
                        Ok((
                            row.get::<_, Option<String>>(0)?,
                            row.get::<_, f64>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, Option<String>>(3)?,
                        ))
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not read the decision", &e))?;
            let Some(drop) = drop else {
                return Ok(None);
            };
            let Some(id) = photo(&key) else {
                return Ok(None);
            };
            Ok(Some(Decision::Drop(Box::new(Rejected {
                image_id: id,
                moment_id: moment(drop.0),
                keep_score: drop.1 as f32,
                reasons: decode_reasons(&drop.2),
                kept_instead: photo_opt(drop.3.as_deref()),
            }))))
        })
    }

    /// Every override in a project.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when they cannot be read.
    pub fn overrides(&self, project: ProjectId) -> AuraResult<BTreeMap<ImageId, bool>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare("SELECT photo_id, action FROM cull_override WHERE project_id = ?1")
                .map_err(|e| statement_failed("could not read the overrides", &e))?;
            let mut out = BTreeMap::new();
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read the overrides", &e))?;
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read the overrides", &e))?
            {
                let text: String = row.get(0).unwrap_or_default();
                let action: String = row.get(1).unwrap_or_default();
                if let Some(id) = photo(&text) {
                    out.insert(id, action == "keep");
                }
            }
            Ok(out)
        })
    }

    /// Record or replace one override.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the write fails.
    pub fn set_override(&self, project: &ProjectId, image: ImageId, keep: bool) -> AuraResult<()> {
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let photo_key = image.to_db();
        let project_key = project.to_db();
        let action = if keep { "keep" } else { "reject" };
        self.catalog.writer().transact(move |conn| {
            conn.execute(
                "INSERT INTO cull_override (photo_id, project_id, action, created_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(photo_id) DO UPDATE SET action = excluded.action",
                params![photo_key, project_key, action, now],
            )
            .map_err(|e| statement_failed("could not record the choice", &e))?;
            Ok(())
        })
    }

    /// Withdraw one override. Returns whether there was one.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the delete fails.
    pub fn clear_override(&self, image: ImageId) -> AuraResult<bool> {
        let key = image.to_db();
        self.catalog.writer().transact(move |conn| {
            let removed = conn
                .execute(
                    "DELETE FROM cull_override WHERE photo_id = ?1",
                    params![key],
                )
                .map_err(|e| statement_failed("could not clear the choice", &e))?;
            Ok(removed > 0)
        })
    }

    /// Which project a photograph belongs to.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the lookup fails.
    pub fn project_of(&self, image: ImageId) -> AuraResult<Option<ProjectId>> {
        let key = image.to_db();
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

    /// The digest of the configuration a stored selection was made with.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when it cannot be read.
    pub fn config_digest(&self, project: ProjectId) -> AuraResult<Option<String>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            conn.query_row(
                "SELECT config_digest FROM cull_run WHERE project_id = ?1",
                params![key],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| statement_failed("could not read the stored configuration digest", &e))
        })
    }
}

// -- encoding ---------------------------------------------------------------

/// One stored reason. `text` is absent when the code's own sentence was used.
#[derive(Debug, Serialize, Deserialize)]
struct StoredReason {
    code: String,
    weight: f32,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    text: Option<String>,
}

fn encode_reasons(reasons: &[CullReason]) -> String {
    let stored: Vec<StoredReason> = reasons
        .iter()
        .map(|reason| StoredReason {
            code: reason.code.as_str().to_string(),
            weight: reason.weight,
            text: if reason.text_is_default() {
                None
            } else {
                Some(reason.text.clone())
            },
        })
        .collect();
    serde_json::to_string(&stored).unwrap_or_else(|_| "[]".to_string())
}

fn decode_reasons(text: &str) -> Vec<CullReason> {
    let stored: Vec<StoredReason> = serde_json::from_str(text).unwrap_or_default();
    stored
        .into_iter()
        .map(|reason| {
            let code = CullCode::from_str_or_winner(&reason.code);
            CullReason {
                code,
                text: reason.text.unwrap_or_else(|| code.user_text().to_string()),
                weight: reason.weight,
            }
        })
        .collect()
}

fn encode_identity(coverage: &[(IdentityId, u32)]) -> String {
    let stored: Vec<(String, u32)> = coverage
        .iter()
        .map(|(id, count)| (id.to_db(), *count))
        .collect();
    serde_json::to_string(&stored).unwrap_or_else(|_| "[]".to_string())
}

fn decode_identity(text: &str) -> Vec<(IdentityId, u32)> {
    let stored: Vec<(String, u32)> = serde_json::from_str(text).unwrap_or_default();
    stored
        .into_iter()
        .filter_map(|(id, count)| IdentityId::from_db(&id).ok().map(|id| (id, count)))
        .collect()
}

fn encode_chapters(counts: &[(ChapterId, u32, u32)]) -> String {
    let stored: Vec<(String, u32, u32)> = counts
        .iter()
        .map(|(chapter, delivered, target)| (chapter.as_str().to_string(), *delivered, *target))
        .collect();
    serde_json::to_string(&stored).unwrap_or_else(|_| "[]".to_string())
}

fn decode_chapters(text: &str) -> Vec<(ChapterId, u32, u32)> {
    let stored: Vec<(String, u32, u32)> = serde_json::from_str(text).unwrap_or_default();
    stored
        .into_iter()
        .filter_map(|(slug, delivered, target)| {
            ChapterId::ALL
                .into_iter()
                .find(|chapter| chapter.as_str() == slug)
                .map(|chapter| (chapter, delivered, target))
        })
        .collect()
}

/// One keeper, as columns.
struct KeepRow {
    photo_id: String,
    moment_id: Option<String>,
    keep_score: f64,
    technical: f64,
    emotion: f64,
    composition: f64,
    prominence: f64,
    scene: String,
    chapter: String,
    timeline_ts: i64,
    confidence: f64,
    reasons: String,
    runner_up: Option<String>,
    coverage_role: Option<String>,
}

/// One rejection, as columns.
struct DropRow {
    photo_id: String,
    moment_id: Option<String>,
    keep_score: f64,
    scene: String,
    chapter: String,
    timeline_ts: i64,
    reasons: String,
    kept_instead: Option<String>,
    was_peak: i64,
}

/// One coverage row, as columns.
struct CoverRow {
    rule: String,
    state: String,
    found: i64,
    required: i64,
    candidates: i64,
    warning: Option<String>,
}

/// Everything one `put` writes, precomputed outside the transaction.
struct OwnedRun {
    mode: String,
    target_count: i64,
    actual_count: i64,
    photos: i64,
    eligible: i64,
    emotion_aware: i64,
    composition_aware: i64,
    grouped: i64,
    hash: i64,
    config_digest: String,
    model_ver: i64,
    analysis_ver: i64,
    calibration_ver: i64,
    elapsed_ms: i64,
    identity_coverage: String,
    chapter_counts: String,
    warnings: String,
    selected: Vec<KeepRow>,
    rejected: Vec<DropRow>,
    coverage: Vec<CoverRow>,
}

impl OwnedRun {
    fn build(
        result: &SelectionResult,
        provenance: &Provenance,
        context: &BTreeMap<ImageId, RowContext>,
    ) -> Self {
        let blank = RowContext {
            scene: "unknown".to_string(),
            chapter: "other".to_string(),
            timeline_ts: 0,
            sub_scores: [0.0; 4],
        };
        let selected = result
            .selected
            .iter()
            .map(|keep| {
                let row = context.get(&keep.image_id).unwrap_or(&blank);
                KeepRow {
                    photo_id: keep.image_id.to_db(),
                    moment_id: keep.moment_id.map(|id| id.to_db()),
                    keep_score: f64::from(keep.keep_score.clamp(0.0, 1.0)),
                    technical: sub(row, 0),
                    emotion: sub(row, 1),
                    composition: sub(row, 2),
                    prominence: sub(row, 3),
                    scene: row.scene.clone(),
                    chapter: row.chapter.clone(),
                    timeline_ts: row.timeline_ts,
                    confidence: f64::from(keep.confidence.clamp(0.0, 1.0)),
                    reasons: encode_reasons(&keep.reasons),
                    runner_up: keep.runner_up.map(|id| id.to_db()),
                    coverage_role: keep.coverage_role.map(|rule| rule.as_str().to_string()),
                }
            })
            .collect();
        let rejected = result
            .rejected
            .iter()
            .map(|drop| {
                let row = context.get(&drop.image_id).unwrap_or(&blank);
                DropRow {
                    photo_id: drop.image_id.to_db(),
                    moment_id: drop.moment_id.map(|id| id.to_db()),
                    keep_score: f64::from(drop.keep_score.clamp(0.0, 1.0)),
                    scene: row.scene.clone(),
                    chapter: row.chapter.clone(),
                    timeline_ts: row.timeline_ts,
                    reasons: encode_reasons(&drop.reasons),
                    kept_instead: drop.kept_instead.map(|id| id.to_db()),
                    was_peak: i64::from(drop.was_peak()),
                }
            })
            .collect();
        let coverage = result
            .coverage
            .must_haves
            .iter()
            .map(|(rule, state)| CoverRow {
                rule: rule.as_str().to_string(),
                state: state.as_str().to_string(),
                found: i64::from(provenance.found.get(rule).copied().unwrap_or(0)),
                required: i64::from(provenance.required.get(rule).copied().unwrap_or(0)),
                candidates: i64::from(provenance.candidates.get(rule).copied().unwrap_or(0)),
                warning: provenance.warnings.get(rule).cloned(),
            })
            .collect();

        Self {
            mode: result.mode.as_str().to_string(),
            target_count: i64::from(result.target_count),
            actual_count: i64::from(result.actual_count),
            photos: i64::try_from(provenance.report.photos).unwrap_or(i64::MAX),
            eligible: i64::try_from(provenance.report.eligible).unwrap_or(i64::MAX),
            emotion_aware: i64::try_from(provenance.report.emotion_aware).unwrap_or(i64::MAX),
            composition_aware: i64::try_from(provenance.report.composition_aware)
                .unwrap_or(i64::MAX),
            grouped: i64::try_from(provenance.report.grouped).unwrap_or(i64::MAX),
            // SQLite has no unsigned integer. The reinterpretation is exact and reversed
            // on read; a hash written as hex would cost eight bytes per project for nothing.
            #[allow(clippy::cast_possible_wrap)]
            hash: result.deterministic_hash as i64,
            config_digest: provenance.config_digest.clone(),
            model_ver: i64::from(result.model_ver),
            analysis_ver: i64::from(result.analysis_ver),
            calibration_ver: i64::from(result.calibration_ver),
            elapsed_ms: i64::try_from(provenance.report.elapsed_ms).unwrap_or(i64::MAX),
            identity_coverage: encode_identity(&result.coverage.identity_coverage),
            chapter_counts: encode_chapters(&result.coverage.chapter_counts),
            warnings: serde_json::to_string(&result.coverage.warnings)
                .unwrap_or_else(|_| "[]".to_string()),
            selected,
            rejected,
            coverage,
        }
    }
}

/// One sub-score, as the column type.
fn sub(row: &RowContext, index: usize) -> f64 {
    f64::from(
        row.sub_scores
            .get(index)
            .copied()
            .unwrap_or(0.0)
            .clamp(0.0, 1.0),
    )
}

fn photo(text: &str) -> Option<ImageId> {
    PhotoId::from_db(text).ok()
}

fn photo_opt(text: Option<&str>) -> Option<ImageId> {
    text.and_then(photo)
}

fn moment(text: Option<String>) -> Option<MomentId> {
    text.and_then(|value| MomentId::from_db(&value).ok())
}

fn clamp_u32(value: i64) -> u32 {
    u32::try_from(value.max(0)).unwrap_or(u32::MAX)
}

fn clamp_u16(value: i64) -> u16 {
    u16::try_from(value.max(0)).unwrap_or(u16::MAX)
}

fn ratio(part: u32, whole: u32) -> f32 {
    if whole == 0 {
        return 0.0;
    }
    let thousandths = u64::from(part)
        .saturating_mul(1_000)
        .checked_div(u64::from(whole))
        .unwrap_or(0)
        .min(1_000);
    f32::from(u16::try_from(thousandths).unwrap_or(1_000)) / 1_000.0
}
