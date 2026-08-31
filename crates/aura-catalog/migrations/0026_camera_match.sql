-- Migration 26 - how each body at a wedding renders colour, what it needs to look like the one
-- everything else is matched to, the photographs that prove it, and how differently each
-- photographer exposes.
--
-- PHASE-26 section 4 names three tables: `camera_fingerprints`, `camera_transforms`,
-- `matched_pairs`. What ships is five, singular and prefixed like every table since migration 09,
-- and the two extra ones are the per-shooter habit section 6.3 requires and the reference choice
-- section 2.1 makes user-selectable.
-- `docs/adr/ADR-0053-camera-matching-and-appearance-transforms.md` records the rest.
--
-- ---------------------------------------------------------------------------
-- WHAT THIS MIGRATION IS FOR
-- ---------------------------------------------------------------------------
--
-- Twenty-four migrations recorded facts about **one photograph**. Migration 25 recorded a fact
-- about a **set** of them. This one records a fact about a **body**, and the difference from both
-- is that the row count does not grow with the wedding: a three-camera wedding has six transform
-- rows whether it is four hundred photographs or six thousand.
--
-- That produces five properties enforced here rather than remembered:
--
-- 1. **A body is two populations, and the key says so.** Every fingerprint and every transform is
--    keyed on `(project_id, camera_id, flash)`. There is no shape in this schema that can hold one
--    transform for a whole body, because section 6.1 is explicit that brand differences are
--    amplified under flash and a single transform fitted across both populations is wrong for
--    both. `flash` is CHECKed against the two values `FlashState::ALL` renders, and there is no
--    third: a photograph whose EXIF does not say is `ambient`, because "we could not tell" and "no
--    flash" want the same correction and a third population would never have enough frames to
--    fingerprint.
--
-- 2. **A transform is a residual and the schema cannot express an absolute.** Every movement column
--    is named `d_` or is a multiplier around one, and every one is CHECKed against the contract's
--    own ceiling. There is no `cct_k` column on `camera_transform`, and a caller that wanted one
--    would be trying to make this phase a second answer to "what colour was the light", which is
--    phase 15's question and phase 15's row. Migration 25 wrote this note for its own deltas; the
--    two compose, in that order, and this one runs first.
--
-- 3. **The evidence is on the row, including the evidence that the check was not run.**
--    `distance_before`, `distance_after`, `heldout_before`, `heldout_after` and `heldout_pairs`
--    are five columns rather than a boolean, so section 6.2's "verify on held-out pairs" is a
--    stored fact and not a claim about a function nobody can inspect afterwards. `heldout_pairs =
--    0` and `heldout_after = heldout_before` is a check that did not happen, which is a different
--    row from a check that passed. Phase 24's rule - an absent input is ignorance, not permission.
--
-- 4. **`source` is three values and not two.** A transform blended between this wedding's own
--    matched pairs and a bundled brand baseline is neither `matched_pairs` nor `brand_baseline`,
--    and `blend` carries the share. A per-camera report whose whole job is to say what was
--    corrected and on what evidence cannot round either way. ADR-0053 section 3.
--
-- 5. **What a photographer chose survives automation, and what a split decided survives a
--    re-solve.** Three triggers: a user-chosen reference is never replaced by an automatic one,
--    `user_edited` is never cleared by an UPDATE, and a pair's `held_out` flag is immutable once
--    written. The first two are the pattern migrations 06, 07, 08, 18 and 25 all needed; the third
--    is this phase's own and it protects the phase's own verification - a held-out flag that could
--    move would let a re-solve promote the pairs that happened to agree with it, and section
--    10.1's held-out gate would keep passing while measuring nothing.
--
--    A trigger is only half of it here, because a pass writes with INSERT OR REPLACE against the
--    primary key and that fires no UPDATE trigger. `CameraStore::take_decisions` reads the
--    reference, the disabled bodies and the hand-set transforms out before a project is cleared and
--    `restore_decisions` puts them back afterwards - phase 25's mechanism, and phase 18's lesson
--    about why the DELETE guard alone is not enough.
--
-- ---------------------------------------------------------------------------
-- STORAGE
-- ---------------------------------------------------------------------------
--
-- Section 11 sets no per-image storage budget for this phase, and the reason is the shape above:
-- four of the five tables are per body, per shooter or per project. The one table that grows with
-- the wedding is `camera_pair`, and it is bounded twice - a pair must be inside one scene node and
-- inside `MAX_PAIR_GAP_MS`, and the pass keeps at most `MAX_PAIRS_PER_CAMERA` of the best per body.
-- `crates/aura-perf/tests/camera_budgets.rs` measures it on every run.
--
-- ---------------------------------------------------------------------------
-- ROLLBACK
-- ---------------------------------------------------------------------------
--
-- Reversible: DROP the five tables, the two views and the three triggers. Nothing here alters an
-- earlier table and nothing earlier references these, so a downgrade loses this phase's decisions
-- and no other. A gallery normalised on top of these transforms is re-solved by phase 25's own
-- version check, which is what makes the downgrade safe rather than merely possible.

