//! The integrity half of the command surface.
//!
//! Six commands, and the shape of the six is the argument. Phase 08's surface had nine
//! and five of them changed a grouping; here five are reads and exactly one changes
//! anything - [`dismiss_flag`], which clears a single mark the photographer disagrees
//! with.
//!
//! **There is no command here that keeps, rejects, ranks or orders a photograph.**
//! Section 2.2 puts every one of those in phase 12, and this is the surface where
//! crossing that line would be easiest: a `technical_score` sorted descending *looks*
//! exactly like a cull. [`within_moment`] comes closest and deliberately stops short - it
//! returns a sharpness ranking inside one moment, which is evidence phase 12 asked for
//! by name, and it says nothing about what should be delivered.
//!
//! **Nothing here blocks for long, except the one command that says it does.**
//! `integrity_status`, `image_integrity`, `flagged_images` and `within_moment` read the
//! catalog. `analyse_integrity` is a whole-project pass that decodes a proxy per frame;
//! the UI calls it from a job rather than from a click handler, and it is cancellable.
//!
//! **The panel is told which marks are the good news.** `IntegrityDto::has_defect` and
//! `IntegrityReasonDto::exoneration` are computed here and sent, rather than derived in
//! the interface from a list of slugs. Two of the fourteen flags describe something
//! *right* with a photograph, and a UI that worked that out for itself would work it out
//! wrong exactly once.

use aura_brain_photo::integrity::MODEL_VER;
use aura_core::contract::integrity::{
    EyeState, ImageId, IntegrityFlags, IntegrityResult, IntegrityService, Reason,
};
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{MomentId, PhotoId, Priority, ProjectId};

use crate::commands::IpcResult;
use crate::contract::ipc::{
    AnalyseIntegrityInput, CropRectDto, DismissFlagInput, EyeStateDto, FlaggedInput, IntegrityDto,
    IntegrityPassDto, IntegrityReasonDto, IntegrityStatusDto, IpcError, RankedFrameDto,
    WithinMomentInput,
};
use crate::state::AppState;

/// What the Integrity panel's header shows.
///
/// # Errors
///
/// `AURA-DB-3006` when the verdicts cannot be read.
pub fn integrity_status(state: &AppState, project_id: &str) -> IpcResult<IntegrityStatusDto> {
    let project = parse_project(project_id)?;
    let outline = state.integrity().outline(project)?;
    Ok(IntegrityStatusDto {
        photos: outline.photos,
        scored: outline.scored,
        coverage: outline.coverage,
        subject_aware: outline.subject_aware,
        reviewed: outline.reviewed,
        flag_counts: outline.flag_histogram.to_vec(),
        flag_names: IntegrityFlags::ALL
            .iter()
            .map(|flag| flag.as_str().unwrap_or("").to_string())
            .collect(),
        mean_score: outline.mean_score,
        defective_at_most: outline.defective_at_most(),
        uncalibrated: outline.uncalibrated.clone(),
        model_ver: outline.model_ver,
        analysis_ver: outline.analysis_ver,
        calib_ver: outline.calib_ver,
    })
}

/// One photograph's verdict, for the Integrity card.
///
/// `None` means **nobody has looked**, and the card says so rather than drawing a clean
/// verdict. Migration 9's fifth property, carried out to the interface.
///
/// # Errors
///
/// `AURA-DB-3006` when the verdict cannot be read.
pub fn image_integrity(state: &AppState, photo_id: &str) -> IpcResult<Option<IntegrityDto>> {
    let image = parse_photo(photo_id)?;
    Ok(state.integrity().of_image(image)?.as_ref().map(dto))
}

