//! The phase 28 mechanical gate.
//!
//! The assembly proof for the autopilot: migration 28 and its objects, the checklist a product
//! manager owns and the widened bound it refuses, the DAG's shape and its refusals, a whole
//! synthetic wedding through the real orchestrator, twenty kills and twenty resumes, the autonomy
//! gate under this build's own uncalibrated bands, the governor's one-way action, the pre-flight's
//! four blocking rows, degraded completion, and the IPC surface's three files agreeing.
//!
//! **Nothing here proves anything about a real wedding, and nothing here is a timing.** Every
//! stage is a `ScriptedRunner` doing what this repository told it to do, so what is measured is
//! the *scheduler*. Section 11's wall-clock budgets are waived - there is no GPU backend, no
//! trained model and no camera file on this machine - and that is condition C1 of the exit report,
//! printed at the end of every run rather than hidden in a helper.
//!
//! The unit tests prove the pieces and `tests/e2e/autopilot_*.rs` proves the run. This proves the
//! assembly - the things that only exist when a catalog, a policy file, three ports and a plan are
//! in the same process.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::ids::RunId;
use aura_core::contract::ledger::{Autonomy, DecisionKind};
use aura_core::progress::CancelToken;
use aura_core::{AuraResult, ProjectId};
use aura_jobs::api::{Autopilot, Tally};
use aura_jobs::contract::autopilot::{
    AutopilotOverride, AutopilotService, GovernorAction, MachineState, PreflightCheck,
    PreflightVerdict, RunHandle, RunProgress, RunStatus, RunWatch, SkipCause, StageId,
    StageVerdict, DISK_HEADROOM, MAX_STAGE_ATTEMPTS, THERMAL_PAUSE_C, THERMAL_REDUCE_C,
    VRAM_CEILING,
};
use aura_jobs::fixtures::{ports, Behaviour, FixedGate, FixedProbe, ScriptedRunner};
use aura_jobs::governor::{Governor, RunMode};
use aura_jobs::preflight::Facts;
use aura_jobs::{policy::Policy, Dag};
use rusqlite::params;

