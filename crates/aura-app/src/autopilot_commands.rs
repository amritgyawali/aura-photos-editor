//! The autopilot command surface, and the three ports the orchestrator runs through. PHASE-28.
//!
//! Nine commands. Four read - the project header, the stage list, the run summary and what the
//! machine had to say - one is the pre-flight, two start and stop a run, one records the checklist,
//! and one reports what a run in flight is doing right now. ADR-0058 records the shape and what is
//! deliberately absent from it.
//!
//! # What this surface does that no earlier command surface does
//!
//! **Its primary object is a run**, and a run is the only thing in this product a photographer
//! starts and then walks away from. Every earlier panel answers a question about a photograph that
//! is already on the screen. This one has to answer, to somebody who has come back two hours
//! later: what did you do, what did you not do, and why.
//!
//! That is why [`crate::contract::ipc::AutopilotStageDto`] carries `skipCause` and `verdict` beside
//! the outcome, why the summary carries `degradedStages` rather than a count, and why
//! `AutopilotStatusDto.calibrated` is on the wire at all.
//!
//! # The three ports, and why they live here
//!
//! `aura-jobs` depends on `aura-core` and `aura-catalog` and on none of the twenty-two deciding
//! crates. It executes a stage through [`StageRunner`], reads a band through [`AutonomyGate`] and
//! reads the machine through [`MachineProbe`]. All three are implemented in this file, over
//! `AppState`, which already owns every pass.
//!
//! That indirection is what stops the scheduler becoming a crate every phase can reach back into -
//! and what stops it acquiring the one thing an orchestrator must never have, which is an opinion
//! about a photograph. Phase 27 used the same shape for its readings.
//!
//! # What is not here
//!
//! No autonomy field, no threshold, no stage strength, no way to reorder the pipeline, and no
//! command that runs a single stage on its own. The last of those is deliberate: a surface that
//! could run the retouch without the cull would be a surface that could edit four thousand frames
//! nobody is delivering, and every individual pass already has its own command from its own phase.

use std::path::PathBuf;
use std::sync::Arc;

use aura_core::contract::ledger::{Autonomy, DecisionKind};
use aura_core::progress::CancelToken;
use aura_core::{AuraResult, ProjectId};
use aura_explain::policy::Risk;
use aura_export::api::ExportPass;
use aura_export::store::ExportStore;
use aura_jobs::api::{Autopilot, Ports, Tally};
use aura_jobs::contract::autopilot::{
    AutonomyGate, AutopilotOverride, AutopilotService, MachineProbe, MachineState, RunHandle,
    RunWatch, SkipCause, StageId, StageOutcome, StageRequest, StageRunner,
};
use aura_jobs::preflight::Facts;

use crate::commands::IpcResult;
use crate::contract::ipc::{
    AutopilotEventDto, AutopilotPreflightDto, AutopilotPreflightRowDto, AutopilotProgressDto,
    AutopilotSettingsInput, AutopilotStageDto, AutopilotStartInput, AutopilotStatusDto,
    AutopilotSummaryDto, IpcError,
};
use crate::delivery_commands::{ExportField, ExportSource};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// The autonomy gate
// ---------------------------------------------------------------------------

/// Phase 13's bands, answered for a whole stage.
///
/// ## What question this answers, and what it deliberately does not
///
/// It answers **"could a decision of this kind ever act unattended on this build"**, which is a
/// ceiling, rather than "what band will this particular decision get". The confidence handed to
/// `preview_band` is 1.0, the most permissive input there is, so a band that still comes back
/// needing review is a band no decision of that kind can beat.
///
/// That is the right question for a *stage* gate and it is important that it is not the other one.
/// Each phase still bands each of its own decisions through phase 13 as it makes them, with that
/// decision's real confidence; this gate decides only whether the stage may run at all. A gate that
/// used an average confidence would be inventing a number, and one that used a pessimistic
/// confidence would hold stages whose easy frames were perfectly safe.
///
/// ## What that comes out as on this build
///
/// `CalibrationSet::is_fitted` is false, so `Risk::uncalibrated` is set on every kind and every
/// band is raised one step. A reversible kind - cull, edit - lands on `AutoZeroTouch`: it runs when
/// Zero-Touch is on and waits otherwise. An irreversible one - retouch, curation, export - is
/// raised twice and lands on `Suggest`: in Zero-Touch it runs **and every decision it makes goes in
/// the review queue**, which is exactly what phase 13's own words for `Suggest` describe.
///
/// So this build's honest Zero-Touch is: it does the work, and it asks about everything that
/// cannot be taken back. The pre-flight says so before the run starts rather than after it.
#[derive(Debug)]
pub struct AppGate {
    policy: Arc<aura_explain::policy::AutonomyPolicy>,
    calibrated: bool,
}

