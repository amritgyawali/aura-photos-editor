-- Migration 17 - a photographer's own look, and the evidence it was learned from.
--
-- PHASE-17 section 4 names three tables: `profiles`, `profile_buckets` and `style_pairs`.
-- All three are here, plus a fourth the phase document does not name and section 6.4 requires:
-- `project_style`, which records which profile a project uses and which profile one chapter of
-- it uses instead. It is a fourth table rather than three more columns on `project` because a
-- chapter override is one row per chapter and because `project` is migration 1's, frozen since
-- phase 01.
--
-- `docs/adr/ADR-0035-style-learning-and-personal-profiles.md` records the decisions behind the
-- columns and `ADR-0036` the ones behind what reaches a panel.
--
-- ---------------------------------------------------------------------------
-- WHAT THIS MIGRATION IS FOR
-- ---------------------------------------------------------------------------
--
-- Migration 16 recorded what a photograph should look like. This one records **how far this
-- particular photographer sits from that**, per kind of photograph and per kind of light, and
-- the pairs of their own work that the difference was measured from.
--
-- Seven properties are enforced here rather than remembered:
--
-- 1. **There is no skin colour anywhere in this file.** Migration 15 said it about white
--    balance and migration 16 said it about grading; this is the third time, and it is the
--    version of the trap that is hardest to see. A "preferred skin tone" column would look
--    *reasonable* here - it is the photographer's own taste, from their own work - and it is
--    the same mistake wearing a friendlier hat. `skin_warmth_deg` and `skin_chroma` are
--    DIFFERENCES from what phases 15 and 16 said about that frame's own people, bounded below
--    phase 16's own ceilings, and there is nothing in this schema for an absolute to live in.
--    The phase gate scans for one on every run.
--
-- 2. **No pixels, and no paths to pixels that leave the machine.** `style_pairs` stores two
--    file NAMES and a verdict. There is no blob column, no thumbnail, no crop and no URL.
--    Section 13's "never uploads imagery" is therefore a property of the schema, and
--    `aura-style` depends on no cloud crate, which is the other half of the same statement.
--
-- 3. **A rejected pair is stored, not dropped.** `accepted = 0` rows are the honesty section
--    6.1 asks for: "report how many pairs were used versus rejected". A schema that only kept
--    the accepted ones could report a hundred percent acceptance on any archive.
--
-- 4. **The pair table is the resume unit.** Section 11 budgets twenty-five minutes for two
--    thousand pairs and section 10.1 requires the run to be cancellable and resumable, so the
--    work remaining is a QUERY - pairs with no fit at this analysis version - rather than a
--    journal. Kill the process at 60 % and the next run re-fits nothing it already fitted.
--
-- 5. **A profile is never updated in place and never deleted.** A new training run writes a new
--    row with the next `version` for that name, and adopting it retires the previous one. A
--    gallery delivered under version 3 has to stay reproducible after version 4 exists, which
--    is invariant 4 reaching further than one render.
--
-- 6. **At most one adopted version per name**, enforced by a partial unique index rather than
--    by the code that writes it. Two adopted profiles called "personal" is two answers to "how
--    does this photographer edit", which is the failure this whole phase exists to prevent.
--
-- 7. **Every profile carries its own diagnostics.** `overall_de00`, `accepted_pairs`,
--    `rejected_pairs` and the per-bucket rows are stored beside the deltas rather than
--    recomputed, because the number a photographer was shown when they adopted a profile is
--    part of the record of why they adopted it.
--
-- ROLLBACK. Every object here is new and nothing outside this file references it:
--
--   DROP VIEW  IF EXISTS v_style_coverage;
--   DROP TABLE IF EXISTS project_style;
--   DROP TABLE IF EXISTS style_pairs;
--   DROP TABLE IF EXISTS profile_buckets;
--   DROP TABLE IF EXISTS profiles;
--   DELETE FROM schema_version WHERE version = 17;
--
-- Running those returns the catalog to schema 16. **It is recomputable only if the archives
-- are still there**, which is the first migration since 01 where that caveat is real: a
-- profile is derived from folders outside the catalog, and if a photographer has since moved
-- or deleted the wedding they trained on, the profile cannot be re-fitted. The rollback runbook
-- says to export every adopted profile to a signed bundle first, which is the whole reason
-- export exists as a first-class feature rather than as a convenience.

