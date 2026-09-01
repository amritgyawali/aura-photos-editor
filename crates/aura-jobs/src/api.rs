//! The orchestrator: the loop that turns twenty-seven phases into one button.
//!
//! One type, [`Autopilot`], and one method that matters, [`Autopilot::execute`]. Everything it
//! does is a decision some other module in this crate already made - [`crate::dag`] decides the
//! order, [`crate::resume`] decides where a stage starts, [`crate::governor`] decides how fast,
//! [`crate::retry`] decides what a failure means, [`crate::summary`] decides what word the run
//! gets - and this is where they meet a catalog and a clock.
//!
//! ## The five things the loop does per stage, in this order
//!
//! 1. **Availability**, from the runner. A stage whose phase is not built never reaches a counter.
//! 2. **The gate**, from [`AutonomyGate`]. A stage the bands do not permit is held here, before
//!    anything is counted or hashed, so a held stage costs nothing.
//! 3. **The plan**, from the checkpoint. Already done, continue, or start again.
//! 4. **The governor**, from the probe. Proceed, reduce, pause or stop.
//! 5. **The work**, through the runner, with the cancel token and the progress watch.
//!
//! The order is the cheapness order and it is deliberate: the two questions that can skip a stage
//! entirely are asked before the two that cost a catalog read.
//!
//! ## What this loop cannot do
//!
//! It cannot decide anything about a photograph. It has no access to a recipe, a renderer, a
//! model or a pixel; the only thing it can do to a wedding is ask a `StageRunner` to run a stage
//! that some other phase owns. `tests/no_decisions.rs` is the grep that keeps that true.
//!
//! It also cannot grant itself autonomy. [`StageVerdict::from_band`] is the only constructor of a
//! verdict that runs, and its input comes from the gate.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::contract::ids::{ProjectId, RunId};
use aura_core::contract::qc::QcReport;
use aura_core::progress::CancelToken;
use aura_core::AuraResult;

use crate::checkpoint::{self, ORCHESTRATOR_VER};
use crate::contract::autopilot::{
    AutonomyGate, AutopilotCode, AutopilotOutline, AutopilotOverride, AutopilotReason,
    AutopilotService, GovernorAction, MachineProbe, PreflightReport, ResourceEvent, RunHandle,
    RunProgress, RunStatus, RunSummary, RunWatch, SkipCause, StageId, StageReport, StageRequest,
    StageRunner, StageVerdict,
};
use crate::dag::Dag;
use crate::governor::{Governor, RunMode};
use crate::policy::Policy;
use crate::preflight::{self, Facts};
use crate::progress::{Measured, Remaining};
use crate::retry::{self, Disposition};
use crate::stages;
use crate::store::AutopilotStore;
use crate::summary::{self, Finished};
use crate::{cancel, errors, resume};

/// Everything the orchestrator needs that it cannot work out for itself.
///
/// Four ports and a policy. The ports are what keep this crate free of the twenty-two deciding
/// crates: `runner` executes a stage, `gate` says what the bands allow, `probe` reads the machine,
/// and `qc` hands back phase 27's report for the summary. `aura-app` supplies all four.
pub struct Ports {
    /// How a stage is actually executed.
    pub runner: Arc<dyn StageRunner>,
    /// Where a band comes from.
    pub gate: Arc<dyn AutonomyGate>,
    /// Where the machine's readings come from.
    pub probe: Arc<dyn MachineProbe>,
}

impl std::fmt::Debug for Ports {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ports")
            .field("runner", &self.runner)
            .field("gate", &self.gate)
            .field("probe", &self.probe)
            .finish()
    }
}

/// What the caller knows that the orchestrator does not.
///
/// Everything here is a *reading* rather than a decision: how many frames were selected, what the
/// QC report said, what the cloud spend was. The orchestrator asks for them at the end so the
/// summary can carry them; it never computes one.
#[derive(Debug, Clone, Default)]
pub struct Tally {
    /// How many photographs phase 12 selected.
    pub selected: u32,
    /// How many files were written.
    pub exported: u32,
    /// How many frames a person is being asked to look at.
    pub needs_review: u32,
    /// Phase 27's report, when the QC stage ran.
    pub qc: Option<QcReport>,
    /// What the run spent on cloud calls.
    pub spend_usd: f32,
    /// Where the delivered files are.
    pub output_path: PathBuf,
}