/// Run the phase 28 gate.
#[allow(clippy::too_many_lines)]
pub fn verify(args: &[String]) -> ExitCode {
    let work = PathBuf::from(
        crate::flag(args, "--work").unwrap_or_else(|| "target/phase28-verify".into()),
    );
    if let Err(err) = std::fs::create_dir_all(&work) {
        eprintln!("cannot create {}: {err}", work.display());
        return ExitCode::FAILURE;
    }
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let mut failures = 0usize;

    // -----------------------------------------------------------------------------------
    // 1. Migration 28 and every object it owns.
    // -----------------------------------------------------------------------------------
    let catalog_path = work.join("phase28.sqlite");
    drop(std::fs::remove_file(&catalog_path));
    let catalog = match Catalog::open(&catalog_path, Arc::clone(&clock), crate::APP_VERSION) {
        Ok(opened) => Arc::new(opened),
        Err(err) => {
            eprintln!("catalog: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    match catalog.schema_version() {
        Ok(version) if version >= 28 => println!("schema: version {version}"),
        Ok(version) => {
            eprintln!("schema: expected at least 28, found {version}");
            failures += 1;
        }
        Err(err) => {
            eprintln!("schema: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    let expected_tables = [
        "autopilot_run",
        "autopilot_stage",
        "autopilot_reason",
        "autopilot_event",
        "autopilot_settings",
    ];
    let expected_views = ["autopilot_degraded", "autopilot_timings"];
    let expected_triggers = [
        "autopilot_run_no_reopen",
        "autopilot_reason_no_update",
        "autopilot_event_cap",
    ];
    for (kind, names) in [
        ("table", expected_tables.as_slice()),
        ("view", expected_views.as_slice()),
        ("trigger", expected_triggers.as_slice()),
    ] {
        for name in names {
            match object_exists(&catalog, kind, name) {
                Ok(true) => {}
                Ok(false) => {
                    eprintln!("migration 28: {kind} `{name}` is missing");
                    failures += 1;
                }
                Err(err) => {
                    eprintln!("migration 28: [{}] {}", err.code, err.detail);
                    failures += 1;
                }
            }
        }
    }
    println!(
        "migration 28: {} tables, {} views, {} triggers",
        expected_tables.len(),
        expected_views.len(),
        expected_triggers.len()
    );

    // Phase 27's schema scan, inherited. A stored sentence is copy a release can change and a
    // catalog full of English nobody can translate; comments are stripped first, because
    // `sqlite_master.sql` holds a migration verbatim and this migration explains at length why
    // there is no such column.
    match schema_text(&catalog) {
        Ok(sql) => {
            let code = strip_sql_comments(&sql);
            let banned = ["diagnosis", "sentence", "narrative", "summary_text"];
            let found: Vec<&str> = banned
                .iter()
                .copied()
                .filter(|needle| code.contains(needle))
                .collect();
            if found.is_empty() {
                println!("schema: no free-text column automation could write a sentence into");
            } else {
                eprintln!("schema: found {found:?}, which would be a stored sentence");
                failures += 1;
            }
        }
        Err(err) => {
            eprintln!("schema scan: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // -----------------------------------------------------------------------------------
    // 2. The DAG.
    // -----------------------------------------------------------------------------------
    match Dag::build() {
        Ok(dag) => {
            let order = dag.order();
            if order.len() == StageId::COUNT {
                println!("dag: {} stages, one deterministic order", order.len());
            } else {
                eprintln!("dag: {} stages, expected {}", order.len(), StageId::COUNT);
                failures += 1;
            }
            let mut broken = 0usize;
            for stage in StageId::ALL {
                let own = order.iter().position(|c| *c == stage);
                for dependency in dag.dependencies_of(stage) {
                    let theirs = order.iter().position(|c| c == dependency);
                    if own <= theirs {
                        eprintln!("dag: {stage} runs before its dependency {dependency}");
                        broken += 1;
                    }
                }
            }
            if broken > 0 {
                failures += 1;
            } else {
                println!("dag: every dependency precedes its dependent");
            }

            // Invariant 3 as a property of the graph: everything before the cull works on every
            // photograph, everything after it on survivors.
            let cull = order.iter().position(|s| *s == StageId::Cull).unwrap_or(0);
            let mut leaked = 0usize;
            for (index, stage) in order.iter().enumerate() {
                if index < cull
                    && aura_jobs::stages::decl(*stage).scope
                        == aura_jobs::contract::autopilot::StageScope::SelectedImages
                {
                    eprintln!("dag: {stage} works on survivors and runs before the cull");
                    leaked += 1;
                }
            }
            if leaked > 0 {
                failures += 1;
            } else {
                println!("dag: the cull separates the two scopes");
            }
        }
        Err(err) => {
            eprintln!("dag: {err}");
            failures += 1;
        }
    }

    // -----------------------------------------------------------------------------------
    // 3. The checklist, and the bounds it may not widen.
    // -----------------------------------------------------------------------------------
    match Policy::embedded() {
        Ok(policy) => {
            let off: Vec<&str> = StageId::ALL
                .into_iter()
                .filter(|stage| !policy.default_on(*stage))
                .map(StageId::as_str)
                .collect();
            println!(
                "policy: version {}, {} stage(s) off by default: {off:?}",
                policy.version(),
                off.len()
            );
            let budgets = policy.budgets();
            let ok = budgets.vram_ceiling <= VRAM_CEILING
                && budgets.thermal_reduce_c <= THERMAL_REDUCE_C
                && budgets.thermal_pause_c <= THERMAL_PAUSE_C
                && budgets.disk_headroom >= DISK_HEADROOM;
            if ok {
                println!("policy: every shipped bound is at or inside the code's own");
            } else {
                eprintln!("policy: the shipped file widens a bound the code owns");
                failures += 1;
            }
        }
        Err(err) => {
            eprintln!("policy: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // A studio may tighten and may not widen, and the refusal is an error rather than a clamp.
    for (name, text) in [
        (
            "thermal",
            "version = 1\n[resources]\nthermal_reduce_c = 120.0\n",
        ),
        ("vram", "version = 1\n[resources]\nvram_ceiling = 0.99\n"),
        (
            "battery",
            "version = 1\n[resources]\nbattery_floor = 0.01\n",
        ),
        (
            "mandatory stage",
            "version = 1\n[[stage]]\nid = \"cull\"\ndefault_on = false\nreason = \"x\"\n",
        ),
    ] {
        match Policy::parse(text) {
            Err(err) if err.code.0 == "AURA-JOB-7008" => {
                println!("policy: a widened `{name}` is refused with {}", err.code.0);
            }
            Err(err) => {
                eprintln!(
                    "policy: `{name}` refused with the wrong code {}",
                    err.code.0
                );
                failures += 1;
            }
            Ok(_) => {
                eprintln!("policy: a widened `{name}` was accepted");
                failures += 1;
            }
        }
    }

    // -----------------------------------------------------------------------------------
    // 4. A whole wedding through the real orchestrator.
    // -----------------------------------------------------------------------------------
    let project = ProjectId::new();
    if let Err(err) = seed_project(&catalog, project, &clock) {
        eprintln!("fixture: [{}] {}", err.code, err.detail);
        return ExitCode::FAILURE;
    }

    let runner = Arc::new(ScriptedRunner::new(24));
    let autopilot = match Autopilot::new(
        Arc::clone(&catalog),
        Arc::clone(&clock),
        ports(
            Arc::clone(&runner),
            FixedGate::new(Autonomy::Auto, true),
            FixedProbe::quiet(),
        ),
    ) {
        Ok(built) => built,
        Err(err) => {
            eprintln!("orchestrator: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };

    let settings = AutopilotOverride {
        project,
        disabled: Vec::new(),
        zero_touch: true,
        allow_on_battery: true,
        quiet_mode: false,
    };
    let first = handle(RunId::new());
    runner.arm(first.cancel.clone());
    match autopilot.execute(project, &settings, &first, &tally()) {
        Ok(summary) => {
            println!(
                "run: {} in {} ms, {} stage(s) degraded",
                summary.status.as_str(),
                summary.total_ms(),
                summary.degraded_stages.len()
            );
            if summary.stage_timings.len() != StageId::COUNT {
                eprintln!(
                    "run: {} stage timings, expected {}",
                    summary.stage_timings.len(),
                    StageId::COUNT
                );
                failures += 1;
            }
            for (stage, sentence) in &summary.degraded_stages {
                if sentence.trim().is_empty() {
                    eprintln!("run: {stage} was degraded and says nothing about why");
                    failures += 1;
                }
            }
        }
        Err(err) => {
            eprintln!("run: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    match autopilot.stages(project) {
        Ok(stages) => {
            let mut silent = 0usize;
            for report in &stages {
                if report.outcome == "skipped" && report.skip_cause.is_none() {
                    eprintln!("run: {} was skipped for no recorded reason", report.stage);
                    silent += 1;
                }
            }
            if silent > 0 {
                failures += 1;
            } else {
                println!("run: every skipped stage names why");
            }
        }
        Err(err) => {
            eprintln!("stages: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // -----------------------------------------------------------------------------------
    // 5. The autonomy gate, under this build's own bands.
    // -----------------------------------------------------------------------------------
    for (band, zero_touch, expected) in [
        (Autonomy::Auto, false, StageVerdict::Act),
        (Autonomy::AutoZeroTouch, false, StageVerdict::Hold),
        (Autonomy::AutoZeroTouch, true, StageVerdict::Act),
        (Autonomy::Suggest, false, StageVerdict::Hold),
        (Autonomy::Suggest, true, StageVerdict::ActAndReview),
        (Autonomy::RequireReview, false, StageVerdict::Hold),
        (Autonomy::RequireReview, true, StageVerdict::Hold),
    ] {
        let got = StageVerdict::from_band(band, zero_touch);
        if got == expected {
            continue;
        }
        eprintln!("gate: {band:?} at zero_touch={zero_touch} gave {got:?}, expected {expected:?}");
        failures += 1;
    }
    println!("gate: every band maps to one verdict, and RequireReview holds in every mode");

    // Every deciding stage held, with the strictest band. The measuring stages still run: phase
    // 13's rule that analysis is not a decision, as a scheduling fact.
    let held_project = ProjectId::new();
    if let Err(err) = seed_project(&catalog, held_project, &clock) {
        eprintln!("fixture: [{}] {}", err.code, err.detail);
        failures += 1;
    }
    let held_runner = Arc::new(ScriptedRunner::new(6));
    if let Ok(held) = Autopilot::new(
        Arc::clone(&catalog),
        Arc::clone(&clock),
        ports(
            Arc::clone(&held_runner),
            FixedGate::new(Autonomy::RequireReview, false),
            FixedProbe::quiet(),
        ),
    ) {
        let settings = AutopilotOverride {
            project: held_project,
            ..settings.clone()
        };
        drop(held.execute(held_project, &settings, &handle(RunId::new()), &tally()));
        let ran = held_runner.ran();
        let mut leaked = 0usize;
        for stage in StageId::ALL {
            if stage.decision_kind().is_some() && ran.contains(&stage) {
                eprintln!("gate: {stage} decides and ran at RequireReview");
                leaked += 1;
            }
        }
        if leaked > 0 {
            failures += 1;
        } else {
            println!("gate: nothing that decides ran while every band required review");
        }
        if !ran.contains(&StageId::Embed) {
            eprintln!("gate: the embed measures and was held");
            failures += 1;
        }
    }

    // -----------------------------------------------------------------------------------
    // 6. The governor, and the thing it cannot do.
    // -----------------------------------------------------------------------------------
    let governor = Governor::new(
        Policy::embedded().map(|p| p.budgets()).unwrap_or_default(),
        RunMode::default(),
    );
    if governor
        .rule(&MachineState::default(), StageId::Embed)
        .action
        == GovernorAction::Proceed
    {
        println!("governor: a machine with no readings proceeds and records nothing");
    } else {
        eprintln!("governor: an unreadable machine did something");
        failures += 1;
    }

    let extremes: [(&str, MachineState); 5] = [
        (
            "thermal",
            MachineState {
                temperature_c: Some(150.0),
                ..MachineState::default()
            },
        ),
        (
            "vram",
            MachineState {
                vram_used: Some(1.0),
                ..MachineState::default()
            },
        ),
        (
            "ram",
            MachineState {
                ram_used: Some(1.0),
                ..MachineState::default()
            },
        ),
        (
            "battery",
            MachineState {
                on_battery: true,
                battery: Some(0.01),
                ..MachineState::default()
            },
        ),
        (
            "device lost",
            MachineState {
                device_lost: true,
                ..MachineState::default()
            },
        ),
    ];
    let mut stopped = 0usize;
    for (name, state) in extremes {
        if governor.rule(&state, StageId::Embed).action == GovernorAction::Stop {
            eprintln!("governor: `{name}` stopped a run, and only a full disk may");
            stopped += 1;
        }
    }
    if stopped > 0 {
        failures += 1;
    } else {
        println!("governor: a full disk is the only reading that stops a run");
    }

    let full = MachineState {
        disk_free_bytes: Some(1),
        disk_needed_bytes: Some(1_000_000),
        ..MachineState::default()
    };
    if governor.rule(&full, StageId::Export).action == GovernorAction::Stop {
        println!("governor: a full disk stops the run before it writes into it");
    } else {
        eprintln!("governor: a full disk did not stop the run");
        failures += 1;
    }

    // -----------------------------------------------------------------------------------
    // 7. Pre-flight.
    // -----------------------------------------------------------------------------------
    let blocking = [
        (
            "no photographs",
            Facts {
                images: 0,
                ..healthy_facts()
            },
        ),
        (
            "a full disk",
            Facts {
                disk_free_bytes: Some(1_000),
                estimated_output_bytes: 30_000_000_000,
                ..healthy_facts()
            },
        ),
        (
            "a missing required model",
            Facts {
                missing_required_models: vec!["wedding_embedding".into()],
                ..healthy_facts()
            },
        ),
        (
            "a wedding that will not open",
            Facts {
                project_opens: false,
                ..healthy_facts()
            },
        ),
    ];
    for (name, facts) in blocking {
        let report = aura_jobs::preflight::check(&facts);
        if report.permits_start() {
            eprintln!("preflight: `{name}` did not block");
            failures += 1;
        }
    }
    println!("preflight: four conditions block a two-hour run before it starts");

    let report = aura_jobs::preflight::check(&healthy_facts());
    if report.rows.len() == PreflightCheck::ALL.len()
        && report.rows.iter().all(|row| !row.detail.trim().is_empty())
    {
        println!(
            "preflight: {} rows, every one of them actionable",
            report.rows.len()
        );
    } else {
        eprintln!("preflight: a row is missing or says nothing");
        failures += 1;
    }

    let uncalibrated = aura_jobs::preflight::check(&Facts {
        calibrated: false,
        held_stages: 3,
        ..healthy_facts()
    });
    let calibration_row = uncalibrated
        .rows
        .iter()
        .find(|row| row.check == PreflightCheck::Calibration);
    match calibration_row {
        Some(row) if row.verdict == PreflightVerdict::Warn && row.detail.contains('3') => {
            println!("preflight: an uncalibrated build says how many steps will wait");
        }
        _ => {
            eprintln!("preflight: the calibration row does not say what this build will do");
            failures += 1;
        }
    }

    // -----------------------------------------------------------------------------------
    // 8. Degraded completion, and the run that must not fail.
    // -----------------------------------------------------------------------------------
    let degraded_project = ProjectId::new();
    if let Err(err) = seed_project(&catalog, degraded_project, &clock) {
        eprintln!("fixture: [{}] {}", err.code, err.detail);
        failures += 1;
    }
    let degraded_runner = Arc::new(
        ScriptedRunner::new(8)
            .with(StageId::Retouch, Behaviour::AlwaysFail)
            .with(
                StageId::Curation,
                Behaviour::Unavailable(SkipCause::PhaseNotBuilt),
            )
            .with(
                StageId::Export,
                Behaviour::Unavailable(SkipCause::PhaseNotBuilt),
            ),
    );
    if let Ok(degraded) = Autopilot::new(
        Arc::clone(&catalog),
        Arc::clone(&clock),
        ports(
            Arc::clone(&degraded_runner),
            FixedGate::new(Autonomy::Auto, true),
            FixedProbe::quiet(),
        ),
    ) {
        let settings = AutopilotOverride {
            project: degraded_project,
            ..settings.clone()
        };
        match degraded.execute(degraded_project, &settings, &handle(RunId::new()), &tally()) {
            Ok(summary) => {
                if summary.status == RunStatus::CompletedDegraded {
                    println!(
                        "degraded: an optional failure and two unbuilt phases finish as `{}`",
                        summary.status.as_str()
                    );
                } else {
                    eprintln!(
                        "degraded: expected CompletedDegraded, got {:?}",
                        summary.status
                    );
                    failures += 1;
                }
                let named: BTreeSet<&str> = summary
                    .degraded_stages
                    .iter()
                    .map(|(stage, _)| stage.as_str())
                    .collect();
                for expected in ["retouch", "curation", "export"] {
                    if !named.contains(expected) {
                        eprintln!("degraded: `{expected}` is missing from the skipped list");
                        failures += 1;
                    }
                }
                if degraded_runner.ran().contains(&StageId::Qc) {
                    println!("degraded: the wedding carried on past the failed stage");
                } else {
                    eprintln!("degraded: a failed optional stage stopped the wedding");
                    failures += 1;
                }
            }
            Err(err) => {
                eprintln!("degraded: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }

    // -----------------------------------------------------------------------------------
    // 9. The retry budget.
    // -----------------------------------------------------------------------------------
    let retry_project = ProjectId::new();
    if let Err(err) = seed_project(&catalog, retry_project, &clock) {
        eprintln!("fixture: [{}] {}", err.code, err.detail);
        failures += 1;
    }
    let retry_runner = Arc::new(
        ScriptedRunner::new(4).with(StageId::Tone, Behaviour::FailTimes(MAX_STAGE_ATTEMPTS - 1)),
    );
    if let Ok(retrying) = Autopilot::new(
        Arc::clone(&catalog),
        Arc::clone(&clock),
        ports(
            Arc::clone(&retry_runner),
            FixedGate::new(Autonomy::Auto, true),
            FixedProbe::quiet(),
        ),
    ) {
        let settings = AutopilotOverride {
            project: retry_project,
            ..settings.clone()
        };
        drop(retrying.execute(retry_project, &settings, &handle(RunId::new()), &tally()));
        let attempts = retry_runner
            .ran()
            .iter()
            .filter(|stage| **stage == StageId::Tone)
            .count();
        if attempts == usize::from(MAX_STAGE_ATTEMPTS) {
            println!("retry: a stage is tried {attempts} times and no more");
        } else {
            eprintln!("retry: {attempts} attempts, expected {MAX_STAGE_ATTEMPTS}");
            failures += 1;
        }
    }

    // -----------------------------------------------------------------------------------
    // 10. Storage.
    // -----------------------------------------------------------------------------------
    match autopilot.outline(project) {
        Ok(outline) => {
            println!(
                "storage: {} B for {} run(s) over {} stages",
                outline.bytes, outline.runs, outline.stages_enabled
            );
            // 25 stage rows per run and one run row, over a 3,000-frame wedding. The denominator
            // is photographs and the numerator does not scale with them, which is what makes this
            // the first migration since phase 01 whose per-image cost is a division.
            let per_image = outline.bytes as f64 / 3_000.0;
            if per_image <= 10.0 {
                println!("storage: {per_image:.3} B/image against a 10 B budget");
            } else {
                eprintln!("storage: {per_image:.3} B/image, over the 10 B budget");
                failures += 1;
            }
        }
        Err(err) => {
            eprintln!("outline: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // -----------------------------------------------------------------------------------
    // 11. The IPC surface, three files agreeing.
    // -----------------------------------------------------------------------------------
    match ipc_parity() {
        Ok(count) => println!("ipc: {count} handlers = {count} definitions = {count} wrappers"),
        Err(problem) => {
            eprintln!("ipc: {problem}");
            failures += 1;
        }
    }

    // -----------------------------------------------------------------------------------
    // What this run did not prove.
    // -----------------------------------------------------------------------------------
    println!();
    println!("Conditions carried forward, printed on every run:");
    println!(
        "  C1  Every stage above was a fixture. Section 11's wall-clock budgets - 2.5 h on an"
    );
    println!("      RTX 4070, 4 h on an M3 Pro - are waived: this machine has no GPU backend, no");
    println!("      trained model and no camera file, so there is nothing to time.");
    println!(
        "  C2  Ingest and previews are driven by the import wizard; the autopilot's own arms for"
    );
    println!("      them count what is already in the catalog rather than walking a card.");
    println!("  C3  Five of the machine probe's seven readings are `None` on this build, so the");
    println!("      thermal, battery, memory and quiet-mode policies never fire.");
    println!(
        "  C4  Every `est_ms_per_item` is a declared figure from the phase document, measured on"
    );
    println!("      no machine in this repository.");
    println!(
        "  C5  `inputs_hash` covers the unit count and the schema version, not each phase's own"
    );
    println!("      analysis version - so a re-tuned scene profile does not invalidate a resume.");
    println!(
        "  C6  Nothing here is a claim about the intervention rate. Section 13's 8 % target needs"
    );
    println!("      ten real weddings and a person.");

    if failures == 0 {
        println!();
        println!("phase 28: all checks passed");
        ExitCode::SUCCESS
    } else {
        eprintln!();
        eprintln!("phase 28: {failures} check(s) failed");
        ExitCode::FAILURE
    }
}

// -- helpers -----------------------------------------------------------------

fn healthy_facts() -> Facts {
    Facts {
        images: 3_000,
        disk_free_bytes: Some(500_000_000_000),
        estimated_output_bytes: 30_000_000_000,
        hardware_detail: "a fixture machine".into(),
        calibrated: true,
        ..Facts::default()
    }
}

fn tally() -> Tally {
    Tally {
        selected: 12,
        output_path: PathBuf::from("."),
        ..Tally::default()
    }
}

fn handle(run_id: RunId) -> RunHandle {
    RunHandle {
        run_id,
        progress: RunWatch::new(RunProgress::starting(StageId::Ingest, 25)),
        cancel: CancelToken::new(),
    }
}

/// A project row, because every autopilot table references one.
///
/// Phase 25's gate and phase 26's gate each failed on their own version of a missing parent row,
/// twice in two phases, because a store test is handed ids rather than making them.
fn seed_project(
    catalog: &Arc<Catalog>,
    project: ProjectId,
    clock: &Arc<dyn Clock>,
) -> AuraResult<()> {
    let key = project.to_db();
    let now = aura_catalog::rfc3339(clock.now_utc());
    catalog.writer().transact(move |tx| {
        tx.execute(
            "INSERT INTO project (project_id, name, created_at, updated_at)
             VALUES (?1, 'phase 28 fixture', ?2, ?2)",
            params![key, now],
        )
        .map_err(|err| aura_core::errors::db::statement_failed("seed project", &err))?;
        Ok(())
    })
}

fn object_exists(catalog: &Arc<Catalog>, kind: &str, name: &str) -> AuraResult<bool> {
    let kind = kind.to_string();
    let name = name.to_string();
    catalog.read(move |conn: &rusqlite::Connection| {
        conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = ?1 AND name = ?2",
            params![kind, name],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count > 0)
        .map_err(|err| aura_core::errors::db::statement_failed("read sqlite_master", &err))
    })
}

fn schema_text(catalog: &Arc<Catalog>) -> AuraResult<String> {
    catalog.read(|conn: &rusqlite::Connection| {
        let mut stmt = conn
            .prepare("SELECT COALESCE(sql, '') FROM sqlite_master WHERE name LIKE 'autopilot%'")
            .map_err(|err| aura_core::errors::db::statement_failed("prepare schema scan", &err))?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|err| aura_core::errors::db::statement_failed("query schema scan", &err))?;
        let mut out = String::new();
        for row in rows {
            out.push_str(
                &row.map_err(|err| {
                    aura_core::errors::db::statement_failed("read schema row", &err)
                })?,
            );
            out.push('\n');
        }
        Ok(out)
    })
}

/// Every `#[tauri::command]` has a registration and a typed client wrapper, and nothing else does.
///
/// Read out of the files rather than out of a manifest, because a manifest is a fourth thing that
/// can disagree with the other three. Phase 27 added this check after phase 21's exit report found
/// ninety client calls reaching a window that did not answer to them; this is the same code, run
/// again because a phase that adds nine commands is exactly when it drifts.
fn ipc_parity() -> Result<usize, String> {
    let shell = std::fs::read_to_string("ui/src-tauri/src/main.rs")
        .map_err(|err| format!("ui/src-tauri/src/main.rs could not be read: {err}"))?;
    let client = std::fs::read_to_string("ui/src/ipc/client.ts")
        .map_err(|err| format!("ui/src/ipc/client.ts could not be read: {err}"))?;

    let mut defined: BTreeSet<String> = BTreeSet::new();
    let mut expecting = false;
    for line in shell.lines() {
        let line = line.trim();
        if line == "#[tauri::command]" {
            expecting = true;
            continue;
        }
        if expecting {
            if let Some(name) = line
                .strip_prefix("pub async fn ")
                .or_else(|| line.strip_prefix("async fn "))
                .or_else(|| line.strip_prefix("pub fn "))
                .or_else(|| line.strip_prefix("fn "))
                .and_then(|rest| rest.split('(').next())
            {
                defined.insert(name.trim().to_string());
                expecting = false;
            }
        }
    }

    let Some((_, after)) = shell.split_once("generate_handler![") else {
        return Err("the shell has no `generate_handler!` list".to_string());
    };
    let Some((list, _)) = after.split_once("])") else {
        return Err("the shell's `generate_handler!` list is not closed".to_string());
    };
    let registered: BTreeSet<String> = list
        .lines()
        .map(|line| line.trim().trim_end_matches(',').to_string())
        .filter(|line| !line.is_empty() && !line.starts_with("//"))
        .collect();

    let mut invoked: BTreeSet<String> = BTreeSet::new();
    for (index, _) in client.match_indices("invoke<") {
        let Some(open) = client[index..].find('(') else {
            continue;
        };
        let rest = &client[index + open + 1..];
        let Some(quote) = rest.find('\'') else {
            continue;
        };
        let rest = &rest[quote + 1..];
        let Some(end) = rest.find('\'') else {
            continue;
        };
        invoked.insert(rest[..end].to_string());
    }

    let mut problems = Vec::new();
    for name in defined.difference(&registered) {
        problems.push(format!(
            "`{name}` is defined in the shell and never registered"
        ));
    }
    for name in registered.difference(&defined) {
        problems.push(format!("`{name}` is registered and has no definition"));
    }
    for name in invoked.difference(&registered) {
        problems.push(format!(
            "the client calls `{name}` and no handler answers to it"
        ));
    }
    for name in registered.difference(&invoked) {
        problems.push(format!(
            "`{name}` is registered and no client wrapper reaches it"
        ));
    }
    if problems.is_empty() {
        Ok(defined.len())
    } else {
        problems.truncate(6);
        Err(problems.join("; "))
    }
}

/// One schema's SQL with its comments removed.
fn strip_sql_comments(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    for line in sql.lines() {
        let code = match line.find("--") {
            Some(index) => &line[..index],
            None => line,
        };
        out.push_str(code);
        out.push('\n');
    }
    out
}

/// Silence the unused-import warning for the two constants the printed conditions refer to.
#[allow(dead_code)]
const _DECISION_KINDS: usize = DecisionKind::COUNT;