-- ---------------------------------------------------------------------------
-- How each body renders colour
-- ---------------------------------------------------------------------------

CREATE TABLE camera_fingerprint (
  project_id          TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,

  -- The catalog's `camera.camera_id`. Deliberately not a foreign key onto `camera`: a body whose
  -- serial could not be read is `CameraId::UNKNOWN`, which is the empty string and is a real value
  -- rather than an absence - a wedding shot on one unidentified body must still be fingerprinted.
  -- Phase 08 made the same argument for `moments.camera_id`.
  camera_id           TEXT    NOT NULL,

  flash               TEXT    NOT NULL CHECK (flash IN ('ambient','flash')),

  -- The manufacturer, for the bundled baseline lookup only, and never an input to the solver. A
  -- body whose brand this cannot name still gets a full solved transform; only the baseline half
  -- of a blend is affected.
  brand               TEXT    NOT NULL DEFAULT 'neutral',

  -- Where this body puts skin and where it puts a neutral, both in CIE 1976 u'v'. The u'v' plane
  -- is bounded by construction, so the CHECKs here are a corruption guard rather than a policy.
  skin_u              REAL    NOT NULL CHECK (skin_u >= 0.0 AND skin_u <= 1.0),
  skin_v              REAL    NOT NULL CHECK (skin_v >= 0.0 AND skin_v <= 1.0),
  white_u             REAL    NOT NULL CHECK (white_u >= 0.0 AND white_u <= 1.0),
  white_v             REAL    NOT NULL CHECK (white_v >= 0.0 AND white_v <= 1.0),

  -- How saturation and contrast respond across four tonal quarters, as multipliers on the
  -- reference, plus how gently the highlights roll off. Eight numbers and one, stored as JSON
  -- arrays for the reason phase 16 stores its bands that way: they are read as a unit, never
  -- queried individually, and nine columns would be nine columns nobody filters on.
  sat_response        TEXT    NOT NULL,
  contrast_response   TEXT    NOT NULL,
  highlight_rolloff   REAL    NOT NULL CHECK (highlight_rolloff >= 0.0 AND highlight_rolloff <= 1.0),

  -- The eight-number colour character a grade-signature distance is measured over, and the robust
  -- subject luminance this body's frames sit at.
  grade_signature     TEXT    NOT NULL,
  subject_luma        REAL    NOT NULL CHECK (subject_luma >= 0.0 AND subject_luma <= 1.0),

  -- How many frames it was measured from, and what the measurement is worth. Both, because a
  -- fingerprint from two hundred frames the product was unsure about is not worth more than one
  -- from twenty it was sure about, and a panel showing "thin evidence" needs the count.
  samples             INTEGER NOT NULL CHECK (samples >= 0),
  confidence          REAL    NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),

  -- The reason set, as a bitmask over `CameraCode::ALL`. Thirty-two codes in a u32, which is
  -- exactly full: adding a thirty-third needs a second column or a wider integer, and the
  -- contract's own test asserts the count so the day that happens is a red build rather than a
  -- silently dropped reason.
  reasons             INTEGER NOT NULL DEFAULT 0,

  analysis_ver        INTEGER NOT NULL,
  created_at          TEXT    NOT NULL,

  PRIMARY KEY (project_id, camera_id, flash)
) STRICT, WITHOUT ROWID;

