-- Migration 13 - what the product decided, why, how sure it was, and what it looked at.
--
-- PHASE-13 section 4 names this file `0013_ledger.sql` and the three tables it must
-- contain: `decisions`, `decision_reasons` and `calibration_models`. All three are here.
-- Section 5 freezes `Decision` and `Reason`; every column below either comes from those
-- shapes or is listed in `docs/adr/ADR-0027-decision-ledger-and-confidence.md`.
--
-- ---------------------------------------------------------------------------
-- WHAT THIS MIGRATION IS FOR
-- ---------------------------------------------------------------------------
--
-- Twelve migrations have recorded what AURA *found*. This one records what AURA *did*, and
-- the difference changes what the schema has to guarantee. A wrong measurement is a bug
-- somebody fixes. A decision nobody can reconstruct is a photographer on the telephone
-- asking why four hundred frames are missing from a wedding that was delivered eight months
-- ago, and an engineer with nothing to say.
--
-- Six properties are enforced here rather than remembered:
--
-- 1. **A decision without a reason cannot be stored.** Invariant 2 as a CHECK the database
--    refuses to break. `reason_count` is denormalised onto `decisions` for exactly this
--    reason: SQLite cannot express "has at least one child row" as a constraint, so the
--    count lives on the parent and the writer sets both inside one transaction. Section
--    10.1's explanation-coverage gate is 100 %, and this is why it can be.
--
-- 2. **The table is append-only, and a trigger says so.** `decisions_no_update` aborts
--    every UPDATE. Section 6.3: "append-only, never updated; corrections create new
--    decisions that supersede old ones, preserving history." A correction is a new row
--    whose `supersedes` points backwards; there is no forward-pointing flag on the old row
--    because setting one would be the UPDATE this trigger exists to forbid.
--
--    DELETE is *not* blocked, and the asymmetry is deliberate: compaction (section 6.3) and
--    the project cascade both need it. An UPDATE rewrites history; a DELETE removes a row
--    that a later row already points past, and the compaction policy in
--    `aura_explain::ledger` is what bounds which ones.
--
-- 3. **Evidence is never a pixel.** `decision_reasons.evidence` holds a crop rectangle,
--    a list of photo ids or a list of named parameter deltas - four numbers, ids, or names
--    and floats. There is no column, no BLOB and no variant of `Evidence` that could hold
--    image bytes, which is what makes section 10.1's "support bundle contains no pixels" a
--    fact about the schema rather than a promise about the exporter.
--
-- 4. **Confidence is two numbers.** `raw_confidence` is what the deciding code believed;
--    `calibrated_confidence` is what that belief is worth. Storing only the second would
--    make a re-calibration unfalsifiable - there would be nothing left to re-map - and
--    storing only the first would make the autonomy band a guess. `calibration_ver = 0` is
--    the unfitted identity map, which is what ships.
--
-- 5. **The autonomy band is stored, not recomputed on read.** A band is what the product
--    was *allowed to do at the time*, and a build that recomputed it from today's config
--    would answer "what did it do" with "what would it do now". Those are different
--    questions and only one of them is a support answer.
--
-- 6. **A reason stores its code.** `text` is NULL whenever the sentence is the registry's
--    own template, which is the overwhelming majority. Phase 09's decision, applied a
--    fourth time and with the largest table it has met: a catalog full of English cannot be
--    translated, and a release that improves a sentence must not have to rewrite a ledger.
--
-- ROLLBACK. Every object here is new and nothing outside this file references it, so the
-- down migration is a list of drops, kept as a comment because the runner is forward-only
-- by design:
--
--   DROP VIEW    IF EXISTS v_ledger_coverage;
--   DROP TRIGGER IF EXISTS decisions_no_update;
--   DROP TABLE   IF EXISTS decision_reasons;
--   DROP TABLE   IF EXISTS calibration_models;
--   DROP TABLE   IF EXISTS decisions;
--   DELETE FROM schema_version WHERE version = 13;
--
-- Running those returns the catalog to schema 12. **Nothing in this migration is
-- recomputable**, and that is the first time that sentence has been true in this product:
-- migrations 5 to 12 all store conclusions that a re-run reproduces, and this one stores
-- history. Re-running the pipeline after a rollback produces today's decisions, not the
-- ones a client's gallery was actually built from. The rollback runbook therefore says to
-- export `decisions` and `decision_reasons` first, and it is the only migration whose
-- runbook says to export the *whole* table.
--
-- STORAGE. Section 11 budgets 6 MB per 1,000 images for a full pipeline, which is roughly
-- 3-6 KB per image across every decision made about it. Four decisions keep it there: a
-- reason stores its code and stores `text` only when the sentence differs from the
-- registry's template; `outputs_json` is canonical compact JSON with quantised floats;
-- evidence is NULL for the `none` kind rather than an empty object; and the compaction
-- policy keeps the newest decision per subject plus every user override.
-- `crates/aura-perf/tests/ledger_budgets.rs` asserts the measured figure.