/// The orchestrator.
#[derive(Debug)]
pub struct Autopilot {
    store: AutopilotStore,
    policy: Policy,
    dag: Dag,
    ports: Ports,
}

impl Autopilot {
    /// Build an orchestrator over an open catalog.
    ///
    /// # Errors
    ///
    /// `AURA-JOB-7008` when the shipped policy file will not load, which is a build defect.
    pub fn new(catalog: Arc<Catalog>, clock: Arc<dyn Clock>, ports: Ports) -> AuraResult<Self> {
        let dag = Dag::build().map_err(|err| errors::policy_refused(err.to_string()))?;
        Ok(Self {
            store: AutopilotStore::new(catalog, clock),
            policy: Policy::embedded()?,
            dag,
            ports,
        })
    }

    /// Use a policy other than the embedded one.
    #[must_use]
    pub fn with_policy(mut self, policy: Policy) -> Self {
        self.policy = policy;
        self
    }

    /// The store, for the panel's read-only queries.
    #[must_use]
    pub const fn store(&self) -> &AutopilotStore {
        &self.store
    }

    /// The stages this run will attempt, in execution order.
    ///
    /// A stage the photographer disabled is still in the list - it is visited, recorded as
    /// [`SkipCause::TurnedOff`] and unblocks its dependents. Removing it would make its dependents
    /// look like stages with fewer dependencies than they have.
    #[must_use]
    pub fn plan(&self) -> Vec<StageId> {
        self.dag.order().to_vec()
    }

    /// Whether a stage runs at all under these settings.
    #[must_use]
    pub fn enabled(&self, stage: StageId, settings: &AutopilotOverride) -> bool {
        if !stages::decl(stage).optional {
            return true;
        }
        if settings.disabled.contains(&stage) {
            return false;
        }
        self.policy.default_on(stage)
    }

    /// What the gate says about one stage.
    ///
    /// A stage with no decision kind runs unconditionally: phase 13's rule that analysis is not a
    /// decision, so there is no band to consult and no way for a measurement to be held.
    ///
    /// # Errors
    ///
    /// Whatever the gate raises when its table cannot be read.
    pub fn verdict(
        &self,
        project: ProjectId,
        stage: StageId,
        zero_touch: bool,
    ) -> AuraResult<StageVerdict> {
        let Some(kind) = stage.decision_kind() else {
            return Ok(StageVerdict::Act);
        };
        let band = self.ports.gate.band(project, kind)?;
        Ok(StageVerdict::from_band(band, zero_touch))
    }

    /// What would happen if the run started now.
    ///
    /// # Errors
    ///
    /// Whatever the runner or the gate raises while counting.
    pub fn preflight_with(
        &self,
        project: ProjectId,
        settings: &AutopilotOverride,
        mut facts: Facts,
    ) -> AuraResult<PreflightReport> {
        facts.calibrated = self.ports.gate.calibrated(project);
        facts.allow_on_battery = settings.allow_on_battery;
        facts.disk_headroom = self.policy.budgets().disk_headroom;

        let mut planned = Vec::new();
        let mut held = 0u32;
        for stage in self.dag.order() {
            if !self.enabled(*stage, settings) {
                continue;
            }
            if self.ports.runner.availability(project, *stage).is_some() {
                continue;
            }
            if !self.verdict(project, *stage, settings.zero_touch)?.runs() {
                held += 1;
                continue;
            }
            planned.push((*stage, self.ports.runner.unit_count(project, *stage)?));
        }
        facts.planned = planned;
        facts.held_stages = held;
        Ok(preflight::check(&facts))
    }

