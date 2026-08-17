-- Migration 12 - which photographs are delivered, which are not, and why.
--
-- PHASE-12 section 4 names this file `0012_selection.sql` and the three tables it must
-- contain: `selection`, `rejections` and `coverage_report`. All three are here. Section 5
-- freezes `SelectionResult`; every column below either comes from that shape or is listed
-- in `docs/adr/ADR-0025-culling-coverage-and-gallery-sizing.md`.
--
-- ---------------------------------------------------------------------------
-- WHAT THIS MIGRATION IS FOR
-- ---------------------------------------------------------------------------
--
-- Eleven phases have written evidence into this catalog. This is the first one that
-- writes a *decision*, and the difference shows up in what the schema has to guarantee.
-- Migrations 6 to 11 all had to guarantee that a missing row could never be read as good
-- news. This one has to guarantee something harder: that a gallery can be rebuilt from
-- scratch on any machine and come out identical, that a photographer's own keep or
-- removal outlives every rebuild, and that the wedding's story survives an engine that is
-- allowed to remove five sixths of it.
--
-- Seven properties are enforced here rather than remembered:
--
-- 1. **Nothing in this schema deletes a photograph.** There is no `deleted`, no `trash`,
--    no path column and no file operation anywhere in phase 12. A rejection is a row in
--    `rejections` with reasons and a pointer to what was kept instead. Invariant 1 says
--    never mutate a RAW; this phase is where the temptation to "just move the rejects"
--    first appears, and the schema has nowhere to put the answer.
--
-- 2. **Every decision carries its reasons, in both directions.** Invariant 2 as a CHECK
--    the database refuses to break, on `selection` *and* on `rejections`. Section 10.1:
--    "every rejected frame has at least one human-readable reason".
--
-- 3. **An override lives in its own table.** `cull_override` is not a column on
--    `selection`, because a re-selection rebuilds `selection` and `rejections` wholesale
--    and a frame can move between them. An override stored on the row it applies to would
--    be destroyed by exactly the operation it is supposed to survive. This is the sixth
--    phase to make a photographer's decision unbeatable and the first where the obvious
--    place to put it is the wrong one.
--
-- 4. **A rejection points at what won.** `kept_instead` and `runner_up` are the two halves
--    of section 2.1's "runner-up pointers for every kept frame" and the answer to "why is
--    this frame not in my gallery" that is not a number. Phase 27 consumes `runner_up`
--    directly.
--
-- 5. **`Missing` is a stored state, not an absent row.** `coverage_report` has a row for
--    every one of the twelve `MustHave` rules on every culled project, including the ones
--    that found nothing. A guarantee that failed silently by not writing a row is the
--    exact failure section 12 says loses the customer forever.
--
-- 6. **The determinism hash covers the config, not just the inputs.** `cull_run` stores it
--    beside `config_digest` so a support case can tell "two machines disagreed" from "two
--    machines were asked different questions". Section 6.4. `AURA-ML-5048`.
--
-- 7. **Three version columns, and they invalidate three different things.** `model_ver`
--    is the *lowest* head version under the sub-scores, `analysis_ver` the passes,
--    `calibration_ver` the per-scene mapping. Eighth phase, eighth version-drift code.
--
-- ROLLBACK. Every object here is new and nothing outside this file references it, so the
-- down migration is a list of drops, kept as a comment because the runner is forward-only
-- by design:
--
--   DROP VIEW  IF EXISTS v_cull_coverage;
--   DROP VIEW  IF EXISTS v_cull_chapters;
--   DROP TABLE IF EXISTS coverage_report;
--   DROP TABLE IF EXISTS rejections;
--   DROP TABLE IF EXISTS selection;
--   DROP TABLE IF EXISTS cull_override;
--   DROP TABLE IF EXISTS cull_run;
--   DELETE FROM schema_version WHERE version = 12;
--
-- Running those returns the catalog to schema 11. Everything it costs is recomputable
-- from the sub-scores except one thing - `cull_override`, which is the photographer's own
-- work - so the rollback runbook says to export that table first. It is deliberately the
-- smallest and most portable table in the migration for that reason: two ids, one verb
-- and a timestamp.
--
-- STORAGE. Section 11 sets no per-image byte budget for this phase, and the reason is
-- worth writing down rather than treating as an omission: a selection is one small row per
-- photograph and its size is dominated by the reason JSON, which the same three decisions
-- as migration 11 keep small - a reason stores its **code** and stores `text` only when
-- the sentence differs from `CullCode::user_text`, the four sub-scores are columns rather
-- than a JSON blob, and there is one index per table and it is the one the panel seeks on.
-- `crates/aura-perf/tests/cull_budgets.rs` asserts the measured figure so that the absence
-- of a stated budget never becomes the absence of a measurement.

