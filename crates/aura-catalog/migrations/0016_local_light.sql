-- Migration 16 - how the light inside one photograph was shaped.
--
-- PHASE-19 section 4 names no table at all: its file list is six decision modules, three
-- shaders, a policy file, two Python scripts and a panel. It needs two tables anyway, and
-- section 2.1's last bullet is why - "everything expressed as recipe masks + parameters so
-- it is fully reversible and inspectable". A recipe is reversible; it is not *inspectable*,
-- because a recipe says a face mask carries +0.34 EV and says nothing about the band that
-- number was aiming at, the noise cap that stopped it going further, or the twelve people
-- it was solved beside. `docs/adr/ADR-0033-local-light-sculpting.md` records the rest.
--
-- ---------------------------------------------------------------------------
-- WHAT THIS MIGRATION IS FOR
-- ---------------------------------------------------------------------------
--
-- Fifteen migrations have recorded what AURA found, what it decided, why the pixels should
-- move and what would happen to them. This one records **what it did locally, and what it
-- decided not to do** - and the second half is the larger one. Six of the thirty reason
-- codes in this phase describe an operation that was gated, capped or declined, and on a
-- build with no phase 18 they are the only codes most frames will ever carry.
--
-- Seven properties are enforced here rather than remembered:
--
-- 1. **There is no mask in this migration.** No alpha column, no matte blob, no geometry.
--    Phase 18 owns masks; this phase reads them through `MaskField` and writes *instructions
--    for* them into `edit_recipes`. A schema with somewhere to put a matte is a schema that
--    invites phase 19 to keep its own, and a second answer to "where does the subject end"
--    is a background reduction that traces an outline nothing else agrees with.
--
-- 2. **There is no image data anywhere, including in the shaping.** Phase 13's rule -
--    evidence can never be a pixel - applied to a *decision*. A dodge-and-burn map is stored
--    as the handful of named `ShapingZone` rows it was generated from, and the grid is
--    re-derived deterministically at render time. Ten zones at four floats each is 160
--    bytes; the 32x32 grids they generate are 2 KB per band per face, and a wedding's worth
--    of those is a catalog nobody can back up.
--
-- 3. **`shaping_ver` is the fourth version column, and it exists because of note 2.** A
--    change to the derivation changes what a delivered JPEG looks like without changing one
--    stored number. `model_ver` invalidates the learned targets, `analysis_ver` every
--    measurement, `policy_ver` the strengths, and `shaping_ver` the grids. `AURA-ML-5066` is
--    raised when a comparison would cross any of the four.
--
-- 4. **Nothing in this migration is an edit.** There is no path, no rendered output and no
--    applied flag. The values reach the pixels through `edit_recipes` and
--    `aura_recipe::schema::merge` only - phase 14's rule, for the third phase running - and
--    this table is the record of what the product thought, kept beside whatever the
--    photographer chose. A row with `user_edited = 1` still carries AURA's own strengths,
--    which is what makes the panel able to show a disagreement and phase 30's learning loop
--    able to read one.
--
-- 5. **`user_edited` is checked inside the statement, not before it.** The eighth time this
--    rule has been written into a migration, and the window it closes is the same one every
--    time: a re-analysis that read the row, decided, and then wrote would lose an override
--    set in between.
--
-- 6. **The budget is a stored number with a CHECK on it.** `budget_used` is bounded to
--    0..1 by the schema, not by the solver's arithmetic. Section 6.4's per-image perceptual
--    budget is the mechanism that stops six defensible adjustments adding up to a photograph
--    that looks processed, and a budget the database will not enforce is a budget that
--    eventually is not one.
--
-- 7. **A gated operation is a row, not an absence.** `local_light_gate` records which
--    operation was reduced or skipped and which mask kind caused it. Storing only the
--    operations that ran would make "phase 18 is not installed" and "there was nothing to
--    do" the same query, and those are the two states this phase most needs to tell apart.
--
-- ROLLBACK. Every object here is new and nothing outside this file references it:
--
--   DROP VIEW  IF EXISTS v_local_coverage;
--   DROP TABLE IF EXISTS local_light_gate;
--   DROP TABLE IF EXISTS local_light_shaping;
--   DROP TABLE IF EXISTS local_light_face;
--   DROP TABLE IF EXISTS local_light_plan;
--   DELETE FROM schema_version WHERE version = 16;
--
-- Running those returns the catalog to schema 15. **It is recomputable**, like migration 15
-- and unlike 13 and 14: every row here is derived from pixels, phase 06's faces, phase 07's
-- scenes, phase 09's noise and phase 15's bands, so a re-run reproduces it exactly - with
-- the usual exception. The six per-operation strengths in `user_strengths` are not derivable
-- from anything, and the rollback runbook says to export that column first. They also reach
-- `edit_recipes` and the sidecars beside the RAWs, which is the second copy that makes the
-- loss survivable.