    /// Run the wedding.
    ///
    /// Synchronous. The caller decides which thread this happens on - `aura-app` puts it on a
    /// worker and hands the shell a [`RunHandle`] - because a scheduler that owned an async
    /// runtime would be a scheduler that could not be driven by a plain test, and section 10.1's
    /// chaos suite is entirely plain tests.
    ///
    /// # Errors
    ///
    /// `AURA-JOB-7009` when a run is already in flight, `AURA-JOB-7005` when a mandatory stage
    /// fails, and whatever the catalog raises.
    #[allow(clippy::too_many_lines)]
    pub fn execute(
        &self,
        project: ProjectId,
        settings: &AutopilotOverride,
        handle: &RunHandle,
        tally: &Tally,
    ) -> AuraResult<RunSummary> {
        let calibrated = self.ports.gate.calibrated(project);
        let enabled: Vec<StageId> = self
            .dag
            .order()
            .iter()
            .copied()
            .filter(|stage| self.enabled(*stage, settings))
            .collect();

        #[allow(clippy::cast_possible_truncation)]
        let stage_total = enabled.len() as u32;
        self.store.open_run(
            handle.run_id,
            project,
            &crate::store::NewRun {
                zero_touch: settings.zero_touch,
                calibrated,
                stages_enabled: stage_total,
                policy_ver: self.policy.version(),
                orchestrator_ver: ORCHESTRATOR_VER,
            },
        )?;

        let run_key = handle.run_id.to_db();
        let governor = Governor::new(
            self.policy.budgets(),
            RunMode {
                allow_on_battery: settings.allow_on_battery,
                quiet_mode: settings.quiet_mode,
            },
        );

        let mut finished: Vec<Finished> = Vec::with_capacity(self.dag.order().len());
        let mut stopped_by_resources = false;
        let mut failed_run = false;

        for (index, stage) in self.dag.order().iter().copied().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            let stage_index = index as u32;

            let outcome = self.run_one_stage(
                project,
                stage,
                stage_index,
                stage_total,
                settings,
                handle,
                &run_key,
                &governor,
                stopped_by_resources,
                &mut finished,
            )?;

            if matches!(
                &outcome,
                crate::contract::autopilot::StageOutcome::Skipped(SkipCause::ResourceStopped)
            ) {
                stopped_by_resources = true;
            }
            if retry::ends_run(&outcome, stages::decl(stage).optional) {
                failed_run = true;
                self.store.add_reason(
                    &run_key,
                    &AutopilotReason {
                        code: AutopilotCode::RunFailed,
                        stage: Some(stage),
                        detail: outcome.as_str().to_string(),
                    },
                )?;
                break;
            }
        }

        let cancelled = handle.cancel.is_cancelled();
        let summary = summary::build(
            handle.run_id,
            &finished,
            cancelled,
            failed_run,
            tally.selected,
            tally.exported,
            tally.needs_review,
            tally.qc.clone(),
            tally.spend_usd,
            tally.output_path.clone(),
        );

        for reason in summary::reasons(&finished, summary.status, calibrated) {
            self.store.add_reason(&run_key, &reason)?;
        }
        self.store.close_run(
            &run_key,
            summary.status,
            summary.selected,
            summary.exported,
            summary.needs_review,
            summary.spend_usd,
            &summary.output_path.to_string_lossy(),
        )?;

