-- Migration 24 - what AURA proposed removing from a photograph, what it refused, and what it
-- actually took out.
--
-- PHASE-24 section 4 names no table: its file list is nine modules, three Python scripts, a
-- policy file, three panels and a public policy document. It needs three tables, two views and
-- four triggers anyway, and section 13's fifth acceptance criterion is why - "every cleanup is
-- disclosed in the recipe and the delivery report". A disclosure that lives only in a recipe is a
-- disclosure a photographer cannot list, and a disclosure nobody can query is a promise nobody can
-- check. `docs/adr/ADR-0049-generative-cleanup-and-the-safety-engine.md` records the rest.
--
-- ---------------------------------------------------------------------------
-- WHAT THIS MIGRATION IS FOR
-- ---------------------------------------------------------------------------
--
-- Twenty-three migrations recorded what AURA found, what it decided, how the pixels should move,
-- what was done to a person's appearance and what was repaired. **This one records what was taken
-- away**, and it is the first schema in the product whose most important rows are the ones where
-- nothing happened. Eight properties are enforced here rather than remembered:
--
-- 1. **A blocked candidate cannot become a proposal, and the database says so twice.**
--    `cleanup_proposal.checks` is five characters of `1` or `0` in `SafetyCheck::ALL` order, and a
--    CHECK constraint refuses any value but `'11111'`. `CleanupProposal::new` refuses it first;
--    this is the second layer, and it exists because the first lives in a constructor a future
--    caller could route around with a raw INSERT.
--
-- 2. **A person and an unnamed object can never be proposed, in any scene, at any confidence.**
--    A CHECK refuses `class IN ('background_person','unclassified')` on `cleanup_proposal`
--    outright. Section 2.2 makes removing a guest a manual confirmed action rather than an
--    automated one, and `DistractionClass::story_safe` is false for both - but that is a method on
--    an enum, and this is a constraint no statement can pass.
--
-- 3. **A borrow names its source, and an inpaint names its model.** Two CHECKs. Phase 21
--    established the shape for its glare borrow: a stored row that says a borrow happened without
--    saying where from is a disclosure that discloses nothing. The inpaint half is the one that
--    matters most in this build, because `CleanupMethod::Inpaint` in a row is supposed to mean a
--    diffusion model ran, and there is none.
--
-- 4. **A refusal is a row.** `cleanup_blocked` carries every candidate the safety engine, the
--    source selector, the self-check and the cloud judgement declined, with the check that
--    declined it and its reason code. More than half of `CleanupCode` is refusals, which is the
--    highest proportion in the product; a schema that recorded only removals would make
--    "AURA declined to touch this photograph" unprovable, and the adversarial audit of section
--    10.1 is scored from exactly these rows.
--
-- 5. **A disclosure cannot be edited or quietly removed.** `cleanup_disclosure` has no UPDATE path
--    at all - a trigger aborts every one - and a DELETE is refused while the proposal it belongs to
--    is still marked applied. Phase 13's ledger is append-only for the same reason and this is a
--    stronger form of it: there, a correction supersedes; here there is nothing to correct, because
--    the row is a statement about pixels that were replaced.
--
-- 6. **An applied removal has a disclosure, in the same transaction.** A trigger aborts the UPDATE
--    that sets `applied = 1` when no disclosure row exists. That is section 13's fifth acceptance
--    criterion as a constraint rather than as a convention.
--
-- 7. **A photographer's decision is never overwritten.** `cleanup_proposal.accepted` and
--    `disabled_by_user` are carried forward inside the statement a re-analysis would overwrite the
--    row with - the twelfth time this rule has been written into a migration, and the reason the
--    proposal id is derived from the region rather than issued fresh.
--
-- 8. **There is no image data and nowhere to put a prompt.** Phase 13's rule, tenth migration
--    running, with a second half this phase adds: no column here could hold a text description of
--    what should be generated. `docs/generative-policy.md` promises AURA never generates from a
--    description, and a schema with no field for one makes adding a field a visible contract
--    change rather than a feature.
--
-- ROLLBACK. Every object here is new and nothing outside this file references it:
--
--   DROP VIEW    IF EXISTS v_cleanup_disclosure;
--   DROP VIEW    IF EXISTS v_cleanup_coverage;
--   DROP TRIGGER IF EXISTS cleanup_applied_needs_disclosure;
--   DROP TRIGGER IF EXISTS cleanup_disclosure_no_delete;
--   DROP TRIGGER IF EXISTS cleanup_disclosure_no_update;
--   DROP TRIGGER IF EXISTS cleanup_proposal_no_person;
--   DROP TABLE   IF EXISTS cleanup_disclosure;
--   DROP TABLE   IF EXISTS cleanup_blocked;
--   DROP TABLE   IF EXISTS cleanup_proposal;
--   DROP TABLE   IF EXISTS cleanup_image;
--   DELETE FROM schema_version WHERE version = 24;
--
-- Running those returns the catalog to schema 23. **It is recomputable with two exceptions**: every
-- proposal is derived from pixels, phase 11's salience and phase 18's regions, so a re-run
-- reproduces them exactly and with the same identifiers - but rows carrying `accepted IS NOT NULL`
-- and images carrying `disabled_by_user = 1` are a photographer's own decisions and are derivable
-- from nothing. The rollback runbook says to export those first.
--
-- It is also the one rollback in the product that can change what a delivered photograph looks
-- like, because dropping `cleanup_disclosure` removes the record of a removal that is still in the
-- recipe. The runbook says to export the disclosures before the drop and to re-import them after,
-- and `aura-cli verify --phase 24` fails on a recipe carrying a cleanup operation with no
-- disclosure behind it.

