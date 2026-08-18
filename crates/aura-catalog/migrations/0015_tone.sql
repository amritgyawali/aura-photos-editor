-- Migration 15 - what the light was, and what the exposure should be.
--
-- PHASE-15 section 4 names one file: "`image_tone_estimate` table with alternatives". It
-- needs three more, and section 6.2, 6.3 and 6.4 are where each of them comes from:
-- a per-identity skin locus that is "accumulated across the wedding, so a single bad frame
-- cannot mislead", and a per-segment set of reference frames that phase 25 normalises
-- toward. `docs/adr/ADR-0031-exposure-white-balance-and-skin.md` records the rest.
--
-- ---------------------------------------------------------------------------
-- WHAT THIS MIGRATION IS FOR
-- ---------------------------------------------------------------------------
--
-- Fourteen migrations have recorded what AURA found, what it decided, and what it would do
-- to the pixels. This one records **why the pixels should move** - the first migration whose
-- rows are an argument rather than an observation or an instruction.
--
-- Six properties are enforced here rather than remembered:
--
-- 1. **There is no ideal-skin column, anywhere in this file.** `identity_skin_locus` holds
--    one chromaticity per person, measured from that person's own best-lit frames in this
--    wedding. There is no table of target skin values, no bucket column, no tone category
--    and no place to put one. Section 6.3's fairness argument is that a fixed target is how
--    an editor lightens dark skin while believing it is correcting a cast; a schema that
--    cannot express a fixed target cannot do it. The Monk-scale buckets the evaluation
--    harness uses to *measure* fairness live in `tests/eval` and never reach here.
--
-- 2. **Nothing in this migration is an edit.** There is no recipe column, no exposure that
--    gets applied, and no path. The values reach the pixels through `edit_recipes` and
--    `aura_recipe::schema::merge` only - phase 14's rule - and this table is the record of
--    what the product thought, kept beside whatever the photographer decided. A row with
--    `user_edited = 1` still carries AURA's own numbers, which is what makes the review
--    queue able to show a disagreement and phase 30's learning loop able to read one.
--
-- 3. **`user_edited` is checked inside the statement, not before it.** A re-analysis
--    overwrites every estimate it recomputes; the upsert re-applies the photographer's own
--    override from the row it is replacing, in the same statement. Phases 06 to 12 wrote
--    this rule for identities, segments, moments, verdicts and framing notes; this is the
--    seventh time and the window it closes is the same one every time.
--
-- 4. **Three JSON columns, and why they are not three tables.** `illuminants`, `reasons`
--    and `alternatives` are always read together by the panel that draws them and never
--    queried across. Section 11 budgets **600 bytes per image**, the tightest per-image
--    figure in the product, and three child tables would cost more in row overhead and
--    index than the documents themselves. The keys inside those documents are one and two
--    characters for the same reason, and `aura_brain_photo::tone::store::codec` is the one
--    place that knows what they mean.
--
-- 5. **A reason stores its code, not its sentence.** Phase 09's rule, for the fourth
--    migration running: a stored sentence is copy that a release can change without any
--    behaviour changing, and a catalog full of English is a catalog nobody can translate.
--    The `t` key is present only when the sentence differs from `ToneCode::user_text`,
--    which it does for the four codes that name a temperature or a person.
--
-- 6. **A reference frame is an anchor, never a correction.** `segment_reference_frames`
--    holds a pointer and a quality. There is no column that says what another frame should
--    become, because that is phase 25's decision and this is phase 15's evidence.
--
-- ROLLBACK. Every object here is new and nothing outside this file references it:
--
--   DROP VIEW  IF EXISTS v_tone_coverage;
--   DROP TABLE IF EXISTS segment_reference_frames;
--   DROP TABLE IF EXISTS identity_skin_locus;
--   DROP TABLE IF EXISTS image_tone_estimate;
--   DELETE FROM schema_version WHERE version = 15;
--
-- Running those returns the catalog to schema 14. **It is recomputable**, unlike migrations
-- 13 and 14: every row here is derived from pixels, phase 06's faces and phase 07's scenes,
-- so a re-run reproduces it exactly - with one exception, and it is the usual one. A
-- photographer's own override in `user_exposure_ev`, `user_temperature_k` and `user_tint`
-- is not derivable from anything, and the rollback runbook says to export those three
-- columns first. They also live in `edit_recipes` and in the sidecars beside the RAWs,
-- which is the second copy that makes the loss survivable.

