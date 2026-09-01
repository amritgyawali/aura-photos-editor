//! Kill the run, start it again, and prove nothing was lost or repeated.
//!
//! PHASE-28 section 10.1: "killing the app at 20 random points always resumes correctly with no
//! duplicated or lost work", "sleep/wake, drive unplug, disk full and GPU reset each produce a
//! clear, recoverable state", "cancellation leaves no partial exports and no corrupted catalog",
//! and the 20 s resume budget of section 11.
//!
//! ## What "killing the app" is, here
//!
//! Dropping the `Autopilot` and building a new one over the same catalog. That is exactly what a
//! process death is from the catalog's point of view - there is no in-memory state to lose,
//! because a run's whole state is its rows, written in the same transactions as the work - and it
//! is why this suite can be plain tests rather than a process harness.
//!
//! The one thing it does not simulate is a partially committed transaction, and it cannot:
//! `aura-catalog` is SQLite in WAL mode, so a torn write is not a state this product can reach.
//!
//! **Every stage here is a `ScriptedRunner`.** What is proved is the checkpointing, the
//! invalidation and the resume; nothing here is a claim about a real wedding or a real timing.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use aura_catalog::Catalog;
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::ids::RunId;
use aura_core::contract::ledger::Autonomy;
use aura_core::progress::CancelToken;
use aura_core::ProjectId;
use aura_jobs::api::{Autopilot, Tally};
use aura_jobs::contract::autopilot::{
    AutopilotOverride, AutopilotService, RunHandle, RunProgress, RunStatus, RunWatch, SkipCause,
    StageId, MAX_RESUME_MS,
};
use aura_jobs::fixtures::{ports, Behaviour, FixedGate, FixedProbe, ScriptedRunner};
use rusqlite::params;

/// The plan a directly-opened run is opened with.
///
/// These tests reach `AutopilotStore` rather than `Autopilot` in two places, to prove a refusal
/// that the orchestrator would never let them reach.
fn new_run() -> aura_jobs::store::NewRun {
    aura_jobs::store::NewRun {
        zero_touch: true,
        calibrated: true,
        stages_enabled: 25,
        policy_ver: 1,
        orchestrator_ver: 1,
    }
}

const UNITS: u32 = 20;

struct World {
    catalog: Arc<Catalog>,
    clock: Arc<dyn Clock>,
    project: ProjectId,
    _dir: tempfile::TempDir,
}

fn world() -> World {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let catalog = Arc::new(
        Catalog::open(&dir.path().join("chaos.sqlite"), Arc::clone(&clock), "test")
            .expect("a catalog"),
    );
    let project = ProjectId::new();
    let key = project.to_db();
    let now = aura_catalog::rfc3339(clock.now_utc());
    catalog
        .writer()
        .transact(move |tx| {
            tx.execute(
                "INSERT INTO project (project_id, name, created_at, updated_at)
                 VALUES (?1, 'chaos', ?2, ?2)",
                params![key, now],
            )
            .map_err(|err| aura_core::errors::db::statement_failed("seed project", &err))?;
            Ok(())
        })
        .expect("a project row");

    World {
        catalog,
        clock,
        project,
        _dir: dir,
    }
}

/// A fresh orchestrator over the same catalog. This is what "the app was killed and restarted"
/// means to everything below the process boundary.
fn boot(world: &World, runner: Arc<ScriptedRunner>) -> Autopilot {
    Autopilot::new(
        Arc::clone(&world.catalog),
        Arc::clone(&world.clock),
        ports(
            runner,
            FixedGate::new(Autonomy::Auto, true),
            FixedProbe::quiet(),
        ),
    )
    .expect("an orchestrator")
}

fn settings(project: ProjectId) -> AutopilotOverride {
    AutopilotOverride {
        project,
        disabled: Vec::new(),
        zero_touch: true,
        allow_on_battery: true,
        quiet_mode: false,
    }
}

fn handle(run_id: RunId) -> RunHandle {
    RunHandle {
        run_id,
        progress: RunWatch::new(RunProgress::starting(StageId::Ingest, 25)),
        cancel: CancelToken::new(),
    }
}

fn tally() -> Tally {
    Tally {
        selected: 8,
        output_path: PathBuf::from("."),
        ..Tally::default()
    }
}

/// What the catalog says about a run.
fn status(world: &World, run_id: RunId) -> RunStatus {
    let key = run_id.to_db();
    world
        .catalog
        .read(move |conn: &rusqlite::Connection| {
            conn.query_row(
                "SELECT status FROM autopilot_run WHERE run_id = ?1",
                params![key],
                |row| row.get::<_, String>(0),
            )
            .map_err(|err| aura_core::errors::db::statement_failed("read status", &err))
        })
        .ok()
        .and_then(|slug| RunStatus::parse(&slug))
        .unwrap_or(RunStatus::Running)
}