-- ---------------------------------------------------------------------------
-- One row per photograph the pass has examined. See note 7.
--
-- Separate from `cleanup_proposal` because "AURA looked at this photograph and found nothing to
-- tidy" and "AURA has not looked at this photograph" are different statements that are delivered
-- identically, and every outline since phase 09 has needed the difference. It is also where a
-- photographer's "leave this one alone" lives, which must survive every re-analysis.
-- ---------------------------------------------------------------------------
CREATE TABLE cleanup_image (
  photo_id            TEXT    NOT NULL PRIMARY KEY REFERENCES photo(photo_id) ON DELETE CASCADE,
  project_id          TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,

  -- The scene it was examined under. Invariant 7, and stored rather than re-read: a
  -- re-classification changes which policy row applied, and a proposal that silently starts
  -- describing itself under a new scene is a proposal nobody can audit.
  scene               TEXT    NOT NULL,

  -- See `Coverage::is_complete`. **The number to read first on a project.** At zero, every
  -- candidate on this photograph was blocked for want of evidence rather than for want of safety,
  -- and the blocked histogram says nothing about what is in the photograph. Phase 18's twenty mask
  -- classes contain no word for a ring or a cake, so this is zero on every frame in this build.
  mask_complete       INTEGER NOT NULL DEFAULT 0 CHECK (mask_complete IN (0,1)),

  -- A photographer's own switch. Never cleared by automation; carried forward inside the upsert.
  disabled_by_user    INTEGER NOT NULL DEFAULT 0 CHECK (disabled_by_user IN (0,1)),

  -- How many cloud editorial judgements this photograph cost, and how many said no. Stored per
  -- photograph rather than per project so the twenty-call ceiling in section 7 can be enforced by
  -- a SUM rather than by a counter somebody has to keep.
  judged              INTEGER NOT NULL DEFAULT 0 CHECK (judged >= 0),
  declined            INTEGER NOT NULL DEFAULT 0 CHECK (declined >= 0 AND declined <= judged),

  -- Removals the self-check undid before anybody saw them. Section 11's `cleanup.reverted`.
  reverted            INTEGER NOT NULL DEFAULT 0 CHECK (reverted >= 0),

  -- The three versions. A change to any of them makes this row pending again, which is what makes
  -- the resumable pass a query rather than a journal. Invariant 5.
  detector_ver        INTEGER NOT NULL DEFAULT 0,
  analysis_ver        INTEGER NOT NULL DEFAULT 0,
  policy_ver          INTEGER NOT NULL DEFAULT 0,

  examined_at         TEXT    NOT NULL
) STRICT;