CREATE INDEX idx_camera_fingerprint_project ON camera_fingerprint(project_id, analysis_ver);

-- ---------------------------------------------------------------------------
-- What each body needs to look like the reference
-- ---------------------------------------------------------------------------

CREATE TABLE camera_transform (
  project_id          TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  camera_id           TEXT    NOT NULL,
  flash               TEXT    NOT NULL CHECK (flash IN ('ambient','flash')),

  -- The body this one is corrected toward. Equal to `camera_id` on the reference's own rows, whose
  -- transform is the identity.
  reference_id        TEXT    NOT NULL,

  -- The five scalar movements and the two vectors, every one of them CHECKed at the contract's own
  -- ceiling. `CameraTransform::within_bounds` refuses first; this is the second layer, and it
  -- exists because the first lives in Rust a future caller could route around with a raw INSERT.
  -- Section 10.1 makes "no camera exceeds documented maximum movement" a gate, and a gate that can
  -- only be measured is weaker than one that cannot be violated.
  d_cct               REAL    NOT NULL CHECK (d_cct        >= -900.0 AND d_cct        <= 900.0),
  d_tint              REAL    NOT NULL CHECK (d_tint       >=  -20.0 AND d_tint       <=  20.0),
  d_exposure          REAL    NOT NULL CHECK (d_exposure   >=   -0.6 AND d_exposure   <=   0.6),
  d_saturation        REAL    NOT NULL CHECK (d_saturation >=  -12.0 AND d_saturation <=  12.0),

  -- Three linear gains around one and three contrast multipliers around one, each within its own
  -- ceiling. Stored as JSON arrays and CHECKed by the component-wise columns beside them: SQLite
  -- cannot reach inside a JSON array in a CHECK, so the worst departure from unity is
  -- denormalised into `max_gain_dev` and `max_shape_dev` and those are what the constraint reads.
  -- Two columns to make one promise enforceable is cheaper than a promise that is only tested.
  channel_gain        TEXT    NOT NULL,
  contrast_shape      TEXT    NOT NULL,
  max_gain_dev        REAL    NOT NULL CHECK (max_gain_dev  >= 0.0 AND max_gain_dev  <= 0.10),
  max_shape_dev       REAL    NOT NULL CHECK (max_shape_dev >= 0.0 AND max_shape_dev <= 0.15),

  -- What it does to skin, and why it did not do more. `skin_de00_after` is the column section
  -- 10.1's headline gate is a SELECT MAX over rather than a sentence in a document - phase 16's
  -- rule, sixth application: a guarantee is measured, not asserted.
  skin_du             REAL    NOT NULL CHECK (skin_du >= -0.012 AND skin_du <= 0.012),
  skin_dv             REAL    NOT NULL CHECK (skin_dv >= -0.012 AND skin_dv <= 0.012),
  skin_dluma          REAL    NOT NULL CHECK (skin_dluma >= -0.04 AND skin_dluma <= 0.04),
  skin_de00_before    REAL    NOT NULL CHECK (skin_de00_before >= 0.0),
  skin_de00_after     REAL    NOT NULL CHECK (skin_de00_after  >= 0.0),
  skin_locus_valid    INTEGER NOT NULL DEFAULT 1 CHECK (skin_locus_valid IN (0,1)),
  skin_capped         INTEGER NOT NULL DEFAULT 0 CHECK (skin_capped IN (0,1)),

  -- Where the numbers came from, and how much of the blend is this wedding's own evidence.
  -- `blend = 1.0` with `source = 'matched_pairs'`, `blend = 0.0` with `source = 'brand_baseline'`,
  -- and anything between with `source = 'blended'` - see note 4.
  source              TEXT    NOT NULL CHECK (source IN ('matched_pairs','blended','brand_baseline')),
  blend               REAL    NOT NULL CHECK (blend >= 0.0 AND blend <= 1.0),
  evidence_pairs      INTEGER NOT NULL CHECK (evidence_pairs >= 0),

  -- The four appearance distances, each a JSON object of the metric's four components, plus how
  -- many pairs were held back. See note 3: `heldout_pairs = 0` is a check that did not run.
  distance_before     TEXT    NOT NULL,
  distance_after      TEXT    NOT NULL,
  heldout_before      TEXT    NOT NULL,
  heldout_after       TEXT    NOT NULL,
  heldout_pairs       INTEGER NOT NULL DEFAULT 0 CHECK (heldout_pairs >= 0),

  -- Which bound stopped it going further, when one did. NULL when none did, which is a different
  -- fact from `'cct'` and is why this is not a boolean.
  bounded_by          TEXT             CHECK (bounded_by IS NULL OR bounded_by IN
                        ('cct','tint','exposure','channel_gain','saturation','contrast_shape','skin')),

  confidence          REAL    NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
  reasons             INTEGER NOT NULL DEFAULT 0,

  -- The two things a photographer controls. Both are protected by triggers below: automation may
  -- set them and may never clear them.
  enabled             INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0,1)),
  user_edited         INTEGER NOT NULL DEFAULT 0 CHECK (user_edited IN (0,1)),

  analysis_ver        INTEGER NOT NULL,
  policy_ver          INTEGER NOT NULL,
  created_at          TEXT    NOT NULL,
  updated_at          TEXT    NOT NULL,

  PRIMARY KEY (project_id, camera_id, flash)
) STRICT, WITHOUT ROWID;