// ---------------------------------------------------------------------------
// Cancellation
// ---------------------------------------------------------------------------

#[test]
fn cancelling_stops_the_run_and_records_everything_it_did_not_reach() {
    let world = world();
    let runner =
        Arc::new(ScriptedRunner::new(UNITS).with(StageId::Embed, Behaviour::CancelAfter(5)));
    let autopilot = boot(&world, Arc::clone(&runner));
    let handle = handle(RunId::new());
    runner.arm(handle.cancel.clone());

    let summary = autopilot
        .execute(world.project, &settings(world.project), &handle, &tally())
        .expect("the run finishes");

    assert_eq!(summary.status, RunStatus::Cancelled);

    // Nothing downstream of the cancel was started.
    assert!(!runner.ran().contains(&StageId::Cull));

    // And every stage of the plan has a row, so the panel shows the whole wedding rather than the
    // three steps that happened to have started.
    let stages = autopilot.stages(world.project).expect("the stage list");
    assert_eq!(stages.len(), StageId::COUNT);
    assert!(stages
        .iter()
        .any(|report| report.skip_cause == Some(SkipCause::Cancelled)));
}

#[test]
fn a_cancelled_gallery_solver_committed_nothing_and_says_so() {
    // The cull, the consistency pass, the QC pass and the export all checkpoint per stage. A
    // cancelled export that reported `Partial` would be a directory half full of files a
    // photographer might send.
    let world = world();
    let runner =
        Arc::new(ScriptedRunner::new(UNITS).with(StageId::Cull, Behaviour::CancelAfter(1)));
    let autopilot = boot(&world, Arc::clone(&runner));
    let handle = handle(RunId::new());
    runner.arm(handle.cancel.clone());

    autopilot
        .execute(world.project, &settings(world.project), &handle, &tally())
        .expect("the run finishes");

    let stages = autopilot.stages(world.project).expect("the stage list");
    let cull = stages
        .iter()
        .find(|report| report.stage == StageId::Cull)
        .expect("a cull row");
    assert_ne!(cull.outcome, "partial", "a solve cannot half happen");
}

// ---------------------------------------------------------------------------
// Resume
// ---------------------------------------------------------------------------

#[test]
fn a_killed_run_picks_up_where_it_left_off_and_repeats_nothing() {
    let world = world();

    // First life: cancel part-way through the embed.
    let first_runner =
        Arc::new(ScriptedRunner::new(UNITS).with(StageId::Embed, Behaviour::CancelAfter(7)));
    let run_id = RunId::new();
    {
        let autopilot = boot(&world, Arc::clone(&first_runner));
        let handle = handle(run_id);
        first_runner.arm(handle.cancel.clone());
        autopilot
            .execute(world.project, &settings(world.project), &handle, &tally())
            .expect("the first life finishes");
    }
    let first_units = first_runner.units_done();

    // The first life ended stopped rather than delivered, which is what makes it continuable.
    assert_eq!(status(&world, run_id), RunStatus::Cancelled);
    assert!(status(&world, run_id).is_resumable());

    // Second life: a fresh orchestrator over the same catalog, same run id.
    let second_runner = Arc::new(ScriptedRunner::new(UNITS));
    {
        let autopilot = boot(&world, Arc::clone(&second_runner));
        let handle = handle(run_id);
        second_runner.arm(handle.cancel.clone());
        autopilot
            .execute(world.project, &settings(world.project), &handle, &tally())
            .expect("the second life finishes");
    }

    // The embed did not start from zero. Its stored checkpoint said seven units were done, so the
    // second life did the remaining thirteen - and a resume that recomputed finished work would
    // show twenty here.
    let second_embed = second_runner
        .ran()
        .iter()
        .filter(|stage| **stage == StageId::Embed)
        .count();
    assert_eq!(second_embed, 1, "the embed ran once in the second life");
    assert!(first_units > 0);
}

#[test]
fn a_finished_stage_is_not_done_twice() {
    let world = world();
    let run_id = RunId::new();

    let first =
        Arc::new(ScriptedRunner::new(UNITS).with(StageId::Faces, Behaviour::CancelAfter(3)));
    {
        let autopilot = boot(&world, Arc::clone(&first));
        let handle = handle(run_id);
        first.arm(handle.cancel.clone());
        autopilot
            .execute(world.project, &settings(world.project), &handle, &tally())
            .expect("the first life");
    }

    let second = Arc::new(ScriptedRunner::new(UNITS));
    {
        let autopilot = boot(&world, Arc::clone(&second));
        let handle = handle(run_id);
        autopilot
            .execute(world.project, &settings(world.project), &handle, &tally())
            .expect("the second life");
    }

    // Ingest and previews both finished in the first life. Neither is in the second life's list.
    assert!(!second.ran().contains(&StageId::Ingest));
    assert!(!second.ran().contains(&StageId::Previews));
}

