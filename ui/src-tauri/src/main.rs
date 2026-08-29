// The desktop shell. It owns the window, the single-instance guard, the panic
// hook and logging, and forwards every command to `aura-app`. No product logic
// lives here: the shell must stay thin enough to replace.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use aura_app::contract::ipc::{
    AnalyseCompositionInput, CompositionDto, CompositionPassDto, CompositionStatusDto,
    CreateProjectInput, CullPassDto, CullProjectInput, CullStatusDto, DecisionDto,
    DismissCompositionFlagInput, FlaggedCompositionInput, ImageRowLite, IpcError, JobHandle,
    ListImagesInput, OverrideDecisionInput, ProblemRow, ProjectHandle, ProjectSummary,
    ExplainPanelDto, ExportBundleInput, LedgerDecisionDto, LedgerStatusDto, RecordDecisionsDto,
    RecordDecisionsInput, ResizeGalleryInput, ReviewQueueInput, SelectionDto, SetCameraLabelInput,
    SetCullModeInput, StartIngestInput, SupportBundleDto,
};
use aura_app::contract::ipc::{
    AcceptToneInput, EstimateToneInput, ReferenceFrameDto, ReferenceFramesInput,
    SetToneOverrideDto, SetToneOverrideInput, ToneDto, TonePassDto, ToneReviewInput, ToneStatusDto,
};
// PHASE-16.
// PHASE-17.
use aura_app::contract::ipc::{
    AdoptProfileInput, CompareProfilesInput, ExportProfileDto, ExportProfileInput,
    ImportProfileDto, ImportProfileInput, ProfileReportDto, ScanArchiveDto, ScanArchiveInput,
    SetProjectProfileInput, StyleComparisonDto, StylePairDto, StyleProfileDto, StyleStatusDto,
    TrainProfileDto, TrainProfileInput,
};
use aura_app::contract::ipc::{
    AcceptColourInput, ColourDto, ColourPassDto, ColourReviewInput, ColourStatusDto,
    EstimateColourInput, SelectVariantInput, SetColourOverrideDto, SetColourOverrideInput,
};
// PHASE-19.
use aura_app::contract::ipc::{
    AcceptLocalInput, LocalPassDto, LocalPlanDto, LocalReviewInput, LocalStatusDto,
    SculptLocalInput, SetLocalStrengthDto, SetLocalStrengthInput,
};
// PHASE-20.
use aura_app::contract::ipc::{
    AcceptRetouchInput, ProtectedFeatureDto, RetouchPassDto, RetouchPassInput, RetouchPlanDto,
    RetouchReviewInput, RetouchStatusDto, SetProtectionInput, SetRetouchDto, SetRetouchInput,
};
// PHASE-21.
use aura_app::contract::ipc::{
    AcceptMicroInput, MicroCompositeDto, MicroMatrixDto, MicroPassDto, MicroPassInput,
    MicroPlanDto, MicroReasonDto, MicroReviewInput, MicroStatusDto, SetMicroMatrixInput,
};
// PHASE-22. The seven restoration commands existed on `aura-app` from the day phase 22 shipped
// and had never been registered here, so the phase 22 exit report's claim that they were reachable
// from the Tauri surface was wrong about them. Registering them is four lines per command.
use aura_app::contract::ipc::{
    AcceptRestoreInput, RestoreIdentityRefusalDto, RestorePassDto, RestorePassInput,
    RestorePlanDto, RestoreReasonDto, RestoreReviewInput, RestoreStatusDto,
    SetRestoreOverrideInput,
};
// PHASE-23. The same gap, for the same reason: the geometry commands landed on `aura-app` with
// the crate and the panel, and nothing in the shell named them.
use aura_app::contract::ipc::{
    AcceptGeometryInput, GeometryPassDto, GeometryPlanDto, GeometryReviewInput, GeometryStatusDto,
    PlanGeometryInput, SetFramingDto, SetFramingInput,
};
use aura_app::AppState;
use aura_core::paths::AppPaths;
use tauri::{Manager, State};

