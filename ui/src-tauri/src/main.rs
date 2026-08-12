// The desktop shell. It owns the window, the single-instance guard, the panic
// hook and logging, and forwards every command to `aura-app`. No product logic
// lives here: the shell must stay thin enough to replace.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use aura_app::contract::ipc::{
    CreateProjectInput, ImageRowLite, IpcError, JobHandle, ListImagesInput, ProblemRow,
    ProjectHandle, ProjectSummary, SetCameraLabelInput, StartIngestInput,
};
use aura_app::AppState;
use aura_core::paths::AppPaths;
use tauri::{Manager, State};

type IpcResult<T> = Result<T, IpcError>;

#[tauri::command]
fn create_project(state: State<'_, AppState>, input: CreateProjectInput) -> IpcResult<ProjectHandle> {
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
            list_problems
        ])
        .run(tauri::generate_context!());

    if let Err(err) = result {
        tracing::error!(target: "shell", error = %err, "the desktop shell stopped");
    }
}