-- ---------------------------------------------------------------------------
-- One row per photograph. What was shaped, how hard, and what stopped it.
-- ---------------------------------------------------------------------------
CREATE TABLE local_light_plan (
  photo_id            TEXT    NOT NULL PRIMARY KEY REFERENCES photo(photo_id) ON DELETE CASCADE,
  project_id          TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,

  -- The six strengths each operation actually ran at, after the scene policy, the mask
  -- scaling and the governor. In `LocalOp::PRIORITY` order and stored as six columns rather
  -- than a JSON array, because the panel's histogram, the outline's `op_histogram` and the
  -- review queue are all aggregates over them and a JSON extract per row per frame is the
  -- query that makes a project header feel slow.
  --
  -- Zero means the operation did not run. That is NOT the same as running and finding
  -- nothing to do, which is why every one of them has a reason code beside it in `reasons`.
  s_face_light        REAL    NOT NULL DEFAULT 0.0 CHECK (s_face_light        >= 0.0 AND s_face_light        <= 1.0),
  s_subject           REAL    NOT NULL DEFAULT 0.0 CHECK (s_subject           >= 0.0 AND s_subject           <= 1.0),
  s_background        REAL    NOT NULL DEFAULT 0.0 CHECK (s_background        >= 0.0 AND s_background        <= 1.0),
  s_shine             REAL    NOT NULL DEFAULT 0.0 CHECK (s_shine             >= 0.0 AND s_shine             <= 1.0),
  s_dodge_burn_low    REAL    NOT NULL DEFAULT 0.0 CHECK (s_dodge_burn_low    >= 0.0 AND s_dodge_burn_low    <= 1.0),
  s_dodge_burn_mid    REAL    NOT NULL DEFAULT 0.0 CHECK (s_dodge_burn_mid    >= 0.0 AND s_dodge_burn_mid    <= 1.0),

  -- See note 6. The mechanism that keeps this phase invisible, enforced by the schema.
  budget_used         REAL    NOT NULL DEFAULT 0.0 CHECK (budget_used >= 0.0 AND budget_used <= 1.0),

  -- The subject half of section 6.2's paired operation. Every one of these is an
  -- instruction for a mask in the recipe rather than a value applied to a pixel.
  subject_clarity     INTEGER NOT NULL DEFAULT 0 CHECK (subject_clarity  >= 0    AND subject_clarity  <= 100),
  subject_texture     INTEGER NOT NULL DEFAULT 0 CHECK (subject_texture  >= 0    AND subject_texture  <= 100),
  subject_contrast    INTEGER NOT NULL DEFAULT 0 CHECK (subject_contrast >= -100 AND subject_contrast <= 100),

  -- The background half. `bg_exposure_ev` is zero or negative and `bg_saturation` is zero or
  -- negative, both by CHECK: this phase calms a background and never enriches one, because
  -- enriching a background is a grade and grades are phase 16's.
  bg_exposure_ev      REAL    NOT NULL DEFAULT 0.0 CHECK (bg_exposure_ev <= 0.0 AND bg_exposure_ev >= -3.0),
  bg_highlights       INTEGER NOT NULL DEFAULT 0 CHECK (bg_highlights  <= 0 AND bg_highlights  >= -100),
  bg_saturation       INTEGER NOT NULL DEFAULT 0 CHECK (bg_saturation  <= 0 AND bg_saturation  >= -100),
  bg_feather          REAL    NOT NULL DEFAULT 0.0 CHECK (bg_feather >= 0.0 AND bg_feather <= 1.0),

  -- Section 6.2's three measured triggers, stored rather than recomputed. An operation that
  -- fired without one of them crossing a threshold is a bug, and `aura-cli verify --phase 19`
  -- checks it as a query rather than as a re-run.
  competition_ratio   REAL    NOT NULL DEFAULT 1.0 CHECK (competition_ratio >= 0.0),
  chroma_energy       REAL    NOT NULL DEFAULT 0.0 CHECK (chroma_energy >= 0.0),
  bright_blobs        INTEGER NOT NULL DEFAULT 0 CHECK (bright_blobs >= 0),

  -- Section 10.1's own acceptance criterion, as two columns rather than as a claim.
  -- `|after - before|` must be within MAX_MEAN_LUMA_DRIFT and the store refuses a row where
  -- it is not.
  mean_luma_before    REAL    NOT NULL DEFAULT 0.0 CHECK (mean_luma_before >= 0.0 AND mean_luma_before <= 1.0),
  mean_luma_after     REAL    NOT NULL DEFAULT 0.0 CHECK (mean_luma_after  >= 0.0 AND mean_luma_after  <= 1.0),

  -- Shine. A luminance operation and nothing else: there is no radius, no strength-of-blur
  -- and no texture column here, which is what keeps the obvious wrong fix one ADR away
  -- rather than one refactor away.
  shine_regions       INTEGER NOT NULL DEFAULT 0 CHECK (shine_regions >= 0 AND shine_regions <= 6),
  shine_ev            REAL    NOT NULL DEFAULT 0.0 CHECK (shine_ev <= 0.0 AND shine_ev >= -1.0),
  shine_area          REAL    NOT NULL DEFAULT 0.0 CHECK (shine_area >= 0.0 AND shine_area <= 1.0),
  -- The regions themselves, as a compact JSON array of crop rectangles. Read only by the
  -- panel that draws them and never queried across, which is the same argument migration 15
  -- made for its three documents.
  shine_boxes         TEXT    NOT NULL DEFAULT '[]',

  -- Group fairness, as one number a query can sort on. Section 10.1's "inter-face luminance
  -- spread after lighting <= a documented threshold" is a `SELECT ... WHERE face_spread >`
  -- rather than a re-measurement.
  face_spread         REAL    NOT NULL DEFAULT 0.0 CHECK (face_spread >= 0.0),
  faces_lit           INTEGER NOT NULL DEFAULT 0 CHECK (faces_lit >= 0),
  group_solved        INTEGER NOT NULL DEFAULT 0 CHECK (group_solved IN (0,1)),

  -- Which scene the strengths came from. Invariant 7: a stored decision that does not say
  -- which scene it assumed is not reproducible.
  scene               TEXT    NOT NULL DEFAULT 'unknown',
  -- True when that scene had no policy row and the neutral strengths were used.
  unpolicied          INTEGER NOT NULL DEFAULT 0 CHECK (unpolicied IN (0,1)),

  confidence          REAL    NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),

  -- A reason stores its code, not its sentence. Phase 09's rule, for the fifth migration
  -- running: a stored sentence is copy a release can change without any behaviour changing,
  -- and a catalog full of English is a catalog nobody can translate. The `t` key is present
  -- only when the sentence differs from `LocalCode::user_text`.
  reasons             TEXT    NOT NULL DEFAULT '[]',
  -- Invariant 2, enforced by the schema rather than by review. Eight rather than six because
  -- a frame can be gated on four masks and still have acted on two.
  reason_count        INTEGER NOT NULL DEFAULT 0 CHECK (reason_count >= 1 AND reason_count <= 8),

  -- The photographer's own answer, and the two bits that protect it. See note 5. Six
  -- nullable strengths in one JSON object rather than six more columns: unlike the machine's
  -- own strengths, nothing aggregates over these, and a sparse override is exactly the shape
  -- a document stores well.
  user_edited         INTEGER NOT NULL DEFAULT 0 CHECK (user_edited IN (0,1)),
  reviewed            INTEGER NOT NULL DEFAULT 0 CHECK (reviewed IN (0,1)),
  user_strengths      TEXT,

  -- Four versions, because they invalidate four different things. See note 3.
  model_ver           INTEGER NOT NULL,
  analysis_ver        INTEGER NOT NULL,
  policy_ver          INTEGER NOT NULL,
  shaping_ver         INTEGER NOT NULL,

  at_ts               INTEGER NOT NULL,
  at                  TEXT    NOT NULL
) STRICT;

