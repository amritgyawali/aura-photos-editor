-- Migration 9 - what is technically wrong with each photograph, and where.
--
-- PHASE-09 section 4 names this file `0009_integrity.sql` and the two tables it must
-- contain: `image_integrity` and `face_eye_state`. Both are here. Section 5 freezes
-- `IntegrityResult`; every column below either comes from that shape or is listed in
-- `docs/adr/ADR-0019-frame-integrity-and-eye-intent.md`.
--
-- ---------------------------------------------------------------------------
-- WHAT THIS MIGRATION IS FOR
-- ---------------------------------------------------------------------------
--
-- Phase 12 culls, phase 13 explains, phase 22 restores and phase 27 audits - and all
-- four of them need to know whether the *right subject* was sharp, whether the motion
-- was a decision, whether the exposure can be brought back, and whether the eyes that
-- matter were open. This is the table that makes those four questions a join.
--
-- Five properties are enforced here rather than remembered:
--
-- 1. **Nothing in this schema can reject a photograph.** No `culled` column, no
--    `rejected` flag, no rank. `technical_score` is a measurement and section 2.2 puts
--    every keep-or-reject decision in phase 12. This is migration 8's fourth property
--    again, and it matters more here: this is the table whose output looks most like a
--    verdict, and section 12's first failure mode is "false rejections destroy trust".
--
-- 2. **`user_reviewed` is unbeatable.** A photographer who dismisses a flag has made a
--    statement about the photograph, not about the build that analysed it. The check is
--    *inside* the upsert a re-analysis performs - not read first and branched on, which
--    leaves a window. Migration 6's `identities.user_locked`, migration 7's
--    `segments.user_locked` and migration 8's `moments.user_locked`, a fourth time.
--
-- 3. **Three version columns, because they invalidate three different things.**
--    `model_ver` invalidates the learned sharpness and every eye state; `analysis_ver`
--    invalidates the motion kind, the exposure verdict, the noise figure, the flags and
--    the score; `calib_ver` invalidates every *normalised* number. `AURA-ML-5033` exists
--    so a comparison across any of them never happens silently. Fifth phase, fifth
--    version-drift code.
--
-- 4. **A verdict carries its reasons, and a reason carries its pixels.** Invariant 2 as
--    a CHECK the database refuses to break, and section 13's last criterion - "the
--    Integrity card shows the exact crop that caused each penalty" - as a required
--    field inside each reason's JSON object.
--
-- 5. **"Not checked" is not "clean".** The absence of a row means nobody looked. There
--    is no `checked` boolean and no zero-row-means-fine convention, because phase 12
--    reads a clean verdict as evidence and would read a missing one the same way.
--    `AURA-ML-5035` exists to make the difference visible.
--
-- ROLLBACK. Every object here is new and nothing outside this file references it, so
-- the down migration is a list of drops, kept as a comment because the runner is
-- forward-only by design:
--
--   DROP VIEW  IF EXISTS v_integrity_flags;
--   DROP VIEW  IF EXISTS v_integrity_coverage;
--   DROP TABLE IF EXISTS face_eye_state;
--   DROP TABLE IF EXISTS image_integrity;
--   DELETE FROM schema_version WHERE version = 9;
--
-- Running those returns the catalog to schema 8. Everything it costs is recomputable
-- from the pixels except one thing - `user_reviewed` and the dismissals it records -
-- so the rollback runbook says to export those rows first.
--
-- STORAGE. Section 11 budgets **1 KB per image including per-face eye states**, which
-- is five times migration 8's budget and the most generous in the product so far. That
-- is not licence to be careless; it is a recognition that this phase stores a `reasons`
-- array with an evidence rectangle per entry, and that showing a photographer the crop
-- that caused a penalty is the difference between a tool they trust and one they turn
-- off. The measured figure is asserted by `crates/aura-perf/tests/integrity_budgets.rs`
-- against a real catalog, not against this comment.

