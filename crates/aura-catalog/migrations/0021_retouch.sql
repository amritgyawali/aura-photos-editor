-- Migration 21 - what was done to somebody's skin, and what will never be done to it.
--
-- PHASE-20 section 4 names no table: its file list is eight decision modules, three shaders,
-- four Python scripts, a preset file, a panel and two model cards. It needs three tables and a
-- view anyway, and section 2.1's last bullet is why - "every operation recorded in the recipe
-- as a reversible retouch op". A recipe is reversible; it is not *explicable*, because a
-- recipe says a blemish op removed something at (0.41, 0.33) and says nothing about the mark
-- that was left alone beside it, the mole that vetoed a third one, or the band ratio the whole
-- thing was measured against. `docs/adr/ADR-0043-portrait-retouch-and-texture-protection.md`
-- records the rest.
--
-- ---------------------------------------------------------------------------
-- WHAT THIS MIGRATION IS FOR
-- ---------------------------------------------------------------------------
--
-- Nineteen migrations recorded what AURA found, what it decided and how the pixels should
-- move. This one records **what it did to a person**, and that is a different kind of row.
-- Eight properties are enforced here rather than remembered:
--
-- 1. **`retouch_protected` is the first table in this product whose rows a photographer
--    creates directly**, and the first whose subject is a person rather than a photograph. A
--    protect row is stored in *face-normalised* coordinates - origin between the eyes, x along
--    the eye-to-eye line, unit the inter-ocular distance - so one row protects the same mole
--    in four hundred photographs. Storing it per frame would mean a mole is protected in the
--    frames the detector happened to see it in, which is the same as not protecting it.
--
-- 2. **A tattoo cannot be unprotected, and the schema says so.** `is_absolute` is a generated
--    column rather than a flag a writer supplies: `kind = 'tattoo'` and nothing else. The
--    trigger `retouch_protected_absolute` aborts any DELETE or any UPDATE that would clear one.
--    Section 10.1 gates tattoo removal at zero per cent; a zero implemented as a small
--    threshold in application code is a zero that expires the next time somebody retrains a
--    detector.
--
-- 3. **A photographer's protect row is never overwritten.** `source = 'user'` is checked
--    inside the DELETE a re-analysis starts with - the ninth time this rule has been written
--    into a migration - and it is checked here in the same statement rather than before it,
--    because a re-analysis that read, decided and then wrote would lose a row set in between.
--
-- 4. **The texture guarantee is a stored number with a CHECK on it.** `band_ratio` is bounded,
--    `texture_floor` may not go below 0.80, and `texture_passed` may not be 1 while the ratio
--    is below the floor. Section 0's headline KPI is then `SELECT MIN(band_ratio)` over a
--    wedding rather than a sentence in a document - and a product that could only assert it
--    would have no way to find out it had stopped being true.
--
-- 5. **A withdrawn retouch is a row with no operations, not an absent row.** `withdrawn = 1`
--    with `op_count = 0` is "AURA tried and could not do this safely"; an absent row is "nobody
--    has looked at this photograph". Phases 21, 25 and 27 all read this table and all three act
--    differently on those two states.
--
-- 6. **There is no image data anywhere.** Phase 13's rule, seventh migration running. An
--    operation is a rectangle, a method and a strength; a protect row is a rectangle, a kind
--    and its evidence. There is no patch, no donor pixel and no mask - phase 18 owns masks and
--    this phase reads them.
--
-- 7. **There is nowhere to put a reshaping, a skin-tone target or a face swap.** Section 11 of
--    `docs/plan/CLAUDE.md` forbids all three permanently, and a schema with no column for them
--    makes adding one a visible contract change rather than a quiet field. The operation kind
--    is a CHECK constraint over four names, and none of them can be widened without this file
--    changing and `contracts.lock` moving with it.
--
-- 8. **Strength belongs to a person, not to a photograph.** `retouch_identity` carries one row
--    per identity per project, and `retouch_plan` stores no per-face strength at all. Section
--    10.1's cross-frame consistency gate - one identity varying by no more than five per cent
--    across a gallery - is then true by construction rather than by measurement, which is a
--    stronger guarantee than the one asked for. ADR-0043 section 6.
--
-- ROLLBACK. Every object here is new and nothing outside this file references it:
--
--   DROP VIEW    IF EXISTS v_retouch_coverage;
--   DROP TRIGGER IF EXISTS retouch_protected_absolute;
--   DROP TABLE   IF EXISTS retouch_op;
--   DROP TABLE   IF EXISTS retouch_protected;
--   DROP TABLE   IF EXISTS retouch_identity;
--   DROP TABLE   IF EXISTS retouch_plan;
--   DELETE FROM schema_version WHERE version = 20;
--
-- Running those returns the catalog to schema 19. **It is recomputable with one exception**,
-- and the exception is the important one: every plan, operation and machine-made protect row
-- is derived from pixels, phase 06's faces and phase 18's masks, so a re-run reproduces them
-- exactly - but `retouch_protected` rows with `source = 'user'` and `retouch_identity` rows
-- with `user_edited = 1` are not derivable from anything. The rollback runbook says to export
-- both first. A photographer telling the product to keep somebody's beauty mark is the single
-- most expensive piece of data in this migration.