-- ---------------------------------------------------------------------------
-- One row per trained profile version. What the photographer's look is.
-- ---------------------------------------------------------------------------
CREATE TABLE profiles (
  profile_id          TEXT    NOT NULL PRIMARY KEY,
  -- What the photographer calls it: "personal", "light and airy", "second shooter". Versions
  -- of one look share a name and differ in `version`.
  name                TEXT    NOT NULL,
  version             INTEGER NOT NULL CHECK (version >= 1),
  -- 'candidate', 'adopted' or 'retired'. Training produces a candidate; adoption is a separate
  -- deliberate act (section 6.3) and it retires whatever was adopted under the same name.
  status              TEXT    NOT NULL DEFAULT 'candidate'
                        CHECK (status IN ('candidate','adopted','retired')),

  -- The global lean, applied to every photograph before any conditioning. Eleven scalars, one
  -- curve shift and one eight-band block, in the recipe's own units so that a stored delta and
  -- an applied one are the same numbers rather than two roundings of one number.
  exposure            REAL    NOT NULL DEFAULT 0.0,
  temperature_k       REAL    NOT NULL DEFAULT 0.0,
  tint                REAL    NOT NULL DEFAULT 0.0,
  contrast            REAL    NOT NULL DEFAULT 0.0,
  highlights          REAL    NOT NULL DEFAULT 0.0,
  shadows             REAL    NOT NULL DEFAULT 0.0,
  whites              REAL    NOT NULL DEFAULT 0.0,
  blacks              REAL    NOT NULL DEFAULT 0.0,
  vibrance            REAL    NOT NULL DEFAULT 0.0,
  saturation          REAL    NOT NULL DEFAULT 0.0,
  -- Five control-point offsets at fixed input levels, as a flat array. Fixed anchors rather
  -- than moved points, because phase 16's fitter produces between four and eight points and the
  -- count depends on the frame.
  curve_shift         TEXT    NOT NULL DEFAULT '[0,0,0,0,0]',
  -- The eight bands, as a flat array of h, s, l triples in the frozen band order.
  hsl                 TEXT    NOT NULL DEFAULT '[]',

  -- How this photographer treats skin, RELATIVE to what phases 15 and 16 said about that
  -- frame's own people. See note 1. Bounded below phase 16's own ceilings by CHECK, so a row
  -- that could ask for more than the guarantee allows cannot be written at all.
  skin_warmth_deg     REAL    NOT NULL DEFAULT 0.0
                        CHECK (skin_warmth_deg >= -1.5 AND skin_warmth_deg <= 1.5),
  skin_chroma         REAL    NOT NULL DEFAULT 0.0
                        CHECK (skin_chroma >= -0.04 AND skin_chroma <= 0.04),

  confidence          REAL    NOT NULL DEFAULT 0.0
                        CHECK (confidence >= 0.0 AND confidence <= 1.0),

  -- The diagnostics, stored rather than recomputed. See note 7.
  trained_pairs       INTEGER NOT NULL DEFAULT 0 CHECK (trained_pairs   >= 0),
  accepted_pairs      INTEGER NOT NULL DEFAULT 0 CHECK (accepted_pairs  >= 0),
  rejected_pairs      INTEGER NOT NULL DEFAULT 0 CHECK (rejected_pairs  >= 0),
  overall_de00        REAL    NOT NULL DEFAULT 0.0 CHECK (overall_de00  >= 0.0),
  weak_buckets        TEXT    NOT NULL DEFAULT '[]',
  recommendation      TEXT    NOT NULL DEFAULT '',

  -- Which render engine the fit was measured against, and which build's fitter produced it.
  -- They invalidate different things and `AURA-ML-5077` is raised when a comparison would
  -- cross either. Ninth version-drift code in the product.
  engine_ver          TEXT    NOT NULL,
  analysis_ver        INTEGER NOT NULL,

  trained_at_ts       INTEGER NOT NULL,
  trained_at          TEXT    NOT NULL
) STRICT;

