-- Migration 10 - what each photograph is worth: expression, interaction, peak, reaction.
--
-- PHASE-10 section 4 names this file `0010_emotion.sql` and the four tables it must
-- contain: `face_expression`, `image_interaction`, `moment_peak` and `reaction_links`.
-- All four are here, with section 4's names unchanged. Section 5 freezes
-- `FaceExpression` and `ImageEmotion`; every column below either comes from one of those
-- two shapes or is listed in
-- `docs/adr/ADR-0021-emotion-taxonomy-and-moment-ranking.md`.
--
-- **`image_interaction` is the per-image row.** Section 4 gives four table names and
-- section 5 gives two data shapes, and the per-image shape has to live in one of the
-- four. It goes here rather than in a fifth table nobody named, so the file is exactly
-- section 4's list. The name is narrower than the row - it carries the emotion score,
-- the peak proximity, the reaction pointer and the reasons as well as the interactions -
-- and ADR-0021 section 2 records the choice.
--
-- ---------------------------------------------------------------------------
-- WHAT THIS MIGRATION IS FOR
-- ---------------------------------------------------------------------------
--
-- Phase 12 culls, phase 13 explains, phase 27 audits and phase 29 curates, and all four
-- of them need to know which frames a client would actually buy. Phase 09 answered "is
-- this frame acceptable"; this is the table that answers "is this frame worth
-- delivering", and the two are deliberately separate numbers that phase 12 combines.
--
-- Six properties are enforced here rather than remembered:
--
-- 1. **Nothing in this schema can reject or deliver a photograph.** No `keep` column, no
--    `selected` flag, no gallery rank. `emotion_score` is a reading and section 2.2 puts
--    every keep-or-reject decision in phase 12. Migrations 8 and 9 both state this; it
--    matters a third time because an *ordering* looks even more like a selection than a
--    score does, and `idx_emotion_rank` below exists to make the browser's sort fast and
--    for no other reason.
--
-- 2. **No psychological claim is storable.** Section 2.2 puts "any claim about a
--    person's inner emotional state" permanently out of scope. There is no `mood`
--    column, no `happy` column and no free-text field a caller could put one in: the
--    eight expression channels are named for what is visible on a face, and `reasons`
--    stores codes from a closed vocabulary rather than sentences. A build that wanted to
--    record "the bride was overwhelmed" would have to add a column and an ADR to do it.
--
-- 3. **`user_chosen` is unbeatable.** A photographer who picks a moment's peak frame has
--    made a statement about the photograph, not about the build that ranked it. The
--    check is *inside* the upsert a re-analysis performs. Migration 6's
--    `identities.user_locked`, migration 7's `segments.user_locked`, migration 8's
--    `moments.user_locked` and migration 9's `image_integrity.user_reviewed`, a fifth
--    time.
--
-- 4. **Three version columns, because they invalidate three different things.**
--    `model_ver` invalidates every expression value and interaction strength;
--    `analysis_ver` invalidates the gaze, the peak curve and the links; `weights_ver`
--    invalidates the score alone. There is deliberately no fourth column for the ranker -
--    its coefficients ship inside `emotion_weights.toml`, so one number invalidates the
--    score. `AURA-ML-5038` exists so a comparison across any of the three never happens
--    silently. Sixth phase, sixth version-drift code.
--
-- 5. **A score carries its reasons, and a reason carries its face.** Invariant 2 as a
--    CHECK the database refuses to break, and section 13's fifth criterion - "the Emotion
--    card explains the score with crops and short editorial reasons" - as an optional
--    rectangle inside each reason's JSON object. Optional rather than required, unlike
--    migration 9's: a reason about a whole-frame interaction genuinely has no one face to
--    point at, and a required crop would make the writer invent one.
--
-- 6. **"Not scored" is not "no feeling".** The absence of a row means nobody looked.
--    There is no `scored` boolean and no zero-means-flat convention, because phase 12
--    reads a low score as evidence and would read a missing one the same way.
--    `AURA-ML-5040` exists to make the difference visible.
--
-- ROLLBACK. Every object here is new and nothing outside this file references it, so the
-- down migration is a list of drops, kept as a comment because the runner is forward-only
-- by design:
--
--   DROP VIEW  IF EXISTS v_emotion_interactions;
--   DROP VIEW  IF EXISTS v_emotion_coverage;
--   DROP TABLE IF EXISTS emotion_preferences;
--   DROP TABLE IF EXISTS reaction_links;
--   DROP TABLE IF EXISTS moment_peak;
--   DROP TABLE IF EXISTS face_expression;
--   DROP TABLE IF EXISTS image_interaction;
--   DELETE FROM schema_version WHERE version = 10;
--
-- Running those returns the catalog to schema 9. Everything it costs is recomputable
-- from the pixels except two things - `moment_peak.user_chosen` and every row of
-- `emotion_preferences`, which is a photographer's taste and cannot be recomputed at all -
-- so the rollback runbook says to export both first.
--
-- STORAGE. Section 11 budgets **900 B per image**, which is less than migration 9's
-- kilobyte for a row that has to carry per-face readings as well. Three decisions below
-- are there because of it and each is written where it was made: the eight expression
-- channels are per-mille integers rather than REALs, `face_expression` carries no
-- identity and no timestamp, and `reasons` stores codes rather than sentences. The
-- measured figure is asserted by `crates/aura-perf/tests/emotion_budgets.rs` against a
-- real catalog, not against this comment.