impl AppGate {
    /// Build the gate from this project's explain service.
    ///
    /// # Errors
    ///
    /// Whatever `AppState::explain` raises when its tables cannot be loaded.
    pub fn new(state: &AppState) -> AuraResult<Self> {
        let explain = state.explain()?;
        Ok(Self {
            policy: Arc::new(explain.policy().clone()),
            calibrated: explain.calibration().is_fitted(),
        })
    }
}

impl AutonomyGate for AppGate {
    fn band(&self, _project: ProjectId, kind: DecisionKind) -> AuraResult<Autonomy> {
        let risk = Risk::NONE
            .for_kind(kind)
            .with_uncalibrated(!self.calibrated);
        Ok(aura_explain::api::preview_band(
            &self.policy,
            kind,
            1.0,
            risk,
        ))
    }

    fn calibrated(&self, _project: ProjectId) -> bool {
        self.calibrated
    }
}

// ---------------------------------------------------------------------------
// The machine probe
// ---------------------------------------------------------------------------

/// What this build can actually read about the machine.
///
/// **Two of seven readings, and the other five are honestly absent.** Free disk comes from the
/// filesystem and the video-memory ceiling comes from phase 03's hardware plan. Temperature,
/// battery charge, mains state, host memory pressure and device-lost detection need platform APIs
/// this product does not link, so they are `None`.
///
/// `None` is the correct value and not a placeholder. Phase 24's rule - an absent input is
/// ignorance rather than permission - and the governor is built so that ignorance costs nothing:
/// every unreadable sensor contributes `GovernorAction::Proceed`, and there is no reading that
/// could make the product go *faster*. A machine whose thermal sensor is unreadable therefore runs
/// exactly as it would have run with no governor at all, which is what this build does.
///
/// That is condition C3 of the phase 28 exit report: the thermal, battery and quiet-mode policies
/// are implemented, unit-tested against authored readings, and **never fire on this build**,
/// because nothing fills their inputs.
#[derive(Debug, Clone)]
pub struct AppProbe {
    root: PathBuf,
    needed_bytes: Option<u64>,
}

impl AppProbe {
    /// A probe over the volume this project's catalog lives on.
    #[must_use]
    pub fn new(root: PathBuf, needed_bytes: Option<u64>) -> Self {
        Self { root, needed_bytes }
    }
}

impl MachineProbe for AppProbe {
    fn sample(&self) -> MachineState {
        MachineState {
            vram_used: None,
            ram_used: None,
            temperature_c: None,
            battery: None,
            on_battery: false,
            disk_free_bytes: free_space(&self.root),
            disk_needed_bytes: self.needed_bytes,
            foreground_busy: false,
            device_lost: false,
        }
    }
}

/// Free bytes on the volume a path lives on, or `None` when it cannot be read.
///
/// `None` rather than zero. Zero free bytes is `GovernorAction::Stop` - it is the one reading that
/// can end a run - and a filesystem call that failed for an unrelated reason must not be read as a
/// full disk.
fn free_space(path: &std::path::Path) -> Option<u64> {
    let dir = if path.is_dir() { path } else { path.parent()? };
    fs4::available_space(dir).ok()
}

// ---------------------------------------------------------------------------
// The stage runner
// ---------------------------------------------------------------------------

/// One thin adapter per pipeline stage, over `AppState`.
///
/// Section 4 of the phase document: "one thin adapter per pipeline stage". `aura-jobs` owns what a
/// stage *is*; this owns how it runs, because running it means reaching twenty-two deciding crates
/// and the scheduler must not.
///
/// ## What "thin" means here
///
/// Each adapter does exactly three things: build the phase's own pass, run it with the caller's
/// cancel token, and turn its report into counts. There is no logic in any of them, and there is
/// nowhere for any to go: an adapter that started deciding something would be this file growing an
/// opinion the phase that owns it does not have.
pub struct AppRunner {
    state: Arc<AppState>,
}

impl std::fmt::Debug for AppRunner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AppRunner")
    }
}

