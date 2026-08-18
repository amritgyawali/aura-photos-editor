-- Migration 14 - the edit recipe, its history, and what was actually delivered.
--
-- PHASE-14 section 4 names no migration, because its file table lists only the two new
-- crates. It needs one anyway: a recipe that lived solely in a sidecar beside the RAW would
-- be an edit that vanishes when a card is re-copied, an undo history that cannot survive
-- closing the application, and a `render_hash` with nowhere to be stored - and section 6.2
-- says that hash "is stored with exports so a delivered file can always be re-created".
--
-- Section 5's `Recipe` and section 5's Render API are the two shapes every column below
-- comes from. `docs/adr/ADR-0029-render-pipeline.md` records the rest.
--
-- ---------------------------------------------------------------------------
-- WHAT THIS MIGRATION IS FOR
-- ---------------------------------------------------------------------------
--
-- Thirteen migrations have recorded what AURA found and what it decided. This one records
-- what AURA would *do to the pixels* - and it is the first migration in the product whose
-- rows describe an output rather than an observation.
--
-- Five properties are enforced here rather than remembered:
--
-- 1. **There is no path column anywhere in this file, and no `deleted` flag.** Invariant 1
--    says the RAW is never mutated. A schema that could name a destination is a schema that
--    invites a later phase to write to one; a schema that cannot is invariant 1 as a
--    property of the catalog rather than as a rule people remember. Migration 12 made the
--    same argument about culling, and this is the phase where "just save over the original,
--    the recipe is in the database" first sounds reasonable.
--
-- 2. **The recipe body is stored canonically, and its hash is stored beside it.**
--    `edit_recipes.body` is exactly the bytes `aura_recipe::hash::canonical` produced, and
--    `recipe_hash` is BLAKE3 over those bytes. Storing both is not redundancy: the hash is
--    what the render cache keys on and what a re-render is checked against, and re-deriving
--    it on every read would make a corrupted body undetectable.
--
-- 3. **`user_edited_fields` is inside the body, and its size is outside it.**
--    `user_edited_count` is denormalised onto the row for one reason: the develop view
--    needs "which photographs has a person touched" across a whole wedding, and answering
--    it by parsing four thousand JSON documents is the query that makes a panel feel slow.
--    The list itself stays in the body, because it is part of the document that gets hashed
--    and the count is not.
--
-- 4. **History is a separate table, and it is bounded.** `edit_history` holds one row per
--    step with the complete document, which is what makes undo exact (see
--    `aura_recipe::history`). `aura_recipe::history::MAX_ENTRIES` bounds it at 100 steps per
--    photograph, and `RecipeStore::save` trims on write rather than leaving a background
--    task to discover a 40,000-row table.
--
-- 5. **An export records the four things its hash was made of.** `export_renders` stores the
--    render hash *and* the engine, the recipe hash and the output spec that produced it, so
--    `AURA-RENDER-8007` can say which of the four moved instead of only that one did.
--
-- ROLLBACK. Every object here is new and nothing outside this file references it:
--
--   DROP VIEW  IF EXISTS v_develop_coverage;
--   DROP TABLE IF EXISTS export_renders;
--   DROP TABLE IF EXISTS edit_snapshots;
--   DROP TABLE IF EXISTS edit_history;
--   DROP TABLE IF EXISTS edit_recipes;
--   DELETE FROM schema_version WHERE version = 14;
--
-- Running those returns the catalog to schema 13 and **loses every edit**, which is the
-- second migration in a row that is not recomputable. Re-running the pipeline after a
-- rollback produces today's automated decisions and cannot produce a photographer's own
-- sliders. The rollback runbook says to export `edit_recipes` first, and the sidecars beside
-- the RAWs are the second copy that makes that survivable.
--
-- STORAGE. Budgeted at **12 KB per image** in `perf/budgets.toml`, which is the largest
-- per-image figure in the product by an order of magnitude, and the budget file carries the
-- measured decomposition and the two reductions that were considered and rejected.
--
-- The short version: `edit_history.body` is a **complete document per step**, not a delta.
-- `aura_recipe::history` explains why - a chain of deltas is exact only if every delta was
-- computed against the state that actually preceded it, and that stops being true the first
-- time a merge refuses a protected path. An undo that is nearly right is worse than no undo.
-- What bounds the cost is `history::MAX_ENTRIES` and the trim inside `RecipeStore::save`.