-- ---------------------------------------------------------------------------
-- One row per photograph. What the light was and what the exposure should be.
-- ---------------------------------------------------------------------------
CREATE TABLE image_tone_estimate (
  photo_id            TEXT    NOT NULL PRIMARY KEY REFERENCES photo(photo_id) ON DELETE CASCADE,
  project_id          TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,

  -- The three numbers this whole phase exists to produce.
  exposure_ev         REAL    NOT NULL CHECK (exposure_ev >= -5.0 AND exposure_ev <= 5.0),
  temperature_k       REAL    NOT NULL CHECK (temperature_k >= 2000.0 AND temperature_k <= 50000.0),
  tint                REAL    NOT NULL CHECK (tint >= -150.0 AND tint <= 150.0),

  -- Two confidences rather than one, because the two fail independently: a backlit frame
  -- can have an obvious white balance and a hard-won exposure, and a tungsten close-up the
  -- reverse. `ToneEstimate::confidence` is their geometric mean and is what the ledger
  -- records; storing only the mean would make either half unfalsifiable.
  exposure_conf       REAL    NOT NULL CHECK (exposure_conf >= 0.0 AND exposure_conf <= 1.0),
  wb_conf             REAL    NOT NULL CHECK (wb_conf >= 0.0 AND wb_conf <= 1.0),

  -- Section 6.1's anchor, before and after. `subject_luma_before` is 0.0 when
  -- `face_anchored` is 0, and that means "not measured" rather than "black" - which is why
  -- the flag is a column and not something a reader infers from a zero.
  subject_luma_before REAL    NOT NULL DEFAULT 0.0,
  subject_luma_target REAL    NOT NULL DEFAULT 0.0,
  face_anchored       INTEGER NOT NULL DEFAULT 0 CHECK (face_anchored IN (0,1)),

  -- Section 2.1's three hard cases, as three bits a query can filter on. `mixed_light` is
  -- the one section 2.1 names explicitly - "record a `MIXED_LIGHT` note so Phase 18 can
  -- apply local WB corrections later" - and phase 18 selects on it.
  mixed_light         INTEGER NOT NULL DEFAULT 0 CHECK (mixed_light IN (0,1)),
  coloured_light      INTEGER NOT NULL DEFAULT 0 CHECK (coloured_light IN (0,1)),
  backlit             INTEGER NOT NULL DEFAULT 0 CHECK (backlit IN (0,1)),

  -- Which of `illuminants` governs the subject. NULL when there was no subject to govern,
  -- which is a different statement from 0 and is why this is nullable rather than defaulted.
  dominant_on_subject INTEGER CHECK (dominant_on_subject IS NULL OR dominant_on_subject >= 0),
  -- The dominant illuminant's kind, denormalised out of the JSON so the outline's histogram
  -- is one GROUP BY rather than four thousand JSON parses.
  dominant_kind       TEXT    NOT NULL DEFAULT 'unknown',

  -- How much highlight clipping this exposure ADDS, as a fraction of the frame. Section
  -- 10.1's "never increases clipping beyond scene tolerance" is a query against this column
  -- rather than a re-render.
  clipping_added      REAL    NOT NULL DEFAULT 0.0 CHECK (clipping_added >= 0.0),
  -- The estimated skin error this solution leaves, in dE00, measured against the identities'
  -- own loci. Consistency across the wedding, not accuracy against a chart: the chart figure
  -- needs expert references and lives in the eval harness.
  skin_de00           REAL    NOT NULL DEFAULT 0.0 CHECK (skin_de00 >= 0.0),
  -- How many identities in this frame had a usable locus. The per-frame denominator behind
  -- `ToneOutline::skin_constrained`.
  constrained_ids     INTEGER NOT NULL DEFAULT 0 CHECK (constrained_ids >= 0),

  -- Which scene the bands came from. Invariant 7: a stored decision that does not say which
  -- scene it assumed is not reproducible.
  scene               TEXT    NOT NULL DEFAULT 'unknown',
  -- True when that scene had no target row and the neutral band was used.
  untargeted          INTEGER NOT NULL DEFAULT 0 CHECK (untargeted IN (0,1)),

  -- The three documents. See note 4. Empty arrays are stored as '[]' rather than NULL so a
  -- reader never has two spellings of "nothing here".
  illuminants         TEXT    NOT NULL DEFAULT '[]',
  reasons             TEXT    NOT NULL DEFAULT '[]',
  alternatives        TEXT    NOT NULL DEFAULT '[]',

  -- Invariant 2, enforced by the schema rather than by review. An estimate with no reason is
  -- a decision that cannot explain itself, which migration 13 refuses for the ledger and
  -- this refuses for the thing the ledger would record.
  reason_count        INTEGER NOT NULL DEFAULT 0 CHECK (reason_count >= 1 AND reason_count <= 6),

  -- The photographer's own answer, and the two bits that protect it. See note 3.
  user_edited         INTEGER NOT NULL DEFAULT 0 CHECK (user_edited IN (0,1)),
  reviewed            INTEGER NOT NULL DEFAULT 0 CHECK (reviewed IN (0,1)),
  user_exposure_ev    REAL,
  user_temperature_k  REAL,
  user_tint           REAL,

  -- Three versions, because they invalidate three different things. See the contract's
  -- header; `AURA-ML-5060` is raised when a comparison would cross any of them.
  model_ver           INTEGER NOT NULL,
  analysis_ver        INTEGER NOT NULL,
  targets_ver         INTEGER NOT NULL,

  at_ts               INTEGER NOT NULL,
  at                  TEXT    NOT NULL
) STRICT;

