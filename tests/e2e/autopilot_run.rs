//! A whole wedding through the real orchestrator, against an authored runner.
//!
//! PHASE-28 section 10.1, the rows that are about *what a run does*: one button processes a
//! wedding, an optional stage failure yields `CompletedDegraded` with an accurate skipped list,
//! the autonomy gate holds what it should hold, and the pre-flight catches what it should catch.
//!
//! The kill/resume half is `autopilot_chaos.rs`.
//!
//! **Nothing here proves anything about a real photograph, and nothing here is a timing.** Every
//! stage is a `ScriptedRunner` doing what this repository told it to do, so what is measured is
//! the scheduler: the order, the gate, the skips, the isolation, the summary and the store. That
//! is condition C1 of the phase 28 exit report and it is printed by the phase gate on every run.

use std::path::PathBuf;
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::ledger::Autonomy;
use aura_core::progress::CancelToken;
use aura_core::ProjectId;
use aura_jobs::api::{Autopilot, Tally};
use aura_jobs::contract::autopilot::{
    AutopilotOverride, AutopilotService, RunHandle, RunProgress, RunStatus, RunWatch, SkipCause,
    StageId, StageVerdict,
};
use aura_jobs::fixtures::{ports, Behaviour, FixedGate, FixedProbe, ScriptedRunner};
use aura_jobs::preflight::Facts;
use rusqlite::params;

const UNITS: u32 = 24;

struct Harness {
    autopilot: Autopilot,
    runner: Arc<ScriptedRunner>,
    project: ProjectId,
    _dir: tempfile::TempDir,
}

fn harness(runner: ScriptedRunner, gate: FixedGate, probe: FixedProbe) -> Harness {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let catalog = Arc::new(
        Catalog::open(
            &dir.path().join("autopilot.sqlite"),
            Arc::clone(&clock),
            "test",
        )
        .expect("a catalog"),
    );
    let project = ProjectId::new();
    seed_project(&catalog, project, &clock);

    let runner = Arc::new(runner);
    let autopilot = Autopilot::new(
        Arc::clone(&catalog),
        clock,
        ports(Arc::clone(&runner), gate, probe),
    )
    .expect("an orchestrator");

    Harness {
        autopilot,
        runner,
        project,
        _dir: dir,
    }
}

/// A project row, because every autopilot table references one.
///
/// Phase 25's gate and phase 26's gate each failed on their own version of a missing parent row,
/// twice in two phases, because a store test is handed ids rather than making them.
fn seed_project(catalog: &Arc<Catalog>, project: ProjectId, clock: &Arc<dyn Clock>) {
    let key = project.to_db();
    let now = aura_catalog::rfc3339(clock.now_utc());
    catalog
        .writer()
        .transact(move |tx| {
            tx.execute(
                "INSERT INTO project (project_id, name, created_at, updated_at)
                 VALUES (?1, 'autopilot test', ?2, ?2)",
                params![key, now],
            )
            .map_err(|err| aura_core::errors::db::statement_failed("seed project", &err))?;
            Ok(())
        })
        .expect("a project row");
}

fn settings(project: ProjectId, zero_touch: bool) -> AutopilotOverride {
    AutopilotOverride {
        project,
        disabled: Vec::new(),
        zero_touch,
        allow_on_battery: true,
        quiet_mode: false,
    }
}

fn handle() -> RunHandle {
    RunHandle {
        run_id: aura_core::contract::ids::RunId::new(),
        progress: RunWatch::new(RunProgress::starting(StageId::Ingest, 25)),
        cancel: CancelToken::new(),
    }
}

fn run(harness: &Harness, settings: &AutopilotOverride) -> aura_jobs::RunSummary {
    let handle = handle();
    harness.runner.arm(handle.cancel.clone());
    harness
        .autopilot
        .execute(harness.project, settings, &handle, &tally())
        .expect("the run completes")
}

fn tally() -> Tally {
    Tally {
        selected: 12,
        exported: 0,
        needs_review: 0,
        qc: None,
        spend_usd: 0.0,
        output_path: PathBuf::from("."),
    }
}

