//! Selection-to-delivery background workflow. See ADR-0066.
use crate::contract::ipc::*;
use crate::AppState;
use aura_core::progress::CancelToken;
use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

// This bounds provider calls, never the number of photos processed or delivered.
const MAX_AI_EDITS: usize = 600;
static REQUESTS: Mutex<()> = Mutex::new(());
static JOBS: Mutex<BTreeMap<String, JobSlot>> = Mutex::new(BTreeMap::new());
struct JobSlot {
    worker_active: bool,
    status: OneClickStatusDto,
    cancel: CancelToken,
    state: AppState,
    project: String,
    child: Option<String>,
}
fn error(message: &str) -> IpcError {
    let mut error: IpcError =
        aura_core::errors::render::recipe_invalid("automatic workflow", message).into();
    error.message = message.to_owned();
    error
}
fn update(job: &str, change: impl FnOnce(&mut OneClickStatusDto)) {
    if let Some(slot) = JOBS.lock().get_mut(job) {
        change(&mut slot.status);
    }
}
fn phase(job: &str, name: &str, label: &str, total: u64) {
    update(job, |s| {
        s.phase = name.into();
        s.phase_label = label.into();
        s.items_done = 0;
        s.items_total = total;
    });
}
fn note(job: &str, message: String) {
    update(job, |s| {
        if !s.notes.contains(&message) {
            s.notes.push(message);
        }
    });
}
fn stopped(job: &str) -> bool {
    JOBS.lock()
        .get(job)
        .map_or(true, |s| s.cancel.is_cancelled())
}
fn child(job: &str, id: Option<String>) {
    if let Some(slot) = JOBS.lock().get_mut(job) {
        if slot.cancel.is_cancelled() {
            if let Some(id) = &id {
                let _ = slot.state.cancel_job(id);
            }
        }
        slot.child = id;
    }
}
fn require_idle() -> Result<(), IpcError> {
    if JOBS.lock().values().any(|s| s.worker_active) {
        return Err(error(
            "Another automatic run is active. Stop it or wait for delivery.",
        ));
    }
    Ok(())
}
fn output_folder(project: &str) -> Result<PathBuf, IpcError> {
    let pictures = dirs::picture_dir().unwrap_or(aura_core::paths::AppPaths::resolve()?.data_dir);
    Ok(pictures
        .join("AURA Exports")
        .join(format!("{project}-{}", uuid::Uuid::new_v4())))
}

/// Selecting files is the only required interaction. Existing manual edits remain protected.
pub fn automatic_start(
    state: &AppState,
    input: AutomaticStartInput,
) -> Result<AutomaticStartDto, IpcError> {
    let _request = REQUESTS.lock();
    require_idle()?;
    if input.roots.is_empty() {
        return Err(error("Select photographs or a folder first."));
    }
    for root in &input.roots {
        if !Path::new(root).is_absolute() || !Path::new(root).exists() {
            return Err(error("Every selected path must exist and be absolute."));
        }
    }
    let project = match input.project_id {
        Some(id) => id,
        None => {
            let path = Path::new(input.roots.first().ok_or_else(|| error("No selection"))?);
            let name = if path.is_dir() {
                path.file_name()
            } else {
                path.parent().and_then(Path::file_name)
            }
            .and_then(|s| s.to_str())
            .unwrap_or("Imported photos")
            .to_string();
            crate::create_project(
                state,
                CreateProjectInput {
                    name,
                    couple_names: None,
                    event_date: None,
                },
            )?
            .id
        }
    };
    let destination = output_folder(&project)?.to_string_lossy().into_owned();
    let ingest = crate::start_ingest(
        state,
        &StartIngestInput {
            project_id: project.clone(),
            roots: input.roots,
        },
    )?;
    let result = start(
        state,
        OneClickFinishInput {
            project_id: project.clone(),
            destination: destination.clone(),
            ingest_job_id: Some(ingest.job_id.clone()),
        },
    );
    match result {
        Ok(handle) => Ok(AutomaticStartDto {
            project_id: project,
            job_id: handle.job_id,
            ingest_job_id: ingest.job_id,
            destination,
        }),
        Err(err) => {
            let _ = state.cancel_job(&ingest.job_id);
            Err(err)
        }
    }
}

