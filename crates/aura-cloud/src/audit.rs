//! Every call, including the ones that never happened.
//!
//! Acceptance criterion 4 is "the audit viewer can trace any AI decision to its
//! call, model, tokens, cost and evidence". The word that shapes this module is
//! *any*. A table holding only successful provider calls answers the easy
//! question and leaves the one support actually gets - "why does this wedding
//! look different from the last one" - unanswerable, because the interesting
//! decisions are exactly the ones that fell back.
//!
//! So a row is written for a cache hit, for a refusal, for a budget stop and for
//! every local fallback, with `source` and `fallback_reason` saying which. Cost
//! and token columns are zero on those rows, which is what keeps the spend meter
//! honest: it is a `SUM(cost_usd)` over rows that were actually billed.
//!
//! Writes are best-effort by design. An audit row that cannot be written is
//! logged and swallowed, never propagated: losing the record of a decision is
//! bad, and losing the decision because its record could not be written is
//! worse.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::errors::db::statement_failed;
use aura_core::AuraResult;
use rusqlite::params;
use uuid::Uuid;

use crate::contract::cloud::Source;

/// One row of the trail.
#[derive(Debug, Clone, PartialEq)]
pub struct CallRecord {
    /// Deterministic identifier, also returned in `CloudResult::call_id`.
    pub id: Uuid,
    /// The project, when the call belonged to one.
    pub project_id: Option<String>,
    /// Registry name of the task.
    pub task: String,
    /// Task version.
    pub task_version: u16,
    /// The model that answered, or `local`.
    pub model: String,
    /// Provider identifier.
    pub provider: String,
    /// Which transport carried it: `http`, `cassette`, `offline`, `none`.
    pub transport: String,
    /// Where the answer came from.
    pub source: Source,
    /// The tier the governor chose, when there was one.
    pub tier: Option<String>,
    /// Hash of the system prompt, the user turn and the image hashes.
    pub prompt_hash: String,
    /// Image content hashes, in the order they were sent.
    pub image_hashes: Vec<String>,
    /// Prompt tokens billed.
    pub tokens_in: u32,
    /// Completion tokens billed.
    pub tokens_out: u32,
    /// What the call cost.
    pub cost_usd: f64,
    /// Wall clock, including retries and the gateway's own work.
    pub latency_ms: u64,
    /// `ok`, `repaired`, `schema_invalid`, `fallback`, `cache`, `refused`.
    pub status: String,
    /// Repairs attempted. Zero or one.
    pub retry_count: u32,
    /// Set when the answer was local, saying why.
    pub fallback_reason: Option<String>,
    /// Which price table priced it.
    pub price_table: u32,
    /// Fields the schema did not define, dropped.
    pub pruned_fields: u32,
    /// Enum members mapped to `unknown`.
    pub coerced_enums: u32,
    /// Reported confidence of the decision.
    pub confidence: f32,
    /// The decision this call produced, when the caller supplied one.
    pub decision_ref: Option<String>,
    /// The agent loop's steps, as JSON, when the call came from one.
    pub scratchpad: Option<String>,
}

impl CallRecord {
    /// A record for a decision the local function answered.
    #[must_use]
    pub fn local(id: Uuid, task: &str, task_version: u16, reason: &str) -> Self {
        Self {
            id,
            project_id: None,
            task: task.to_string(),
            task_version,
            model: "local".to_string(),
            provider: "none".to_string(),
            transport: "none".to_string(),
            source: Source::LocalFallback,
            tier: None,
            prompt_hash: String::new(),
            image_hashes: Vec::new(),
            tokens_in: 0,
            tokens_out: 0,
            cost_usd: 0.0,
            latency_ms: 0,
            status: "fallback".to_string(),
            retry_count: 0,
            fallback_reason: Some(reason.to_string()),
            price_table: 0,
            pruned_fields: 0,
            coerced_enums: 0,
            confidence: 0.0,
            decision_ref: None,
            scratchpad: None,
        }
    }

    /// Attach the project this call belonged to.
    #[must_use]
    pub fn in_project(mut self, project_id: &str) -> Self {
        self.project_id = Some(project_id.to_string());
        self
    }

    /// Attach the decision this call produced.
    #[must_use]
    pub fn for_decision(mut self, decision_ref: &str) -> Self {
        self.decision_ref = Some(decision_ref.to_string());
        self
    }
}

