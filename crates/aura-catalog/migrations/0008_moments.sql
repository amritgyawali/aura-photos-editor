-- Migration 8 - what the photographer shot once, and the frames that repeat it.
--
-- PHASE-08 section 4 names this file `0008_moments.sql` and the three tables it must
-- contain: `moments`, `moment_images` and `duplicates`. All three are here. Section 5
-- freezes the shapes of `Moment` and `DuplicateSet`; every column below either comes
-- from one of those or is listed in
-- `docs/adr/ADR-0017-burst-grouping-and-duplicate-policy.md`.
--
-- ---------------------------------------------------------------------------
-- WHAT THIS MIGRATION IS FOR
-- ---------------------------------------------------------------------------
--
-- Every phase from 09 to 29 operates on moments rather than on files. Phase 12's
-- coverage guarantee is written against them, because rejecting a burst is a moment
-- lost and rejecting individual frames is tidying. This is the table that makes the
-- distinction a join instead of an argument.
--
-- Four properties are enforced here rather than remembered:
--
-- 1. **`user_locked` is unbeatable.** Section 13: "manual grouping edits are
--    permanent and undoable". Permanent means the re-grouping pass cannot touch
--    them, and the check is *inside* the delete that a re-grouping starts with -
--    not read first and branched on, which leaves a window. This is migration 6's
--    `identities.user_locked` and migration 7's `segments.user_locked`, a third time.
--
-- 2. **Three version columns, because they invalidate three different things.**
--    `embed_ver` invalidates every distance and therefore every edge; `group_ver`
--    invalidates the graph construction and the union-find; `profile_ver`
--    invalidates the thresholds those edges were compared against. Two groupings
--    made under different values of any of them are not comparable, and
--    `AURA-ML-5028` exists so that comparison never happens silently.
--
-- 3. **A moment carries its reasons.** Invariant 2, as a CHECK the database refuses
--    to break, copied from `segments.reasons` and `identities.role_reasons`.
--
-- 4. **Nothing here can reject a photograph.** There is no `culled` column, no
--    `rejected` flag and no rank. `duplicates.keep_hint` is spelled *hint* and is
--    documented as provisional in three places. Section 2.2 puts the decision in
--    phase 12 and this schema keeps that structural.
--
-- ROLLBACK. Every object here is new and nothing outside this file references it, so
-- the down migration is a list of drops, kept as a comment because the runner is
-- forward-only by design:
--
--   DROP VIEW  IF EXISTS v_moment_summary;
--   DROP VIEW  IF EXISTS v_moment_coverage;
--   DROP TABLE IF EXISTS moment_edits;
--   DROP TABLE IF EXISTS duplicates;
--   DROP TABLE IF EXISTS moment_images;
--   DROP TABLE IF EXISTS moments;
--   DELETE FROM schema_version WHERE version = 8;
--
-- Running those returns the catalog to schema 7. It costs the photographer's manual
-- splits and merges, which is the one thing here that cannot be recomputed - so the
-- rollback runbook says to export `moment_edits` first, and the table exists partly
-- so that export is one statement.
--
-- STORAGE. Section 11 budgets **200 bytes** of extra storage per image, which is half
-- of what migration 7 was given, and the schema below is shaped by that number rather
-- than merely measured against it. Two decisions come directly from it:
--
--   * `moment_images` has **no `project_id` column**. Migration 6 and migration 7 both
--     carry a redundant `project_id` and both justify it as "a `WHERE project_id = ?`
--     that cannot be forgotten is worth one text column". At 40 characters that column
--     is a fifth of this phase's entire per-image budget, and unlike a face or a scene
--     row, every read of this table already joins `moments` - which has the column.
--     So the redundancy is dropped here, deliberately and against precedent.
--   * `bursts` are a **column on the membership row**, not a table. A burst has no
--     identity a photographer can refer to, no lock and no lifetime of its own, so an
--     id for it would cost 40 bytes per frame to name something nothing looks up.
--
-- The measured figure is asserted by `crates/aura-perf/tests/moment_budgets.rs`
-- against a real catalog, not against this comment - which is how migration 7's claim
-- of 330 bytes was found to be 410.

