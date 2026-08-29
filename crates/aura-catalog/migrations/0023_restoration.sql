-- Migration 23 - what was repaired in a photograph, and the distance a face was allowed to move.
--
-- PHASE-22 section 4 names no table: its file list is eight decision modules, four Python
-- scripts, a directory of camera noise models, two shaders, a panel and two model cards. It
-- needs two tables, two views and one trigger anyway, and section 6.3's last sentence is why -
-- "This is the guarantee that the product never changes what someone looks like". A guarantee
-- that lives only in a solver is a guarantee until somebody writes a second caller, and a
-- guarantee nobody can query is a guarantee nobody can find out they have lost.
-- `docs/adr/ADR-0047-restoration-denoise-sharpen-and-identity.md` records the rest.
--
-- ---------------------------------------------------------------------------
-- WHAT THIS MIGRATION IS FOR
-- ---------------------------------------------------------------------------
--
-- Twenty-one migrations recorded what AURA found, what it decided, how the pixels should move
-- and what was done to a person's appearance. This one records **what was repaired**, and it is
-- the first schema in the product whose central column is a measurement of how far something was
-- allowed to change rather than of what it was changed to. Seven properties are enforced here
-- rather than remembered:
--
-- 1. **A delivered face never moved further than the ceiling, and it is one query.**
--    `restore_face.identity_drift` is stored on every row that reached a render, whether the
--    recovery stood or was refused, with a CHECK that a row which was *not* skipped may not
--    carry a drift above `MAX_IDENTITY_DRIFT`. Section 10.1 gates identity preservation at
--    100 %, and the gate is then
--    `SELECT MAX(identity_drift) FROM restore_face WHERE skipped = 0` over a wedding.
--
-- 2. **A refusal is a row rather than an absence.** A face that was skipped keeps its measured
--    distance, its strength of zero and the code that explains it. Phase 17 established that a
--    rejection is written rather than dropped when the failure *is* the evidence; here the
--    refusal is the product working, and a schema that recorded only successes would make
--    "AURA declined to change what somebody looks like" unprovable.
--
-- 3. **The tier and the numbers it became are both stored.** `denoise_tier` is the decision and
--    `denoise_luminance`, `denoise_colour`, `denoise_detail` and `denoise_sigma` are what it
--    became under this frame's camera at this ISO. The same `standard` on two bodies is two
--    different renders, so a row carrying only the tier is a row phase 27 cannot audit. A CHECK
--    stops a row naming a tier with no numbers, and one stops a row carrying numbers with no
--    tier.
--
-- 4. **The self-check's two measurements are stored with the count they were taken over.**
--    `texture_retention` and `ringing` are bounded, and a row may not sit outside its bound
--    while claiming the operation ran. `measured_on` is the sample count, because a ratio over
--    eleven samples is arithmetic rather than evidence - phase 21's rule.
--
-- 5. **A sharpened frame had regions.** `sharpen_amount > 0` requires `region_covered = 1`, as a
--    CHECK. ADR-0047 section 4: an unmasked global sharpen spends its whole artefact budget on
--    skin, sky and bokeh, so this phase refuses rather than reduces. The database says so too.
--
-- 6. **There is no image data and nowhere to put a scale.** Phase 13's rule, ninth migration
--    running. A restoration is four numbers, a kernel width and a face box; there is no patch, no
--    donor pixel, no output scale and no synthesised region. Section 2.2 puts upscaling and
--    generative reconstruction out of scope, and a schema with no column for either makes adding
--    one a visible contract change.
--
-- 7. **A photographer's tier is never overwritten.** `restore_plan.user_edited` is checked
--    inside the statement a re-analysis would overwrite the row with - the eleventh time this
--    rule has been written into a migration.
--
-- ROLLBACK. Every object here is new and nothing outside this file references it:
--
--   DROP VIEW    IF EXISTS v_restore_identity;
--   DROP VIEW    IF EXISTS v_restore_coverage;
--   DROP TRIGGER IF EXISTS restore_face_drift_disclosed;
--   DROP TABLE   IF EXISTS restore_face;
--   DROP TABLE   IF EXISTS restore_plan;
--   DELETE FROM schema_version WHERE version = 22;
--
-- Running those returns the catalog to schema 21. **It is recomputable with one exception**: every
-- plan is derived from pixels, phase 09's verdicts, phase 06's faces and phase 18's regions, so a
-- re-run reproduces them exactly - but `restore_plan` rows with `user_edited = 1` are not
-- derivable from anything. The rollback runbook says to export those rows first.