-- One version of one name, once.
CREATE UNIQUE INDEX idx_profiles_name_version ON profiles(name, version);

-- At most one adopted version per name. See note 6: this is a guarantee about the product's
-- behaviour and it is enforced by the database rather than by the code that writes it.
CREATE UNIQUE INDEX idx_profiles_adopted
  ON profiles(name)
  WHERE status = 'adopted';

-- The profile list, newest first.
CREATE INDEX idx_profiles_recent ON profiles(trained_at_ts DESC, profile_id);

-- ---------------------------------------------------------------------------
-- One row per populated leaf of one profile's tree.
--
-- Eighty leaves are possible - eight scene groups times ten lighting buckets - and a real
-- photographer populates twelve of them. Only the populated ones are stored: an empty bucket
-- is the absence of a row, and a caller reading no row gets the parent's answer, which is
-- exactly what an empty bucket means.
-- ---------------------------------------------------------------------------
CREATE TABLE profile_buckets (
  profile_id          TEXT    NOT NULL REFERENCES profiles(profile_id) ON DELETE CASCADE,
  -- 'group/lighting', which is the coordinate rather than an id. A bucket is not a thing that
  -- can be looked up by anything the coordinate cannot look it up by; see the note on
  -- `ProfileId` in `crates/aura-core/src/contract/ids.rs`.
  bucket              TEXT    NOT NULL,
  -- Denormalised out of the key so the matrix can group by one of them without parsing eighty
  -- strings. The pair is the primary key; these two are the same information, indexed.
  scene_group         TEXT    NOT NULL,
  lighting            TEXT    NOT NULL,

  -- The same eleven scalars, curve shift and band block as `profiles`, and they mean the same
  -- thing: this bucket's contribution ON TOP OF its scene group's, which is on top of the
  -- global. The resolution chain adds them; it never replaces one with another.
  exposure            REAL    NOT NULL DEFAULT 0.0,
  temperature_k       REAL    NOT NULL DEFAULT 0.0,
  tint                REAL    NOT NULL DEFAULT 0.0,
  contrast            REAL    NOT NULL DEFAULT 0.0,
  highlights          REAL    NOT NULL DEFAULT 0.0,
  shadows             REAL    NOT NULL DEFAULT 0.0,
  whites              REAL    NOT NULL DEFAULT 0.0,
  blacks              REAL    NOT NULL DEFAULT 0.0,
  vibrance            REAL    NOT NULL DEFAULT 0.0,
  saturation          REAL    NOT NULL DEFAULT 0.0,
  curve_shift         TEXT    NOT NULL DEFAULT '[0,0,0,0,0]',
  hsl                 TEXT    NOT NULL DEFAULT '[]',
  skin_warmth_deg     REAL    NOT NULL DEFAULT 0.0
                        CHECK (skin_warmth_deg >= -1.5 AND skin_warmth_deg <= 1.5),
  skin_chroma         REAL    NOT NULL DEFAULT 0.0
                        CHECK (skin_chroma >= -0.04 AND skin_chroma <= 0.04),

  confidence          REAL    NOT NULL DEFAULT 0.0
                        CHECK (confidence >= 0.0 AND confidence <= 1.0),
  -- True when this row is a scene group's delta rather than a leaf's. The two live in one table
  -- because they are the same shape and the same arithmetic; `bucket` for a group row is
  -- 'group/*', which `StyleBucket::from_key` never produces and so cannot collide with a leaf.
  is_group            INTEGER NOT NULL DEFAULT 0 CHECK (is_group IN (0,1)),

  samples             INTEGER NOT NULL DEFAULT 0 CHECK (samples  >= 0),
  held_out            INTEGER NOT NULL DEFAULT 0 CHECK (held_out >= 0),
  -- NULL, not zero, when nothing was held out. Zero would read as a perfect match and this
  -- column is the honest half of the whole feature. ADR-0036 decision 2.
  match_de00          REAL             CHECK (match_de00 IS NULL OR match_de00 >= 0.0),
  -- How much of its own answer this bucket kept, after shrinkage toward its parent.
  shrink              REAL    NOT NULL DEFAULT 0.0 CHECK (shrink >= 0.0 AND shrink <= 1.0),

  PRIMARY KEY (profile_id, bucket)
) STRICT, WITHOUT ROWID;

