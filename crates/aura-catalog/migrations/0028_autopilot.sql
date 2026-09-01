-- Migration 28: the zero-touch autopilot orchestrator.
--
-- PHASE-28 sections 5 and 6. Four tables, two views and three triggers.
--
-- What this migration stores is a *run*, which is the first subject in the product that is not a
-- photograph, a person, a camera or a gallery. Everything here is about what the product did to a
-- wedding rather than about what it decided, and that distinction is why there is no confidence
-- column, no reason-code column carrying an opinion about a frame, and no column anywhere that
-- could hold a keep, a rejection, a parameter or a strength.
--
-- ## Why the checklist is a row and the graph is not
--
-- `autopilot_settings` holds which stages a photographer switched off. It does not hold what
-- depends on what, in what order stages run, or what any of them may do - all of which live in
-- `crates/aura-jobs/src/stages/` as a compile-time table. A schema that could express a dependency
-- would be a schema a studio could edit into a wedding that graded before it culled.
--
-- ## Why a stage row is written before the stage runs
--
-- Resumability. A run that crashed between "decided to run the retouch" and "started the retouch"
-- must resume knowing the retouch was planned, and a row written afterwards cannot say that. Every
-- stage row is inserted when the stage is planned and updated as it goes, which is also what makes
-- `autopilot_stage` the checkpoint table rather than a separate one: a checkpoint is a stage's
-- progress, and two tables would be two things that can disagree about the same stage.
--
-- ## Why the storage figure is small and stays small
--
-- One `autopilot_run` row per run, twenty-five `autopilot_stage` rows per run, and a bounded
-- number of `autopilot_event` rows. None of them scales with the number of photographs, which
-- makes this the first migration since phase 01 whose per-image cost is a division rather than a
-- measurement: 25 stage rows over a 3,000-frame wedding is a fraction of a byte per photograph.
--
-- The one thing that could scale is the event table, and it is capped by a trigger rather than by
-- hope: a governor polling every second for two hours would write 7,200 rows, so
-- `autopilot_event_cap` keeps the newest 500 per run. Phase 26 learned that a bound has to be
-- asserted as a bound and not only as a size.

-- ---------------------------------------------------------------------------
-- The run
-- ---------------------------------------------------------------------------

CREATE TABLE autopilot_run (
    run_id             TEXT PRIMARY KEY,
    project_id         TEXT NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,

    -- 'running', 'completed', 'completed_degraded', 'cancelled', 'failed'.
    status             TEXT NOT NULL,

    -- Whether the run was unattended. Stored rather than derived, because a summary read a year
    -- later has to be able to say whether the photographer was watching.
    zero_touch         INTEGER NOT NULL DEFAULT 0 CHECK (zero_touch IN (0, 1)),

    -- Whether this build's confidences were calibrated when the run happened. The single most
    -- consequential column in this table: it is what lets a support case tell a run that queued
    -- four hundred frames because the wedding was hard from one that queued them because phase
    -- 13's calibration_ver was still 0.
    calibrated         INTEGER NOT NULL DEFAULT 0 CHECK (calibrated IN (0, 1)),

    stages_enabled     INTEGER NOT NULL DEFAULT 0,
    stages_completed   INTEGER NOT NULL DEFAULT 0,
    stages_degraded    INTEGER NOT NULL DEFAULT 0,

    selected           INTEGER NOT NULL DEFAULT 0,
    exported           INTEGER NOT NULL DEFAULT 0,
    needs_review       INTEGER NOT NULL DEFAULT 0,

    spend_usd          REAL NOT NULL DEFAULT 0.0 CHECK (spend_usd >= 0.0),

    -- Where the delivered files went. Empty until an exporter exists.
    output_path        TEXT NOT NULL DEFAULT '',

    -- Versions. `policy_ver` is autopilot.toml's, `orchestrator_ver` is the DAG's own semantics.
    -- Two rather than one because they invalidate different things: a policy change means the
    -- photographer would be offered a different checklist, and an orchestrator change means every
    -- stored checkpoint describes a plan this build does not have.
    policy_ver         INTEGER NOT NULL DEFAULT 0,
    orchestrator_ver   INTEGER NOT NULL DEFAULT 0,

    started_at         TEXT NOT NULL,
    finished_at        TEXT,
    updated_at         TEXT NOT NULL,

    CHECK (status IN ('running', 'completed', 'completed_degraded', 'cancelled', 'failed')),

    -- `stages_enabled` is what the photographer asked for: the plan minus the rows they switched
    -- off. `stages_completed` counts only the stages that actually did their work, and
    -- `stages_degraded` counts the ones that could not. All three are recomputed from
    -- `autopilot_stage` when the run closes, so these two CHECKs are an assertion about that
    -- arithmetic rather than a constraint anybody writes against directly.
    --
    -- The first version of this counted a switched-off stage as completed, which made
    -- `stages_completed` 25 against a `stages_enabled` of 23 and failed the CHECK on the first
    -- run with a disabled stage. `StageOutcome::is_clean` is true for a switched-off stage - a
    -- run that skipped one is still `Completed` - and that is a different question from whether
    -- the stage did any work. Phase 27's lesson, one level up: a predicate named for one question
    -- must not be spent on a second it answers wrongly.
    CHECK (stages_completed <= stages_enabled),
    CHECK (stages_degraded <= stages_enabled)
);

