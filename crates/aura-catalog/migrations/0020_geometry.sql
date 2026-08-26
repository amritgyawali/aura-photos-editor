-- Migration 20 - how one photograph's frame was finished.
--
-- PHASE-23 section 4 names no table: its file list is eight decision modules, a bundled
-- profile directory, a shader, a rules file and a panel. It needs two tables anyway, and
-- section 2.1's last bullet is why - "all geometry recorded in the recipe, fully reversible,
-- with the original framing always recoverable". A recipe is reversible; it is not
-- *inspectable*, because a recipe says the frame is cropped to [0.08, 0.05, 0.94, 0.97] and
-- says nothing about the three hundred rectangles that were refused to arrive at it, which of
-- them cut a face, or the score it had to beat.
-- `docs/adr/ADR-0041-geometry-lens-straightening-and-crop-safety.md` records the rest.
--
-- ---------------------------------------------------------------------------
-- WHAT THIS MIGRATION IS FOR
-- ---------------------------------------------------------------------------
--
-- Nineteen migrations have recorded what AURA found, what it decided, why the pixels should
-- move, what would happen to them and what it did locally. This one records **what was
-- removed from a photograph, and everything it refused to remove** - and as with phase 19 the
-- second half is the larger one. Eleven of the twenty-four reason codes in this phase describe
-- a refusal, and on a well-shot wedding they are the only codes most frames will ever carry.
--
-- Eight properties are enforced here rather than remembered:
--
-- 1. **The original framing is always recoverable, and it is not stored.** Ordinal zero -
--    `purpose = 'original'` - is a pure function of two columns already on the plan row: the
--    whole frame when `rotate_deg` is zero, and the rectangle that rotation left inside it
--    when it is not. `GeometryStore` regenerates it on every read and never writes it.
--
--    That is stronger than storing it, not weaker. A stored row is a row somebody can delete;
--    a derived one cannot be lost, cannot drift from the rotation it belongs to, and cannot
--    be edited into something that is not the frame as shot. Section 13's "original framing is
--    always one click away" becomes a property of the reader. It also saves about ninety bytes
--    a photograph for a rectangle that could not have been anything else - the same argument
--    phase 19 made about regenerating shaping zones from the four numbers they derive from.
--
--    `primary_ordinal = 0` therefore means "deliver the frame as shot", and it addresses a row
--    that is generated rather than selected.
--
-- 2. **There is no path, no rendered output and no `applied` flag.** Phase 14's rule for the
--    seventh phase running: the values reach the pixels through `edit_recipes` and
--    `aura_recipe::schema::merge` only. This table is the record of what the product thought,
--    kept beside whatever the photographer chose. A row with `user_edited = 1` still carries
--    AURA's own rectangle, which is what makes the panel able to show a disagreement and phase
--    30's learning loop able to read one.
--
-- 3. **There is no image data anywhere, including in the evidence.** Phase 13's rule. A
--    reason's evidence is a rectangle - the face that would have been cut - and there is no
--    column here that could hold a pixel. That is what makes "a support bundle contains no
--    pixels" a property of the shape rather than a promise about the exporter.
--
-- 4. **`user_edited` is checked inside the statement, not before it.** The ninth migration to
--    write this rule and the same window it closes every time: a re-analysis that read the
--    row, decided, and then wrote would lose an override set in between. Here it matters more
--    than usual, because the thing being overwritten is somebody's own crop.
--
-- 5. **Three version columns, because they invalidate three different things.**
--    `profile_ver` invalidates the lens corrections - a new bundled profile changes the
--    distortion, the vignette and the fringing. `analysis_ver` invalidates the rotation, the
--    keystone and every crop, because all three come out of one search. `rules_ver`
--    invalidates every safety margin those rectangles were checked against: a crop that
--    passed under a 60 % resolution floor has not passed under a 70 % one. `AURA-ML-5090` is
--    raised when a comparison would cross any of the three.
--
-- 6. **A refused candidate is a count, not a row.** Four counters on the plan rather than a
--    child table: a wedding's crop search refuses on the order of two hundred rectangles per
--    photograph, which is 800,000 rows on a 4,000-image wedding for information nobody
--    queries across. What a photographer needs is "AURA tried and could not, because of
--    faces" and that is a number. The rectangle that caused it is carried by the reason's
--    evidence for the ones worth showing. This is the same argument phase 19 made about
--    shaping zones, taken to its natural end.
--
-- 6b. **A reason stores its code, and its sentence only when the sentence carries a number.**
--    Phase 09's rule, for the sixth migration running: a stored sentence is copy a release can
--    change, and a catalog full of English cannot be translated. `GeometryCode::user_text`
--    regenerates it on read. Four of this phase's twenty-four reasons carry a *measured* value
--    inside the sentence - "scored 0.71 against 0.63", "levelled 4.2 degrees of the 7.0 it
--    needed" - which no static string can reproduce, and only those rows store their text.
--    Measured: 1,474 bytes per image with every sentence stored, 999 without, 839 once ordinal
--    zero stopped being written.
--
-- 7. **`faces_checked` and `hands_checked` are counts rather than booleans.** A crop over a
--    frame with no detected faces satisfies the face rule trivially, and storing that as
--    `faces_intact = 1` alone would make a build with no face detector look like a build whose
--    crops are provably safe. Phase 09's rule about denominators, applied to a guarantee.
--    **On this build `hands_checked` is zero on every row in the product**, because there is
--    no pose estimate; the exit report says so.
--
-- 8. **`lens_id` is stored whether or not it matched.** Section 11's
--    `geometry.lens_profile_missing {lens_id}` is only answerable as a histogram if the lens
--    is on the row in both cases, and the monthly profile expansion task section 12 asks for
--    needs to know which unprofiled lenses are actually being shot on.
--
-- ---------------------------------------------------------------------------
-- ROLLBACK
-- ---------------------------------------------------------------------------
--
-- `DROP VIEW v_geometry_coverage; DROP TABLE geometry_crop; DROP TABLE geometry_plan;` and set
-- `user_version = 19`. Nothing else references these tables, and nothing in a recipe depends
-- on them: a catalog rolled back to 19 renders every photograph exactly as it did before this
-- phase, because the frame as shot is what `recipe.geometry` defaults to.