CREATE INDEX idx_camera_transform_project ON camera_transform(project_id, analysis_ver, policy_ver);
CREATE INDEX idx_camera_transform_source  ON camera_transform(project_id, source);

-- ---------------------------------------------------------------------------
-- The photographs that prove it
-- ---------------------------------------------------------------------------

CREATE TABLE camera_pair (
  pair_id             TEXT    NOT NULL PRIMARY KEY,
  project_id          TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,

  -- The scene node both frames belong to. Phase 25's tree is this pairing's outer key: two frames
  -- in different nodes were shot under different light by construction, whatever their subjects
  -- look like. Not a foreign key onto `gallery_node`, because a re-pass rebuilds that tree and a
  -- CASCADE would delete a pass's evidence half way through writing it.
  node_id             TEXT    NOT NULL,

  -- The reference body's frame and the other body's, and the two bodies. `left` and `right` are
  -- ordered: left is always the reference, so a viewer never has to work out which is which.
  left_image          TEXT    NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,
  right_image         TEXT    NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,
  left_camera         TEXT    NOT NULL,
  right_camera        TEXT    NOT NULL,

  -- The flash state both frames share. A pair across states is never formed - see note 1.
  flash               TEXT    NOT NULL CHECK (flash IN ('ambient','flash')),

  gap_ms              INTEGER NOT NULL CHECK (gap_ms >= 0 AND gap_ms <= 90000),

  -- How alike the subjects are, and how much the **backgrounds** agree. The second is what decides:
  -- two frames of the same face from two bodies differ in exactly the way this phase is trying to
  -- measure, so scoring the pair on the subject would be scoring the thing under test. Section 6.1.
  subject_similarity  REAL    NOT NULL CHECK (subject_similarity >= -1.0 AND subject_similarity <= 1.0),
  background_agreement REAL   NOT NULL CHECK (background_agreement >= 0.0 AND background_agreement <= 1.0),

  -- Whether the pair passed background verification, and whether it was held out of the fit.
  -- A rejected pair is **written rather than dropped**: phase 17's rule, second application, and
  -- for the same reason - here the rejection is the evidence a photographer needs when they ask
  -- why their second camera was matched from a brand baseline in a wedding both cameras shot.
  verified            INTEGER NOT NULL DEFAULT 0 CHECK (verified IN (0,1)),
  held_out            INTEGER NOT NULL DEFAULT 0 CHECK (held_out IN (0,1)),

  analysis_ver        INTEGER NOT NULL,
  created_at          TEXT    NOT NULL,

  -- A pair that was not verified can never be held out: holding out an unverified pair would put a
  -- pair the solver rejected into the set that judges it.
  CHECK (held_out = 0 OR verified = 1),

  -- A body is never paired with itself. The whole construct is a comparison between two cameras.
  CHECK (left_camera <> right_camera)
) STRICT;