-- ---------------------------------------------------------------------------
-- One row per photograph. The current edit.
-- ---------------------------------------------------------------------------
CREATE TABLE edit_recipes (
  photo_id            TEXT    NOT NULL PRIMARY KEY REFERENCES photo(photo_id) ON DELETE CASCADE,
  project_id          TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,

  -- The document's own schema version and the engine that wrote it. Both are inside `body`
  -- as well; they are columns because migration and re-render passes select on them, and
  -- `WHERE schema < 2` must not mean "parse every document".
  schema_ver          INTEGER NOT NULL CHECK (schema_ver >= 1),
  engine              TEXT    NOT NULL,

  -- The canonical JSON, exactly as hashed. See note 2 above.
  body                TEXT    NOT NULL CHECK (length(body) > 2),
  recipe_hash         TEXT    NOT NULL CHECK (length(recipe_hash) = 64),

  -- `Provenance`, lifted out for querying. `source` is the closed set of `EditSource`.
  source              TEXT    NOT NULL DEFAULT 'default'
                      CHECK (source IN ('ai','user','qc','preset','default')),
  confidence          REAL    NOT NULL DEFAULT 1.0
                      CHECK (confidence >= 0.0 AND confidence <= 1.0),
  -- The ledger row that decided this edit, when one did. Not a foreign key, for migration
  -- 13's own reason: compaction may remove the decision and must not remove the edit.
  decision_id         TEXT,
  scene               TEXT,
  style_profile       TEXT,

  -- How many parameters a person has touched. See note 3 above.
  user_edited_count   INTEGER NOT NULL DEFAULT 0 CHECK (user_edited_count >= 0),

  -- Whether this photograph's sidecar on disk is in step with this row. Set to 0 by a save
  -- that could not write the sidecar, so a later pass can retry rather than a photographer
  -- discovering it when they hand the folder to somebody.
  sidecar_written     INTEGER NOT NULL DEFAULT 0 CHECK (sidecar_written IN (0,1)),

  updated_ts          INTEGER NOT NULL,
  updated_at          TEXT    NOT NULL
) STRICT;

-- Every edit in one wedding, most recently touched first. What the develop filmstrip reads.
CREATE INDEX idx_edit_recipes_project ON edit_recipes(project_id, updated_ts DESC);

-- The photographs a person has touched. A partial index: the column is zero for the
-- overwhelming majority, and the only query that reads it asks about the ones where it is
-- not - which is the query behind "show me what I have already been through".
CREATE INDEX idx_edit_recipes_user_edited
  ON edit_recipes(project_id) WHERE user_edited_count > 0;

-- Photographs whose sidecar is behind. Retried by the next save pass.
CREATE INDEX idx_edit_recipes_sidecar
  ON edit_recipes(project_id) WHERE sidecar_written = 0;

-- ---------------------------------------------------------------------------
-- One row per step. Undo, redo, and what one step changed.
-- ---------------------------------------------------------------------------
CREATE TABLE edit_history (
  photo_id            TEXT    NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,
  seq                 INTEGER NOT NULL CHECK (seq > 0),

  source              TEXT    NOT NULL
                      CHECK (source IN ('ai','user','qc','preset','default')),
  -- A JSON array of dotted paths, sorted, e.g. `["global.exposure","global.shadows"]`.
  -- Stored rather than derived, because deriving it needs the previous body and the
  -- previous body is exactly what a trimmed history no longer has.
  changed             TEXT    NOT NULL DEFAULT '[]',
  -- A short line for the history panel. English today; when this product is translated it
  -- becomes a code, exactly as phase 09's reasons did. Recorded here so that conversation
  -- happens once rather than per-phase.
  label               TEXT    NOT NULL DEFAULT '',

  -- The complete document after this step. See note 4 above.
  body                TEXT    NOT NULL,

  at_ts               INTEGER NOT NULL,
  at                  TEXT    NOT NULL,

  PRIMARY KEY (photo_id, seq)
) STRICT, WITHOUT ROWID;