-- The outline, the histogram and the pending set all scan one project. One index rather
-- than five, which is phase 09's lesson about indexes that serve no query.
CREATE INDEX idx_local_project ON local_light_plan(project_id, photo_id);

-- The low-confidence review queue, weakest first. Partial, because the column is above the
-- threshold for the overwhelming majority of a wedding and the only query that reads it asks
-- about the ones where it is not.
CREATE INDEX idx_local_review
  ON local_light_plan(project_id, confidence)
  WHERE reviewed = 0 AND user_edited = 0;

-- Phase 20's query: which frames has phase 19 already evened, so retouch does not do it
-- twice. Partial for the same reason - most frames carry no mid-frequency work at all.
CREATE INDEX idx_local_evened
  ON local_light_plan(project_id)
  WHERE s_dodge_burn_mid > 0.0;

-- ---------------------------------------------------------------------------
-- One row per face that was lit. What it was aiming at and what stopped it.
--
-- A child table rather than a JSON column, unlike migration 15's illuminants, and the
-- reason is the group-fairness rule: `face_spread` on the parent is an aggregate over
-- these, phase 20 joins them by identity to avoid double-lifting a face, and both of those
-- are queries across rows rather than reads of a document.
-- ---------------------------------------------------------------------------
CREATE TABLE local_light_face (
  photo_id            TEXT    NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,
  -- Zero first, in the order the solver produced them. Deterministic: the solver sorts faces
  -- by prominence and then by bounding box, so a re-run addresses the same face with the
  -- same ordinal.
  ordinal             INTEGER NOT NULL CHECK (ordinal >= 0 AND ordinal < 64),

  -- NULL where phase 06 identified nobody, which at most weddings is the majority of
  -- guests. A schema that required an identity here would have to either invent one or
  -- refuse to light most of the people in the room.
  identity_id         TEXT    REFERENCES identities(id) ON DELETE SET NULL,

  exposure_ev         REAL    NOT NULL DEFAULT 0.0 CHECK (exposure_ev >= -1.0 AND exposure_ev <= 1.5),
  shadows             INTEGER NOT NULL DEFAULT 0 CHECK (shadows    >= -100 AND shadows    <= 100),
  -- Never positive. A face is never lifted by pushing its highlights up; that is exactly the
  -- operation that makes a forehead glow, and the CHECK is cheaper than remembering.
  highlights          INTEGER NOT NULL DEFAULT 0 CHECK (highlights <= 0 AND highlights >= -100),
  feather             REAL    NOT NULL DEFAULT 0.0 CHECK (feather >= 0.0 AND feather <= 1.0),

  -- Three luminances: where it was, where the band wanted it, where it ended up. The third
  -- differs from the second on the majority of real frames, and the difference is the whole
  -- explanation the panel shows.
  luma_before         REAL    NOT NULL DEFAULT 0.0 CHECK (luma_before >= 0.0 AND luma_before <= 1.0),
  luma_target         REAL    NOT NULL DEFAULT 0.0 CHECK (luma_target >= 0.0 AND luma_target <= 1.0),
  luma_after          REAL    NOT NULL DEFAULT 0.0 CHECK (luma_after  >= 0.0 AND luma_after  <= 1.0),

  -- The dynamic cap of section 6.1, from phase 09's measured noise and the scene's shadow
  -- budget. Stored so the panel can say what stopped the lift rather than only that it did.
  noise_cap_ev        REAL    NOT NULL DEFAULT 0.0 CHECK (noise_cap_ev >= 0.0),
  mask_scale          REAL    NOT NULL DEFAULT 0.0 CHECK (mask_scale >= 0.0 AND mask_scale <= 1.0),

  PRIMARY KEY (photo_id, ordinal)
) STRICT, WITHOUT ROWID;