-- ---------------------------------------------------------------------------
-- One row per photograph: what was done, under what preset, and what it cost the texture.
-- ---------------------------------------------------------------------------
CREATE TABLE retouch_plan (
  photo_id            TEXT    NOT NULL PRIMARY KEY REFERENCES photo(photo_id) ON DELETE CASCADE,
  project_id          TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,

  -- 'off', 'light', 'natural' or 'polished'. `off` is a preset rather than a NULL because a
  -- frame the photographer switched retouching off on and a frame that found nothing to do are
  -- two different answers, and a coverage report has to tell them apart.
  preset              TEXT    NOT NULL DEFAULT 'natural'
                      CHECK (preset IN ('off','light','natural','polished')),

  -- The scene it was decided under. Invariant 7, and stored rather than re-read: a
  -- re-classification changes what the plan should have been, and a plan that silently starts
  -- describing itself under a new scene is a plan nobody can audit.
  scene               TEXT    NOT NULL,

  -- See note 4. The whole phase in three numbers.
  band_ratio          REAL    NOT NULL DEFAULT 1.0 CHECK (band_ratio >= 0.0 AND band_ratio <= 4.0),
  texture_floor       REAL    NOT NULL DEFAULT 0.90 CHECK (texture_floor >= 0.80 AND texture_floor <= 1.0),
  texture_passed      INTEGER NOT NULL DEFAULT 1 CHECK (texture_passed IN (0,1)),
  -- How many skin samples the ratio was measured over. A ratio over eleven samples is
  -- arithmetic rather than evidence, and the panel says so rather than printing three decimals.
  texture_samples     INTEGER NOT NULL DEFAULT 0 CHECK (texture_samples >= 0),
  -- How much strength the guard had to give up to get there, 0..3.
  texture_resolves    INTEGER NOT NULL DEFAULT 0 CHECK (texture_resolves >= 0 AND texture_resolves <= 3),
  -- See note 5.
  withdrawn           INTEGER NOT NULL DEFAULT 0 CHECK (withdrawn IN (0,1)),

  -- Denormalised from `retouch_op` so the outline, the review queue and the grid badge are one
  -- aggregate each rather than a join per frame. The store writes both in one transaction and
  -- `v_retouch_coverage` is what a caller compares them through.
  op_count            INTEGER NOT NULL DEFAULT 0 CHECK (op_count >= 0 AND op_count <= 200),
  blemish_count       INTEGER NOT NULL DEFAULT 0 CHECK (blemish_count >= 0),
  faces_seen          INTEGER NOT NULL DEFAULT 0 CHECK (faces_seen >= 0),
  faces_retouched     INTEGER NOT NULL DEFAULT 0 CHECK (faces_retouched >= 0),
  -- Anomalies the detector found and deliberately did not touch. The most important number in
  -- the row for a photographer who asks why a mark is still there.
  anomalies_left      INTEGER NOT NULL DEFAULT 0 CHECK (anomalies_left >= 0),

  -- The share of phase 19's shared per-image perceptual allowance this plan spent. Shared
  -- rather than duplicated: six local operations and a retouch that each respect their own
  -- budget still add up to a photograph that looks worked on.
  budget_used         REAL    NOT NULL DEFAULT 0.0 CHECK (budget_used >= 0.0 AND budget_used <= 1.0),

  -- True when phase 18 supplied a usable skin mask for this frame. Zero on a build with no
  -- mask generator wired in, which is the honest reading of such a build - and the difference
  -- between "there was nothing to do" and "AURA could not see where the skin was".
  mask_covered        INTEGER NOT NULL DEFAULT 0 CHECK (mask_covered IN (0,1)),

  confidence          REAL    NOT NULL DEFAULT 0.0 CHECK (confidence >= 0.0 AND confidence <= 1.0),
  -- Reason codes, comma-separated slugs from `RetouchCode::as_str`, in emission order.
  -- Codes rather than sentences: phase 09's rule, seventh migration running. A stored sentence
  -- is copy a release can change, and a catalog full of English cannot be translated.
  reasons             TEXT    NOT NULL DEFAULT '',

  user_edited         INTEGER NOT NULL DEFAULT 0 CHECK (user_edited IN (0,1)),
  reviewed            INTEGER NOT NULL DEFAULT 0 CHECK (reviewed IN (0,1)),

  model_ver           INTEGER NOT NULL DEFAULT 0,
  analysis_ver        INTEGER NOT NULL DEFAULT 0,
  preset_ver          INTEGER NOT NULL DEFAULT 0,
  planned_at          TEXT    NOT NULL,

  -- See note 4. The guarantee, as a constraint rather than as a convention: a row may not
  -- claim it passed while sitting below the floor it was held to. A withdrawn plan is exempt
  -- because it applied nothing at all.
  CHECK (texture_passed = 0 OR withdrawn = 1 OR band_ratio >= texture_floor - 0.0001),
  -- See note 5.
  CHECK (withdrawn = 0 OR op_count = 0)
);

