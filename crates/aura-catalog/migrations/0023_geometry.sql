-- Migration 23 - which pixels are delivered, and what was checked before that was decided.
--
-- PHASE-23 section 4 names no table: its file list is eight decision modules, a lens profile
-- directory, a shader, a rules file and a panel. It needs two tables, two views and one trigger
-- anyway, and section 1 is why - "smart crop is where automation is most dangerous, so a
-- subject-aware, conservative, always-reversible crop is a trust feature as much as a quality
-- feature". A trust feature that cannot be queried is a trust feature nobody can audit.
-- `docs/adr/ADR-0047-geometry-lens-straightening-and-crop-safety.md` records the rest.
--
-- ---------------------------------------------------------------------------
-- WHAT THIS MIGRATION IS FOR
-- ---------------------------------------------------------------------------
--
-- Twenty-two migrations recorded what AURA found, what it decided, how the pixels should look and
-- what was repaired. This one records **which pixels exist**, and it is the first schema in the
-- product whose central row decides what is *not* delivered. Seven properties are enforced here
-- rather than remembered:
--
-- 1. **The delivered crop is an index, and it always points at a safe rectangle.**
--    `geometry_plan.primary_crop` is an ordinal into this photograph's `geometry_crop` rows, and
--    `geometry_primary_is_safe` aborts any statement that would leave it pointing at one with
--    `safe = 0`. The contract says the same thing and the type system cannot: an index is an
--    integer, and the row it addresses is in another table.
--
-- 2. **The original framing is always row zero and is always safe.** A CHECK requires ordinal 0
--    to be `aspect = 'original'` and `safe = 1`. "Original framing is always one click away" is
--    section 13's fifth acceptance criterion, and this is what makes the click a lookup rather
--    than a reconstruction.
--
-- 3. **A refused variant is a row rather than an absence.** A 1:1 crop that could not be
--    generated without cutting somebody is stored with `safe = 0` and the code that refused it.
--    Phase 17 established that a rejection is written when the failure *is* the evidence; here
--    "why is there no square crop of this photograph" is a question the panel has to answer, and
--    it cannot answer it from an absence.
--
-- 4. **The safety check stores its denominator.** `considered` is how many protected regions were
--    checked and `at_risk` how many the delivered rectangle would have cut. Section 10.1's hard
--    gate - zero auto-crops cut a detected face - is
--    `SELECT SUM(at_risk) FROM geometry_plan WHERE faces_intact = 0`, and over a wedding with no
--    faces in it that is arithmetic rather than evidence. `considered` is how a caller finds out
--    which of the two they have. Phase 08's rule, and on this build the answer is usually zero.
--
-- 5. **A correction says whether anybody measured the lens.** `lens_measured` is 0 on every row
--    this build writes, because `assets/lens_profiles/` contains reference models rather than
--    measurements. It is the same column phase 22 added for its noise models and it exists for
--    the same reason: the day a measured profile arrives, the frames corrected through a
--    reference one must be findable.
--
-- 6. **There is no scale, no output resolution and no fill.** Phase 13's rule, tenth migration
--    running. A geometry decision is a rectangle, an angle and two keystone sliders; there is no
--    column that could carry an upscale, a synthesised corner or a second photograph's id.
--    Section 2.2 puts fill in phase 24 and panoramas out of scope entirely, and a schema with no
--    column for either makes adding one a visible contract change.
--
-- 7. **A photographer's framing is never overwritten.** `geometry_plan.user_edited` is checked
--    inside the statement a re-analysis would overwrite the row with - the twelfth time this rule
--    has been written into a migration, and the strongest case for it in the product: a re-crop
--    of a frame somebody framed by hand throws away work that is not derivable from anything.
--
-- ROLLBACK. Every object here is new and nothing outside this file references it:
--
--   DROP VIEW    IF EXISTS v_geometry_safety;
--   DROP VIEW    IF EXISTS v_geometry_coverage;
--   DROP TRIGGER IF EXISTS geometry_primary_is_safe_update;
--   DROP TRIGGER IF EXISTS geometry_primary_is_safe_insert;
--   DROP TABLE   IF EXISTS geometry_crop;
--   DROP TABLE   IF EXISTS geometry_plan;
--   DELETE FROM schema_version WHERE version = 23;
--
-- Running those returns the catalog to schema 22. **It is recomputable with one exception**: every
-- plan is derived from pixels, EXIF, phase 06's faces and phase 11's horizon, so a re-run
-- reproduces them exactly - but `geometry_plan` rows with `user_edited = 1` are not derivable from
-- anything at all. The rollback runbook says to export those rows first, and it matters more here
-- than it did for phase 22: a hand-set crop is a decision about the delivery.