CREATE INDEX idx_camera_pair_camera ON camera_pair(project_id, right_camera, flash, verified);
CREATE INDEX idx_camera_pair_node   ON camera_pair(project_id, node_id);
CREATE UNIQUE INDEX idx_camera_pair_frames ON camera_pair(left_image, right_image);

-- ---------------------------------------------------------------------------
-- How differently each photographer exposes
-- ---------------------------------------------------------------------------

CREATE TABLE camera_shooter_bias (
  project_id          TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  camera_id           TEXT    NOT NULL,

  -- The scene class the habit was measured in. Invariant 7: a second shooter who works darker
  -- during a ceremony may not during a reception, and one number for both is wrong twice.
  scene               TEXT    NOT NULL,

  -- The shooter's label, denormalised from `camera.shooter_label` so the report reads without a
  -- join and so a label a photographer later renames does not silently rewrite what was corrected.
  shooter             TEXT    NOT NULL,

  -- What was measured, and what was applied. Two columns rather than one, and that is the whole
  -- mechanism of section 6.3's cap: a report that only stored what was applied could not tell a
  -- photographer that their second shooter is two thirds of a stop darker and has been moved by a
  -- third of one.
  measured_ev         REAL    NOT NULL CHECK (measured_ev >= -6.0 AND measured_ev <= 6.0),
  applied_ev          REAL    NOT NULL CHECK (applied_ev  >=  -0.3 AND applied_ev  <=  0.3),

  frames              INTEGER NOT NULL CHECK (frames >= 0),
  capped              INTEGER NOT NULL DEFAULT 0 CHECK (capped IN (0,1)),
  reasons             INTEGER NOT NULL DEFAULT 0,
  analysis_ver        INTEGER NOT NULL,
  created_at          TEXT    NOT NULL,

  -- The applied correction may never exceed the measured habit. A correction larger than the thing
  -- it corrects is not a cap that failed, it is a sign error, and this is where it stops.
  CHECK (abs(applied_ev) <= abs(measured_ev) + 0.0001),

  PRIMARY KEY (project_id, camera_id, scene)
) STRICT, WITHOUT ROWID;

CREATE INDEX idx_camera_shooter_project ON camera_shooter_bias(project_id, shooter);

-- ---------------------------------------------------------------------------
-- Which body everything is matched to
-- ---------------------------------------------------------------------------

CREATE TABLE camera_reference (
  project_id          TEXT    NOT NULL PRIMARY KEY REFERENCES project(project_id) ON DELETE CASCADE,
  camera_id           TEXT    NOT NULL,

  -- How it was chosen. A photographer's choice beats a shooter label, which beats a frame count -
  -- section 2.1's three policies, in the order they are tried. The trigger below is what makes the
  -- first of the three durable.
  source              TEXT    NOT NULL CHECK (source IN ('user','primary_shooter','frame_count')),

  frames              INTEGER NOT NULL CHECK (frames >= 0),
  shooter             TEXT,
  analysis_ver        INTEGER NOT NULL,
  created_at          TEXT    NOT NULL,
  updated_at          TEXT    NOT NULL
) STRICT, WITHOUT ROWID;

-- ---------------------------------------------------------------------------
-- Two views: what a report leads with, and what a photographer should look at
-- ---------------------------------------------------------------------------