/// The frames carrying any of these flags, worst score first.
///
/// The filter chips and the technical review queue.
///
/// # Errors
///
/// `AURA-DB-3006` when the verdicts cannot be read.
pub fn flagged_images(state: &AppState, input: &FlaggedInput) -> IpcResult<Vec<String>> {
    let project = parse_project(&input.project_id)?;
    let mut flags = IntegrityFlags::EMPTY;
    for slug in &input.flags {
        flags = flags.union(IntegrityFlags::from_str_or_empty(slug));
    }
    // An unrecognised slug contributes nothing rather than matching everything. A chip
    // built against a newer build must return an empty queue, not the whole wedding.
    let limit = input.limit.unwrap_or(200).clamp(1, 5_000) as usize;
    Ok(state
        .integrity()
        .flagged(project, flags, limit)?
        .into_iter()
        .map(|id| id.to_db())
        .collect())
}

/// One moment's frames ranked by subject sharpness, sharpest first.
///
/// Section 6.1's within-moment answer. **Evidence, not a cull**: it says which of six
/// frames is sharpest and nothing about which of them a client sees.
///
/// # Errors
///
/// `AURA-DB-3006` when the moment or its verdicts cannot be read.
pub fn within_moment(
    state: &AppState,
    input: &WithinMomentInput,
) -> IpcResult<Vec<RankedFrameDto>> {
    let moment = MomentId::from_db(&input.moment_id).map_err(|_| bad_id("moment"))?;
    Ok(state
        .integrity()
        .within_moment(moment)?
        .into_iter()
        .map(|(photo, rank)| RankedFrameDto {
            photo_id: photo.to_db(),
            relative_sharpness: rank,
        })
        .collect())
}

/// Tell AURA that one technical mark is wrong.
///
/// The only command on this surface that changes anything. It clears the flag, records
/// the dismissal, and is not undone by a re-analysis - the dismissal is re-applied to
/// the freshly computed flags inside the statement that stores them.
///
/// It changes what the panel shows and what a chip returns. It does not change whether
/// the photograph is delivered, because nothing in this phase does.
///
/// # Errors
///
/// `AURA-ML-5034` when the photograph has no verdict, when the flag is not a single
/// flag, when it is not set, or when it is one of the three that are not faults.
pub fn dismiss_flag(state: &AppState, input: &DismissFlagInput) -> IpcResult<IntegrityDto> {
    let image = parse_photo(&input.photo_id)?;
    let flag = IntegrityFlags::from_str_or_empty(&input.flag);
    let integrity = state.integrity();
    integrity.dismiss(image, flag)?;
    integrity.of_image(image)?.as_ref().map(dto).ok_or_else(|| {
        IpcError::from(aura_core::errors::db::statement_failed(
            "the verdict disappeared between two reads",
            &std::io::Error::from(std::io::ErrorKind::NotFound),
        ))
    })
}

/// Analyse every photograph in a project that has no current verdict.
///
/// The one command here that is a pass rather than a read. Resumable: the work remaining
/// is a query, so a killed run continues where it stopped, and a calibration-table bump
/// makes every affected row pending without anybody triggering anything.
///
/// # Errors
///
/// `AURA-ML-5036` when the calibration table will not load - the one thing in phase 09
/// that halts - or `AURA-DB-3006` when the pending set cannot be read. Per-photograph
/// failures are counted in the report rather than returned.
pub fn analyse_integrity(
    state: &AppState,
    input: &AnalyseIntegrityInput,
) -> IpcResult<IntegrityPassDto> {
    let project = parse_project(&input.project_id)?;
    let pass = state.integrity_pass(&input.project_id)?;
    // The token is registered under the caller's id so `cancel_job` can reach it. A pass
    // that decodes four thousand proxies is the one command on this surface a
    // photographer will want to stop.
    let cancel = CancelToken::new();
    if let Some(id) = input.cancel_id.as_deref() {
        state.register_job(id, cancel.clone());
    }
    let report = pass.run(&project, Priority::AiBatch, &cancel, &NullProgress);
    if let Some(id) = input.cancel_id.as_deref() {
        state.finish_job(id);
    }
    let report = report?;
    Ok(IntegrityPassDto {
        scored: u32::try_from(report.scored).unwrap_or(u32::MAX),
        failed: u32::try_from(report.failed).unwrap_or(u32::MAX),
        faces: u32::try_from(report.faces).unwrap_or(u32::MAX),
        closed: u32::try_from(report.closed).unwrap_or(u32::MAX),
        closed_ok: u32::try_from(report.closed_ok).unwrap_or(u32::MAX),
        mean_score: report.mean_score,
        uncalibrated: report.uncalibrated.clone(),
        elapsed_ms: report.elapsed_ms,
        cancelled: report.cancelled,
    })
}