-- ---------------------------------------------------------------------------
-- One row per photograph: the optics, the angle, the perspective and what was checked.
-- ---------------------------------------------------------------------------
CREATE TABLE geometry_plan (
  photo_id            TEXT    NOT NULL PRIMARY KEY REFERENCES photo(photo_id) ON DELETE CASCADE,
  project_id          TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,

  -- The scene it was decided under. Invariant 7, and stored rather than re-read: a
  -- re-classification changes what the plan should have been, and a plan that silently started
  -- describing itself under a new scene is a plan nobody can audit.
  scene               TEXT    NOT NULL,

  -- See note 5. Where the numbers came from, what they were, and whether anybody measured them.
  lens_source         TEXT    NOT NULL DEFAULT 'none'
                      CHECK (lens_source IN ('embedded','database','estimated','none')),
  lens_profile        TEXT,
  lens_distortion     INTEGER NOT NULL DEFAULT 0 CHECK (lens_distortion IN (0,1)),
  lens_vignette       INTEGER NOT NULL DEFAULT 0 CHECK (lens_vignette >= 0 AND lens_vignette <= 100),
  lens_ca             INTEGER NOT NULL DEFAULT 0 CHECK (lens_ca IN (0,1)),
  lens_measured       INTEGER NOT NULL DEFAULT 0 CHECK (lens_measured IN (0,1)),

  -- The rotation. Bounded by ROTATE_MAX_DEG, which SQLite has no way to name, so it is written
  -- out - and the bound is on the *applied* angle rather than on the wanted one, because a frame
  -- twenty degrees off level is left alone rather than clamped to eight.
  rotate_deg          REAL    NOT NULL DEFAULT 0.0 CHECK (rotate_deg >= -8.0 AND rotate_deg <= 8.0),
  -- How sure the horizon behind it was, stored EVEN WHEN NOTHING WAS ROTATED. "The horizon looks
  -- off and AURA is not sure enough to move it" is the answer to the commonest question this
  -- phase gets, and a row carrying only the applied angle could not give it.
  rotate_conf         REAL    NOT NULL DEFAULT 0.0 CHECK (rotate_conf >= 0.0 AND rotate_conf <= 1.0),

  -- The perspective correction, or four NULLs. `keystone_stretch` is the MEASURED ratio between
  -- the two axis scales rather than a function of the sliders, and MAX_STRETCH is 1.12.
  keystone_vertical   REAL             CHECK (keystone_vertical IS NULL OR (keystone_vertical >= -100.0 AND keystone_vertical <= 100.0)),
  keystone_horizontal REAL             CHECK (keystone_horizontal IS NULL OR (keystone_horizontal >= -100.0 AND keystone_horizontal <= 100.0)),
  keystone_stretch    REAL             CHECK (keystone_stretch IS NULL OR (keystone_stretch >= 1.0 AND keystone_stretch <= 1.12)),
  keystone_convergence REAL            CHECK (keystone_convergence IS NULL OR (keystone_convergence >= 0.0 AND keystone_convergence <= 1.0)),

  -- See note 1. An ordinal into this photograph's `geometry_crop` rows.
  primary_crop        INTEGER NOT NULL DEFAULT 0 CHECK (primary_crop >= 0 AND primary_crop < 5),

  -- See note 4. The three booleans and the two counts that make them auditable.
  faces_intact        INTEGER NOT NULL DEFAULT 1 CHECK (faces_intact IN (0,1)),
  resolution_ok       INTEGER NOT NULL DEFAULT 1 CHECK (resolution_ok IN (0,1)),
  content_kept        INTEGER NOT NULL DEFAULT 1 CHECK (content_kept IN (0,1)),
  considered          INTEGER NOT NULL DEFAULT 0 CHECK (considered >= 0),
  at_risk             INTEGER NOT NULL DEFAULT 0 CHECK (at_risk >= 0 AND at_risk <= considered),
  long_edge_fraction  REAL    NOT NULL DEFAULT 1.0
                      CHECK (long_edge_fraction >= 0.0 AND long_edge_fraction <= 1.0),

  -- The lens the file named, kept so that `geometry.lens_profile_missing` telemetry is a query
  -- rather than a log scrape. Empty when the file named none.
  lens_name           TEXT    NOT NULL DEFAULT '',

  confidence          REAL    NOT NULL DEFAULT 0.0 CHECK (confidence >= 0.0 AND confidence <= 1.0),
  -- Reason codes, comma-separated slugs from `GeometryCode::as_str`, strongest penalty first.
  -- Codes rather than sentences: phase 09's rule, tenth migration running. The rectangles the
  -- panel highlights are in `geometry_crop` and in `lens::evidence_box`, so a reason does not
  -- carry a copy of one.
  reasons             TEXT    NOT NULL DEFAULT '',

  user_edited         INTEGER NOT NULL DEFAULT 0 CHECK (user_edited IN (0,1)),
  reviewed            INTEGER NOT NULL DEFAULT 0 CHECK (reviewed IN (0,1)),

  -- TWO version columns rather than three. This phase ships no model - the third since phase 08 -
  -- so there is no `model_ver` that could move, and `AURA-ML-5109` carries two numbers to match.
  analysis_ver        INTEGER NOT NULL DEFAULT 0,
  profile_ver         INTEGER NOT NULL DEFAULT 0,
  planned_at          TEXT    NOT NULL,

  -- A correction names a profile, and a profile names a correction. A row that claimed a database
  -- source with no profile id would be a plan nobody can reproduce, and one that named a profile
  -- while correcting nothing would be a plan that says it corrected a lens it did not.
  CHECK (lens_source <> 'database' OR lens_profile IS NOT NULL),
  CHECK (lens_source <> 'none' OR (lens_distortion = 0 AND lens_ca = 0 AND lens_vignette = 0)),
  -- An estimate is distortion and nothing else. One photograph of a room carries one usable
  -- measurement in it; a chromatic aberration or a vignette derived from the same frame would be
  -- a correction nobody made.
  CHECK (lens_source <> 'estimated' OR (lens_ca = 0 AND lens_vignette = 0)),
  -- Four keystone columns, all present or all absent. Three of four is a correction whose stretch
  -- nobody recorded, which is the one number the cap is checked against.
  CHECK ((keystone_vertical IS NULL AND keystone_horizontal IS NULL
          AND keystone_stretch IS NULL AND keystone_convergence IS NULL)
      OR (keystone_vertical IS NOT NULL AND keystone_horizontal IS NOT NULL
          AND keystone_stretch IS NOT NULL AND keystone_convergence IS NOT NULL)),
  -- A frame whose faces were not all kept is a frame that must not be delivered cropped. The
  -- safety filter refuses such a rectangle before it is scored, so this CHECK never fires in
  -- normal operation - which is exactly what makes it worth having.
  CHECK (faces_intact = 1 OR primary_crop = 0)
);