-- One row per body, ordered worst first: the bodies whose transforms rest on the least evidence,
-- and the skin difference each one is left with. What the per-camera report of section 2.1 renders
-- and what `aura-cli verify --phase 26` prints.
CREATE VIEW v_camera_evidence AS
SELECT
  t.project_id,
  t.camera_id,
  t.flash,
  t.source,
  t.blend,
  t.evidence_pairs,
  t.heldout_pairs,
  t.skin_de00_before,
  t.skin_de00_after,
  t.confidence,
  t.enabled,
  t.user_edited,
  f.samples          AS fingerprint_samples,
  f.confidence       AS fingerprint_confidence,
  f.brand            AS brand
FROM camera_transform t
LEFT JOIN camera_fingerprint f
       ON f.project_id = t.project_id
      AND f.camera_id  = t.camera_id
      AND f.flash      = t.flash
ORDER BY t.evidence_pairs ASC, t.skin_de00_after DESC;

-- The bodies matched from a bundled baseline alone, with the reason. Section 10.1: "with no
-- matched pairs, brand baselines are used and the report says so honestly" - this is the query
-- that makes "says so" checkable rather than a promise about a panel.
CREATE VIEW v_camera_unmatched AS
SELECT
  project_id,
  camera_id,
  flash,
  reference_id,
  evidence_pairs,
  reasons,
  skin_de00_after
FROM camera_transform
WHERE source = 'brand_baseline';

-- ---------------------------------------------------------------------------
-- Three triggers: what a photographer chose is not undone by automation
-- ---------------------------------------------------------------------------

-- A user-chosen reference is never replaced by an automatic one. A re-pass writes its own choice
-- with INSERT OR REPLACE, which deletes the row it conflicts with - so guarding the DELETE would
-- not be enough here. Phase 18's lesson, applied before it could bite.
CREATE TRIGGER camera_reference_keep_user
BEFORE UPDATE ON camera_reference
FOR EACH ROW WHEN OLD.source = 'user' AND NEW.source <> 'user'
BEGIN
  SELECT RAISE(ABORT, 'AURA-ML-5131: a reference camera the photographer chose is not replaced by automation');
END;

-- `user_edited` is never cleared by a re-solve, and neither are the values underneath it. A
-- photographer who set a camera's correction by hand has made a decision about their own wedding;
-- automation may propose beside it and never over it.
--
-- The one thing that clears it is a photographer choosing "reset to AI suggestion", and that path
-- is a DELETE followed by a re-solve rather than an UPDATE - deliberately, so that the only way to
-- undo a person's decision is to remove the row that records it. Phase 15 established the rule and
-- phase 25 the mechanism; this is where the two meet.
CREATE TRIGGER camera_transform_keep_user_edit
BEFORE UPDATE ON camera_transform
FOR EACH ROW WHEN OLD.user_edited = 1 AND NEW.user_edited = 0
BEGIN
  SELECT RAISE(ABORT, 'AURA-ML-5131: a camera correction the photographer set is not overwritten by automation');
END;

-- A pair's held-out flag is fixed the moment it is written.
--
-- This is the trigger that protects the phase's own verification. Section 6.2 checks a solved
-- transform against pairs the solver never saw, and the check is only worth anything if the split
-- is decided before the fit and cannot move afterwards. A held-out flag that could be flipped
-- would let a re-solve quietly promote the pairs that happened to agree with it, and section
-- 10.1's held-out gate would keep passing while measuring nothing. The split is deterministic by
-- pair id, it is written once, and it is immutable here.
--
-- A new split is a new pass: the pass DELETEs a project's pairs and re-forms them, which does not
-- fire this trigger, and every transform written afterwards carries the new `heldout_pairs` count.
CREATE TRIGGER camera_pair_heldout_is_fixed
BEFORE UPDATE ON camera_pair
FOR EACH ROW WHEN OLD.held_out <> NEW.held_out
BEGIN
  SELECT RAISE(ABORT, 'AURA-ML-5130: which pairs were held out of a fit is decided before the fit and never after it');
END;