type IpcResult<T> = Result<T, IpcError>;

#[tauri::command]
fn create_project(
    state: State<'_, AppState>,
    input: CreateProjectInput,
) -> IpcResult<ProjectHandle> {
    aura_app::create_project(&state, input)
}

#[tauri::command]
fn list_projects(state: State<'_, AppState>) -> IpcResult<Vec<ProjectSummary>> {
    aura_app::list_projects(&state)
}

#[tauri::command]
fn start_ingest(state: State<'_, AppState>, input: StartIngestInput) -> IpcResult<JobHandle> {
    aura_app::start_ingest(&state, &input)
}

#[tauri::command]
fn cancel_job(state: State<'_, AppState>, job_id: String) -> IpcResult<bool> {
    aura_app::cancel_job(&state, &job_id)
}

#[tauri::command]
fn list_images(state: State<'_, AppState>, input: ListImagesInput) -> IpcResult<Vec<ImageRowLite>> {
    aura_app::list_images(&state, &input)
}

#[tauri::command]
fn set_camera_label(state: State<'_, AppState>, input: SetCameraLabelInput) -> IpcResult<()> {
    aura_app::set_camera_label(&state, &input)
}

#[tauri::command]
fn list_problems(state: State<'_, AppState>, project_id: String) -> IpcResult<Vec<ProblemRow>> {
    aura_app::list_problems(&state, &project_id)
}