impl AppRunner {
    /// Wrap the application state.
    #[must_use]
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    /// How many photographs the project has.
    fn photo_count(&self, project: ProjectId) -> AuraResult<u32> {
        let key = project.to_db();
        self.state
            .catalog()
            .read(move |conn: &rusqlite::Connection| {
                conn.query_row(
                    "SELECT COUNT(*) FROM photo WHERE project_id = ?1",
                    rusqlite::params![key],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(|err| aura_core::errors::db::statement_failed("count photos", &err))
            })
            .map(|count| u32::try_from(count).unwrap_or(u32::MAX))
    }

    /// How many photographs phase 12 selected.
    ///
    /// Zero before the cull has run, which is the honest answer and is why every stage after the
    /// cull reports `SkipCause::NoInput` on a wedding that was never culled rather than silently
    /// editing nothing.
    fn selected_count(&self, project: ProjectId) -> AuraResult<u32> {
        let key = project.to_db();
        self.state
            .catalog()
            .read(move |conn: &rusqlite::Connection| {
                conn.query_row(
                    "SELECT COUNT(*) FROM cull_keep k
                   JOIN cull_run r ON r.run_id = k.run_id
                  WHERE r.project_id = ?1",
                    rusqlite::params![key],
                    |row| row.get::<_, i64>(0),
                )
                .optional_or_zero()
            })
            .map(|count| u32::try_from(count).unwrap_or(u32::MAX))
    }
}

/// A tiny helper so a missing table reads as zero rather than as an error.
///
/// Not a general-purpose shim: it exists for `selected_count`, whose query joins two phase 12
/// tables that a project may legitimately have no rows in. A caller that wanted to tell "no cull
/// has run" from "the query failed" would use the store directly.
trait OptionalOrZero {
    fn optional_or_zero(self) -> Result<i64, aura_core::AuraError>;
}

impl OptionalOrZero for Result<i64, rusqlite::Error> {
    fn optional_or_zero(self) -> Result<i64, aura_core::AuraError> {
        match self {
            Ok(value) => Ok(value),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(0),
            Err(err) => Err(aura_core::errors::db::statement_failed(
                "count selected",
                &err,
            )),
        }
    }
}

/// Turn a command's `IpcError` back into the `AuraError` the orchestrator's port speaks.
///
/// The two shapes are not the same and the conversion is genuinely lossy: `IpcError` keeps the
/// code and the sentence a photographer reads, and drops the severity, the recovery and the
/// context. `From<AuraError> for IpcError` exists; the reverse cannot, because the information is
/// not there to reconstruct.
///
/// What is rebuilt here is the half the orchestrator uses. The **code** is the part that matters -
/// it is what lands in `autopilot_stage.error_code` and what a runbook is keyed on - and the
/// severity and recovery are set to what a failed stage means to a run rather than guessed at:
/// `ItemFailed` and `Retry`, because `retry::disposition` is what decides whether it is actually
/// retried and it reads `StageDecl::optional` rather than this.
fn lift<T>(result: IpcResult<T>) -> AuraResult<T> {
    result.map_err(|err| {
        aura_core::AuraError::new(
            aura_core::contract::error::ErrorCode(Box::leak(err.code.clone().into_boxed_str())),
            aura_core::contract::error::Severity::ItemFailed,
            aura_core::contract::error::Recovery::Retry,
            err.message.clone(),
            err.message,
        )
    })
}

/// A count from a phase whose own report counts in `i64`, brought down to the wire's `u32`.
///
/// Saturating rather than truncating. A wedding with more than four billion moments is not a
/// number this product will ever see, and a wrapping conversion that produced a small number from
/// a large one would be a progress bar that went backwards.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn clamp(value: i64) -> u32 {
    value.clamp(0, i64::from(u32::MAX)) as u32
}

impl StageRunner for AppRunner {
    fn unit_count(&self, project: ProjectId, stage: StageId) -> AuraResult<u32> {
        use aura_jobs::contract::autopilot::StageScope;
        match aura_jobs::stages::decl(stage).scope {
            StageScope::Gallery => Ok(1),
            StageScope::AllImages => self.photo_count(project),
            StageScope::SelectedImages => self.selected_count(project),
        }
    }

    fn availability(&self, _project: ProjectId, _stage: StageId) -> Option<SkipCause> {
        // **Empty, for the first time since phase 28 wrote it.** Curation left this table when
        // phase 29 landed and export left it when phase 30 did, which closes phase 28's condition
        // C7: every stage in the DAG is now built, and a completed run writes files.
        //
        // It is kept rather than deleted because it is the one place a stage can say "this
        // release does not have me" before any work starts, and the next feature that ships behind
        // its own phase will need it.
        //
        // Note what is *not* here. Export is available, and whether it can actually write is a
        // question about this wedding rather than about this release - it needs a destination
        // somebody chose - so the answer is `NoInput` from the stage itself rather than
        // `PhaseNotBuilt` from this table. The two read very differently on the run summary, and
        // only one of them is true.
        None
    }

    /// Run one stage by calling the command the phase that owns it already ships.
    ///
    /// Every arm is one call. That is what makes these adapters thin in the sense section 4 asks
    /// for: the wiring each pass needs - its preview service, its inference engine, its store, its
    /// policy table, its cancellation registration - is already correct inside that phase's own
    /// command, and a second copy of it here would be a second place for it to drift.
    ///
    /// It also means the autopilot runs a wedding through exactly the code path a photographer
    /// clicking each panel's button would run it through. A scheduler with its own route to a pass
    /// is a scheduler whose results can differ from the panel's, and nothing would record which
    /// route a gallery came from.
    fn run(
        &self,
        request: &StageRequest,
        _progress: &RunWatch,
        cancel: &CancelToken,
    ) -> AuraResult<StageOutcome> {
        // The run's own cancel token, registered under the stage's name so a pass that takes a
        // `cancel_id` stops with the run rather than only with its own panel.
        let cancel_id = format!("{}:{}", request.run_id.to_db(), request.stage.as_str());
        self.state.register_job(&cancel_id, cancel.clone());
        let outcome = self.dispatch(request, &cancel_id);
        self.state.finish_job(&cancel_id);
        outcome
    }

