-- Migration 22 - the small fixes, and the one place this product composites two photographs.
--
-- PHASE-21 section 4 names no table: its file list is seven decision modules, four Python
-- scripts, a config file, a panel and an ethics document. It needs three tables, a view and two
-- triggers anyway, and section 6.3's last sentence is why - "cross-frame borrowing is limited to
-- small regions, requires high alignment confidence, and is always recorded in the recipe and the
-- Explain panel so it is never a hidden composite". A recipe records what happened to a
-- photograph; it does not record what was *refused*, which is most of this phase, and it cannot
-- answer "did anything in this gallery get composited" without opening four hundred files.
-- `docs/adr/ADR-0045-micro-retouch-and-cross-frame-borrowing.md` records the rest.
--
-- ---------------------------------------------------------------------------
-- WHAT THIS MIGRATION IS FOR
-- ---------------------------------------------------------------------------
--
-- Twenty migrations recorded what AURA found, what it decided, how the pixels should move and
-- what was done to somebody's skin. This one records **what was done to the rest of a person**,
-- and it is the first schema in the product in which a stored row can point at a second
-- photograph. Seven properties are enforced here rather than remembered:
--
-- 1. **A borrow names its source, or it is not a borrow.** `micro_op.method = 'borrow'` requires
--    `borrowed_from` to be non-NULL and `alignment` to be at or above the contract's floor, as a
--    CHECK. The trigger `micro_op_borrow_disclosed` catches the UPDATE that would clear the
--    source afterwards. Section 6.3 says a composite is never hidden; a promise that lives only
--    in the type is a promise until somebody writes an INSERT by hand.
--
-- 2. **The disclosure question is answerable in one query.** `v_micro_composites` lists every
--    frame in a project that borrowed and what it borrowed from. A composite that can only be
--    found by opening plans one at a time is a composite nobody finds, and the delivery report,
--    the Explain panel and phase 27's QC agent all read this view.
--
-- 3. **A withdrawn family is a fact about a frame, not an absence.** `withdrawn_hair`,
--    `withdrawn_teeth` and `withdrawn_eyes` are three columns rather than one flag, because the
--    three measurements are over three disjoint regions and a photographer whose complaint is
--    "her teeth look odd" needs to find out which of the three was low. A CHECK stops a row
--    claiming a family was withdrawn while operations of that family are still counted.
--
-- 4. **The naturalness guarantee is a stored number with a CHECK on it.** `catchlight_ratio` and
--    `hair_energy_ratio` are bounded and may not sit below their floors while the row claims it
--    passed; `teeth_excursion` may not sit above its ceiling. Section 0's headline KPI is then
--    `SELECT MIN(catchlight_ratio)` over a wedding rather than a sentence in a document.
--
-- 5. **There is no image data anywhere.** Phase 13's rule, eighth migration running. An
--    operation is a rectangle, a kind and a magnitude; a borrow is a rectangle and a photo id.
--    There is no patch, no donor pixel and no mask - phase 18 owns masks and this phase reads
--    them.
--
-- 6. **There is nowhere to put a displacement, a scale or a landmark.** Section 11 of
--    `docs/plan/CLAUDE.md` forbids body reshaping, face swapping and eye replacement
--    permanently, and a schema with no column for them makes adding one a visible contract
--    change. The operation kind is a CHECK over five names and the clothing kind is a CHECK over
--    five more; neither can be widened without this file changing and `contracts.lock` moving.
--
-- 7. **The opt-in matrix is per project and a photographer's version of it is never
--    overwritten.** `micro_matrix` carries one row per project with `user_edited` checked inside
--    the statement a re-analysis would overwrite it with - the tenth time this rule has been
--    written into a migration.
--
-- ROLLBACK. Every object here is new and nothing outside this file references it:
--
--   DROP VIEW    IF EXISTS v_micro_composites;
--   DROP VIEW    IF EXISTS v_micro_coverage;
--   DROP TRIGGER IF EXISTS micro_op_borrow_disclosed;
--   DROP TRIGGER IF EXISTS micro_op_no_opt_in_by_default;
--   DROP TABLE   IF EXISTS micro_op;
--   DROP TABLE   IF EXISTS micro_matrix;
--   DROP TABLE   IF EXISTS micro_plan;
--   DELETE FROM schema_version WHERE version = 21;
--
-- Running those returns the catalog to schema 20. **It is recomputable with one exception**: every
-- plan and operation is derived from pixels, phase 06's faces, phase 08's moments and phase 18's
-- regions, so a re-run reproduces them exactly - but `micro_matrix` rows with `user_edited = 1`
-- are not derivable from anything. The rollback runbook says to export that table first. A studio
-- telling the product it does not want composited pixels in its deliveries is the most expensive
-- piece of data in this migration.