CREATE INDEX idx_cleanup_image_project ON cleanup_image(project_id, mask_complete);

-- ---------------------------------------------------------------------------
-- One row per proposal, applied or not. See notes 1, 2, 3 and 7.
-- ---------------------------------------------------------------------------
CREATE TABLE cleanup_proposal (
  proposal_id         TEXT    NOT NULL PRIMARY KEY,
  photo_id            TEXT    NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,
  project_id          TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,

  -- Where, normalised to the frame. Four columns rather than a blob, because the delivery report,
  -- the panel and phase 27's QC agent all filter on position and a blob cannot be indexed.
  x                   REAL    NOT NULL CHECK (x >= 0.0 AND x <= 1.0),
  y                   REAL    NOT NULL CHECK (y >= 0.0 AND y <= 1.0),
  w                   REAL    NOT NULL CHECK (w >  0.0 AND w <= 1.0),
  h                   REAL    NOT NULL CHECK (h >  0.0 AND h <= 1.0),

  -- See note 2. The constraint, not the convention.
  class               TEXT    NOT NULL
                      CHECK (class NOT IN ('background_person','unclassified')),

  area_frac           REAL    NOT NULL CHECK (area_frac >= 0.0 AND area_frac <= 1.0),
  salience            REAL    NOT NULL DEFAULT 0.0 CHECK (salience >= 0.0 AND salience <= 1.0),
  confidence          REAL    NOT NULL DEFAULT 0.0 CHECK (confidence >= 0.0 AND confidence <= 1.0),

  -- See note 3. The method, then what it needs to be honest about itself. The three CHECKs that
  -- tie them together are at the foot of the table: SQLite takes every table-level constraint
  -- after the last column, and one in the middle is a syntax error at the next column.
  method_kind         TEXT    NOT NULL CHECK (method_kind IN ('borrow','fill','inpaint')),
  method_source       TEXT             REFERENCES photo(photo_id) ON DELETE RESTRICT,
  method_model        TEXT,

  -- See note 1. Five characters, one per `SafetyCheck::ALL`, and only one value is storable.
  checks              TEXT    NOT NULL DEFAULT '11111' CHECK (checks = '11111'),

  -- Phase 13's band, raised one for this phase and raised again while nothing is calibrated.
  autonomy            TEXT    NOT NULL DEFAULT 'require_review'
                      CHECK (autonomy IN ('auto','auto_zero_touch','suggest','require_review')),

  scene               TEXT    NOT NULL,

  -- The reason codes, comma separated in weight order. Codes rather than sentences: phase 09's
  -- decision, and a catalog full of English cannot be translated.
  reasons             TEXT    NOT NULL DEFAULT '',

  -- What the self-check measured on the result, `0..1`, lower is cleaner. Stored on every
  -- proposal, including the ones nobody applied, because it is the evidence behind section 10.1's
  -- artefact-free gate and a figure that only existed for applied rows would measure the wrong set.
  artefact_score      REAL    NOT NULL DEFAULT 0.0
                      CHECK (artefact_score >= 0.0 AND artefact_score <= 1.0),

  -- See note 7. NULL is undecided, 1 is accepted, 0 is rejected. Never overwritten by automation.
  accepted            INTEGER          CHECK (accepted IS NULL OR accepted IN (0,1)),

  -- See note 6. Only a trigger-checked UPDATE may set this.
  applied             INTEGER NOT NULL DEFAULT 0 CHECK (applied IN (0,1)),

  detector_ver        INTEGER NOT NULL DEFAULT 0,
  analysis_ver        INTEGER NOT NULL DEFAULT 0,
  policy_ver          INTEGER NOT NULL DEFAULT 0,
  proposed_at         TEXT    NOT NULL,

  -- See note 3. A borrow names its source, an inpaint names its model, and nothing but a borrow
  -- carries a source. Three constraints rather than one, so a violated row says which promise it
  -- broke rather than that it broke one of three.
  CHECK (method_kind <> 'borrow'  OR method_source IS NOT NULL),
  CHECK (method_kind <> 'inpaint' OR method_model  IS NOT NULL),
  CHECK (method_kind =  'borrow'  OR method_source IS NULL)
) STRICT;