-- The matrix reads one profile's rows in group order. One index, because that is the one query.
CREATE INDEX idx_profile_buckets_group ON profile_buckets(profile_id, scene_group, lighting);

-- ---------------------------------------------------------------------------
-- One row per original-and-final pair the scan found. The evidence, and the resume unit.
--
-- No pixels. See note 2.
-- ---------------------------------------------------------------------------
CREATE TABLE style_pairs (
  -- BLAKE3 of the original's bytes. The key, so a re-scan of an archive that was renamed
  -- between runs re-uses the fit rather than re-computing it - which is what makes note 4 true
  -- across a rename as well as across a cancel.
  original_hash       TEXT    NOT NULL,
  profile_name        TEXT    NOT NULL,
  -- File names, without their directories. Enough for a report and a support conversation, and
  -- not enough to reconstruct where anybody's wedding lives.
  original_name       TEXT    NOT NULL,
  final_name          TEXT    NOT NULL,

  -- 'content_hash', 'filename_stem', 'capture_time', 'perceptual' or 'unmatched'.
  matched_by          TEXT    NOT NULL DEFAULT 'unmatched',
  -- 'xmp', 'fitted' or 'none'.
  extracted_from      TEXT    NOT NULL DEFAULT 'none',
  bucket              TEXT    NOT NULL DEFAULT 'other/unknown',

  -- The photographer's own parameters, as extracted. The same eleven scalars and the curve, in
  -- the recipe's units. These are ABSOLUTE values - what the photographer set - and the delta
  -- that reaches a profile is the difference between these and what phases 15 and 16 would have
  -- said about the same frame.
  their_exposure      REAL    NOT NULL DEFAULT 0.0,
  their_temperature_k REAL    NOT NULL DEFAULT 5500.0,
  their_tint          REAL    NOT NULL DEFAULT 0.0,
  their_contrast      REAL    NOT NULL DEFAULT 0.0,
  their_highlights    REAL    NOT NULL DEFAULT 0.0,
  their_shadows       REAL    NOT NULL DEFAULT 0.0,
  their_whites        REAL    NOT NULL DEFAULT 0.0,
  their_blacks        REAL    NOT NULL DEFAULT 0.0,
  their_vibrance      REAL    NOT NULL DEFAULT 0.0,
  their_saturation    REAL    NOT NULL DEFAULT 0.0,
  their_curve         TEXT    NOT NULL DEFAULT '[0,0,255,255]',
  their_hsl           TEXT    NOT NULL DEFAULT '[]',

  -- The six features section 6.2 conditions the regression on, measured once and stored so a
  -- re-fit at a new shrinkage does not re-open a single file.
  subject_luma        REAL    NOT NULL DEFAULT 0.0,
  cct_k               REAL    NOT NULL DEFAULT 5500.0,
  dynamic_range_ev    REAL    NOT NULL DEFAULT 0.0,
  iso                 INTEGER NOT NULL DEFAULT 0 CHECK (iso >= 0),
  flash               INTEGER NOT NULL DEFAULT 0 CHECK (flash       IN (0,1)),
  mixed_light         INTEGER NOT NULL DEFAULT 0 CHECK (mixed_light IN (0,1)),

  -- What the fit could not explain, and whether the pair survived it. See note 3.
  residual_de00       REAL    NOT NULL DEFAULT 0.0 CHECK (residual_de00 >= 0.0),
  accepted            INTEGER NOT NULL DEFAULT 0 CHECK (accepted  IN (0,1)),
  -- The stable reason code when it was not, from the frozen `StyleCode` vocabulary. NULL when
  -- the pair was accepted, which is the one place in this schema where NULL means "nothing to
  -- say" rather than "not measured".
  rejection           TEXT,
  -- True when this pair was kept out of the fit and used to measure it. Section 6.3's
  -- "re-render a held-out set of the photographer's own pairs".
  held_out            INTEGER NOT NULL DEFAULT 0 CHECK (held_out  IN (0,1)),

  -- Which archive this pair came from, so section 10.1's robustness gate - one wedding cannot
  -- move the profile beyond a bounded amount - is a GROUP BY rather than a promise.
  archive             TEXT    NOT NULL DEFAULT '',

  analysis_ver        INTEGER NOT NULL,
  at_ts               INTEGER NOT NULL,

  PRIMARY KEY (profile_name, original_hash)
) STRICT, WITHOUT ROWID;