-- The review queue: weakest first, and only frames nobody has settled.
CREATE INDEX idx_retouch_review ON retouch_plan(project_id, confidence)
  WHERE reviewed = 0 AND user_edited = 0;

-- The resumable pass, which is a query rather than a journal: every frame in this project whose
-- versions do not match the running build is pending by definition.
CREATE INDEX idx_retouch_versions ON retouch_plan(project_id, model_ver, analysis_ver, preset_ver);

-- Phase 25's query, and phase 27's: which frames were retouched at all, and how hard the
-- texture guard had to work on them.
CREATE INDEX idx_retouch_texture ON retouch_plan(project_id, band_ratio)
  WHERE op_count > 0;

-- ---------------------------------------------------------------------------
-- One row per person per project: the gallery-constant strength. See note 8.
-- ---------------------------------------------------------------------------
CREATE TABLE retouch_identity (
  project_id          TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  identity_id         TEXT    NOT NULL REFERENCES identities(id) ON DELETE CASCADE,

  -- What every frame in the gallery retouches this person at. One number, deliberately: this
  -- is the column that makes "the same person looks like the same person across the whole
  -- gallery" (section 13) structural rather than measured.
  strength            REAL    NOT NULL DEFAULT 0.0 CHECK (strength >= 0.0 AND strength <= 1.0),

  -- The four gallery statistics it was computed from, kept so the panel can explain the number
  -- and so a re-computation under a new preset table can be compared against the old one.
  -- 'couple', 'close_family', 'guest' or 'unknown', from phase 06.
  role                TEXT    NOT NULL DEFAULT 'unknown',
  median_face_frac    REAL    NOT NULL DEFAULT 0.0 CHECK (median_face_frac >= 0.0 AND median_face_frac <= 1.0),
  dominant_scene      TEXT    NOT NULL DEFAULT '',
  frames              INTEGER NOT NULL DEFAULT 0 CHECK (frames >= 0),

  -- Set by `RetouchService::set_override` with an identity strength, and never overwritten by
  -- a re-analysis. See note 3.
  user_edited         INTEGER NOT NULL DEFAULT 0 CHECK (user_edited IN (0,1)),
  preset_ver          INTEGER NOT NULL DEFAULT 0,
  updated_at          TEXT    NOT NULL,

  PRIMARY KEY (project_id, identity_id)
) WITHOUT ROWID;

-- ---------------------------------------------------------------------------
-- One row per protected feature per person. See notes 1, 2 and 3.
-- ---------------------------------------------------------------------------
CREATE TABLE retouch_protected (
  protected_id        TEXT    NOT NULL PRIMARY KEY,
  identity_id         TEXT    NOT NULL REFERENCES identities(id) ON DELETE CASCADE,
  project_id          TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,

  kind                TEXT    NOT NULL
                      CHECK (kind IN ('mole','freckle','birthmark','scar','tattoo','dimple')),

  -- See note 1. Face-normalised, so x and y may be negative: the origin is the midpoint
  -- between the eyes and the unit is the inter-ocular distance.
  fx                  REAL    NOT NULL,
  fy                  REAL    NOT NULL,
  fw                  REAL    NOT NULL CHECK (fw > 0.0),
  fh                  REAL    NOT NULL CHECK (fh > 0.0),

  confidence          REAL    NOT NULL DEFAULT 1.0 CHECK (confidence >= 0.0 AND confidence <= 1.0),
  -- 'cross_frame', 'classifier' or 'user', in ascending order of authority.
  source              TEXT    NOT NULL DEFAULT 'cross_frame'
                      CHECK (source IN ('cross_frame','classifier','user')),

  -- Section 6.1's cross-frame evidence, stored because it is the answer to the only question a
  -- photographer asks of this list: why do you think that is permanent.
  frames              INTEGER NOT NULL DEFAULT 0 CHECK (frames >= 0),
  span_minutes        REAL    NOT NULL DEFAULT 0.0 CHECK (span_minutes >= 0.0),
  first_seen_photo    TEXT             REFERENCES photo(photo_id) ON DELETE SET NULL,

  created_at          TEXT    NOT NULL,

  -- See note 2. Generated rather than supplied, so no writer can set it and no writer can
  -- forget it.
  is_absolute         INTEGER GENERATED ALWAYS AS (CASE WHEN kind = 'tattoo' THEN 1 ELSE 0 END) VIRTUAL
);