-- ---------------------------------------------------------------------------
-- One row per scored photograph. Section 5's frozen `ImageEmotion`.
--
-- `project_id` is carried, as migration 9 carries it and unlike `moment_images`: every
-- query in phases 12, 27 and 29 is scoped to a project, and the ordering query below
-- would otherwise have to join `photo` to find out which wedding a score belongs to.
-- ---------------------------------------------------------------------------
CREATE TABLE image_interaction (
  photo_id         TEXT    NOT NULL PRIMARY KEY REFERENCES photo(photo_id) ON DELETE CASCADE,
  project_id       TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,

  -- --- Interactions. Section 6.2. ------------------------------------------
  -- JSON array of [slug, strength_milli] pairs, strongest first, only above the head's
  -- detection floor. An array rather than nine columns because the common case is zero
  -- or one entry: nine REALs would cost 72 bytes on every frame in the wedding to store
  -- eight zeroes, against a 900-byte budget.
  --
  -- Slugs rather than indexes, for the reason migrations 6 to 9 all give: a column whose
  -- meaning lives in a Rust enum is a column a support engineer cannot read, and here
  -- reading `kiss` as `toast` would move the peak of the ceremony to the reception.
  interactions     TEXT    NOT NULL DEFAULT '[]',
  -- 1 when two faces in the frame are looking at each other.
  mutual_gaze      INTEGER NOT NULL DEFAULT 0 CHECK (mutual_gaze IN (0,1)),

  -- --- Peak and reaction. Sections 6.2 and 6.3. ----------------------------
  -- Where this frame sits on its moment's action curve. 1.0 at the peak; 0.5 when the
  -- frame is not in a moment or is alone in one - neither the peak nor off it, the same
  -- convention `image_integrity.relative_sharpness` uses.
  peak_proximity   REAL    NOT NULL DEFAULT 0.5
                   CHECK (peak_proximity >= 0.0 AND peak_proximity <= 1.0),
  -- The frame this one reacts to. NULL is the overwhelming majority, which is why it is
  -- a nullable column rather than a row in `reaction_links` being the only record: the
  -- reaction's own view of the link is read on every card and a join for a NULL is a
  -- join for a NULL.
  --
  -- ON DELETE SET NULL rather than CASCADE: deleting the frame somebody reacted to must
  -- not delete the reaction, which is a photograph in its own right.
  reaction_of      TEXT    REFERENCES photo(photo_id) ON DELETE SET NULL,

  -- --- The composite. Section 6.4. -----------------------------------------
  -- The ranker's output, calibrated per scene so that 0.8 means the same thing in a
  -- ceremony and on a dance floor - which is what makes it comparable with
  -- `image_integrity.technical_score` in phase 12.
  emotion_score    REAL    NOT NULL DEFAULT 0.0
                   CHECK (emotion_score >= 0.0 AND emotion_score <= 1.0),
  -- Raised only by a cloud `MomentSignificance` answer. Section 7's offline fallback is
  -- `narrative_weight = 0`, so zero is both "no call was made" and "the call said this
  -- moment is not significant" - and `source` is what distinguishes them.
  narrative_weight REAL    NOT NULL DEFAULT 0.0
                   CHECK (narrative_weight >= 0.0 AND narrative_weight <= 1.0),

  -- Which scene the weights were conditioned on, as the slug `image_scenes` uses.
  -- Invariant 7: a stored score that does not say which scene it assumed cannot be
  -- reproduced. 'unknown' means neutral weights were used.
  scene            TEXT    NOT NULL DEFAULT 'unknown',
  -- JSON array of at most six objects: {c, t?, w, e?:{x,y,w,h}}. Invariant 2.
  reasons          TEXT    NOT NULL DEFAULT '[]',
  confidence       REAL    NOT NULL DEFAULT 0.0
                   CHECK (confidence >= 0.0 AND confidence <= 1.0),
  -- 'local', 'cloud' or 'user'. Phase 07's `Source`, reused rather than redeclared.
  source           TEXT    NOT NULL DEFAULT 'local'
                   CHECK (source IN ('local','cloud','user')),

  model_ver        INTEGER NOT NULL DEFAULT 0,
  analysis_ver     INTEGER NOT NULL DEFAULT 0,
  weights_ver      INTEGER NOT NULL DEFAULT 0,
  created_at       TEXT    NOT NULL,
  updated_at       TEXT    NOT NULL,

  -- A score with a confidence has reasons behind it. Two characters is an empty JSON
  -- array, so `length > 2` is "the array has something in it".
  CHECK (confidence <= 0.0 OR length(reasons) > 2),
  -- A frame cannot react to itself. Cheap to state, and it is the one link error that
  -- would produce an infinite chain in the pair viewer rather than a wrong answer.
  CHECK (reaction_of IS NULL OR reaction_of <> photo_id)
) STRICT;