-- ---------------------------------------------------------------------------
-- One row per analysed photograph. Section 5's frozen `IntegrityResult`.
--
-- `project_id` is carried here, unlike `moment_images`. Migration 8 dropped it to make
-- a 200-byte budget and justified the break with precedent; this table has a 1 KB
-- budget and every query in phase 12 and phase 27 is scoped to a project, so the
-- `WHERE project_id = ?` that cannot be forgotten is worth its forty characters again.
-- ---------------------------------------------------------------------------
CREATE TABLE image_integrity (
  photo_id           TEXT    NOT NULL PRIMARY KEY REFERENCES photo(photo_id) ON DELETE CASCADE,
  project_id         TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,

  -- --- Focus. Section 6.1. -------------------------------------------------
  -- Prominence-weighted across the subject's regions, normalised by the camera
  -- calibration row so that "sharp" means sharp for this body.
  subject_sharpness  REAL    NOT NULL DEFAULT 0.0
                     CHECK (subject_sharpness >= 0.0 AND subject_sharpness <= 1.0),
  bg_sharpness       REAL    NOT NULL DEFAULT 0.0
                     CHECK (bg_sharpness >= 0.0 AND bg_sharpness <= 1.0),
  -- Negative is front focus, positive is back focus, zero is on the subject or not
  -- determinable. The sign convention is section 5's and it is easy to invert by
  -- accident, so `tests/eval/integrity_eval.rs` asserts it on a synthetic ladder.
  focus_offset       REAL    NOT NULL DEFAULT 0.0
                     CHECK (focus_offset >= -1.0 AND focus_offset <= 1.0),
  -- Where this frame's subject sharpness sits inside its moment, 0..1. Section 6.1:
  -- phase 12 mostly needs "which of these six is sharpest". 0.5 means "alone, or not
  -- in a moment" - neither the best nor the worst of no siblings.
  relative_sharpness REAL    NOT NULL DEFAULT 0.5
                     CHECK (relative_sharpness >= 0.0 AND relative_sharpness <= 1.0),

  -- --- Motion. Section 6.2. ------------------------------------------------
  -- 'none', 'camera_shake', 'subject_motion' or 'intentional'. Stored as the slug for
  -- the reason migrations 6, 7 and 8 all give: an integer whose meaning lives in a Rust
  -- enum is a column a support engineer cannot read. Here reading 'intentional' as
  -- 'camera_shake' turns the panned exit frame into a defect, which is the exact
  -- failure section 12 lists second.
  motion             TEXT    NOT NULL DEFAULT 'none'
                     CHECK (motion IN ('none','camera_shake','subject_motion','intentional')),
  motion_severity    REAL    NOT NULL DEFAULT 0.0
                     CHECK (motion_severity >= 0.0 AND motion_severity <= 1.0),

  -- --- Exposure. Section 6.3. ----------------------------------------------
  -- 'good', 'recoverable', 'marginal' or 'lost'. Four rather than section 6.3's three;
  -- see ADR-0019 section 2.
  exposure           TEXT    NOT NULL DEFAULT 'good'
                     CHECK (exposure IN ('good','recoverable','marginal','lost')),
  -- Fraction of pixels clipped, specular highlights already excluded. A clipped candle
  -- flame is not a defect and must not reach this column.
  clip_hi            REAL    NOT NULL DEFAULT 0.0 CHECK (clip_hi >= 0.0 AND clip_hi <= 1.0),
  clip_lo            REAL    NOT NULL DEFAULT 0.0 CHECK (clip_lo >= 0.0 AND clip_lo <= 1.0),
  -- Stops from a correct exposure. Negative is under. Bounded at six stops either way
  -- because past that the number is a guess about a file with no information in it.
  ev_offset          REAL    NOT NULL DEFAULT 0.0
                     CHECK (ev_offset >= -6.0 AND ev_offset <= 6.0),

  -- --- Noise. Section 6.3. -------------------------------------------------
  -- Sigma relative to the scene's tolerance at this ISO on this body: 1.0 is exactly
  -- tolerable. Not an absolute sigma - the same noise in a dance frame and a family
  -- portrait must produce two different numbers, which is invariant 7 applied to a
  -- measurement rather than to a threshold.
  noise_sigma_rel    REAL    NOT NULL DEFAULT 0.0 CHECK (noise_sigma_rel >= 0.0),

  -- --- Eyes, at the frame level. Section 6.4. ------------------------------
  -- Fraction of *gating* subjects whose eyes are closed, and the denominator beside it
  -- so that 0.0 with no gating faces is distinguishable from 0.0 with six of them.
  closed_eye_ratio   REAL    NOT NULL DEFAULT 0.0
                     CHECK (closed_eye_ratio >= 0.0 AND closed_eye_ratio <= 1.0),
  gating_faces       INTEGER NOT NULL DEFAULT 0 CHECK (gating_faces >= 0),

  -- --- The composite. Section 6.5. -----------------------------------------
  technical_score    REAL    NOT NULL DEFAULT 0.0
                     CHECK (technical_score >= 0.0 AND technical_score <= 1.0),
  -- The fourteen bits of `IntegrityFlags`. A stored bit pattern, so the positions in
  -- the contract are a wire format and a test asserts them.
  flags              INTEGER NOT NULL DEFAULT 0 CHECK (flags >= 0),
  -- JSON array of at most six objects: {code, text, weight, evidence:{x,y,w,h}|null}.
  -- Invariant 2, plus section 13's evidence-crop criterion.
  reasons            TEXT    NOT NULL DEFAULT '[]',
  confidence         REAL    NOT NULL DEFAULT 0.0
                     CHECK (confidence >= 0.0 AND confidence <= 1.0),

  -- Which scene the thresholds were conditioned on, as the slug `image_scenes` uses.
  -- Invariant 7: a stored verdict that does not say which scene it assumed cannot be
  -- reproduced. 'unknown' means the neutral profile was used.
  scene              TEXT    NOT NULL DEFAULT 'unknown',
  -- 1 when this body had no calibration row and the fallback was used.
  --
  -- The body's *name* is deliberately not here. A camera id is 68 characters and it is
  -- one join through `photo` away; the flag is asked per row by the coverage view and
  -- the name is asked once per project by the telemetry event. Storing both would spend
  -- a fifteenth of section 11's whole per-image budget answering a question nobody asks
  -- per row.
  uncalibrated       INTEGER NOT NULL DEFAULT 0 CHECK (uncalibrated IN (0,1)),

  -- 1 when the photographer dismissed a flag here. Checked inside the upsert.
  user_reviewed      INTEGER NOT NULL DEFAULT 0 CHECK (user_reviewed IN (0,1)),
  -- The bits the photographer dismissed, kept separately from `flags` so that a
  -- re-analysis can re-apply the dismissal to a freshly computed set. Storing only the
  -- cleared `flags` would lose *which* judgement was overruled the moment the analyser
  -- stopped making it.
  dismissed          INTEGER NOT NULL DEFAULT 0 CHECK (dismissed >= 0),

  model_ver          INTEGER NOT NULL DEFAULT 0,
  analysis_ver       INTEGER NOT NULL DEFAULT 0,
  calib_ver          INTEGER NOT NULL DEFAULT 0,
  created_at         TEXT    NOT NULL,
  updated_at         TEXT    NOT NULL,

  -- A verdict with a confidence has reasons behind it. Two characters is an empty JSON
  -- array, so `length > 2` is "the array has something in it".
  CHECK (confidence <= 0.0 OR length(reasons) > 2)
) STRICT;

