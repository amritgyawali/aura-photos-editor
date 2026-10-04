-- Migration 31 - a look somebody else published, and what matching it did.
--
-- PHASE-31 section 4. Four tables, two views and three triggers.
--
-- `docs/adr/ADR-0063-reference-look-matching.md` records the decisions behind the columns and
-- `ADR-0064` the ones behind what reaches a panel.
--
-- ---------------------------------------------------------------------------
-- WHAT THIS MIGRATION IS FOR
-- ---------------------------------------------------------------------------
--
-- Migration 17 recorded how far *this* photographer sits from the consensus, learned from their
-- own delivered weddings. This one records how far a **reference** sits from it - a page of
-- finished photographs somebody else made - and what happened when a project was moved toward
-- it.
--
-- Six properties are enforced here rather than remembered:
--
-- 1. **There is no skin colour anywhere in this file, and this time there is no field a
--    difference could live in either.** Migrations 15, 16, 17 and 25 each said it; this is the
--    fifth, and it is the version where the trap is hardest to see, because measuring a
--    stranger's skin from a stranger's JPEG requires declaring a hue window and calling what
--    falls inside it skin - which is the fixed skin constant this product has refused four
--    times, wearing a measurement's clothes. Phase 17's `skin_warmth_deg` and `skin_chroma`
--    have no counterpart here. The phase gate scans this schema for one on every run.
--
-- 2. **No pixels, and no path that leaves the machine.** `look_reference` stores a content hash
--    and a file name. There is no blob column, no thumbnail, no crop and no URL that anything
--    fetches - `look_profile.origin_key` is a LABEL, it is never dereferenced, and
--    `aura-look` depends on no cloud crate, which is the other half of the same statement.
--
-- 3. **A look stores what it was a difference FROM, not just the difference.** Every bucket
--    keeps the reference aggregate and the baseline aggregate beside the delta they were
--    subtracted to make. A delta on its own is a number a photographer cannot argue with; three
--    numbers is a claim somebody can check, and phase 16's rule has a second half - the
--    measurement is kept.
--
-- 4. **A match figure is measured or it is absent.** `look_match.after_de00` is NOT NULL and a
--    row exists only once `verify::measure` has run. There is no default, no zero and no
--    "pending" value, because a zero in a dE00 column renders in every panel in this product as
--    a perfect match. Phase 17's own comment about `match_de00` being an `Option` is the same
--    decision one layer up.
--
-- 5. **What a project was told cannot be rewritten.** `look_match_no_update` aborts every
--    UPDATE, exactly as phase 28's `autopilot_run_no_reopen` and phase 30's
--    `delivery_manifest_no_update` do. Re-measuring writes a new row; the old one is the record
--    of what a photographer was shown on the day they looked at it.
--
-- 6. **There is no sentence stored anywhere.** Every reason is a code in `look_reason`, and the
--    English is rendered from `LookCode::sentence` at the moment a panel asks. Phase 09 wrote
--    the rule and phase 27 wrote its conclusion: a stored sentence is copy a release has to
--    maintain and a catalog full of English cannot be translated. The gate scans for one, with
--    comments stripped first - phase 27 found this check matching its own documentation twice.

-- ---------------------------------------------------------------------------
-- The look itself
-- ---------------------------------------------------------------------------