-- ---------------------------------------------------------------------------
-- One row per photograph with a plan.
-- ---------------------------------------------------------------------------
CREATE TABLE geometry_plan (
  photo_id            TEXT    PRIMARY KEY REFERENCES photo(photo_id) ON DELETE CASCADE,

  -- Which scene the bands were conditioned on. Invariant 7. `SceneId::as_str`.
  scene               TEXT    NOT NULL DEFAULT 'unknown',
  -- 0 when the scene had no row in `crop_rules.toml` and the conservative fallback was used.
  -- `AURA-ML-5094`, and the numerator of `GeometryOutline::unpolicied_scenes`.
  rules_row           INTEGER NOT NULL DEFAULT 1 CHECK (rules_row IN (0, 1)),
  -- Width over height of the frame as shot. Needed to re-check a stored rectangle against a
  -- resolution floor without decoding the photograph.
  frame_aspect        REAL    NOT NULL DEFAULT 1.5 CHECK (frame_aspect > 0.0),

  -- --- the optics --------------------------------------------------------------------
  -- `LensSource::as_str`: none, embedded, profile, estimated.
  lens_source         TEXT    NOT NULL DEFAULT 'none',
  -- The lens as EXIF named it, kept whether or not it matched. See note 8.
  lens_id             TEXT,
  -- The profile that produced the numbers, when one did.
  lens_profile        TEXT,
  -- Brown-Conrady radial terms in normalised radius, where one is the corner.
  k1                  REAL    NOT NULL DEFAULT 0.0,
  k2                  REAL    NOT NULL DEFAULT 0.0,
  k3                  REAL    NOT NULL DEFAULT 0.0,
  -- Vignette correction strength as a FRACTION, not the recipe's 0..100 integer. This is the
  -- decision; the recipe is the instruction.
  vignette            REAL    NOT NULL DEFAULT 0.0 CHECK (vignette >= 0.0 AND vignette <= 1.0),
  -- Per-channel radial scale relative to green. Green is never scaled: it is the channel the
  -- sensor has twice as many of and the one a focus system was aimed with.
  ca_red              REAL    NOT NULL DEFAULT 1.0,
  ca_blue             REAL    NOT NULL DEFAULT 1.0,

  -- --- levelling ---------------------------------------------------------------------
  -- Positive clockwise. Zero when nothing was levelled. The CHECK is the contract's band
  -- restated: outside it a tilt is a decision, and this schema will not store a photograph
  -- turned onto its side.
  rotate_deg          REAL    NOT NULL DEFAULT 0.0
                              CHECK (rotate_deg > -45.0 AND rotate_deg < 45.0),
  rotate_conf         REAL    NOT NULL DEFAULT 0.0
                              CHECK (rotate_conf >= 0.0 AND rotate_conf <= 1.0),

  -- --- the keystone ------------------------------------------------------------------
  -- NULL when no correction survived the cap. All four move together or not at all.
  keystone_v          REAL,
  keystone_h          REAL,
  keystone_scale      REAL,
  -- Bounded by the contract's MAX_STRETCH at construction, and again here. A stored
  -- correction that survived a cap that has since tightened is re-checked on read; a stored
  -- correction that never met the cap it was written under is a bug, and this is where it
  -- fails loudly.
  keystone_stretch    REAL    CHECK (keystone_stretch IS NULL
                                     OR (keystone_stretch >= 1.0 AND keystone_stretch <= 1.25)),
  keystone_verticals  INTEGER NOT NULL DEFAULT 0 CHECK (keystone_verticals >= 0),

  -- --- the crop ----------------------------------------------------------------------
  -- Which `geometry_crop.ordinal` is delivered. Zero is the original, which always exists.
  primary_ordinal     INTEGER NOT NULL DEFAULT 0 CHECK (primary_ordinal >= 0),

  -- --- the safety report -------------------------------------------------------------
  faces_intact        INTEGER NOT NULL DEFAULT 1 CHECK (faces_intact IN (0, 1)),
  resolution_ok       INTEGER NOT NULL DEFAULT 1 CHECK (resolution_ok IN (0, 1)),
  content_kept        INTEGER NOT NULL DEFAULT 1 CHECK (content_kept IN (0, 1)),
  -- See note 7. Zero is "nothing was checked", never "nothing was cut".
  faces_checked       INTEGER NOT NULL DEFAULT 0 CHECK (faces_checked >= 0),
  hands_checked       INTEGER NOT NULL DEFAULT 0 CHECK (hands_checked >= 0),
  -- See note 6. `GeometryCode::REFUSALS` order: face, hands, resolution, content.
  refused_face        INTEGER NOT NULL DEFAULT 0 CHECK (refused_face >= 0),
  refused_hands       INTEGER NOT NULL DEFAULT 0 CHECK (refused_hands >= 0),
  refused_small       INTEGER NOT NULL DEFAULT 0 CHECK (refused_small >= 0),
  refused_content     INTEGER NOT NULL DEFAULT 0 CHECK (refused_content >= 0),

  -- --- the explanation ---------------------------------------------------------------
  -- `[{"code": "...", "text": "...", "weight": 0.0, "evidence": {...}}]`. Codes rather than
  -- sentences would be smaller and would make the panel's text a release-time string; the
  -- text is stored because a reason carries a measured number inside it ("scored 0.71 against
  -- 0.63") that no static string can. Phase 09's rule read in the direction it points here.
  reasons             TEXT    NOT NULL DEFAULT '[]',
  confidence          REAL    NOT NULL DEFAULT 0.0
                              CHECK (confidence >= 0.0 AND confidence <= 1.0),

  -- --- versions and provenance -------------------------------------------------------
  profile_ver         INTEGER NOT NULL DEFAULT 0,
  analysis_ver        INTEGER NOT NULL DEFAULT 0,
  rules_ver           INTEGER NOT NULL DEFAULT 0,
  -- See note 4. Set by `GeometryService::set_framing` and never cleared by a re-analysis.
  user_edited         INTEGER NOT NULL DEFAULT 0 CHECK (user_edited IN (0, 1)),
  reviewed            INTEGER NOT NULL DEFAULT 0 CHECK (reviewed IN (0, 1)),
  planned_at          TEXT    NOT NULL
) STRICT;

-- The re-plan query: every row whose versions have moved, `user_edited` first so the
-- statement that skips them does not scan.
CREATE INDEX idx_geometry_versions
  ON geometry_plan(user_edited, profile_ver, analysis_ver, rules_ver);

-- Section 11's `geometry.lens_profile_missing {lens_id}` is a GROUP BY on this.
CREATE INDEX idx_geometry_lens ON geometry_plan(lens_id) WHERE lens_source = 'none';

-- The review queue: weakest plans first, skipping what a photographer has already settled.
CREATE INDEX idx_geometry_review
  ON geometry_plan(confidence) WHERE reviewed = 0 AND user_edited = 0;

-- ---------------------------------------------------------------------------
-- One row per crop variant. Ordinal zero is always the original. See note 1.
-- ---------------------------------------------------------------------------
CREATE TABLE geometry_crop (
  photo_id            TEXT    NOT NULL REFERENCES geometry_plan(photo_id) ON DELETE CASCADE,
  -- Position in `GeometryPlan::crops`. Zero is the original framing, always.
  ordinal             INTEGER NOT NULL CHECK (ordinal >= 0 AND ordinal < 6),

  -- `CropPurpose::as_str`: original, primary, album, social, wide.
  purpose             TEXT    NOT NULL,
  -- `Aspect::as_str`: original, 4:5, 5:4, 1:1, 16:9.
  aspect              TEXT    NOT NULL DEFAULT 'original',

  -- Normalised to the CORRECTED frame - after the lens model and the keystone, not the file.
  -- A rectangle expressed against the raw frame would drift by however much the optics bent,
  -- which on a 14 mm wide is several per cent of the width.
  -- Ordinal zero is never written; see note 1. The rows here are the primary and the aspect
  -- variants, so a plan delivered as shot carries no crop rows at all.
  x                   REAL    NOT NULL CHECK (x >= 0.0 AND x <= 1.0),
  y                   REAL    NOT NULL CHECK (y >= 0.0 AND y <= 1.0),
  w                   REAL    NOT NULL CHECK (w > 0.0 AND w <= 1.0),
  h                   REAL    NOT NULL CHECK (h > 0.0 AND h <= 1.0),

  -- The objective's score, comparable between this row and ordinal zero by construction -
  -- which is what makes the improvement margin a margin rather than a threshold.
  score               REAL    NOT NULL DEFAULT 0.0 CHECK (score >= 0.0 AND score <= 1.0),
  -- Always 1 on a well-formed plan; the filter runs before the objective. Stored so a row
  -- written under an older `rules_ver` can be re-checked and marked unsafe without being
  -- deleted - a photographer who cropped it themselves still gets to see it.
  safe                INTEGER NOT NULL DEFAULT 1 CHECK (safe IN (0, 1)),

  PRIMARY KEY (photo_id, ordinal)
) STRICT, WITHOUT ROWID;

-- Phase 29's join: "give me every album variant in this wedding".
CREATE INDEX idx_geometry_crop_purpose ON geometry_crop(purpose);

-- ---------------------------------------------------------------------------
-- How much of a wedding has a plan, and how much of that plan left the frame alone.
--
-- The denominator is **every photograph in the project**, as phases 09 to 19's are. The
-- second column is the one that matters, and it is the only outline number in the product
-- where MORE restraint is the passing direction: section 10.1 asks that at least seventy per
-- cent of frames keep their original framing.
-- ---------------------------------------------------------------------------
CREATE VIEW v_geometry_coverage AS
SELECT
  p.project_id                                                  AS project_id,
  COUNT(*)                                                      AS images,
  COUNT(g.photo_id)                                             AS planned,
  SUM(CASE WHEN g.primary_ordinal = 0 THEN 1 ELSE 0 END)        AS kept_original,
  SUM(CASE WHEN g.lens_source IN ('embedded', 'profile')
           THEN 1 ELSE 0 END)                                   AS profile_covered,
  SUM(CASE WHEN g.rotate_deg <> 0.0 THEN 1 ELSE 0 END)          AS levelled,
  SUM(CASE WHEN g.keystone_v IS NOT NULL THEN 1 ELSE 0 END)     AS keystoned,
  SUM(CASE WHEN g.faces_checked > 0 THEN 1 ELSE 0 END)          AS face_checked,
  SUM(g.refused_face + g.refused_hands
      + g.refused_small + g.refused_content)                    AS refused_total,
  SUM(CASE WHEN g.user_edited = 1 THEN 1 ELSE 0 END)            AS user_edited,
  SUM(CASE WHEN g.photo_id IS NOT NULL
             AND g.reviewed = 0
             AND g.user_edited = 0
             AND g.confidence < 0.50 THEN 1 ELSE 0 END)         AS needs_review
FROM photo p
LEFT JOIN geometry_plan g ON g.photo_id = p.photo_id
GROUP BY p.project_id;