CREATE INDEX idx_cleanup_proposal_photo ON cleanup_proposal(photo_id, confidence DESC);
CREATE INDEX idx_cleanup_proposal_project ON cleanup_proposal(project_id, applied);
-- The review queue: undecided proposals, strongest first. The one query the panel makes on every
-- keystroke, so it is the one index that is not merely a good idea.
CREATE INDEX idx_cleanup_proposal_queue ON cleanup_proposal(project_id, confidence DESC)
  WHERE accepted IS NULL AND applied = 0;

-- ---------------------------------------------------------------------------
-- One row per candidate that was refused. See note 4.
--
-- WITHOUT ROWID and keyed on `(photo_id, seq)`, phase 09's shape for `face_eye_state`: these rows
-- are read only in bulk for one photograph or counted for one project, they carry no identity of
-- their own, and there are several of them for every proposal that survives.
-- ---------------------------------------------------------------------------
CREATE TABLE cleanup_blocked (
  photo_id            TEXT    NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,
  seq                 INTEGER NOT NULL CHECK (seq >= 0),

  x                   REAL    NOT NULL CHECK (x >= 0.0 AND x <= 1.0),
  y                   REAL    NOT NULL CHECK (y >= 0.0 AND y <= 1.0),
  w                   REAL    NOT NULL CHECK (w >  0.0 AND w <= 1.0),
  h                   REAL    NOT NULL CHECK (h >  0.0 AND h <= 1.0),

  -- Which of the five checks stopped it, in `SafetyCheck::ALL`'s own slugs.
  failed_check        TEXT    NOT NULL
                      CHECK (failed_check IN
                             ('size_cap','denylist','identity_protect','structure_span','confidence')),

  -- Which reason code. Not constrained to a list here: `CleanupCode` has thirty-one variants and a
  -- CHECK naming sixteen of them would be a second copy of `is_refusal` that could drift from the
  -- enum. `aura-cli verify --phase 24` reads every stored code back through `CleanupCode::parse`,
  -- which is the check that cannot drift.
  code                TEXT    NOT NULL,

  PRIMARY KEY (photo_id, seq)
) WITHOUT ROWID;

CREATE INDEX idx_cleanup_blocked_check ON cleanup_blocked(failed_check, code);