-- ---------------------------------------------------------------------------
-- One row per decision. Append-only.
-- ---------------------------------------------------------------------------
CREATE TABLE decisions (
  decision_id         TEXT    NOT NULL PRIMARY KEY,
  project_id          TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,

  -- One of the six `DecisionKind` slugs. A CHECK list here and not for `subject_id`,
  -- because the vocabulary is closed by section 5 and a seventh kind is a contract change
  -- with an ADR behind it - so the second copy of the list is a second thing that has to be
  -- changed deliberately, which is the point.
  kind                TEXT    NOT NULL
                      CHECK (kind IN ('cull','edit','retouch','qc','curate','export')),

  -- What it was about, as the two halves of `DecisionSubject`. Not a foreign key: a
  -- decision outlives its subject on purpose. A moment is rebuilt by every re-grouping and
  -- a segment by every re-analysis, and a ledger that a re-grouping could cascade-delete
  -- would be a ledger that lost its history to an unrelated pass.
  subject_kind        TEXT    NOT NULL
                      CHECK (subject_kind IN ('image','moment','segment','gallery')),
  subject_id          TEXT    NOT NULL,

  -- Section 6.3's hash over analysis outputs, config versions and model versions. Stored as
  -- the signed reinterpretation of the u64, exactly as `cull_run.deterministic_hash` is:
  -- SQLite has no unsigned integer, and a hash written as TEXT would be sixteen bytes of
  -- hex per decision for no gain.
  inputs_hash         INTEGER NOT NULL DEFAULT 0,
  -- What was decided, as canonical compact JSON: sorted keys, no whitespace, floats
  -- quantised by the producer. `aura replay` compares this byte for byte, so a
  -- non-canonical writer would produce false drift reports.
  outputs_json        TEXT    NOT NULL DEFAULT '{}',

  -- What the deciding code believed, and what that belief is worth. See note 4 above.
  raw_confidence      REAL    NOT NULL DEFAULT 0.0
                      CHECK (raw_confidence >= 0.0 AND raw_confidence <= 1.0),
  calibrated_confidence REAL  NOT NULL DEFAULT 0.0
                      CHECK (calibrated_confidence >= 0.0 AND calibrated_confidence <= 1.0),
  -- Which calibration table mapped the second from the first. 0 is the identity map.
  calibration_ver     INTEGER NOT NULL DEFAULT 0 CHECK (calibration_ver >= 0),

  -- What the product was allowed to do with it, at the time. See note 5 above.
  autonomy            TEXT    NOT NULL DEFAULT 'require_review'
                      CHECK (autonomy IN ('auto','auto_zero_touch','suggest','require_review')),
  -- 'local', 'cloud' or 'user'. Cloud decisions are calibrated separately because their
  -- error profile differs (section 6.1), and a user's own decision is calibrated against
  -- nothing because there is no outcome for it to be wrong about.
  source              TEXT    NOT NULL DEFAULT 'local'
                      CHECK (source IN ('local','cloud','user')),

  -- `[[name, version], ...]`, sorted by name. JSON rather than a join table because
  -- nothing queries by model version - `AURA-ML-5057` compares the whole list - and a join
  -- table would be two more rows per decision against a 6 MB budget.
  model_versions      TEXT    NOT NULL DEFAULT '[]',
  config_versions     TEXT    NOT NULL DEFAULT '[]',

  -- How long the deciding code took. Telemetry, never an input to anything: a decision that
  -- differed because one machine was slower would break determinism.
  ms                  INTEGER NOT NULL DEFAULT 0 CHECK (ms >= 0),

  -- Milliseconds since the epoch, for ordering, and the human-readable copy the catalog
  -- writes everywhere else. Both, for the reason migration 12 keeps `timeline_ts` beside
  -- `created_at`: the index seeks on the integer and the support engineer reads the text.
  created_ts          INTEGER NOT NULL,
  created_at          TEXT    NOT NULL,

  -- The decision this one replaces. Section 6.3.
  --
  -- **Not a foreign key**, and the reason is the trigger below. Every referential action
  -- SQLite offers on delete is either an UPDATE of this column (`SET NULL`, `SET DEFAULT`),
  -- which `decisions_no_update` aborts, or a CASCADE, which would delete a correction
  -- because the thing it corrected was compacted away. Both are wrong: a backward pointer
  -- into a compacted row must simply dangle, and a reader that cannot resolve one reports
  -- "the earlier decision is no longer on record" rather than losing the newer one.
  --
  -- `RESTRICT` would have been the fourth option and it is worse than dangling: it would
  -- make compaction impossible for exactly the rows compaction exists to remove.
  supersedes          TEXT,

  -- How many reasons are in `decision_reasons` for this row. Invariant 2, as something the
  -- database refuses to break. See note 1 above.
  reason_count        INTEGER NOT NULL CHECK (reason_count > 0 AND reason_count <= 6),

  -- A decision cannot replace itself.
  CHECK (supersedes IS NULL OR supersedes <> decision_id)
) STRICT;

