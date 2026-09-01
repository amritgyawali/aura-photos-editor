-- Migration 27 - what quality control found in a delivered gallery, what it tried, what worked,
-- what it put back, and what a photographer thought of the finding.
--
-- PHASE-27 section 4 names no tables at all: it names `crates/aura-qc/src/*` and one config file,
-- and section 5 freezes three shapes whose persistence it leaves entirely open. What ships is four
-- tables, singular and prefixed like every table since migration 09, plus two views and four
-- triggers. `docs/adr/ADR-0055-quality-control-tickets-and-the-re-edit-loop.md` records the rest.
--
-- ---------------------------------------------------------------------------
-- WHAT THIS MIGRATION IS FOR
-- ---------------------------------------------------------------------------
--
-- Twenty-four migrations recorded a fact about **one photograph**. Migration 25 recorded a fact
-- about a **set** of them and migration 26 about a **body**. This one records a fact about
-- **AURA's own earlier decisions**, and that is a fourth kind of subject with a property none of
-- the first three has: a row here can outlive the thing it is about.
--
-- A frame replaced by its runner-up leaves a ticket that must keep pointing at the replacement it
-- caused. A remedy that was reverted leaves a ticket whose entire value is the record that the
-- product tried something and put it back. And a ticket a photographer dismissed has to survive
-- every future pass, because a finding somebody has rejected that comes back next week is a
-- product arguing with its user.
--
-- Six properties are enforced here rather than remembered:
--
-- 1. **A finding is a number against a threshold, and the schema cannot express an opinion.**
--    `deviation`, `threshold` and `unit` are all NOT NULL, `threshold > 0` is a CHECK, and there is
--    no free-text column a diagnosis could be typed into. Section 6.1: a ticket is actionable,
--    testable and explainable exactly when it says how far something is from what.
--
-- 2. **The sentence is not stored.** There is no `diagnosis` column. `QcTicket::render_diagnosis`
--    builds it from `code`, `deviation`, `threshold` and the evidence on read. Phase 09's rule -
--    a reason stores its code rather than its copy - and it matters more here than anywhere: a QC
--    ticket is the most user-facing sentence the product produces, so it is the one most likely to
--    be rewritten between releases, and a studio archiving reports would otherwise hold two
--    weddings whose identical findings read differently. ADR-0055 section 3.
--
-- 3. **Both deviations are on every round row.** `deviation_before` and `deviation_after`, never a
--    single "improved" boolean, because section 6.3's loop decides on the share of a *predicted*
--    gain that was realised and a boolean cannot be re-derived. It is also what makes the bound
--    auditable: "the product tried twice and gave up" and "the product tried once and it worked"
--    are two different query results rather than the same one. ADR-0055 section 4.
--
-- 4. **A replacement row exists only for a swap that happened, and it carries the coverage proof.**
--    `coverage_held` is CHECKed to 1. A swap that would have broken coverage is not a worse
--    candidate - it is not a candidate - so it leaves a `replacement_breaks_coverage` reason on the
--    ticket and no row here. The column is not redundant with the CHECK: it makes "no replacement
--    broke coverage" a `SELECT MIN(coverage_held)` rather than a claim about a function nobody can
--    inspect afterwards. Phase 16's rule, seventh application.
--
-- 5. **A photographer's verdict is immutable and automation cannot reach it.** `accepted` and
--    `dismissed` are the two statuses a person owns; `qc_ticket_keep_user_status` aborts any
--    UPDATE that would move a ticket out of one of them, and `QcStore::sweep` excludes them from
--    re-analysis. Migrations 06, 07, 08, 18, 25 and 26 all needed the same guard; here the
--    disagreement is about a *judgement* rather than a value, which is the one case where the
--    product has the strongest temptation to be right anyway.
--
-- 6. **A round can never be edited or deleted while its ticket lives.** Two triggers, and this is
--    the second append-only table in the product after migration 13's ledger. Section 6.3 asks
--    that "all rounds are recorded so the history of an image's edit is fully reconstructable",
--    and a history a later pass can rewrite is not one.
--
-- ---------------------------------------------------------------------------
-- STORAGE
-- ---------------------------------------------------------------------------
--
-- Section 11 sets no per-image storage budget for this phase. `perf/budgets.toml` carries one
-- anyway, measured rather than estimated, because phase 21 learned that a per-image figure written
-- before it is measured is wrong - and this phase has phase 21's shape rather than phase 09's:
-- every migration from 09 to 26 stores **one fixed-width verdict** per photograph, and this stores
-- a **list** whose length is the number of things that were wrong with the frame.
--
-- Two structural bounds keep that list short. `MAX_TICKETS_PER_IMAGE` is eight - above it the image
-- escalates whole rather than accumulating a ninth row - and `MAX_ROUNDS` is two, so a ticket can
-- own at most two round rows. The denominator is also **selected frames** rather than photographs,
-- because a QC check over a frame nobody is delivering is not an inspection anybody asked for.
-- Phase 18 established that denominator; `crates/aura-perf/tests/qc_budgets.rs` measures against it
-- on every run and asserts the bound as well as the number, which is the assertion phase 26 had to
-- add after a size test alone would have passed on a build with its cap removed.
--
-- ---------------------------------------------------------------------------
-- ROLLBACK
-- ---------------------------------------------------------------------------
--
-- Reversible: DROP the four tables, the two views and the four triggers. Nothing here alters an
-- earlier table and nothing earlier references these, so a downgrade loses this phase's findings
-- and no other. A gallery whose frames were replaced by this phase keeps the replacements - they
-- live in `selection_keep` where phase 12 owns them - and loses the record of why, which is a
-- worse outcome than losing both and is why `qc_replacement` is the row a support case reads.

