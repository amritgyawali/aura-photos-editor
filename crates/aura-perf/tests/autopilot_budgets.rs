#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]
#![allow(clippy::panic, clippy::float_cmp, clippy::disallowed_methods)]
#![allow(clippy::uninlined_format_args)]

//! PHASE-28 section 11's budgets, as tests - and the four of five that are **waived**.
//!
//! | Metric | Budget | Here |
//! |---|---|---|
//! | 3,000 images end to end (RTX 4070 laptop) | <= 2.5 h | waived |
//! | 3,000 images end to end (M3 Pro) | <= 4 h | waived |
//! | Analysis + cull portion | < 8 min | waived (inherited) |
//! | Peak VRAM | <= 80 % of available | waived |
//! | Resume overhead | <= 20 s | **measured** |
//!
//! ## Why four rows are waived rather than filled in
//!
//! A wall clock over a wedding is a statement about the *stages*, and on this machine every stage
//! is a fixture: there is no GPU backend (ADR-0029 section 4), no trained model in any of the
//! twenty-two deciding phases, and no camera file. A number produced here would look like a wall
//! clock and be a measurement of `ScriptedRunner`, which is worse than publishing nothing, because
//! the second is honest and the first would be quoted.
//!
//! Phase 03 wrote the rule this follows: numbers come from runs, and an unmeasured row is left
//! empty with an expiry condition rather than filled with a plausible figure. The expiry is a GPU
//! backend plus one trained model, and it is condition C1 of the phase 28 exit report.
//!
//! ## What is measured instead, and why it is worth measuring
//!
//! The **orchestrator's own overhead** - the part that is real on this machine and would still be
//! real on a reference laptop, because it is arithmetic over rows rather than work on pixels.
//!
//! Two rows, and they are what a photographer pays *on top of* the work itself: planning a
//! twenty-five stage run, and resuming one. Both are bounded by the **stage count** rather than by
//! the photograph count, which is the property that makes a 6,000-frame wedding resume as fast as a
//! 300-frame one - and it is a property worth a regression guard, because the obvious way to
//! implement a resume (re-read what each stage did) would not have it.
//!
//! ## The storage row has a shape no migration since phase 01 has had
//!
//! Every migration from 09 to 27 stores something **per photograph**. This one stores nothing per
//! photograph at all: one `autopilot_run` row per run, one `autopilot_stage` row per stage, a
//! bounded number of reasons, and at most the 500 governor events `autopilot_event_cap` keeps.
//!
//! So a per-image figure here is a division rather than a measurement, and it *falls* as a wedding
//! grows. That makes a size assertion alone nearly meaningless - it would pass on a build that had
//! removed the event cap and happened to be measured on a machine with no sensors, which is this
//! one. Phase 26 learned that from the other side, when a note about a table growing with the
//! square of a wedding's overlap turned out to describe a table that was capped.
//!
//! **So the bound is asserted as well as the number**, by running the same orchestrator over a
//! wedding ten times the size and checking that the store did not grow with it.

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
    AutopilotOverride, RunHandle, RunProgress, RunWatch, StageId,
};
use aura_jobs::fixtures::{ports, Behaviour, FixedGate, FixedProbe, ScriptedRunner};
use rusqlite::params;

/// `perf/budgets.toml`, `[stage.autopilot_plan]`. The orchestrator's share of a two-hour run.
const PLAN_MS: u128 = 5_000;

/// `perf/budgets.toml`, `[stage.autopilot_resume]`. Section 11's own row.
const RESUME_MS: u128 = 20_000;

/// `perf/budgets.toml`, `[size.autopilot_store_per_1000_images]`.
const BYTES_PER_1000_IMAGES: u64 = 10_000;

/// The wedding the storage figure is quoted against.
const IMAGES: u64 = 3_000;

/// A catalog with one project row, and the temporary directory that owns it.
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
        Catalog::open(&dir.path().join("perf.sqlite"), Arc::clone(&clock), "test")
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
                 VALUES (?1, 'perf', ?2, ?2)",
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