-- The ledger of one wedding, in the order it happened. The support bundle, the outline and
-- the compaction pass all read exactly this.
CREATE INDEX idx_decisions_project ON decisions(project_id, created_ts);

-- Every decision about one thing, newest first. What the Explain panel opens with, and
-- what `history` seeks on.
CREATE INDEX idx_decisions_subject ON decisions(subject_kind, subject_id, created_ts DESC);

-- Which rows have been superseded. A partial index: the column is NULL for the
-- overwhelming majority of decisions, and the only queries that read it - "is this still in
-- force" and the compaction pass - are asking about the ones where it is not.
CREATE INDEX idx_decisions_supersedes ON decisions(supersedes) WHERE supersedes IS NOT NULL;

-- ---------------------------------------------------------------------------
-- Append-only, enforced.
--
-- The one trigger in this catalog, and it is here rather than in a code review because
-- "never update the ledger" is a rule that holds until the first afternoon somebody needs
-- to fix one field. RAISE(ABORT) turns that afternoon into a failing test.
--
-- DELETE is deliberately not blocked. See note 2 above.
-- ---------------------------------------------------------------------------
CREATE TRIGGER decisions_no_update
BEFORE UPDATE ON decisions
BEGIN
  SELECT RAISE(ABORT, 'the decision ledger is append-only: record a superseding decision instead');
END;

-- ---------------------------------------------------------------------------
-- Why. One row per reason, in the order the deciding code ranked them.
-- ---------------------------------------------------------------------------
CREATE TABLE decision_reasons (
  decision_id         TEXT    NOT NULL REFERENCES decisions(decision_id) ON DELETE CASCADE,
  -- Position in the decision's own ranking, zero-based. Part of the key rather than a
  -- sort column, because the order *is* the explanation: the panel shows the top three.
  ix                  INTEGER NOT NULL CHECK (ix >= 0 AND ix < 6),

  -- The stable slug, documented in `docs/reason-codes.md`. Not a CHECK list: the registry
  -- spans five phases' vocabularies and grows with every deciding phase, and a copy of it
  -- in SQL would be a copy that goes stale. `aura_explain::reason::Catalog` refuses an
  -- unknown code before a row is ever written - `AURA-ML-5054`.
  code                TEXT    NOT NULL,
  -- NULL when the sentence is the registry's own template for this code, which is the
  -- common case. See note 6 above.
  text                TEXT,
  -- How much this moved the decision. Positive toward it, negative against.
  weight              REAL    NOT NULL DEFAULT 0.0,

  -- What the photographer can look at. 'none', 'crop', 'frames' or 'params'.
  evidence_kind       TEXT    NOT NULL DEFAULT 'none'
                      CHECK (evidence_kind IN ('none','crop','frames','params')),
  -- The evidence itself, as JSON: {"x":..,"y":..,"w":..,"h":..} for a crop, ["pht_..",..]
  -- for frames, [["temperature_k",-610.0],..] for params. NULL for 'none' rather than an
  -- empty object, because four thousand `{}` strings is four thousand rows of nothing.
  evidence            TEXT,

  PRIMARY KEY (decision_id, ix),

  -- Evidence and its kind agree, in both directions. The second half matters more than the
  -- first: a row that claimed a crop and stored nothing would render an empty rectangle
  -- over a photograph and call it proof.
  CHECK ((evidence_kind = 'none' AND evidence IS NULL)
      OR (evidence_kind <> 'none' AND evidence IS NOT NULL))
) WITHOUT ROWID;