/// Where audit rows go.
pub trait AuditSink: Send + Sync + std::fmt::Debug {
    /// Write one row. Failures are the sink's problem, never the caller's.
    fn record(&self, call: &CallRecord);

    /// Total billed spend for one project, in US dollars.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the trail cannot be read.
    fn spent_usd(&self, project_id: &str) -> AuraResult<f64>;

    /// The most recent rows for one project, newest first.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the trail cannot be read.
    fn recent(&self, project_id: &str, limit: u32) -> AuraResult<Vec<CallRecord>>;
}

/// The trail in the project's catalog.
#[derive(Debug)]
pub struct CatalogAudit {
    catalog: Arc<Catalog>,
    clock: Arc<dyn Clock>,
}

impl CatalogAudit {
    /// Use one catalog's `cloud_calls` table.
    #[must_use]
    pub fn new(catalog: Arc<Catalog>, clock: Arc<dyn Clock>) -> Self {
        Self { catalog, clock }
    }
}

impl AuditSink for CatalogAudit {
    fn record(&self, call: &CallRecord) {
        let row = call.clone();
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let written = self.catalog.writer().with(move |conn| {
            conn.execute(
                "INSERT OR REPLACE INTO cloud_calls
                   (id, project_id, task, task_version, model, prompt_hash, image_hashes,
                    tokens_in, tokens_out, cost_usd, latency_ms, status, retry_count,
                    decision_ref, created_at, provider, transport, source, tier,
                    fallback_reason, price_table, pruned_fields, coerced_enums, confidence,
                    scratchpad)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                         ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)",
                params![
                    row.id.to_string(),
                    row.project_id,
                    row.task,
                    i64::from(row.task_version),
                    row.model,
                    row.prompt_hash,
                    row.image_hashes.join(","),
                    i64::from(row.tokens_in),
                    i64::from(row.tokens_out),
                    row.cost_usd,
                    i64::try_from(row.latency_ms).unwrap_or(i64::MAX),
                    row.status,
                    i64::from(row.retry_count),
                    row.decision_ref,
                    now,
                    row.provider,
                    row.transport,
                    row.source.as_str(),
                    row.tier,
                    row.fallback_reason,
                    i64::from(row.price_table),
                    i64::from(row.pruned_fields),
                    i64::from(row.coerced_enums),
                    f64::from(row.confidence),
                    row.scratchpad,
                ],
            )
            .map(|_| ())
            .map_err(|err| statement_failed("cloud audit write failed", &err))
        });
        if let Err(err) = written {
            tracing::warn!(
                target: "cloud.audit",
                code = err.code.0,
                "an audit row could not be written; the decision itself is unaffected"
            );
        }
    }

    fn spent_usd(&self, project_id: &str) -> AuraResult<f64> {
        let project = project_id.to_string();
        self.catalog.read(move |conn| {
            conn.query_row(
                "SELECT COALESCE(SUM(cost_usd), 0.0) FROM cloud_calls
                 WHERE project_id = ?1 AND source = 'cloud'",
                params![project],
                |row| row.get(0),
            )
            .map_err(|err| statement_failed("cloud spend query failed", &err))
        })
    }

    fn recent(&self, project_id: &str, limit: u32) -> AuraResult<Vec<CallRecord>> {
        let project = project_id.to_string();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT id, project_id, task, task_version, model, prompt_hash, image_hashes,
                            tokens_in, tokens_out, cost_usd, latency_ms, status, retry_count,
                            decision_ref, provider, transport, source, tier, fallback_reason,
                            price_table, pruned_fields, coerced_enums, confidence, scratchpad
                     FROM cloud_calls WHERE project_id = ?1
                     ORDER BY created_at DESC, id DESC LIMIT ?2",
                )
                .map_err(|err| statement_failed("cloud audit query failed", &err))?;

            let rows = statement
                .query_map(params![project, i64::from(limit)], |row| {
                    let hashes: String = row.get(6)?;
                    let source: String = row.get(16)?;
                    Ok(CallRecord {
                        id: row
                            .get::<_, String>(0)?
                            .parse()
                            .unwrap_or_else(|_| Uuid::nil()),
                        project_id: row.get(1)?,
                        task: row.get(2)?,
                        task_version: u16::try_from(row.get::<_, i64>(3)?).unwrap_or(0),
                        model: row.get(4)?,
                        prompt_hash: row.get(5)?,
                        image_hashes: hashes
                            .split(',')
                            .filter(|part| !part.is_empty())
                            .map(ToString::to_string)
                            .collect(),
                        tokens_in: u32::try_from(row.get::<_, i64>(7)?).unwrap_or(0),
                        tokens_out: u32::try_from(row.get::<_, i64>(8)?).unwrap_or(0),
                        cost_usd: row.get(9)?,
                        latency_ms: u64::try_from(row.get::<_, i64>(10)?).unwrap_or(0),
                        status: row.get(11)?,
                        retry_count: u32::try_from(row.get::<_, i64>(12)?).unwrap_or(0),
                        decision_ref: row.get(13)?,
                        provider: row.get(14)?,
                        transport: row.get(15)?,
                        source: match source.as_str() {
                            "cache" => Source::Cache,
                            "local_fallback" => Source::LocalFallback,
                            _ => Source::Cloud,
                        },
                        tier: row.get(17)?,
                        fallback_reason: row.get(18)?,
                        price_table: u32::try_from(row.get::<_, i64>(19)?).unwrap_or(0),
                        pruned_fields: u32::try_from(row.get::<_, i64>(20)?).unwrap_or(0),
                        coerced_enums: u32::try_from(row.get::<_, i64>(21)?).unwrap_or(0),
                        confidence: row.get::<_, f64>(22)? as f32,
                        scratchpad: row.get(23)?,
                    })
                })
                .map_err(|err| statement_failed("cloud audit query failed", &err))?;

            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(|err| statement_failed("cloud audit row failed", &err))?);
            }
            Ok(out)
        })
    }
}