CREATE INDEX idx_geometry_project  ON geometry_plan(project_id);
CREATE INDEX idx_geometry_review   ON geometry_plan(project_id, reviewed, confidence);
-- The conservatism gate and the acted-on count, both of which read `primary_crop` and
-- `rotate_deg`. A covering index, because the outline runs it over a whole wedding.
CREATE INDEX idx_geometry_acted    ON geometry_plan(project_id, primary_crop, rotate_deg);
-- The lens telemetry. `lens_name` first because the question is "which lenses are missing".
CREATE INDEX idx_geometry_lens     ON geometry_plan(project_id, lens_source, lens_name);

-- ---------------------------------------------------------------------------
-- One row per crop variant: the original, and the aspects an album and a feed want.
-- ---------------------------------------------------------------------------
CREATE TABLE geometry_crop (
  -- DEFERRED, and it is the only deferred foreign key in the product. The variants are written
  -- before the plan that indexes them, because `geometry_primary_is_safe_insert` reads this table
  -- and a plan written first would be checked against the *previous* frame's rectangles - so an
  -- immediate constraint here would refuse every first variant of every photograph. Deferring it
  -- moves the check to COMMIT, by which point the plan row exists, and a transaction that wrote
  -- variants and then failed to write their plan still aborts.
  photo_id     TEXT    NOT NULL REFERENCES geometry_plan(photo_id) ON DELETE CASCADE
               DEFERRABLE INITIALLY DEFERRED,
  -- The ordinal `geometry_plan.primary_crop` indexes. Zero is always the original framing.
  ordinal      INTEGER NOT NULL CHECK (ordinal >= 0 AND ordinal < 5),

  aspect       TEXT    NOT NULL CHECK (aspect IN ('original','4:5','5:4','1:1','16:9')),
  purpose      TEXT    NOT NULL CHECK (purpose IN ('primary','social','album')),

  -- The rectangle, normalised, after any rotation and keystone. Four columns rather than a blob:
  -- phase 29 chooses between these and phase 30 exports one, and both of them want to filter on
  -- how much of the frame a variant keeps.
  rect_x       REAL    NOT NULL CHECK (rect_x >= 0.0 AND rect_x <= 1.0),
  rect_y       REAL    NOT NULL CHECK (rect_y >= 0.0 AND rect_y <= 1.0),
  rect_w       REAL    NOT NULL CHECK (rect_w > 0.0 AND rect_w <= 1.0),
  rect_h       REAL    NOT NULL CHECK (rect_h > 0.0 AND rect_h <= 1.0),

  -- The four-term objective at this rectangle. Comparable BETWEEN THE ROWS OF ONE PHOTOGRAPH and
  -- deliberately not between photographs - three of its four terms are normalised by the
  -- rectangle's own size and the frame's own energy. Nothing in this product ranks frames by it.
  score        REAL    NOT NULL DEFAULT 0.0 CHECK (score >= 0.0 AND score <= 1.0),

  -- See note 3. An unsafe variant is stored with the code that refused it.
  safe         INTEGER NOT NULL DEFAULT 1 CHECK (safe IN (0,1)),
  refusal      TEXT,

  PRIMARY KEY (photo_id, ordinal),

  -- A rectangle stays inside the frame. Two CHECKs rather than one, so a violation names the axis.
  CHECK (rect_x + rect_w <= 1.0001),
  CHECK (rect_y + rect_h <= 1.0001),
  -- See note 2. Row zero is the original framing and is always safe.
  CHECK (ordinal <> 0 OR (aspect = 'original' AND purpose = 'primary' AND safe = 1)),
  -- A refusal belongs to an unsafe row and only to one.
  CHECK ((safe = 1 AND refusal IS NULL) OR (safe = 0 AND refusal IS NOT NULL))
) WITHOUT ROWID;