-- ---------------------------------------------------------------------------
-- One row per photograph: what ran, what was refused, and what the guard measured.
-- ---------------------------------------------------------------------------
CREATE TABLE micro_plan (
  photo_id            TEXT    NOT NULL PRIMARY KEY REFERENCES photo(photo_id) ON DELETE CASCADE,
  project_id          TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,

  -- The scene it was decided under. Invariant 7, and stored rather than re-read: a
  -- re-classification changes what the plan should have been, and a plan that silently starts
  -- describing itself under a new scene is a plan nobody can audit.
  scene               TEXT    NOT NULL,

  -- See note 4. The guarantee, as three numbers.
  catchlight_ratio    REAL    NOT NULL DEFAULT 1.0 CHECK (catchlight_ratio >= 0.0 AND catchlight_ratio <= 4.0),
  hair_energy_ratio   REAL    NOT NULL DEFAULT 1.0 CHECK (hair_energy_ratio >= 0.0 AND hair_energy_ratio <= 4.0),
  teeth_excursion     REAL    NOT NULL DEFAULT 0.0 CHECK (teeth_excursion >= 0.0 AND teeth_excursion <= 1.0),
  -- How many pixels the three were measured over, summed. A ratio over eleven samples is
  -- arithmetic rather than evidence, and the panel says so rather than printing three decimals.
  measured_on         INTEGER NOT NULL DEFAULT 0 CHECK (measured_on >= 0),
  -- How much strength the guard gave up to get there, 0..9 across three families.
  resolves            INTEGER NOT NULL DEFAULT 0 CHECK (resolves >= 0 AND resolves <= 9),

  -- See note 3. Three columns, not one flag.
  withdrawn_hair      INTEGER NOT NULL DEFAULT 0 CHECK (withdrawn_hair IN (0,1)),
  withdrawn_teeth     INTEGER NOT NULL DEFAULT 0 CHECK (withdrawn_teeth IN (0,1)),
  withdrawn_eyes      INTEGER NOT NULL DEFAULT 0 CHECK (withdrawn_eyes IN (0,1)),

  -- Which operations the matrix permitted on this frame when it was planned. Stored rather than
  -- looked up, because the matrix is a project setting that can change and a plan has to remain
  -- explicable after it does.
  allow_flyaway       INTEGER NOT NULL DEFAULT 0 CHECK (allow_flyaway IN (0,1)),
  allow_teeth         INTEGER NOT NULL DEFAULT 0 CHECK (allow_teeth IN (0,1)),
  allow_eyes          INTEGER NOT NULL DEFAULT 0 CHECK (allow_eyes IN (0,1)),
  allow_clothing      INTEGER NOT NULL DEFAULT 0 CHECK (allow_clothing IN (0,1)),
  allow_glare         INTEGER NOT NULL DEFAULT 0 CHECK (allow_glare IN (0,1)),

  -- Denormalised from `micro_op` so the outline, the review queue and the grid badge are one
  -- aggregate each rather than a join per frame. The store writes both in one transaction.
  op_count            INTEGER NOT NULL DEFAULT 0 CHECK (op_count >= 0 AND op_count <= 80),
  flyaway_count       INTEGER NOT NULL DEFAULT 0 CHECK (flyaway_count >= 0),
  teeth_count         INTEGER NOT NULL DEFAULT 0 CHECK (teeth_count >= 0),
  eyes_count          INTEGER NOT NULL DEFAULT 0 CHECK (eyes_count >= 0),
  clothing_count      INTEGER NOT NULL DEFAULT 0 CHECK (clothing_count >= 0),
  glare_count         INTEGER NOT NULL DEFAULT 0 CHECK (glare_count >= 0),
  -- How many of the glare operations borrowed. See note 2: this is what makes the project-level
  -- disclosure a single aggregate rather than a scan of `micro_op`.
  borrow_count        INTEGER NOT NULL DEFAULT 0 CHECK (borrow_count >= 0),

  -- The share of phase 19's shared per-image perceptual allowance this plan spent. Shared for the
  -- third time: twelve operations across three phases that each respect their own budget still
  -- add up to a photograph that looks worked on.
  budget_used         REAL    NOT NULL DEFAULT 0.0 CHECK (budget_used >= 0.0 AND budget_used <= 1.0),

  -- True when phase 18 supplied at least one usable region for this frame. Zero on a build with
  -- no mask generator wired in, which is the honest reading of such a build - and the difference
  -- between "there was nothing to fix" and "AURA could not see where the teeth were".
  region_covered      INTEGER NOT NULL DEFAULT 0 CHECK (region_covered IN (0,1)),

  confidence          REAL    NOT NULL DEFAULT 0.0 CHECK (confidence >= 0.0 AND confidence <= 1.0),
  -- Reason codes, comma-separated slugs from `MicroCode::as_str`, in emission order. Codes
  -- rather than sentences: phase 09's rule, eighth migration running.
  reasons             TEXT    NOT NULL DEFAULT '',

  user_edited         INTEGER NOT NULL DEFAULT 0 CHECK (user_edited IN (0,1)),
  reviewed            INTEGER NOT NULL DEFAULT 0 CHECK (reviewed IN (0,1)),

  model_ver           INTEGER NOT NULL DEFAULT 0,
  analysis_ver        INTEGER NOT NULL DEFAULT 0,
  matrix_ver          INTEGER NOT NULL DEFAULT 0,
  planned_at          TEXT    NOT NULL,

  -- See note 4. A row may not claim a floor it is sitting below.
  CHECK (catchlight_ratio >= 0.98 - 0.0001 OR withdrawn_eyes = 1),
  CHECK (hair_energy_ratio >= 0.94 - 0.0001 OR withdrawn_hair = 1),
  CHECK (teeth_excursion <= 0.003 + 0.000001 OR withdrawn_teeth = 1),
  -- See note 3. A withdrawn family carries no operations.
  CHECK (withdrawn_hair = 0 OR flyaway_count = 0),
  CHECK (withdrawn_teeth = 0 OR teeth_count = 0),
  CHECK (withdrawn_eyes = 0 OR (eyes_count = 0 AND glare_count = 0)),
  -- An operation that was not permitted did not run.
  CHECK (allow_flyaway = 1 OR flyaway_count = 0),
  CHECK (allow_teeth = 1 OR teeth_count = 0),
  CHECK (allow_eyes = 1 OR eyes_count = 0),
  CHECK (allow_clothing = 1 OR clothing_count = 0),
  CHECK (allow_glare = 1 OR glare_count = 0),
  -- See note 1, at the plan level: a frame cannot have borrowed more often than it had glare.
  CHECK (borrow_count <= glare_count)
);