        Ok(summary)
    }

    /// One stage, from availability to outcome.
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn run_one_stage(
        &self,
        project: ProjectId,
        stage: StageId,
        stage_index: u32,
        stage_total: u32,
        settings: &AutopilotOverride,
        handle: &RunHandle,
        run_key: &str,
        governor: &Governor,
        already_stopped: bool,
        finished: &mut Vec<Finished>,
    ) -> AuraResult<crate::contract::autopilot::StageOutcome> {
        use crate::contract::autopilot::StageOutcome;

        let decl = stages::decl(stage);
        // DETERMINISM: a wall clock, and it feeds nothing that decides anything. `elapsed_ms` is
        // reported to a photographer, stored on the stage row and used by `perf` to check a
        // budget; no branch anywhere in this crate reads it, and the run's outcome is identical
        // whether a stage took a second or an hour. Invariant 4 is about inputs to decisions, and
        // the one thing in this phase that *is* time-shaped - the ETA - is derived from this and is
        // shown rather than acted on.
        //
        // `Instant` rather than the injected `Clock` deliberately: this is a duration on one
        // machine, and a clock a test can move would make a stage that appears to have taken
        // negative time. The injected clock is what stamps the rows, which is where an ordering
        // that has to be reproducible lives.
        let started = Instant::now(); // DETERMINISM: measured, reported, never decided on.

        // A run that has already been stopped or cancelled records the rest of the plan rather
        // than silently truncating it. A stage list that ends early is a photographer wondering
        // what happened to the other nine steps.
        let short_circuit = if already_stopped {
            Some(SkipCause::ResourceStopped)
        } else if handle.cancel.is_cancelled() {
            Some(SkipCause::Cancelled)
        } else if !self.enabled(stage, settings) {
            Some(SkipCause::TurnedOff)
        } else {
            self.ports.runner.availability(project, stage)
        };

        if let Some(cause) = short_circuit {
            let outcome = StageOutcome::Skipped(cause);
            self.record(
                run_key,
                stage,
                stage_index,
                0,
                "",
                StageVerdict::Hold,
                &outcome,
                0,
                0,
            )?;
            finished.push(Finished {
                stage,
                outcome: outcome.clone(),
                elapsed_ms: 0,
            });
            return Ok(outcome);
        }

        // The gate, before anything is counted. A held stage costs one band lookup.
        let verdict = self.verdict(project, stage, settings.zero_touch)?;
        if !verdict.runs() {
            let outcome = StageOutcome::Skipped(SkipCause::AwaitingReview);
            self.record(run_key, stage, stage_index, 0, "", verdict, &outcome, 0, 0)?;
            finished.push(Finished {
                stage,
                outcome: outcome.clone(),
                elapsed_ms: 0,
            });
            return Ok(outcome);
        }

        let units = self.ports.runner.unit_count(project, stage)?;
        if units == 0 {
            let outcome = StageOutcome::Skipped(SkipCause::NoInput);
            self.record(run_key, stage, stage_index, 0, "", verdict, &outcome, 0, 0)?;
            finished.push(Finished {
                stage,
                outcome: outcome.clone(),
                elapsed_ms: 0,
            });
            return Ok(outcome);
        }

        let hash = self.ports.runner.inputs_hash(project, stage)?;

        // The stored checkpoint is read **before** the row is re-planned, and the order is the
        // whole of section 6.1's invalidation rule.
        //
        // `plan_stage` writes the current `inputs_hash` into the row. Doing that first and then
        // comparing the row against the same hash compares a value with itself: every checkpoint
        // is valid, always, and a re-tuned scene profile silently resumes onto stale work with
        // every unit test passing. That is what the first version of this function did.
        //
        // Phase 19 wrote the general shape - a converged value cannot be used to detect its own
        // constraints - and phase 25 met it again in a change-point test whose divisor had a trend
        // in it. This is the same defect in a scheduler: **read the evidence before writing the
        // thing you are about to compare it against.**
        let stored = self.store.checkpoint(handle.run_id, stage)?;
        let plan = resume::plan(stored.as_ref(), &hash, units);
        let opening = match plan {
            resume::Plan::Continue { from } => from,
            _ => 0,
        };
        self.store
            .plan_stage(run_key, stage, stage_index, units, &hash, verdict, opening)?;
        if let resume::Plan::Restart { invalidation } = plan {
            if invalidation.restarts() {
                self.store.add_reason(
                    run_key,
                    &AutopilotReason {
                        code: AutopilotCode::StageReplanned,
                        stage: Some(stage),
                        detail: invalidation.as_str().to_string(),
                    },
                )?;
            }
        }
        if !plan.runs() {
            let outcome = StageOutcome::Completed { items: units };
            self.record(
                run_key,
                stage,
                stage_index,
                units,
                &hash,
                verdict,
                &outcome,
                0,
                0,
            )?;
            finished.push(Finished {
                stage,
                outcome: outcome.clone(),
                elapsed_ms: 0,
            });
            return Ok(outcome);
        }
        let resume_from = match plan {
            resume::Plan::Continue { from } => {
                self.store.add_reason(
                    run_key,
                    &AutopilotReason {
                        code: AutopilotCode::StageResumed,
                        stage: Some(stage),
                        detail: from.to_string(),
                    },
                )?;
                from
            }
            _ => 0,
        };

        // The governor. Its readings are recorded whatever it decides to do about them.
        let ruling = governor.rule(&self.ports.probe.sample(), stage);
        for event in &ruling.events {
            self.store.add_event(run_key, event)?;
            self.note_resource(run_key, stage, event)?;
        }
        if ruling.action == GovernorAction::Stop {
            let outcome = StageOutcome::Skipped(SkipCause::ResourceStopped);
            self.record(
                run_key,
                stage,
                stage_index,
                0,
                &hash,
                verdict,
                &outcome,
                0,
                0,
            )?;
            finished.push(Finished {
                stage,
                outcome: outcome.clone(),
                elapsed_ms: 0,
            });
            return Ok(outcome);
        }

        handle.progress.publish(RunProgress {
            stage,
            stage_index,
            stage_total,
            items_done: resume_from,
            items_total: units,
            eta_s: self.eta_seconds(stage, resume_from, units, project, settings)?,
            throughput_per_s: 0.0,
            spend_usd: handle.progress.borrow().spend_usd,
            warnings: Vec::new(),
            current_image: None,
        });

        let request = StageRequest {
            project,
            run_id: handle.run_id,
            stage,
            resume_from,
            zero_touch: settings.zero_touch,
            verdict,
            concurrency: ruling.parallel_stages.max(1),
        };

        let mut attempts: u8 = 0;
        let outcome = loop {
            attempts = attempts.saturating_add(1);
            let attempt = self
                .ports
                .runner
                .run(&request, &handle.progress, &handle.cancel);
            let outcome = match attempt {
                Ok(outcome) => outcome,
                Err(err) => StageOutcome::Failed {
                    code: err.code.0.to_string(),
                    detail: err.detail.clone(),
                },
            };
            if !matches!(outcome, StageOutcome::Failed { .. }) {
                break outcome;
            }
            match retry::disposition(attempts, decl.optional) {
                Disposition::Retry { attempt, .. } => {
                    self.store.add_reason(
                        run_key,
                        &AutopilotReason {
                            code: AutopilotCode::StageRetried,
                            stage: Some(stage),
                            detail: attempt.to_string(),
                        },
                    )?;
                    if handle.cancel.is_cancelled() {
                        break cancel::unstarted_outcome();
                    }
                }
                Disposition::Isolate => {
                    self.store.add_reason(
                        run_key,
                        &AutopilotReason {
                            code: AutopilotCode::StageIsolated,
                            stage: Some(stage),
                            detail: outcome.as_str().to_string(),
                        },
                    )?;
                    break outcome;
                }
                Disposition::FailRun => break outcome,
            }
        };

        // A cancel observed during the stage turns a completed-looking outcome into an honest one:
        // the units that finished are committed and the rest were never started.
        let outcome =
            if handle.cancel.is_cancelled() && !matches!(outcome, StageOutcome::Failed { .. }) {
                let done = outcome.items().max(handle.progress.borrow().items_done);
                if done >= units {
                    outcome
                } else {
                    cancel::interrupted_outcome(stage, done, units)
                }
            } else {
                outcome
            };

        #[allow(clippy::cast_possible_truncation)]
        let elapsed_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        let error = match &outcome {
            StageOutcome::Failed { code, detail } => Some((code.as_str(), detail.as_str())),
            _ => None,
        };
        self.store.finish_stage(
            run_key,
            stage,
            outcome.as_str(),
            match &outcome {
                StageOutcome::Skipped(cause) => Some(*cause),
                _ => None,
            },
            outcome.items(),
            attempts,
            elapsed_ms,
            error,
        )?;
        finished.push(Finished {
            stage,
            outcome: outcome.clone(),
            elapsed_ms,
        });
        Ok(outcome)
    }

    /// Write a stage row and its outcome in one go, for the paths that never started work.
    #[allow(clippy::too_many_arguments)]
    fn record(
        &self,
        run_key: &str,
        stage: StageId,
        stage_index: u32,
        units: u32,
        hash: &str,
        verdict: StageVerdict,
        outcome: &crate::contract::autopilot::StageOutcome,
        attempts: u8,
        elapsed_ms: u64,
    ) -> AuraResult<()> {
        use crate::contract::autopilot::StageOutcome;
        self.store
            .plan_stage(run_key, stage, stage_index, units, hash, verdict, 0)?;
        self.store.finish_stage(
            run_key,
            stage,
            outcome.as_str(),
            match outcome {
                StageOutcome::Skipped(cause) => Some(*cause),
                _ => None,
            },
            outcome.items(),
            attempts,
            elapsed_ms,
            None,
        )
    }

    /// Record what a governor event meant, in the run's own reason list.
    fn note_resource(
        &self,
        run_key: &str,
        stage: StageId,
        event: &ResourceEvent,
    ) -> AuraResult<()> {
        let code = match event.action {
            GovernorAction::Proceed => return Ok(()),
            GovernorAction::Reduce => AutopilotCode::ResourceReduced,
            GovernorAction::Pause => AutopilotCode::ResourcePaused,
            GovernorAction::Stop => AutopilotCode::ResourceStopped,
        };
        self.store.add_reason(
            run_key,
            &AutopilotReason {
                code,
                stage: Some(stage),
                detail: event.kind.as_str().to_string(),
            },
        )
    }

    /// Seconds left, from this stage's position in the plan.
    fn eta_seconds(
        &self,
        stage: StageId,
        done: u32,
        total: u32,
        project: ProjectId,
        settings: &AutopilotOverride,
    ) -> AuraResult<u32> {
        let mut pending = Vec::new();
        let mut seen = false;
        for candidate in self.dag.order() {
            if *candidate == stage {
                seen = true;
                continue;
            }
            if !seen || !self.enabled(*candidate, settings) {
                continue;
            }
            if self
                .ports
                .runner
                .availability(project, *candidate)
                .is_some()
            {
                continue;
            }
            pending.push(Remaining {
                stage: *candidate,
                units: self.ports.runner.unit_count(project, *candidate)?,
            });
        }
        Ok(crate::progress::estimate(
            Measured {
                stage,
                done,
                total,
                elapsed: std::time::Duration::ZERO,
            },
            &pending,
        )
        .remaining_s())
    }
}