-- The review queue's query: this project's worst frames first. **The only index on this
-- table**, and the two that were here first were removed after they were measured.
--
--   * `(project_id, flags)` looked like the filter chips' index and was not one. A chip
--     asks `flags & mask <> 0`, which is a bit test rather than a range, and SQLite
--     cannot seek a btree on it - so the index narrowed the scan to one project and this
--     one already does that. Sixty bytes per photograph for a covering column.
--   * `(project_id, model_ver, analysis_ver, calib_ver)` looked like the re-analysis
--     pass's index and was not one either: `IntegrityStore::pending` drives from `photo`
--     LEFT JOIN this table, so it is a lookup by `photo_id` per row and never a range
--     scan over versions.
--
-- Both were written from a guess about a query plan rather than from one. Section 11's
-- kilobyte is what made the difference worth measuring.
CREATE INDEX idx_integrity_project ON image_integrity(project_id, technical_score);

-- ---------------------------------------------------------------------------
-- One row per face, per analysed frame. Section 2.1's per-face eye state.
--
-- A separate table rather than JSON on `image_integrity`, for one reason that is worth
-- stating: the question "which frames have the bride blinking" is asked by identity,
-- and an identity inside a JSON blob is not a query. It is also the row the QA
-- false-rejection audit reads, and section 10.1 measures blink F1 against it.
--
-- `face_id` is phase 06's derived id - a hash of the photograph, the box and the
-- detector version - so a re-run of the face pass at the same version produces the
-- same key and this row is replaced rather than duplicated.
--
-- **No biometric data here.** An eye state is a five-way label about a rectangle, not a
-- template, and `docs/adr/ADR-0013` puts everything that identifies a person in the
-- sealed store in `aura-people`. This table is deleted by the same erasure path that
-- deletes a face row, by way of the foreign key.
-- ---------------------------------------------------------------------------
CREATE TABLE face_eye_state (
  -- `faces(id)`, not `faces(face_id)`: migration 6 names that column `id`. SQLite
  -- accepts a foreign key onto a column that does not exist at CREATE time and
  -- raises `foreign key mismatch` on the first INSERT instead, which is exactly how
  -- the phase 09 storage budget found this.
  face_id      TEXT    NOT NULL PRIMARY KEY REFERENCES faces(id) ON DELETE CASCADE,
  -- The identity, when one has been assigned. NULL is "not clustered yet", which is why
  -- `gates` is 0 for such a face: an unassigned face has no importance.
  identity_id  TEXT    REFERENCES identities(id) ON DELETE SET NULL,
  -- 'open', 'squint', 'closed', 'looking_down' or 'occluded'. The five classes are the
  -- eye head's output order and renumbering them relabels a trained model.
  state        TEXT    NOT NULL DEFAULT 'open'
               CHECK (state IN ('open','squint','closed','looking_down','occluded')),
  confidence   REAL    NOT NULL DEFAULT 0.0 CHECK (confidence >= 0.0 AND confidence <= 1.0),
  -- 1 when section 6.4's intent rules justify the closure. The single most
  -- trust-critical bit in the product: a kiss, a prayer and a tearful hug all have
  -- closed eyes in them.
  intentional  INTEGER NOT NULL DEFAULT 0 CHECK (intentional IN (0,1)),
  -- 1 when this face's eyes decide anything about the frame. A guest blinking in row
  -- four is not a defect; the bride blinking in a portrait is.
  gates        INTEGER NOT NULL DEFAULT 0 CHECK (gates IN (0,1)),
  -- No `area_frac` and no evidence rectangle. Both are already in `faces`: the area is
  -- `faces.area_frac` and the rectangle is the face box plus the two eye landmarks in
  -- `faces.landmarks_json`, which is exactly what `eyes::eye_crop` computes from. Five
  -- REAL columns per face is 120 bytes per photograph on a three-face frame - an eighth
  -- of section 11's budget - to store a second copy of phase 06's geometry, and the
  -- second copy would go stale the moment the face pass re-ran at a new detector version.
  model_ver    INTEGER NOT NULL DEFAULT 0,
  created_at   TEXT    NOT NULL
) STRICT, WITHOUT ROWID;