    fn inputs_hash(&self, project: ProjectId, stage: StageId) -> AuraResult<String> {
        // What a stage reads: its own scope's unit count and the catalog's schema version.
        //
        // The schema version stands in for the per-phase analysis versions each phase already
        // keeps - `tone::ANALYSIS_VER`, `moments::GROUP_VER`, and the twenty others - because none
        // of them is exposed through a frozen service today. That is condition C5 of the exit
        // report, and it is a real weakening rather than a detail: this hash notices a photographer
        // importing more frames and a migration landing, and does **not** notice a scene profile
        // being re-tuned. Until each phase publishes its own version, an upstream re-tune has to be
        // followed by a fresh run rather than by a resume.
        let units = self.unit_count(project, stage)?;
        let schema = self.state.catalog().schema_version()?.to_string();
        Ok(aura_jobs::api::stage_inputs(
            stage,
            &[("schema_ver", schema.as_str())],
            units,
        ))
    }
}

impl AppRunner {
    /// The twenty-five arms, each one call into the phase that owns the stage.
    #[allow(clippy::too_many_lines)]
    fn dispatch(&self, request: &StageRequest, cancel_id: &str) -> AuraResult<StageOutcome> {
        use crate::contract::ipc::{
            AnalyseCompositionInput, AnalyseIntegrityInput, CameraPassInput, ClassifyScenesInput,
            CleanupPassInput, CullProjectInput, CurateProjectInput, EmbedProjectInput,
            EstimateColourInput, EstimateToneInput, GalleryPassInput, GroupMomentsInput,
            MicroPassInput, PlanGeometryInput, QcPassInput, RestorePassInput, RetouchPassInput,
            ScanFacesInput, ScoreEmotionInput, SculptLocalInput,
        };

        let state = self.state.as_ref();
        let project = request.project;
        let project_id = project.to_db();
        let cancel = Some(cancel_id.to_string());

        // Every command returns its own report shape, so each arm ends in the count that shape
        // calls "how many did you do". A per-photograph failure never becomes a run-level
        // `Partial` here: it is that phase's own business, is already counted in that phase's own
        // report, and the orchestrator promoting it would be the scheduler re-judging another
        // phase's result.
        let items: u32 = match request.stage {
            // Ingest and previews are driven by the import wizard, which knows where the files
            // are. An autopilot run over an already-imported wedding has nothing to walk; one over
            // an empty project is stopped by the pre-flight's `HasImages` row rather than by a
            // walker this stage would have to invent a root for. Condition C2 of the exit report.
            StageId::Ingest | StageId::Previews => self.photo_count(project)?,

            StageId::Embed => {
                let report = lift(crate::index_commands::embed_project(
                    state,
                    &EmbedProjectInput { project_id },
                ))?;
                report.embedded
            }
            StageId::Faces => {
                let report = lift(crate::people_commands::scan_faces(
                    state,
                    &ScanFacesInput { project_id },
                ))?;
                report.scanned
            }
            StageId::Story => {
                let report = lift(crate::story_commands::classify_scenes(
                    state,
                    &ClassifyScenesInput {
                        project_id: project_id.clone(),
                    },
                ))?;
                // Classification and segmentation are two commands and one stage, because they are
                // one *decision*: phase 07's chapters are drawn over the posteriors, and a run that
                // classified without segmenting would leave every later stage comparing against the
                // neutral profile with no sign that anything was missing.
                drop(lift(crate::story_commands::segment_story(
                    state,
                    &project_id,
                ))?);
                clamp(report.classified)
            }
            StageId::Moments => {
                let report = lift(crate::moment_commands::group_moments(
                    state,
                    &GroupMomentsInput { project_id },
                ))?;
                clamp(report.moments)
            }
            StageId::Integrity => {
                let report = lift(crate::integrity_commands::analyse_integrity(
                    state,
                    &AnalyseIntegrityInput {
                        project_id,
                        cancel_id: cancel,
                    },
                ))?;
                report.scored
            }
            StageId::Emotion => {
                let report = lift(crate::emotion_commands::score_emotion(
                    state,
                    &ScoreEmotionInput {
                        project_id,
                        cancel_id: cancel,
                    },
                ))?;
                report.scored
            }
            StageId::Composition => {
                let report = lift(crate::composition_commands::analyse_composition(
                    state,
                    &AnalyseCompositionInput {
                        project_id,
                        cancel_id: cancel,
                    },
                ))?;
                report.scored
            }
            StageId::Cull => {
                let report = lift(crate::cull_commands::cull_project(
                    state,
                    &CullProjectInput {
                        project_id,
                        mode: None,
                        target: None,
                        cancel_id: cancel,
                    },
                ))?;
                report.selected
            }

            StageId::Tone => {
                let report = lift(crate::tone_commands::estimate_tone(
                    state,
                    &EstimateToneInput {
                        project_id,
                        cancel_id: cancel,
                    },
                ))?;
                report.estimated
            }
            StageId::Colour => {
                let report = lift(crate::colour_commands::estimate_colour(
                    state,
                    &EstimateColourInput {
                        project_id,
                        cancel_id: cancel,
                    },
                ))?;
                report.decided
            }
            // Two stages with nothing of their own to run, for two different reasons, counted the
            // same way because the count means the same thing: the survivors this stage reached.
            //
            // **Masks.** Phase 18's regions are generated lazily, per photograph, by the phases
            // that consume them. There is no project-wide mask command and there deliberately is
            // not one: a pass that segmented every selected frame up front would spend 320 ms a
            // frame on regions six later stages may never ask for. The stage exists so the
            // checklist can switch masking off as a whole, and so the summary can say it was off.
            //
            // **Style.** Phase 17's inference is three map lookups and an addition, applied
            // *inside* the tone and colour solves rather than as a walk of its own - which is
            // phase 17's own rule that a style is a residual and the baseline is never re-derived.
            StageId::Masks | StageId::Style => self.selected_count(project)?,

            StageId::LocalLight => {
                let report = lift(crate::local_commands::sculpt_local(
                    state,
                    &SculptLocalInput {
                        project_id,
                        photo_ids: Vec::new(),
                        cancel_id: cancel,
                    },
                ))?;
                report.planned
            }
            StageId::Retouch => {
                let report = lift(crate::retouch_commands::retouch_pass(
                    state,
                    &RetouchPassInput {
                        project_id,
                        photo_ids: Vec::new(),
                        preset: None,
                        cancel_id: cancel,
                    },
                ))?;
                report.planned
            }
            StageId::Micro => {
                let report = lift(crate::micro_commands::micro_pass(
                    state,
                    &MicroPassInput {
                        project_id,
                        priority: None,
                        enabled: None,
                    },
                ))?;
                report.planned
            }
            StageId::Restoration => {
                let report = lift(crate::restore_commands::restore_pass(
                    state,
                    &RestorePassInput {
                        project_id,
                        when: None,
                        priority: None,
                        output_long_edge: None,
                        enabled: None,
                    },
                ))?;
                report.planned
            }
            StageId::Geometry => {
                let report = lift(crate::geometry_commands::plan_geometry(
                    state,
                    &PlanGeometryInput {
                        project_id,
                        limit: None,
                    },
                ))?;
                report.planned
            }
            StageId::Cleanup => {
                let report = lift(crate::cleanup_commands::cleanup_pass(
                    state,
                    CleanupPassInput {
                        project_id,
                        photo_ids: Vec::new(),
                    },
                ))?;
                report.examined
            }
            StageId::CameraMatch => {
                let report = lift(crate::camera_commands::camera_pass(
                    state,
                    CameraPassInput { project_id },
                ))?;
                report.cameras
            }
            StageId::Consistency => {
                let report = lift(crate::gallery_commands::gallery_pass(
                    state,
                    GalleryPassInput { project_id },
                ))?;
                report.normalised
            }
            StageId::Qc => {
                let report = lift(crate::qc_commands::qc_run(
                    state,
                    QcPassInput {
                        project_id,
                        // The re-edit loop runs only when the run may act on an edit. A stage the
                        // gate held never reaches this line, because it was skipped before the
                        // dispatch; `ActAndReview` still remediates, and the change goes in the
                        // review queue like every other decision that band produces.
                        remediate: request.verdict.runs(),
                    },
                ))?;
                report.images
            }
            StageId::Curation => {
                // The album size is the configured default. A run cannot be given one, and that is
                // deliberate: `autopilot.toml` is a checklist of what runs rather than a place to
                // set a phase's parameters, and an album size on the run would be a second answer to
                // a question `curation.toml` already answers. A photographer who wants a different
                // album asks for one in the curation panel, which re-composes in under a second.
                let status = lift(crate::curate_commands::curate_project(
                    state,
                    CurateProjectInput {
                        project_id: project.to_db(),
                        album_size: None,
                    },
                ))?;
                // The units are the frames curation could actually read, not the album's size. A
                // stage that reported 80 on a 600-frame gallery would make the progress bar jump and
                // the throughput figure meaningless, and `StageScope::SelectedImages` is what this
                // stage declares.
                status.curated
            }
            StageId::Export => {
                // The one stage that writes outside the catalog, and the only arm here that can
                // decline on the wedding rather than on the release.
                //
                // **The autopilot never chooses a destination.** An export needs a folder, a
                // naming template, a size and a quality, and every one of those is a decision a
                // photographer makes in the export panel about this client. A run that invented
                // them would write three thousand JPEGs somewhere nobody asked for, at a size
                // nobody chose, and the fact that they were deletable afterwards is not a defence.
                //
                // So a run repeats the export this wedding has already been given - the same
                // destination, the same sets, the same policy, over whatever is selected *now* -
                // and when there has never been one it skips with `NoInput`: "There was nothing
                // for this step to work on", which is exactly true, and which leaves the run
                // `CompletedDegraded` with export named rather than reported as delivered.
                // `autopilot.toml`'s own row says a photographer whose run wrote nothing needs to
                // read why on the summary.
                let store = ExportStore::new(Arc::clone(self.state.catalog()));
                let Some(spec) = lift(store.last_spec(project).map_err(IpcError::from))? else {
                    return Ok(StageOutcome::Skipped(SkipCause::NoInput));
                };
                let images = lift(crate::delivery_commands::selected_images(
                    &self.state,
                    project,
                ))?;
                if images.is_empty() {
                    return Ok(StageOutcome::Skipped(SkipCause::NoInput));
                }
                let field = lift(ExportField::new(&self.state, project))?;
                let source = ExportSource::new(&self.state);
                let pass = ExportPass::new(&store, &field, &source, crate::state::APP_VERSION);
                let result = lift(
                    pass.run(project, &spec.over(&images))
                        .map_err(IpcError::from),
                )?;
                clamp(i64::try_from(result.files.len()).unwrap_or(i64::MAX))
            }
        };

        Ok(StageOutcome::Completed { items })
    }
}