/// The `dbstat` payload of every table this phase owns, in bytes.
///
/// Payload rather than `PRAGMA page_count`, which quantises to 4 KiB - phase 09's correction, and
/// the reason its own 1 KB budget read "exactly 1,024" for ten phases.
fn store_bytes(catalog: &Arc<Catalog>) -> u64 {
    catalog
        .read(|conn: &rusqlite::Connection| {
            conn.query_row(
                "SELECT COALESCE(SUM(payload), 0) FROM dbstat WHERE name LIKE 'autopilot%'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|err| aura_core::errors::db::statement_failed("dbstat", &err))
        })
        .map(|value| u64::try_from(value.max(0)).unwrap_or(0))
        .unwrap_or(0)
}

#[test]
fn planning_and_running_twenty_five_stages_costs_almost_nothing() {
    // What is measured is the scheduler: twenty-five availability questions, band lookups, unit
    // counts, input hashes, checkpoint reads, resume decisions and governor polls, plus two catalog
    // writes per stage. The stages themselves do one unit each, so their own time is negligible and
    // what is left is the overhead this row exists to bound.
    let world = world();
    let runner = Arc::new(ScriptedRunner::new(1));
    let autopilot = boot(&world, Arc::clone(&runner));

    let started = Instant::now();
    let summary = autopilot
        .execute(
            world.project,
            &settings(world.project),
            &handle(RunId::new()),
            &tally(),
        )
        .expect("the run finishes");
    let elapsed = started.elapsed().as_millis();
    let budget = PLAN_MS * u128::from(aura_perf::host_scale());

    println!(
        "autopilot plan: {} stages, {elapsed} ms against {budget} ms",
        summary.stage_timings.len()
    );
    assert_eq!(
        summary.stage_timings.len(),
        StageId::COUNT,
        "every stage is visited, including the ones that are skipped"
    );
    assert!(
        elapsed <= budget,
        "planning and running 25 stages took {elapsed} ms, over the {budget} ms budget"
    );
}

#[test]
fn a_resume_is_bounded_by_the_stage_count_rather_than_the_wedding() {
    // Section 11's row, measured the way `tests/e2e/autopilot_chaos.rs` measures it and asserted
    // here so the budget file has a test. The second life's runner has *zero* units, so nothing
    // this measures is a stage doing work: it is twenty-five checkpoint reads, twenty-five input
    // hashes and one ready-set rebuild.
    let world = world();
    let run_id = RunId::new();

    let first = Arc::new(ScriptedRunner::new(4).with(StageId::Colour, Behaviour::CancelAfter(2)));
    {
        let autopilot = boot(&world, Arc::clone(&first));
        let handle = handle(run_id);
        first.arm(handle.cancel.clone());
        autopilot
            .execute(world.project, &settings(world.project), &handle, &tally())
            .expect("the first life");
    }

    let started = Instant::now();
    {
        let autopilot = boot(&world, Arc::new(ScriptedRunner::new(0)));
        autopilot
            .execute(
                world.project,
                &settings(world.project),
                &handle(run_id),
                &tally(),
            )
            .expect("the second life");
    }
    let elapsed = started.elapsed().as_millis();
    let budget = RESUME_MS * u128::from(aura_perf::host_scale());

    println!("autopilot resume: {elapsed} ms against {budget} ms");
    assert!(
        elapsed <= budget,
        "a resume took {elapsed} ms, over the {budget} ms budget"
    );
}

#[test]
fn the_store_is_bounded_by_the_run_rather_than_by_the_wedding() {
    // The size, and the *shape*. A size assertion alone would pass on a build that had removed the
    // event cap and happened to be measured on a machine with no sensors - which is this one - so
    // the second half runs the same orchestrator over a wedding ten times the size and checks that
    // nothing grew. Phase 26's lesson, applied to the phase whose store has the least excuse to
    // grow with a wedding.
    let one = world();
    let autopilot = boot(&one, Arc::new(ScriptedRunner::new(2)));
    autopilot
        .execute(
            one.project,
            &settings(one.project),
            &handle(RunId::new()),
            &tally(),
        )
        .expect("the run finishes");
    let small = store_bytes(&one.catalog);

    let bigger = world();
    let ten_times = boot(&bigger, Arc::new(ScriptedRunner::new(20)));
    ten_times
        .execute(
            bigger.project,
            &settings(bigger.project),
            &handle(RunId::new()),
            &tally(),
        )
        .expect("the bigger run finishes");
    let large = store_bytes(&bigger.catalog);

    let per_1000 = small * 1_000 / IMAGES;
    println!(
        "autopilot store: {small} B for a {IMAGES}-image wedding = {per_1000} B/1000 images \
         against {BYTES_PER_1000_IMAGES}; ten times the units stores {large} B"
    );
    assert!(
        per_1000 <= BYTES_PER_1000_IMAGES,
        "the store holds {per_1000} B per 1,000 images, over the {BYTES_PER_1000_IMAGES} B budget"
    );
    assert!(
        large <= small * 2,
        "ten times the work grew the store from {small} B to {large} B, so something in this \
         phase is per-photograph after all"
    );
}