-- The pending set: pairs with no fit at this analysis version. Invariant 5 as a query.
CREATE INDEX idx_style_pairs_pending ON style_pairs(profile_name, analysis_ver);

-- The per-bucket fit reads one profile's accepted pairs grouped by bucket, and the archive cap
-- reads them grouped by archive. Two columns, one index, because the fit does both in one pass.
CREATE INDEX idx_style_pairs_fit
  ON style_pairs(profile_name, bucket, archive)
  WHERE accepted = 1;

-- ---------------------------------------------------------------------------
-- Which profile a project uses, and which profile one chapter of it uses instead.
--
-- Section 6.4: "Profiles are selectable per project and per chapter, which supports real
-- studio practice (a moody reception with an airy ceremony)". The chapter is phase 07's, from
-- the nine-chapter vocabulary, and '*' is the project-wide default rather than a NULL - a NULL
-- in a primary key is not a value SQLite will enforce uniqueness on.
--
-- These rows live here rather than inside a profile because a profile is a PORTABLE artefact a
-- studio distributes, and a chapter assignment is a decision about one wedding. A profile
-- carrying chapter assignments would rearrange somebody else's catalog on import.
-- ---------------------------------------------------------------------------
CREATE TABLE project_style (
  project_id          TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  -- '*' for the project default, otherwise one of phase 07's nine chapter slugs.
  chapter             TEXT    NOT NULL DEFAULT '*',
  profile_id          TEXT    NOT NULL REFERENCES profiles(profile_id) ON DELETE CASCADE,
  at_ts               INTEGER NOT NULL,

  PRIMARY KEY (project_id, chapter)
) STRICT, WITHOUT ROWID;

-- ---------------------------------------------------------------------------
-- How much of a profile's tree the photographer actually taught.
--
-- The denominator is **populated buckets**, not the eighty possible ones - unlike every
-- coverage view since phase 09, and deliberately. A photographer who only ever shoots outdoor
-- daylight weddings has seventy-eight empty buckets and a perfectly good profile, and a
-- coverage figure of 2 % would be a number that says nothing about them. What matters here is
-- how many of the buckets that DO have evidence have enough of it, which is what this counts.
-- ---------------------------------------------------------------------------
CREATE VIEW v_style_coverage AS
SELECT
  p.profile_id                                                  AS profile_id,
  p.name                                                        AS name,
  p.version                                                     AS version,
  p.status                                                      AS status,
  p.trained_pairs                                               AS trained_pairs,
  p.accepted_pairs                                              AS accepted_pairs,
  p.rejected_pairs                                              AS rejected_pairs,
  p.overall_de00                                                AS overall_de00,
  COUNT(b.bucket)                                               AS buckets,
  SUM(CASE WHEN b.is_group = 0 AND b.samples >  10 THEN 1 ELSE 0 END) AS strong_buckets,
  SUM(CASE WHEN b.is_group = 0 AND b.samples <= 10
             AND b.samples > 0                     THEN 1 ELSE 0 END) AS weak_buckets,
  SUM(CASE WHEN b.match_de00 IS NOT NULL           THEN 1 ELSE 0 END) AS measured_buckets,
  MAX(COALESCE(b.match_de00, 0.0))                              AS worst_bucket_de00
FROM profiles p
LEFT JOIN profile_buckets b ON b.profile_id = p.profile_id
GROUP BY p.profile_id;