-- ---------------------------------------------------------------------------
-- One row per moment. Section 5's frozen `Moment`.
--
-- `segment_id` is nullable, and that is load-bearing rather than lax: grouping runs
-- on frames that have embeddings, and segmentation runs on frames that have scene
-- labels. A wedding that has been embedded but not classified must still group - the
-- cadence, the hashes and the timestamps are all present - and a NOT NULL here would
-- make phase 08 depend on phase 07 having succeeded rather than on phase 07 having
-- run. See ADR-0017 section 5.
-- ---------------------------------------------------------------------------
CREATE TABLE moments (
  id             TEXT    NOT NULL PRIMARY KEY,
  project_id     TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  -- The chapter, when the wedding has been segmented. NULL is "not yet placed",
  -- never "does not belong anywhere".
  segment_id     TEXT    REFERENCES segments(id) ON DELETE SET NULL,
  -- Milliseconds since the Unix epoch, the same representation `segments` uses. A
  -- moment's start and a chapter's start are compared directly by phase 12.
  start_ts       INTEGER NOT NULL,
  end_ts         INTEGER NOT NULL,
  frame_count    INTEGER NOT NULL DEFAULT 0 CHECK (frame_count >= 0),
  burst_count    INTEGER NOT NULL DEFAULT 0 CHECK (burst_count >= 0),
  -- Distinct camera bodies, denormalised so the grid's stacked cell can show the
  -- two-shooter badge without a second query. The bodies themselves are derivable
  -- from `moment_images` joined to `photo`; this is the count, not the list.
  camera_count   INTEGER NOT NULL DEFAULT 1 CHECK (camera_count >= 0),
  -- Mean pairwise embedding distance inside the moment, normalised to 0..1.
  -- Section 6.4: low means a bracketed static subject, high means the action moved.
  diversity      REAL    NOT NULL DEFAULT 0.0 CHECK (diversity >= 0.0 AND diversity <= 1.0),
  confidence     REAL    NOT NULL DEFAULT 0.0 CHECK (confidence >= 0.0 AND confidence <= 1.0),
  -- JSON array of at most six short evidence strings. Invariant 2.
  reasons        TEXT    NOT NULL DEFAULT '[]',
  -- 1 when the photographer split, merged or pinned this moment. Checked inside the
  -- statement that would overwrite it.
  user_locked    INTEGER NOT NULL DEFAULT 0 CHECK (user_locked IN (0,1)),
  -- Which embedding produced the distances underneath. A bump invalidates every edge.
  embed_ver      INTEGER NOT NULL DEFAULT 0,
  -- Which grouping implementation built this moment.
  group_ver      INTEGER NOT NULL DEFAULT 1,
  -- Which threshold table it read.
  profile_ver    INTEGER NOT NULL DEFAULT 1,
  created_at     TEXT    NOT NULL,
  updated_at     TEXT    NOT NULL,
  CHECK (end_ts >= start_ts),
  -- A moment with a confidence has reasons behind it. Two characters is an empty
  -- JSON array, so `length > 2` is "the array has something in it".
  CHECK (confidence <= 0.0 OR length(reasons) > 2)
) STRICT;

CREATE INDEX idx_moments_project ON moments(project_id, start_ts);
CREATE INDEX idx_moments_segment ON moments(segment_id);
-- The re-grouping pass reads exactly this: which moments in this project may be
-- deleted and rebuilt, and which the photographer has taken ownership of.
CREATE INDEX idx_moments_locked  ON moments(project_id, user_locked);

-- ---------------------------------------------------------------------------
-- Which photographs are in which moment, and which burst inside it.
--
-- `burst_ix` is the two-tier structure from section 2.1. Frames sharing a moment and
-- a `burst_ix` are one press-and-hold of the shutter; frames sharing a moment and
-- differing in `burst_ix` are the same subject and action shot in separate takes.
-- Zero-based, dense within a moment, and every frame has one - a frame with no
-- neighbours close enough to be a burst is a burst of one, so that a caller
-- iterating bursts can never silently drop a frame.
--
-- No `project_id` column, unlike `segment_images` and `faces`. See this file's
-- storage note: at 40 characters it would be a fifth of section 11's whole per-image
-- budget, and every read of this table already joins `moments`.
-- ---------------------------------------------------------------------------
CREATE TABLE moment_images (
  moment_id  TEXT    NOT NULL REFERENCES moments(id) ON DELETE CASCADE,
  photo_id   TEXT    NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,
  -- Position within the moment, in timeline order. Zero-based.
  position   INTEGER NOT NULL DEFAULT 0,
  -- Which burst inside the moment. Zero-based, dense.
  burst_ix   INTEGER NOT NULL DEFAULT 0 CHECK (burst_ix >= 0),
  PRIMARY KEY (moment_id, photo_id)
) STRICT, WITHOUT ROWID;