// ---------------------------------------------------------------------------
// Building the orchestrator
// ---------------------------------------------------------------------------

/// Build an autopilot over this application state.
///
/// # Errors
///
/// `AURA-JOB-7008` when the shipped policy will not load, and whatever the explain service raises.
pub fn build_autopilot(state: &AppState) -> AuraResult<Autopilot> {
    let gate = AppGate::new(state)?;
    let probe = AppProbe::new(state.cache_root().to_path_buf(), None);
    // `AppState` is cheap to clone - the catalog, the clock and every store live behind an `Arc` -
    // so the runner owns its own handle rather than borrowing one that would have to outlive the
    // worker thread the run happens on.
    let owned = Arc::new(state.clone());
    Autopilot::new(
        Arc::clone(state.catalog()),
        Arc::clone(state.clock()),
        Ports {
            runner: Arc::new(AppRunner::new(owned)),
            gate: Arc::new(gate),
            probe: Arc::new(probe),
        },
    )
}

// ---------------------------------------------------------------------------
// The commands
// ---------------------------------------------------------------------------

/// What the Autopilot panel's header shows.
///
/// # Errors
///
/// `AURA-DB-3006` when the stored rows cannot be read.
pub fn autopilot_status(state: &AppState, project_id: &str) -> IpcResult<AutopilotStatusDto> {
    let project = parse_project(project_id)?;
    let autopilot = build_autopilot(state)?;
    let outline = autopilot.outline(project)?;
    Ok(AutopilotStatusDto {
        runs: outline.runs,
        latest_run: outline.latest_run.clone(),
        status: outline.status.map(|status| status.as_str().to_string()),
        stages_enabled: outline.stages_enabled,
        stages_completed: outline.stages_completed,
        stages_degraded: outline.stages_degraded,
        completeness: outline.completeness(),
        zero_touch: outline.zero_touch,
        calibrated: outline.calibrated,
        resource_events: outline.resource_events,
        bytes: outline.bytes,
        policy_ver: outline.policy_ver,
        orchestrator_ver: outline.orchestrator_ver,
    })
}

