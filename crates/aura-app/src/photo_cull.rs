//! The cull a photographer runs, reviews and corrects before anything is edited. ADR-0185.
//!
//! [`crate::measured_cull`] decides; this module is everything around the decision that a
//! person needs:
//!
//! * **A run of its own.** The Cull screen culls a collection on a background thread and
//!   reports progress, without editing or exporting anything.
//! * **A stored review.** The decision is kept in the collection's catalog, so the Best and
//!   Left out lists are there after a restart, and Finish Folder uses the reviewed decision
//!   instead of measuring the same photographs again.
//! * **The person's word is final.** "Bring back", "Leave out" and "Keep every photo" are
//!   phase 12's `cull_override` rows - the table every cull in the product already obeys - so a
//!   choice survives a fresh cull and nothing in the product can overrule it. Clearing a choice
//!   hands the photograph back to the measured decision.
//! * **Strictness without measuring again.** The review keeps every frame's measurements, so
//!   moving between gentle, balanced and strict re-decides in a moment, and the review says
//!   what each level would deliver before the photographer picks one.
//!
//! Nothing here deletes, moves or edits a photograph. A left-out photograph stays in the
//! collection, is shown in its own list, and is one click from being delivered.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use aura_core::progress::CancelToken;
use aura_core::PhotoId;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::commands::IpcResult;
use crate::contract::ipc::{
    ImageRowLite, IpcError, ListImagesInput, PhotoCullCheckDto, PhotoCullChecksInput,
    PhotoCullInput, PhotoCullJobDto, PhotoCullLevelDto, PhotoCullPhotoDto, PhotoCullReviewDto,
    PhotoCullStatusDto, SetPhotoCullChoiceInput, SetPhotoCullStrictnessInput,
};
use crate::cull_checks::{Category, CHECKS};
use crate::measured_cull::{Frame, Reason, Report, Strictness};
use crate::AppState;

/// The `setting` row a collection's review lives in. Versioned, so a later layout is read as
/// "no review yet" rather than misread.
fn setting_key(project: &str) -> String {
    format!("photo_cull.v1.{project}")
}

/// One photograph of a stored review.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoredPhoto {
    pub(crate) photo_id: String,
    pub(crate) file_name: String,
    pub(crate) keep: bool,
    pub(crate) reason: Reason,
    pub(crate) detail: String,
    pub(crate) burst: Option<usize>,
    pub(crate) moment: Option<usize>,
    pub(crate) kept_instead: Option<String>,
    pub(crate) score: Option<f32>,
    pub(crate) top_pick: bool,
    pub(crate) categories: Vec<Option<u8>>,
    pub(crate) checks: Vec<Option<u8>>,
    /// The raw measurement behind each check, rounded for storage.
    pub(crate) values: Vec<Option<f32>>,
}

/// A collection's stored review: what the measured cull decided, before anybody's choices.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Stored {
    pub(crate) measured_at: String,
    pub(crate) summary: String,
    pub(crate) photos: Vec<StoredPhoto>,
    /// The level this review was decided at.
    #[serde(default)]
    pub(crate) strictness: Strictness,
    /// Every frame as the decision read it, without the checks (`photos[].values` holds
    /// those), so another level can be decided without measuring again. Empty in a review
    /// written before strictness existed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) frames: Vec<Frame>,
    /// The photographs each level leaves out, before anybody's choices.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) levels: Vec<(Strictness, Vec<String>)>,
}

/// Four significant figures are more than any threshold here can tell apart.
// The exponent of a finite f32 is within +-45, so the cast cannot truncate.
#[allow(clippy::cast_possible_truncation)]
fn rounded(value: f32) -> f32 {
    if value.abs() < f32::MIN_POSITIVE || !value.is_finite() {
        return value;
    }
    let digits = 3 - value.abs().log10().floor() as i32;
    let scale = 10_f32.powi(digits.clamp(-6, 8));
    (value * scale).round() / scale
}

/// Frames rounded as the review stores them, so a decision made now and one made later from
/// the stored review are the same decision.
pub(crate) fn as_stored(frames: Vec<Frame>) -> Vec<Frame> {
    frames
        .into_iter()
        .map(|mut frame| {
            if let Some(measure) = frame.measure.as_mut() {
                for value in measure.checks.iter_mut().flatten() {
                    *value = rounded(*value);
                }
            }
            frame
        })
        .collect()
}