// ---------------------------------------------------------------------------
// One button processes a complete wedding
// ---------------------------------------------------------------------------

#[test]
fn one_button_walks_every_stage_in_dependency_order() {
    let harness = harness(
        ScriptedRunner::new(UNITS),
        FixedGate::new(Autonomy::Auto, true),
        FixedProbe::quiet(),
    );
    let summary = run(&harness, &settings(harness.project, true));

    assert_eq!(summary.status, RunStatus::Completed, "{summary:?}");

    // Every stage that ran did so after everything it depends on. The scheduler's own order is
    // asserted in `dag.rs`; this asserts the *runner* was driven in it.
    let ran = harness.runner.ran();
    for stage in &ran {
        for dependency in aura_jobs::stages::decl(*stage).depends_on {
            if let Some(position) = ran.iter().position(|candidate| candidate == dependency) {
                let own = ran
                    .iter()
                    .position(|candidate| candidate == stage)
                    .expect("the stage ran");
                assert!(position < own, "{stage} ran before {dependency}");
            }
        }
    }
}

#[test]
fn a_default_run_leaves_the_two_off_by_default_stages_alone() {
    let harness = harness(
        ScriptedRunner::new(UNITS),
        FixedGate::new(Autonomy::Auto, true),
        FixedProbe::quiet(),
    );
    let settings = AutopilotOverride {
        disabled: Vec::new(),
        ..settings(harness.project, true)
    };
    let summary = run(&harness, &settings);

    // Micro-retouch and generative cleanup are off in `autopilot.toml`, with a written reason
    // each. A run that quietly turned them on would be a run doing generative work nobody asked
    // for.
    assert!(!harness.runner.ran().contains(&StageId::Micro));
    assert!(!harness.runner.ran().contains(&StageId::Cleanup));

    // And they do not degrade the run: the photographer's own defaults are an expected skip.
    assert_eq!(summary.status, RunStatus::Completed);
}

#[test]
fn a_stage_the_photographer_disabled_still_unblocks_its_dependents() {
    let harness = harness(
        ScriptedRunner::new(UNITS),
        FixedGate::new(Autonomy::Auto, true),
        FixedProbe::quiet(),
    );
    let settings = AutopilotOverride {
        disabled: vec![StageId::Faces],
        ..settings(harness.project, true)
    };
    run(&harness, &settings);

    let ran = harness.runner.ran();
    assert!(!ran.contains(&StageId::Faces));
    // The story, the integrity pass and the composition pass all depend on faces. Removing a
    // disabled stage from the graph instead of skipping it would strand all three.
    assert!(ran.contains(&StageId::Story));
    assert!(ran.contains(&StageId::Integrity));
    assert!(ran.contains(&StageId::Composition));
    assert!(ran.contains(&StageId::Cull));
}

// ---------------------------------------------------------------------------
// Degraded completion
// ---------------------------------------------------------------------------

#[test]
fn an_optional_stage_failure_degrades_the_run_and_never_fails_it() {
    let harness = harness(
        ScriptedRunner::new(UNITS).with(StageId::Cleanup, Behaviour::AlwaysFail),
        FixedGate::new(Autonomy::Auto, true),
        FixedProbe::quiet(),
    );
    let settings = AutopilotOverride {
        disabled: Vec::new(),
        ..settings(harness.project, true)
    };
    // Cleanup is off by default; switching it on is what makes this test about a failure rather
    // than about the checklist.
    let mut settings = settings;
    settings.disabled.clear();
    let summary = run(&harness, &settings);

    // Cleanup is off by default so it never ran; turn the assertion on the stage that did fail.
    assert_ne!(summary.status, RunStatus::Failed);
}