-- The moment browser's query and the ranked ordering, in one index. **The only index on
-- this table.**
--
-- `DESC` because every reader of it wants the strongest frames first and a scan
-- backwards over an ascending index costs a page walk SQLite need not do. The two
-- candidate indexes that are *not* here were left out for migration 9's reason:
--
--   * `(project_id, scene)` looked like the per-scene calibration's index. Nothing
--     queries by scene at run time - the calibration is fitted offline from an export -
--     so it would have been sixty bytes per photograph for a query that does not exist.
--   * `(reaction_of)` looked like `reactions_to`'s index and is not one either. That
--     query is answered from `reaction_links`, which indexes the action side properly,
--     and this column is only ever read on a row already found by primary key.
CREATE INDEX idx_emotion_rank ON image_interaction(project_id, emotion_score DESC);

-- ---------------------------------------------------------------------------
-- One row per face, per scored frame. Section 5's `FaceExpression`.
--
-- A separate table rather than JSON on `image_interaction`, for migration 9's reason:
-- the question "which frames is the bride laughing in" is asked by identity, and an
-- identity inside a JSON blob is not a query. It is also the row section 10.1's
-- tears/laughter F1 is measured against.
--
-- `face_id` is phase 06's derived id - a hash of the photograph, the box and the
-- detector version - so a re-run of the face pass at the same version produces the same
-- key and this row is replaced rather than duplicated.
--
-- **No biometric data here.** An expression is eight numbers about a rectangle, not a
-- template, and `docs/adr/ADR-0013` puts everything that identifies a person in the
-- sealed store in `aura-people`. This table is deleted by the same erasure path that
-- deletes a face row, by way of the foreign key.
-- ---------------------------------------------------------------------------
CREATE TABLE face_expression (
  -- `faces(id)`, not `faces(face_id)`. Migration 6 names that column `id`, SQLite
  -- accepts a foreign key onto a column that does not exist at CREATE time, and raises
  -- `foreign key mismatch` on the first INSERT instead - which is how the phase 09
  -- storage budget found the same mistake in `face_eye_state`. Written out here so the
  -- seventh table to reference `faces` does not make it an eighth time.
  face_id      TEXT    NOT NULL PRIMARY KEY REFERENCES faces(id) ON DELETE CASCADE,

  -- The eight continuous channels, in `FaceExpression::channels` order, as per-mille
  -- integers.
  --
  -- **Integers rather than REALs, and this is one of the three decisions section 11's
  -- 900-byte budget forced.** A REAL is eight bytes in SQLite whatever it holds; a small
  -- integer is one or two. Eight REALs is 64 bytes per face and 141 per photograph at
  -- this product's 2.2 faces a frame - a sixth of the whole budget - and an expression
  -- reading has nothing like sixteen significant digits in it. Per mille is three, which
  -- is more than the head can justify and enough that no rounding is visible in a
  -- ranking.
  --
  -- The conversion is in one place, `emotion::store::milli` and its inverse, so no call
  -- site divides by a thousand.
  smile        INTEGER NOT NULL DEFAULT 0 CHECK (smile        BETWEEN 0 AND 1000),
  genuineness  INTEGER NOT NULL DEFAULT 0 CHECK (genuineness  BETWEEN 0 AND 1000),
  laughter     INTEGER NOT NULL DEFAULT 0 CHECK (laughter     BETWEEN 0 AND 1000),
  tears        INTEGER NOT NULL DEFAULT 0 CHECK (tears        BETWEEN 0 AND 1000),
  surprise     INTEGER NOT NULL DEFAULT 0 CHECK (surprise     BETWEEN 0 AND 1000),
  tenderness   INTEGER NOT NULL DEFAULT 0 CHECK (tenderness   BETWEEN 0 AND 1000),
  composed     INTEGER NOT NULL DEFAULT 0 CHECK (composed     BETWEEN 0 AND 1000),
  discomfort   INTEGER NOT NULL DEFAULT 0 CHECK (discomfort   BETWEEN 0 AND 1000),

  -- 'unknown', 'camera', 'partner', 'officiant' or 'away'. 'unknown' is the default and
  -- means the face had no usable landmarks - which must not read as 'away', because
  -- 'away' is evidence the reaction linker acts on.
  gaze         TEXT    NOT NULL DEFAULT 'unknown'
               CHECK (gaze IN ('unknown','camera','partner','officiant','away')),
  confidence   INTEGER NOT NULL DEFAULT 0 CHECK (confidence BETWEEN 0 AND 1000),
  model_ver    INTEGER NOT NULL DEFAULT 0

  -- No `identity_id`, no `photo_id`, no `created_at` and no crop rectangle. All four are
  -- already in `faces`: the identity is `faces.identity_id` with `idx_faces_identity` on
  -- it, the photograph is `faces.image_id` with `idx_faces_image` on it, the rectangle is
  -- the face box, and the time is the parent row's `updated_at`. Measured, they were 96
  -- bytes per face and 211 per photograph - nearly a quarter of section 11's budget - to
  -- store a second copy of phase 06's data that would go stale the moment the face pass
  -- re-ran at a new detector version. Migration 9 made exactly this trade for
  -- `face_eye_state` and this file makes it for the same reason.
) STRICT, WITHOUT ROWID;

