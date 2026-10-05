//! Selection-to-delivery background workflow. See ADR-0069, and ADR-0088 for the measured
//! cull, the look carried from the start screen and the per-photo edit the studio shares.
use crate::contract::ipc::{
    AcceptGeometryInput, AutomaticLookInput, AutomaticStartDto, AutomaticStartInput,
    AutopilotStartInput, CreateProjectInput, CullProjectInput, DevelopImageInput, ExportJobInput,
    ExportSetInput, GeometryReviewInput, ImageRowLite, IpcError, ListImagesInput,
    OneClickFinishDto, OneClickFinishInput, OneClickStatusDto, PhotoAutoEditInput,
    PlanGeometryInput, RecipeDto, StartIngestInput,
};
use crate::AppState;
use aura_core::progress::CancelToken;
use parking_lot::Mutex;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
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
/// One entry of the run's `photo-edits.json`.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct EditedPhoto {
    photo_id: String,
    source: String,
    model: String,
    reasons: Vec<String>,
    recipe: RecipeDto,
}

/// `photo-edits.json`, written as the run goes. A wedding is two thousand recipes with a
/// few hundred retouch operations each, and holding them all to write one array at the end
/// is how a long run dies of memory on the laptop it was meant for.
struct EditLog {
    file: Option<std::io::BufWriter<std::fs::File>>,
    path: PathBuf,
    entries: usize,
}

impl EditLog {
    fn create(path: PathBuf) -> Result<Self, IpcError> {
        let mut file = std::io::BufWriter::new(
            std::fs::File::create(&path).map_err(|e| aura_core::errors::io::from_io(&e, &path))?,
        );
        file.write_all(b"[")
            .map_err(|e| aura_core::errors::io::from_io(&e, &path))?;
        Ok(Self {
            file: Some(file),
            path,
            entries: 0,
        })
    }

    fn push(&mut self, entry: &EditedPhoto) -> Result<(), IpcError> {
        let Some(file) = self.file.as_mut() else {
            return Ok(());
        };
        let body = serde_json::to_vec(entry).map_err(|e| error(&e.to_string()))?;
        let separator: &[u8] = if self.entries == 0 { b"\n" } else { b",\n" };
        file.write_all(separator)
            .and_then(|()| file.write_all(&body))
            .map_err(|e| aura_core::errors::io::from_io(&e, &self.path))?;
        self.entries += 1;
        Ok(())
    }

    /// Close the array. A stopped or failed run still leaves a file that parses.
    fn close(&mut self) -> Result<(), IpcError> {
        let Some(mut file) = self.file.take() else {
            return Ok(());
        };
        file.write_all(b"\n]\n")
            .and_then(|()| file.flush())
            .map_err(|e| aura_core::errors::io::from_io(&e, &self.path).into())
    }
}

impl Drop for EditLog {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

/// "About 1 h 20 min left", from the photographs already finished in this phase.
fn remaining(started_ms: u64, now_ms: u64, done: usize, total: usize) -> String {
    if done == 0 || total <= done {
        return String::new();
    }
    let left = now_ms.saturating_sub(started_ms) / done as u64 * (total - done) as u64 / 1000;
    match left {
        0..=89 => " About a minute left.".into(),
        90..=3599 => format!(" About {} min left.", left.div_ceil(60)),
        _ => format!(" About {} h {} min left.", left / 3600, left % 3600 / 60),
    }
}

/// Whether to run the learned analysis pass. Off unless `AURA_LEARNED_ANALYSIS=1`: see the
/// note where it is read.
fn learned_analysis_enabled() -> bool {
    std::env::var_os("AURA_LEARNED_ANALYSIS").is_some_and(|value| value == "1")
}

/// The cull that needs no learned model: focus, motion, exposure and bursts, measured from
/// each photograph. `None` means the run was stopped. Nothing is deleted, and a frame that
/// cannot be measured is delivered.
fn measured_cull(
    state: &AppState,
    job: &str,
    project: &str,
    photos: &[ImageRowLite],
    readable: &[String],
    destination: &Path,
) -> Result<Option<Vec<String>>, IpcError> {
    phase(
        job,
        "cull",
        "Culling: measuring focus, motion, exposure and bursts in each photo.",
        readable.len() as u64,
    );
    let times = crate::measured_cull::camera_times(state, project).unwrap_or_default();
    let wanted: BTreeSet<&String> = readable.iter().collect();
    let mut frames = Vec::with_capacity(readable.len());
    for photo in photos.iter().filter(|photo| wanted.contains(&photo.id)) {
        if stopped(job) {
            return Ok(None);
        }
        frames.push(
            crate::measured_cull::measure_photo(state, project, photo, &times).unwrap_or_else(
                |_| crate::measured_cull::Frame {
                    photo_id: photo.id.clone(),
                    file_name: photo.file_name.clone(),
                    measure: None,
                    time_ms: None,
                    edited_by_hand: false,
                },
            ),
        );
        let done = frames.len() as u64;
        update(job, |s| s.items_done = done);
    }
    let report = crate::measured_cull::decide(&frames);
    let path = destination.join("photo-cull.json");
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&report).map_err(|e| error(&e.to_string()))?,
    )
    .map_err(|e| aura_core::errors::io::from_io(&e, &path))?;
    note(job, report.summary());
    let kept = report.kept();
    Ok(Some(if kept.is_empty() {
        readable.to_vec()
    } else {
        kept
    }))
}