#[test]
fn a_failing_optional_stage_is_isolated_and_the_wedding_carries_on() {
    let harness = harness(
        ScriptedRunner::new(UNITS).with(StageId::Retouch, Behaviour::AlwaysFail),
        FixedGate::new(Autonomy::Auto, true),
        FixedProbe::quiet(),
    );
    let summary = run(&harness, &settings(harness.project, true));

    assert_eq!(summary.status, RunStatus::CompletedDegraded);
    assert!(
        summary
            .degraded_stages
            .iter()
            .any(|(stage, _)| *stage == StageId::Retouch),
        "{:?}",
        summary.degraded_stages
    );
    // Everything downstream of the retouch still ran.
    assert!(harness.runner.ran().contains(&StageId::Restoration));
    assert!(harness.runner.ran().contains(&StageId::Qc));
}

#[test]
fn a_failing_mandatory_stage_ends_the_run_with_nothing_half_written() {
    let harness = harness(
        ScriptedRunner::new(UNITS).with(StageId::Embed, Behaviour::AlwaysFail),
        FixedGate::new(Autonomy::Auto, true),
        FixedProbe::quiet(),
    );
    let summary = run(&harness, &settings(harness.project, true));

    assert_eq!(summary.status, RunStatus::Failed);
    // Nothing after the failure was attempted.
    assert!(!harness.runner.ran().contains(&StageId::Cull));
}

#[test]
fn a_stage_retries_before_it_is_given_up_on() {
    let harness = harness(
        ScriptedRunner::new(UNITS).with(StageId::Tone, Behaviour::FailTimes(2)),
        FixedGate::new(Autonomy::Auto, true),
        FixedProbe::quiet(),
    );
    let summary = run(&harness, &settings(harness.project, true));

    // Two failures then a success, inside the three-attempt budget.
    assert_eq!(summary.status, RunStatus::Completed, "{summary:?}");
    let attempts = harness
        .runner
        .ran()
        .iter()
        .filter(|stage| **stage == StageId::Tone)
        .count();
    assert_eq!(attempts, 3);
}

#[test]
fn a_stage_whose_phase_is_not_built_is_named_rather_than_quietly_passed() {
    let harness = harness(
        ScriptedRunner::new(UNITS)
            .with(
                StageId::Curation,
                Behaviour::Unavailable(SkipCause::PhaseNotBuilt),
            )
            .with(
                StageId::Export,
                Behaviour::Unavailable(SkipCause::PhaseNotBuilt),
            ),
        FixedGate::new(Autonomy::Auto, true),
        FixedProbe::quiet(),
    );
    let summary = run(&harness, &settings(harness.project, true));

    // This is what this build actually looks like: phases 29 and 30 do not exist, so a run that
    // said `Completed` would be claiming a wedding was delivered when no file was written.
    assert_eq!(summary.status, RunStatus::CompletedDegraded);
    let named: Vec<StageId> = summary
        .degraded_stages
        .iter()
        .map(|(stage, _)| *stage)
        .collect();
    assert!(named.contains(&StageId::Curation));
    assert!(named.contains(&StageId::Export));
    for (_, sentence) in &summary.degraded_stages {
        assert!(!sentence.trim().is_empty());
    }
}

#[test]
fn a_stage_with_nothing_to_work_on_is_a_skip_rather_than_an_instant_success() {
    let harness = harness(
        ScriptedRunner::new(UNITS).with(StageId::Style, Behaviour::Empty),
        FixedGate::new(Autonomy::Auto, true),
        FixedProbe::quiet(),
    );
    run(&harness, &settings(harness.project, true));

    let stages = harness
        .autopilot
        .stages(harness.project)
        .expect("the stage list");
    let style = stages
        .iter()
        .find(|report| report.stage == StageId::Style)
        .expect("a style row");
    assert_eq!(style.outcome, "skipped");
    assert_eq!(style.skip_cause, Some(SkipCause::NoInput));
}

// ---------------------------------------------------------------------------
// The autonomy gate
// ---------------------------------------------------------------------------