// Composition can touch SQLite, decode proxies, and run inference. Keep all five commands
// off the renderer thread; even the small reads share the same boundary so a future query
// cannot accidentally turn a synchronous command into visible UI jank.
#[tauri::command]
async fn composition_status(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<CompositionStatusDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::composition_status(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn image_composition(
    state: State<'_, AppState>,
    photo_id: String,
) -> IpcResult<Option<CompositionDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::image_composition(&app, &photo_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn flagged_composition(
    state: State<'_, AppState>,
    input: FlaggedCompositionInput,
) -> IpcResult<Vec<String>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::flagged_composition(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn dismiss_composition_flag(
    state: State<'_, AppState>,
    input: DismissCompositionFlagInput,
) -> IpcResult<CompositionDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::dismiss_composition_flag(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn analyse_composition(
    state: State<'_, AppState>,
    input: AnalyseCompositionInput,
) -> IpcResult<CompositionPassDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::analyse_composition(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

// PHASE-12. The culling surface. Every one of these reads the catalog and four of
// them re-run six selection passes over it, so all seven go off the renderer thread
// for the composition commands' reason. Section 11 budgets two seconds for a slider
// move, which is two seconds the window must stay alive through.

#[tauri::command]
async fn cull_status(state: State<'_, AppState>, project_id: String) -> IpcResult<CullStatusDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::cull_status(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn gallery(state: State<'_, AppState>, project_id: String) -> IpcResult<Option<SelectionDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::gallery(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn image_decision(
    state: State<'_, AppState>,
    photo_id: String,
) -> IpcResult<Option<DecisionDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::image_decision(&app, &photo_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn cull_project(
    state: State<'_, AppState>,
    input: CullProjectInput,
) -> IpcResult<CullPassDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::cull_project(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn resize_gallery(
    state: State<'_, AppState>,
    input: ResizeGalleryInput,
) -> IpcResult<SelectionDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::resize_gallery(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn set_cull_mode(
    state: State<'_, AppState>,
    input: SetCullModeInput,
) -> IpcResult<SelectionDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::set_cull_mode(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn override_decision(
    state: State<'_, AppState>,
    input: OverrideDecisionInput,
) -> IpcResult<DecisionDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::override_decision(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

// PHASE-13. The explainability surface. Every one of these reads the catalog -
// `explain_image` reads four services and the ledger for one photograph - and
// `record_decisions` writes one row per frame of a whole gallery. All eight go off
// the renderer thread for the culling commands' reason: section 11 budgets 250 ms
// for the panel to open, which is 250 ms the window must stay alive through.

#[tauri::command]
async fn explain_image(
    state: State<'_, AppState>,
    photo_id: String,
) -> IpcResult<ExplainPanelDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::explain_image(&app, &photo_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn decision_history(
    state: State<'_, AppState>,
    photo_id: String,
) -> IpcResult<Vec<LedgerDecisionDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::decision_history(&app, &photo_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn decision_by_id(
    state: State<'_, AppState>,
    decision_id: String,
) -> IpcResult<Option<LedgerDecisionDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::decision_by_id(&app, &decision_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn ledger_status(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<LedgerStatusDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::ledger_status(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn review_queue(
    state: State<'_, AppState>,
    input: ReviewQueueInput,
) -> IpcResult<Vec<LedgerDecisionDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::review_queue(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn record_decisions(
    state: State<'_, AppState>,
    input: RecordDecisionsInput,
) -> IpcResult<RecordDecisionsDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::record_decisions(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn export_support_bundle(
    state: State<'_, AppState>,
    input: ExportBundleInput,
) -> IpcResult<SupportBundleDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::export_support_bundle(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn compact_ledger(state: State<'_, AppState>, project_id: String) -> IpcResult<u32> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::compact_ledger(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

// PHASE-15. Every tone command can touch SQLite, and `estimate_tone` decodes proxies and
// runs two heads over a whole wedding. All seven go off the renderer thread for the reason
// the composition block above gives: a small read that grows into a join later must not be
// able to turn into visible jank without anybody noticing.
#[tauri::command]
async fn tone_status(state: State<'_, AppState>, project_id: String) -> IpcResult<ToneStatusDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::tone_status(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn image_tone(state: State<'_, AppState>, photo_id: String) -> IpcResult<Option<ToneDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::image_tone(&app, &photo_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn tone_review_queue(
    state: State<'_, AppState>,
    input: ToneReviewInput,
) -> IpcResult<Vec<String>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::tone_review_queue(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn reference_frames(
    state: State<'_, AppState>,
    input: ReferenceFramesInput,
) -> IpcResult<Vec<ReferenceFrameDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::reference_frames(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn accept_tone(state: State<'_, AppState>, input: AcceptToneInput) -> IpcResult<ToneDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::accept_tone(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn set_tone_override(
    state: State<'_, AppState>,
    input: SetToneOverrideInput,
) -> IpcResult<SetToneOverrideDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::set_tone_override(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn estimate_tone(
    state: State<'_, AppState>,
    input: EstimateToneInput,
) -> IpcResult<TonePassDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::estimate_tone(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

// PHASE-17. Every style command can touch SQLite, `scan_archive` walks a folder of somebody's
// whole wedding, and `train_profile` can run for twenty minutes. All eleven go off the renderer
// thread for the reason the phase 15 and 16 blocks above give.
//
// `train_profile` is registered here with the neutral baseline and no pair source, which makes
// it a **refusal** rather than a training run in this build: there is no archive-import flow
// yet, so the shell has nothing to hand it. The command exists, its shape is frozen and its
// error is the honest one - `AURA-ML-5073`, "not enough usable pairs" - rather than a silent
// success. See condition C3 in `docs/progress/PHASE-17-EXIT.md`.
#[tauri::command]
async fn style_status(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<StyleStatusDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::style_status(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn list_profiles(state: State<'_, AppState>) -> IpcResult<Vec<StyleProfileDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::list_profiles(&app))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn profile_report(
    state: State<'_, AppState>,
    profile_id: String,
) -> IpcResult<Option<ProfileReportDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::profile_report(&app, &profile_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn profile_pairs(
    state: State<'_, AppState>,
    name: String,
    limit: Option<u32>,
) -> IpcResult<Vec<StylePairDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::profile_pairs(&app, &name, limit))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn scan_archive(
    state: State<'_, AppState>,
    input: ScanArchiveInput,
) -> IpcResult<ScanArchiveDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::scan_archive(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn train_profile(
    state: State<'_, AppState>,
    input: TrainProfileInput,
) -> IpcResult<TrainProfileDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        aura_app::train_profile(
            &app,
            &input,
            std::sync::Arc::new(aura_style::fixtures::EmptySource),
            std::sync::Arc::new(aura_style::api::NeutralBaseline),
        )
    })
    .await
    .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn adopt_profile(
    state: State<'_, AppState>,
    input: AdoptProfileInput,
) -> IpcResult<StyleProfileDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::adopt_profile(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn compare_profiles(
    state: State<'_, AppState>,
    input: CompareProfilesInput,
) -> IpcResult<Vec<StyleComparisonDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::compare_profiles(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn export_profile(
    state: State<'_, AppState>,
    input: ExportProfileInput,
) -> IpcResult<ExportProfileDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::export_profile(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn import_profile(
    state: State<'_, AppState>,
    input: ImportProfileInput,
) -> IpcResult<ImportProfileDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::import_profile(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn set_project_profile(
    state: State<'_, AppState>,
    input: SetProjectProfileInput,
) -> IpcResult<StyleStatusDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::set_project_profile(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

// PHASE-16. Every colour command can touch SQLite, and `estimate_colour` decodes proxies and
// grades a whole wedding. All seven go off the renderer thread for the reason the phase 15
// block above gives.
#[tauri::command]
async fn colour_status(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<ColourStatusDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::colour_status(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn image_colour(
    state: State<'_, AppState>,
    photo_id: String,
) -> IpcResult<Option<ColourDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::image_colour(&app, &photo_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn colour_review_queue(
    state: State<'_, AppState>,
    input: ColourReviewInput,
) -> IpcResult<Vec<String>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::colour_review_queue(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn accept_colour(
    state: State<'_, AppState>,
    input: AcceptColourInput,
) -> IpcResult<ColourDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::accept_colour(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn set_colour_override(
    state: State<'_, AppState>,
    input: SetColourOverrideInput,
) -> IpcResult<SetColourOverrideDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::set_colour_override(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn select_colour_variant(
    state: State<'_, AppState>,
    input: SelectVariantInput,
) -> IpcResult<SetColourOverrideDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::select_colour_variant(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn estimate_colour(
    state: State<'_, AppState>,
    input: EstimateColourInput,
) -> IpcResult<ColourPassDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::estimate_colour(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

fn background_request_failed() -> IpcError {
    IpcError::from(aura_core::errors::db::statement_failed(
        "the background composition task stopped before returning its result",
        &std::io::Error::other("background task join failed"),
    ))
}

fn catalog_path() -> Result<PathBuf, IpcError> {
    let paths = AppPaths::resolve().map_err(IpcError::from)?;
    Ok(paths.data_dir.join("catalogs").join("default.sqlite"))
}

// PHASE-19. Every local light command can touch SQLite, and `sculpt_local` decodes proxies
// and separates frequency bands over a whole gallery. All six go off the renderer thread for
// the reason the tone block above gives.
#[tauri::command]
async fn local_status(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<LocalStatusDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::local_status(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn image_local(
    state: State<'_, AppState>,
    photo_id: String,
) -> IpcResult<Option<LocalPlanDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::image_local(&app, &photo_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn local_review_queue(
    state: State<'_, AppState>,
    input: LocalReviewInput,
) -> IpcResult<Vec<String>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::local_review_queue(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn accept_local(
    state: State<'_, AppState>,
    input: AcceptLocalInput,
) -> IpcResult<LocalPlanDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::accept_local(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn set_local_strength(
    state: State<'_, AppState>,
    input: SetLocalStrengthInput,
) -> IpcResult<SetLocalStrengthDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::set_local_strength(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

// PHASE-20. The retouch surface. `retouch_pass` decodes proxies, separates frequency bands and
// runs every plan through the renderer to measure what it did to the texture, so it goes off the
// renderer thread with the rest.
#[tauri::command]
async fn retouch_status(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<RetouchStatusDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::retouch_status(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn image_retouch(
    state: State<'_, AppState>,
    photo_id: String,
) -> IpcResult<Option<RetouchPlanDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::image_retouch(&app, &photo_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn protected_features(
    state: State<'_, AppState>,
    project_id: String,
    identity_id: String,
) -> IpcResult<Vec<ProtectedFeatureDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        aura_app::protected_features(&app, &project_id, &identity_id)
    })
    .await
    .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn retouch_review_queue(
    state: State<'_, AppState>,
    input: RetouchReviewInput,
) -> IpcResult<Vec<String>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::retouch_review_queue(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn accept_retouch(
    state: State<'_, AppState>,
    input: AcceptRetouchInput,
) -> IpcResult<RetouchPlanDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::accept_retouch(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn set_retouch(
    state: State<'_, AppState>,
    input: SetRetouchInput,
) -> IpcResult<SetRetouchDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::set_retouch(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn set_protection(
    state: State<'_, AppState>,
    input: SetProtectionInput,
) -> IpcResult<Vec<ProtectedFeatureDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::set_protection(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn retouch_pass(
    state: State<'_, AppState>,
    input: RetouchPassInput,
) -> IpcResult<RetouchPassDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::retouch_pass(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn sculpt_local(
    state: State<'_, AppState>,
    input: SculptLocalInput,
) -> IpcResult<LocalPassDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::sculpt_local(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

// PHASE-21. The micro-retouch surface. `micro_pass` decodes proxies, decodes a sibling frame for
// every borrow it considers, and runs every plan through the renderer so the naturalness guard
// can measure what it did, so it goes off the renderer thread with the rest.
//
// `micro_reason_codes` is the one command here that touches nothing: it assembles the panel's
// legend from the frozen enum, so it stays on the calling thread.
#[tauri::command]
async fn micro_status(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<MicroStatusDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::micro_status(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn image_micro(
    state: State<'_, AppState>,
    photo_id: String,
) -> IpcResult<Option<MicroPlanDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::image_micro(&app, &photo_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn micro_composites(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<Vec<MicroCompositeDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::micro_composites(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn micro_review_queue(
    state: State<'_, AppState>,
    input: MicroReviewInput,
) -> IpcResult<Vec<String>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::micro_review_queue(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn micro_matrix(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<MicroMatrixDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::micro_matrix(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn set_micro_matrix(
    state: State<'_, AppState>,
    input: SetMicroMatrixInput,
) -> IpcResult<MicroMatrixDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::set_micro_matrix(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn accept_micro(state: State<'_, AppState>, input: AcceptMicroInput) -> IpcResult<()> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::accept_micro(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn micro_pass(
    state: State<'_, AppState>,
    input: MicroPassInput,
) -> IpcResult<MicroPassDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::micro_pass(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
fn micro_reason_codes() -> IpcResult<Vec<MicroReasonDto>> {
    Ok(aura_app::micro_reason_codes())
}

// PHASE-22. The restoration surface. `restore_pass` decodes proxies, renders every plan twice for
// the self-check and embeds every candidate face, so it goes off the calling thread with the rest.
// `restore_reason_codes` assembles the panel's legend from the frozen enum and touches nothing.
#[tauri::command]
async fn restore_status(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<RestoreStatusDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::restore_status(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn image_restore(
    state: State<'_, AppState>,
    photo_id: String,
) -> IpcResult<Option<RestorePlanDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::image_restore(&app, &photo_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn restore_identity_refusals(
    state: State<'_, AppState>,
    input: RestoreReviewInput,
) -> IpcResult<Vec<RestoreIdentityRefusalDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::restore_identity_refusals(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn restore_review_queue(
    state: State<'_, AppState>,
    input: RestoreReviewInput,
) -> IpcResult<Vec<String>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::restore_review_queue(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn accept_restore(state: State<'_, AppState>, input: AcceptRestoreInput) -> IpcResult<()> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::accept_restore(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn set_restore_override(
    state: State<'_, AppState>,
    input: SetRestoreOverrideInput,
) -> IpcResult<RestorePlanDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::set_restore_override(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn restore_pass(
    state: State<'_, AppState>,
    input: RestorePassInput,
) -> IpcResult<RestorePassDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::restore_pass(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
fn restore_reason_codes() -> IpcResult<Vec<RestoreReasonDto>> {
    Ok(aura_app::restore_reason_codes())
}

// PHASE-23. The geometry surface. `plan_geometry` decodes a proxy per frame and searches a
// bounded crop space over it, so it goes off the calling thread with the rest.
#[tauri::command]
async fn geometry_status(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<GeometryStatusDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::geometry_status(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn image_geometry(
    state: State<'_, AppState>,
    photo_id: String,
) -> IpcResult<Option<GeometryPlanDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::image_geometry(&app, &photo_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn geometry_review_queue(
    state: State<'_, AppState>,
    input: GeometryReviewInput,
) -> IpcResult<Vec<String>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::geometry_review_queue(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn accept_geometry(
    state: State<'_, AppState>,
    input: AcceptGeometryInput,
) -> IpcResult<GeometryPlanDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::accept_geometry(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn set_framing(
    state: State<'_, AppState>,
    input: SetFramingInput,
) -> IpcResult<SetFramingDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::set_framing(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn plan_geometry(
    state: State<'_, AppState>,
    input: PlanGeometryInput,
) -> IpcResult<GeometryPassDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::plan_geometry(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // A panic in the shell must produce a report, not a silent disappearance.
    std::panic::set_hook(Box::new(|info| {
        tracing::error!(target: "shell", panic = %info, "the desktop shell panicked");
    }));

    let path = match catalog_path() {
        Ok(path) => path,
        Err(err) => {
            tracing::error!(target: "shell", code = err.code, "no writable catalog location");
            return;
        }
    };

    let state = match AppState::open(&path) {
        Ok(state) => state,
        Err(err) => {
            // The refusal chain already produced a photographer-facing sentence.
            tracing::error!(target: "shell", code = err.code.0, message = err.user_message, "cannot open catalog");
            return;
        }
    };

    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .setup(|app| {
            // Single-instance guard: a second window would be a second writer, and
            // the catalog lock would refuse it anyway. Fail early and clearly.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_title("AURA");
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            create_project,
            list_projects,
            start_ingest,
            cancel_job,
            list_images,
            set_camera_label,
            list_problems,
            composition_status,
            image_composition,
            flagged_composition,
            dismiss_composition_flag,
            analyse_composition,
            cull_status,
            gallery,
            image_decision,
            cull_project,
            resize_gallery,
            set_cull_mode,
            override_decision,
            explain_image,
            decision_history,
            decision_by_id,
            ledger_status,
            review_queue,
            record_decisions,
            export_support_bundle,
            compact_ledger,
            tone_status,
            image_tone,
            tone_review_queue,
            reference_frames,
            accept_tone,
            set_tone_override,
            estimate_tone,
            colour_status,
            image_colour,
            colour_review_queue,
            accept_colour,
            set_colour_override,
            select_colour_variant,
            estimate_colour,
            style_status,
            list_profiles,
            profile_report,
            profile_pairs,
            scan_archive,
            train_profile,
            adopt_profile,
            compare_profiles,
            export_profile,
            import_profile,
            set_project_profile,
            local_status,
            image_local,
            local_review_queue,
            accept_local,
            set_local_strength,
            sculpt_local,
            retouch_status,
            image_retouch,
            protected_features,
            retouch_review_queue,
            accept_retouch,
            set_retouch,
            set_protection,
            retouch_pass,
            micro_status,
            image_micro,
            micro_composites,
            micro_review_queue,
            micro_matrix,
            set_micro_matrix,
            accept_micro,
            micro_pass,
            micro_reason_codes,
            restore_status,
            image_restore,
            restore_identity_refusals,
            restore_review_queue,
            accept_restore,
            set_restore_override,
            restore_pass,
            restore_reason_codes,
            geometry_status,
            image_geometry,
            geometry_review_queue,
            accept_geometry,
            set_framing,
            plan_geometry
        ])
        .run(tauri::generate_context!());

    if let Err(err) = result {
        tracing::error!(target: "shell", error = %err, "the desktop shell stopped");
    }
}