-- The review queue: weakest first, and only frames nobody has settled.
CREATE INDEX idx_micro_review ON micro_plan(project_id, confidence)
  WHERE reviewed = 0 AND user_edited = 0;

-- The resumable pass, which is a query rather than a journal: every frame in this project whose
-- versions do not match the running build is pending by definition.
CREATE INDEX idx_micro_versions ON micro_plan(project_id, model_ver, analysis_ver, matrix_ver);

-- See note 2. The disclosure index: which frames in this wedding composited anything.
CREATE INDEX idx_micro_borrows ON micro_plan(project_id)
  WHERE borrow_count > 0;

-- Phase 25's query and phase 27's: which frames were touched at all, and how hard the guard had
-- to work on them.
CREATE INDEX idx_micro_guarded ON micro_plan(project_id, catchlight_ratio)
  WHERE op_count > 0;

-- ---------------------------------------------------------------------------
-- One row per project: which operations the studio permits. See note 7.
-- ---------------------------------------------------------------------------
CREATE TABLE micro_matrix (
  project_id          TEXT    NOT NULL PRIMARY KEY REFERENCES project(project_id) ON DELETE CASCADE,

  allow_flyaway       INTEGER NOT NULL DEFAULT 1 CHECK (allow_flyaway IN (0,1)),
  allow_teeth         INTEGER NOT NULL DEFAULT 1 CHECK (allow_teeth IN (0,1)),
  allow_eyes          INTEGER NOT NULL DEFAULT 1 CHECK (allow_eyes IN (0,1)),
  allow_clothing      INTEGER NOT NULL DEFAULT 1 CHECK (allow_clothing IN (0,1)),
  allow_glare         INTEGER NOT NULL DEFAULT 1 CHECK (allow_glare IN (0,1)),

  -- The five clothing issues. `strap` and `crease` default to 0 and the contract marks both
  -- opt-in only: `ClothingIssue::is_opt_in_only` is what stops a config file switching them on,
  -- and this default is what stops a project starting with them on.
  allow_lint          INTEGER NOT NULL DEFAULT 1 CHECK (allow_lint IN (0,1)),
  allow_thread        INTEGER NOT NULL DEFAULT 1 CHECK (allow_thread IN (0,1)),
  allow_stain         INTEGER NOT NULL DEFAULT 1 CHECK (allow_stain IN (0,1)),
  allow_strap         INTEGER NOT NULL DEFAULT 0 CHECK (allow_strap IN (0,1)),
  allow_crease        INTEGER NOT NULL DEFAULT 0 CHECK (allow_crease IN (0,1)),

  -- Separate from `allow_glare` deliberately: a studio can want reflections calmed and want no
  -- composited pixels in a delivery. Collapsing the two would force them to choose.
  allow_borrow        INTEGER NOT NULL DEFAULT 1 CHECK (allow_borrow IN (0,1)),

  user_edited         INTEGER NOT NULL DEFAULT 0 CHECK (user_edited IN (0,1)),
  matrix_ver          INTEGER NOT NULL DEFAULT 0,
  updated_at          TEXT    NOT NULL
) WITHOUT ROWID;

