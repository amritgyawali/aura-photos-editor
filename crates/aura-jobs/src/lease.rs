//! Leases and heartbeats.
//!
//! A lease expires on wall-clock time recorded in the catalog, but a worker
//! renews it on monotonic time, so an NTP correction after a flight cannot
//! reclaim thousands of healthy running tasks at once.

use aura_catalog::{rfc3339, Catalog};
use aura_core::errors::db::statement_failed;
use aura_core::AuraResult;
use rusqlite::{params, OptionalExtension};
use time::Duration;

/// How long a lease lives and how often it is renewed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeaseConfig {
    /// Lease duration in milliseconds.
    pub lease_ms: i64,
    /// Renewal interval; must be well under `lease_ms`.
    pub heartbeat_ms: i64,
}

impl Default for LeaseConfig {
    fn default() -> Self {
        Self {
            lease_ms: 30_000,
            heartbeat_ms: 5_000,
        }
    }
}

/// A claimed task, held by one worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lease {
    /// The claimed task.
    pub task_id: String,
    /// The stage the task belongs to.
    pub stage: String,
    /// The subject the stage should act on.
    pub subject_id: String,
    /// The worker that owns the claim.
    pub owner: String,
    /// When the claim lapses, RFC-3339 UTC.
    pub expires_at: String,
}

/// Claim the highest-priority ready task for `owner`, if there is one.
///
/// The claim is a single conditional UPDATE, so two workers racing for the last
/// task cannot both win.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when a statement fails.
pub fn claim_next(
    catalog: &Catalog,
    project_id: &str,
    owner: &str,
    config: LeaseConfig,
) -> AuraResult<Option<Lease>> {
    let project_id = project_id.to_string();
    let owner = owner.to_string();
    let now_dt = catalog.clock().now_utc();
    let now = rfc3339(now_dt);
    let expires_at = rfc3339(now_dt + Duration::milliseconds(config.lease_ms));

    catalog.writer().transact(move |tx| {
        let candidate: Option<(String, String, String)> = tx
            .query_row(
                "SELECT task_id, stage, subject_id FROM task
                  WHERE project_id = ?1 AND state = 'ready'
                  ORDER BY priority, created_at, task_id
                  LIMIT 1",
                params![project_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(|e| statement_failed("select claimable task", &e))?;

        let Some((task_id, stage, subject_id)) = candidate else {
            return Ok(None);
        };

        let claimed = tx
            .execute(
                "UPDATE task
                    SET state = 'running', lease_owner = ?2, lease_expires_at = ?3,
                        heartbeat_at = ?4, updated_at = ?4
                  WHERE task_id = ?1 AND state = 'ready'",
                params![task_id, owner, expires_at, now],
            )
            .map_err(|e| statement_failed("claim task", &e))?;

        if claimed == 0 {
            return Ok(None); // another worker won the race
        }

        Ok(Some(Lease {
            task_id,
            stage,
            subject_id,
            owner,
            expires_at,
        }))
    })
}

/// Renew a lease. A worker that stops calling this loses the task.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when the update fails, and `AURA-JOB-7002` when the
/// lease has already been reclaimed by the sweeper.
pub fn heartbeat(catalog: &Catalog, lease: &Lease, config: LeaseConfig) -> AuraResult<String> {
    let task_id = lease.task_id.clone();
    let owner = lease.owner.clone();
    let now_dt = catalog.clock().now_utc();
    let now = rfc3339(now_dt);
    let expires_at = rfc3339(now_dt + Duration::milliseconds(config.lease_ms));

    catalog.writer().transact(move |tx| {
        let renewed = tx
            .execute(
                "UPDATE task
                    SET lease_expires_at = ?3, heartbeat_at = ?4, updated_at = ?4
                  WHERE task_id = ?1 AND lease_owner = ?2 AND state = 'running'",
                params![task_id, owner, expires_at, now],
            )
            .map_err(|e| statement_failed("renew lease", &e))?;

        if renewed == 0 {
            return Err(aura_core::errors::job::lease_expired(task_id));
        }
        Ok(expires_at)
    })
}

/// Reclaim every task whose lease has lapsed, returning them to the ready queue.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when the update fails.
pub fn reclaim_expired(catalog: &Catalog) -> AuraResult<usize> {
    let now = rfc3339(catalog.clock().now_utc());
    catalog.writer().transact(move |tx| {
        tx.execute(
            "UPDATE task
                SET state = 'ready', lease_owner = NULL, lease_expires_at = NULL,
                    attempts = attempts + 1, updated_at = ?1
              WHERE state = 'running' AND lease_expires_at IS NOT NULL AND lease_expires_at < ?1",
            params![now],
        )
        .map_err(|e| statement_failed("reclaim expired leases", &e))
    })
}