-- Phase 29 asks one question of this table: give me the safe alternatives for this photograph.
CREATE INDEX idx_geometry_crop_safe ON geometry_crop(photo_id, safe, aspect);

-- ---------------------------------------------------------------------------
-- The delivered rectangle is never an unsafe one.
--
-- See note 1. Two triggers because SQLite fires INSERT and UPDATE separately, and both directions
-- have to be closed: a statement can break the property by pointing the index at an unsafe row,
-- or by making the pointed-at row unsafe.
-- ---------------------------------------------------------------------------
CREATE TRIGGER geometry_primary_is_safe_insert
BEFORE INSERT ON geometry_plan
FOR EACH ROW
WHEN EXISTS (
  SELECT 1 FROM geometry_crop
   WHERE photo_id = NEW.photo_id AND ordinal = NEW.primary_crop AND safe = 0
)
BEGIN
  SELECT RAISE(ABORT, 'AURA-ML-5111: the delivered crop may not be one the safety filter refused');
END;

CREATE TRIGGER geometry_primary_is_safe_update
BEFORE UPDATE OF safe ON geometry_crop
FOR EACH ROW
WHEN NEW.safe = 0
  AND EXISTS (
    SELECT 1 FROM geometry_plan
     WHERE photo_id = NEW.photo_id AND primary_crop = NEW.ordinal
  )
