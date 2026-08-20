-- Migration 16 - what a photograph should look like, and the proof that grading it did not
-- move anybody's skin.
--
-- PHASE-16 section 5 names one shape, `ColourDecision`, and section 4 names no file at all -
-- this phase's file table is entirely code. What it needs from the catalog is one row per
-- photograph, and `docs/adr/ADR-0033-tone-curves-hsl-and-skin-protection.md` records the
-- decisions behind the columns.
--
-- ---------------------------------------------------------------------------
-- WHAT THIS MIGRATION IS FOR
-- ---------------------------------------------------------------------------
--
-- Migration 15 recorded why the pixels should move. This one records **how far they should
-- move and what that did to the people in them** - the first migration in the product whose
-- rows include a measurement of the product's own behaviour.
--
-- Six properties are enforced here rather than remembered:
--
-- 1. **There is no skin target column, anywhere in this file.** Migration 15 said the same
--    thing about white balance and it is said again here, because this is the phase where a
--    "preferred skin hue" column would look most reasonable. The guarantee in section 6.3 is
--    measured against THIS FRAME'S OWN SKIN BEFORE THE GRADE - `skin_hue_shift` is a
--    *difference*, not a distance from an ideal - so there is nothing for such a column to
--    hold and nowhere to put one. The phase gate scans for one on every run.
--
-- 2. **The guarantee is a stored measurement, not a claim.** `skin_hue_shift`,
--    `skin_chroma_change`, `skin_attenuation` and `skin_resolves` are what the guard actually
--    measured on this frame's skin pixels after grading them through the real renderer. So
--    "skin never shifts measurably" is a query - `SELECT MAX(skin_hue_shift)` - rather than a
--    sentence in a document. A product that could only assert it would have no way to find
--    out it had stopped being true.
--
-- 3. **Nothing in this migration is an edit.** There is no recipe column, no applied flag and
--    no path. The values reach the pixels through `edit_recipes` and
--    `aura_recipe::schema::merge` only - phase 14's rule, for the second phase running - and
--    this table is the record of what the product thought, kept beside whatever the
--    photographer decided.
--
-- 4. **`user_edited` is checked inside the statement, not before it.** A re-analysis
--    overwrites every decision it recomputes; the upsert re-applies the photographer's own
--    override from the row it is replacing, in the same statement. Eighth phase, same rule,
--    same window closed.
--
-- 5. **Six JSON columns, and why they are not six tables.** `curve`, `hsl`, `bands`,
--    `alternatives`, `reasons` and `skin_guard` are always read together by the panel that
--    draws them and never queried across. The one thing a query does need - the worst skin
--    shift in a project - is denormalised into its own column for exactly that reason.
--
-- 6. **A photographer's override is one document rather than nine columns.** Phase 15 used
--    three columns because it had three scalars. This phase's override can carry a whole
--    curve and a whole eight-band block, and nine columns of which seven are usually NULL is
--    a row that costs more empty than full. `user_values` is NULL when nobody has touched the
--    frame, which is the common case and the cheapest representation of it.
--
-- ROLLBACK. Every object here is new and nothing outside this file references it:
--
--   DROP VIEW  IF EXISTS v_colour_coverage;
--   DROP TABLE IF EXISTS image_colour_decision;
--   DELETE FROM schema_version WHERE version = 16;
--
-- Running those returns the catalog to schema 15. **It is recomputable**: every row here is
-- derived from pixels, phase 06's faces, phase 07's scenes and phase 09's calibration, so a
-- re-run reproduces it exactly - with the usual exception. `user_values` is not derivable
-- from anything and the rollback runbook says to export it first. It also lives in
-- `edit_recipes` and in the sidecars beside the RAWs, which is the second copy that makes the
-- loss survivable.

