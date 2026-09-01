//! Migration 28, and the only place the autopilot's own rows are read or written.
//!
//! Every function here is arithmetic-free. The decisions live in [`crate::dag`],
//! [`crate::governor`], [`crate::retry`] and [`crate::progress`]; this turns them into rows and
//! back, and refuses a row it cannot understand rather than guessing.
//!
//! ## Why an unreadable row is `None` rather than a default
//!
//! A stage slug this build does not know is a checkpoint written by a different release, and a
//! default would resume a wedding onto somebody else's plan. Phase 08 wrote this rule for a
//! moment, phase 13 for an autonomy band and phase 27 for a ticket status; here the thing being
//! protected is two hours of a photographer's evening.

use aura_catalog::{rfc3339, Catalog};
use aura_core::clock::Clock;
use aura_core::contract::ids::{ProjectId, RunId};
use aura_core::errors::db::statement_failed;
use aura_core::AuraResult;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use std::sync::Arc;

use crate::contract::autopilot::{
    AutopilotCode, AutopilotOutline, AutopilotOverride, AutopilotReason, Checkpoint,
    GovernorAction, ResourceEvent, ResourceKind, RunStatus, SkipCause, StageId, StageReport,
    StageVerdict,
};

/// Reads and writes the autopilot's rows.
#[derive(Debug, Clone)]
pub struct AutopilotStore {
    catalog: Arc<Catalog>,
    clock: Arc<dyn Clock>,
}

impl AutopilotStore {
    /// Wrap an open catalog.
    #[must_use]
    pub fn new(catalog: Arc<Catalog>, clock: Arc<dyn Clock>) -> Self {
        Self { catalog, clock }
    }

    /// Open a run, or re-open the stopped one this id names.
    ///
    /// Three cases, and the middle one is the whole of resumption:
    ///
    /// * **No row.** A fresh run is inserted.
    /// * **A row this id names that is resumable** - still `running` because the process died, or
    ///   `cancelled` or `failed` because it stopped - is set back to `running` and its stage
    ///   checkpoints are waiting where they were. This is what "start again" does to a wedding
    ///   somebody stopped.
    /// * **A row this id names that is delivered.** Refused. `autopilot_run_no_reopen` refuses it
    ///   too, so the promise holds even against a caller that never came through here.
    ///
    /// A *different* run in flight for the same project is refused whatever this run is doing.
    /// `idx_autopilot_run_one_in_flight` is a partial unique index, so two schedulers racing to
    /// start the same wedding end with one row and one error rather than with two runs writing the
    /// same stages.
    ///
    /// # Errors
    ///
    /// `AURA-JOB-7009` when another run is in flight or this one is already delivered,
    /// `AURA-DB-3006` when the statement fails.
    pub fn open_run(
        &self,
        run_id: RunId,
        project: ProjectId,
        zero_touch: bool,
        calibrated: bool,
        stages_enabled: u32,
        policy_ver: i64,
        orchestrator_ver: i64,
    ) -> AuraResult<()> {
        let run = run_id.to_db();
        if let Some(existing) = self.in_flight(project)? {
            if existing != run {
                return Err(crate::errors::run_in_flight(existing));
            }
        }
        if let Some(status) = self.status_of(&run)? {
            if status.is_finished() {
                return Err(crate::errors::run_in_flight(run));
            }
        }

        let now = rfc3339(self.clock.now_utc());
        let key = project.to_db();
        self.catalog.writer().transact(move |tx| {
            tx.execute(
                "INSERT INTO autopilot_run
                   (run_id, project_id, status, zero_touch, calibrated, stages_enabled,
                    policy_ver, orchestrator_ver, started_at, updated_at)
                 VALUES (?1, ?2, 'running', ?3, ?4, ?5, ?6, ?7, ?8, ?8)
                 ON CONFLICT (run_id) DO UPDATE SET
                     status           = 'running',
                     zero_touch       = excluded.zero_touch,
                     calibrated       = excluded.calibrated,
                     stages_enabled   = excluded.stages_enabled,
                     policy_ver       = excluded.policy_ver,
                     orchestrator_ver = excluded.orchestrator_ver,
                     finished_at      = NULL,
                     updated_at       = excluded.updated_at",
                params![
                    run,
                    key,
                    i64::from(zero_touch),
                    i64::from(calibrated),
                    i64::from(stages_enabled),
                    policy_ver,
                    orchestrator_ver,
                    now
                ],
            )
            .map_err(|err| statement_failed("open autopilot run", &err))?;
            Ok(())
        })
    }