-- ---------------------------------------------------------------------------
-- One row per project that has been culled. The run-level facts.
--
-- Separate from the per-photograph tables because the mode, the target and the hash are
-- properties of the *decision*, not of any frame in it - and because the size slider
-- rewrites `selection` and `rejections` while changing exactly two columns here.
-- ---------------------------------------------------------------------------
CREATE TABLE cull_run (
  project_id          TEXT    NOT NULL PRIMARY KEY REFERENCES project(project_id) ON DELETE CASCADE,

  -- 'conservative', 'balanced' or 'aggressive'. The slug, for the reason every migration
  -- since 6 gives: an integer whose meaning lives in a Rust enum is a column a support
  -- engineer cannot read.
  mode                TEXT    NOT NULL DEFAULT 'balanced'
                      CHECK (mode IN ('conservative','balanced','aggressive')),
  -- How many frames were aimed for, and how many came out. They differ whenever the
  -- coverage guard forced frames in, and the difference is information the panel shows.
  target_count        INTEGER NOT NULL DEFAULT 0 CHECK (target_count >= 0),
  actual_count        INTEGER NOT NULL DEFAULT 0 CHECK (actual_count >= 0),

  -- Photographs in the project, and photographs that carried a phase 09 verdict. The two
  -- denominators of `CullOutline`. `eligible` is the one that matters: a gallery culled
  -- from 60 % of a wedding is a gallery of 60 % of a wedding, and nothing in the grid
  -- shows that.
  photos              INTEGER NOT NULL DEFAULT 0 CHECK (photos >= 0),
  eligible            INTEGER NOT NULL DEFAULT 0 CHECK (eligible >= 0),
  -- How many eligible frames also carried an emotion reading, a composition judgement and
  -- a moment. How much of the fusion was real, per signal.
  emotion_aware       INTEGER NOT NULL DEFAULT 0 CHECK (emotion_aware >= 0),
  composition_aware   INTEGER NOT NULL DEFAULT 0 CHECK (composition_aware >= 0),
  grouped             INTEGER NOT NULL DEFAULT 0 CHECK (grouped >= 0),

  -- Section 6.4's hash over inputs + config. Stored as the signed reinterpretation of the
  -- u64, because SQLite has no unsigned integer and a hash written as TEXT would be eight
  -- bytes of hex per project for no gain. The Rust side casts both ways and a round-trip
  -- test asserts it.
  deterministic_hash  INTEGER NOT NULL DEFAULT 0,
  -- BLAKE3 of `cull_weights.toml` + `coverage_rules.toml`, hex. The fourth thing that
  -- invalidates a selection and the only one that is not a version number: a release can
  -- change a threshold in TOML without touching a line of Rust, and a stored gallery that
  -- cannot tell it happened is a stored gallery nobody can reproduce.
  config_digest       TEXT    NOT NULL DEFAULT '',

  model_ver           INTEGER NOT NULL DEFAULT 0,
  analysis_ver        INTEGER NOT NULL DEFAULT 0,
  calibration_ver     INTEGER NOT NULL DEFAULT 0,
  -- Wall clock for the selection passes alone, not for the analysis underneath them.
  -- Section 11 budgets 1.5 s over 4,000 analysed frames and 2 s for a slider move.
  elapsed_ms          INTEGER NOT NULL DEFAULT 0 CHECK (elapsed_ms >= 0),

  -- The three parts of `CoverageReport` that are not per-rule, as JSON.
  --
  -- They are here rather than in `coverage_report` because they are not *about* a rule:
  -- the identity counts are per person, the chapter counts are per chapter, and the
  -- warnings are per run. Putting them in a table keyed by `(project_id, rule)` would
  -- mean inventing a rule for each of them, which is how a schema starts lying about
  -- what its rows mean.
  --
  --   identity_coverage  [[identity_id, frames], ...] - every identity, including zeros,
  --                      because the zero is the number the panel exists to show.
  --   chapter_counts     [[chapter, delivered, target], ...] in ChapterId::ALL order.
  --   warnings           ["...", ...] - the sentences a photographer reads before
  --                      delivering. Free text, because they name specific identities and
  --                      counts and a closed vocabulary would be a template with holes.
  identity_coverage   TEXT    NOT NULL DEFAULT '[]',
  chapter_counts      TEXT    NOT NULL DEFAULT '[]',
  warnings            TEXT    NOT NULL DEFAULT '[]',
  created_at          TEXT    NOT NULL,
  updated_at          TEXT    NOT NULL,

  -- A gallery cannot be larger than the frames it was chosen from.
  CHECK (actual_count <= eligible),
  CHECK (eligible <= photos)
) STRICT;