CREATE INDEX idx_autopilot_run_project ON autopilot_run (project_id, started_at DESC);

-- At most one run in flight per project. A partial unique index rather than a trigger, because a
-- second run over the same wedding is not a race the product should arbitrate - it is two
-- schedulers writing the same rows, and the database refusing the row is the only place that
-- cannot be routed around.
CREATE UNIQUE INDEX idx_autopilot_run_one_in_flight
    ON autopilot_run (project_id)
 WHERE status = 'running';

-- ---------------------------------------------------------------------------
-- One stage of one run, which is also its checkpoint
-- ---------------------------------------------------------------------------

CREATE TABLE autopilot_stage (
    run_id             TEXT NOT NULL REFERENCES autopilot_run(run_id) ON DELETE CASCADE,

    -- The stage slug from `StageId::as_str`. Not a foreign key onto anything: the stage list is a
    -- compile-time table, and a lookup table here would be a second place it could be edited.
    stage              TEXT NOT NULL,

    -- Its position in this run's plan, so a panel lists stages in execution order without
    -- re-deriving the topological sort.
    stage_index        INTEGER NOT NULL,

    -- 'completed', 'partial', 'skipped', 'failed', or NULL while it is still going.
    outcome            TEXT,

    -- The `SkipCause` slug when it was skipped. Separate from `outcome` and not folded into it,
    -- because "skipped" and "skipped because the photographer turned it off" are the difference
    -- between a degraded run and a complete one, and one column could not hold both.
    skip_cause         TEXT,

    -- What the autonomy gate said: 'act', 'act_and_review' or 'hold'.
    verdict            TEXT NOT NULL DEFAULT 'hold',

    items_done         INTEGER NOT NULL DEFAULT 0 CHECK (items_done >= 0),
    items_total        INTEGER NOT NULL DEFAULT 0 CHECK (items_total >= 0),

    -- The digest of what this stage read. Section 6.1's invalidation rule lives in this column.
    inputs_hash        TEXT NOT NULL DEFAULT '',

    attempts           INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0 AND attempts <= 3),
    elapsed_ms         INTEGER NOT NULL DEFAULT 0 CHECK (elapsed_ms >= 0),

    -- The error code when it failed, and its detail. Codes rather than sentences: phase 09's rule,
    -- and a stored sentence here would be a run summary a release could change under a
    -- photographer's archived report.
    error_code         TEXT,
    error_detail       TEXT,

    started_at         TEXT,
    finished_at        TEXT,
    updated_at         TEXT NOT NULL,

    PRIMARY KEY (run_id, stage),

    CHECK (outcome IS NULL OR outcome IN ('completed', 'partial', 'skipped', 'failed')),
    CHECK (verdict IN ('act', 'act_and_review', 'hold')),

    -- A skip has a cause and nothing else does. This is the schema half of the rule that a stage
    -- which could not run is named rather than quietly passed: an outcome of 'skipped' with no
    -- cause would be exactly the silent skip the whole design exists to prevent.
    CHECK ((outcome = 'skipped') = (skip_cause IS NOT NULL))
) WITHOUT ROWID;

CREATE INDEX idx_autopilot_stage_outcome ON autopilot_stage (run_id, outcome);

-- ---------------------------------------------------------------------------
-- Reasons
-- ---------------------------------------------------------------------------

CREATE TABLE autopilot_reason (
    run_id             TEXT NOT NULL REFERENCES autopilot_run(run_id) ON DELETE CASCADE,

    -- NULL for a reason about the whole run.
    stage              TEXT,

    -- The `AutopilotCode` slug. A code and never a sentence: phase 09's rule at its conclusion,
    -- and phase 27's schema scan looks for exactly this kind of column in every migration.
    code               TEXT NOT NULL,

    -- A short factual detail - a number, a stage name, an error code - never prose a panel
    -- renders on its own.
    detail             TEXT NOT NULL DEFAULT '',

    seq                INTEGER NOT NULL,
    created_at         TEXT NOT NULL,

    PRIMARY KEY (run_id, seq)
) WITHOUT ROWID;

CREATE INDEX idx_autopilot_reason_code ON autopilot_reason (run_id, code);

-- ---------------------------------------------------------------------------
-- What the machine had to say
-- ---------------------------------------------------------------------------

CREATE TABLE autopilot_event (
    event_id           INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id             TEXT NOT NULL REFERENCES autopilot_run(run_id) ON DELETE CASCADE,

    -- 'vram', 'ram', 'thermal', 'battery', 'disk', 'quiet', 'device_lost'.
    kind               TEXT NOT NULL,

    -- 'proceed', 'reduce', 'pause', 'stop'. `proceed` is never stored - a governor that recorded
    -- every reading that was fine would write seven rows a second - so the column's presence of a
    -- row means the machine asked for something.
    action             TEXT NOT NULL,

    reading            REAL NOT NULL,
    threshold          REAL NOT NULL,
    stage              TEXT NOT NULL,
    created_at         TEXT NOT NULL,

    CHECK (kind IN ('vram', 'ram', 'thermal', 'battery', 'disk', 'quiet', 'device_lost')),
    CHECK (action IN ('reduce', 'pause', 'stop'))
);