// -- shapes ------------------------------------------------------------------

fn dto(result: &IntegrityResult) -> IntegrityDto {
    IntegrityDto {
        photo_id: result.image_id.to_db(),
        subject_sharpness: result.subject_sharpness,
        bg_sharpness: result.bg_sharpness,
        focus_offset: result.focus_offset,
        relative_sharpness: result.relative_sharpness,
        motion: result.motion.as_str().to_string(),
        motion_severity: result.motion_severity,
        exposure: result.exposure.as_str().to_string(),
        clip_hi: result.clip_hi,
        clip_lo: result.clip_lo,
        ev_offset: result.ev_offset,
        noise_sigma_rel: result.noise_sigma_rel,
        closed_eye_ratio: result.closed_eye_ratio,
        gating_faces: result.gating_faces,
        technical_score: result.technical_score,
        scene: result.scene.as_str().to_string(),
        flags: result
            .flags
            .set_flags()
            .into_iter()
            .filter_map(|flag| flag.as_str().map(str::to_string))
            .collect(),
        has_defect: result.has_defect(),
        reasons: result.reasons.iter().map(reason_dto).collect(),
        eyes: result.eyes.iter().map(eye_dto).collect(),
        confidence: result.confidence,
        user_reviewed: result.user_reviewed,
        model_ver: result.model_ver,
        analysis_ver: result.analysis_ver,
        calib_ver: result.calib_ver,
    }
}

fn reason_dto(reason: &Reason) -> IntegrityReasonDto {
    IntegrityReasonDto {
        code: reason.code.as_str().to_string(),
        text: reason.text.clone(),
        weight: reason.weight,
        exoneration: reason.code.is_exoneration(),
        evidence: reason.evidence.map(crop_dto),
    }
}

fn eye_dto(eye: &EyeState) -> EyeStateDto {
    EyeStateDto {
        face_id: eye.face_id.to_db(),
        identity_id: eye.identity.map(|id| id.to_db()),
        state: eye.state.as_str().to_string(),
        confidence: eye.confidence,
        intentional: eye.intentional,
        gates: eye.gates,
        area_frac: eye.area_frac,
        crop: crop_dto(eye.crop),
    }
}

fn crop_dto(crop: aura_core::contract::integrity::CropRect) -> CropRectDto {
    CropRectDto {
        x: crop.x,
        y: crop.y,
        w: crop.w,
        h: crop.h,
    }
}

/// The versions this build would produce, for the panel's staleness note.
///
/// A free function rather than a command: the panel already has the stored versions on
/// `IntegrityStatusDto` and needs the current ones to compare, and a round trip for three
/// constants would be a round trip for three constants.
#[must_use]
pub fn current_model_ver() -> u16 {
    MODEL_VER
}

fn parse_project(text: &str) -> IpcResult<ProjectId> {
    ProjectId::from_db(text).map_err(|_| bad_id("project"))
}

fn parse_photo(text: &str) -> IpcResult<ImageId> {
    PhotoId::from_db(text).map_err(|_| bad_id("photo"))
}

/// The refusal every id parse here shares.
///
/// The same shape the other command modules use, gathered into one function because this
/// surface parses three kinds of id and three copies of a five-line closure is three
/// places for the message to drift.
fn bad_id(kind: &str) -> IpcError {
    IpcError::from(aura_core::errors::db::statement_failed(
        format!("{kind} id was not a valid AURA id"),
        &std::io::Error::from(std::io::ErrorKind::InvalidInput),
    ))
}