-- Phase 20's join, and the People panel's "what has been done to this person" question.
CREATE INDEX idx_local_face_identity ON local_light_face(identity_id);

-- ---------------------------------------------------------------------------
-- The shaping, as zones rather than as pixels. See note 2.
-- ---------------------------------------------------------------------------
CREATE TABLE local_light_shaping (
  photo_id            TEXT    NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,
  -- Which face in `local_light_face` this shapes.
  face_ordinal        INTEGER NOT NULL CHECK (face_ordinal >= 0 AND face_ordinal < 64),
  -- Zero first, in `FaceZone::ALL` order among the zones present.
  zone_ordinal        INTEGER NOT NULL CHECK (zone_ordinal >= 0 AND zone_ordinal < 16),

  zone                TEXT    NOT NULL,
  cx                  REAL    NOT NULL CHECK (cx >= 0.0 AND cx <= 1.0),
  cy                  REAL    NOT NULL CHECK (cy >= 0.0 AND cy <= 1.0),
  radius              REAL    NOT NULL CHECK (radius > 0.0 AND radius <= 1.0),

  -- Bounded here as well as in `ShapingZone::MAX_GAIN_EV`, and deliberately: a sixth of a
  -- stop is small enough that a bug producing ten times it would still render, and the only
  -- place that would be caught is a schema that refuses to store it.
  gain_ev             REAL    NOT NULL CHECK (gain_ev >= -0.2 AND gain_ev <= 0.2),

  PRIMARY KEY (photo_id, face_ordinal, zone_ordinal)
) STRICT, WITHOUT ROWID;