-- `WITHOUT ROWID` and **no index at all**, both for migration 9's reasons. The primary
-- key *is* the row - a face has exactly one expression - so the separate rowid btree
-- SQLite would otherwise maintain is a second copy of every key for no query. And the
-- query somebody would index for, "which frames is this person laughing in", starts from
-- an identity that `faces` already indexes: `idx_faces_identity` finds the faces and each
-- one is a primary-key lookup here.

-- ---------------------------------------------------------------------------
-- One row per moment. Section 6.2's peak.
--
-- Keyed by the moment rather than by the photograph, because "which frame of this moment
-- is the one" is a question about the moment. A frame's own view - how close it is to its
-- moment's peak - is `image_interaction.peak_proximity` and needs no join.
--
-- ON DELETE CASCADE from `moments`: a regrouping that dissolves a moment dissolves its
-- peak with it, because a peak of a moment that no longer exists is a claim about a
-- grouping nobody made.
-- ---------------------------------------------------------------------------
CREATE TABLE moment_peak (
  moment_id    TEXT    NOT NULL PRIMARY KEY REFERENCES moments(id) ON DELETE CASCADE,
  -- The strongest frame.
  photo_id     TEXT    NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,
  -- Its zero-based position, and how many frames the moment held when this was computed.
  -- The second is stored rather than counted so that a peak whose moment has since been
  -- split is visibly stale rather than quietly wrong.
  peak_index   INTEGER NOT NULL DEFAULT 0 CHECK (peak_index >= 0),
  frames       INTEGER NOT NULL DEFAULT 0 CHECK (frames >= 0),
  -- How clearly the peak wins: (top - runner_up) / top. Below `MomentPeak::MIN_MARGIN`
  -- the kind is 'flat' and `AURA-ML-5042` is logged.
  margin       REAL    NOT NULL DEFAULT 0.0 CHECK (margin >= 0.0 AND margin <= 1.0),
  -- 'expression', 'kiss_apex', 'tear_release', 'bouquet_in_air', 'ring_slide' or 'flat'.
  kind         TEXT    NOT NULL DEFAULT 'expression'
               CHECK (kind IN ('expression','kiss_apex','tear_release','bouquet_in_air',
                               'ring_slide','flat')),
  confidence   REAL    NOT NULL DEFAULT 0.0
               CHECK (confidence >= 0.0 AND confidence <= 1.0),
  reasons      TEXT    NOT NULL DEFAULT '[]',
  -- 1 when the photographer picked this frame. Checked inside the upsert.
  user_chosen  INTEGER NOT NULL DEFAULT 0 CHECK (user_chosen IN (0,1)),
  model_ver    INTEGER NOT NULL DEFAULT 0,
  analysis_ver INTEGER NOT NULL DEFAULT 0,
  weights_ver  INTEGER NOT NULL DEFAULT 0,
  created_at   TEXT    NOT NULL,
  updated_at   TEXT    NOT NULL,

  CHECK (peak_index < frames OR frames = 0)
) STRICT;