-- ---------------------------------------------------------------------------
-- One row per photograph: what was repaired, and what the self-check measured.
-- ---------------------------------------------------------------------------
CREATE TABLE restore_plan (
  photo_id            TEXT    NOT NULL PRIMARY KEY REFERENCES photo(photo_id) ON DELETE CASCADE,
  project_id          TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,

  -- The scene it was decided under. Invariant 7, and stored rather than re-read: a
  -- re-classification changes what the plan should have been, and a plan that silently starts
  -- describing itself under a new scene is a plan nobody can audit.
  scene               TEXT    NOT NULL,

  -- See note 3. The decision, then the numbers it became.
  denoise_tier        TEXT    NOT NULL DEFAULT 'off'
                      CHECK (denoise_tier IN ('off','light','standard','strong')),
  denoise_luminance   REAL    NOT NULL DEFAULT 0.0 CHECK (denoise_luminance >= 0.0 AND denoise_luminance <= 1.0),
  denoise_colour      REAL    NOT NULL DEFAULT 0.0 CHECK (denoise_colour >= 0.0 AND denoise_colour <= 1.0),
  denoise_detail      REAL    NOT NULL DEFAULT 0.0 CHECK (denoise_detail >= 0.0 AND denoise_detail <= 1.0),
  -- The predicted sensor sigma the tier was chosen from, in linear working-space units.
  denoise_sigma       REAL    NOT NULL DEFAULT 0.0 CHECK (denoise_sigma >= 0.0 AND denoise_sigma <= 1.0),
  -- Which body's noise model conditioned it, and whether that model was ever measured. The
  -- second is the column that closes when a photographed reference arrives: on this build every
  -- row carries 0, and an unmeasured model caps the tier at 'standard'.
  denoise_camera      TEXT    NOT NULL DEFAULT '',
  denoise_measured    INTEGER NOT NULL DEFAULT 0 CHECK (denoise_measured IN (0,1)),

  -- The deconvolution. Zero amount means it did not run, and the kernel is then meaningless
  -- rather than zero - `sharpen_kernel` is NULL in that case, because a zero kernel would read
  -- as "perfectly sharp" and a photographer filtering on it would get the frames that were never
  -- measured.
  sharpen_kernel      REAL             CHECK (sharpen_kernel IS NULL OR (sharpen_kernel >= 0.0 AND sharpen_kernel <= 8.0)),
  sharpen_amount      REAL    NOT NULL DEFAULT 0.0 CHECK (sharpen_amount >= 0.0 AND sharpen_amount <= 0.50),
  sharpen_skin_atten  REAL    NOT NULL DEFAULT 0.0 CHECK (sharpen_skin_atten >= 0.0 AND sharpen_skin_atten <= 1.0),
  sharpen_coverage    REAL    NOT NULL DEFAULT 0.0 CHECK (sharpen_coverage >= 0.0 AND sharpen_coverage <= 1.0),
  sharpen_iterations  INTEGER NOT NULL DEFAULT 0 CHECK (sharpen_iterations >= 0 AND sharpen_iterations <= 3),

  -- The plan-wide face-recovery strength that survived. Capped at 0.40 by the contract and by
  -- section 5's own comment; written out here because SQLite has no constants.
  face_recovery       REAL    NOT NULL DEFAULT 0.0 CHECK (face_recovery >= 0.0 AND face_recovery <= 0.40),

  -- See note 4. The guarantee, as two numbers and a sample count.
  texture_retention   REAL    NOT NULL DEFAULT 1.0 CHECK (texture_retention >= 0.0 AND texture_retention <= 4.0),
  ringing             REAL    NOT NULL DEFAULT 0.0 CHECK (ringing >= 0.0 AND ringing <= 1.0),
  measured_on         INTEGER NOT NULL DEFAULT 0 CHECK (measured_on >= 0),
  -- How much strength the self-check gave up to get there, across the three operations.
  resolves            INTEGER NOT NULL DEFAULT 0 CHECK (resolves >= 0 AND resolves <= 9),
  denoise_reduced     INTEGER NOT NULL DEFAULT 0 CHECK (denoise_reduced IN (0,1)),
  sharpen_reduced     INTEGER NOT NULL DEFAULT 0 CHECK (sharpen_reduced IN (0,1)),

  -- Where and when the heavy pixels were pushed. `run_where` has a 'cloud' spelling that nothing
  -- in this build writes: ADR-0047 section 7 keeps the variant because section 5 freezes it, and
  -- keeps the code path absent because section 7 of the phase document says the Cloud AI Gateway
  -- stays idle here.
  run_where           TEXT    NOT NULL DEFAULT 'local_cpu'
                      CHECK (run_where IN ('local_gpu','local_cpu','cloud')),
  run_when            TEXT    NOT NULL DEFAULT 'export'
                      CHECK (run_when IN ('export','background')),

  -- True when phase 18 supplied at least one usable region for this frame. Zero on a build with
  -- no mask generator wired in, which is the honest reading of such a build.
  region_covered      INTEGER NOT NULL DEFAULT 0 CHECK (region_covered IN (0,1)),

  confidence          REAL    NOT NULL DEFAULT 0.0 CHECK (confidence >= 0.0 AND confidence <= 1.0),
  -- Reason codes, comma-separated slugs from `RestoreCode::as_str`, in emission order. Codes
  -- rather than sentences: phase 09's rule, ninth migration running.
  reasons             TEXT    NOT NULL DEFAULT '',

  user_edited         INTEGER NOT NULL DEFAULT 0 CHECK (user_edited IN (0,1)),
  reviewed            INTEGER NOT NULL DEFAULT 0 CHECK (reviewed IN (0,1)),

  model_ver           INTEGER NOT NULL DEFAULT 0,
  analysis_ver        INTEGER NOT NULL DEFAULT 0,
  profile_ver         INTEGER NOT NULL DEFAULT 0,
  planned_at          TEXT    NOT NULL,

  -- See note 3. A tier names numbers, and numbers name a tier.
  CHECK (denoise_tier <> 'off' OR (denoise_luminance = 0.0 AND denoise_colour = 0.0)),
  CHECK (denoise_tier =  'off' OR denoise_colour > 0.0 OR denoise_luminance > 0.0),
  -- Chroma reduction is never below luminance reduction. Section 6.1: chroma noise carries no
  -- detail anybody wants and luminance noise is half a stop from being grain, so a row that
  -- inverted the two would be smearing fabric in order to remove grain.
  CHECK (denoise_colour >= denoise_luminance),
  -- An unmeasured noise model may not have produced the strongest tier. ADR-0047 section 3.
  CHECK (denoise_measured = 1 OR denoise_tier <> 'strong'),

  -- See note 5. A sharpened frame had regions, and a kernel, and iterations.
  CHECK (sharpen_amount = 0.0 OR region_covered = 1),
  CHECK (sharpen_amount = 0.0 OR sharpen_kernel IS NOT NULL),
  CHECK (sharpen_amount = 0.0 OR sharpen_iterations > 0),
  CHECK (sharpen_amount = 0.0 OR sharpen_skin_atten >= 0.80),

  -- See note 4. A row may not sit outside a bound while claiming the operation ran.
  CHECK (texture_retention >= 0.90 - 0.0001 OR denoise_tier = 'off'),
  CHECK (ringing <= 0.020 + 0.000001 OR sharpen_amount = 0.0),
  -- A plan that changed pixels measured something.
  CHECK (measured_on > 0
      OR (denoise_tier = 'off' AND sharpen_amount = 0.0 AND face_recovery = 0.0))
);