-- ---------------------------------------------------------------------------
-- The gallery. One row per delivered photograph.
-- ---------------------------------------------------------------------------
CREATE TABLE selection (
  photo_id            TEXT    NOT NULL PRIMARY KEY REFERENCES photo(photo_id) ON DELETE CASCADE,
  project_id          TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  -- NULL when the frame is in no moment. Phase 08 groups *groupable* frames, so a frame
  -- with no embedding has no moment, is still deliverable, and must not be given a
  -- fabricated moment id to satisfy a foreign key. Not a reference for that reason:
  -- moments are rebuilt by their own pass, and a selection that a re-grouping could
  -- cascade-delete would lose a gallery to an unrelated phase.
  moment_id           TEXT,

  -- The fused score and the four things underneath it. Columns rather than a JSON blob,
  -- because the panel sorts on the first and explains with the other four, and because
  -- `weakest()` is the sentence a photographer reads when they disagree.
  keep_score          REAL    NOT NULL DEFAULT 0.0
                      CHECK (keep_score >= 0.0 AND keep_score <= 1.0),
  technical           REAL    NOT NULL DEFAULT 0.0
                      CHECK (technical >= 0.0 AND technical <= 1.0),
  emotion             REAL    NOT NULL DEFAULT 0.0
                      CHECK (emotion >= 0.0 AND emotion <= 1.0),
  composition         REAL    NOT NULL DEFAULT 0.0
                      CHECK (composition >= 0.0 AND composition <= 1.0),
  prominence          REAL    NOT NULL DEFAULT 0.0
                      CHECK (prominence >= 0.0 AND prominence <= 1.0),
  -- Which scene's weights fused them. Invariant 7: a stored decision that does not say
  -- which scene it assumed is not reproducible. 'unknown' means neutral weights.
  scene               TEXT    NOT NULL DEFAULT 'unknown',
  -- Which chapter it counts toward, as phase 07's slug. Stored rather than joined because
  -- the quota panel groups by it on every render and the segment a frame belongs to can
  -- move under an unrelated re-analysis.
  chapter             TEXT    NOT NULL DEFAULT 'other',
  -- Timeline milliseconds. The gallery is ordered by this rather than by score, because a
  -- gallery is a story; phase 29 does highlight reels.
  timeline_ts         INTEGER NOT NULL DEFAULT 0,

  confidence          REAL    NOT NULL DEFAULT 0.0
                      CHECK (confidence >= 0.0 AND confidence <= 1.0),
  -- JSON array of at most four objects: {code, weight, text?}. `text` is present only
  -- when the sentence differs from `CullCode::user_text` - which is the case for the
  -- reasons that name a rule, a rival frame or an identity. Invariant 2.
  reasons             TEXT    NOT NULL DEFAULT '[]',
  -- The best alternative in the same moment. Phase 27 reads this column directly. NULL
  -- when the moment held one frame, which is honest and is not the same as "nothing was
  -- as good".
  runner_up           TEXT,
  -- The must-have slug protecting this frame, or NULL. A column rather than a search
  -- through `reasons`, because the sizing pass reads it on every frame it considers
  -- dropping and section 6.4 requires that shrinking never breaks a must-have.
  coverage_role       TEXT,

  created_at          TEXT    NOT NULL,

  -- Invariant 2, as something the database refuses to break. Two characters is an empty
  -- JSON array.
  CHECK (length(reasons) > 2),
  -- A frame cannot be its own alternative.
  CHECK (runner_up IS NULL OR runner_up <> photo_id)
) STRICT;

