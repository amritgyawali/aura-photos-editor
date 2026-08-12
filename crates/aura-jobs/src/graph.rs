//! Tasks and their dependencies, stored in the catalog so a crash cannot lose them.

use aura_catalog::{repo as catalog_repo, rfc3339, Catalog};
use aura_core::errors::db::statement_failed;
use aura_core::errors::job;
use aura_core::AuraResult;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

/// Where a task is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    /// Created, dependencies not yet satisfied.
    Pending,
    /// Every dependency succeeded; claimable.
    Ready,
    /// Leased by a worker.
    Running,
    /// Finished successfully.
    Succeeded,
    /// Finished unsuccessfully, retries remain.
    Failed,
    /// Out of retries; set aside with its reason.
    Quarantined,
    /// Deliberately not run.
    Skipped,
}

impl TaskState {
    /// The text form stored in the catalog.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Ready => "ready",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Quarantined => "quarantined",
            Self::Skipped => "skipped",
        }
    }

    /// Parse the text form, returning `None` for anything unknown.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "pending" => Some(Self::Pending),
            "ready" => Some(Self::Ready),
            "running" => Some(Self::Running),
            "succeeded" => Some(Self::Succeeded),
            "failed" => Some(Self::Failed),
            "quarantined" => Some(Self::Quarantined),
            "skipped" => Some(Self::Skipped),
            _ => None,
        }
    }
}

/// One unit of work. The id is derived, not random, so re-planning a run is a
/// no-op instead of a duplicate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    /// `{stage}:{subject}:{stage_version}`.
    pub task_id: String,
    /// Owning project.
    pub project_id: String,
    /// The run that planned it.
    pub run_id: String,
    /// Pipeline stage name.
    pub stage: String,
    /// Photo, file or import id this task acts on.
    pub subject_id: String,
    /// Version of the stage's logic; bumping it re-plans the work.
    pub stage_version: i64,
    /// Lower runs first.
    pub priority: i64,
    /// How many attempts are allowed before quarantine.
    pub max_attempts: i64,
}

impl Task {
    /// Build a task with the derived id and default budget.
    #[must_use]
    pub fn new(
        project_id: impl Into<String>,
        run_id: impl Into<String>,
        stage: impl Into<String>,
        subject_id: impl Into<String>,
        stage_version: i64,
    ) -> Self {
        let stage = stage.into();
        let subject_id = subject_id.into();
        Self {
            task_id: format!("{stage}:{subject_id}:{stage_version}"),
            project_id: project_id.into(),
            run_id: run_id.into(),
            stage,
            subject_id,
            stage_version,
            priority: 100,
            max_attempts: 3,
        }
    }
}

/// The scheduler's view of the catalog.
#[derive(Debug)]
pub struct JobGraph<'a> {
    catalog: &'a Catalog,
}

impl<'a> JobGraph<'a> {
    /// Wrap an open catalog.
    #[must_use]
    pub fn new(catalog: &'a Catalog) -> Self {
        Self { catalog }
    }

    /// Insert tasks and their dependency edges. Re-planning the same run is a no-op.
    ///
    /// # Errors
    ///
    /// Returns `AURA-DB-3006` when a statement fails.
    pub fn plan(&self, tasks: Vec<Task>, edges: Vec<(String, String)>) -> AuraResult<usize> {
        let now = rfc3339(self.catalog.clock().now_utc());
        self.catalog.writer().transact(move |tx| {
            let mut inserted = 0usize;
            for task in &tasks {
                let state = if edges.iter().any(|(child, _)| child == &task.task_id) {
                    TaskState::Pending
                } else {
                    TaskState::Ready
                };
                inserted += tx
                    .execute(
                        "INSERT INTO task
                           (task_id, project_id, run_id, stage, subject_id, stage_version, state,
                            priority, max_attempts, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)
                         ON CONFLICT (task_id) DO NOTHING",
                        params![
                            task.task_id,
                            task.project_id,
                            task.run_id,
                            task.stage,
                            task.subject_id,
                            task.stage_version,
                            state.as_str(),
                            task.priority,
                            task.max_attempts,
                            now
                        ],
                    )
                    .map_err(|e| statement_failed("insert task", &e))?;
            }
            for (task_id, depends_on) in &edges {
                tx.execute(
                    "INSERT INTO task_dep (task_id, depends_on) VALUES (?1, ?2)
                     ON CONFLICT (task_id, depends_on) DO NOTHING",
                    params![task_id, depends_on],
                )
                .map_err(|e| statement_failed("insert task dependency", &e))?;
            }
            Ok(inserted)
        })
    }

    /// Promote every pending task whose dependencies have all succeeded.
    ///
    /// # Errors
    ///
    /// Returns `AURA-DB-3006` when the update fails.
    pub fn promote_ready(&self) -> AuraResult<usize> {
        let now = rfc3339(self.catalog.clock().now_utc());
        self.catalog.writer().transact(move |tx| {
            tx.execute(
                "UPDATE task SET state = 'ready', updated_at = ?1
                  WHERE state = 'pending'
                    AND NOT EXISTS (
                        SELECT 1 FROM task_dep d
                          JOIN task parent ON parent.task_id = d.depends_on
                         WHERE d.task_id = task.task_id
                           AND parent.state <> 'succeeded')",
                params![now],
            )
            .map_err(|e| statement_failed("promote ready tasks", &e))
        })
    }