-- The pending set and the outline both scan one project. One index rather than four, which
-- is phase 09's lesson about indexes that serve no query: the review queue, the histogram
-- and the coverage view all start from this one.
CREATE INDEX idx_tone_project ON image_tone_estimate(project_id, photo_id);

-- The low-confidence review queue, weakest first. A partial index because the column is
-- above the threshold for the overwhelming majority of a wedding, and the only query that
-- reads it asks about the ones where it is not.
CREATE INDEX idx_tone_review
  ON image_tone_estimate(project_id, wb_conf)
  WHERE reviewed = 0 AND user_edited = 0;

-- Phase 18's query: which frames need a local white-balance correction. Partial for the same
-- reason - a wedding has a handful of genuinely mixed-light frames and four thousand that
-- are not.
CREATE INDEX idx_tone_mixed
  ON image_tone_estimate(project_id)
  WHERE mixed_light = 1;

-- ---------------------------------------------------------------------------
-- One row per identity. Where that person's skin actually sits.
--
-- See note 1. There is no target here and nowhere to put one.
-- ---------------------------------------------------------------------------
CREATE TABLE identity_skin_locus (
  identity_id         TEXT    NOT NULL PRIMARY KEY REFERENCES identities(id) ON DELETE CASCADE,
  project_id          TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,

  -- CIE 1976 u'v'. The solve happens in this space rather than in kelvin, because a
  -- distance in kelvin is not a distance in colour and a fluorescent tube has no honest CCT.
  u                   REAL    NOT NULL,
  v                   REAL    NOT NULL,
  -- How far a sample may sit from the centre and still be plausible. Bounded by
  -- `SkinLocus::MIN_RADIUS` and `MAX_RADIUS`: a person shot under one light all day has a
  -- measured spread of nearly zero, and a constraint that tight rejects every honest answer
  -- at the reception.
  radius              REAL    NOT NULL CHECK (radius > 0.0 AND radius <= 0.5),
  -- The reference luminance of this person's skin, 0..1. Measured, never assumed: this is
  -- the number a fixed "ideal skin brightness" would have been, and the single place a
  -- lightening bias would enter the product.
  luma                REAL    NOT NULL CHECK (luma >= 0.0 AND luma <= 1.0),

  -- Below `MIN_LOCUS_SAMPLES` the locus constrains nothing and `ToneCode::SkinLocusUnavailable`
  -- is emitted instead. A weak locus is worse than none, because it looks like evidence.
  samples             INTEGER NOT NULL DEFAULT 0 CHECK (samples >= 0),
  cohesion            REAL    NOT NULL DEFAULT 0.0 CHECK (cohesion >= 0.0 AND cohesion <= 1.0),

  analysis_ver        INTEGER NOT NULL,
  at_ts               INTEGER NOT NULL,
  at                  TEXT    NOT NULL
) STRICT;