-- No `photo_id` column and no index on one. `faces.image_id` already answers "which
-- photograph is this face in", `idx_faces_image` already indexes it, and at 40 characters
-- plus its own index the redundancy measured 300 bytes per photograph against section
-- 11's 1 KB - nearly a third of the budget to save one join. Migration 8 made the same
-- trade for `moment_images`, and this file makes it for the same reason.
--
-- `WITHOUT ROWID` because the primary key *is* the row: a face has exactly one eye state,
-- so the separate rowid btree SQLite would otherwise maintain is a second copy of every
-- key for no query.
-- **No index at all**, and the one that was here was redundant with phase 06's.
--
-- The query it was written for - "which frames have this person blinking" - starts from
-- an identity, and `faces` already indexes that: `idx_faces_identity` finds the faces,
-- and each one is a primary-key lookup into this `WITHOUT ROWID` table. An index on
-- `(identity_id, state, intentional)` here duplicated `faces.identity_id` and cost 150
-- bytes per photograph on a three-face frame, which is a seventh of section 11's budget
-- to avoid a lookup that is already O(1).

-- ---------------------------------------------------------------------------
-- How much of a project has been checked.
--
-- The fifth coverage view, after embeddings, people, scenes and moments. The
-- denominator here is **every photograph**, which is different from
-- `v_moment_coverage`'s and deliberately so: grouping needs an embedding, so a frame
-- without one is a phase 05 gap, but a technical verdict needs only pixels. A frame
-- with no verdict is this phase's gap and reporting it any other way would hide it.
--
-- `subject_aware` is this phase's refinement of the rule. A wedding at 100 % coverage
-- and 2 % subject-aware has been judged on frame-wide sharpness nearly everywhere -
-- which is the ordinary global measure phase 09 exists to replace, and a caller about
-- to trust `subject_sharpness` has to be able to find that out.
-- ---------------------------------------------------------------------------
CREATE VIEW v_integrity_coverage AS
SELECT p.project_id,
       COUNT(DISTINCT ph.photo_id)                                    AS photos,
       COUNT(DISTINCT ii.photo_id)                                    AS scored,
       SUM(CASE WHEN ii.flags & 4096 = 0 THEN 1 ELSE 0 END)           AS subject_aware,
       SUM(CASE WHEN ii.user_reviewed = 1 THEN 1 ELSE 0 END)          AS reviewed,
       SUM(CASE WHEN ii.uncalibrated = 1 THEN 1 ELSE 0 END)           AS uncalibrated,
       AVG(ii.technical_score)                                        AS mean_score,
       MIN(ii.model_ver)                                              AS model_ver,
       MIN(ii.analysis_ver)                                           AS analysis_ver,
       MIN(ii.calib_ver)                                              AS calib_ver