/// Decide `frames` at `strictness`, and note what every level would leave out.
pub(crate) fn decide_all(frames: &[Frame], strictness: Strictness) -> (Report, Levels) {
    let report = crate::measured_cull::decide_with(frames, strictness);
    let levels = Strictness::ALL
        .iter()
        .map(|level| {
            let left: Vec<String> = if *level == strictness {
                report
                    .verdicts
                    .iter()
                    .filter(|v| !v.keep)
                    .map(|v| v.photo_id.clone())
                    .collect()
            } else {
                crate::measured_cull::decide_with(frames, *level)
                    .verdicts
                    .into_iter()
                    .filter(|v| !v.keep)
                    .map(|v| v.photo_id)
                    .collect()
            };
            (*level, left)
        })
        .collect();
    (report, levels)
}

/// What each strictness level leaves out.
pub(crate) type Levels = Vec<(Strictness, Vec<String>)>;

impl Stored {
    fn from_report(report: &Report, measured_at: String, frames: &[Frame], levels: Levels) -> Self {
        Self {
            measured_at,
            summary: report.summary(),
            strictness: report.strictness,
            frames: frames
                .iter()
                .map(|frame| {
                    let mut frame = frame.clone();
                    if let Some(measure) = frame.measure.as_mut() {
                        measure.checks.clear();
                    }
                    frame
                })
                .collect(),
            levels,
            photos: report
                .verdicts
                .iter()
                .map(|v| StoredPhoto {
                    photo_id: v.photo_id.clone(),
                    file_name: v.file_name.clone(),
                    keep: v.keep,
                    reason: v.reason,
                    detail: v.detail.clone(),
                    burst: v.burst,
                    moment: v.moment,
                    kept_instead: v.kept_instead.clone(),
                    score: v.score,
                    top_pick: v.top_pick,
                    categories: v.categories.clone(),
                    checks: v.checks.clone(),
                    values: v
                        .measure
                        .as_ref()
                        .map(|m| m.checks.iter().map(|c| c.map(rounded)).collect())
                        .unwrap_or_default(),
                })
                .collect(),
        }
    }

    /// The frames as the decision read them, checks restored; `None` when the review
    /// predates stored frames or does not line up with its photographs.
    pub(crate) fn frames(&self) -> Option<Vec<Frame>> {
        if self.frames.is_empty() || self.frames.len() != self.photos.len() {
            return None;
        }
        self.frames
            .iter()
            .zip(&self.photos)
            .map(|(frame, photo)| {
                (frame.photo_id == photo.photo_id).then(|| {
                    let mut frame = frame.clone();
                    if let Some(measure) = frame.measure.as_mut() {
                        // The frame's own ninety, as a fresh measurement has them: the ten
                        // collection checks are the decision's to fill, and a stored copy of
                        // them would score the frame against its last decision.
                        measure.checks = photo
                            .values
                            .iter()
                            .take(crate::cull_checks::CONTEXT)
                            .copied()
                            .collect();
                        if !measure.checks.is_empty() {
                            measure.checks.resize(crate::cull_checks::COUNT, None);
                        }
                    }
                    frame
                })
            })
            .collect()
    }

    /// The review as an export folder's `photo-cull.json` carries it: the decision, not the
    /// measurements kept for re-deciding.
    pub(crate) fn for_export(&self) -> Self {
        Self {
            frames: Vec::new(),
            ..self.clone()
        }
    }
}

/// Keep a collection's review in its catalog, replacing the last one. `frames` are the ones
/// `report` was decided from, and `levels` what each strictness leaves out.
///
/// # Errors
/// The catalog write failing.
pub(crate) fn save(
    state: &AppState,
    project: &str,
    report: &Report,
    frames: &[Frame],
    levels: Levels,
) -> IpcResult<()> {
    let now = aura_catalog::rfc3339(state.clock().now_utc());
    write(
        state,
        project,
        &Stored::from_report(report, now, frames, levels),
    )
}

fn write(state: &AppState, project: &str, stored: &Stored) -> IpcResult<()> {
    let now = aura_catalog::rfc3339(state.clock().now_utc());
    let encoded = serde_json::to_string(&stored).map_err(|e| error(&e.to_string()))?;
    let key = setting_key(project);
    state.catalog().writer().transact(move |conn| {
        conn.execute(
            "INSERT INTO setting (key, value_json, updated_at) VALUES (?1, ?2, ?3)
               ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json,
                                              updated_at = excluded.updated_at",
            rusqlite::params![key, encoded, now],
        )
        .map_err(|e| aura_core::errors::db::statement_failed("could not save the cull", &e))?;
        Ok(())
    })?;
    Ok(())
}