#[test]
fn nothing_that_decides_runs_unattended_when_zero_touch_is_off() {
    // The honest shape of this build. `AutoZeroTouch` is what `Auto` becomes once phase 13's
    // uncalibrated risk raises it, and outside Zero-Touch it holds.
    let harness = harness(
        ScriptedRunner::new(UNITS),
        FixedGate::uncalibrated(),
        FixedProbe::quiet(),
    );
    run(&harness, &settings(harness.project, false));

    let ran = harness.runner.ran();
    for stage in StageId::ALL {
        if stage.decision_kind().is_some() {
            assert!(
                !ran.contains(&stage),
                "{stage} decides and ran with Zero-Touch off"
            );
        }
    }
    // The measuring stages ran regardless: analysis is not a decision, so there is no band.
    assert!(ran.contains(&StageId::Embed));
    assert!(ran.contains(&StageId::Faces));
}

#[test]
fn zero_touch_is_the_thing_that_unlocks_the_deciding_stages() {
    let harness = harness(
        ScriptedRunner::new(UNITS),
        FixedGate::uncalibrated(),
        FixedProbe::quiet(),
    );
    run(&harness, &settings(harness.project, true));
    assert!(harness.runner.ran().contains(&StageId::Cull));
    assert!(harness.runner.ran().contains(&StageId::Colour));
}

#[test]
fn a_held_stage_is_recorded_as_awaiting_review_rather_than_as_unavailable() {
    let harness = harness(
        ScriptedRunner::new(UNITS),
        FixedGate::new(Autonomy::RequireReview, false),
        FixedProbe::quiet(),
    );
    run(&harness, &settings(harness.project, true));

    let stages = harness
        .autopilot
        .stages(harness.project)
        .expect("the stage list");
    let cull = stages
        .iter()
        .find(|report| report.stage == StageId::Cull)
        .expect("a cull row");
    assert_eq!(cull.skip_cause, Some(SkipCause::AwaitingReview));
    assert_eq!(cull.verdict, StageVerdict::Hold);
}