impl AutopilotService for Autopilot {
    fn preflight(&self, project: ProjectId) -> AuraResult<PreflightReport> {
        let settings = self.store.settings(project)?;
        self.preflight_with(project, &settings, Facts::default())
    }

    fn start(&self, project: ProjectId, settings: &AutopilotOverride) -> AuraResult<RunHandle> {
        // `start` returns the handle; the caller drives `execute` on a thread it owns. That split
        // is what lets the shell hand a photographer a progress stream immediately rather than
        // after the ingest finishes.
        self.store.set_settings(settings)?;
        if let Some(existing) = self.store.in_flight(project)? {
            return Err(errors::run_in_flight(existing));
        }

        // Pressing the button on a wedding that was stopped continues that run rather than
        // starting a new one, because a checkpoint is keyed `(run_id, stage)` and a new id would
        // find none of them. `RunStatus::is_resumable` is the rule; this is the one place in the
        // product that applies it.
        let run_id = match self.store.resumable(project)? {
            Some(existing) => RunId::from_db(&existing).unwrap_or_default(),
            None => RunId::new(),
        };

        let first = self.dag.order().first().copied().unwrap_or(StageId::Ingest);
        #[allow(clippy::cast_possible_truncation)]
        let total = self.dag.order().len() as u32;
        Ok(RunHandle {
            run_id,
            progress: RunWatch::new(RunProgress::starting(first, total)),
            cancel: CancelToken::new(),
        })
    }