/// A collection's stored review, or `None` when it was never culled or the row is from a
/// layout this build does not read.
///
/// # Errors
/// The catalog read failing.
pub(crate) fn stored(state: &AppState, project: &str) -> IpcResult<Option<Stored>> {
    let key = setting_key(project);
    let text: Option<String> = state.catalog().read(move |conn| {
        conn.query_row(
            "SELECT value_json FROM setting WHERE key = ?1",
            [key],
            |row| row.get(0),
        )
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })
        .map_err(|e| aura_core::errors::db::statement_failed("could not read the cull", &e))
    })?;
    Ok(text.and_then(|t| serde_json::from_str(&t).ok()))
}

fn store(state: &AppState) -> aura_cull::store::CullStore {
    aura_cull::store::CullStore::new(Arc::clone(state.catalog()), Arc::clone(state.clock()))
}

/// The person's choices in a collection: `true` to keep, `false` to leave out.
///
/// # Errors
/// The catalog read failing.
pub(crate) fn choices(state: &AppState, project: &str) -> IpcResult<BTreeMap<String, bool>> {
    let id = aura_core::ProjectId::from_db(project).map_err(|_| error("Invalid project id."))?;
    Ok(store(state)
        .overrides(id)?
        .into_iter()
        .map(|(photo, keep)| (photo.to_db(), keep))
        .collect())
}

/// What is delivered once the person's choices are applied to a stored review.
pub(crate) fn delivered(review: &Stored, choices: &BTreeMap<String, bool>) -> Vec<String> {
    review
        .photos
        .iter()
        .filter(|p| choices.get(&p.photo_id).copied().unwrap_or(p.keep))
        .map(|p| p.photo_id.clone())
        .collect()
}

/// The same for a fresh report.
pub(crate) fn delivered_from(report: &Report, choices: &BTreeMap<String, bool>) -> Vec<String> {
    report
        .verdicts
        .iter()
        .filter(|v| choices.get(&v.photo_id).copied().unwrap_or(v.keep))
        .map(|v| v.photo_id.clone())
        .collect()
}

fn error(message: &str) -> IpcError {
    let mut error: IpcError = aura_core::errors::render::recipe_invalid("cull", message).into();
    message.clone_into(&mut error.message);
    error
}

/// Every photograph of a collection, in timeline order.
fn all_photos(state: &AppState, project: &str) -> IpcResult<Vec<ImageRowLite>> {
    let mut photos = Vec::new();
    let mut offset = 0;
    loop {
        let page = crate::list_images(
            state,
            &ListImagesInput {
                project_id: project.to_owned(),
                offset,
                limit: 500,
                order_by: Some("timeline".into()),
            },
        )?;
        let count = page.len();
        photos.extend(page);
        offset = offset.saturating_add(i64::try_from(count).unwrap_or(i64::MAX));
        if count < 500 {
            return Ok(photos);
        }
    }
}

/// Measure every photograph, a few at a time. `progress` is told after each one; `stop`
/// ends the walk early and returns `None`. A frame that cannot be read is kept unmeasured -
/// and so delivered - rather than dropped.
pub(crate) fn measure_frames(
    state: &AppState,
    project: &str,
    photos: &[&ImageRowLite],
    stop: &(dyn Fn() -> bool + Sync),
    progress: &(dyn Fn() + Sync),
) -> Option<Vec<Frame>> {
    let times = crate::measured_cull::camera_times(state, project).unwrap_or_default();
    let settings = crate::measured_cull::camera_settings(state, project).unwrap_or_default();
    let lanes = crate::one_click_commands::workers(state);
    let mut frames = Vec::with_capacity(photos.len());
    for chunk in photos.chunks(lanes.saturating_mul(16).max(1)) {
        if stop() {
            return None;
        }
        let measured = crate::one_click_commands::in_order(chunk, lanes, |photo| {
            let frame =
                crate::measured_cull::measure_photo(state, project, photo, &times, &settings)
                    .unwrap_or_else(|_| Frame {
                        photo_id: photo.id.clone(),
                        file_name: photo.file_name.clone(),
                        measure: None,
                        time_ms: None,
                        edited_by_hand: false,
                    });
            progress();
            frame
        });
        frames.extend(measured);
    }
    (!stop()).then_some(frames)
}

