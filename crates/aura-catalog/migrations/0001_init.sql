-- AURA catalog schema v1. FROZEN. Additive migrations only; any change to an
-- existing column requires an ADR and a new numbered migration file.
--
-- Conventions used throughout:
--   * IDs are TEXT with a type prefix, e.g. 'pht_018f...'. Typed in Rust.
--   * Timestamps are TEXT in RFC-3339 UTC with a Z suffix, sortable as text.
--   * Booleans are INTEGER 0/1 with a CHECK constraint.
--   * Enums are TEXT with a CHECK constraint, never integers, so a hex dump of
--     the catalog is readable during a support call.
--
-- The schema_version row is written by the migration runner, not by this file,
-- because the runner records the app version and the migration hash.

PRAGMA foreign_keys = ON;

----------------------------------------------------------------------
-- Schema versioning and provenance
----------------------------------------------------------------------
CREATE TABLE schema_version (
  version        INTEGER NOT NULL PRIMARY KEY,
  applied_at     TEXT    NOT NULL,
  app_version    TEXT    NOT NULL,
  migration_hash TEXT    NOT NULL           -- blake3 of the migration file
) STRICT;

CREATE TABLE catalog_meta (
  key   TEXT NOT NULL PRIMARY KEY,
  value TEXT NOT NULL
) STRICT;
-- Seeded rows: catalog_uuid, created_at, created_by_app_version, machine_hash.

----------------------------------------------------------------------
-- Projects: one wedding, one project
----------------------------------------------------------------------
CREATE TABLE project (
  project_id     TEXT NOT NULL PRIMARY KEY,
  name           TEXT NOT NULL,
  couple_label   TEXT,                        -- C2. Optional, photographer's words
  event_date     TEXT,                        -- date only, no time
  timezone       TEXT NOT NULL DEFAULT 'UTC', -- IANA name, for timeline ordering
  status         TEXT NOT NULL DEFAULT 'active'
                 CHECK (status IN ('active','archived','delivered')),
  created_at     TEXT NOT NULL,
  updated_at     TEXT NOT NULL,
  deleted_at     TEXT                         -- soft delete, purge is explicit
) STRICT;

CREATE INDEX idx_project_status_date ON project(status, event_date DESC);

-- Consent lives in its own table so it can be revoked and audited without
-- touching the project row, and so a JOIN is required to discover any
-- permission. There is exactly one row per project, created with the project.
CREATE TABLE project_consent (
  project_id            TEXT NOT NULL PRIMARY KEY REFERENCES project(project_id) ON DELETE CASCADE,
  cloud_metadata        INTEGER NOT NULL DEFAULT 0 CHECK (cloud_metadata IN (0,1)),
  cloud_derived_imagery INTEGER NOT NULL DEFAULT 0 CHECK (cloud_derived_imagery IN (0,1)),
  cloud_full_imagery    INTEGER NOT NULL DEFAULT 0 CHECK (cloud_full_imagery IN (0,1)),
  telemetry             INTEGER NOT NULL DEFAULT 0 CHECK (telemetry IN (0,1)),
  improve_models        INTEGER NOT NULL DEFAULT 0 CHECK (improve_models IN (0,1)),
  granted_at            TEXT,
  granted_by            TEXT,
  expires_at            TEXT,
  revoked_at            TEXT
) STRICT;

-- Append-only audit of every consent change. Never updated, never deleted.
CREATE TABLE consent_ledger (
  entry_id   INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
  project_id TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  changed_at TEXT    NOT NULL,
  field      TEXT    NOT NULL,
  old_value  INTEGER NOT NULL,
  new_value  INTEGER NOT NULL,
  actor      TEXT    NOT NULL DEFAULT 'user'
) STRICT;

----------------------------------------------------------------------
-- Where the files came from
----------------------------------------------------------------------
CREATE TABLE source_root (
  root_id       TEXT NOT NULL PRIMARY KEY,
  project_id    TEXT NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  abs_path      TEXT NOT NULL,               -- C2, local only, never egressed
  volume_label  TEXT,
  volume_serial TEXT,                        -- lets us recognise a reconnected drive
  is_removable  INTEGER NOT NULL DEFAULT 0 CHECK (is_removable IN (0,1)),
  added_at      TEXT NOT NULL,
  last_seen_at  TEXT,
  UNIQUE (project_id, abs_path)
) STRICT;