-- ---------------------------------------------------------------------------
-- One row per removal that actually happened. See notes 3, 5 and 6.
--
-- The delivery report is `SELECT * FROM v_cleanup_disclosure WHERE project_id = ?`, and that is
-- the whole point of the table: a photographer hands their client a gallery and has to be able to
-- say, without opening a single file, exactly what was taken out of it.
-- ---------------------------------------------------------------------------
CREATE TABLE cleanup_disclosure (
  proposal_id         TEXT    NOT NULL PRIMARY KEY
                      REFERENCES cleanup_proposal(proposal_id) ON DELETE CASCADE,
  photo_id            TEXT    NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,
  project_id          TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,

  method_kind         TEXT    NOT NULL CHECK (method_kind IN ('borrow','fill','inpaint')),
  method_source       TEXT             REFERENCES photo(photo_id) ON DELETE RESTRICT,
  method_model        TEXT,

  x                   REAL    NOT NULL CHECK (x >= 0.0 AND x <= 1.0),
  y                   REAL    NOT NULL CHECK (y >= 0.0 AND y <= 1.0),
  w                   REAL    NOT NULL CHECK (w >  0.0 AND w <= 1.0),
  h                   REAL    NOT NULL CHECK (h >  0.0 AND h <= 1.0),

  -- True when a person accepted it rather than a mode applying it. The column a client's question
  -- is answered from, and the reason `CleanupDisclosure` carries it rather than deriving it.
  accepted_by_user    INTEGER NOT NULL CHECK (accepted_by_user IN (0,1)),

  artefact_score      REAL    NOT NULL
                      CHECK (artefact_score >= 0.0 AND artefact_score <= 1.0),

  applied_at          TEXT    NOT NULL,

  -- See note 3, and see `cleanup_proposal`'s foot for why these sit here rather than beside the
  -- columns they constrain.
  CHECK (method_kind <> 'borrow'  OR method_source IS NOT NULL),
  CHECK (method_kind <> 'inpaint' OR method_model  IS NOT NULL)
) STRICT;

CREATE INDEX idx_cleanup_disclosure_project ON cleanup_disclosure(project_id, method_kind);

-- ---------------------------------------------------------------------------
-- See note 5. A disclosure is a statement about pixels that were replaced. There is nothing to
-- correct in it, so there is no UPDATE path at all.
--
-- There is no code path in `aura-generative` that attempts this, which is the point: it catches the
-- path somebody adds in phase 27, 28 or 30 without reading ADR-0049.
-- ---------------------------------------------------------------------------
CREATE TRIGGER cleanup_disclosure_no_update
BEFORE UPDATE ON cleanup_disclosure
BEGIN
  SELECT RAISE(ABORT, 'a cleanup disclosure is a record of pixels that were replaced and is never edited: write a new proposal instead');
END;

-- See note 5. The removal may be undone - by rejecting the proposal, which clears `applied` - and
-- only then may its disclosure go.
CREATE TRIGGER cleanup_disclosure_no_delete
BEFORE DELETE ON cleanup_disclosure
WHEN EXISTS (SELECT 1 FROM cleanup_proposal p
              WHERE p.proposal_id = OLD.proposal_id AND p.applied = 1)
BEGIN
  SELECT RAISE(ABORT, 'a removal that is still applied may not lose its disclosure: undo the removal first');
END;

-- See note 6. Section 13's fifth acceptance criterion, as a constraint.
CREATE TRIGGER cleanup_applied_needs_disclosure
BEFORE UPDATE OF applied ON cleanup_proposal
WHEN NEW.applied = 1
 AND NOT EXISTS (SELECT 1 FROM cleanup_disclosure d WHERE d.proposal_id = NEW.proposal_id)
BEGIN
  SELECT RAISE(ABORT, 'a cleanup may not be applied without a disclosure: write the disclosure in the same transaction');
END;

-- See note 2, from the other direction. The CHECK stops a person being INSERTed as a proposal;
-- this stops one being UPDATEd into an existing row, which is the shape a "reclassify this
-- candidate" feature would have.
CREATE TRIGGER cleanup_proposal_no_person
BEFORE UPDATE OF class ON cleanup_proposal
WHEN NEW.class IN ('background_person','unclassified')
BEGIN
  SELECT RAISE(ABORT, 'removing a person is a manual, confirmed action and is never a proposal');
END;