/// One photograph's edit and retouch - the same three paths the studio's own batch takes,
/// so a wedding finished unattended and a photo edited by hand in the studio agree.
fn edit_one(
    state: &AppState,
    job: &str,
    project: &str,
    photo: &str,
    look: Option<&AutomaticLookInput>,
    cloud: bool,
) -> Result<EditedPhoto, IpcError> {
    let develop = DevelopImageInput {
        photo_id: photo.to_owned(),
    };
    if let Some(look) = look {
        let mut reasons = Vec::new();
        let profile = look.profile_id.clone().filter(|id| !id.is_empty());
        let strength = f32::from(look.profile_strength.min(150)) / 100.0;
        // A profile already contains the measured correction, so it replaces the plain one.
        let mut recipe = if let Some(id) = &profile {
            let report = crate::edit_profiles::apply_edit_profile(
                state,
                &crate::edit_profiles::ApplyProfileInput {
                    photo_id: photo.to_owned(),
                    profile_id: id.clone(),
                    strength,
                },
            )?;
            reasons.push(format!(
                "Look {id} at {}% over this photo's own measured correction.",
                look.profile_strength.min(150)
            ));
            reasons.extend(report.adaptations);
            crate::photo_enhance::enhance_portrait(state, &develop)?
        } else {
            crate::photo_enhance::enhance_photo(state, &develop)?
        };
        if let Some(reference) = look.reference_id.clone().filter(|id| !id.is_empty()) {
            let fitted = crate::reference_style::apply_reference(
                state,
                &crate::reference_style::ApplyReferenceInput {
                    photo_id: photo.to_owned(),
                    reference_id: reference,
                    strength: f32::from(look.reference_strength.min(100)) / 100.0,
                    profile_id: profile.clone(),
                    profile_strength: profile.as_ref().map(|_| strength),
                },
            )?;
            reasons.push(format!(
                "Reference look fitted at {}%: distance to the reference {:.3} before, {:.3} after.",
                look.reference_strength.min(100),
                fitted.before_distance,
                fitted.after_distance
            ));
            recipe = crate::image_recipe(state, &develop)?;
        }
        reasons.push("Portrait retouch evaluated with this photo's saved preferences.".into());
        return Ok(EditedPhoto {
            photo_id: photo.to_owned(),
            source: "local".into(),
            model: "AURA measured edit with your look".into(),
            reasons,
            recipe,
        });
    }
    if cloud {
        let id = format!("{job}-{photo}");
        child(job, Some(id.clone()));
        let answer = crate::photo_auto_edit(
            state,
            &PhotoAutoEditInput {
                project_id: project.to_owned(),
                photo_id: photo.to_owned(),
                job_id: id,
            },
        );
        child(job, None);
        let mut answer = answer?;
        // Run the portrait pass AFTER grading so its luminance selections use the exposure
        // being exported. It reads this photo's saved choices and protects manual work.
        answer.recipe = crate::photo_enhance::enhance_portrait(state, &develop)?;
        answer.reasons.push("Local portrait retouch evaluated with this photo's saved preferences. Applied and skipped corrections are recorded in its portrait report.".into());
        return Ok(EditedPhoto {
            photo_id: photo.to_owned(),
            source: answer.source,
            model: answer.model,
            reasons: answer.reasons,
            recipe: answer.recipe,
        });
    }
    // The measured edit: light, colour, white balance and scene from this photograph's own
    // pixels, then skin, blemishes, eyes and teeth for each detected person.
    let recipe = crate::photo_enhance::enhance_photo(state, &develop)?;
    Ok(EditedPhoto {
        photo_id: photo.to_owned(),
        source: "local".into(),
        model: "AURA measured edit".into(),
        reasons: vec!["Light, colour, white balance and scene measured from this photograph; portrait retouch evaluated with its saved preferences. Every decision is recorded as a step in its history.".into()],
        recipe,
    })
}
fn error(message: &str) -> IpcError {
    let mut error: IpcError =
        aura_core::errors::render::recipe_invalid("automatic workflow", message).into();
    message.clone_into(&mut error.message);
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
    JOBS.lock().get(job).is_none_or(|s| s.cancel.is_cancelled())
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
    Ok(unique_folder(&pictures.join("AURA Exports"), project))
}
/// Every run gets a folder of its own, so two weddings sent to one drive never share a
/// report or overwrite each other's files.
fn unique_folder(parent: &Path, project: &str) -> PathBuf {
    parent.join(format!("{project}-{}", uuid::Uuid::new_v4()))
}