-- The gallery, in order, for one project. The only index on this table: the panel reads a
-- project's keepers in timeline order and nothing else does a range scan here.
CREATE INDEX idx_selection_project ON selection(project_id, timeline_ts);

-- ---------------------------------------------------------------------------
-- Everything that is not in the gallery, and what it lost to.
--
-- A separate table from `selection` rather than a `kept` boolean on one table, and the
-- reason is a query rather than a preference: every read in phases 14, 27, 29 and 30 asks
-- for the *gallery*, on every project open, and a table that also holds four times as many
-- rejections makes that read four times as wide for no caller's benefit. The rejection
-- panel is the only surface that reads this table and it reads one project at a time.
-- ---------------------------------------------------------------------------
CREATE TABLE rejections (
  photo_id            TEXT    NOT NULL PRIMARY KEY REFERENCES photo(photo_id) ON DELETE CASCADE,
  project_id          TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  moment_id           TEXT,

  -- Zero when a veto fired: a veto did not lower a score, it replaced one, and storing the
  -- score a vetoed frame would have had would invite somebody to compare it with a keeper.
  keep_score          REAL    NOT NULL DEFAULT 0.0
                      CHECK (keep_score >= 0.0 AND keep_score <= 1.0),
  scene               TEXT    NOT NULL DEFAULT 'unknown',
  chapter             TEXT    NOT NULL DEFAULT 'other',
  timeline_ts         INTEGER NOT NULL DEFAULT 0,
  -- JSON array of at most four objects: {code, weight, text?}.
  reasons             TEXT    NOT NULL DEFAULT '[]',
  -- The frame that is in the gallery instead. NULL for a veto - a frame nobody would have
  -- delivered lost to nothing - and for a size trim that dropped several frames together.
  kept_instead        TEXT,
  -- 1 when this frame was its moment's peak and lost anyway. Section 6.2 forbids dropping
  -- a peak silently, so the fact is a column the gate can count rather than a string the
  -- gate would have to parse.
  was_peak            INTEGER NOT NULL DEFAULT 0 CHECK (was_peak IN (0,1)),
  created_at          TEXT    NOT NULL,

  CHECK (length(reasons) > 2),
  CHECK (kept_instead IS NULL OR kept_instead <> photo_id)
) STRICT;

CREATE INDEX idx_rejections_project ON rejections(project_id, timeline_ts);

-- ---------------------------------------------------------------------------
-- The coverage guarantee, as twelve rows per culled project.
--
-- Twelve rows and not "a row when something went wrong", because the panel's job is to
-- show every guarantee and its state - and because a guarantee that fails by writing no
-- row is a guarantee that fails invisibly.
-- ---------------------------------------------------------------------------
CREATE TABLE coverage_report (
  project_id          TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  -- One of the twelve `MustHave` slugs. Not a CHECK list, deliberately: the vocabulary is
  -- frozen in `aura-core` and enumerated by `MustHave::ALL`, and a second copy of it in
  -- SQL is a second place to forget when a thirteenth rule is argued for. The loader
  -- refuses an unknown slug before a row is ever written.
  rule                TEXT    NOT NULL,
  -- 'covered', 'covered_weak' or 'missing'.
  state               TEXT    NOT NULL DEFAULT 'missing'
                      CHECK (state IN ('covered','covered_weak','missing')),
  -- How many frames the rule ended up with, and how many it asked for.
  found               INTEGER NOT NULL DEFAULT 0 CHECK (found >= 0),
  required            INTEGER NOT NULL DEFAULT 0 CHECK (required >= 0),
  -- How many candidates existed at all. The number that separates "nobody shot it" from
  -- "the engine chose not to", which is the distinction this whole table exists for.
  candidates          INTEGER NOT NULL DEFAULT 0 CHECK (candidates >= 0),
  -- The sentence the panel shows, when there is one. Free text on purpose: it names
  -- specific identities and counts, and a closed vocabulary that could express "Priya
  -- appears once and close family should appear three times" would be a template with
  -- four holes in it.
  warning             TEXT,
  created_at          TEXT    NOT NULL,

  PRIMARY KEY (project_id, rule),
  -- A rule with no candidates cannot be covered. The one CHECK here that is about
  -- arithmetic rather than range, and it is the schema's copy of the promise in section
  -- 6.3: the product cannot invent coverage.
  CHECK (candidates > 0 OR state = 'missing'),
  -- ...and a rule that found nothing is missing, whatever else is true.
  CHECK (found > 0 OR state = 'missing')
) STRICT;