FROM project p
LEFT JOIN photo ph            ON ph.project_id = p.project_id
LEFT JOIN image_integrity ii  ON ii.photo_id = ph.photo_id
WHERE p.deleted_at IS NULL
GROUP BY p.project_id;

-- ---------------------------------------------------------------------------
-- The flag histogram, as one row per project.
--
-- Section 11's `integrity.scored {flag_histogram}` event and the filter chips' badge
-- counts. Fourteen sums rather than fourteen queries: the chips are drawn on every
-- render of the grid header and a fan-out of one count per chip is fourteen scans of
-- the same rows.
--
-- Bit 3 (intentional motion), bit 10 (justified closed eyes) and bit 12 (no subject)
-- are counted like any other, because they are things the photographer wants to filter
-- *for* rather than defects to hunt down. `IntegrityFlags::DEFECTS` is where the
-- distinction lives, and it lives in one place.
-- ---------------------------------------------------------------------------
CREATE VIEW v_integrity_flags AS
SELECT project_id,
       COUNT(*)                                              AS scored,
       SUM(CASE WHEN flags &     1 <> 0 THEN 1 ELSE 0 END)   AS subject_soft,
       SUM(CASE WHEN flags &     2 <> 0 THEN 1 ELSE 0 END)   AS camera_shake,
       SUM(CASE WHEN flags &     4 <> 0 THEN 1 ELSE 0 END)   AS subject_motion,
       SUM(CASE WHEN flags &     8 <> 0 THEN 1 ELSE 0 END)   AS intentional_motion,
       SUM(CASE WHEN flags &    16 <> 0 THEN 1 ELSE 0 END)   AS back_focus,
       SUM(CASE WHEN flags &    32 <> 0 THEN 1 ELSE 0 END)   AS front_focus,
       SUM(CASE WHEN flags &    64 <> 0 THEN 1 ELSE 0 END)   AS highlight_lost,
       SUM(CASE WHEN flags &   128 <> 0 THEN 1 ELSE 0 END)   AS shadow_lost,
       SUM(CASE WHEN flags &   256 <> 0 THEN 1 ELSE 0 END)   AS heavy_noise,
       SUM(CASE WHEN flags &   512 <> 0 THEN 1 ELSE 0 END)   AS eyes_closed,
       SUM(CASE WHEN flags &  1024 <> 0 THEN 1 ELSE 0 END)   AS eyes_closed_ok,
       SUM(CASE WHEN flags &  2048 <> 0 THEN 1 ELSE 0 END)   AS squint,
       SUM(CASE WHEN flags &  4096 <> 0 THEN 1 ELSE 0 END)   AS no_subject_detected,
       SUM(CASE WHEN flags &  8192 <> 0 THEN 1 ELSE 0 END)   AS mixed_light_risk
FROM image_integrity
GROUP BY project_id;