CREATE INDEX idx_autopilot_event_run ON autopilot_event (run_id, event_id DESC);

-- ---------------------------------------------------------------------------
-- What the photographer chose
-- ---------------------------------------------------------------------------

CREATE TABLE autopilot_settings (
    project_id         TEXT PRIMARY KEY REFERENCES project(project_id) ON DELETE CASCADE,

    -- A JSON array of stage slugs the photographer switched off. JSON rather than a child table
    -- because it is read whole, written whole and never queried by member - and a child table
    -- would invite a query that treated one disabled stage as a fact about the wedding rather
    -- than as one entry in a preference.
    disabled           TEXT NOT NULL DEFAULT '[]',

    zero_touch         INTEGER NOT NULL DEFAULT 0 CHECK (zero_touch IN (0, 1)),
    allow_on_battery   INTEGER NOT NULL DEFAULT 0 CHECK (allow_on_battery IN (0, 1)),
    quiet_mode         INTEGER NOT NULL DEFAULT 1 CHECK (quiet_mode IN (0, 1)),

    updated_at         TEXT NOT NULL
) WITHOUT ROWID;

-- ---------------------------------------------------------------------------
-- Triggers
-- ---------------------------------------------------------------------------

-- A *delivered* run is finished. Phase 13's ledger established that a record automation can
-- rewrite is not a record; here the thing being protected is what a photographer was told happened
-- to their wedding. A correction is a new run, not an edit to an old one.
--
-- 'cancelled' and 'failed' are deliberately not covered, and the distinction is the one this
-- migration would most easily have got wrong. A stopped wedding and a delivered wedding are
-- different things: a cancel and a failed mandatory stage both leave every finished stage
-- committed and a resumable checkpoint behind, so pressing the button again continues that run.
-- A trigger that treated every terminal status as final would force a resume to mint a new run id,
-- which - because checkpoints are keyed on (run_id, stage) - would find no checkpoints and repeat
-- every finished stage. Two hours of a photographer's evening, lost to a bookkeeping rule rather
-- than to a bug. `RunStatus::is_resumable` is the same rule in Rust.
CREATE TRIGGER autopilot_run_no_reopen
BEFORE UPDATE OF status ON autopilot_run
WHEN OLD.status IN ('completed', 'completed_degraded') AND NEW.status <> OLD.status
BEGIN
    SELECT RAISE(ABORT, 'AURA-JOB-7005: a delivered run cannot be reopened');
END;

-- A reason is written once. The run summary a studio archives has to say the same thing next year
-- as it says today, and a mutable reason row is a summary that quietly changes.
CREATE TRIGGER autopilot_reason_no_update
BEFORE UPDATE ON autopilot_reason
BEGIN
    SELECT RAISE(ABORT, 'AURA-JOB-7005: a recorded reason cannot be edited');
END;

-- The event table is bounded rather than trusted. A governor polling once a second through a
-- two-hour run under sustained pressure would write thousands of rows; the newest 500 per run are
-- what a support case needs and the rest are the same sentence repeated.
CREATE TRIGGER autopilot_event_cap
AFTER INSERT ON autopilot_event
BEGIN
    DELETE FROM autopilot_event
     WHERE run_id = NEW.run_id
       AND event_id <= (
           SELECT event_id FROM autopilot_event
            WHERE run_id = NEW.run_id
            ORDER BY event_id DESC
            LIMIT 1 OFFSET 500
       );
END;

-- ---------------------------------------------------------------------------
-- Views
-- ---------------------------------------------------------------------------

-- Every stage that did not do what it was meant to, in the newest run of each project. This is
-- what `RunSummary::degraded_stages` is built from, and it is a view rather than a query in Rust
-- so "what did this build fail to do" is answerable from a support bundle with sqlite3.
CREATE VIEW autopilot_degraded AS
SELECT r.project_id,
       s.run_id,
       s.stage,
       s.outcome,
       s.skip_cause,
       s.error_code,
       s.elapsed_ms
  FROM autopilot_stage s
  JOIN autopilot_run r ON r.run_id = s.run_id
 WHERE s.outcome IN ('failed', 'partial')
    OR (s.outcome = 'skipped' AND s.skip_cause <> 'turned_off');

-- How long each stage took, newest run first. The benchmark campaign's own table.
CREATE VIEW autopilot_timings AS
SELECT r.project_id,
       s.run_id,
       s.stage_index,
       s.stage,
       s.items_done,
       s.elapsed_ms,
       CASE WHEN s.items_done > 0
            THEN CAST(s.elapsed_ms AS REAL) / s.items_done
            ELSE NULL
       END AS ms_per_item
  FROM autopilot_stage s
  JOIN autopilot_run r ON r.run_id = s.run_id
 WHERE s.outcome IS NOT NULL;