-- ---------------------------------------------------------------------------
-- One row per photograph. How it should be graded, and what that did to skin.
-- ---------------------------------------------------------------------------
CREATE TABLE image_colour_decision (
  photo_id            TEXT    NOT NULL PRIMARY KEY REFERENCES photo(photo_id) ON DELETE CASCADE,
  project_id          TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,

  -- The five tone parameters, in the recipe's own units so a stored decision and a rendered
  -- one are the same numbers rather than two roundings of one number.
  contrast            REAL    NOT NULL CHECK (contrast   >= -100.0 AND contrast   <= 100.0),
  highlights          REAL    NOT NULL CHECK (highlights >= -100.0 AND highlights <= 100.0),
  shadows             REAL    NOT NULL CHECK (shadows    >= -100.0 AND shadows    <= 100.0),
  whites              REAL    NOT NULL CHECK (whites     >= -100.0 AND whites     <= 100.0),
  blacks              REAL    NOT NULL CHECK (blacks     >= -100.0 AND blacks     <= 100.0),
  vibrance            REAL    NOT NULL CHECK (vibrance   >= -100.0 AND vibrance   <= 100.0),
  saturation          REAL    NOT NULL CHECK (saturation >= -100.0 AND saturation <= 100.0),

  -- The point curve, as a flat array of alternating input and output levels. A curve whose
  -- points are not monotone reads as the identity - `aura_brain_photo::colour::codec` refuses
  -- it on the way out exactly as `ToneCurve::new` refuses it on the way in, so a row written
  -- by a future build cannot deliver a posterising curve to the renderer.
  curve               TEXT    NOT NULL DEFAULT '[0,0,255,255]',
  -- The eight bands, as a flat array of h, s, l triples in the frozen band order.
  hsl                 TEXT    NOT NULL DEFAULT '[]',
  -- What the content pass read: greenery, sky, dress, wood, decor and skin, with the
  -- confidence each was inferred at. Stored because a photographer asking "why did you change
  -- the green" needs the reading and not only the adjustment.
  bands               TEXT    NOT NULL DEFAULT '[]',
  -- Three complete alternative parameter sets. Complete rather than deltas: every one has
  -- been through the clipping guard and the skin guard, so promoting one is promoting
  -- something somebody checked. ADR-0033 decision 6.
  alternatives        TEXT    NOT NULL DEFAULT '[]',
  reasons             TEXT    NOT NULL DEFAULT '[]',
  -- The guard's own report: mask area, hue shift, chroma change, attenuation, re-solves and
  -- whether it was measured at all.
  skin_guard          TEXT    NOT NULL DEFAULT '[]',

  -- The guarantee, denormalised out of the document above so that the one query that
  -- matters - the worst skin movement in a project - is a MAX over a column rather than four
  -- thousand JSON parses. See note 2.
  skin_measured       INTEGER NOT NULL DEFAULT 0 CHECK (skin_measured IN (0,1)),
  skin_hue_shift      REAL    NOT NULL DEFAULT 0.0 CHECK (skin_hue_shift >= 0.0),
  skin_chroma_change  REAL    NOT NULL DEFAULT 0.0 CHECK (skin_chroma_change >= 0.0),
  skin_attenuation    REAL    NOT NULL DEFAULT 1.0
                        CHECK (skin_attenuation >= 0.0 AND skin_attenuation <= 1.0),
  skin_resolves       INTEGER NOT NULL DEFAULT 0 CHECK (skin_resolves >= 0),

  -- Clipping before and after, because section 10.1's gate is about what a frame GAINS. A
  -- sparkler that was blown before the grade is not the grade's fault, and storing only the
  -- after would make the two indistinguishable.
  clip_before_high    REAL    NOT NULL DEFAULT 0.0 CHECK (clip_before_high >= 0.0),
  clip_before_low     REAL    NOT NULL DEFAULT 0.0 CHECK (clip_before_low  >= 0.0),
  clip_after_high     REAL    NOT NULL DEFAULT 0.0 CHECK (clip_after_high  >= 0.0),
  clip_after_low      REAL    NOT NULL DEFAULT 0.0 CHECK (clip_after_low   >= 0.0),

  confidence          REAL    NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
  -- The subtlety metric: the root-mean-square of every normalised parameter this phase sets.
  -- Section 12's first failure mode is over-graded results, and this is the number that makes
  -- "professionally subtle" a query rather than an opinion.
  subtlety            REAL    NOT NULL DEFAULT 0.0 CHECK (subtlety >= 0.0),

  -- Which scene the intents came from. Invariant 7: a stored decision that does not say which
  -- scene it assumed is not reproducible.
  scene               TEXT    NOT NULL DEFAULT 'unknown',
  untargeted          INTEGER NOT NULL DEFAULT 0 CHECK (untargeted IN (0,1)),

  -- Three bits the outline and section 11's telemetry aggregate, denormalised for the same
  -- reason `skin_hue_shift` is.
  guard_triggered     INTEGER NOT NULL DEFAULT 0 CHECK (guard_triggered IN (0,1)),
  clip_resolved       INTEGER NOT NULL DEFAULT 0 CHECK (clip_resolved   IN (0,1)),
  subtlety_capped     INTEGER NOT NULL DEFAULT 0 CHECK (subtlety_capped IN (0,1)),

  -- Invariant 2, enforced by the schema rather than by review. A grade with no reason is a
  -- decision that cannot explain itself.
  reason_count        INTEGER NOT NULL DEFAULT 0 CHECK (reason_count >= 1 AND reason_count <= 6),

  -- The photographer's own answer, and the two bits that protect it. See notes 4 and 6.
  user_edited         INTEGER NOT NULL DEFAULT 0 CHECK (user_edited IN (0,1)),
  reviewed            INTEGER NOT NULL DEFAULT 0 CHECK (reviewed    IN (0,1)),
  user_values         TEXT,

  -- Three versions, because they invalidate three different things. `AURA-ML-5071` is raised
  -- when a comparison would cross any of them.
  model_ver           INTEGER NOT NULL,
  analysis_ver        INTEGER NOT NULL,
  intent_ver          INTEGER NOT NULL,

  at_ts               INTEGER NOT NULL,
  at                  TEXT    NOT NULL
) STRICT;

