//! The command surface. Every function here is short, typed, and returns an
//! [`IpcError`] the UI can render without translation.

use std::path::PathBuf;

use aura_catalog::model::ProjectRow;
use aura_catalog::repo::PhotoOrder;
use aura_catalog::{repo, rfc3339};
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ImportId, ProjectId};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};

use crate::contract::ipc::{
    CreateProjectInput, ImageRowLite, JobHandle, ListImagesInput, ProblemRow, ProjectHandle,
    ProjectSummary, SetCameraLabelInput, StartIngestInput,
};
use crate::state::AppState;

/// Result type every command returns.
pub type IpcResult<T> = Result<T, crate::contract::ipc::IpcError>;

/// Create a wedding project with consent denied by default.
///
/// # Errors
///
/// Returns the catalog error, already translated for the UI.
pub fn create_project(state: &AppState, input: CreateProjectInput) -> IpcResult<ProjectHandle> {
    let project_id = ProjectId::new();
    let now = rfc3339(state.clock().now_utc());
    let row = ProjectRow {
        project_id: project_id.to_db(),
        name: input.name,
        couple_label: input.couple_names,
        event_date: input.event_date,
        timezone: "UTC".to_string(),
        status: "active".to_string(),
        created_at: now.clone(),
        updated_at: now,
    };

    state
        .catalog()
        .writer()
        .transact(move |tx| repo::create_project(tx, &row))?;

    Ok(ProjectHandle {
        id: project_id.to_db(),
    })
}

/// Every project, newest event first, with its photo count.
///
/// # Errors
///
/// Returns the catalog error, already translated for the UI.
pub fn list_projects(state: &AppState) -> IpcResult<Vec<ProjectSummary>> {
    let rows = state.catalog().read(repo::list_projects)?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let photo_count = state
            .catalog()
            .read(|conn| repo::count_photos(conn, &row.project_id))?;
        out.push(ProjectSummary {
            id: row.project_id,
            name: row.name,
            event_date: row.event_date,
            photo_count,
        });
    }
    Ok(out)
}

/// Start an import and return immediately with a job handle.
///
/// The command itself does no IO beyond validating the request, so it stays well
/// inside the 50 ms budget; the work runs on a background thread.
///
/// # Errors
///
/// Returns `AURA-IO-1001` when a root does not exist, or the catalog error.
pub fn start_ingest(state: &AppState, input: &StartIngestInput) -> IpcResult<JobHandle> {
    let project_id = ProjectId::from_db(&input.project_id).map_err(|_| {
        crate::contract::ipc::IpcError::from(aura_core::errors::db::statement_failed(
            "project id was not a valid AURA id",
            &std::io::Error::from(std::io::ErrorKind::InvalidInput),
        ))
    })?;

    let roots: Vec<PathBuf> = input.roots.iter().map(PathBuf::from).collect();
    for root in &roots {
        if !root.exists() {
            return Err(aura_core::errors::io::not_found(root).into());
        }
    }

    let import_id = ImportId::new();
    let job_id = import_id.to_db();
    let cancel = CancelToken::new();
    state.register_job(&job_id, cancel.clone());

    let plan = ImportPlan {
        import_id,
        project_id,
        roots,
        mode: ImportMode::Reference,
        extensions: Vec::new(),
        extract_embedded_previews: false,
        settle_window_ms: 2_000,
    };

    let worker_state = state.clone();
    let worker_job_id = job_id.clone();
    std::thread::Builder::new()
        .name("aura-ingest".into())
        .spawn(move || {
            let outcome = aura_ingest::run(worker_state.catalog(), &plan, &cancel, &NullProgress);
            match outcome {
                Ok(report) => tracing::info!(
                    target: "ingest",
                    imported = report.files_imported,
                    duplicates = report.files_already_present,
                    quarantined = report.files_quarantined,
                    elapsed_ms = report.duration_ms,
                    "import finished"
                ),
                Err(err) => tracing::error!(
                    target: "ingest",
                    code = err.code.0,
                    detail = err.detail,
                    "import failed"
                ),
            }
            worker_state.finish_job(&worker_job_id);
        })
        .map_err(|e| {
            crate::contract::ipc::IpcError::from(aura_core::errors::db::statement_failed(
                "could not start the import thread",
                &e,
            ))
        })?;

    Ok(JobHandle { job_id })
}

/// Ask a running import to stop. Cancellation is cooperative and typed.
///
/// # Errors
///
/// Never fails: an unknown job id means the run already finished.
pub fn cancel_job(state: &AppState, job_id: &str) -> IpcResult<bool> {
    Ok(state.cancel_job(job_id))
}

/// One page of the grid.
///
/// # Errors
///
/// Returns the catalog error, already translated for the UI.
pub fn list_images(state: &AppState, input: &ListImagesInput) -> IpcResult<Vec<ImageRowLite>> {
    let order = match input.order_by.as_deref() {
        Some("filename") => PhotoOrder::FileName,
        _ => PhotoOrder::Timeline,
    };
    let rows = state.catalog().read(|conn| {
        repo::list_photos(conn, &input.project_id, input.offset, input.limit, order)
    })?;

    Ok(rows
        .into_iter()
        .map(|row| ImageRowLite {
            id: row.id,
            file_name: row.file_name,
            timeline_ts: row.timeline_ts,
            camera_id: row.camera_id,
            width: row.width,
            height: row.height,
            status: row.status,
        })
        .collect())
}

/// Rename a body or nudge its clock, then re-order the timeline.
///
/// # Errors
///
/// Returns the catalog error, already translated for the UI.
pub fn set_camera_label(state: &AppState, input: &SetCameraLabelInput) -> IpcResult<()> {
    let now = rfc3339(state.clock().now_utc());
    let camera_id = input.camera_id.clone();

    let project_id = state.catalog().read(|conn| {
        conn.query_row(
            "SELECT project_id FROM camera WHERE camera_id = ?1",
            rusqlite::params![camera_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(|e| aura_core::errors::db::statement_failed("read camera project", &e))
    })?;

    let shooter_label = input.shooter_label.clone();
    let clock_offset_ms = input.clock_offset_ms;

    state.catalog().writer().transact(move |tx| {
        tx.execute(
            "UPDATE camera SET shooter_label = ?2, updated_at = ?3 WHERE camera_id = ?1",
            rusqlite::params![camera_id, shooter_label, now],
        )
        .map_err(|e| aura_core::errors::db::statement_failed("update shooter label", &e))?;

        // A manual nudge is certain by definition: the photographer compared two
        // frames and said so.
        repo::set_camera_offset(tx, &camera_id, clock_offset_ms, 1.0, "manual", &now)?;
        repo::recompute_timeline(tx, &project_id, &now).map(|_| ())
    })?;

    Ok(())
}

/// The Problems list: every file set aside, with a plain-language reason.
///
/// # Errors
///
/// Returns the catalog error, already translated for the UI.
pub fn list_problems(state: &AppState, project_id: &str) -> IpcResult<Vec<ProblemRow>> {
    let rows = state
        .catalog()
        .read(|conn| repo::list_quarantine(conn, project_id))?;
    Ok(rows
        .into_iter()
        .map(|(path, code, detail)| ProblemRow {
            path,
            code,
            message: detail,
        })
        .collect())
}