/// An in-process trail. The gate and the unit tests use it.
#[derive(Debug, Default)]
pub struct MemoryAudit {
    rows: parking_lot::Mutex<Vec<CallRecord>>,
}

impl MemoryAudit {
    /// An empty trail.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Every row, in the order they were written.
    #[must_use]
    pub fn rows(&self) -> Vec<CallRecord> {
        self.rows.lock().clone()
    }
}

impl AuditSink for MemoryAudit {
    fn record(&self, call: &CallRecord) {
        self.rows.lock().push(call.clone());
    }

    fn spent_usd(&self, project_id: &str) -> AuraResult<f64> {
        Ok(self
            .rows
            .lock()
            .iter()
            .filter(|row| {
                row.source == Source::Cloud && row.project_id.as_deref() == Some(project_id)
            })
            .map(|row| row.cost_usd)
            .sum())
    }

    fn recent(&self, project_id: &str, limit: u32) -> AuraResult<Vec<CallRecord>> {
        let rows = self.rows.lock();
        Ok(rows
            .iter()
            .rev()
            .filter(|row| row.project_id.as_deref() == Some(project_id))
            .take(limit as usize)
            .cloned()
            .collect())
    }
}

/// The sequence every call identifier is drawn from.
///
/// Process-wide rather than per-gateway, and that is the whole point. Two
/// gateways over the same catalog - which is what re-opening a project produces,
/// and what the phase 04 gate builds to measure a re-run - would otherwise each
/// start at zero, derive the same id for the same cache key, and have the second
/// row overwrite the first. The spend meter would then read zero for a wedding
/// that had been billed, which is exactly the kind of quiet wrong number an
/// audit trail exists to prevent.
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// A call identifier that is unique within a process and reproducible across
/// runs that make the same calls in the same order.
///
/// A v4 UUID would be a random number, and random numbers are banned by the
/// determinism gate for a good reason: the phase 04 verifier prints call ids, and
/// a gate whose output changes on every run cannot be diffed. So the id is a
/// digest of the cache key and a counter instead.
///
/// The honest limit: with several threads calling at once the counter is still
/// unique but the *assignment* is not reproducible, so two concurrent runs can
/// swap two ids. Nothing depends on which id a call got, only that no two calls
/// share one.
#[must_use]
pub fn call_id(cache_key: &str, _hint: u64) -> Uuid {
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let digest = blake3::hash(format!("{cache_key}:{sequence}").as_bytes());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(digest.as_bytes().get(..16).unwrap_or(&[0u8; 16]));
    Uuid::from_bytes(bytes)
}