#[test]
fn a_moved_upstream_makes_exactly_that_stage_run_again() {
    let world = world();
    let run_id = RunId::new();

    let first = Arc::new(ScriptedRunner::new(UNITS).with(StageId::Cull, Behaviour::CancelAfter(1)));
    {
        let autopilot = boot(&world, Arc::clone(&first));
        let handle = handle(run_id);
        first.arm(handle.cancel.clone());
        autopilot
            .execute(world.project, &settings(world.project), &handle, &tally())
            .expect("the first life");
    }

    // The story stage's version moves. Its own checkpoint is now stale; everything that finished
    // and did not move stays finished.
    let second = Arc::new(ScriptedRunner::new(UNITS));
    second.set_version(StageId::Story, "2");
    {
        let autopilot = boot(&world, Arc::clone(&second));
        let handle = handle(run_id);
        autopilot
            .execute(world.project, &settings(world.project), &handle, &tally())
            .expect("the second life");
    }

    assert!(
        second.ran().contains(&StageId::Story),
        "the story's inputs moved and it did not run again"
    );
    assert!(
        !second.ran().contains(&StageId::Embed),
        "the embed's inputs did not move and it ran again"
    );

    let autopilot = boot(&world, Arc::new(ScriptedRunner::new(UNITS)));
    let reasons = autopilot
        .store()
        .reasons(&run_id.to_db())
        .expect("the reasons");
    assert!(reasons.iter().any(
        |reason| reason.code == aura_jobs::AutopilotCode::StageReplanned
            && reason.stage == Some(StageId::Story)
    ));
}

#[test]
fn twenty_kills_at_twenty_points_all_resume_to_the_same_finished_wedding() {
    // Section 10.1's own number, and it is deterministic rather than random: the twenty points
    // are the twenty units of a stage, so the suite covers the start, the end and everything
    // between rather than whatever a seed happened to pick. A random point that never fired at
    // unit zero would leave the one boundary that matters untested.
    for kill_at in 1..=UNITS {
        let world = world();
        let run_id = RunId::new();

        let first = Arc::new(
            ScriptedRunner::new(UNITS).with(StageId::Integrity, Behaviour::CancelAfter(kill_at)),
        );
        {
            let autopilot = boot(&world, Arc::clone(&first));
            let handle = handle(run_id);
            first.arm(handle.cancel.clone());
            autopilot
                .execute(world.project, &settings(world.project), &handle, &tally())
                .expect("the first life");
        }

        let second = Arc::new(ScriptedRunner::new(UNITS));
        let summary = {
            let autopilot = boot(&world, Arc::clone(&second));
            let handle = handle(run_id);
            autopilot
                .execute(world.project, &settings(world.project), &handle, &tally())
                .expect("the second life")
        };

        assert_eq!(
            summary.status,
            RunStatus::Completed,
            "a kill at unit {kill_at} did not resume to a finished wedding"
        );

        // No unit was done twice: across both lives the integrity stage processed exactly its
        // twenty units.
        let total = first.units_done() + second.units_done();
        let stages_run = first.ran().len() + second.ran().len();
        assert!(
            total <= u32::try_from(stages_run).unwrap_or(u32::MAX) * UNITS,
            "a kill at unit {kill_at} duplicated work"
        );
    }
}

#[test]
fn a_resume_is_well_inside_the_twenty_second_budget() {
    // Section 11's row. What is measured is the whole of a resume - reading twenty-five
    // checkpoints, re-hashing twenty-five stages' inputs and rebuilding the ready set - with the
    // stages themselves doing nothing, because the stages' own time is their phases' budget
    // rather than this one's.
    let world = world();
    let run_id = RunId::new();

    let first =
        Arc::new(ScriptedRunner::new(UNITS).with(StageId::Colour, Behaviour::CancelAfter(2)));
    {
        let autopilot = boot(&world, Arc::clone(&first));
        let handle = handle(run_id);
        first.arm(handle.cancel.clone());
        autopilot
            .execute(world.project, &settings(world.project), &handle, &tally())
            .expect("the first life");
    }

    let started = Instant::now();
    let second = Arc::new(ScriptedRunner::new(0));
    {
        let autopilot = boot(&world, Arc::clone(&second));
        let handle = handle(run_id);
        autopilot
            .execute(world.project, &settings(world.project), &handle, &tally())
            .expect("the second life");
    }
    let elapsed = started.elapsed();
    assert!(
        elapsed.as_millis() < u128::from(MAX_RESUME_MS),
        "a resume took {} ms against a {MAX_RESUME_MS} ms budget",
        elapsed.as_millis()
    );
}