/// What would happen if the run started now.
///
/// # Errors
///
/// `AURA-DB-3006` when the catalog cannot be read.
pub fn autopilot_preflight(state: &AppState, project_id: &str) -> IpcResult<AutopilotPreflightDto> {
    let project = parse_project(project_id)?;
    let autopilot = build_autopilot(state)?;
    let settings = autopilot.store().settings(project)?;

    let images = image_count(state, project)?;
    let facts = Facts {
        project_opens: true,
        images,
        disk_free_bytes: free_space(state.cache_root()),
        // Twelve megabytes a delivered frame, which is a full-resolution JPEG at quality 90 from a
        // 45-megapixel body. An estimate rather than a measurement, and it is deliberately on the
        // generous side: the cost of over-estimating is a pre-flight warning, and the cost of
        // under-estimating is a disk that fills at 90 % of a two-hour run.
        estimated_output_bytes: u64::from(images) * 12_000_000,
        hardware_ready: state.hardware_plan().is_ok(),
        hardware_detail: hardware_sentence(state),
        missing_required_models: Vec::new(),
        missing_optional_models: Vec::new(),
        cloud_budget_usd: None,
        calibrated: false,
        on_battery: false,
        allow_on_battery: settings.allow_on_battery,
        planned: Vec::new(),
        held_stages: 0,
        disk_headroom: aura_jobs::contract::autopilot::DISK_HEADROOM,
    };

    let report = autopilot.preflight_with(project, &settings, facts)?;
    Ok(AutopilotPreflightDto {
        verdict: report.verdict().as_str().to_string(),
        permits_start: report.permits_start(),
        images: report.images,
        estimated_output_bytes: report.estimated_output_bytes,
        estimated_ms: report.estimated_ms,
        rows: report
            .rows
            .iter()
            .map(|row| AutopilotPreflightRowDto {
                check: row.check.as_str().to_string(),
                title: row.check.title().to_string(),
                verdict: row.verdict.as_str().to_string(),
                detail: row.detail.clone(),
            })
            .collect(),
    })
}