CREATE TABLE look_profile (
  id                TEXT PRIMARY KEY,
  name              TEXT NOT NULL,
  -- `instagram:handle`, `web:url` or `local:label`. A label, never dereferenced. See note 2.
  origin_key        TEXT NOT NULL,
  -- How the files arrived: `folder`, `instagram_export` or `public_url`. Stored separately from
  -- the origin because they are two facts - a photographer who exports a page they admire and
  -- points AURA at the folder has an Instagram origin and a folder source, and a report that
  -- could only say "a folder" would have lost the thing they want to see.
  source            TEXT NOT NULL,
  -- The global lean, as the canonical JSON of a `StyleDelta`.
  global_delta      TEXT NOT NULL,
  references_used   INTEGER NOT NULL,
  references_found  INTEGER NOT NULL,
  references_refused INTEGER NOT NULL,
  baseline_frames   INTEGER NOT NULL,
  strength          REAL NOT NULL,
  measured_at       INTEGER NOT NULL,
  -- Which renderer the match was measured against, and which measurer produced the readings.
  -- Two columns because they invalidate two different things: a new renderer invalidates the
  -- match figure, a new measurer invalidates every stored aggregate. `AURA-ML-5148` is what
  -- stops a comparison across either from happening silently.
  engine_ver        TEXT NOT NULL,
  analysis_ver      INTEGER NOT NULL,
  CHECK (references_used >= 0),
  CHECK (strength >= 0.0 AND strength <= 1.0)
) STRICT;

CREATE INDEX idx_look_profile_measured ON look_profile(measured_at DESC);

-- ---------------------------------------------------------------------------
-- One kind of light
-- ---------------------------------------------------------------------------

CREATE TABLE look_bucket (
  profile_id        TEXT NOT NULL REFERENCES look_profile(id) ON DELETE CASCADE,
  -- One of `LightingBucket`'s ten slugs. There is no scene column and there is deliberately no
  -- room for one: a reference photograph does not say whether it is a ceremony or a reception,
  -- and a column here would be a place for somebody to later put a guess.
  lighting          TEXT NOT NULL,
  reference_json    TEXT NOT NULL,
  baseline_json     TEXT NOT NULL,
  delta_json        TEXT NOT NULL,
  samples           INTEGER NOT NULL,
  confidence        REAL NOT NULL,
  PRIMARY KEY (profile_id, lighting),
  CHECK (samples >= 0),
  CHECK (confidence >= 0.0 AND confidence <= 1.0)
) STRICT, WITHOUT ROWID;

-- ---------------------------------------------------------------------------
-- What was measured, and what was refused
-- ---------------------------------------------------------------------------

CREATE TABLE look_reference (
  profile_id        TEXT NOT NULL REFERENCES look_profile(id) ON DELETE CASCADE,
  -- The content hash. Content rather than path, so the same photograph in two folders gets one
  -- vote, and so a folder whose files were renamed measures the same.
  content_hash      TEXT NOT NULL,
  -- The file's own name, for a report a photographer can act on. Never a full path: a report
  -- that named `/Users/somebody/...` is a report that cannot be attached to a support case.
  file_name         TEXT NOT NULL,
  lighting          TEXT NOT NULL,
  lighting_confidence REAL NOT NULL,
  reading_json      TEXT NOT NULL,
  PRIMARY KEY (profile_id, content_hash)
) STRICT, WITHOUT ROWID;

-- ---------------------------------------------------------------------------
-- Which project uses which look, and how strongly
-- ---------------------------------------------------------------------------

CREATE TABLE project_look (
  project_id        TEXT PRIMARY KEY REFERENCES project(project_id) ON DELETE CASCADE,
  profile_id        TEXT REFERENCES look_profile(id) ON DELETE SET NULL,
  -- `0..1`. A photographer may apply less of a look and may never apply more; the contract's
  -- `LookOverride::Strength` clamps on construction and this CHECK is the second lock. Phase
  -- 21's rule - a ceiling can be lowered by a studio and raised by nobody.
  strength          REAL NOT NULL DEFAULT 1.0,
  selected_at       INTEGER NOT NULL,
  CHECK (strength >= 0.0 AND strength <= 1.0)
) STRICT;

-- ---------------------------------------------------------------------------
-- What the match actually did
-- ---------------------------------------------------------------------------

