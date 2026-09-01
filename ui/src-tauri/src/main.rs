// The desktop shell. It owns the window, the single-instance guard, the panic
// hook and logging, and forwards every command to `aura-app`. No product logic
// lives here: the shell must stay thin enough to replace.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use aura_app::contract::ipc::{
    // PHASE-28.
    AutopilotEventDto, AutopilotPreflightDto, AutopilotProgressDto, AutopilotSettingsInput,
    AutopilotStageDto, AutopilotStartInput, AutopilotStatusDto, AutopilotSummaryDto,
    AcceptToneInput, CleanupBlockedDto, CleanupDisclosureDto, CleanupPassDto, CleanupPassInput,
    CleanupProposalDto, CleanupReasonDto, CleanupStatusDto, DecideCleanupInput,
    DisableCleanupInput, EstimateToneInput, ManualRemoveDto, ManualRemoveInput, ReferenceFrameDto,
    ReferenceFramesInput, SetToneOverrideDto, SetToneOverrideInput, ToneDto, TonePassDto,
    ToneReviewInput, ToneStatusDto,
};
use aura_app::contract::ipc::{
    AnalyseCompositionInput, CompositionDto, CompositionPassDto, CompositionStatusDto,
    CreateProjectInput, CullPassDto, CullProjectInput, CullStatusDto, DecisionDto,
    DismissCompositionFlagInput, ExplainPanelDto, ExportBundleInput, FlaggedCompositionInput,
    ImageRowLite, IpcError, JobHandle, LedgerDecisionDto, LedgerStatusDto, ListImagesInput,
    OverrideDecisionInput, ProblemRow, ProjectHandle, ProjectSummary, RecordDecisionsDto,
    RecordDecisionsInput, ResizeGalleryInput, ReviewQueueInput, SelectionDto, SetCameraLabelInput,
    SetCullModeInput, StartIngestInput, SupportBundleDto,
};
// PHASE-16.
// PHASE-17.
use aura_app::contract::ipc::{
    AcceptColourInput, ColourDto, ColourPassDto, ColourReviewInput, ColourStatusDto,
    EstimateColourInput, SelectVariantInput, SetColourOverrideDto, SetColourOverrideInput,
};
use aura_app::contract::ipc::{
    AdoptProfileInput, CompareProfilesInput, ExportProfileDto, ExportProfileInput,
    ImportProfileDto, ImportProfileInput, ProfileReportDto, ScanArchiveDto, ScanArchiveInput,
    SetProjectProfileInput, StyleComparisonDto, StylePairDto, StyleProfileDto, StyleStatusDto,
    TrainProfileDto, TrainProfileInput,
};
// PHASE-25. The gallery consistency surface: nine commands whose subject is a wedding
// rather than a photograph. `GalleryStatusDto` carries two denominators and a panel has to
// read both: a project at 100 % coverage with 20 % anchored has had almost nothing done to
// it, because an unanchored node produces a zero delta for every frame in it.
use aura_app::contract::ipc::{
    DisableGalleryInput, GalleryDeltaDto, GalleryOutlierDto, GalleryOverrideInput, GalleryPassDto,
    GalleryPassInput, GalleryReasonDto, GalleryStatusDto, PinAnchorInput, SceneNodeDto,
};
// PHASE-26. The multi-camera matching surface: eleven commands whose subject is a camera body
// rather than a photograph or a wedding. `CameraStatusDto` carries the evidence beside every
// number and `baselinesMeasured` beside the whole thing, because a body corrected from twenty
// pairs of its own ceremony and one corrected from a fabricated brand setting are the same
// arithmetic and completely different claims.
use aura_app::contract::ipc::{
    CameraFingerprintDto, CameraOverrideInput, CameraPassDto, CameraPassInput, CameraReasonDto,
    CameraReportDto, CameraStatusDto, CameraTransformDto, DisableCameraInput, MatchedPairDto,
    SetCameraReferenceInput, ShooterBiasDto,
};
// PHASE-27. The quality-control surface: nine commands whose subject is a *problem* rather than a
// photograph. `QcStatusDto.completeness` travels with every other number on it, because an empty
// QC result means either "AURA looked at everything and it is fine" or "AURA could not look", and
// in this build the second is the common case.
use aura_app::contract::ipc::{
    QcDecideBulkInput, QcDecideInput, QcGroupDto, QcPassInput, QcReportDto, QcRoundDto,
    QcStatusDto, QcTicketDto,
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
// The types the ninety newly registered commands name.
use aura_app::contract::ipc::{
    AnalyseIntegrityInput, CacheStatsDto, ChapterHandleDto, ClassifyScenesInput,
    CloudCacheStatsDto, CloudCallDto, CloudSpendDto, CloudStatusDto, DescriptorsDto,
    DevelopImageInput, DevelopStatusDto, DismissFlagInput, DuplicateSetDto, EditMaskInput,
    EmbedProgressDto, EmbedProjectInput, EmotionDto, EmotionPassDto, EmotionStatusDto,
    EnsureMasksInput, EraseBiometricsDto, EraseBiometricsInput, FaceCropDto, FindSimilarInput,
    FlaggedInput, GetPreviewInput, GroupMomentsInput, GroupPeopleDto, GroupPeopleInput,
    HardwarePlanDto, HistoryDto, HistoryStepInput, IdentityCardDto, IdentityHandleDto,
    IdentityTimelineDto, ImageSubjectsDto, IndexStatusDto, InferStatsDto, IntegrityDto,
    IntegrityPassDto, IntegrityStatusDto, KeyCheckDto, LockMomentInput, MaskAllowanceDto, MaskDto,
    MaskOverlayDto, MaskStatusDto, MergeChaptersInput, MergeIdentitiesInput, MergeMomentsInput,
    ModelStatusDto, MomentDto, MomentEditDto, MomentHandleDto, MomentListDto, MomentPeakDto,
    MomentStatusDto, MomentsInput, MoveBoundaryInput, PeopleStatusDto, PreferInput, PrefetchInput,
    PreviewPayload, RankedByEmotionDto, RankedFrameDto, RankedInput, ReactionLinkDto, RecipeDto,
    RenameIdentityInput, RenderCapsDto, RenderDto, RenderImageInput, ScanFacesDto, ScanFacesInput,
    SceneDto, SceneProfileDto, ScoreEmotionInput, SetAiKeyInput, SetCacheBudgetInput,
    SetChapterInput, SetCloudBudgetInput, SetCloudPrivacyInput, SetExecutionProviderInput,
    SetIdentityImportanceInput, SetIdentityRoleInput, SetKeepHintInput, SetParamDto, SetParamInput,
    SetPeakInput, SimilarResultDto, SnapshotInput, SplitChapterInput, SplitIdentityInput,
    SplitMomentInput, StoryOutlineDto, StoryStatusDto, WarmupReportDto, WithinMomentInput,
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
async fn gallery(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<Option<SelectionDto>> {
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
async fn explain_image(state: State<'_, AppState>, photo_id: String) -> IpcResult<ExplainPanelDto> {
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
async fn style_status(state: State<'_, AppState>, project_id: String) -> IpcResult<StyleStatusDto> {
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
async fn local_status(state: State<'_, AppState>, project_id: String) -> IpcResult<LocalStatusDto> {
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
async fn micro_status(state: State<'_, AppState>, project_id: String) -> IpcResult<MicroStatusDto> {
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
async fn micro_matrix(state: State<'_, AppState>, project_id: String) -> IpcResult<MicroMatrixDto> {
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
async fn micro_pass(state: State<'_, AppState>, input: MicroPassInput) -> IpcResult<MicroPassDto> {
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

// Registered by the merge that brought phases 20, 21 and 22 onto main. Every command
// below was exported by `aura-app` when its phase shipped and was never named in
// `generate_handler!`, so `ui/src/ipc/client.ts` called ninety commands the window did
// not answer to. The wrappers are generated from the command functions' own signatures
// rather than typed by hand, because this crate cannot be compiled on this machine and a
// wrong argument name would not be caught by anything.
//
// Every one of them goes through `spawn_blocking` for the reason phases 15 to 23 give:
// a command that opens a catalog, decodes a proxy or renders must not run on the thread
// the window paints from.

// PHASE-04. The cloud AI gateway.
#[tauri::command]
async fn check_ai_key(state: State<'_, AppState>) -> IpcResult<KeyCheckDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::check_ai_key(&app))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn clear_ai_key(state: State<'_, AppState>, provider: String) -> IpcResult<CloudStatusDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::clear_ai_key(&app, &provider))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn cloud_cache_stats(state: State<'_, AppState>) -> IpcResult<CloudCacheStatsDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::cloud_cache_stats(&app))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn cloud_calls(
    state: State<'_, AppState>,
    project_id: String,
    limit: u32,
) -> IpcResult<Vec<CloudCallDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::cloud_calls(&app, &project_id, limit))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn cloud_spend(state: State<'_, AppState>, project_id: String) -> IpcResult<CloudSpendDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::cloud_spend(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn cloud_status(state: State<'_, AppState>) -> IpcResult<CloudStatusDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::cloud_status(&app))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn purge_cloud_cache(
    state: State<'_, AppState>,
    task: String,
    task_version: u32,
) -> IpcResult<u64> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        aura_app::purge_cloud_cache(&app, &task, task_version)
    })
    .await
    .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn set_ai_key(state: State<'_, AppState>, input: SetAiKeyInput) -> IpcResult<CloudStatusDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::set_ai_key(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn set_cloud_budget(
    state: State<'_, AppState>,
    input: SetCloudBudgetInput,
) -> IpcResult<CloudSpendDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::set_cloud_budget(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn set_cloud_privacy(
    state: State<'_, AppState>,
    input: SetCloudPrivacyInput,
) -> IpcResult<CloudStatusDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::set_cloud_privacy(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

// PHASE-14. The develop engine and the edit recipe.
#[tauri::command]
async fn develop_status(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<DevelopStatusDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::develop_status(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn history_step(
    state: State<'_, AppState>,
    input: HistoryStepInput,
) -> IpcResult<SetParamDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::history_step(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn image_history(
    state: State<'_, AppState>,
    input: DevelopImageInput,
) -> IpcResult<HistoryDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::image_history(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn image_recipe(
    state: State<'_, AppState>,
    input: DevelopImageInput,
) -> IpcResult<RecipeDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::image_recipe(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn render_caps(state: State<'_, AppState>) -> IpcResult<RenderCapsDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::render_caps(&app))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn render_image(state: State<'_, AppState>, input: RenderImageInput) -> IpcResult<RenderDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::render_image(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn set_param(state: State<'_, AppState>, input: SetParamInput) -> IpcResult<SetParamDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::set_param(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn snapshot(state: State<'_, AppState>, input: SnapshotInput) -> IpcResult<HistoryDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::snapshot(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

// PHASE-10. Emotion and moment ranking.
#[tauri::command]
async fn emotion_status(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<EmotionStatusDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::emotion_status(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn image_emotion(
    state: State<'_, AppState>,
    photo_id: String,
) -> IpcResult<Option<EmotionDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::image_emotion(&app, &photo_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn moment_peak(
    state: State<'_, AppState>,
    moment_id: String,
) -> IpcResult<Option<MomentPeakDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::moment_peak(&app, &moment_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn prefer_frame(state: State<'_, AppState>, input: PreferInput) -> IpcResult<()> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::prefer_frame(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn ranked_by_emotion(
    state: State<'_, AppState>,
    input: RankedInput,
) -> IpcResult<Vec<RankedByEmotionDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::ranked_by_emotion(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn reactions_of(
    state: State<'_, AppState>,
    photo_id: String,
) -> IpcResult<Vec<ReactionLinkDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::reactions_of(&app, &photo_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn score_emotion(
    state: State<'_, AppState>,
    input: ScoreEmotionInput,
) -> IpcResult<EmotionPassDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::score_emotion(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn set_moment_peak(
    state: State<'_, AppState>,
    input: SetPeakInput,
) -> IpcResult<MomentPeakDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::set_moment_peak(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

// PHASE-05. Embeddings and the similarity index.
#[tauri::command]
async fn build_index(state: State<'_, AppState>, project_id: String) -> IpcResult<IndexStatusDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::build_index(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn embed_project(
    state: State<'_, AppState>,
    input: EmbedProjectInput,
) -> IpcResult<EmbedProgressDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::embed_project(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn find_similar(
    state: State<'_, AppState>,
    input: FindSimilarInput,
) -> IpcResult<SimilarResultDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::find_similar(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn image_descriptors(
    state: State<'_, AppState>,
    project_id: String,
    photo_id: String,
) -> IpcResult<DescriptorsDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        aura_app::image_descriptors(&app, &project_id, &photo_id)
    })
    .await
    .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn index_status(state: State<'_, AppState>, project_id: String) -> IpcResult<IndexStatusDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::index_status(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

// PHASE-03. The inference runtime and the model registry.
#[tauri::command]
async fn hardware_plan(state: State<'_, AppState>) -> IpcResult<HardwarePlanDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::hardware_plan(&app))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn infer_stats(state: State<'_, AppState>) -> IpcResult<InferStatsDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::infer_stats(&app))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn list_models(state: State<'_, AppState>) -> IpcResult<Vec<ModelStatusDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::list_models(&app))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn recheck_hardware(state: State<'_, AppState>) -> IpcResult<HardwarePlanDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::recheck_hardware(&app))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn set_execution_provider(
    state: State<'_, AppState>,
    input: SetExecutionProviderInput,
) -> IpcResult<HardwarePlanDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::set_execution_provider(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn warmup_models(state: State<'_, AppState>) -> IpcResult<WarmupReportDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::warmup_models(&app))
        .await
        .map_err(|_| background_request_failed())?
}

// PHASE-09. Frame integrity.
#[tauri::command]
async fn analyse_integrity(
    state: State<'_, AppState>,
    input: AnalyseIntegrityInput,
) -> IpcResult<IntegrityPassDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::analyse_integrity(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn dismiss_flag(
    state: State<'_, AppState>,
    input: DismissFlagInput,
) -> IpcResult<IntegrityDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::dismiss_flag(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn flagged_images(state: State<'_, AppState>, input: FlaggedInput) -> IpcResult<Vec<String>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::flagged_images(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn image_integrity(
    state: State<'_, AppState>,
    photo_id: String,
) -> IpcResult<Option<IntegrityDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::image_integrity(&app, &photo_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn integrity_status(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<IntegrityStatusDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::integrity_status(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn within_moment(
    state: State<'_, AppState>,
    input: WithinMomentInput,
) -> IpcResult<Vec<RankedFrameDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::within_moment(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

// PHASE-18. Semantic masks.
#[tauri::command]
async fn edit_mask(state: State<'_, AppState>, input: EditMaskInput) -> IpcResult<MaskDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::edit_mask(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn ensure_masks(
    state: State<'_, AppState>,
    input: EnsureMasksInput,
) -> IpcResult<Vec<MaskDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::ensure_masks(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn image_masks(state: State<'_, AppState>, image_id: String) -> IpcResult<Vec<MaskDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::image_masks(&app, &image_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn mask_allowance(
    state: State<'_, AppState>,
    mask_id: String,
    operation: String,
) -> IpcResult<MaskAllowanceDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        aura_app::mask_allowance(&app, &mask_id, &operation)
    })
    .await
    .map_err(|_| background_request_failed())?
}

#[tauri::command]
fn mask_kinds() -> IpcResult<Vec<String>> {
    Ok(aura_app::mask_kinds())
}

#[tauri::command]
async fn mask_overlay(state: State<'_, AppState>, mask_id: String) -> IpcResult<MaskOverlayDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::mask_overlay(&app, &mask_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn mask_status(state: State<'_, AppState>, project_id: String) -> IpcResult<MaskStatusDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::mask_status(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn regenerate_mask(state: State<'_, AppState>, mask_id: String) -> IpcResult<bool> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::regenerate_mask(&app, &mask_id))
        .await
        .map_err(|_| background_request_failed())?
}

// PHASE-08. Moments, bursts and duplicates.
#[tauri::command]
async fn group_moments(
    state: State<'_, AppState>,
    input: GroupMomentsInput,
) -> IpcResult<MomentStatusDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::group_moments(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn list_moments(state: State<'_, AppState>, input: MomentsInput) -> IpcResult<MomentListDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::list_moments(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn lock_moment(
    state: State<'_, AppState>,
    input: LockMomentInput,
) -> IpcResult<MomentHandleDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::lock_moment(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn merge_moments(
    state: State<'_, AppState>,
    input: MergeMomentsInput,
) -> IpcResult<MomentHandleDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::merge_moments(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn moment_duplicates(
    state: State<'_, AppState>,
    moment_id: String,
) -> IpcResult<Vec<DuplicateSetDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::moment_duplicates(&app, &moment_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn moment_of_image(
    state: State<'_, AppState>,
    photo_id: String,
) -> IpcResult<Option<MomentDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::moment_of_image(&app, &photo_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn moment_status(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<MomentStatusDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::moment_status(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn set_keep_hint(
    state: State<'_, AppState>,
    input: SetKeepHintInput,
) -> IpcResult<MomentHandleDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::set_keep_hint(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn split_moment(
    state: State<'_, AppState>,
    input: SplitMomentInput,
) -> IpcResult<MomentHandleDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::split_moment(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn undo_moment_edit(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<MomentEditDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::undo_moment_edit(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

// PHASE-06. People intelligence.
#[tauri::command]
async fn image_subjects(
    state: State<'_, AppState>,
    photo_id: String,
) -> IpcResult<ImageSubjectsDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::image_subjects(&app, &photo_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn erase_biometrics(
    state: State<'_, AppState>,
    input: EraseBiometricsInput,
) -> IpcResult<EraseBiometricsDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::erase_biometrics(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn group_people(
    state: State<'_, AppState>,
    input: GroupPeopleInput,
) -> IpcResult<GroupPeopleDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::group_people(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn identity_cover(
    state: State<'_, AppState>,
    project_id: String,
    face_id: String,
) -> IpcResult<FaceCropDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        aura_app::identity_cover(&app, &project_id, &face_id)
    })
    .await
    .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn identity_timelines(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<Vec<IdentityTimelineDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::identity_timelines(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn list_identities(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<Vec<IdentityCardDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::list_identities(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn merge_identities(
    state: State<'_, AppState>,
    input: MergeIdentitiesInput,
) -> IpcResult<IdentityHandleDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::merge_identities(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn people_status(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<PeopleStatusDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::people_status(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn rename_identity(state: State<'_, AppState>, input: RenameIdentityInput) -> IpcResult<()> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::rename_identity(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn scan_faces(state: State<'_, AppState>, input: ScanFacesInput) -> IpcResult<ScanFacesDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::scan_faces(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn set_identity_importance(
    state: State<'_, AppState>,
    input: SetIdentityImportanceInput,
) -> IpcResult<()> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::set_identity_importance(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn set_identity_role(
    state: State<'_, AppState>,
    input: SetIdentityRoleInput,
) -> IpcResult<()> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::set_identity_role(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn split_identity(
    state: State<'_, AppState>,
    input: SplitIdentityInput,
) -> IpcResult<IdentityHandleDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::split_identity(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

// PHASE-02. Previews and the pixel cache.
#[tauri::command]
async fn cancel_previews(
    state: State<'_, AppState>,
    project_id: String,
    photo_ids: [String],
) -> IpcResult<i64> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        aura_app::cancel_previews(&app, &project_id, &photo_ids)
    })
    .await
    .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn get_preview(
    state: State<'_, AppState>,
    input: GetPreviewInput,
) -> IpcResult<PreviewPayload> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::get_preview(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn prefetch_previews(state: State<'_, AppState>, input: PrefetchInput) -> IpcResult<i64> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::prefetch_previews(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn preview_problems(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<Vec<(String, String)>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::preview_problems(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn preview_stats(state: State<'_, AppState>, project_id: String) -> IpcResult<CacheStatsDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::preview_stats(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn purge_cache(state: State<'_, AppState>, project_id: String) -> IpcResult<CacheStatsDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::purge_cache(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn set_cache_budget(
    state: State<'_, AppState>,
    input: SetCacheBudgetInput,
) -> IpcResult<CacheStatsDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::set_cache_budget(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

// PHASE-07. The wedding scene and story surface.
#[tauri::command]
async fn classify_scenes(
    state: State<'_, AppState>,
    input: ClassifyScenesInput,
) -> IpcResult<StoryStatusDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::classify_scenes(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn image_scene(state: State<'_, AppState>, photo_id: String) -> IpcResult<Option<SceneDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::image_scene(&app, &photo_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn merge_chapters(
    state: State<'_, AppState>,
    input: MergeChaptersInput,
) -> IpcResult<ChapterHandleDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::merge_chapters(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn move_chapter_boundary(
    state: State<'_, AppState>,
    input: MoveBoundaryInput,
) -> IpcResult<ChapterHandleDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::move_chapter_boundary(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn scene_profiles(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<Vec<SceneProfileDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::scene_profiles(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn segment_story(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<StoryOutlineDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::segment_story(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn set_chapter(
    state: State<'_, AppState>,
    input: SetChapterInput,
) -> IpcResult<ChapterHandleDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::set_chapter(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn split_chapter(
    state: State<'_, AppState>,
    input: SplitChapterInput,
) -> IpcResult<ChapterHandleDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::split_chapter(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn story_outline(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<StoryOutlineDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::story_outline(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn story_status(state: State<'_, AppState>, project_id: String) -> IpcResult<StoryStatusDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::story_status(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

// PHASE-24. The distraction-cleanup surface. `cleanup_pass` decodes proxies, searches sibling
// frames for a homography and runs an exemplar synthesis per candidate, so it goes off the calling
// thread with the rest. `manual_remove` does the same work for one region a person drew, and
// `cleanup_reason_codes` assembles the panel's legend from the frozen enum and touches nothing.
#[tauri::command]
async fn cleanup_status(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<CleanupStatusDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::cleanup_status(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn image_cleanup(
    state: State<'_, AppState>,
    photo_id: String,
) -> IpcResult<Vec<CleanupProposalDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::image_cleanup(&app, &photo_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn cleanup_blocked(
    state: State<'_, AppState>,
    photo_id: String,
) -> IpcResult<Vec<CleanupBlockedDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::cleanup_blocked(&app, &photo_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn cleanup_disclosures(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<Vec<CleanupDisclosureDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::cleanup_disclosures(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn cleanup_pass(
    state: State<'_, AppState>,
    input: CleanupPassInput,
) -> IpcResult<CleanupPassDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::cleanup_pass(&app, input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn decide_cleanup(state: State<'_, AppState>, input: DecideCleanupInput) -> IpcResult<()> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::decide_cleanup(&app, input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn disable_cleanup(state: State<'_, AppState>, input: DisableCleanupInput) -> IpcResult<()> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::disable_cleanup(&app, input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn manual_remove(
    state: State<'_, AppState>,
    input: ManualRemoveInput,
) -> IpcResult<ManualRemoveDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::manual_remove(&app, input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn cleanup_reason_codes(state: State<'_, AppState>) -> IpcResult<Vec<CleanupReasonDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::cleanup_reason_codes(&app))
        .await
        .map_err(|_| background_request_failed())?
}

// PHASE-26. Eleven commands, all of them off the UI thread. The pass is the one that takes real
// time - it fingerprints every body, pairs two cameras across a whole wedding and solves a bounded
// descent per body - and it runs to completion rather than returning a job id, because a project
// half solved against one reference and half against another has been matched to nothing.
// ADR-0054 section 5.
#[tauri::command]
async fn camera_status(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<CameraStatusDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::camera_status(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn camera_transforms(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<Vec<CameraTransformDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::camera_transforms(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn camera_fingerprints(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<Vec<CameraFingerprintDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::camera_fingerprints(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn camera_reports(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<Vec<CameraReportDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::camera_reports(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn camera_pairs(
    state: State<'_, AppState>,
    project_id: String,
    camera_id: String,
    limit: usize,
) -> IpcResult<Vec<MatchedPairDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        aura_app::camera_pairs(&app, &project_id, &camera_id, limit)
    })
    .await
    .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn camera_shooter_bias(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<Vec<ShooterBiasDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::camera_shooter_bias(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn camera_pass(
    state: State<'_, AppState>,
    input: CameraPassInput,
) -> IpcResult<CameraPassDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::camera_pass(&app, input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn set_camera_reference(
    state: State<'_, AppState>,
    input: SetCameraReferenceInput,
) -> IpcResult<()> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::set_camera_reference(&app, input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn disable_camera(state: State<'_, AppState>, input: DisableCameraInput) -> IpcResult<()> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::disable_camera(&app, input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn set_camera_override(
    state: State<'_, AppState>,
    input: CameraOverrideInput,
) -> IpcResult<()> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::set_camera_override(&app, input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn camera_reason_codes() -> IpcResult<Vec<CameraReasonDto>> {
    Ok(aura_app::camera_reason_codes())
}

// PHASE-27. Nine commands, all of them off the UI thread. `qc_run` is the one that takes real
// time - it inspects every delivered frame against ten checks and, when it is asked to, runs a
// bounded re-edit loop over what it found - and it runs to completion rather than returning a job
// id, because a gallery half inspected under one thresholds table and half under another has been
// checked against nothing. ADR-0056 section 4.
//
// `qc_run` is also the only command on this surface that can change a photograph, and only when
// `remediate` is true. The other eight read, or record what a photographer decided.
#[tauri::command]
async fn qc_status(state: State<'_, AppState>, project_id: String) -> IpcResult<QcStatusDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::qc_status(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn qc_report(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<Option<QcReportDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::qc_report(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn qc_report_markdown(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<Option<String>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::qc_report_markdown(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn qc_queue(
    state: State<'_, AppState>,
    project_id: String,
    category: Option<String>,
    limit: usize,
) -> IpcResult<Vec<QcTicketDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        aura_app::qc_queue(&app, &project_id, category, limit)
    })
    .await
    .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn qc_queue_grouped(
    state: State<'_, AppState>,
    project_id: String,
    limit: usize,
) -> IpcResult<Vec<QcGroupDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::qc_queue_grouped(&app, &project_id, limit))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn qc_tickets(state: State<'_, AppState>, image_id: String) -> IpcResult<Vec<QcTicketDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::qc_tickets(&app, &image_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn qc_rounds(
    state: State<'_, AppState>,
    project_id: String,
    ticket_id: String,
) -> IpcResult<Vec<QcRoundDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        aura_app::qc_rounds(&app, &project_id, &ticket_id)
    })
    .await
    .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn qc_run(state: State<'_, AppState>, input: QcPassInput) -> IpcResult<QcReportDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::qc_run(&app, input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn qc_decide(
    state: State<'_, AppState>,
    project_id: String,
    input: QcDecideInput,
) -> IpcResult<()> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::qc_decide(&app, &project_id, input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn qc_decide_bulk(state: State<'_, AppState>, input: QcDecideBulkInput) -> IpcResult<u32> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::qc_decide_bulk(&app, input))
        .await
        .map_err(|_| background_request_failed())?
}

// PHASE-25. Nine commands, all of them off the UI thread. The pass is the one that takes real time
// - it walks every photograph in a project through three services and solves a tree - and it runs
// to completion rather than returning a job id, because a half-solved tree has no state a reader
// could make sense of. ADR-0052 section 5.
#[tauri::command]
async fn gallery_status(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<GalleryStatusDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::gallery_status(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn gallery_nodes(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<Vec<SceneNodeDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::gallery_nodes(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn gallery_node_strip(
    state: State<'_, AppState>,
    node_id: String,
) -> IpcResult<Vec<GalleryDeltaDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::gallery_node_strip(&app, &node_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn image_gallery(
    state: State<'_, AppState>,
    photo_id: String,
) -> IpcResult<Option<GalleryDeltaDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::image_gallery(&app, &photo_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn gallery_outliers(
    state: State<'_, AppState>,
    project_id: String,
    limit: usize,
) -> IpcResult<Vec<GalleryOutlierDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        aura_app::gallery_outliers(&app, &project_id, limit)
    })
    .await
    .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn gallery_pass(
    state: State<'_, AppState>,
    input: GalleryPassInput,
) -> IpcResult<GalleryPassDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::gallery_pass(&app, input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn pin_gallery_anchor(state: State<'_, AppState>, input: PinAnchorInput) -> IpcResult<()> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::pin_gallery_anchor(&app, input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn set_gallery_override(
    state: State<'_, AppState>,
    input: GalleryOverrideInput,
) -> IpcResult<()> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::set_gallery_override(&app, input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn disable_gallery(state: State<'_, AppState>, input: DisableGalleryInput) -> IpcResult<()> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::disable_gallery(&app, input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn gallery_reason_codes(state: State<'_, AppState>) -> IpcResult<Vec<GalleryReasonDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::gallery_reason_codes(&app))
        .await
        .map_err(|_| background_request_failed())?
}
// ---------------------------------------------------------------------------
// PHASE-28. The zero-touch autopilot.
// ---------------------------------------------------------------------------
//
// Nine commands, all of them off the UI thread. `autopilot_start` is the one that matters: it
// returns as soon as the run is planned and the wedding continues on a worker thread inside
// `aura-app`, because a command that returned when the run finished would hold this surface for
// two hours.
//
// `autopilot_cancel` reaches the run's cancel token, which is polled between units and never
// inside a write - so a stopped run leaves the catalog exactly as consistent as a finished one.

#[tauri::command]
async fn autopilot_status(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<AutopilotStatusDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::autopilot_status(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn autopilot_preflight(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<AutopilotPreflightDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::autopilot_preflight(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn autopilot_start(
    state: State<'_, AppState>,
    input: AutopilotStartInput,
) -> IpcResult<AutopilotProgressDto> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::autopilot_start(&app, &input))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn autopilot_progress(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<Option<AutopilotProgressDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::autopilot_progress(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn autopilot_cancel(state: State<'_, AppState>, project_id: String) -> IpcResult<bool> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::autopilot_cancel(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn autopilot_stages(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<Vec<AutopilotStageDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::autopilot_stages(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn autopilot_summary(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<Option<AutopilotSummaryDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::autopilot_summary(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn autopilot_events(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<Vec<AutopilotEventDto>> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::autopilot_events(&app, &project_id))
        .await
        .map_err(|_| background_request_failed())?
}

#[tauri::command]
async fn autopilot_set_settings(
    state: State<'_, AppState>,
    input: AutopilotSettingsInput,
) -> IpcResult<()> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || aura_app::autopilot_set_settings(&app, &input))
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
            cleanup_status,
            image_cleanup,
            cleanup_blocked,
            cleanup_disclosures,
            cleanup_pass,
            decide_cleanup,
            disable_cleanup,
            manual_remove,
            cleanup_reason_codes,
            gallery_status,
            gallery_nodes,
            gallery_node_strip,
            image_gallery,
            gallery_outliers,
            gallery_pass,
            pin_gallery_anchor,
            set_gallery_override,
            disable_gallery,
            gallery_reason_codes,
            camera_status,
            camera_transforms,
            camera_fingerprints,
            camera_reports,
            camera_pairs,
            camera_shooter_bias,
            camera_pass,
            set_camera_reference,
            disable_camera,
            set_camera_override,
            camera_reason_codes,
            qc_status,
            qc_report,
            qc_report_markdown,
            qc_queue,
            qc_queue_grouped,
            qc_tickets,
            qc_rounds,
            qc_run,
            qc_decide,
            qc_decide_bulk,
            set_framing,
            plan_geometry,
            analyse_integrity,
            build_index,
            cancel_previews,
            check_ai_key,
            classify_scenes,
            clear_ai_key,
            cloud_cache_stats,
            cloud_calls,
            cloud_spend,
            cloud_status,
            develop_status,
            dismiss_flag,
            edit_mask,
            embed_project,
            emotion_status,
            ensure_masks,
            erase_biometrics,
            find_similar,
            flagged_images,
            get_preview,
            group_moments,
            group_people,
            hardware_plan,
            history_step,
            identity_cover,
            image_subjects,
            identity_timelines,
            image_descriptors,
            image_emotion,
            image_history,
            image_integrity,
            image_masks,
            image_recipe,
            image_scene,
            index_status,
            infer_stats,
            integrity_status,
            list_identities,
            list_models,
            list_moments,
            lock_moment,
            mask_allowance,
            mask_kinds,
            mask_overlay,
            mask_status,
            merge_chapters,
            merge_identities,
            merge_moments,
            moment_duplicates,
            moment_of_image,
            moment_peak,
            moment_status,
            // Registered by PHASE-26. It has been defined and unregistered since phase 10,
            // so `within_moment` reached a window that did not answer to it - the same class of
            // defect the phase 20-22 merge found ninety of, in the one place the count had
            // stayed at ninety-nine per cent ever since.
            within_moment,
            move_chapter_boundary,
            people_status,
            prefer_frame,
            prefetch_previews,
            preview_problems,
            preview_stats,
            purge_cache,
            purge_cloud_cache,
            ranked_by_emotion,
            reactions_of,
            recheck_hardware,
            regenerate_mask,
            rename_identity,
            render_caps,
            render_image,
            scan_faces,
            scene_profiles,
            score_emotion,
            segment_story,
            set_ai_key,
            set_cache_budget,
            set_chapter,
            set_cloud_budget,
            set_cloud_privacy,
            set_execution_provider,
            set_identity_importance,
            set_identity_role,
            set_keep_hint,
            set_moment_peak,
            set_param,
            snapshot,
            split_chapter,
            split_identity,
            split_moment,
            story_outline,
            story_status,
            undo_moment_edit,
            warmup_models,
            within_moment
                    autopilot_status,
            autopilot_preflight,
            autopilot_start,
            autopilot_progress,
            autopilot_cancel,
            autopilot_stages,
            autopilot_summary,
            autopilot_events,
            autopilot_set_settings,
        ])
        .run(tauri::generate_context!());

    if let Err(err) = result {
        tracing::error!(target: "shell", error = %err, "the desktop shell stopped");
    }
}