-- ---------------------------------------------------------------------------
-- Named points a photographer can come back to. Not bounded, because a snapshot is
-- something a person chose to keep and the product does not get to decide it has kept too
-- many of them.
-- ---------------------------------------------------------------------------
CREATE TABLE edit_snapshots (
  photo_id            TEXT    NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,
  name                TEXT    NOT NULL CHECK (length(trim(name)) > 0),
  body                TEXT    NOT NULL,
  at_ts               INTEGER NOT NULL,
  at                  TEXT    NOT NULL,

  PRIMARY KEY (photo_id, name)
) STRICT, WITHOUT ROWID;

-- ---------------------------------------------------------------------------
-- What was delivered, and everything needed to make it again.
-- ---------------------------------------------------------------------------
CREATE TABLE export_renders (
  render_hash         TEXT    NOT NULL PRIMARY KEY CHECK (length(render_hash) = 64),
  photo_id            TEXT    NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,
  project_id          TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,

  -- The four inputs of the hash, so a mismatch can say which one moved. See note 5 above.
  content_hash        TEXT    NOT NULL,
  recipe_hash         TEXT    NOT NULL,
  engine              TEXT    NOT NULL,
  output_spec         TEXT    NOT NULL,

  -- What was written. `level` is the `RenderLevel` slug; `backend` is `cpu` or the
  -- accelerator's name, recorded because a delivered file rendered on a path that later
  -- fails its parity gate is the one case where knowing which path matters.
  level               TEXT    NOT NULL CHECK (level IN ('proxy2048','screen','full')),
  colour_space        TEXT    NOT NULL CHECK (colour_space IN ('srgb','adobe_rgb','display_p3')),
  bit_depth           INTEGER NOT NULL CHECK (bit_depth IN (8, 16)),
  backend             TEXT    NOT NULL DEFAULT 'cpu',

  width               INTEGER NOT NULL CHECK (width > 0),
  height              INTEGER NOT NULL CHECK (height > 0),
  ms                  INTEGER NOT NULL DEFAULT 0 CHECK (ms >= 0),

  at_ts               INTEGER NOT NULL,
  at                  TEXT    NOT NULL
) STRICT;

-- Every export of one wedding, newest first.
CREATE INDEX idx_export_renders_project ON export_renders(project_id, at_ts DESC);

-- Every export of one photograph. What "has this already been delivered" asks.
CREATE INDEX idx_export_renders_photo ON export_renders(photo_id, at_ts DESC);

-- ---------------------------------------------------------------------------
-- How much of a wedding has an edit, and how much of it a person has been through.
--
-- The denominator is **every photograph in the project**, as phase 09's and phase 10's are,
-- and not the delivered gallery. A develop coverage of 100 % over a culled gallery would
-- say nothing about the four hundred frames nobody has looked at, and phase 27's QC pass
-- reads exactly this view to decide where to start.
-- ---------------------------------------------------------------------------
CREATE VIEW v_develop_coverage AS
SELECT
  p.project_id                                              AS project_id,
  COUNT(*)                                                  AS images,
  COUNT(r.photo_id)                                         AS with_recipe,
  SUM(CASE WHEN r.source = 'ai'   THEN 1 ELSE 0 END)         AS from_ai,
  SUM(CASE WHEN r.source = 'user' THEN 1 ELSE 0 END)         AS from_user,
  SUM(CASE WHEN r.user_edited_count > 0 THEN 1 ELSE 0 END)   AS touched_by_hand,
  SUM(CASE WHEN r.sidecar_written = 0 THEN 1 ELSE 0 END)     AS sidecar_behind
FROM photo p
LEFT JOIN edit_recipes r ON r.photo_id = p.photo_id
GROUP BY p.project_id;