CREATE TABLE look_match (
  id                INTEGER PRIMARY KEY AUTOINCREMENT,
  project_id        TEXT NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  profile_id        TEXT NOT NULL REFERENCES look_profile(id) ON DELETE CASCADE,
  -- Both sides, always. `before_de00` is what makes `realised_share` computable, and without it
  -- a report can only say where a gallery ended up - which phase 27's rule calls no result at
  -- all.
  before_de00       REAL NOT NULL,
  after_de00        REAL NOT NULL,
  frames            INTEGER NOT NULL,
  user_edited       INTEGER NOT NULL,
  buckets_json      TEXT NOT NULL,
  measured_at       INTEGER NOT NULL,
  engine_ver        TEXT NOT NULL,
  CHECK (frames >= 0),
  CHECK (user_edited >= 0),
  CHECK (before_de00 >= 0.0 AND after_de00 >= 0.0)
) STRICT;

CREATE INDEX idx_look_match_project ON look_match(project_id, measured_at DESC);

-- ---------------------------------------------------------------------------
-- Reasons
-- ---------------------------------------------------------------------------

CREATE TABLE look_reason (
  profile_id        TEXT NOT NULL REFERENCES look_profile(id) ON DELETE CASCADE,
  ordinal           INTEGER NOT NULL,
  -- A `LookCode` slug. There is no `sentence` column and there is not going to be one: see
  -- note 6.
  code              TEXT NOT NULL,
  value             REAL,
  threshold         REAL,
  PRIMARY KEY (profile_id, ordinal)
) STRICT, WITHOUT ROWID;

-- ---------------------------------------------------------------------------
-- Triggers
-- ---------------------------------------------------------------------------

-- A record of what a photographer was told cannot be edited. Note 5.
CREATE TRIGGER look_match_no_update
BEFORE UPDATE ON look_match
BEGIN
  SELECT RAISE(ABORT, 'a look match is a record of what was measured; re-measure instead');
END;

-- A look that was never measured cannot be selected. The panel offers only measured looks, and
-- this is the lock underneath that: a project pointed at an unmeasured look would apply a delta
-- whose effect nobody has checked, which is the one thing phase 22's rule forbids.
CREATE TRIGGER project_look_needs_a_match
BEFORE INSERT ON project_look
WHEN NEW.profile_id IS NOT NULL
  AND NOT EXISTS (SELECT 1 FROM look_match WHERE profile_id = NEW.profile_id)
BEGIN
  SELECT RAISE(ABORT, 'that look has not been measured on any project yet');
END;

-- The same lock on the way in through an update, because a selection is changed far more often
-- than it is created and a trigger on INSERT alone guards the rarer path.
CREATE TRIGGER project_look_update_needs_a_match
BEFORE UPDATE OF profile_id ON project_look
WHEN NEW.profile_id IS NOT NULL
  AND NOT EXISTS (SELECT 1 FROM look_match WHERE profile_id = NEW.profile_id)
BEGIN
  SELECT RAISE(ABORT, 'that look has not been measured on any project yet');
END;

-- ---------------------------------------------------------------------------
-- Views
-- ---------------------------------------------------------------------------

-- What the panel lists: every look with the best match anybody has measured for it.
CREATE VIEW look_catalogue AS
SELECT
  p.id,
  p.name,
  p.origin_key,
  p.source,
  p.references_used,
  p.strength,
  p.measured_at,
  (SELECT MIN(m.after_de00) FROM look_match m WHERE m.profile_id = p.id) AS best_de00,
  (SELECT COUNT(*) FROM look_bucket b WHERE b.profile_id = p.id) AS buckets
FROM look_profile p;

-- "Did matching a look actually move the gallery" as a query rather than as a sentence.
-- Phase 25's rule: a promise that can only be asserted has no way of finding out it has stopped
-- being true.
CREATE VIEW look_match_effect AS
SELECT
  m.project_id,
  m.profile_id,
  m.before_de00,
  m.after_de00,
  m.frames,
  CASE
    WHEN m.before_de00 <= 0.0001 THEN 1.0
    ELSE MAX(0.0, MIN(1.0, (m.before_de00 - m.after_de00) / m.before_de00))
  END AS realised_share
FROM look_match m;