-- ---------------------------------------------------------------------------
-- The photographer's own decisions. Unbeatable, and deliberately not a column.
--
-- Sixth phase, sixth unbeatable decision - after `identities.user_locked`,
-- `segments.user_locked`, `moments.user_locked`, `image_integrity.user_reviewed` and
-- `image_composition.user_reviewed`. The first five are columns on the row they qualify,
-- because a re-analysis *updates* those rows and the check goes inside the update.
--
-- This one cannot be. A re-selection rebuilds `selection` and `rejections` wholesale, and
-- a frame moves between the two tables. An override stored on the row it applies to would
-- be destroyed by exactly the operation it exists to survive, so it lives here and is
-- **re-applied** onto every fresh selection rather than subtracted from the input. Phase
-- 09 wrote the argument for re-applying rather than excluding: the frame is still
-- re-measured, and the disagreement is carried onto the new measurement.
-- ---------------------------------------------------------------------------
CREATE TABLE cull_override (
  photo_id            TEXT    NOT NULL PRIMARY KEY REFERENCES photo(photo_id) ON DELETE CASCADE,
  project_id          TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  -- 'keep' or 'reject'. Two values and not a boolean, because "I have no opinion" is the
  -- absence of a row and a tri-state boolean is how that gets conflated with "reject".
  action              TEXT    NOT NULL CHECK (action IN ('keep','reject')),
  created_at          TEXT    NOT NULL
) STRICT;

CREATE INDEX idx_cull_override_project ON cull_override(project_id, action);

-- ---------------------------------------------------------------------------
-- How much of a project the gallery was chosen from.
--
-- The eighth coverage view, after embeddings, people, scenes, moments, integrity, emotion
-- and composition. The denominator here is **every photograph**, and the second number -
-- `eligible` - is the one that matters, because a frame with no phase 09 verdict was not
-- rejected, it was never considered. `AURA-ML-5050` exists so that difference is never
-- read as a decision.
-- ---------------------------------------------------------------------------
CREATE VIEW v_cull_coverage AS
SELECT p.project_id,
       COUNT(DISTINCT ph.photo_id)                                AS photos,
       COUNT(DISTINCT s.photo_id)                                 AS selected,
       COUNT(DISTINCT r.photo_id)                                 AS rejected,
       COUNT(DISTINCT CASE WHEN o.action = 'keep'   THEN o.photo_id END) AS user_kept,
       COUNT(DISTINCT CASE WHEN o.action = 'reject' THEN o.photo_id END) AS user_rejected,
       MAX(cr.eligible)                                           AS eligible,
       MAX(cr.emotion_aware)                                      AS emotion_aware,
       MAX(cr.composition_aware)                                  AS composition_aware,
       MAX(cr.grouped)                                            AS grouped,
       MAX(cr.target_count)                                       AS target_count,
       MAX(cr.mode)                                               AS mode,
       MAX(cr.deterministic_hash)                                 AS deterministic_hash,
       MAX(cr.model_ver)                                          AS model_ver,
       MAX(cr.analysis_ver)                                       AS analysis_ver,
       MAX(cr.calibration_ver)                                    AS calibration_ver
FROM project p
LEFT JOIN photo ph         ON ph.project_id = p.project_id
LEFT JOIN selection s      ON s.photo_id    = ph.photo_id
LEFT JOIN rejections r     ON r.photo_id    = ph.photo_id
LEFT JOIN cull_override o  ON o.photo_id    = ph.photo_id
LEFT JOIN cull_run cr      ON cr.project_id = p.project_id
WHERE p.deleted_at IS NULL
GROUP BY p.project_id;

-- ---------------------------------------------------------------------------
-- Delivered frames per chapter, for the quota panel.
--
-- One scan rather than nine queries: the panel draws all nine chapters on every render of
-- the cull view, and section 11 budgets two seconds for a slider move that redraws it.
-- ---------------------------------------------------------------------------
CREATE VIEW v_cull_chapters AS
SELECT project_id,
       chapter,
       COUNT(*)              AS delivered,
       AVG(keep_score)       AS mean_score,
       MIN(timeline_ts)      AS first_ts,
       MAX(timeline_ts)      AS last_ts
FROM selection
GROUP BY project_id, chapter;