    fn cancel(&self, project: ProjectId) -> AuraResult<bool> {
        Ok(self.store.in_flight(project)?.is_some())
    }

    fn outline(&self, project: ProjectId) -> AuraResult<AutopilotOutline> {
        self.store.outline(project)
    }

    fn summary(&self, project: ProjectId) -> AuraResult<Option<RunSummary>> {
        let Some(run_id) = self.store.latest_run(project)? else {
            return Ok(None);
        };
        let stages = self.store.stages(&run_id)?;
        let outline = self.store.outline(project)?;
        let finished: Vec<Finished> = stages
            .iter()
            .filter_map(|report| {
                let outcome = match report.outcome.as_str() {
                    "completed" => crate::contract::autopilot::StageOutcome::Completed {
                        items: report.items_done,
                    },
                    "skipped" => crate::contract::autopilot::StageOutcome::Skipped(
                        report.skip_cause.unwrap_or(SkipCause::ServiceAbsent),
                    ),
                    "partial" => crate::contract::autopilot::StageOutcome::Partial {
                        items: report.items_done,
                        failed: report.items_total.saturating_sub(report.items_done),
                        detail: String::new(),
                    },
                    "failed" => crate::contract::autopilot::StageOutcome::Failed {
                        code: errors::AUTOPILOT_STAGE_FAILED.0.to_string(),
                        detail: String::new(),
                    },
                    _ => return None,
                };
                Some(Finished {
                    stage: report.stage,
                    outcome,
                    elapsed_ms: report.elapsed_ms,
                })
            })
            .collect();

        let status = outline.status.unwrap_or(RunStatus::Running);
        let run = RunId::from_db(&run_id).unwrap_or_default();
        Ok(Some(RunSummary {
            run_id: run,
            status,
            selected: 0,
            exported: 0,
            needs_review: 0,
            qc: None,
            stage_timings: finished
                .iter()
                .map(|row| (row.stage, row.elapsed_ms))
                .collect(),
            spend_usd: 0.0,
            output_path: PathBuf::new(),
            degraded_stages: summary::degraded(&finished),
        }))
    }