----------------------------------------------------------------------
-- Photos: the logical unit. One photo may have several files
-- (RAW + camera JPEG + XMP sidecar).
----------------------------------------------------------------------
CREATE TABLE photo (
  photo_id      TEXT NOT NULL PRIMARY KEY,
  project_id    TEXT NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  capture_time  TEXT,                        -- from EXIF, UTC, may be NULL
  capture_time_source TEXT NOT NULL DEFAULT 'unknown'
                 CHECK (capture_time_source IN ('exif_original','exif_digitized','file_mtime','unknown')),
  camera_make   TEXT,
  camera_model  TEXT,
  camera_serial TEXT,
  lens_model    TEXT,
  iso           INTEGER,
  aperture      REAL,
  shutter_us    INTEGER,                     -- microseconds, integer for exactness
  focal_len_mm  REAL,
  flash_fired   INTEGER CHECK (flash_fired IN (0,1)),
  orientation   INTEGER NOT NULL DEFAULT 1 CHECK (orientation BETWEEN 1 AND 8),
  width_px      INTEGER,
  height_px     INTEGER,
  sub_sec       INTEGER,
  -- Clock-aligned capture time. Authoritative ordering everywhere in the product.
  timeline_time TEXT,
  -- Populated in PHASE-02 onward. Present now so no migration is needed later.
  primary_file_id TEXT,
  created_at    TEXT NOT NULL,
  updated_at    TEXT NOT NULL
) STRICT;

CREATE INDEX idx_photo_project_time  ON photo(project_id, capture_time);
CREATE INDEX idx_photo_timeline      ON photo(project_id, timeline_time);
CREATE INDEX idx_photo_project_id_pk ON photo(project_id, photo_id);
CREATE INDEX idx_photo_camera        ON photo(project_id, camera_model);

----------------------------------------------------------------------
-- Cameras: one row per body seen in the project. Clock offsets live here.
----------------------------------------------------------------------
CREATE TABLE camera (
  camera_id       TEXT NOT NULL PRIMARY KEY,
  project_id      TEXT NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  make            TEXT,
  model           TEXT,
  body_serial     TEXT NOT NULL,
  shooter_label   TEXT NOT NULL DEFAULT 'primary',
  clock_offset_ms INTEGER NOT NULL DEFAULT 0,
  offset_confidence REAL NOT NULL DEFAULT 0.0,
  offset_source   TEXT NOT NULL DEFAULT 'none'
                  CHECK (offset_source IN ('none','estimated','manual')),
  photo_count     INTEGER NOT NULL DEFAULT 0,
  created_at      TEXT NOT NULL,
  updated_at      TEXT NOT NULL,
  UNIQUE (project_id, body_serial)
) STRICT;

CREATE INDEX idx_camera_project ON camera(project_id, shooter_label);

----------------------------------------------------------------------
-- Files: content-addressed. This table is the dedupe mechanism.
----------------------------------------------------------------------
CREATE TABLE photo_file (
  file_id       TEXT NOT NULL PRIMARY KEY,
  photo_id      TEXT NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,
  project_id    TEXT NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  root_id       TEXT NOT NULL REFERENCES source_root(root_id) ON DELETE RESTRICT,
  rel_path      TEXT NOT NULL,               -- relative to root, forward slashes
  file_name     TEXT NOT NULL,
  ext           TEXT NOT NULL,               -- lower case, no dot
  kind          TEXT NOT NULL
                 CHECK (kind IN ('raw','jpeg','heif','tiff','png','sidecar_xmp','video','other')),
  role          TEXT NOT NULL
                 CHECK (role IN ('primary','companion_jpeg','sidecar','derivative')),
  size_bytes    INTEGER NOT NULL CHECK (size_bytes >= 0),
  content_hash  TEXT NOT NULL,               -- blake3 hex, 64 chars
  mtime         TEXT NOT NULL,
  -- Cheap change detector: if size and mtime match, skip rehashing on rescan.
  fast_key      TEXT NOT NULL,               -- '<size>:<mtime_ms>'
  first_seen_at TEXT NOT NULL,
  last_seen_at  TEXT NOT NULL,
  -- Which import last saw this file. Absence is decided by "the current run did
  -- not see it", never by comparing timestamps: a frozen or corrected clock must
  -- not be able to mark a whole card as missing.
  last_seen_import TEXT,
  is_present    INTEGER NOT NULL DEFAULT 1 CHECK (is_present IN (0,1)),
  UNIQUE (project_id, content_hash, rel_path),
  UNIQUE (root_id, rel_path)
) STRICT;