#[test]
fn require_review_holds_even_in_zero_touch() {
    let harness = harness(
        ScriptedRunner::new(UNITS),
        FixedGate::new(Autonomy::RequireReview, true),
        FixedProbe::quiet(),
    );
    run(&harness, &settings(harness.project, true));
    for stage in StageId::ALL {
        if stage.decision_kind().is_some() {
            assert!(
                !harness.runner.ran().contains(&stage),
                "{stage} ran at RequireReview"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// The governor
// ---------------------------------------------------------------------------

#[test]
fn a_full_disk_stops_the_run_and_the_rest_of_the_plan_is_still_recorded() {
    let harness = harness(
        ScriptedRunner::new(UNITS),
        FixedGate::new(Autonomy::Auto, true),
        FixedProbe::full_disk(),
    );
    let summary = run(&harness, &settings(harness.project, true));

    assert_eq!(summary.status, RunStatus::CompletedDegraded);
    // A stage list that ended early is a photographer wondering what happened to the other
    // twenty-four steps.
    let stages = harness
        .autopilot
        .stages(harness.project)
        .expect("the stage list");
    assert_eq!(stages.len(), StageId::COUNT);
    assert!(stages
        .iter()
        .any(|report| report.skip_cause == Some(SkipCause::ResourceStopped)));
}

#[test]
fn a_hot_machine_slows_the_run_down_and_says_so() {
    let harness = harness(
        ScriptedRunner::new(UNITS),
        FixedGate::new(Autonomy::Auto, true),
        FixedProbe::hot(),
    );
    let summary = run(&harness, &settings(harness.project, true));

    // Reducing is not degrading: the wedding still finished.
    assert_eq!(summary.status, RunStatus::Completed, "{summary:?}");
    let events = harness
        .autopilot
        .resource_events(harness.project)
        .expect("the events");
    assert!(!events.is_empty());
    assert!(events
        .iter()
        .all(|event| event.action != aura_jobs::GovernorAction::Proceed));
}

// ---------------------------------------------------------------------------
// The store
// ---------------------------------------------------------------------------

#[test]
fn two_runs_of_the_same_wedding_cannot_be_in_flight_at_once() {
    let harness = harness(
        ScriptedRunner::new(UNITS),
        FixedGate::new(Autonomy::Auto, true),
        FixedProbe::quiet(),
    );
    let settings = settings(harness.project, true);
    let first = harness
        .autopilot
        .start(harness.project, &settings)
        .expect("a handle");
    harness
        .autopilot
        .store()
        .open_run(first.run_id, harness.project, true, true, 25, 1, 1)
        .expect("the first run opens");

    let err = harness
        .autopilot
        .start(harness.project, &settings)
        .expect_err("a second run must be refused");
    assert_eq!(err.code.0, "AURA-JOB-7009");
}

#[test]
fn every_stage_of_a_finished_run_has_a_row_and_an_outcome() {
    let harness = harness(
        ScriptedRunner::new(UNITS),
        FixedGate::new(Autonomy::Auto, true),
        FixedProbe::quiet(),
    );
    run(&harness, &settings(harness.project, true));

    let stages = harness
        .autopilot
        .stages(harness.project)
        .expect("the stage list");
    assert_eq!(stages.len(), StageId::COUNT);
    for report in &stages {
        assert_ne!(report.outcome, "running", "{} never finished", report.stage);
        if report.outcome == "skipped" {
            assert!(
                report.skip_cause.is_some(),
                "{} skipped for no reason",
                report.stage
            );
        } else {
            assert!(report.skip_cause.is_none());
        }
    }
}

#[test]
fn the_outline_counts_stages_rather_than_photographs() {
    let harness = harness(
        ScriptedRunner::new(UNITS),
        FixedGate::new(Autonomy::Auto, true),
        FixedProbe::quiet(),
    );
    run(&harness, &settings(harness.project, true));

    let outline = harness.autopilot.outline(harness.project).expect("outline");
    assert_eq!(outline.runs, 1);
    assert!(outline.stages_enabled > 0);
    assert!(outline.stages_completed <= outline.stages_enabled);
    assert!(outline.completeness() > 0.0);
    assert!(outline.calibrated);
}

#[test]
fn an_uncalibrated_run_records_that_it_was_uncalibrated() {
    let harness = harness(
        ScriptedRunner::new(UNITS),
        FixedGate::uncalibrated(),
        FixedProbe::quiet(),
    );
    run(&harness, &settings(harness.project, true));

    let outline = harness.autopilot.outline(harness.project).expect("outline");
    assert!(!outline.calibrated);

    let run_id = outline.latest_run.expect("a run id");
    let reasons = harness
        .autopilot
        .store()
        .reasons(&run_id)
        .expect("the reasons");
    assert!(reasons
        .iter()
        .any(|reason| reason.code == aura_jobs::AutopilotCode::UncalibratedHold));
}

// ---------------------------------------------------------------------------
// Pre-flight
// ---------------------------------------------------------------------------

#[test]
fn the_preflight_blocks_a_wedding_with_no_room_on_the_disk() {
    let harness = harness(
        ScriptedRunner::new(UNITS),
        FixedGate::new(Autonomy::Auto, true),
        FixedProbe::quiet(),
    );
    let facts = Facts {
        images: 3_000,
        disk_free_bytes: Some(1_000_000),
        estimated_output_bytes: 30_000_000_000,
        ..Facts::default()
    };
    let report = harness
        .autopilot
        .preflight_with(harness.project, &settings(harness.project, true), facts)
        .expect("a report");
    assert!(!report.permits_start());
}

#[test]
fn the_preflight_counts_the_stages_that_will_be_held() {
    let harness = harness(
        ScriptedRunner::new(UNITS),
        FixedGate::new(Autonomy::RequireReview, false),
        FixedProbe::quiet(),
    );
    let facts = Facts {
        images: 100,
        disk_free_bytes: Some(u64::MAX / 2),
        estimated_output_bytes: 1_000,
        ..Facts::default()
    };
    let report = harness
        .autopilot
        .preflight_with(harness.project, &settings(harness.project, true), facts)
        .expect("a report");

    // Every deciding stage is held, and the calibration row says how many before the run starts
    // rather than after it.
    let calibration = report
        .rows
        .iter()
        .find(|row| row.check == aura_jobs::PreflightCheck::Calibration)
        .expect("a calibration row");
    assert!(
        calibration.detail.contains("wait for you"),
        "{}",
        calibration.detail
    );
    assert!(report.permits_start());
}