-- The review queue: weakest first, and only frames nobody has settled.
CREATE INDEX idx_restore_review ON restore_plan(project_id, confidence)
  WHERE reviewed = 0 AND user_edited = 0;

-- The resumable pass, which is a query rather than a journal: every frame in this project whose
-- versions do not match the running build is pending by definition.
CREATE INDEX idx_restore_versions ON restore_plan(project_id, model_ver, analysis_ver, profile_ver);

-- Phase 25's query: a gallery denoised at four different tiers has one noisy frame in it, and
-- this is how it is found.
CREATE INDEX idx_restore_tier ON restore_plan(project_id, denoise_tier);

-- Phase 27's query: which frames were repaired at all, and how hard the self-check had to work.
CREATE INDEX idx_restore_guarded ON restore_plan(project_id, ringing)
  WHERE sharpen_amount > 0.0;

-- The condition that closes when a photographed reference arrives: which bodies in this wedding
-- were denoised against a synthetic noise model.
CREATE INDEX idx_restore_unmeasured ON restore_plan(project_id, denoise_camera)
  WHERE denoise_measured = 0;

-- ---------------------------------------------------------------------------
-- One row per face this phase considered. See notes 1, 2 and 6.
-- ---------------------------------------------------------------------------
CREATE TABLE restore_face (
  photo_id            TEXT    NOT NULL REFERENCES restore_plan(photo_id) ON DELETE CASCADE,
  seq                 INTEGER NOT NULL CHECK (seq >= 0 AND seq < 16),

  -- Whose face, when phase 06 has assigned one. NULL is an unassigned face rather than a missing
  -- one: this phase acts on a face box whether or not it belongs to a known identity.
  identity_id         TEXT             REFERENCES identities(id) ON DELETE SET NULL,

  -- Where it is, in frame coordinates. Four columns rather than a blob, because phase 27 filters
  -- on size and a blob is not a predicate.
  x                   REAL    NOT NULL CHECK (x >= 0.0 AND x <= 1.0),
  y                   REAL    NOT NULL CHECK (y >= 0.0 AND y <= 1.0),
  w                   REAL    NOT NULL CHECK (w > 0.0 AND w <= 1.0),
  h                   REAL    NOT NULL CHECK (h > 0.0 AND h <= 1.0),

  -- The measured sharpness that decided whether this face was inside the narrow band section 6.3
  -- requires. Stored because "why was this face not recovered" is answered by it and by nothing
  -- else.
  sharpness           REAL    NOT NULL DEFAULT 0.0 CHECK (sharpness >= 0.0 AND sharpness <= 1.0),

  -- What survived. Zero on a skipped face, by CHECK.
  strength            REAL    NOT NULL DEFAULT 0.0 CHECK (strength >= 0.0 AND strength <= 0.40),

  -- See note 1. **The guarantee.** How far the phase 06 embedding moved between the render
  -- before and the render after, as a cosine distance.
  identity_drift      REAL    NOT NULL DEFAULT 0.0 CHECK (identity_drift >= 0.0 AND identity_drift <= 1.0),
  resolves            INTEGER NOT NULL DEFAULT 0 CHECK (resolves >= 0 AND resolves <= 3),

  -- See note 2. A refusal is a row.
  skipped             INTEGER NOT NULL DEFAULT 0 CHECK (skipped IN (0,1)),
  -- A `RestoreCode` slug, for a skipped face. NULL otherwise.
  skipped_because     TEXT,

  PRIMARY KEY (photo_id, seq),

  -- See note 1. **The one CHECK this whole phase exists for.** A face that was delivered may not
  -- carry a drift above `MAX_IDENTITY_DRIFT`. The number is written out rather than referenced
  -- because SQLite has no constants, and it is `MAX_IDENTITY_DRIFT` in
  -- `aura_core::contract::restore`.
  CHECK (skipped = 1 OR identity_drift <= 0.08 + 0.000001),
  -- A skipped face carries no strength and does carry a reason.
  CHECK (skipped = 0 OR strength = 0.0),
  CHECK (skipped = 0 OR skipped_because IS NOT NULL),
  CHECK (skipped = 1 OR skipped_because IS NULL)
) WITHOUT ROWID;