-- ---------------------------------------------------------------------------
-- One row per operation that was reduced or skipped. See note 7.
-- ---------------------------------------------------------------------------
CREATE TABLE local_light_gate (
  photo_id            TEXT    NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,
  -- `LocalOp::as_str`.
  op                  TEXT    NOT NULL,
  -- `MaskKind::as_str`. The mask kind that was missing or weak.
  mask_kind           TEXT    NOT NULL,
  -- What the mask's own confidence was, so a support engineer can tell "phase 18 is not
  -- installed" (0.0) from "phase 18 is unsure here" (0.4) without opening a proxy.
  mask_conf           REAL    NOT NULL DEFAULT 0.0 CHECK (mask_conf >= 0.0 AND mask_conf <= 1.0),

  PRIMARY KEY (photo_id, op)
) STRICT, WITHOUT ROWID;

-- Section 11's `local.gated {mask_kind, count}` is a GROUP BY on this index.
CREATE INDEX idx_local_gate_kind ON local_light_gate(mask_kind);

-- ---------------------------------------------------------------------------
-- How much of a wedding has a local light plan, and how much of that plan actually did
-- anything.
--
-- The denominator is **every photograph in the project**, as phases 09, 10, 11, 14 and 15's
-- are. The second column is the one that matters and it is this phase's own refinement of
-- the rule: because the work is meant to be invisible, a wedding at 100 % coverage and 4 %
-- acted-on looks exactly like a wedding that was worked on, and `acted_on` is the only way a
-- photographer finds out otherwise.
-- ---------------------------------------------------------------------------
CREATE VIEW v_local_coverage AS
SELECT
  p.project_id                                                AS project_id,
  COUNT(*)                                                    AS images,
  COUNT(l.photo_id)                                           AS planned,
  SUM(CASE WHEN l.s_face_light     > 0.0
             OR l.s_subject        > 0.0
             OR l.s_background     > 0.0
             OR l.s_shine          > 0.0
             OR l.s_dodge_burn_low > 0.0
             OR l.s_dodge_burn_mid > 0.0 THEN 1 ELSE 0 END)   AS acted_on,
  SUM(CASE WHEN l.group_solved = 1 THEN 1 ELSE 0 END)         AS group_solved,
  SUM(CASE WHEN l.shine_regions > 0 THEN 1 ELSE 0 END)        AS shine_reduced,
  SUM(CASE WHEN l.user_edited  = 1 THEN 1 ELSE 0 END)         AS user_edited,
  SUM(CASE WHEN l.photo_id IS NOT NULL
             AND l.reviewed = 0
             AND l.user_edited = 0
             AND l.confidence < 0.50 THEN 1 ELSE 0 END)       AS needs_review
FROM photo p
LEFT JOIN local_light_plan l ON l.photo_id = p.photo_id
GROUP BY p.project_id;