-- ---------------------------------------------------------------------------
-- The pass
-- ---------------------------------------------------------------------------

CREATE TABLE qc_run (
  project_id          TEXT    NOT NULL PRIMARY KEY REFERENCES project(project_id) ON DELETE CASCADE,

  -- Frames in the delivered gallery when the pass started, and how many it actually reached. Two
  -- columns rather than one because section 11 gives the pass a wall-clock budget, and a pass that
  -- inspected 800 of 1,000 frames and reported no problems has not reported that the gallery is
  -- clean. ADR-0055 section 8, and `QcReport::complete` is the query.
  images              INTEGER NOT NULL CHECK (images >= 0),
  images_unreached    INTEGER NOT NULL DEFAULT 0 CHECK (images_unreached >= 0),

  -- Inspections that ran, and inspections that could not. The second is the number that makes the
  -- first honest: a category with zero findings and four hundred skips is not a clean category.
  checks_run          INTEGER NOT NULL CHECK (checks_run >= 0),
  checks_skipped      INTEGER NOT NULL DEFAULT 0 CHECK (checks_skipped >= 0),

  -- What the loop did.
  found               INTEGER NOT NULL DEFAULT 0 CHECK (found >= 0),
  fixed               INTEGER NOT NULL DEFAULT 0 CHECK (fixed >= 0),
  reverted            INTEGER NOT NULL DEFAULT 0 CHECK (reverted >= 0),
  escalated           INTEGER NOT NULL DEFAULT 0 CHECK (escalated >= 0),
  replaced            INTEGER NOT NULL DEFAULT 0 CHECK (replaced >= 0),

  -- Per-category tallies, as a JSON array of ten objects in `QcCategory::ALL` order. JSON rather
  -- than forty columns for the reason phase 16 stores its bands that way: they are read as a unit
  -- to render one table and nobody filters on `fixed_sharpness`.
  by_category         TEXT    NOT NULL,

  -- Planner calls made, bounded by the contract's own ceiling. `cloud_used = 0` with
  -- `planner_calls = 0` is a pass that never needed a second opinion; `cloud_used = 1` with
  -- `planner_calls = 0` is impossible and the CHECK says so.
  planner_calls       INTEGER NOT NULL DEFAULT 0 CHECK (planner_calls >= 0 AND planner_calls <= 40),
  cloud_used          INTEGER NOT NULL DEFAULT 0 CHECK (cloud_used IN (0,1)),

  duration_ms         INTEGER NOT NULL CHECK (duration_ms >= 0),

  thresholds_ver      INTEGER NOT NULL,
  analysis_ver        INTEGER NOT NULL,
  created_at          TEXT    NOT NULL,

  CHECK (images_unreached = 0 OR images >= 0),
  CHECK (cloud_used = 0 OR planner_calls > 0)
) STRICT, WITHOUT ROWID;