-- ---------------------------------------------------------------------------
-- One row per operation. See notes 1, 5 and 6.
-- ---------------------------------------------------------------------------
CREATE TABLE micro_op (
  photo_id            TEXT    NOT NULL REFERENCES micro_plan(photo_id) ON DELETE CASCADE,
  seq                 INTEGER NOT NULL CHECK (seq >= 0),

  -- See note 6. Five names, and widening the set is a contract change that moves
  -- `contracts.lock`.
  kind                TEXT    NOT NULL
                      CHECK (kind IN ('flyaway','teeth','eyes','clothing','glare')),

  -- Frame coordinates for the three operations that name a rectangle; NULL for teeth and eyes,
  -- which act through a landmark region. A zero rectangle would read as "the top-left corner".
  x                   REAL,
  y                   REAL,
  w                   REAL,
  h                   REAL,

  -- Whose face, for the two operations that name a person.
  identity_id         TEXT             REFERENCES identities(id) ON DELETE SET NULL,

  -- Flyaway, clothing and conservative glare all carry one magnitude. Bounded here at the
  -- widest of the three ceilings; the exact per-kind bound is in the contract, which the store
  -- checks before it writes.
  strength            REAL    NOT NULL DEFAULT 0.0 CHECK (strength >= 0.0 AND strength <= 1.0),

  -- Teeth carry two magnitudes and eyes carry two, and each pair is stored separately because
  -- the panel shows them separately: somebody who thinks the teeth look wrong wants to know
  -- whether it was the lift or the colour.
  luma_ev             REAL    NOT NULL DEFAULT 0.0 CHECK (luma_ev >= 0.0 AND luma_ev <= 0.20),
  yellow_reduce       REAL    NOT NULL DEFAULT 0.0 CHECK (yellow_reduce >= 0.0 AND yellow_reduce <= 0.35),
  sclera              REAL    NOT NULL DEFAULT 0.0 CHECK (sclera >= 0.0 AND sclera <= 0.30),
  iris_clarity        REAL    NOT NULL DEFAULT 0.0 CHECK (iris_clarity >= 0.0 AND iris_clarity <= 0.25),

  -- 'lint', 'thread', 'stain', 'strap' or 'crease', for a clothing operation. NULL otherwise.
  clothing_kind       TEXT             CHECK (clothing_kind IS NULL OR clothing_kind IN ('lint','thread','stain','strap','crease')),

  -- 'reduce' or 'borrow', for a glare operation. NULL otherwise.
  method              TEXT             CHECK (method IS NULL OR method IN ('reduce','borrow')),
  -- See note 1. The source photograph, for a borrow.
  borrowed_from       TEXT             REFERENCES photo(photo_id) ON DELETE RESTRICT,
  -- How well the two regions aligned. At or above the contract's floor for a borrow.
  alignment           REAL    NOT NULL DEFAULT 0.0 CHECK (alignment >= 0.0 AND alignment <= 1.0),

  PRIMARY KEY (photo_id, seq),

  -- See note 1. A borrow names its source and clears the alignment floor; anything else names
  -- neither. The floor is written out rather than referenced because SQLite has no constants,
  -- and it is `MIN_ALIGNMENT` in `aura_core::contract::micro`.
  CHECK (method IS NOT 'borrow' OR (borrowed_from IS NOT NULL AND alignment >= 0.82)),
  CHECK (method IS 'borrow' OR borrowed_from IS NULL),
  -- A borrow may not name the frame it is repairing.
  CHECK (borrowed_from IS NULL OR borrowed_from <> photo_id)
);