-- Phase 27's query: every face in this wedding whose recovery was refused, worst first.
CREATE INDEX idx_restore_face_refused ON restore_face(skipped_because, identity_drift)
  WHERE skipped = 1;

-- See note 1. The promise, enforced by the database rather than by the application.
--
-- There is no code path in `aura-restore` that attempts this, which is the point: it catches the
-- path somebody adds in phase 24 or phase 28 without reading ADR-0047. An UPDATE that un-skips a
-- face without also bringing its drift inside the ceiling is aborted rather than obeyed - which
-- is the exact shape of the change a well-meaning "recover this one anyway" button would make.
CREATE TRIGGER restore_face_drift_disclosed
BEFORE UPDATE OF skipped, identity_drift ON restore_face
WHEN NEW.skipped = 0 AND NEW.identity_drift > 0.08 + 0.000001
BEGIN
  SELECT RAISE(ABORT, 'a recovered face may never be delivered past the identity ceiling: AURA does not change what somebody looks like');
END;

-- ---------------------------------------------------------------------------
-- Coverage, in the shape every outline since phase 09 reports it.
-- ---------------------------------------------------------------------------
CREATE VIEW v_restore_coverage AS
SELECT
  p.project_id                                                    AS project_id,
  COUNT(*)                                                        AS photos,
  SUM(CASE WHEN r.photo_id IS NOT NULL THEN 1 ELSE 0 END)         AS planned,
  SUM(CASE WHEN r.denoise_tier <> 'off'
             OR r.sharpen_amount > 0.0
             OR r.face_recovery > 0.0 THEN 1 ELSE 0 END)          AS acted_on,
  SUM(CASE WHEN r.region_covered = 1 THEN 1 ELSE 0 END)           AS region_covered,
  SUM(CASE WHEN r.denoise_tier = 'off'      THEN 1 ELSE 0 END)    AS tier_off,
  SUM(CASE WHEN r.denoise_tier = 'light'    THEN 1 ELSE 0 END)    AS tier_light,
  SUM(CASE WHEN r.denoise_tier = 'standard' THEN 1 ELSE 0 END)    AS tier_standard,
  SUM(CASE WHEN r.denoise_tier = 'strong'   THEN 1 ELSE 0 END)    AS tier_strong,
  SUM(CASE WHEN r.sharpen_amount > 0.0 THEN 1 ELSE 0 END)         AS sharpened,
  SUM(CASE WHEN r.resolves > 0 THEN 1 ELSE 0 END)                 AS reduced,
  SUM(CASE WHEN r.denoise_measured = 0 AND r.denoise_tier <> 'off' THEN 1 ELSE 0 END)
                                                                  AS unmeasured_camera,
  AVG(CASE WHEN r.denoise_tier <> 'off' THEN r.texture_retention END) AS mean_texture_retention,
  AVG(CASE WHEN r.sharpen_amount > 0.0  THEN r.ringing END)           AS mean_ringing