/// Selecting files is the only required interaction. Existing manual edits remain protected.
///
/// # Errors
/// Refuses while another run is active, when a selected path is missing or relative, and
/// when the project, import or run cannot be started.
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
    let project = if let Some(id) = input.project_id {
        id
    } else {
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
    };
    let destination = match input.destination.filter(|chosen| !chosen.trim().is_empty()) {
        Some(chosen) if Path::new(chosen.trim()).is_absolute() => {
            unique_folder(Path::new(chosen.trim()), &project)
        }
        Some(_) => return Err(error("The export folder must be an absolute path.")),
        None => output_folder(&project)?,
    }
    .to_string_lossy()
    .into_owned();
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
            look: input.look,
            keep_everything: input.keep_everything,
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
///
/// # Errors
/// Refuses while another run is active, for an unknown project, and when the worker cannot
/// be started.
pub fn one_click_finish(
    state: &AppState,
    input: OneClickFinishInput,
) -> Result<OneClickFinishDto, IpcError> {
    let _request = REQUESTS.lock();
    require_idle()?;
    start(state, input)
}
#[allow(clippy::too_many_lines)]
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
            // The slot is inserted before this thread starts and removed only when the
            // spawn fails, so it is always present here. Should that ever stop being
            // true, release the job rather than panic on a background thread.
            let Ok(mut final_status) = one_click_status(&worker_job) else {
                worker.finish_job(&worker_job);
                child(&worker_job, None);
                return;
            };
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
                            s.phase_label.clone_from(&err.message);
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
///
/// # Errors
/// Refuses a job id this process is not running or has not kept.
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

