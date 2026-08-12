-- Migration 4 - the cloud AI audit trail, response cache and spending caps.
--
-- PHASE-04 section 5. The three tables below are copied from the phase document
-- and then extended; the extension is recorded in
-- docs/adr/ADR-0009-cloud-ai-policy.md and every added column is nullable or has
-- a default, so a reader written against the phase document's DDL still works.
--
-- Why the version jumps from 1 to 4: the file name `0004_cloud_audit.sql` is
-- specified by the phase document, and migration file names are published
-- identifiers that support tooling reads. Phases 02 and 03 added no tables -
-- previews, models and hardware plans are all caches on disk rather than truth -
-- so versions 2 and 3 were never used. The migration runner skips absent
-- versions; there is nothing to backfill.
--
-- ROLLBACK. Every object here is new and nothing outside this file references
-- it, so the down migration is four statements, kept as a comment because the
-- runner is forward-only by design:
--
--   DROP TABLE IF EXISTS cloud_budget;
--   DROP TABLE IF EXISTS cloud_cache;
--   DROP TABLE IF EXISTS cloud_calls;
--   DELETE FROM schema_version WHERE version = 4;
--
-- Running those returns the catalog to schema 1 with no loss of photographic
-- truth: everything in these tables is an audit record or a cache, and the
-- decisions themselves live in decision_ledger.

-- ---------------------------------------------------------------------------
-- Every call, whether it happened or not.
--
-- A row is written for a local fallback too, with source='local_fallback' and
-- zero cost. That is the point of the table: "the audit viewer can trace any AI
-- decision to its call, model, tokens, cost and evidence" (acceptance criterion
-- 4) is unanswerable if the decisions that did *not* reach a model are missing.
-- ---------------------------------------------------------------------------
CREATE TABLE cloud_calls (
  -- Deterministic UUID derived from the cache key and a per-run sequence.
  id            TEXT PRIMARY KEY,
  project_id    TEXT,
  task          TEXT    NOT NULL,
  task_version  INTEGER NOT NULL,
  model         TEXT    NOT NULL,
  prompt_hash   TEXT    NOT NULL,
  -- Comma-separated blake3 hashes of the images sent, in order.
  image_hashes  TEXT,
  tokens_in     INTEGER,
  tokens_out    INTEGER,
  cost_usd      REAL,
  latency_ms    INTEGER,
  -- ok | repaired | schema_invalid | fallback | cache | refused
  status        TEXT,
  retry_count   INTEGER,
  -- The decision this call produced, so the Explain panel can walk either way.
  decision_ref  TEXT,
  created_at    TEXT    NOT NULL,

  -- Extensions beyond the phase document's DDL. See ADR-0009.
  provider        TEXT    NOT NULL DEFAULT 'unknown',
  transport       TEXT    NOT NULL DEFAULT 'unknown',
  source          TEXT    NOT NULL DEFAULT 'cloud',
  tier            TEXT,
  fallback_reason TEXT,
  -- Which price table priced this call, so a bill can be reconciled later.
  price_table     INTEGER NOT NULL DEFAULT 0,
  pruned_fields   INTEGER NOT NULL DEFAULT 0,
  coerced_enums   INTEGER NOT NULL DEFAULT 0,
  confidence      REAL,
  -- The agent loop's steps, when this call came from one. JSON.
  scratchpad      TEXT
);

CREATE INDEX idx_cloud_calls_project  ON cloud_calls(project_id, created_at);
CREATE INDEX idx_cloud_calls_task     ON cloud_calls(task, task_version);
CREATE INDEX idx_cloud_calls_decision ON cloud_calls(decision_ref);
CREATE INDEX idx_cloud_calls_prompt   ON cloud_calls(prompt_hash);

-- ---------------------------------------------------------------------------
-- The response cache. Re-running a wedding must be nearly free.
--
-- The key is blake3(task | task_version | prompt_hash | image hashes | model),
-- computed in crates/aura-cloud/src/cache.rs. Bumping a task's VERSION changes
-- every key it owns, which is how a prompt edit invalidates the answers made
-- under the old prompt without anyone having to remember to purge.
-- ---------------------------------------------------------------------------
CREATE TABLE cloud_cache (
  key          TEXT PRIMARY KEY,
  task         TEXT,
  response_json TEXT NOT NULL,
  created_at   TEXT NOT NULL,
  hits         INTEGER NOT NULL DEFAULT 0,

  -- Extensions beyond the phase document's DDL. See ADR-0009.
  task_version INTEGER NOT NULL DEFAULT 0,
  model        TEXT,
  bytes        INTEGER NOT NULL DEFAULT 0,
  last_hit_at  TEXT
);

CREATE INDEX idx_cloud_cache_task ON cloud_cache(task, task_version);
CREATE INDEX idx_cloud_cache_age  ON cloud_cache(created_at);

-- ---------------------------------------------------------------------------
-- Spending caps, one row per project plus one for the month.
--
-- `hard_stop` defaults to 1 because the alternative default is a surprise on
-- somebody's credit card. The month row uses the reserved project id
-- '__month__' so one table answers both questions without a second schema.
-- ---------------------------------------------------------------------------
CREATE TABLE cloud_budget (
  project_id TEXT PRIMARY KEY,
  cap_usd    REAL,
  spent_usd  REAL NOT NULL DEFAULT 0,
  month      TEXT,
  hard_stop  INTEGER NOT NULL DEFAULT 1,

  -- Extensions beyond the phase document's DDL. See ADR-0009.
  calls        INTEGER NOT NULL DEFAULT 0,
  downgrades   INTEGER NOT NULL DEFAULT 0,
  stopped_at   TEXT,
  updated_at   TEXT
);