-- ---------------------------------------------------------------------------
-- Coverage, in the shape every outline since phase 09 reports it.
-- ---------------------------------------------------------------------------
CREATE VIEW v_cleanup_coverage AS
SELECT
  p.project_id                                                          AS project_id,
  COUNT(*)                                                              AS photos,
  SUM(CASE WHEN c.photo_id IS NOT NULL THEN 1 ELSE 0 END)               AS examined,
  SUM(CASE WHEN c.mask_complete = 1 THEN 1 ELSE 0 END)                  AS mask_complete,
  SUM(CASE WHEN c.disabled_by_user = 1 THEN 1 ELSE 0 END)               AS disabled,
  SUM(COALESCE(c.reverted, 0))                                          AS reverted,
  SUM(COALESCE(c.judged, 0))                                            AS judged,
  SUM(COALESCE(c.declined, 0))                                          AS declined,
  SUM(CASE WHEN EXISTS (SELECT 1 FROM cleanup_proposal q
                         WHERE q.photo_id = p.photo_id) THEN 1 ELSE 0 END)
                                                                        AS with_proposals,
  (SELECT COUNT(*) FROM cleanup_proposal q
    WHERE q.project_id = p.project_id AND q.applied = 1)                AS applied,
  (SELECT COUNT(*) FROM cleanup_proposal q
    WHERE q.project_id = p.project_id AND q.applied = 1
      AND q.method_kind = 'borrow')                                     AS borrowed,
  (SELECT COUNT(*) FROM cleanup_proposal q
    WHERE q.project_id = p.project_id AND q.applied = 1
      AND q.method_kind = 'fill')                                       AS filled,
  (SELECT COUNT(*) FROM cleanup_proposal q
    WHERE q.project_id = p.project_id AND q.applied = 1
      AND q.method_kind = 'inpaint')                                    AS inpainted,
  (SELECT COUNT(*) FROM cleanup_blocked b
     JOIN photo ph ON ph.photo_id = b.photo_id
    WHERE ph.project_id = p.project_id AND b.failed_check = 'size_cap')         AS blocked_size_cap,
  (SELECT COUNT(*) FROM cleanup_blocked b
     JOIN photo ph ON ph.photo_id = b.photo_id
    WHERE ph.project_id = p.project_id AND b.failed_check = 'denylist')         AS blocked_denylist,
  (SELECT COUNT(*) FROM cleanup_blocked b
     JOIN photo ph ON ph.photo_id = b.photo_id
    WHERE ph.project_id = p.project_id AND b.failed_check = 'identity_protect') AS blocked_identity,
  (SELECT COUNT(*) FROM cleanup_blocked b
     JOIN photo ph ON ph.photo_id = b.photo_id
    WHERE ph.project_id = p.project_id AND b.failed_check = 'structure_span')   AS blocked_structure,
  (SELECT COUNT(*) FROM cleanup_blocked b
     JOIN photo ph ON ph.photo_id = b.photo_id
    WHERE ph.project_id = p.project_id AND b.failed_check = 'confidence')       AS blocked_confidence
FROM photo p
LEFT JOIN cleanup_image c ON c.photo_id = p.photo_id
GROUP BY p.project_id;

-- ---------------------------------------------------------------------------
-- The delivery report, as a view, because it is asked by four callers: the panel, the delivery
-- report itself, phase 27's QC agent and the phase gate. A list four callers assemble separately
-- is a list they will eventually disagree about.
-- ---------------------------------------------------------------------------
CREATE VIEW v_cleanup_disclosure AS
SELECT
  d.project_id                                                   AS project_id,
  d.photo_id                                                     AS photo_id,
  d.proposal_id                                                  AS proposal_id,
  d.method_kind                                                  AS method_kind,
  d.method_source                                                AS method_source,
  d.method_model                                                 AS method_model,
  q.class                                                        AS class,
  d.x                                                            AS x,
  d.y                                                            AS y,
  d.w                                                            AS w,
  d.h                                                            AS h,
  d.accepted_by_user                                             AS accepted_by_user,
  d.artefact_score                                               AS artefact_score,
  q.confidence                                                   AS confidence,
  q.autonomy                                                     AS autonomy,
  q.reasons                                                      AS reasons,
  d.applied_at                                                   AS applied_at
FROM cleanup_disclosure d
JOIN cleanup_proposal q ON q.proposal_id = d.proposal_id;