CREATE INDEX idx_retouch_protected_identity ON retouch_protected(identity_id, kind);
CREATE INDEX idx_retouch_protected_project ON retouch_protected(project_id, source);

-- See note 2. The promise, enforced by the database rather than by the application.
--
-- Two statements because SQLite triggers are per operation: a DELETE that would remove an
-- absolute row, and an UPDATE that would move one out of `tattoo` and thereby make it
-- deletable. There is no code path in `aura-retouch` that attempts either, which is the point:
-- this catches the path somebody adds in phase 24 without reading ADR-0043.
CREATE TRIGGER retouch_protected_absolute
BEFORE DELETE ON retouch_protected
WHEN OLD.kind = 'tattoo'
BEGIN
  SELECT RAISE(ABORT, 'AURA never alters tattoos: a protected tattoo cannot be removed');
END;

CREATE TRIGGER retouch_protected_absolute_update
BEFORE UPDATE OF kind ON retouch_protected
WHEN OLD.kind = 'tattoo' AND NEW.kind <> 'tattoo'
BEGIN
  SELECT RAISE(ABORT, 'AURA never alters tattoos: a protected tattoo cannot be reclassified');
END;

-- ---------------------------------------------------------------------------
-- One row per operation. See note 6: a rectangle, a method and a strength.
-- ---------------------------------------------------------------------------
CREATE TABLE retouch_op (
  photo_id            TEXT    NOT NULL REFERENCES retouch_plan(photo_id) ON DELETE CASCADE,
  seq                 INTEGER NOT NULL CHECK (seq >= 0),

  -- See note 7. Four names, and widening the set is a contract change that moves
  -- `contracts.lock`.
  kind                TEXT    NOT NULL
                      CHECK (kind IN ('blemish','under_eye','tone_evening','shine_reduce')),

  -- Frame coordinates for the operations that name a rectangle; NULL for the two that act
  -- through a mask or a landmark region. A zero rectangle would read as "the top-left corner",
  -- which is the mistake `FaceRef::has_eyes` exists to prevent one phase earlier.
  x                   REAL,
  y                   REAL,
  w                   REAL,
  h                   REAL,

  strength            REAL    NOT NULL DEFAULT 0.0 CHECK (strength >= 0.0 AND strength <= 1.0),
  -- 'patch' or 'learned', for blemishes. NULL otherwise.
  method              TEXT             CHECK (method IS NULL OR method IN ('patch','learned')),
  -- Whose face, for the operations that name a person.
  identity_id         TEXT             REFERENCES identities(id) ON DELETE SET NULL,
  -- The phase 18 region, for tone evening.
  mask_id             TEXT,
  -- Under-eye carries two magnitudes rather than one strength, and both are capped by the
  -- contract. Stored separately because the panel shows them separately: a photographer who
  -- thinks the eyes look lifted wants to know which of the two moved.
  luma_ev             REAL    NOT NULL DEFAULT 0.0 CHECK (luma_ev >= -0.25 AND luma_ev <= 0.25),
  chroma              REAL    NOT NULL DEFAULT 0.0 CHECK (chroma >= -0.12 AND chroma <= 0.12),
  -- 'low' or 'mid'. There is no 'high': the high band is pores, and no operation in this phase
  -- may name it. See `FreqBand`.
  band                TEXT             CHECK (band IS NULL OR band IN ('low','mid')),

  PRIMARY KEY (photo_id, seq)
) WITHOUT ROWID;

-- ---------------------------------------------------------------------------
-- Coverage, in the shape every outline since phase 09 reports it.
-- ---------------------------------------------------------------------------
CREATE VIEW v_retouch_coverage AS
SELECT
  p.project_id                                              AS project_id,
  COUNT(*)                                                  AS photos,
  SUM(CASE WHEN r.photo_id IS NOT NULL THEN 1 ELSE 0 END)   AS planned,
  SUM(CASE WHEN r.op_count > 0 THEN 1 ELSE 0 END)           AS acted_on,
  SUM(CASE WHEN r.mask_covered = 1 THEN 1 ELSE 0 END)       AS mask_covered,
  SUM(CASE WHEN r.withdrawn = 1 THEN 1 ELSE 0 END)          AS withdrawn,
  SUM(CASE WHEN r.texture_resolves > 0 THEN 1 ELSE 0 END)   AS texture_resolved,
  AVG(CASE WHEN r.op_count > 0 THEN r.band_ratio END)       AS mean_band_ratio
FROM photo p
LEFT JOIN retouch_plan r ON r.photo_id = p.photo_id
GROUP BY p.project_id;