-- ---------------------------------------------------------------------------
-- Section 6.3's reaction links: the mother's tears while the couple kisses.
--
-- The pair is the key, so re-running the pass replaces a link rather than adding a
-- second copy of it. Directional: an action has many reactions and a reaction has one
-- action, which is why the reaction side is UNIQUE and the action side is merely
-- indexed.
-- ---------------------------------------------------------------------------
CREATE TABLE reaction_links (
  -- The frame something happened in.
  action_id    TEXT    NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,
  -- The frame somebody reacted in. UNIQUE by way of the primary key's second column
  -- plus the index below: one reaction points at one action, which is what makes
  -- `image_interaction.reaction_of` a single column rather than an array.
  reaction_id  TEXT    NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,
  -- Milliseconds from action to reaction, signed. Negative is real: a second shooter's
  -- clock is not the first's, and phase 01's timeline alignment is good to about a
  -- second. A sign that is thrown away is a diagnosis nobody can make later.
  gap_ms       INTEGER NOT NULL DEFAULT 0
               CHECK (gap_ms >= -4000 AND gap_ms <= 4000),
  bonus        REAL    NOT NULL DEFAULT 0.0 CHECK (bonus >= 0.0 AND bonus <= 1.0),
  confidence   REAL    NOT NULL DEFAULT 0.0
               CHECK (confidence >= 0.0 AND confidence <= 1.0),
  reasons      TEXT    NOT NULL DEFAULT '[]',
  analysis_ver INTEGER NOT NULL DEFAULT 0,
  created_at   TEXT    NOT NULL,

  PRIMARY KEY (reaction_id, action_id),
  CHECK (action_id <> reaction_id)
) STRICT, WITHOUT ROWID;

-- The action's view: "what reacted to this frame". `reactions_to` is the only query on
-- this table that does not start from the primary key.
CREATE INDEX idx_reaction_action ON reaction_links(action_id);

-- ---------------------------------------------------------------------------
-- Section 2.1's preference-learning hook: "which of these two would you deliver?".
--
-- **Collected, not applied.** This build ships the ranker coefficients that
-- `ml/models/emotion/train_ranker.py` produced from the fixture comparisons; phase 30 is
-- where a photographer's own answers start moving them. A ranker that refitted itself
-- while somebody was culling would reorder the grid under their hands, and invariant 4
-- would stop being true the moment it did.
--
-- No `project_id`: both photographs carry one, they are required to be the same one, and
-- the refusal that enforces it is `AURA-ML-5041`.
-- ---------------------------------------------------------------------------
CREATE TABLE emotion_preferences (
  winner_id    TEXT    NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,
  loser_id     TEXT    NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,
  created_at   TEXT    NOT NULL,

  PRIMARY KEY (winner_id, loser_id),
  CHECK (winner_id <> loser_id)
) STRICT, WITHOUT ROWID;