-- ---------------------------------------------------------------------------
-- The findings
-- ---------------------------------------------------------------------------

CREATE TABLE qc_ticket (
  ticket_id           TEXT    NOT NULL PRIMARY KEY,
  project_id          TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  image_id            TEXT    NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,

  category            TEXT    NOT NULL CHECK (category IN
                        ('consistency','skin','exposure','sharpness','retouch',
                         'mask','crop','cleanup','duplicate','coverage')),

  -- What exactly was found, as a `QcCode` slug. Only the twenty-eight finding codes may appear
  -- here; the outcome and refusal codes live in `outcome_code` and in `qc_round.outcome`. A schema
  -- that let `remedy_applied` be filed as a finding would make section 10.1's detection gate
  -- countable in two incompatible ways.
  code                TEXT    NOT NULL,

  -- The finding, in `QcCategory::unit`. `unit` is denormalised onto the row rather than derived
  -- from `category`, because a report exported today has to keep reading correctly after a check's
  -- formulation changes - and a stored 4.2 whose unit moved from dE00 to a ratio is a number that
  -- silently means something else.
  deviation           REAL    NOT NULL,
  threshold           REAL    NOT NULL CHECK (threshold > 0.0),
  unit                TEXT    NOT NULL,

  -- What to look at: a discriminant plus its payload as JSON. `none | crop | frames | anchors |
  -- params`, and there is no variant that could hold image bytes - phase 13's rule, inherited
  -- unchanged, which is what makes "a QC report contains no pixels" a property of the shape rather
  -- than a promise about the exporter.
  evidence_kind       TEXT    NOT NULL CHECK (evidence_kind IN ('none','crop','frames','anchors','params')),
  evidence_json       TEXT    NOT NULL DEFAULT '{}',

  -- The person this finding is about, when it is about one. Not a foreign key onto `identity` for
  -- the reason phase 25's skin target is not: a re-clustering rebuilds identities and a CASCADE
  -- would delete the finding rather than orphan it, and an orphaned finding is still a true
  -- statement about a photograph.
  identity_id         TEXT,

  -- What should be done. `remedy_kind` is one of section 5's five and nothing else; `remedy_target`
  -- is the solve target, the operation name or the replacement frame depending on the kind;
  -- `remedy_factor` is the strength multiplier, NULL for every other kind.
  --
  -- The factor's CHECK is the contract's own band and it is deliberately not `<= 1.0`: this remedy
  -- *reduces*, and a QC agent that could raise a strength would be a QC agent that edits. Below
  -- 0.25 the operation is being switched off rather than reduced, which is `revert_op` - a
  -- different row, a different reason code and a different sentence in the report.
  remedy_kind         TEXT    NOT NULL CHECK (remedy_kind IN
                        ('resolve_param','reduce_strength','revert_op','replace_frame','escalate')),
  remedy_target       TEXT    NOT NULL,
  remedy_factor       REAL             CHECK (remedy_factor IS NULL OR
                                              (remedy_factor >= 0.25 AND remedy_factor <= 0.90)),

  -- How much the deviation is predicted to fall. The denominator of `QcRound::realised_share`, and
  -- the reason a round can be judged without re-reading the threshold.
  expected_gain       REAL    NOT NULL CHECK (expected_gain >= 0.0),

  confidence          REAL    NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
  autonomy            TEXT    NOT NULL CHECK (autonomy IN
                        ('auto','auto_zero_touch','suggest','require_review')),

  -- The reason set, as a bitmask over the twenty-eight finding codes plus a JSON side-table for
  -- the evidence each one points at. A bitmask alone would lose the weights, and a JSON blob alone
  -- would make "how many tickets cited a skin drift" a scan.
  reasons             INTEGER NOT NULL DEFAULT 0,
  reasons_json        TEXT    NOT NULL DEFAULT '[]',

  round               INTEGER NOT NULL DEFAULT 0 CHECK (round >= 0 AND round <= 2),

  status              TEXT    NOT NULL DEFAULT 'open' CHECK (status IN
                        ('open','fixed','reverted','escalated','accepted','dismissed')),

  -- What happened to it, when something has. NULL while open, which is what makes "this was never
  -- acted on" a different query from "this was acted on and nothing changed". Phase 24's rule:
  -- an absent input is ignorance, not permission, and the two must not share a row shape.
  outcome_code        TEXT,

  -- One sentence a photographer typed, kept for the studio's record and never rendered as a reason.
  user_note           TEXT             CHECK (user_note IS NULL OR length(user_note) <= 280),

  scene               TEXT    NOT NULL,
  thresholds_ver      INTEGER NOT NULL,
  analysis_ver        INTEGER NOT NULL,
  created_at          TEXT    NOT NULL,
  updated_at          TEXT    NOT NULL,

  -- A strength factor belongs to exactly one remedy kind. Without this a `revert_op` row could
  -- carry a factor nobody reads, and a `reduce_strength` row could carry none - which the applier
  -- would read as "multiply by nothing".
  CHECK ((remedy_kind = 'reduce_strength') = (remedy_factor IS NOT NULL)),

  -- A ticket that has been acted on says what happened; one that has not, does not.
  CHECK ((status = 'open') = (outcome_code IS NULL)),

  -- Invariant 2, at the schema layer. `QcTicket::is_well_formed` refuses first; this is the second
  -- layer, and it exists because the first lives in Rust a future caller could route around with a
  -- raw INSERT. Section 10.1 gates explanation coverage at 100 %, and a gate that can only be
  -- measured is weaker than one that cannot be violated.
  CHECK (reasons <> 0 OR reasons_json <> '[]')
) STRICT;