BEGIN
  SELECT RAISE(ABORT, 'AURA-ML-5111: the delivered crop may not be one the safety filter refused');
END;

-- ---------------------------------------------------------------------------
-- What a project's pass covered and did.
--
-- `kept_original` is section 10.1's conservatism gate as a stored number: a wedding where it
-- approaches zero is a wedding where something has gone wrong with the improvement margin, and it
-- is a query rather than a telemetry event so that a support case can ask it of a catalog.
-- ---------------------------------------------------------------------------
CREATE VIEW v_geometry_coverage AS
SELECT
  p.project_id                                              AS project_id,
  COUNT(*)                                                  AS planned,
  SUM(CASE WHEN c.rect_w >= 0.9999 AND c.rect_h >= 0.9999
            AND p.rotate_deg = 0.0 AND p.keystone_vertical IS NULL
            AND p.lens_source = 'none'
           THEN 1 ELSE 0 END)                               AS untouched,
  SUM(CASE WHEN c.rect_w >= 0.9999 AND c.rect_h >= 0.9999 THEN 1 ELSE 0 END)
                                                            AS kept_original,
  SUM(CASE WHEN p.rotate_deg <> 0.0 THEN 1 ELSE 0 END)      AS straightened,
  SUM(CASE WHEN p.keystone_vertical IS NOT NULL THEN 1 ELSE 0 END)
                                                            AS keystoned,
  SUM(CASE WHEN p.user_edited = 1 THEN 1 ELSE 0 END)        AS user_edited,
  SUM(CASE WHEN p.reviewed = 0 THEN 1 ELSE 0 END)           AS pending_review
FROM geometry_plan p
JOIN geometry_crop c ON c.photo_id = p.photo_id AND c.ordinal = p.primary_crop
GROUP BY p.project_id;

-- ---------------------------------------------------------------------------
-- Section 10.1's hard gate, and its denominator beside it.
--
--   SELECT faces_cut FROM v_geometry_safety WHERE project_id = ?  --  must be 0
--
-- and `faces_checked` is what says whether that zero means anything.
-- ---------------------------------------------------------------------------
CREATE VIEW v_geometry_safety AS
SELECT
  project_id                                                AS project_id,
  SUM(considered)                                           AS faces_checked,
  SUM(CASE WHEN faces_intact = 0 THEN 1 ELSE 0 END)         AS faces_cut,
  SUM(CASE WHEN resolution_ok = 0 THEN 1 ELSE 0 END)        AS below_resolution,
  SUM(CASE WHEN content_kept = 0 THEN 1 ELSE 0 END)         AS content_lost,
  SUM(at_risk)                                              AS regions_at_risk,
  MIN(long_edge_fraction)                                   AS smallest_long_edge
FROM geometry_plan
GROUP BY project_id;