/// Start/re-run an existing project; an empty destination requests a unique default folder.
pub fn one_click_finish(
    state: &AppState,
    input: OneClickFinishInput,
) -> Result<OneClickFinishDto, IpcError> {
    let _request = REQUESTS.lock();
    require_idle()?;
    start(state, input)
}
fn start(state: &AppState, mut input: OneClickFinishInput) -> Result<OneClickFinishDto, IpcError> {
    aura_core::ProjectId::from_db(&input.project_id).map_err(|_| error("Invalid project id."))?;
    let exists = crate::list_projects(state)?
        .iter()
        .any(|p| p.id == input.project_id);
    if !exists {
        return Err(error("This project no longer exists."));
    }
    if input.destination.trim().is_empty() {
        input.destination = output_folder(&input.project_id)?
            .to_string_lossy()
            .into_owned();
    }
    let destination = Path::new(&input.destination);
    if !destination.is_absolute() {
        return Err(error("The export folder must be an absolute path."));
    }
    std::fs::create_dir_all(destination)
        .map_err(|e| aura_core::errors::io::from_io(&e, destination))?;
    let job = format!("oneclick-{}", uuid::Uuid::new_v4());
    let cancel = CancelToken::new();
    state.register_job(&job, cancel.clone());
    let status = OneClickStatusDto {
        job_id: job.clone(),
        status: "running".into(),
        phase: "queue".into(),
        phase_label: "Preparing your photographs.".into(),
        items_done: 0,
        items_total: 0,
        frames: 0,
        ai_edited: 0,
        local_edited: 0,
        analyzed: 0,
        failed_edits: 0,
        selected: 0,
        written: 0,
        verified: 0,
        destination: input.destination.clone(),
        model: String::new(),
        notes: vec![],
    };
    JOBS.lock().insert(
        job.clone(),
        JobSlot {
            worker_active: true,
            status,
            cancel,
            state: state.clone(),
            project: input.project_id.clone(),
            child: input.ingest_job_id.clone(),
        },
    );
    let worker = state.clone();
    let worker_job = job.clone();
    if let Err(e) = std::thread::Builder::new()
        .name("aura-automatic".into())
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                stages(&worker, &worker_job, &input)
            }))
            .unwrap_or_else(|_| {
                Err(error(
                    "The automatic worker stopped unexpectedly. Completed files are preserved.",
                ))
            });
            let mut final_status = one_click_status(&worker_job).expect("registered worker status");
            {
                let s = &mut final_status;
                if s.status == "cancelling" {
                    s.status = "cancelled".into();
                    s.phase_label = "Stopped. Completed files and edits were preserved.".into();
                } else if s.status == "running" {
                    match result {
                        Ok(()) => {
                            s.status = if s.failed_edits > 0 {
                                "completed_with_issues"
                            } else {
                                "completed"
                            }
                            .into();
                            s.phase = "done".into();
                            s.phase_label =
                                "Export finished. See the counts and run report.".into();
                        }
                        Err(err) => {
                            s.status = "failed".into();
                            s.phase_label = err.message.clone();
                            s.notes.push(format!("{}: {}", err.code, err.message));
                        }
                    }
                }
            }
            worker.finish_job(&worker_job);
            child(&worker_job, None);
            if let Ok(bytes) = serde_json::to_vec_pretty(&final_status) {
                if let Err(err) = std::fs::write(
                    Path::new(&final_status.destination).join("aura-run.json"),
                    bytes,
                ) {
                    final_status.status = "failed".into();
                    final_status
                        .notes
                        .push(format!("Could not save run report: {err}"));
                }
            }
            if let Some(slot) = JOBS.lock().get_mut(&worker_job) {
                slot.status = final_status;
                slot.worker_active = false;
            }
        })
    {
        JOBS.lock().remove(&job);
        state.finish_job(&job);
        return Err(error(&format!("Could not start the automatic worker: {e}")));
    }
    Ok(OneClickFinishDto { job_id: job })
}