// ----------------------------------------------------------------------------------------
// The background run.
// ----------------------------------------------------------------------------------------

struct Job {
    status: PhotoCullStatusDto,
    cancel: CancelToken,
}

static JOBS: Mutex<BTreeMap<String, Job>> = Mutex::new(BTreeMap::new());

fn update(job: &str, change: impl FnOnce(&mut PhotoCullStatusDto)) {
    if let Some(slot) = JOBS.lock().get_mut(job) {
        change(&mut slot.status);
    }
}

fn cancelled(job: &str) -> bool {
    JOBS.lock().get(job).is_none_or(|s| s.cancel.is_cancelled())
}

/// Cull every photograph of a collection in the background: measure, decide and store the
/// review. Nothing is edited, exported or deleted. Starting it again while it runs returns
/// the run already going.
///
/// # Errors
/// An invalid or unknown collection, or the worker failing to start.
pub fn start_photo_cull(state: &AppState, input: PhotoCullInput) -> IpcResult<PhotoCullJobDto> {
    let project = input.project_id;
    aura_core::ProjectId::from_db(&project).map_err(|_| error("Invalid project id."))?;
    let strictness = match input.strictness.as_deref() {
        Some(slug) => Strictness::parse(slug)
            .ok_or_else(|| error(&format!("`{slug}` is not gentle, balanced or strict.")))?,
        None => last_strictness(state, &project),
    };
    if !crate::list_projects(state)?.iter().any(|p| p.id == project) {
        return Err(error("This collection no longer exists."));
    }
    let mut jobs = JOBS.lock();
    if let Some((id, _)) = jobs
        .iter()
        .find(|(_, j)| j.status.project_id == project && j.status.status == "running")
    {
        return Ok(PhotoCullJobDto { job_id: id.clone() });
    }
    let job = format!("cull-{}", uuid::Uuid::new_v4());
    let cancel = CancelToken::new();
    state.register_job(&job, cancel.clone());
    jobs.insert(
        job.clone(),
        Job {
            status: PhotoCullStatusDto {
                job_id: job.clone(),
                project_id: project.clone(),
                status: "running".into(),
                done: 0,
                total: 0,
                label: "Reading the collection.".into(),
            },
            cancel,
        },
    );
    drop(jobs);
    let worker = state.clone();
    let worker_job = job.clone();
    let spawned = std::thread::Builder::new()
        .name("aura-cull".into())
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run(&worker, &worker_job, &project, strictness)
            }))
            .unwrap_or_else(|_| Err(error("The cull stopped unexpectedly. Nothing was changed.")));
            update(&worker_job, |s| match result {
                Ok(true) => {
                    s.status = "completed".into();
                    s.label = "Culled. Review the best photos and the ones left out.".into();
                }
                Ok(false) => {
                    s.status = "cancelled".into();
                    s.label = "Stopped. The last saved cull is unchanged.".into();
                }
                Err(err) => {
                    s.status = "failed".into();
                    s.label = err.message;
                }
            });
            worker.finish_job(&worker_job);
        });
    if let Err(e) = spawned {
        JOBS.lock().remove(&job);
        state.finish_job(&job);
        return Err(error(&format!("Could not start the cull: {e}")));
    }
    Ok(PhotoCullJobDto { job_id: job })
}

/// The level a collection was last culled at; balanced when it never was.
pub(crate) fn last_strictness(state: &AppState, project: &str) -> Strictness {
    stored(state, project)
        .ok()
        .flatten()
        .map(|review| review.strictness)
        .unwrap_or_default()
}

/// `Ok(false)` when stopped.
fn run(state: &AppState, job: &str, project: &str, strictness: Strictness) -> IpcResult<bool> {
    let photos = all_photos(state, project)?;
    if photos.is_empty() {
        return Err(error("This collection has no photos to cull yet."));
    }
    let total = photos.len() as u64;
    update(job, |s| {
        s.total = total;
        s.label = format!("Checking {total} photos against 100 measurements each.");
    });
    let chosen: Vec<&ImageRowLite> = photos.iter().collect();
    let Some(frames) = measure_frames(state, project, &chosen, &|| cancelled(job), &|| {
        update(job, |s| s.done = s.done.saturating_add(1));
    }) else {
        return Ok(false);
    };
    update(job, |s| {
        s.label = "Choosing the best of every moment.".into();
    });
    let frames = as_stored(frames);
    let (report, levels) = decide_all(&frames, strictness);
    save(state, project, &report, &frames, levels)?;
    Ok(true)
}