#[allow(clippy::too_many_lines)]
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
        offset = offset.saturating_add(i64::try_from(count).unwrap_or(i64::MAX));
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
                .unwrap_or_default();
        });
    } else {
        update(job, |s| s.model = "local reference".into());
        note(job, "No working provider or cloud disabled by privacy settings; using measured local reference edits. Configure a vision model in AI provider for semantic photo-aware choices.".into());
    }

    // Learned scene, people and framing analysis. Every model it consults is still a
    // placeholder and nothing in this build is calibrated, so its readings may not be acted
    // on - and it costs about three seconds a photograph to produce them. An unattended run
    // therefore measures instead, and the learned pass is opt-in for whoever is validating it.
    let mut advanced = false;
    if learned_analysis_enabled() {
        match crate::autopilot_start(
            state,
            &AutopilotStartInput {
                project_id: project.clone(),
                disabled: vec![],
                zero_touch: true,
                allow_on_battery: false,
                quiet_mode: false,
            },
        ) {
            Ok(_) => {
                phase(
                    job,
                    "analyze",
                    "Running available advanced analysis models.",
                    photos.len() as u64,
                );
                let deadline = state
                    .clock()
                    .monotonic_ms()
                    .saturating_add(6 * 3600 * 1000);
                // The progress row exists only while the run does; the summary says how it ended.
                while let Some(p) = crate::autopilot_progress(state, project)? {
                    if stopped(job) {
                        let _ = crate::autopilot_cancel(state, project);
                        return Ok(());
                    }
                    update(job, |s| s.phase_label = p.stage_title);
                    if state.clock().monotonic_ms() > deadline {
                        let _ = crate::autopilot_cancel(state, project);
                        return Err(error("Advanced analysis timed out and was stopped."));
                    }
                    std::thread::sleep(Duration::from_millis(500));
                }
                advanced = crate::autopilot_commands::autopilot_summary(state, project)?
                    .is_some_and(|summary| summary.status == "completed");
                if !advanced {
                    note(job, "Learned analysis finished without a result this run may act on; each photograph was measured instead.".into());
                }
            }
            Err(err) => note(
                job,
                format!(
                    "Optional learned analysis unavailable: {}. Pixel-based analysis completed; automatic face/subject claims are not made.",
                    err.message
                ),
            ),
        }
    } else {
        note(job, "Learned scene, people and framing analysis was not run: its bundled models are placeholders and nothing in this build is calibrated. Each photograph was measured instead, and its framing is preserved.".into());
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
    } else if input.keep_everything {
        note(
            job,
            "Every readable photograph is delivered: the cull was switched off for this run."
                .into(),
        );
    } else {
        match measured_cull(
            state,
            job,
            project,
            &photos,
            &readable,
            Path::new(&input.destination),
        )? {
            Some(kept) => targets = kept,
            None => return Ok(()),
        }
    }
    update(job, |s| s.selected = targets.len() as u64);
    phase(
        job,
        "edit",
        "Editing and retouching each photo using its own measurements and preferences.",
        targets.len() as u64,
    );
    let look = input
        .look
        .as_ref()
        .filter(|look| look.profile_id.is_some() || look.reference_id.is_some());
    if look.is_some() && use_cloud {
        note(job, "The look you chose is applied to every photograph, so the configured provider was not asked for its own grade.".into());
    }
    let names: BTreeMap<&str, &str> = photos
        .iter()
        .map(|photo| (photo.id.as_str(), photo.file_name.as_str()))
        .collect();
    let mut edits = EditLog::create(Path::new(&input.destination).join("photo-edits.json"))?;
    let started = state.clock().monotonic_ms();
    for (index, photo) in targets.iter().enumerate() {
        if stopped(job) {
            return Ok(());
        }
        let left = remaining(started, state.clock().monotonic_ms(), index, targets.len());
        update(job, |s| {
            s.phase_label = format!(
                "Editing and retouching photo {} of {}: {}.{left}",
                index + 1,
                targets.len(),
                names.get(photo.as_str()).copied().unwrap_or("photograph")
            );
        });
        let result = edit_one(
            state,
            job,
            project,
            photo,
            look,
            use_cloud && look.is_none() && index < MAX_AI_EDITS,
        );
        if stopped(job) {
            return Ok(());
        }
        match result {
            Ok(entry) => {
                update(job, |s| {
                    if entry.source == "cloud" || entry.source == "cache" {
                        s.ai_edited += 1;
                    } else {
                        s.local_edited += 1;
                    }
                });
                edits.push(&entry)?;
            }
            Err(err) => {
                update(job, |s| s.failed_edits += 1);
                note(
                    job,
                    format!(
                        "{} edit or retouch failed; exporting its last saved reversible recipe: {}",
                        names.get(photo.as_str()).copied().unwrap_or(photo),
                        err.message
                    ),
                );
            }
        }
        update(job, |s| s.items_done = (index + 1) as u64);
    }
    if use_cloud && look.is_none() && targets.len() > MAX_AI_EDITS {
        note(job, format!("Provider calls capped at {MAX_AI_EDITS}; ALL remaining photos received local adaptive edits and remain in the export."));
    }
    edits.close()?;
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