-- One photograph is in at most one moment, and "which moment is this frame in" is
-- the question the grid asks four thousand times while scrolling.
--
-- The only index on this table, deliberately. `WITHOUT ROWID` already clusters the
-- rows by `moment_id`, so reading one moment's members in burst and position order is
-- a range scan plus a sort of at most `max_group` rows - and a second index would cost
-- another 60 bytes per photograph out of section 11's 200-byte budget to save that
-- sort. Measured, not assumed: `crates/aura-perf/tests/moment_budgets.rs`.
CREATE UNIQUE INDEX idx_moment_images_photo ON moment_images(photo_id);

-- ---------------------------------------------------------------------------
-- The duplicate sets inside a moment. Section 5's `DuplicateSet`.
--
-- **Only sets that cap the gallery are stored** - `identical` and `near_identical`.
-- A `variant` set says "all frames stay eligible and Phase 12 chooses", which is a
-- statement that nothing is constrained, about data the catalog already holds: the
-- frames of one burst *are* the alternatives, and `moment_images.burst_ix` is that
-- list. Storing them measured 380 bytes per photograph against section 11's 200-byte
-- budget, every byte of it restating a column. The class still exists - it is what
-- `duplicate::classify` returns and what the review panel shows - it is simply not a
-- row. See ADR-0017 section 7.
--
-- One row per (set, frame) rather than one row per set with a JSON array of ids,
-- because the question "may this frame reach a gallery" is asked per frame by phase
-- 12 and would otherwise mean parsing every set in the wedding. `set_ix` groups the
-- rows back into sets.
--
-- `kind` is stored as the slug rather than an integer for the reason migration 6
-- gives for role text and migration 7 for scene text: an integer whose meaning lives
-- in a Rust enum is a column a support engineer cannot read and a newer build can
-- silently reinterpret. Here the stakes are higher than usual - reading
-- `near_identical` as `variant` shows a client two copies of one photograph, and
-- reading it the other way round loses a frame.
-- ---------------------------------------------------------------------------
CREATE TABLE duplicates (
  moment_id   TEXT    NOT NULL REFERENCES moments(id) ON DELETE CASCADE,
  -- Which set inside the moment. Zero-based, dense.
  set_ix      INTEGER NOT NULL CHECK (set_ix >= 0),
  photo_id    TEXT    NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,
  -- 'identical', 'near_identical' or 'variant'. Repeated on every row of a set so
  -- that the per-frame question needs no join back to a set header.
  kind        TEXT    NOT NULL DEFAULT 'variant'
              CHECK (kind IN ('identical','near_identical','variant')),
  -- 1 on exactly one row per set: the technically strongest frame. NOT a decision -
  -- section 6.3 and section 2.2 both put the decision in phase 12.
  keep_hint   INTEGER NOT NULL DEFAULT 0 CHECK (keep_hint IN (0,1)),
  -- 1 when the photographer pressed "keep this one" in the review panel.
  user_chosen INTEGER NOT NULL DEFAULT 0 CHECK (user_chosen IN (0,1)),
  confidence  REAL    NOT NULL DEFAULT 0.0 CHECK (confidence >= 0.0 AND confidence <= 1.0),
  -- JSON array of at most four short evidence strings, repeated per set on its hint
  -- row and empty elsewhere. Invariant 2: "these two frames are the same photograph"
  -- is the most consequential claim this phase makes and it has to say why.
  reasons     TEXT    NOT NULL DEFAULT '[]',
  -- Hamming distance between the two difference hashes, 0..64, against the set's
  -- hint. Stored rather than recomputed because the review panel shows it and
  -- because a support engineer arguing with a classification needs the number that
  -- produced it, not the number today's code would produce.
  dhash_dist  INTEGER NOT NULL DEFAULT 0 CHECK (dhash_dist >= 0 AND dhash_dist <= 64),
  -- Cosine distance to the hint, 0..2.
  embed_dist  REAL    NOT NULL DEFAULT 0.0,
  PRIMARY KEY (moment_id, set_ix, photo_id)
) STRICT, WITHOUT ROWID;