CREATE INDEX idx_file_hash        ON photo_file(project_id, content_hash);
CREATE INDEX idx_file_photo       ON photo_file(photo_id, role);
CREATE INDEX idx_file_fast_key    ON photo_file(root_id, fast_key);
CREATE INDEX idx_file_missing     ON photo_file(project_id, is_present) WHERE is_present = 0;

----------------------------------------------------------------------
-- EXIF: the raw key/value truth, kept separately from the promoted columns
-- on photo so that nothing is silently lost by our own parsing choices.
----------------------------------------------------------------------
CREATE TABLE exif (
  file_id  TEXT NOT NULL REFERENCES photo_file(file_id) ON DELETE CASCADE,
  tag      TEXT NOT NULL,
  value    TEXT NOT NULL,
  PRIMARY KEY (file_id, tag)
) STRICT, WITHOUT ROWID;

-- GPS is stored separately and is class C2. A wedding venue location plus a
-- date identifies a real event, so it is opt-in per project to use it at all.
CREATE TABLE photo_gps (
  photo_id   TEXT NOT NULL PRIMARY KEY REFERENCES photo(photo_id) ON DELETE CASCADE,
  lat        REAL NOT NULL,
  lon        REAL NOT NULL,
  altitude_m REAL,
  source     TEXT NOT NULL DEFAULT 'exif'
) STRICT;

----------------------------------------------------------------------
-- Preview cache index. Bytes live on disk under cache_dir; this table only
-- records what exists so a deleted cache is self-healing.
----------------------------------------------------------------------
CREATE TABLE preview (
  photo_id     TEXT NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,
  tier         INTEGER NOT NULL CHECK (tier IN (1,2,3)),
  rel_cache_path TEXT NOT NULL,
  width_px     INTEGER NOT NULL,
  height_px    INTEGER NOT NULL,
  source       TEXT NOT NULL CHECK (source IN ('embedded','decoded')),
  bytes        INTEGER NOT NULL,
  built_at     TEXT NOT NULL,
  stage_version INTEGER NOT NULL DEFAULT 1,
  PRIMARY KEY (photo_id, tier)
) STRICT, WITHOUT ROWID;

----------------------------------------------------------------------
-- Import runs and quarantine
----------------------------------------------------------------------
CREATE TABLE import_run (
  import_id       TEXT NOT NULL PRIMARY KEY,
  project_id      TEXT NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  root_id         TEXT NOT NULL REFERENCES source_root(root_id) ON DELETE CASCADE,
  state           TEXT NOT NULL DEFAULT 'scanning'
                   CHECK (state IN ('scanning','importing','paused','completed','failed','cancelled')),
  started_at      TEXT NOT NULL,
  finished_at     TEXT,
  files_discovered INTEGER NOT NULL DEFAULT 0,
  files_imported   INTEGER NOT NULL DEFAULT 0,
  files_duplicate  INTEGER NOT NULL DEFAULT 0,
  files_quarantined INTEGER NOT NULL DEFAULT 0,
  bytes_hashed     INTEGER NOT NULL DEFAULT 0,
  app_version      TEXT NOT NULL,
  error_code       TEXT
) STRICT;

CREATE INDEX idx_import_project ON import_run(project_id, started_at DESC);

CREATE TABLE quarantine (
  quarantine_id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
  project_id  TEXT NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  import_id   TEXT REFERENCES import_run(import_id) ON DELETE SET NULL,
  abs_path    TEXT NOT NULL,
  error_code  TEXT NOT NULL,
  detail      TEXT NOT NULL,
  attempts    INTEGER NOT NULL DEFAULT 1,
  first_at    TEXT NOT NULL,
  last_at     TEXT NOT NULL,
  resolved_at TEXT,
  UNIQUE (project_id, abs_path)
) STRICT;

CREATE INDEX idx_quarantine_open ON quarantine(project_id, resolved_at) WHERE resolved_at IS NULL;

----------------------------------------------------------------------
-- Ingest journal: crash-safe per-path progress, the resume mechanism.
----------------------------------------------------------------------
CREATE TABLE ingest_journal (
  project_id TEXT NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  abs_path   TEXT NOT NULL,
  import_id  TEXT REFERENCES import_run(import_id) ON DELETE SET NULL,
  phase      TEXT NOT NULL
             CHECK (phase IN ('discovered','hashed','exif','inserted','skipped','failed')),
  content_hash TEXT,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (project_id, abs_path)
) STRICT, WITHOUT ROWID;

CREATE INDEX idx_journal_phase ON ingest_journal(project_id, phase);