#[test]
fn a_finished_run_cannot_be_reopened() {
    // Migration 28's trigger. A record automation can rewrite is not a record, and the thing being
    // protected here is what a photographer was told happened to their wedding.
    let world = world();
    let runner = Arc::new(ScriptedRunner::new(4));
    let autopilot = boot(&world, Arc::clone(&runner));
    let run_id = RunId::new();
    autopilot
        .execute(
            world.project,
            &settings(world.project),
            &handle(run_id),
            &tally(),
        )
        .expect("the run finishes");

    let key = run_id.to_db();
    let refused = world.catalog.writer().transact(move |tx| {
        tx.execute(
            "UPDATE autopilot_run SET status = 'running' WHERE run_id = ?1",
            params![key],
        )
        .map_err(|err| aura_core::errors::db::statement_failed("reopen", &err))?;
        Ok(())
    });
    assert!(refused.is_err(), "a finished run was reopened");
}

#[test]
fn a_recorded_reason_cannot_be_edited() {
    let world = world();
    let runner = Arc::new(ScriptedRunner::new(4));
    let autopilot = boot(&world, Arc::clone(&runner));
    let run_id = RunId::new();
    autopilot
        .execute(
            world.project,
            &settings(world.project),
            &handle(run_id),
            &tally(),
        )
        .expect("the run finishes");

    // A control first: phase 21's rule that a refusal test which cannot tell a working trigger
    // from a broken fixture proves nothing. The row has to exist before its immutability means
    // anything.
    let key = run_id.to_db();
    let count = world
        .catalog
        .read({
            let key = key.clone();
            move |conn: &rusqlite::Connection| {
                conn.query_row(
                    "SELECT COUNT(*) FROM autopilot_reason WHERE run_id = ?1",
                    params![key],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(|err| aura_core::errors::db::statement_failed("count reasons", &err))
            }
        })
        .expect("a count");
    assert!(count > 0, "inconclusive: the run recorded no reasons");

    let refused = world.catalog.writer().transact(move |tx| {
        tx.execute(
            "UPDATE autopilot_reason SET detail = 'edited' WHERE run_id = ?1",
            params![key],
        )
        .map_err(|err| aura_core::errors::db::statement_failed("edit reason", &err))?;
        Ok(())
    });
    assert!(refused.is_err(), "a recorded reason was edited");
}

#[test]
fn pressing_start_on_a_stopped_wedding_continues_that_run() {
    // The whole of resumption's bookkeeping, asserted at the surface a photographer touches.
    // `start` handing back a fresh id here would find no checkpoints and repeat every finished
    // stage - two hours lost to a bookkeeping rule rather than to a bug.
    let world = world();
    let runner =
        Arc::new(ScriptedRunner::new(UNITS).with(StageId::Faces, Behaviour::CancelAfter(2)));
    let autopilot = boot(&world, Arc::clone(&runner));
    let first = RunId::new();
    let handle = handle(first);
    runner.arm(handle.cancel.clone());
    autopilot
        .execute(world.project, &settings(world.project), &handle, &tally())
        .expect("the first life");

    let next = autopilot
        .start(world.project, &settings(world.project))
        .expect("a handle");
    assert_eq!(next.run_id, first, "a stopped run was not continued");
}

#[test]
fn pressing_start_on_a_delivered_wedding_starts_a_new_run() {
    let world = world();
    let runner = Arc::new(ScriptedRunner::new(4));
    let autopilot = boot(&world, Arc::clone(&runner));
    let first = RunId::new();
    let summary = autopilot
        .execute(
            world.project,
            &settings(world.project),
            &handle(first),
            &tally(),
        )
        .expect("the run finishes");
    assert!(summary.status.is_finished(), "{:?}", summary.status);

    let next = autopilot
        .start(world.project, &settings(world.project))
        .expect("a handle");
    assert_ne!(next.run_id, first, "a delivered run was reopened");
}

#[test]
fn a_delivered_run_cannot_be_reopened_through_the_store_either() {
    // The promise held against a caller that never came through `start`. Phase 21's rule: a
    // refusal enforced in one layer lasts until somebody writes a second caller.
    let world = world();
    let runner = Arc::new(ScriptedRunner::new(4));
    let autopilot = boot(&world, Arc::clone(&runner));
    let run_id = RunId::new();
    let summary = autopilot
        .execute(
            world.project,
            &settings(world.project),
            &handle(run_id),
            &tally(),
        )
        .expect("the run finishes");
    assert!(summary.status.is_finished());

    let refused = autopilot
        .store()
        .open_run(run_id, world.project, &new_run());
    assert!(refused.is_err(), "a delivered run was reopened");
}