CREATE INDEX idx_qc_ticket_project  ON qc_ticket(project_id, status, category);
CREATE INDEX idx_qc_ticket_image    ON qc_ticket(image_id, category);
-- The escalation queue's own query: open tickets for one project, worst first. `deviation` and
-- `threshold` are both in the index so the severity ratio is computed from the index rather than
-- from the table - the queue is the one read in this phase a photographer waits on.
CREATE INDEX idx_qc_ticket_queue    ON qc_ticket(project_id, status, deviation, threshold)
                                     WHERE status IN ('open','escalated','reverted');

-- ---------------------------------------------------------------------------
-- The attempts
-- ---------------------------------------------------------------------------

CREATE TABLE qc_round (
  ticket_id           TEXT    NOT NULL REFERENCES qc_ticket(ticket_id) ON DELETE CASCADE,
  round               INTEGER NOT NULL CHECK (round >= 1 AND round <= 2),

  remedy_kind         TEXT    NOT NULL CHECK (remedy_kind IN
                        ('resolve_param','reduce_strength','revert_op','replace_frame','escalate')),
  remedy_target       TEXT    NOT NULL,
  remedy_factor       REAL             CHECK (remedy_factor IS NULL OR
                                              (remedy_factor >= 0.25 AND remedy_factor <= 0.90)),

  -- Both deviations, never a boolean. See note 3.
  deviation_before    REAL    NOT NULL,
  deviation_after     REAL    NOT NULL,
  expected_gain       REAL    NOT NULL CHECK (expected_gain >= 0.0),

  -- The worst movement this remedy caused in another check, as a share of *that* check's own
  -- threshold, and which check took it. A share rather than an absolute because the ten checks are
  -- measured in five units and one tolerance stated in dE00 does not stop at the same place in EV.
  collateral          REAL    NOT NULL DEFAULT 0.0 CHECK (collateral >= 0.0),
  collateral_category TEXT             CHECK (collateral_category IS NULL OR collateral_category IN
                        ('consistency','skin','exposure','sharpness','retouch',
                         'mask','crop','cleanup','duplicate','coverage')),

  kept                INTEGER NOT NULL CHECK (kept IN (0,1)),
  outcome             TEXT    NOT NULL,

  ms                  INTEGER NOT NULL CHECK (ms >= 0),
  created_at          TEXT    NOT NULL,

  PRIMARY KEY (ticket_id, round),

  CHECK ((remedy_kind = 'reduce_strength') = (remedy_factor IS NOT NULL)),

  -- A round that reports collateral damage names the check that took it. A number with no subject
  -- is a number nobody can act on.
  CHECK (collateral = 0.0 OR collateral_category IS NOT NULL)
) STRICT, WITHOUT ROWID;