    /// One run's status, when the row exists.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn status_of(&self, run_id: &str) -> AuraResult<Option<RunStatus>> {
        let run = run_id.to_string();
        self.catalog.read(move |conn: &Connection| {
            let stored: Option<String> = conn
                .query_row(
                    "SELECT status FROM autopilot_run WHERE run_id = ?1",
                    params![run],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|err| statement_failed("read run status", &err))?;
            Ok(stored.as_deref().and_then(RunStatus::parse))
        })
    }

    /// The project's run that pressing start would continue, when there is one.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn resumable(&self, project: ProjectId) -> AuraResult<Option<String>> {
        let key = project.to_db();
        self.catalog.read(move |conn: &Connection| {
            conn.query_row(
                "SELECT run_id FROM autopilot_run
                  WHERE project_id = ?1
                    AND status IN ('running', 'cancelled', 'failed')
                  ORDER BY started_at DESC, run_id DESC LIMIT 1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .map_err(|err| statement_failed("read resumable run", &err))
        })
    }

    /// The id of this project's run in flight, when there is one.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn in_flight(&self, project: ProjectId) -> AuraResult<Option<String>> {
        let key = project.to_db();
        self.catalog.read(move |conn: &Connection| {
            conn.query_row(
                "SELECT run_id FROM autopilot_run
                  WHERE project_id = ?1 AND status = 'running'",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .map_err(|err| statement_failed("read run in flight", &err))
        })
    }

    /// The newest run of this project, in flight or not.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn latest_run(&self, project: ProjectId) -> AuraResult<Option<String>> {
        let key = project.to_db();
        self.catalog.read(move |conn: &Connection| {
            conn.query_row(
                "SELECT run_id FROM autopilot_run
                  WHERE project_id = ?1
                  ORDER BY started_at DESC, run_id DESC LIMIT 1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .map_err(|err| statement_failed("read latest run", &err))
        })
    }

    /// Plan one stage: write its row before it starts.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the insert fails.
    #[allow(clippy::too_many_arguments)]
    pub fn plan_stage(
        &self,
        run_id: &str,
        stage: StageId,
        index: u32,
        items_total: u32,
        inputs_hash: &str,
        verdict: StageVerdict,
        items_done: u32,
    ) -> AuraResult<()> {
        let now = rfc3339(self.clock.now_utc());
        let run = run_id.to_string();
        let hash = inputs_hash.to_string();
        self.catalog.writer().transact(move |tx| {
            tx.execute(
                "INSERT INTO autopilot_stage
                   (run_id, stage, stage_index, verdict, items_total, items_done, inputs_hash,
                    outcome, skip_cause, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, NULL, ?8)
                 ON CONFLICT (run_id, stage) DO UPDATE SET
                     stage_index = excluded.stage_index,
                     verdict     = excluded.verdict,
                     items_total = excluded.items_total,
                     -- The caller decides where the stage starts. A restart passes zero, so a
                     -- stale count from an invalidated checkpoint cannot survive into the run that
                     -- replaced it.
                     items_done  = excluded.items_done,
                     inputs_hash = excluded.inputs_hash,
                     -- Re-opened: the row is about to be run again, and a stale outcome would make
                     -- a stage that is still going look finished to anything reading mid-run.
                     outcome     = NULL,
                     skip_cause  = NULL,
                     updated_at  = excluded.updated_at",
                params![
                    run,
                    stage.as_str(),
                    i64::from(index),
                    verdict.as_str(),
                    i64::from(items_total),
                    i64::from(items_done),
                    hash,
                    now
                ],
            )
            .map_err(|err| statement_failed("plan autopilot stage", &err))?;
            Ok(())
        })
    }

    /// Record what happened to a stage.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the update fails.
    #[allow(clippy::too_many_arguments)]
    pub fn finish_stage(
        &self,
        run_id: &str,
        stage: StageId,
        outcome: &str,
        skip_cause: Option<SkipCause>,
        items_done: u32,
        attempts: u8,
        elapsed_ms: u64,
        error: Option<(&str, &str)>,
    ) -> AuraResult<()> {
        let now = rfc3339(self.clock.now_utc());
        let run = run_id.to_string();
        let outcome = outcome.to_string();
        let cause = skip_cause.map(|cause| cause.as_str().to_string());
        let code = error.map(|(code, _)| code.to_string());
        let detail = error.map(|(_, detail)| detail.to_string());
        #[allow(clippy::cast_possible_wrap)]
        let elapsed = elapsed_ms as i64;
        self.catalog.writer().transact(move |tx| {
            tx.execute(
                "UPDATE autopilot_stage
                    SET outcome = ?3, skip_cause = ?4, items_done = ?5, attempts = ?6,
                        elapsed_ms = ?7, error_code = ?8, error_detail = ?9,
                        finished_at = ?10, updated_at = ?10
                  WHERE run_id = ?1 AND stage = ?2",
                params![
                    run,
                    stage.as_str(),
                    outcome,
                    cause,
                    i64::from(items_done),
                    i64::from(attempts),
                    elapsed,
                    code,
                    detail,
                    now
                ],
            )
            .map_err(|err| statement_failed("finish autopilot stage", &err))?;
            Ok(())
        })
    }

    /// Close a run.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the update fails, including the trigger's refusal to reopen a finished
    /// run.
    #[allow(clippy::too_many_arguments)]
    pub fn close_run(
        &self,
        run_id: &str,
        status: RunStatus,
        selected: u32,
        exported: u32,
        needs_review: u32,
        spend_usd: f32,
        output_path: &str,
    ) -> AuraResult<()> {
        let now = rfc3339(self.clock.now_utc());
        let run = run_id.to_string();
        let path = output_path.to_string();
        self.catalog.writer().transact(move |tx| {
            tx.execute(
                "UPDATE autopilot_run
                    SET status = ?2,
                        selected = ?3, exported = ?4, needs_review = ?5,
                        spend_usd = ?6, output_path = ?7,
                        -- Three counts over the same rows, answering three different questions.
                        -- `enabled` is what the photographer asked for, so a stage they switched
                        -- off is subtracted from it rather than counted as finished; `completed`
                        -- is the stages that did their work; `degraded` is the ones that could
                        -- not. Counting a switched-off stage as completed is what failed the
                        -- CHECK the first time this ran.
                        stages_enabled = (
                            SELECT COUNT(*) FROM autopilot_stage
                             WHERE run_id = ?1
                               AND NOT (outcome = 'skipped' AND skip_cause = 'turned_off')),
                        stages_completed = (
                            SELECT COUNT(*) FROM autopilot_stage
                             WHERE run_id = ?1 AND outcome = 'completed'),
                        stages_degraded = (
                            SELECT COUNT(*) FROM autopilot_stage
                             WHERE run_id = ?1
                               AND (outcome IN ('failed', 'partial')
                                    OR (outcome = 'skipped' AND skip_cause <> 'turned_off'))),
                        finished_at = ?8, updated_at = ?8
                  WHERE run_id = ?1",
                params![
                    run,
                    status.as_str(),
                    i64::from(selected),
                    i64::from(exported),
                    i64::from(needs_review),
                    f64::from(spend_usd),
                    path,
                    now
                ],
            )
            .map_err(|err| statement_failed("close autopilot run", &err))?;
            Ok(())
        })
    }

    /// Append a reason.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the insert fails.
    pub fn add_reason(&self, run_id: &str, reason: &AutopilotReason) -> AuraResult<()> {
        let now = rfc3339(self.clock.now_utc());
        let run = run_id.to_string();
        let code = reason.code.as_str().to_string();
        let stage = reason.stage.map(|stage| stage.as_str().to_string());
        let detail = reason.detail.clone();
        self.catalog.writer().transact(move |tx| {
            let next: i64 = tx
                .query_row(
                    "SELECT COALESCE(MAX(seq), 0) + 1 FROM autopilot_reason WHERE run_id = ?1",
                    params![run],
                    |row| row.get(0),
                )
                .map_err(|err| statement_failed("read next reason sequence", &err))?;
            tx.execute(
                "INSERT INTO autopilot_reason (run_id, stage, code, detail, seq, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![run, stage, code, detail, next, now],
            )
            .map_err(|err| statement_failed("insert autopilot reason", &err))?;
            Ok(())
        })
    }

    /// Record what the governor did.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the insert fails.
    pub fn add_event(&self, run_id: &str, event: &ResourceEvent) -> AuraResult<()> {
        if event.action == GovernorAction::Proceed {
            return Ok(());
        }
        let now = rfc3339(self.clock.now_utc());
        let run = run_id.to_string();
        let kind = event.kind.as_str().to_string();
        let action = event.action.as_str().to_string();
        let stage = event.stage.as_str().to_string();
        let reading = f64::from(event.reading);
        let threshold = f64::from(event.threshold);
        self.catalog.writer().transact(move |tx| {
            tx.execute(
                "INSERT INTO autopilot_event
                   (run_id, kind, action, reading, threshold, stage, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![run, kind, action, reading, threshold, stage, now],
            )
            .map_err(|err| statement_failed("insert autopilot event", &err))?;
            Ok(())
        })
    }

    /// Every stage of one run, in execution order.
    ///
    /// A row whose stage slug this build does not know is dropped rather than defaulted. That is a
    /// checkpoint from a different release, and the honest thing a newer build can say about it is
    /// nothing.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn stages(&self, run_id: &str) -> AuraResult<Vec<StageReport>> {
        let run = run_id.to_string();
        let reasons = self.reasons(run_id)?;
        self.catalog.read(move |conn: &Connection| {
            let mut stmt = conn
                .prepare(
                    "SELECT stage, outcome, skip_cause, verdict, items_done, items_total,
                            elapsed_ms, attempts
                       FROM autopilot_stage
                      WHERE run_id = ?1
                      ORDER BY stage_index",
                )
                .map_err(|err| statement_failed("prepare autopilot stages", &err))?;
            let rows = stmt
                .query_map(params![run], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, i64>(7)?,
                    ))
                })
                .map_err(|err| statement_failed("query autopilot stages", &err))?;

            let mut out = Vec::new();
            for row in rows {
                let (slug, outcome, cause, verdict, done, total, elapsed, attempts) =
                    row.map_err(|err| statement_failed("read autopilot stage", &err))?;
                let Some(stage) = StageId::parse(&slug) else {
                    continue;
                };
                out.push(StageReport {
                    stage,
                    outcome: outcome.unwrap_or_else(|| "running".to_string()),
                    skip_cause: cause.as_deref().and_then(SkipCause::parse),
                    verdict: parse_verdict(&verdict),
                    items_done: clamp_u32(done),
                    items_total: clamp_u32(total),
                    elapsed_ms: elapsed.max(0).unsigned_abs(),
                    attempts: clamp_u8(attempts),
                    reasons: reasons
                        .iter()
                        .filter(|reason| reason.stage == Some(stage))
                        .cloned()
                        .collect(),
                });
            }
            Ok(out)
        })
    }

    /// One stage's stored checkpoint.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn checkpoint(&self, run_id: RunId, stage: StageId) -> AuraResult<Option<Checkpoint>> {
        let run = run_id.to_db();
        self.catalog.read(move |conn: &Connection| {
            let stored = conn
                .query_row(
                    "SELECT items_done, items_total, inputs_hash, attempts, outcome, elapsed_ms
                       FROM autopilot_stage
                      WHERE run_id = ?1 AND stage = ?2",
                    params![run, stage.as_str()],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, i64>(3)?,
                            row.get::<_, Option<String>>(4)?,
                            row.get::<_, i64>(5)?,
                        ))
                    },
                )
                .optional()
                .map_err(|err| statement_failed("read autopilot checkpoint", &err))?;
            Ok(stored.map(
                |(done, total, hash, attempts, outcome, elapsed)| Checkpoint {
                    run_id,
                    stage,
                    items_done: clamp_u32(done),
                    items_total: clamp_u32(total),
                    inputs_hash: hash,
                    attempts: clamp_u8(attempts),
                    outcome,
                    elapsed_ms: elapsed.max(0).unsigned_abs(),
                },
            ))
        })
    }

    /// Every reason on one run.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn reasons(&self, run_id: &str) -> AuraResult<Vec<AutopilotReason>> {
        let run = run_id.to_string();
        self.catalog.read(move |conn: &Connection| {
            let mut stmt = conn
                .prepare(
                    "SELECT code, stage, detail FROM autopilot_reason
                      WHERE run_id = ?1 ORDER BY seq",
                )
                .map_err(|err| statement_failed("prepare autopilot reasons", &err))?;
            let rows = stmt
                .query_map(params![run], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(|err| statement_failed("query autopilot reasons", &err))?;
            let mut out = Vec::new();
            for row in rows {
                let (code, stage, detail) =
                    row.map_err(|err| statement_failed("read autopilot reason", &err))?;
                let Some(code) = AutopilotCode::parse(&code) else {
                    continue;
                };
                out.push(AutopilotReason {
                    code,
                    stage: stage.as_deref().and_then(StageId::parse),
                    detail,
                });
            }
            Ok(out)
        })
    }

    /// Everything the governor did during one run, newest first.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn events(&self, run_id: &str) -> AuraResult<Vec<ResourceEvent>> {
        let run = run_id.to_string();
        self.catalog.read(move |conn: &Connection| {
            let mut stmt = conn
                .prepare(
                    "SELECT kind, action, reading, threshold, stage FROM autopilot_event
                      WHERE run_id = ?1 ORDER BY event_id DESC",
                )
                .map_err(|err| statement_failed("prepare autopilot events", &err))?;
            let rows = stmt
                .query_map(params![run], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, f64>(2)?,
                        row.get::<_, f64>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                })
                .map_err(|err| statement_failed("query autopilot events", &err))?;
            let mut out = Vec::new();
            for row in rows {
                let (kind, action, reading, threshold, stage) =
                    row.map_err(|err| statement_failed("read autopilot event", &err))?;
                let (Some(kind), Some(stage)) =
                    (ResourceKind::parse(&kind), StageId::parse(&stage))
                else {
                    continue;
                };
                #[allow(clippy::cast_possible_truncation)]
                out.push(ResourceEvent {
                    kind,
                    action: GovernorAction::from_str_or_pause(&action),
                    reading: reading as f32,
                    threshold: threshold as f32,
                    stage,
                });
            }
            Ok(out)
        })
    }

    /// The project header.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn outline(&self, project: ProjectId) -> AuraResult<AutopilotOutline> {
        let key = project.to_db();
        let bytes = self.bytes(project)?;
        self.catalog.read(move |conn: &Connection| {
            let runs: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM autopilot_run WHERE project_id = ?1",
                    params![key],
                    |row| row.get(0),
                )
                .map_err(|err| statement_failed("count autopilot runs", &err))?;

            let latest = conn
                .query_row(
                    "SELECT run_id, status, stages_enabled, stages_completed, stages_degraded,
                            zero_touch, calibrated, policy_ver, orchestrator_ver
                       FROM autopilot_run
                      WHERE project_id = ?1
                      ORDER BY started_at DESC, run_id DESC LIMIT 1",
                    params![key],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, i64>(2)?,
                            row.get::<_, i64>(3)?,
                            row.get::<_, i64>(4)?,
                            row.get::<_, i64>(5)?,
                            row.get::<_, i64>(6)?,
                            row.get::<_, i64>(7)?,
                            row.get::<_, i64>(8)?,
                        ))
                    },
                )
                .optional()
                .map_err(|err| statement_failed("read autopilot outline", &err))?;

            let Some((
                run_id,
                status,
                enabled,
                completed,
                degraded,
                zero_touch,
                calibrated,
                policy_ver,
                orchestrator_ver,
            )) = latest
            else {
                return Ok(AutopilotOutline {
                    runs: clamp_u32(runs),
                    latest_run: None,
                    status: None,
                    stages_enabled: 0,
                    stages_completed: 0,
                    stages_degraded: 0,
                    zero_touch: false,
                    calibrated: false,
                    resource_events: 0,
                    bytes,
                    policy_ver: 0,
                    orchestrator_ver: 0,
                });
            };

            let events: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM autopilot_event WHERE run_id = ?1",
                    params![run_id],
                    |row| row.get(0),
                )
                .map_err(|err| statement_failed("count autopilot events", &err))?;

            Ok(AutopilotOutline {
                runs: clamp_u32(runs),
                latest_run: Some(run_id),
                status: RunStatus::parse(&status),
                stages_enabled: clamp_u32(enabled),
                stages_completed: clamp_u32(completed),
                stages_degraded: clamp_u32(degraded),
                zero_touch: zero_touch != 0,
                calibrated: calibrated != 0,
                resource_events: clamp_u32(events),
                bytes,
                policy_ver,
                orchestrator_ver,
            })
        })
    }

    /// What the photographer chose, or the defaults when they have chosen nothing.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn settings(&self, project: ProjectId) -> AuraResult<AutopilotOverride> {
        let key = project.to_db();
        self.catalog.read(move |conn: &Connection| {
            let stored = conn
                .query_row(
                    "SELECT disabled, zero_touch, allow_on_battery, quiet_mode
                       FROM autopilot_settings WHERE project_id = ?1",
                    params![key],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, i64>(2)?,
                            row.get::<_, i64>(3)?,
                        ))
                    },
                )
                .optional()
                .map_err(|err| statement_failed("read autopilot settings", &err))?;

            let Some((disabled, zero_touch, battery, quiet)) = stored else {
                return Ok(AutopilotOverride {
                    project,
                    disabled: Vec::new(),
                    zero_touch: false,
                    allow_on_battery: false,
                    quiet_mode: true,
                });
            };
            let slugs: Vec<String> = serde_json::from_str(&disabled).unwrap_or_default();
            Ok(AutopilotOverride {
                project,
                disabled: slugs
                    .iter()
                    .filter_map(|slug| StageId::parse(slug))
                    .collect(),
                zero_touch: zero_touch != 0,
                allow_on_battery: battery != 0,
                quiet_mode: quiet != 0,
            })
        })
    }

    /// Record what the photographer chose.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the write fails.
    pub fn set_settings(&self, settings: &AutopilotOverride) -> AuraResult<()> {
        let now = rfc3339(self.clock.now_utc());
        let key = settings.project.to_db();
        let slugs: Vec<&str> = settings
            .disabled
            .iter()
            .map(|stage| stage.as_str())
            .collect();
        let disabled = serde_json::to_string(&slugs).unwrap_or_else(|_| "[]".to_string());
        let zero_touch = i64::from(settings.zero_touch);
        let battery = i64::from(settings.allow_on_battery);
        let quiet = i64::from(settings.quiet_mode);
        self.catalog.writer().transact(move |tx| {
            tx.execute(
                "INSERT INTO autopilot_settings
                   (project_id, disabled, zero_touch, allow_on_battery, quiet_mode, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT (project_id) DO UPDATE SET
                     disabled         = excluded.disabled,
                     zero_touch       = excluded.zero_touch,
                     allow_on_battery = excluded.allow_on_battery,
                     quiet_mode       = excluded.quiet_mode,
                     updated_at       = excluded.updated_at",
                params![key, disabled, zero_touch, battery, quiet, now],
            )
            .map_err(|err| statement_failed("write autopilot settings", &err))?;
            Ok(())
        })
    }

    /// Bytes migration 28 holds for one project.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn bytes(&self, project: ProjectId) -> AuraResult<u64> {
        let key = project.to_db();
        self.catalog.read(move |conn: &Connection| {
            let runs: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM autopilot_run WHERE project_id = ?1",
                    params![key],
                    |row| row.get(0),
                )
                .map_err(|err| statement_failed("count runs for bytes", &err))?;
            let stages: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM autopilot_stage s
                       JOIN autopilot_run r ON r.run_id = s.run_id
                      WHERE r.project_id = ?1",
                    params![key],
                    |row| row.get(0),
                )
                .map_err(|err| statement_failed("count stages for bytes", &err))?;
            let events: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM autopilot_event e
                       JOIN autopilot_run r ON r.run_id = e.run_id
                      WHERE r.project_id = ?1",
                    params![key],
                    |row| row.get(0),
                )
                .map_err(|err| statement_failed("count events for bytes", &err))?;
            let reasons: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM autopilot_reason a
                       JOIN autopilot_run r ON r.run_id = a.run_id
                      WHERE r.project_id = ?1",
                    params![key],
                    |row| row.get(0),
                )
                .map_err(|err| statement_failed("count reasons for bytes", &err))?;
            // Payload estimates, not page counts. Phase 09's lesson: a budget measured with a
            // quantised instrument must not be set at its own measurement, and `PRAGMA page_count`
            // quantises to 4 KiB - which for a table this small would report the same number
            // whether it held one run or forty.
            Ok((runs.max(0).unsigned_abs() * 220)
                + (stages.max(0).unsigned_abs() * 150)
                + (events.max(0).unsigned_abs() * 90)
                + (reasons.max(0).unsigned_abs() * 80))
        })
    }
}

