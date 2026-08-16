// The desktop shell. It owns the window, the single-instance guard, the panic
// hook and logging, and forwards every command to `aura-app`. No product logic
// lives here: the shell must stay thin enough to replace.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use aura_app::contract::ipc::{
    AnalyseCompositionInput, CompositionDto, CompositionPassDto, CompositionStatusDto,
    CreateProjectInput, DismissCompositionFlagInput, FlaggedCompositionInput, ImageRowLite,
    IpcError, JobHandle, ListImagesInput, ProblemRow, ProjectHandle, ProjectSummary,
    SetCameraLabelInput, StartIngestInput,
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
            analyse_composition
        ])
        .run(tauri::generate_context!());

    if let Err(err) = result {
        tracing::error!(target: "shell", error = %err, "the desktop shell stopped");
    }
}