-- The pending set and the outline both scan one project. One index rather than five, which is
-- phase 09's lesson about indexes that serve no query.
CREATE INDEX idx_colour_project ON image_colour_decision(project_id, photo_id);

-- The low-confidence review queue, weakest first. Partial, because the column is above the
-- threshold for the overwhelming majority of a wedding and the only query that reads it asks
-- about the ones where it is not.
CREATE INDEX idx_colour_review
  ON image_colour_decision(project_id, confidence)
  WHERE reviewed = 0 AND user_edited = 0;

-- Phase 27's query, and the support engineer's: which frames did the skin guard have to act
-- on. Partial for the same reason - a wedding has a handful and four thousand that it did
-- not.
CREATE INDEX idx_colour_guard
  ON image_colour_decision(project_id, skin_hue_shift)
  WHERE guard_triggered = 1;

-- ---------------------------------------------------------------------------
-- How much of a wedding has a grade, and how much of the guarantee was actually checked.
--
-- The denominator is **every photograph in the project**, as phases 09, 10, 11, 14 and 15's
-- are. The second column is the one that matters: `skin_measured` is how a caller finds out
-- that the headline guarantee was verified on three percent of a wedding, which is what a
-- wedding with an untrained face detector looks like.
-- ---------------------------------------------------------------------------
CREATE VIEW v_colour_coverage AS
SELECT
  p.project_id                                                AS project_id,
  COUNT(*)                                                    AS images,
  COUNT(c.photo_id)                                           AS decided,
  SUM(CASE WHEN c.skin_measured   = 1 THEN 1 ELSE 0 END)      AS skin_measured,
  SUM(CASE WHEN c.guard_triggered = 1 THEN 1 ELSE 0 END)      AS skin_guard_triggered,
  SUM(CASE WHEN c.clip_resolved   = 1 THEN 1 ELSE 0 END)      AS clip_guard_resolved,
  SUM(CASE WHEN c.subtlety_capped = 1 THEN 1 ELSE 0 END)      AS subtlety_capped,
  SUM(CASE WHEN c.user_edited     = 1 THEN 1 ELSE 0 END)      AS user_edited,
  SUM(CASE WHEN c.photo_id IS NOT NULL
             AND c.reviewed = 0
             AND c.user_edited = 0
             AND c.confidence < 0.55 THEN 1 ELSE 0 END)       AS needs_review,
  MAX(COALESCE(c.skin_hue_shift, 0.0))                        AS worst_skin_hue_shift
FROM photo p
LEFT JOIN image_colour_decision c ON c.photo_id = p.photo_id
GROUP BY p.project_id;