/// Progress of a cull this process is running or ran.
///
/// # Errors
/// An unknown job id.
pub fn photo_cull_status(job_id: &str) -> IpcResult<PhotoCullStatusDto> {
    JOBS.lock()
        .get(job_id)
        .map(|j| j.status.clone())
        .ok_or_else(|| error("This cull is no longer running. Open the Cull screen to see the last saved result."))
}

/// Stop a running cull between photographs. True when it was running.
#[must_use]
pub fn photo_cull_cancel(job_id: &str) -> bool {
    let mut jobs = JOBS.lock();
    let Some(job) = jobs
        .get_mut(job_id)
        .filter(|j| j.status.status == "running")
    else {
        return false;
    };
    job.cancel.cancel();
    job.status.label = "Stopping after the current photos.".into();
    true
}

// ----------------------------------------------------------------------------------------
// The review.
// ----------------------------------------------------------------------------------------

/// The reason, in words a photographer uses.
fn reason_label(reason: Reason) -> &'static str {
    match reason {
        Reason::Keep => "Good photo",
        Reason::BestOfBurst => "Best of its burst",
        Reason::EditedByHand => "You edited it",
        Reason::Unmeasured => "Could not be checked",
        Reason::RulesWithdrawn => "Kept: the technical rules were withdrawn",
        Reason::UnusableExposure => "Unusable exposure",
        Reason::OutOfFocus => "Out of focus",
        Reason::MotionBlur => "Motion blur",
        Reason::BurstDuplicate => "Duplicate in a burst",
        Reason::WeakerOfMoment => "Weaker shot of the moment",
        Reason::LowScore => "Very low score",
    }
}

fn review_dto(
    project: &str,
    review: &Stored,
    choices: &BTreeMap<String, bool>,
    in_collection: &BTreeSet<String>,
) -> PhotoCullReviewDto {
    let photos: Vec<PhotoCullPhotoDto> = review
        .photos
        .iter()
        .filter(|p| in_collection.contains(&p.photo_id))
        .map(|p| {
            let choice = choices.get(&p.photo_id).copied();
            PhotoCullPhotoDto {
                photo_id: p.photo_id.clone(),
                file_name: p.file_name.clone(),
                keep: choice.unwrap_or(p.keep),
                cull_keep: p.keep,
                choice: choice.map(|keep| if keep { "keep" } else { "leave_out" }.into()),
                top_pick: p.top_pick && choice.unwrap_or(p.keep),
                reason: serde_json::to_value(p.reason)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_default(),
                reason_label: reason_label(p.reason).into(),
                detail: p.detail.clone(),
                score: p.score,
                categories: p.categories.clone(),
                kept_instead: p.kept_instead.clone(),
                burst: p.burst.map(|b| b as u64),
                moment: p.moment.map(|m| m as u64),
            }
        })
        .collect();
    let seen: BTreeSet<&str> = review.photos.iter().map(|p| p.photo_id.as_str()).collect();
    let best = photos.iter().filter(|p| p.keep).count() as u64;
    PhotoCullReviewDto {
        project_id: project.to_owned(),
        measured_at: review.measured_at.clone(),
        summary: review.summary.clone(),
        frames: photos.len() as u64,
        best,
        left_out: photos.len() as u64 - best,
        top_picks: photos.iter().filter(|p| p.top_pick).count() as u64,
        kept_by_you: photos
            .iter()
            .filter(|p| p.choice.as_deref() == Some("keep"))
            .count() as u64,
        left_out_by_you: photos
            .iter()
            .filter(|p| p.choice.as_deref() == Some("leave_out"))
            .count() as u64,
        not_culled: in_collection
            .iter()
            .filter(|id| !seen.contains(id.as_str()))
            .count() as u64,
        groups: Category::ALL.iter().map(|c| c.label().to_owned()).collect(),
        strictness: review.strictness.slug().into(),
        levels: review
            .levels
            .iter()
            .map(|(level, left)| {
                let left: BTreeSet<&str> = left.iter().map(String::as_str).collect();
                let best = photos
                    .iter()
                    .filter(|p| {
                        choices
                            .get(&p.photo_id)
                            .copied()
                            .unwrap_or(!left.contains(p.photo_id.as_str()))
                    })
                    .count() as u64;
                let (label, description) = level_words(*level);
                PhotoCullLevelDto {
                    strictness: level.slug().into(),
                    label: label.into(),
                    description: description.into(),
                    best,
                    left_out: photos.len() as u64 - best,
                }
            })
            .collect(),
        photos,
    }
}