/// Start or continue this wedding's run.
///
/// Returns as soon as the run is planned; the work happens on a worker thread and the panel polls
/// `autopilot_progress`. A command that returned when the wedding finished would be a command that
/// blocked the IPC surface for two hours.
///
/// # Errors
///
/// `AURA-JOB-7004` when the pre-flight blocks, `AURA-JOB-7009` when a run is already in flight.
pub fn autopilot_start(
    state: &AppState,
    input: &AutopilotStartInput,
) -> IpcResult<AutopilotProgressDto> {
    let project = parse_project(&input.project_id)?;
    let autopilot = build_autopilot(state)?;

    let settings = AutopilotOverride {
        project,
        disabled: input
            .disabled
            .iter()
            .filter_map(|slug| StageId::parse(slug))
            .collect(),
        zero_touch: input.zero_touch,
        allow_on_battery: input.allow_on_battery,
        quiet_mode: input.quiet_mode,
    };

    let report = autopilot_preflight(state, &input.project_id)?;
    if !report.permits_start {
        let blocked = report
            .rows
            .iter()
            .filter(|row| row.verdict == "block")
            .map(|row| row.detail.clone())
            .collect::<Vec<_>>()
            .join(" ");
        return Err(IpcError::from(aura_jobs::errors::preflight_blocked(
            blocked,
        )));
    }

    let handle = autopilot.start(project, &settings)?;
    state.register_job(&handle.run_id.to_db(), handle.cancel.clone());
    let dto = progress_dto(&handle, "running");
    state.register_run(project, handle.clone());

    let worker_state = state.clone();
    let worker_handle = handle.clone();
    std::thread::spawn(move || {
        let Ok(autopilot) = build_autopilot(&worker_state) else {
            return;
        };
        // Where the summary sends a photographer at one in the morning. The destination this
        // wedding was last exported to when it has one, and the project's own directory when it
        // has not - which is the case where the export stage skips with `NoInput`, so the two
        // answers agree about what happened.
        let output_path = ExportStore::new(Arc::clone(worker_state.catalog()))
            .last_spec(project)
            .ok()
            .flatten()
            .and_then(|spec| spec.destination.local_root().map(PathBuf::from))
            .unwrap_or_else(|| worker_state.cache_root().to_path_buf());
        let tally = Tally {
            selected: 0,
            exported: 0,
            needs_review: 0,
            qc: None,
            spend_usd: 0.0,
            output_path,
        };
        drop(autopilot.execute(project, &settings, &worker_handle, &tally));
        worker_state.finish_job(&worker_handle.run_id.to_db());
        worker_state.finish_run(project);
    });

    Ok(dto)
}

/// What the run in flight is doing right now.
///
/// # Errors
///
/// `AURA-DB-3006` when the stored rows cannot be read.
pub fn autopilot_progress(
    state: &AppState,
    project_id: &str,
) -> IpcResult<Option<AutopilotProgressDto>> {
    let project = parse_project(project_id)?;
    let Some(handle) = state.run_of(project) else {
        return Ok(None);
    };
    Ok(Some(progress_dto(&handle, "running")))
}

/// Stop this wedding's run.
///
/// # Errors
///
/// `AURA-DB-3006` when the stored rows cannot be read.
pub fn autopilot_cancel(state: &AppState, project_id: &str) -> IpcResult<bool> {
    let project = parse_project(project_id)?;
    let Some(handle) = state.run_of(project) else {
        return Ok(false);
    };
    handle.cancel.cancel();
    Ok(true)
}

/// Every stage of the newest run.
///
/// # Errors
///
/// `AURA-DB-3006` when the stored rows cannot be read.
pub fn autopilot_stages(state: &AppState, project_id: &str) -> IpcResult<Vec<AutopilotStageDto>> {
    let project = parse_project(project_id)?;
    let autopilot = build_autopilot(state)?;
    Ok(autopilot
        .stages(project)?
        .iter()
        .map(|report| AutopilotStageDto {
            stage: report.stage.as_str().to_string(),
            title: report.stage.title().to_string(),
            outcome: report.outcome.clone(),
            skip_cause: report.skip_cause.map(|cause| cause.as_str().to_string()),
            skip_text: report.skip_cause.map(|cause| cause.user_text().to_string()),
            verdict: report.verdict.as_str().to_string(),
            items_done: report.items_done,
            items_total: report.items_total,
            elapsed_ms: report.elapsed_ms,
            attempts: u32::from(report.attempts),
            reasons: report
                .reasons
                .iter()
                .map(|reason| reason.code.as_str().to_string())
                .collect(),
        })
        .collect())
}