-- Phase 27's query: everything borrowed in this wedding, by source.
CREATE INDEX idx_micro_op_borrow ON micro_op(borrowed_from) WHERE borrowed_from IS NOT NULL;

-- See note 1. The promise, enforced by the database rather than by the application.
--
-- There is no code path in `aura-retouch` that attempts this, which is the point: this catches
-- the path somebody adds in phase 24 or phase 30 without reading ADR-0045. `ON DELETE RESTRICT`
-- on `borrowed_from` is the third layer - deleting the source photograph of a borrow fails
-- rather than silently orphaning the disclosure.
CREATE TRIGGER micro_op_borrow_disclosed
BEFORE UPDATE OF borrowed_from, method ON micro_op
WHEN OLD.method = 'borrow' AND (NEW.borrowed_from IS NULL OR NEW.method <> 'borrow')
BEGIN
  SELECT RAISE(ABORT, 'a borrowed region may never lose its source: AURA does not deliver undisclosed composites');
END;

-- The two operations the contract marks opt-in only cannot arrive by default. A studio switches
-- them on through `micro_matrix`; nothing else may write one, and an INSERT that names one while
-- the project has not enabled it is aborted rather than obeyed.
CREATE TRIGGER micro_op_no_opt_in_by_default
BEFORE INSERT ON micro_op
WHEN NEW.clothing_kind IN ('strap','crease')
 AND NOT EXISTS (
   SELECT 1 FROM micro_matrix m
     JOIN micro_plan p ON p.project_id = m.project_id
    WHERE p.photo_id = NEW.photo_id
      AND ((NEW.clothing_kind = 'strap'  AND m.allow_strap  = 1)
        OR (NEW.clothing_kind = 'crease' AND m.allow_crease = 1)))
BEGIN
  SELECT RAISE(ABORT, 'bra straps and creases are removed only when a studio has switched them on');
END;

-- ---------------------------------------------------------------------------
-- Coverage, in the shape every outline since phase 09 reports it.
-- ---------------------------------------------------------------------------
CREATE VIEW v_micro_coverage AS
SELECT
  p.project_id                                              AS project_id,
  COUNT(*)                                                  AS photos,
  SUM(CASE WHEN m.photo_id IS NOT NULL THEN 1 ELSE 0 END)   AS planned,
  SUM(CASE WHEN m.op_count > 0 THEN 1 ELSE 0 END)           AS acted_on,
  SUM(CASE WHEN m.region_covered = 1 THEN 1 ELSE 0 END)     AS region_covered,
  SUM(CASE WHEN m.borrow_count > 0 THEN 1 ELSE 0 END)       AS borrowed,
  SUM(CASE WHEN m.resolves > 0 THEN 1 ELSE 0 END)           AS resolved,
  SUM(m.withdrawn_hair)                                     AS withdrawn_hair,
  SUM(m.withdrawn_teeth)                                    AS withdrawn_teeth,
  SUM(m.withdrawn_eyes)                                     AS withdrawn_eyes,
  AVG(CASE WHEN m.eyes_count + m.glare_count > 0 THEN m.catchlight_ratio END)  AS mean_catchlight_ratio,
  AVG(CASE WHEN m.flyaway_count > 0 THEN m.hair_energy_ratio END)              AS mean_hair_energy_ratio
FROM photo p
LEFT JOIN micro_plan m ON m.photo_id = p.photo_id
GROUP BY p.project_id;

-- ---------------------------------------------------------------------------
-- See note 2. The disclosure query, as a view, because it is asked by three callers.
-- ---------------------------------------------------------------------------
CREATE VIEW v_micro_composites AS
SELECT
  pl.project_id       AS project_id,
  op.photo_id         AS photo_id,
  op.borrowed_from    AS source_photo_id,
  op.alignment        AS alignment,
  op.x                AS x,
  op.y                AS y,
  op.w                AS w,
  op.h                AS h
FROM micro_op op
JOIN micro_plan pl ON pl.photo_id = op.photo_id
WHERE op.borrowed_from IS NOT NULL
ORDER BY pl.project_id, op.photo_id, op.seq;