/// A level in a word and a sentence.
fn level_words(level: Strictness) -> (&'static str, &'static str) {
    match level {
        Strictness::Gentle => (
            "Gentle",
            "Leaves out only mistakes - black, blown, out of focus, smeared - and near-identical frames.",
        ),
        Strictness::Balanced => (
            "Balanced",
            "Also leaves out clearly weaker shots of the same moment and very low scores.",
        ),
        Strictness::Strict => (
            "Strict",
            "Keeps only the strongest shots of every moment and leaves out weak photos.",
        ),
    }
}

fn collection_ids(state: &AppState, project: &str) -> IpcResult<BTreeSet<String>> {
    Ok(all_photos(state, project)?
        .into_iter()
        .map(|p| p.id)
        .collect())
}

/// The last cull of a collection with the person's choices applied, or `None` when it has
/// never been culled.
///
/// # Errors
/// An invalid collection id or the catalog read failing.
pub fn photo_cull_review(
    state: &AppState,
    project_id: &str,
) -> IpcResult<Option<PhotoCullReviewDto>> {
    aura_core::ProjectId::from_db(project_id).map_err(|_| error("Invalid project id."))?;
    let Some(review) = stored(state, project_id)? else {
        return Ok(None);
    };
    let choices = choices(state, project_id)?;
    let ids = collection_ids(state, project_id)?;
    Ok(Some(review_dto(project_id, &review, &choices, &ids)))
}

/// One photograph's hundred checks: what each measured and what it scored.
///
/// # Errors
/// A collection that was never culled, or a photograph not in its cull.
pub fn photo_cull_checks(
    state: &AppState,
    input: PhotoCullChecksInput,
) -> IpcResult<Vec<PhotoCullCheckDto>> {
    let review = stored(state, &input.project_id)?
        .ok_or_else(|| error("This collection has not been culled yet."))?;
    let photo = review
        .photos
        .iter()
        .find(|p| p.photo_id == input.photo_id)
        .ok_or_else(|| {
            error("This photo was not part of the last cull. Cull again to check it.")
        })?;
    Ok(CHECKS
        .iter()
        .enumerate()
        .map(|(index, check)| PhotoCullCheckDto {
            id: check.id.into(),
            group: check.category.label().into(),
            name: check.name.into(),
            score: photo.checks.get(index).copied().flatten(),
            value: photo.values.get(index).copied().flatten(),
            weight: check.weight,
        })
        .collect())
}

/// Re-decide a collection's last cull at another strictness, from the measurements already
/// taken: nothing is measured again, and every choice the photographer made still stands.
///
/// # Errors
/// An unknown level, a collection never culled, or a review from before stored measurements
/// (cull again once to change level).
pub fn set_photo_cull_strictness(
    state: &AppState,
    input: SetPhotoCullStrictnessInput,
) -> IpcResult<PhotoCullReviewDto> {
    aura_core::ProjectId::from_db(&input.project_id).map_err(|_| error("Invalid project id."))?;
    let level = Strictness::parse(&input.strictness).ok_or_else(|| {
        error(&format!(
            "`{}` is not gentle, balanced or strict.",
            input.strictness
        ))
    })?;
    let review = stored(state, &input.project_id)?
        .ok_or_else(|| error("This collection has not been culled yet."))?;
    if review.strictness != level {
        let frames = review.frames().ok_or_else(|| {
            error(
                "This cull was made before strictness levels. Cull again once to change the level.",
            )
        })?;
        let (report, levels) = decide_all(&frames, level);
        let mut next = Stored::from_report(&report, review.measured_at.clone(), &frames, levels);
        next.measured_at = review.measured_at;
        write(state, &input.project_id, &next)?;
    }
    photo_cull_review(state, &input.project_id)?
        .ok_or_else(|| error("This collection has not been culled yet."))
}