/// What the newest finished run did.
///
/// # Errors
///
/// `AURA-DB-3006` when the stored rows cannot be read.
pub fn autopilot_summary(
    state: &AppState,
    project_id: &str,
) -> IpcResult<Option<AutopilotSummaryDto>> {
    let project = parse_project(project_id)?;
    let autopilot = build_autopilot(state)?;
    let Some(summary) = autopilot.summary(project)? else {
        return Ok(None);
    };
    Ok(Some(AutopilotSummaryDto {
        run_id: summary.run_id.to_db(),
        status: summary.status.as_str().to_string(),
        status_title: summary.status.title().to_string(),
        selected: summary.selected,
        exported: summary.exported,
        needs_review: summary.needs_review,
        total_ms: summary.total_ms(),
        spend_usd: summary.spend_usd,
        output_path: summary.output_path.to_string_lossy().into_owned(),
        stage_timings: summary
            .stage_timings
            .iter()
            .map(|(stage, ms)| (stage.as_str().to_string(), *ms))
            .collect(),
        degraded_stages: summary
            .degraded_stages
            .iter()
            .map(|(stage, why)| (stage.as_str().to_string(), why.clone()))
            .collect(),
    }))
}

/// Everything the governor did during the newest run.
///
/// # Errors
///
/// `AURA-DB-3006` when the stored rows cannot be read.
pub fn autopilot_events(state: &AppState, project_id: &str) -> IpcResult<Vec<AutopilotEventDto>> {
    let project = parse_project(project_id)?;
    let autopilot = build_autopilot(state)?;
    Ok(autopilot
        .resource_events(project)?
        .iter()
        .map(|event| AutopilotEventDto {
            kind: event.kind.as_str().to_string(),
            action: event.action.as_str().to_string(),
            action_text: event.action.user_text().to_string(),
            reading: event.reading,
            threshold: event.threshold,
            stage: event.stage.as_str().to_string(),
        })
        .collect())
}

/// Record what the photographer chose in the checklist.
///
/// # Errors
///
/// `AURA-DB-3006` when the row cannot be written.
pub fn autopilot_set_settings(state: &AppState, input: &AutopilotSettingsInput) -> IpcResult<()> {
    let project = parse_project(&input.project_id)?;
    let autopilot = build_autopilot(state)?;
    autopilot.set_settings(&AutopilotOverride {
        project,
        disabled: input
            .disabled
            .iter()
            .filter_map(|slug| StageId::parse(slug))
            .collect(),
        zero_touch: input.zero_touch,
        allow_on_battery: input.allow_on_battery,
        quiet_mode: input.quiet_mode,
    })?;
    Ok(())
}

// -- helpers -----------------------------------------------------------------

fn progress_dto(handle: &RunHandle, status: &str) -> AutopilotProgressDto {
    let progress = handle.progress.borrow();
    AutopilotProgressDto {
        run_id: handle.run_id.to_db(),
        status: status.to_string(),
        stage: progress.stage.as_str().to_string(),
        stage_title: progress.stage.title().to_string(),
        stage_index: progress.stage_index,
        stage_total: progress.stage_total,
        items_done: progress.items_done,
        items_total: progress.items_total,
        eta_s: progress.eta_s,
        throughput_per_s: progress.throughput_per_s,
        spend_usd: progress.spend_usd,
        warnings: progress.warnings.clone(),
        current_image: progress.current_image.map(|id| id.to_db()),
        cancelled: handle.cancel.is_cancelled(),
    }
}

fn image_count(state: &AppState, project: ProjectId) -> IpcResult<u32> {
    let key = project.to_db();
    let count = state.catalog().read(move |conn: &rusqlite::Connection| {
        conn.query_row(
            "SELECT COUNT(*) FROM photo WHERE project_id = ?1",
            rusqlite::params![key],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|err| aura_core::errors::db::statement_failed("count photos", &err))
    })?;
    Ok(u32::try_from(count).unwrap_or(u32::MAX))
}

fn hardware_sentence(state: &AppState) -> String {
    match state.hardware_plan() {
        Ok(plan) => match &plan.gpu {
            Some(gpu) => format!("{} with {} MB.", gpu.name, plan.vram_budget_mb),
            None => "No accelerator was found, so everything runs on the processor.".to_string(),
        },
        Err(_) => String::new(),
    }
}

/// Parse a project id, refusing anything that is not one.
///
/// `AURA-JOB-7009` rather than a generic bad-request code, because there is no generic
/// bad-request code in this product: every error on every surface is a registered code with a
/// runbook, which is what makes `docs/runbooks/` navigable from a support bundle.
fn parse_project(project_id: &str) -> IpcResult<ProjectId> {
    ProjectId::from_db(project_id).map_err(|_| {
        IpcError::from(aura_jobs::errors::run_in_flight(format!(
            "`{project_id}` is not a project id"
        )))
    })
}