CREATE INDEX idx_duplicates_photo ON duplicates(photo_id);
-- Phase 12's question: which frames in this wedding may not all reach a gallery.
CREATE INDEX idx_duplicates_kind  ON duplicates(kind, moment_id);

-- ---------------------------------------------------------------------------
-- The undo journal. Section 13: "manual grouping edits are permanent and undoable."
--
-- The same shape as migration 6's `identity_links` and for the same reason: an edit
-- that can be replayed onto a fresh grouping survives a re-analysis, and one that can
-- be reversed is an edit a photographer can risk making.
--
-- `undone_at` rather than a delete, so that an undo is itself a recorded fact. A
-- journal that forgets what was undone cannot answer "why did this moment come back".
-- ---------------------------------------------------------------------------
CREATE TABLE moment_edits (
  edit_id     TEXT    NOT NULL PRIMARY KEY,
  project_id  TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  -- 'split', 'merge', 'lock', 'unlock' or 'keep_hint'.
  action      TEXT    NOT NULL
              CHECK (action IN ('split','merge','lock','unlock','keep_hint')),
  moment_id   TEXT    NOT NULL,
  -- The other moment, for a merge or the half a split created.
  other_id    TEXT,
  -- The photograph, for a split or a keep-hint change.
  photo_id    TEXT,
  -- How many frames the moment held when the edit was made. For telemetry, which
  -- never carries photo ids: `moments.user_edit {action, moment_size}` is section
  -- 11's event and a size is not a shoot.
  moment_size INTEGER NOT NULL DEFAULT 0 CHECK (moment_size >= 0),
  created_at  TEXT    NOT NULL,
  -- NULL until the edit is undone. An undone edit is not replayed onto a fresh
  -- grouping and is not deleted from the journal.
  undone_at   TEXT
) STRICT;

CREATE INDEX idx_moment_edits_project ON moment_edits(project_id, created_at);
CREATE INDEX idx_moment_edits_live    ON moment_edits(project_id, undone_at, created_at);

-- ---------------------------------------------------------------------------
-- How much of a project has been grouped.
--
-- The same argument as `v_embedding_coverage`, `v_people_coverage` and
-- `v_scene_coverage`, for the fourth time: a moment list drawn over a 40 %-embedded
-- wedding is a list of 40 % of a wedding, and the grid has to be able to say so.
--
-- `groupable` is the honest denominator. A frame with no embedding cannot be grouped
-- by any amount of trying, so reporting coverage against every photograph in the
-- project would report a phase 05 gap as a phase 08 failure.
-- ---------------------------------------------------------------------------
CREATE VIEW v_moment_coverage AS
SELECT p.project_id,
       COUNT(DISTINCT ph.photo_id)                            AS photos,
       COUNT(DISTINCT e.photo_id)                             AS groupable,
       COUNT(DISTINCT mi.photo_id)                            AS grouped,
       COUNT(DISTINCT m.id)                                   AS moments,
       SUM(CASE WHEN m.user_locked = 1 THEN 1 ELSE 0 END)     AS locked
FROM project p
LEFT JOIN photo ph          ON ph.project_id = p.project_id
LEFT JOIN embeddings e      ON e.photo_id = ph.photo_id
LEFT JOIN moment_images mi  ON mi.photo_id = ph.photo_id
LEFT JOIN moments m         ON m.id = mi.moment_id
WHERE p.deleted_at IS NULL
GROUP BY p.project_id;

-- ---------------------------------------------------------------------------
-- One row per moment, with the counts the stacked grid cell opens with.
--
-- Section 11 budgets 60 ms for an expand or collapse. A view that aggregates once is
-- what makes the moments view a read of a bounded list rather than a fan-out of one
-- query per stack.
-- ---------------------------------------------------------------------------
CREATE VIEW v_moment_summary AS
SELECT m.id,
       m.project_id,
       m.segment_id,
       m.start_ts,
       m.end_ts,
       (m.end_ts - m.start_ts) / 1000                         AS duration_s,
       m.frame_count,
       m.burst_count,
       m.camera_count,
       m.diversity,
       m.confidence,
       m.reasons,
       m.user_locked,
       COUNT(DISTINCT d.set_ix)                               AS duplicate_sets,
       SUM(CASE WHEN d.kind = 'near_identical' THEN 1 ELSE 0 END) AS near_identical_frames
FROM moments m
LEFT JOIN duplicates d ON d.moment_id = m.id
GROUP BY m.id;