----------------------------------------------------------------------
-- Job graph. Owned by aura-jobs, defined here because it must migrate with
-- the catalog and because resume depends on it surviving a crash.
----------------------------------------------------------------------
CREATE TABLE task (
  task_id       TEXT NOT NULL PRIMARY KEY,   -- '{stage}:{subject}:{stage_version}'
  project_id    TEXT NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  run_id        TEXT NOT NULL,
  stage         TEXT NOT NULL,
  subject_id    TEXT NOT NULL,               -- photo_id, file_id or import_id
  stage_version INTEGER NOT NULL,
  state         TEXT NOT NULL DEFAULT 'pending'
                 CHECK (state IN ('pending','ready','running','succeeded','failed','quarantined','skipped')),
  priority      INTEGER NOT NULL DEFAULT 100,
  attempts      INTEGER NOT NULL DEFAULT 0,
  max_attempts  INTEGER NOT NULL DEFAULT 3,
  cost_cpu_ms   INTEGER NOT NULL DEFAULT 0,
  cost_gpu_ms   INTEGER NOT NULL DEFAULT 0,
  cost_ram_mb   INTEGER NOT NULL DEFAULT 0,
  cost_vram_mb  INTEGER NOT NULL DEFAULT 0,
  lease_owner   TEXT,                        -- worker uuid
  lease_expires_at TEXT,
  heartbeat_at  TEXT,
  error_code    TEXT,
  error_detail  TEXT,
  created_at    TEXT NOT NULL,
  updated_at    TEXT NOT NULL,
  finished_at   TEXT
) STRICT;

-- Deliberately NOT a partial index. SQLite cannot prove that `state = 'ready'`
-- satisfies `WHERE state IN ('ready','pending')`, so a partial index turns the
-- hottest query in the scheduler into a table scan. Proven by
-- tests/catalog.rs::hot_queries_use_indexes_not_table_scans.
CREATE INDEX idx_task_claimable ON task(project_id, state, priority, created_at);
CREATE INDEX idx_task_lease     ON task(state, lease_expires_at) WHERE state = 'running';
CREATE INDEX idx_task_run       ON task(run_id, state);
CREATE INDEX idx_task_subject   ON task(subject_id, stage);

CREATE TABLE task_dep (
  task_id     TEXT NOT NULL REFERENCES task(task_id) ON DELETE CASCADE,
  depends_on  TEXT NOT NULL REFERENCES task(task_id) ON DELETE CASCADE,
  PRIMARY KEY (task_id, depends_on),
  CHECK (task_id <> depends_on)
) STRICT, WITHOUT ROWID;

CREATE INDEX idx_dep_reverse ON task_dep(depends_on);

----------------------------------------------------------------------
-- Decision ledger. Empty in phase 01, created now because PHASE-13 must be
-- able to explain decisions made by earlier phases retrospectively.
----------------------------------------------------------------------
CREATE TABLE decision_ledger (
  decision_id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
  project_id  TEXT NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  photo_id    TEXT REFERENCES photo(photo_id) ON DELETE CASCADE,
  run_id      TEXT NOT NULL,
  stage       TEXT NOT NULL,
  stage_version INTEGER NOT NULL,
  decision    TEXT NOT NULL,
  confidence  REAL,
  reason_json TEXT NOT NULL DEFAULT '{}',
  model_id    TEXT,
  decided_at  TEXT NOT NULL,
  reverted_at TEXT,
  reverted_by TEXT
) STRICT;

CREATE INDEX idx_decision_photo ON decision_ledger(photo_id, stage);
CREATE INDEX idx_decision_run   ON decision_ledger(run_id, decided_at);

----------------------------------------------------------------------
-- Settings, per catalog
----------------------------------------------------------------------
CREATE TABLE setting (
  key        TEXT NOT NULL PRIMARY KEY,
  value_json TEXT NOT NULL,
  updated_at TEXT NOT NULL
) STRICT;

----------------------------------------------------------------------
-- Convenience views. Views are safe to change without a migration because
-- nothing persists through them.
----------------------------------------------------------------------
CREATE VIEW v_project_counts AS
SELECT p.project_id,
       p.name,
       COUNT(DISTINCT ph.photo_id) AS photo_count,
       COUNT(f.file_id)            AS file_count,
       COALESCE(SUM(f.size_bytes), 0) AS bytes_total
FROM project p
LEFT JOIN photo ph ON ph.project_id = p.project_id
LEFT JOIN photo_file f ON f.photo_id = ph.photo_id
WHERE p.deleted_at IS NULL
GROUP BY p.project_id;