-- Every locus in one wedding, read once per pass rather than once per frame.
CREATE INDEX idx_skin_locus_project ON identity_skin_locus(project_id);

-- ---------------------------------------------------------------------------
-- Three to five rows per segment. What phase 25 normalises toward.
--
-- See note 6. An anchor, never a correction.
-- ---------------------------------------------------------------------------
CREATE TABLE segment_reference_frames (
  segment_id          TEXT    NOT NULL REFERENCES segments(id) ON DELETE CASCADE,
  rank                INTEGER NOT NULL CHECK (rank >= 0 AND rank < 5),
  photo_id            TEXT    NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,

  -- Copied from the estimate rather than joined. Phase 25 reads this table and nothing else
  -- for a whole wedding's worth of segments, and a join per anchor to fetch two floats is
  -- the query that makes a normalisation pass feel slow. The copy is invalidated with the
  -- rest of the row: a re-analysis deletes and rebuilds a segment's anchors together.
  wb_conf             REAL    NOT NULL CHECK (wb_conf >= 0.0 AND wb_conf <= 1.0),
  temperature_k       REAL    NOT NULL,
  tint                REAL    NOT NULL,
  subject_luma        REAL    NOT NULL,
  quality             REAL    NOT NULL CHECK (quality >= 0.0 AND quality <= 1.0),

  analysis_ver        INTEGER NOT NULL,
  at_ts               INTEGER NOT NULL,
  at                  TEXT    NOT NULL,

  PRIMARY KEY (segment_id, rank)
) STRICT, WITHOUT ROWID;

-- One photograph can anchor only one segment, and finding out which is how the panel puts a
-- badge on it.
CREATE INDEX idx_reference_frames_photo ON segment_reference_frames(photo_id);

-- ---------------------------------------------------------------------------
-- How much of a wedding has an exposure decision, and how much of that decision was made
-- by looking at a face.
--
-- The denominator is **every photograph in the project**, as phases 09, 10, 11 and 14's are.
-- The second column is the one that matters: `face_anchored` is section 1's entire argument
-- expressed as a fraction, and a wedding at 100 % coverage and 3 % face-anchored has been
-- exposed exactly the way every other editor exposes it.
-- ---------------------------------------------------------------------------
CREATE VIEW v_tone_coverage AS
SELECT
  p.project_id                                                AS project_id,
  COUNT(*)                                                    AS images,
  COUNT(t.photo_id)                                           AS estimated,
  SUM(CASE WHEN t.face_anchored  = 1 THEN 1 ELSE 0 END)       AS face_anchored,
  SUM(CASE WHEN t.constrained_ids > 0 THEN 1 ELSE 0 END)      AS skin_constrained,
  SUM(CASE WHEN t.mixed_light    = 1 THEN 1 ELSE 0 END)       AS mixed_light,
  SUM(CASE WHEN t.coloured_light = 1 THEN 1 ELSE 0 END)       AS coloured_light,
  SUM(CASE WHEN t.user_edited    = 1 THEN 1 ELSE 0 END)       AS user_edited,
  SUM(CASE WHEN t.photo_id IS NOT NULL
             AND t.reviewed = 0
             AND t.user_edited = 0
             AND t.wb_conf < 0.55 THEN 1 ELSE 0 END)          AS needs_review
FROM photo p
LEFT JOIN image_tone_estimate t ON t.photo_id = p.photo_id
GROUP BY p.project_id;