-- ---------------------------------------------------------------------------
-- The swaps
-- ---------------------------------------------------------------------------

CREATE TABLE qc_replacement (
  ticket_id           TEXT    NOT NULL PRIMARY KEY REFERENCES qc_ticket(ticket_id) ON DELETE CASCADE,
  project_id          TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,

  -- The frame that was in the gallery, and the one that is in it now.
  replaced_image      TEXT    NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,
  replacement_image   TEXT    NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,

  category            TEXT    NOT NULL CHECK (category IN
                        ('consistency','skin','exposure','sharpness','retouch',
                         'mask','crop','cleanup','duplicate','coverage')),

  -- Both post-edit metrics, not the difference. A photographer looking at a swap wants to know what
  -- each frame measured, and a stored subtraction cannot be re-read as two numbers. Section 6.4's
  -- "always recorded with a before/after".
  metric_before       REAL    NOT NULL,
  metric_after        REAL    NOT NULL,

  -- Section 6.4: replacements require higher confidence than parameter fixes. The floor is in the
  -- contract and again here, because this is the one remedy whose mistake a photographer cannot
  -- notice - the frame they would have chosen is simply not in the gallery.
  confidence          REAL    NOT NULL CHECK (confidence >= 0.85 AND confidence <= 1.0),

  -- See note 4. CHECKed to 1 rather than merely defaulted: the column exists so that "no
  -- replacement broke coverage" is a query, and a column that could hold 0 would make it a query
  -- with a different answer.
  coverage_held       INTEGER NOT NULL DEFAULT 1 CHECK (coverage_held = 1),

  note                TEXT    NOT NULL,
  created_at          TEXT    NOT NULL,

  -- A frame is never replaced by itself.
  CHECK (replaced_image <> replacement_image)
) STRICT, WITHOUT ROWID;

CREATE INDEX idx_qc_replacement_project ON qc_replacement(project_id);

-- ---------------------------------------------------------------------------
-- Two views: what a photographer should look at, and what the loop could not check
-- ---------------------------------------------------------------------------

-- The escalation queue as one query, worst first. Section 2.1 asks for it "grouped by category so a
-- photographer can clear 40 tickets in minutes", and the severity ratio is the ordering that lets
-- one queue hold findings measured in five different units - a difference would put every dE00
-- finding above every EV one for no reason but the scale.
CREATE VIEW v_qc_queue AS
SELECT
  t.ticket_id,
  t.project_id,
  t.image_id,
  t.category,
  t.code,
  t.deviation,
  t.threshold,
  t.unit,
  t.deviation / t.threshold AS severity,
  t.remedy_kind,
  t.confidence,
  t.autonomy,
  t.round,
  t.status,
  t.scene
FROM qc_ticket t
WHERE t.status IN ('open','escalated','reverted')
ORDER BY severity DESC, t.category ASC, t.ticket_id ASC;