/// Bring photographs back, leave them out, or hand them back to the cull (`keep`,
/// `leave_out` or `auto`). "Keep every photo" is `keep` for everything left out. Returns
/// the review as it now stands.
///
/// # Errors
/// An unknown choice, an invalid id, or the catalog write failing.
pub fn set_photo_cull_choice(
    state: &AppState,
    input: SetPhotoCullChoiceInput,
) -> IpcResult<PhotoCullReviewDto> {
    let project = aura_core::ProjectId::from_db(&input.project_id)
        .map_err(|_| error("Invalid project id."))?;
    let keep = match input.choice.as_str() {
        "keep" => Some(true),
        "leave_out" => Some(false),
        "auto" => None,
        other => return Err(error(&format!("`{other}` is not keep, leave_out or auto."))),
    };
    let ids = collection_ids(state, &input.project_id)?;
    let store = store(state);
    for photo in &input.photo_ids {
        if !ids.contains(photo) {
            return Err(error("A photo in this choice is not in the collection."));
        }
        let image = PhotoId::from_db(photo).map_err(|_| error("Invalid photo id."))?;
        match keep {
            Some(keep) => store.set_override(&project, image, keep)?,
            None => {
                store.clear_override(image)?;
            }
        }
    }
    photo_cull_review(state, &input.project_id)?
        .ok_or_else(|| error("This collection has not been culled yet."))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photo(id: &str, keep: bool) -> StoredPhoto {
        StoredPhoto {
            photo_id: id.into(),
            file_name: format!("{id}.jpg"),
            keep,
            reason: if keep {
                Reason::Keep
            } else {
                Reason::BurstDuplicate
            },
            detail: String::new(),
            burst: None,
            moment: None,
            kept_instead: None,
            score: Some(70.0),
            top_pick: keep,
            categories: vec![],
            checks: vec![],
            values: vec![],
        }
    }

    fn review() -> Stored {
        Stored {
            measured_at: "2026-10-28T00:00:00Z".into(),
            summary: String::new(),
            photos: vec![photo("a", true), photo("b", false), photo("c", true)],
            strictness: Strictness::Balanced,
            frames: Vec::new(),
            levels: vec![
                (Strictness::Gentle, vec![]),
                (Strictness::Balanced, vec!["b".into()]),
                (Strictness::Strict, vec!["b".into(), "c".into()]),
            ],
        }
    }

    #[test]
    fn each_level_is_counted_with_the_choices_applied() {
        let choices = BTreeMap::from([("c".to_string(), true)]);
        let ids: BTreeSet<String> = ["a", "b", "c"].iter().map(ToString::to_string).collect();
        let dto = review_dto("p", &review(), &choices, &ids);
        assert_eq!(dto.strictness, "balanced");
        let counts: Vec<(String, u64, u64)> = dto
            .levels
            .iter()
            .map(|l| (l.strictness.clone(), l.best, l.left_out))
            .collect();
        // Strict would leave c out, but the photographer kept it.
        assert_eq!(
            counts,
            [
                ("gentle".to_string(), 3, 0),
                ("balanced".to_string(), 2, 1),
                ("strict".to_string(), 2, 1)
            ]
        );
    }

    #[test]
    fn a_review_without_frames_cannot_be_re_decided_and_exports_without_them() {
        assert!(review().frames().is_none());
        let frame = |id: &str| Frame {
            photo_id: id.into(),
            file_name: format!("{id}.jpg"),
            measure: Some(crate::measured_cull::Measure::default()),
            time_ms: Some(5),
            edited_by_hand: false,
        };
        let mut with = review();
        with.photos[0].values = vec![Some(0.5); 3];
        with.frames = vec![frame("a"), frame("b"), frame("c")];
        let frames = with.frames().expect("frames");
        let mut restored = vec![Some(0.5); 3];
        restored.resize(crate::cull_checks::COUNT, None);
        assert_eq!(frames[0].measure.as_ref().unwrap().checks, restored);
        assert!(with.for_export().frames.is_empty());
        // A stored review read back is the same review.
        let text = serde_json::to_string(&with).expect("json");
        assert_eq!(serde_json::from_str::<Stored>(&text).expect("read"), with);
        // Frames out of line with the photos are not trusted.
        with.frames.swap(0, 1);
        assert!(with.frames().is_none());
    }

    #[test]
    fn a_choice_overrules_the_cull_in_both_directions() {
        let choices = BTreeMap::from([("b".to_string(), true), ("c".to_string(), false)]);
        assert_eq!(delivered(&review(), &choices), ["a", "b"]);
        assert_eq!(delivered(&review(), &BTreeMap::new()), ["a", "c"]);
    }

    #[test]
    fn the_review_counts_choices_and_photos_added_since() {
        let choices = BTreeMap::from([("b".to_string(), true)]);
        let ids: BTreeSet<String> = ["a", "b", "c", "new"]
            .iter()
            .map(ToString::to_string)
            .collect();
        let dto = review_dto("p", &review(), &choices, &ids);
        assert_eq!((dto.best, dto.left_out, dto.kept_by_you), (3, 0, 1));
        assert_eq!(dto.not_culled, 1);
        let b = dto.photos.iter().find(|p| p.photo_id == "b").unwrap();
        assert!(b.keep && !b.cull_keep);
        assert_eq!(b.choice.as_deref(), Some("keep"));
        assert_eq!(b.reason, "burst_duplicate");
        // A photograph removed from the collection drops out of the review.
        let fewer: BTreeSet<String> = ["a"].iter().map(ToString::to_string).collect();
        assert_eq!(
            review_dto("p", &review(), &BTreeMap::new(), &fewer).frames,
            1
        );
    }

    #[test]
    fn a_left_out_top_pick_is_not_shown_as_one() {
        let choices = BTreeMap::from([("a".to_string(), false)]);
        let ids: BTreeSet<String> = ["a", "b", "c"].iter().map(ToString::to_string).collect();
        let dto = review_dto("p", &review(), &choices, &ids);
        let a = dto.photos.iter().find(|p| p.photo_id == "a").unwrap();
        assert!(!a.keep && !a.top_pick);
        assert_eq!(dto.left_out_by_you, 1);
    }

    /// A frame of large blocks, as `measured_cull`'s tests paint them, measured as the cull
    /// measures one. `soft` widens each block's edge.
    // Block coordinates are under 16 and a value is reduced below 256 before its cast.
    #[allow(clippy::cast_possible_truncation)]
    fn painted(id: &str, seed: u32, soft: usize, time_ms: i64) -> Frame {
        let (width, height) = (640_usize, 480_usize);
        let rgb: Vec<u8> = (0..width * height)
            .flat_map(|i| {
                let (x, y) = (i % width, i / width);
                let (x, y) = (x - x % (soft + 1), y - y % (soft + 1));
                let cell = (x / 64) as u32 * 31 + (y / 64) as u32 * 17 + seed * 7919;
                let value = (cell.wrapping_mul(2_654_435_761).rotate_left(13) % 200 + 30) as u8;
                [value; 3]
            })
            .collect();
        let mut measure = crate::measured_cull::measure_pixels(&rgb, width, height, &[]).unwrap();
        measure.checks = crate::cull_checks::measure(&crate::cull_checks::Sample {
            rgb: &rgb,
            width,
            height,
            faces: &[],
            settings: crate::cull_checks::Settings::default(),
        })
        .unwrap();
        Frame {
            photo_id: id.into(),
            file_name: format!("{id}.jpg"),
            measure: Some(measure),
            time_ms: Some(time_ms),
            edited_by_hand: false,
        }
    }

    #[test]
    fn a_stored_review_re_decides_exactly_as_it_was_decided() {
        let frames = as_stored(vec![
            painted("a", 1, 0, 0),
            painted("a2", 1, 1, 3_000),
            painted("a3", 1, 3, 6_000),
            painted("b", 2, 0, 90_000),
            painted("c", 3, 0, 400_000),
        ]);
        let summary = |r: &Report| -> Vec<(String, bool, Reason, Option<f32>)> {
            r.verdicts
                .iter()
                .map(|v| (v.photo_id.clone(), v.keep, v.reason, v.score))
                .collect()
        };
        for level in Strictness::ALL {
            let (report, levels) = decide_all(&frames, level);
            let stored = Stored::from_report(&report, "now".into(), &frames, levels);
            let text = serde_json::to_string(&stored).unwrap();
            let read: Stored = serde_json::from_str(&text).unwrap();
            let again = crate::measured_cull::decide_with(&read.frames().unwrap(), level);
            assert_eq!(summary(&again), summary(&report), "{level:?}");
        }
    }

    #[test]
    fn stored_values_keep_four_significant_figures() {
        assert!((rounded(0.123_456) - 0.1235).abs() < 1e-6);
        assert!((rounded(6400.4) - 6400.0).abs() < 1e-3);
        assert_eq!(rounded(0.0), 0.0);
    }
}