FROM photo p
LEFT JOIN restore_plan r ON r.photo_id = p.photo_id
GROUP BY p.project_id;

-- ---------------------------------------------------------------------------
-- See note 1. The guarantee, as a view, because it is asked by four callers: the panel, the
-- delivery report, phase 27's QC agent and the phase gate. A number four callers derive
-- separately is a number they will eventually disagree about.
-- ---------------------------------------------------------------------------
CREATE VIEW v_restore_identity AS
SELECT
  pl.project_id                                                   AS project_id,
  COUNT(*)                                                        AS faces,
  SUM(CASE WHEN f.skipped = 0 THEN 1 ELSE 0 END)                  AS recovered,
  SUM(CASE WHEN f.skipped_because = 'restore_identity_drift_skipped' THEN 1 ELSE 0 END)
                                                                  AS refused_for_identity,
  -- **The gate.** Section 10.1's "below threshold on 100 % of fixtures", as one number.
  MAX(CASE WHEN f.skipped = 0 THEN f.identity_drift ELSE 0.0 END) AS worst_kept_drift,
  AVG(CASE WHEN f.skipped = 0 THEN f.identity_drift END)          AS mean_kept_drift
FROM restore_face f
JOIN restore_plan pl ON pl.photo_id = f.photo_id
GROUP BY pl.project_id;