/// Current native progress survives changing tabs or reloading the frontend.
pub fn one_click_status(job_id: &str) -> Result<OneClickStatusDto, IpcError> {
    JOBS.lock().get(job_id).map(|s| s.status.clone()).ok_or_else(|| error("This run is no longer active in this application process. Check its output folder for aura-run.json."))
}
/// Cancel the pipeline and its active import/model/analysis child.
#[must_use]
pub fn one_click_cancel(job_id: &str) -> bool {
    let active = JOBS
        .lock()
        .get_mut(job_id)
        .filter(|s| s.status.status == "running")
        .map(|s| {
            s.status.status = "cancelling".into();
            s.status.phase_label =
                "Stopping after the active operation finishes. Keep AURA open.".into();
            (
                s.cancel.clone(),
                s.state.clone(),
                s.project.clone(),
                s.child.clone(),
            )
        });
    let Some((token, state, project, child)) = active else {
        return false;
    };
    token.cancel();
    if let Some(id) = child {
        let _ = state.cancel_job(&id);
    }
    let _ = crate::autopilot_cancel(&state, &project);
    true
}

fn stages(state: &AppState, job: &str, input: &OneClickFinishInput) -> Result<(), IpcError> {
    let project = &input.project_id;
    if let Some(id) = &input.ingest_job_id {
        phase(job, "ingest", "Importing the selected photographs.", 0);
        let deadline = state
            .clock()
            .monotonic_ms()
            .saturating_add(24 * 3600 * 1000);
        loop {
            if stopped(job) {
                let _ = state.cancel_job(id);
                return Ok(());
            }
            let p = crate::ingest_progress(state, id)?;
            if !p.known {
                return Err(error(
                    "The import job disappeared; automatic processing stopped.",
                ));
            }
            update(job, |s| {
                s.items_done = p.done;
                s.items_total = p.total;
            });
            if !p.running {
                break;
            }
            if state.clock().monotonic_ms() > deadline {
                let _ = state.cancel_job(id);
                return Err(error(
                    "Import timed out; no partial gallery was marked complete.",
                ));
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        child(job, None);
    }
    let mut photos = Vec::new();
    let mut offset = 0;
    loop {
        if stopped(job) {
            return Ok(());
        }
        let page = crate::list_images(
            state,
            &ListImagesInput {
                project_id: project.clone(),
                offset,
                limit: 250,
                order_by: Some("timeline".into()),
            },
        )?;
        let count = page.len();
        photos.extend(page);
        offset += count as i64;
        if count < 250 {
            break;
        }
    }
    if photos.is_empty() {
        return Err(error("No readable photographs were imported. Check the selected file formats and Import problems."));
    }
    update(job, |s| s.frames = photos.len() as u64);
    let problems = crate::list_problems(state, project)?;
    if !problems.is_empty() {
        note(job, format!("Import reported {} problems; check the Problems tab for skipped or unreadable files.", problems.len()));
    }

    phase(
        job,
        "analyze",
        "Measuring lighting, clipping, color and white balance for each photo.",
        photos.len() as u64,
    );
    let mut readable = Vec::new();
    let mut analysis = Vec::new();
    for (index, photo) in photos.iter().enumerate() {
        if stopped(job) {
            return Ok(());
        }
        match crate::photo_analysis(
            state,
            &PhotoAutoEditInput {
                project_id: project.clone(),
                photo_id: photo.id.clone(),
                job_id: job.into(),
            },
        ) {
            Ok(row) => {
                readable.push(photo.id.clone());
                analysis.push(serde_json::to_value(row).map_err(|e| error(&e.to_string()))?);
                update(job, |s| s.analyzed += 1);
            }
            Err(err) => {
                update(job, |s| s.failed_edits += 1);
                note(
                    job,
                    format!("{} could not be analyzed: {}", photo.id, err.message),
                );
            }
        }
        update(job, |s| s.items_done = (index + 1) as u64);
    }
    let report = Path::new(&input.destination).join("photo-analysis.json");
    std::fs::write(
        &report,
        serde_json::to_vec_pretty(&analysis).map_err(|e| error(&e.to_string()))?,
    )
    .map_err(|e| aura_core::errors::io::from_io(&e, &report))?;
    if readable.is_empty() {
        return Err(error("None of the imported photographs could be decoded."));
    }

    phase(job, "cloud", "Checking the configured vision provider.", 0);
    let policy = state.cloud_policy();
    let checked = if !policy.project_enabled || policy.offline_studio_mode || policy.blur_faces {
        None
    } else {
        crate::check_ai_key(state).ok()
    };
    let use_cloud = checked.as_ref().is_some_and(|c| c.ok);
    if use_cloud {
        update(job, |s| {
            s.model = checked
                .as_ref()
                .map(|c| c.model.clone())
                .unwrap_or_default()
        });
    } else {
        update(job, |s| s.model = "local reference".into());
        note(job, "No working provider or cloud disabled by privacy settings; using measured local reference edits. Configure a vision model in AI provider for semantic photo-aware choices.".into());
    }

    // Optional learned analysis is never presented as successful pixel/face recognition
    // when the bundled model preflight refuses it. Every photo still receives a real edit.
    let mut advanced = false;
    match crate::autopilot_start(state, &AutopilotStartInput { project_id: project.clone(), disabled: vec![], zero_touch: true, allow_on_battery: false, quiet_mode: false }) {
        Ok(_) => {
            phase(job, "analyze", "Running available advanced analysis models.", photos.len() as u64);
            let deadline = state.clock().monotonic_ms().saturating_add(6 * 3600 * 1000);
            loop {
                if stopped(job) { let _ = crate::autopilot_cancel(state, project); return Ok(()); }
                match crate::autopilot_progress(state, project)? {
                    Some(p) if p.status == "running" => update(job, |s| s.phase_label = p.stage_title),
                    Some(p) => { advanced = p.status == "completed"; break; }
                    None => break,
                }
                if state.clock().monotonic_ms() > deadline { let _ = crate::autopilot_cancel(state, project); return Err(error("Advanced analysis timed out and was stopped.")); }
                std::thread::sleep(Duration::from_millis(500));
            }
        }
        Err(err) => note(job, format!("Optional learned analysis unavailable: {}. Pixel-based analysis completed; automatic face/subject claims are not made.", err.message)),
    }
    if stopped(job) {
        return Ok(());
    }
    if advanced {
        phase(job, "geometry", "Applying validated framing proposals.", 0);
        if crate::plan_geometry(
            state,
            &PlanGeometryInput {
                project_id: project.clone(),
                limit: None,
            },
        )
        .is_ok()
        {
            // One pass over the queue, with no fixed 400-photo truncation.
            let queue = crate::geometry_review_queue(
                state,
                &GeometryReviewInput {
                    project_id: project.clone(),
                    limit: Some(u32::MAX),
                },
            )?;
            for photo in queue {
                if stopped(job) {
                    return Ok(());
                }
                if let Err(err) =
                    crate::accept_geometry(state, &AcceptGeometryInput { photo_id: photo })
                {
                    note(job, format!("Framing left unchanged: {}", err.message));
                }
            }
        }
    } else {
        note(
            job,
            "Preserved original framing because validated subject-aware analysis was unavailable."
                .into(),
        );
    }
    phase(
        job,
        "cull",
        "Preparing the delivery selection.",
        readable.len() as u64,
    );
    let mut targets = readable.clone();
    if advanced {
        match crate::cull_project(
            state,
            &CullProjectInput {
                project_id: project.clone(),
                mode: Some("balanced".into()),
                target: None,
                cancel_id: None,
            },
        ) {
            Ok(_) => {
                if let Some(gallery) = crate::gallery(state, project)? {
                    let kept: std::collections::BTreeSet<_> =
                        gallery.selected.into_iter().map(|p| p.photo_id).collect();
                    let selection: Vec<_> = readable
                        .iter()
                        .filter(|id| kept.contains(*id))
                        .cloned()
                        .collect();
                    if !selection.is_empty() {
                        targets = selection;
                    }
                }
            }
            Err(err) => note(
                job,
                format!(
                    "Cull unavailable; keeping every readable photograph: {}",
                    err.message
                ),
            ),
        }
    } else {
        note(job, "Every readable photograph is retained; no images were rejected without reliable learned analysis.".into());
    }
    update(job, |s| s.selected = targets.len() as u64);
    phase(
        job,
        "edit",
        "Choosing and applying an individual edit to every selected photo.",
        targets.len() as u64,
    );
    let mut edited = Vec::new();
    for (index, photo) in targets.iter().enumerate() {
        if stopped(job) {
            return Ok(());
        }
        let id = format!("{job}-{photo}");
        child(job, Some(id.clone()));
        let request = PhotoAutoEditInput {
            project_id: project.clone(),
            photo_id: photo.clone(),
            job_id: id,
        };
        let result = if use_cloud && index < MAX_AI_EDITS {
            crate::photo_auto_edit(state, &request)
        } else {
            crate::auto_edit_commands::photo_auto_edit_local(state, &request)
        };
        child(job, None);
        if stopped(job) {
            return Ok(());
        }
        match result {
            Ok(answer) => {
                update(job, |s| {
                    if answer.source == "cloud" || answer.source == "cache" {
                        s.ai_edited += 1;
                    } else {
                        s.local_edited += 1;
                    }
                });
                edited.push(serde_json::json!({"photoId":photo,"source":answer.source,"model":answer.model,"reasons":answer.reasons,"recipe":answer.recipe}));
            }
            Err(err) => {
                update(job, |s| s.failed_edits += 1);
                note(
                    job,
                    format!(
                        "{} edit failed; exporting its previous reversible recipe: {}",
                        photo, err.message
                    ),
                );
            }
        }
        update(job, |s| s.items_done = (index + 1) as u64);
    }
    if use_cloud && targets.len() > MAX_AI_EDITS {
        note(job, format!("Provider calls capped at {MAX_AI_EDITS}; ALL remaining photos received local adaptive edits and remain in the export."));
    }
    let report = Path::new(&input.destination).join("photo-edits.json");
    std::fs::write(
        &report,
        serde_json::to_vec_pretty(&edited).map_err(|e| error(&e.to_string()))?,
    )
    .map_err(|e| aura_core::errors::io::from_io(&e, &report))?;
    if stopped(job) {
        return Ok(());
    }
    phase(
        job,
        "export",
        "Exporting and reading every file back for verification.",
        targets.len() as u64,
    );
    let presets = crate::export_presets()?;
    let preset = presets
        .iter()
        .find(|p| p.name == "gallery")
        .or_else(|| presets.first())
        .ok_or_else(|| error("No export preset is installed."))?;
    let report = crate::export_run(
        state,
        ExportJobInput {
            project_id: project.clone(),
            sets: vec![ExportSetInput {
                name: preset.name.clone(),
                image_ids: targets.clone(),
                format: preset.format.clone(),
                quality: preset.quality,
                colour: preset.colour.clone(),
                bit_depth: preset.bit_depth,
                resize: preset.resize.clone(),
                sharpen: preset.sharpen.clone(),
                naming: preset.naming.clone(),
                sidecar: preset.sidecar,
            }],
            destination: input.destination.clone(),
            destination_kind: "folder".into(),
            copyright: None,
            contact: None,
            creator: None,
            keywords: vec![],
            strip_gps: true,
            strip_camera_serial: true,
            verify: true,
        },
    )?;
    update(job, |s| {
        s.written = u64::from(report.written);
        s.verified = u64::from(report.verified);
        s.items_done = s.verified;
    });
    if report.corrupt > 0
        || report.render_failed > 0
        || report.verified != report.written
        || (report.written as usize) < targets.len()
    {
        return Err(error("Export is incomplete or failed verification. Completed files are preserved; inspect the run report."));
    }
    Ok(())
}