/// Write one checkpoint inside the caller's transaction.
///
/// Free rather than a method, because the caller owns the transaction: the whole point of
/// [`crate::checkpoint::CheckpointWriter`] is that a checkpoint goes in the same transaction as
/// the work it describes, and a method on a store holding its own writer could not do that.
///
/// # Errors
///
/// `AURA-DB-3006` when the statement fails.
pub fn write_checkpoint(tx: &Transaction<'_>, checkpoint: &Checkpoint) -> AuraResult<()> {
    #[allow(clippy::cast_possible_wrap)]
    let elapsed = checkpoint.elapsed_ms as i64;
    tx.execute(
        "UPDATE autopilot_stage
            SET items_done = ?3, attempts = ?4, elapsed_ms = ?5, inputs_hash = ?6
          WHERE run_id = ?1 AND stage = ?2",
        params![
            checkpoint.run_id.to_db(),
            checkpoint.stage.as_str(),
            i64::from(checkpoint.items_done),
            i64::from(checkpoint.attempts),
            elapsed,
            checkpoint.inputs_hash
        ],
    )
    .map_err(|err| statement_failed("write autopilot checkpoint", &err))?;
    Ok(())
}

fn parse_verdict(text: &str) -> StageVerdict {
    match text {
        "act" => StageVerdict::Act,
        "act_and_review" => StageVerdict::ActAndReview,
        // Anything else holds, which is the cautious direction: an unreadable verdict that
        // defaulted to `Act` would be a stored row granting autonomy the gate never gave.
        _ => StageVerdict::Hold,
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn clamp_u32(value: i64) -> u32 {
    value.clamp(0, i64::from(u32::MAX)) as u32
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn clamp_u8(value: i64) -> u8 {
    value.clamp(0, i64::from(u8::MAX)) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unreadable_verdict_holds() {
        assert_eq!(parse_verdict("act"), StageVerdict::Act);
        assert_eq!(parse_verdict("act_and_review"), StageVerdict::ActAndReview);
        assert_eq!(parse_verdict("hold"), StageVerdict::Hold);
        assert_eq!(parse_verdict("auto"), StageVerdict::Hold);
        assert_eq!(parse_verdict(""), StageVerdict::Hold);
    }

    #[test]
    fn clamping_is_saturating_rather_than_wrapping() {
        assert_eq!(clamp_u32(-5), 0);
        assert_eq!(clamp_u32(i64::MAX), u32::MAX);
        assert_eq!(clamp_u8(-1), 0);
        assert_eq!(clamp_u8(9_000), u8::MAX);
    }
}