    /// The current state of one task.
    ///
    /// # Errors
    ///
    /// Returns `AURA-DB-3006` when the query fails.
    pub fn state_of(&self, task_id: &str) -> AuraResult<Option<TaskState>> {
        let task_id = task_id.to_string();
        self.catalog.read(move |conn: &Connection| {
            let text: Option<String> = conn
                .query_row(
                    "SELECT state FROM task WHERE task_id = ?1",
                    params![task_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| statement_failed("read task state", &e))?;
            Ok(text.as_deref().and_then(TaskState::parse))
        })
    }

    /// Mark a task finished successfully.
    ///
    /// # Errors
    ///
    /// Returns `AURA-DB-3006` when the update fails.
    pub fn succeed(&self, task_id: &str) -> AuraResult<()> {
        let task_id = task_id.to_string();
        let now = rfc3339(self.catalog.clock().now_utc());
        self.catalog.writer().transact(move |tx| {
            tx.execute(
                "UPDATE task
                    SET state = 'succeeded', finished_at = ?2, updated_at = ?2,
                        lease_owner = NULL, lease_expires_at = NULL
                  WHERE task_id = ?1",
                params![task_id, now],
            )
            .map_err(|e| statement_failed("mark task succeeded", &e))?;
            Ok(())
        })
    }

    /// Record a failure, retrying while the budget allows and quarantining after.
    ///
    /// # Errors
    ///
    /// Returns `AURA-DB-3006` when the update fails.
    pub fn fail(&self, task_id: &str, error_code: &str, detail: &str) -> AuraResult<TaskState> {
        let task_id = task_id.to_string();
        let error_code = error_code.to_string();
        let detail = detail.to_string();
        let now = rfc3339(self.catalog.clock().now_utc());

        self.catalog.writer().transact(move |tx| {
            let (attempts, max_attempts): (i64, i64) = tx
                .query_row(
                    "SELECT attempts, max_attempts FROM task WHERE task_id = ?1",
                    params![task_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(|e| statement_failed("read task attempts", &e))?;

            let next = if attempts + 1 >= max_attempts {
                TaskState::Quarantined
            } else {
                TaskState::Failed
            };

            tx.execute(
                "UPDATE task
                    SET state = ?2, attempts = attempts + 1, error_code = ?3, error_detail = ?4,
                        updated_at = ?5, lease_owner = NULL, lease_expires_at = NULL
                  WHERE task_id = ?1",
                params![task_id, next.as_str(), error_code, detail, now],
            )
            .map_err(|e| statement_failed("mark task failed", &e))?;
            Ok(next)
        })
    }

    /// Re-arm failed tasks that still have retries left.
    ///
    /// # Errors
    ///
    /// Returns `AURA-DB-3006` when the update fails.
    pub fn requeue_failed(&self) -> AuraResult<usize> {
        let now = rfc3339(self.catalog.clock().now_utc());
        self.catalog.writer().transact(move |tx| {
            tx.execute(
                "UPDATE task SET state = 'ready', updated_at = ?1
                  WHERE state = 'failed' AND attempts < max_attempts",
                params![now],
            )
            .map_err(|e| statement_failed("requeue failed tasks", &e))
        })
    }

    /// How many tasks are in each state, for the progress panel.
    ///
    /// # Errors
    ///
    /// Returns `AURA-DB-3006` when the query fails.
    pub fn state_counts(&self, run_id: &str) -> AuraResult<Vec<(String, i64)>> {
        let run_id = run_id.to_string();
        self.catalog.read(move |conn: &Connection| {
            let mut stmt = conn
                .prepare(
                    "SELECT state, COUNT(*) FROM task WHERE run_id = ?1 GROUP BY state ORDER BY state",
                )
                .map_err(|e| statement_failed("prepare state counts", &e))?;
            let rows = stmt
                .query_map(params![run_id], |row| Ok((row.get(0)?, row.get(1)?)))
                .map_err(|e| statement_failed("query state counts", &e))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(|e| statement_failed("read state count", &e))?);
            }
            Ok(out)
        })
    }

    /// Cancel every unfinished task in a run, so a stop is immediate and typed.
    ///
    /// # Errors
    ///
    /// Returns `AURA-DB-3006` when the update fails.
    pub fn cancel_run(&self, run_id: &str) -> AuraResult<usize> {
        let run_id = run_id.to_string();
        let now = rfc3339(self.catalog.clock().now_utc());
        let cancelled = job::cancelled("jobs");
        self.catalog.writer().transact(move |tx| {
            tx.execute(
                "UPDATE task
                    SET state = 'skipped', updated_at = ?2, error_code = ?3,
                        lease_owner = NULL, lease_expires_at = NULL
                  WHERE run_id = ?1 AND state IN ('pending','ready','running','failed')",
                params![run_id, now, cancelled.code.0],
            )
            .map_err(|e| statement_failed("cancel run", &e))
        })
    }
}

/// Re-exported so callers do not need to depend on `aura-catalog` for one type.
pub use catalog_repo::PhotoOrder;