-- ---------------------------------------------------------------------------
-- The calibration models, one per (kind, source).
--
-- Section 6.1: per-decision-type isotonic regression fitted on labelled outcomes, with
-- cloud decisions calibrated separately from local ones. What ships is the identity map at
-- version 0 for every pair, because fitting needs labelled outcomes from real weddings and
-- there are none in this repository - and `calibration_ver = 0` is what makes an unfitted
-- map impossible to mistake for a fitted one.
--
-- A table rather than a shipped file, even though the artefacts ship with the release: a
-- calibration is refreshed with each model release *and* refitted locally from a
-- photographer's own overrides in phase 30, and the second one has nowhere to live but the
-- catalog.
-- ---------------------------------------------------------------------------
CREATE TABLE calibration_models (
  kind                TEXT    NOT NULL
                      CHECK (kind IN ('cull','edit','retouch','qc','curate','export')),
  source              TEXT    NOT NULL CHECK (source IN ('local','cloud')),
  -- 0 is the identity map. Any other value is a fitted table, and the two can never be
  -- confused because nothing else may be written as 0.
  version             INTEGER NOT NULL DEFAULT 0 CHECK (version >= 0),
  -- 'identity', 'isotonic' or 'temperature'. Section 6.1 allows temperature scaling where
  -- the relationship is monotonic and the data are thin, and says so out loud rather than
  -- letting a two-knot isotonic fit pretend to be one.
  method              TEXT    NOT NULL DEFAULT 'identity'
                      CHECK (method IN ('identity','isotonic','temperature')),
  -- The fitted map as JSON `[[raw, calibrated], ...]`, ascending in `raw`. Empty for the
  -- identity map.
  knots               TEXT    NOT NULL DEFAULT '[]',

  -- What the fit was measured on and how it came out. Section 6.1 fails CI when `ece`
  -- exceeds 0.05 for any decision type with at least 500 samples, so all three are columns
  -- rather than a report somebody has to keep beside the table.
  samples             INTEGER NOT NULL DEFAULT 0 CHECK (samples >= 0),
  ece                 REAL    NOT NULL DEFAULT 0.0 CHECK (ece >= 0.0),
  brier               REAL    NOT NULL DEFAULT 0.0 CHECK (brier >= 0.0),
  fitted_at           TEXT,
  created_at          TEXT    NOT NULL,

  PRIMARY KEY (kind, source),

  -- The identity map is fitted on nothing and claims nothing.
  CHECK (version > 0 OR (method = 'identity' AND samples = 0)),
  -- ...and a fitted map has knots.
  CHECK (version = 0 OR length(knots) > 2)
) STRICT;

-- ---------------------------------------------------------------------------
-- What one wedding's ledger holds.
--
-- The ninth coverage view. Its denominator is *decisions* rather than photographs, and the
-- distinction is the one thing a reader of this view has to hold on to: a wedding of three
-- thousand photographs with four hundred decisions in it is not 13 % explained. It is a
-- wedding where four hundred decisions were made, and `explained / decisions` - which
-- section 10.1 gates at 100 % - is the only ratio in here that is a claim about coverage.
-- ---------------------------------------------------------------------------
CREATE VIEW v_ledger_coverage AS
SELECT d.project_id,
       COUNT(*)                                                      AS decisions,
       SUM(CASE WHEN d.reason_count > 0 THEN 1 ELSE 0 END)           AS explained,
       SUM(CASE WHEN d.calibration_ver > 0 THEN 1 ELSE 0 END)        AS calibrated,
       SUM(CASE WHEN d.autonomy IN ('suggest','require_review')
                THEN 1 ELSE 0 END)                                   AS review_queue,
       SUM(CASE WHEN d.source = 'user' THEN 1 ELSE 0 END)            AS user_decisions,
       SUM(d.reason_count)                                           AS reasons,
       MAX(d.calibration_ver)                                        AS calibration_ver,
       MIN(d.created_ts)                                             AS first_ts,
       MAX(d.created_ts)                                             AS last_ts
FROM decisions d
GROUP BY d.project_id;