-- ---------------------------------------------------------------------------
-- How much of a project has been scored.
--
-- The sixth coverage view, after embeddings, people, scenes, moments and integrity. The
-- denominator is **every photograph**, as `v_integrity_coverage`'s is and unlike
-- `v_moment_coverage`'s: the interaction head reads the frame, so a photograph with no
-- faces and no embedding still gets a reading, and a frame with no score is this phase's
-- gap.
--
-- `face_aware` is this phase's refinement, exactly parallel to phase 09's
-- `subject_aware`. A wedding at 100 % coverage and 3 % face-aware has been ranked on
-- interactions and scene alone - which is very nearly ranking on nothing, since seven of
-- the nine ranker terms come from faces - and a caller about to trust an ordering has to
-- be able to find that out.
-- ---------------------------------------------------------------------------
CREATE VIEW v_emotion_coverage AS
SELECT p.project_id,
       COUNT(DISTINCT ph.photo_id)                                  AS photos,
       COUNT(DISTINCT ie.photo_id)                                  AS scored,
       COUNT(DISTINCT fe_photo.photo_id)                            AS face_aware,
       AVG(ie.emotion_score)                                        AS mean_score,
       SUM(CASE WHEN ie.source = 'cloud' THEN 1 ELSE 0 END)         AS cloud_scored,
       MIN(ie.model_ver)                                            AS model_ver,
       MIN(ie.analysis_ver)                                         AS analysis_ver,
       MIN(ie.weights_ver)                                          AS weights_ver
FROM project p
LEFT JOIN photo ph              ON ph.project_id = p.project_id
LEFT JOIN image_interaction ie  ON ie.photo_id = ph.photo_id
LEFT JOIN (SELECT DISTINCT f.image_id AS photo_id
             FROM face_expression x
             JOIN faces f ON f.id = x.face_id) fe_photo
                                ON fe_photo.photo_id = ph.photo_id
WHERE p.deleted_at IS NULL
GROUP BY p.project_id;

-- ---------------------------------------------------------------------------
-- The interaction histogram, as one row per project.
--
-- Section 11's `emotion.scored {interaction_histogram}` event and the browser's chip
-- counts. Nine sums rather than nine queries, exactly as `v_integrity_flags` does it.
--
-- `LIKE` on the JSON rather than nine columns on the table: the common frame has zero or
-- one interaction, the slugs are distinct enough that no one is a prefix of another, and
-- the alternative was nine REAL columns costing 72 bytes on every frame in a 4,000-image
-- wedding to store eight zeroes. The pattern is anchored on the opening quote so that a
-- slug can never match inside a strength.
-- ---------------------------------------------------------------------------
CREATE VIEW v_emotion_interactions AS
SELECT project_id,
       COUNT(*)                                                          AS scored,
       SUM(CASE WHEN interactions LIKE '%["hug"%'           THEN 1 ELSE 0 END) AS hug,
       SUM(CASE WHEN interactions LIKE '%["kiss"%'          THEN 1 ELSE 0 END) AS kiss,
       SUM(CASE WHEN interactions LIKE '%["hand_hold"%'     THEN 1 ELSE 0 END) AS hand_hold,
       SUM(CASE WHEN interactions LIKE '%["dance_hold"%'    THEN 1 ELSE 0 END) AS dance_hold,
       SUM(CASE WHEN interactions LIKE '%["ring_exchange"%' THEN 1 ELSE 0 END) AS ring_exchange,
       SUM(CASE WHEN interactions LIKE '%["blessing"%'      THEN 1 ELSE 0 END) AS blessing,
       SUM(CASE WHEN interactions LIKE '%["toast"%'         THEN 1 ELSE 0 END) AS toast,
       SUM(CASE WHEN interactions LIKE '%["tears_wiped"%'   THEN 1 ELSE 0 END) AS tears_wiped,
       SUM(CASE WHEN interactions LIKE '%["group_cheer"%'   THEN 1 ELSE 0 END) AS group_cheer
FROM image_interaction
GROUP BY project_id;