-- What this pass could not check, per project. ADR-0055 section 8: an absent input is a skipped
-- check and not a passed one, and a wedding whose masks are missing must not report zero mask
-- artefacts and read as a clean bill of health. This is the query that makes that visible rather
-- than a sentence in a document - and in this build, with several heads untrained, it is the
-- common case rather than the exotic one.
CREATE VIEW v_qc_unchecked AS
SELECT
  project_id,
  images,
  images_unreached,
  checks_run,
  checks_skipped,
  CASE WHEN (checks_run + checks_skipped) = 0 THEN 0.0
       ELSE CAST(checks_run AS REAL) / (checks_run + checks_skipped) END AS completeness,
  thresholds_ver,
  analysis_ver
FROM qc_run
WHERE checks_skipped > 0 OR images_unreached > 0;

-- ---------------------------------------------------------------------------
-- Four triggers: a person's verdict stands, and the history cannot be rewritten
-- ---------------------------------------------------------------------------

-- A photographer's verdict is never moved by automation.
--
-- `accepted` and `dismissed` are the two statuses a person owns. A pass that re-ran and found the
-- same thing must leave both alone, because a finding somebody rejected that reappears next week is
-- a product arguing with its user - and a finding somebody accepted that quietly becomes "fixed"
-- because a later remedy touched the frame is a product taking credit for a decision that was made
-- for it.
--
-- The DELETE guard alone is not enough here, exactly as it was not in migration 18: `QcStore::sweep`
-- clears a project with a DELETE, so the statuses have to be read out before it and put back after,
-- which is what `take_decisions` and `restore_decisions` do. This trigger is the second layer.
-- The guard is stated as "a status a person owns may only become another status a person owns",
-- which is exactly the separation the two writers already have: automation writes only `open`,
-- `fixed`, `reverted` and `escalated`, and a person writes only `accepted` and `dismissed`. So a
-- photographer changing their own mind - dismissed to accepted, or back - passes, and every
-- automatic write against a reviewed ticket is refused. A trigger cannot ask who the caller is,
-- and this is the formulation that does not need to.
CREATE TRIGGER qc_ticket_keep_user_status
BEFORE UPDATE ON qc_ticket
FOR EACH ROW WHEN OLD.status IN ('accepted','dismissed')
                  AND NEW.status NOT IN ('accepted','dismissed')
BEGIN
  SELECT RAISE(ABORT, 'AURA-ML-5137: a quality-control verdict the photographer set is not overwritten by automation');
END;

-- A round is written once. Second append-only table in the product after migration 13's ledger, and
-- for the same reason: section 6.3 asks that the history of an image's edit be fully
-- reconstructable, and a history a later pass can rewrite is not one. A correction is a second
-- round, never an edit to the first.
CREATE TRIGGER qc_round_no_update
BEFORE UPDATE ON qc_round
BEGIN
  SELECT RAISE(ABORT, 'AURA-ML-5138: a remediation round is append-only; record a second round instead of editing the first');
END;

-- A round is deleted only with its ticket. `ON DELETE CASCADE` on the foreign key is what removes
-- it; a direct DELETE against this table is somebody removing the evidence that a bound was
-- respected, and the bound is the whole of section 6.3.
CREATE TRIGGER qc_round_no_direct_delete
BEFORE DELETE ON qc_round
FOR EACH ROW WHEN (SELECT COUNT(*) FROM qc_ticket WHERE ticket_id = OLD.ticket_id) > 0
BEGIN
  SELECT RAISE(ABORT, 'AURA-ML-5138: a remediation round is removed only with the ticket it belongs to');
END;

-- A disclosed replacement is never quietly un-disclosed.
--
-- Phase 24 made a removal's disclosure immutable for the same reason a swap's record has to be:
-- the delivered gallery contains a photograph the photographer never chose, and the row that says
-- which one it replaced is the only way anybody finds that out afterwards. Phase 14's rule - a
-- delivered file is re-creatable from four values - has no fifth for "and it was a different
-- frame", so this table is where that fact lives and it does not move.
CREATE TRIGGER qc_replacement_is_immutable
BEFORE UPDATE ON qc_replacement
BEGIN
  SELECT RAISE(ABORT, 'AURA-ML-5139: a recorded frame replacement cannot be edited');
END;