    fn stages(&self, project: ProjectId) -> AuraResult<Vec<StageReport>> {
        let Some(run_id) = self.store.latest_run(project)? else {
            return Ok(Vec::new());
        };
        self.store.stages(&run_id)
    }

    fn resource_events(&self, project: ProjectId) -> AuraResult<Vec<ResourceEvent>> {
        let Some(run_id) = self.store.latest_run(project)? else {
            return Ok(Vec::new());
        };
        self.store.events(&run_id)
    }

    fn set_settings(&self, settings: &AutopilotOverride) -> AuraResult<()> {
        self.store.set_settings(settings)
    }
}

/// A checkpoint hash built from a stage's declared inputs.
///
/// Exposed so `aura-app`'s runner and the phase gate build the same digest from the same parts.
/// The parts are the stage's own version columns and the units it faces; what is *not* in here is
/// as important as what is - no run id, no clock, no machine, no governor action. See
/// [`crate::checkpoint`].
#[must_use]
pub fn stage_inputs(stage: StageId, versions: &[(&str, &str)], units: u32) -> String {
    let units = units.to_string();
    let mut parts: Vec<(&str, &str)> = Vec::with_capacity(versions.len() + 3);
    let orchestrator = ORCHESTRATOR_VER.to_string();
    parts.push(("stage", stage.as_str()));
    parts.push(("orchestrator_ver", orchestrator.as_str()));
    parts.push(("units", units.as_str()));
    parts.extend_from_slice(versions);
    checkpoint::inputs_hash(&parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_stage_with_the_same_versions_hashes_the_same_way() {
        let a = stage_inputs(StageId::Colour, &[("colour_ver", "2")], 400);
        let b = stage_inputs(StageId::Colour, &[("colour_ver", "2")], 400);
        assert_eq!(a, b);
    }

    #[test]
    fn a_moved_version_moves_the_hash() {
        let a = stage_inputs(StageId::Colour, &[("colour_ver", "2")], 400);
        let b = stage_inputs(StageId::Colour, &[("colour_ver", "3")], 400);
        assert_ne!(a, b);
    }

    #[test]
    fn two_stages_with_identical_versions_hash_differently() {
        let a = stage_inputs(StageId::Tone, &[("v", "1")], 10);
        let b = stage_inputs(StageId::Colour, &[("v", "1")], 10);
        assert_ne!(a, b);
    }

    #[test]
    fn importing_more_photographs_moves_the_hash() {
        let a = stage_inputs(StageId::Embed, &[("embed_ver", "1")], 400);
        let b = stage_inputs(StageId::Embed, &[("embed_ver", "1")], 900);
        assert_ne!(a, b);
    }
}
