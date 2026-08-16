//! The composition command surface.
//!
//! Five commands, and the boundary is deliberate. Three read stored framing
//! judgements, one records that a photographer disagrees with a single mark, and
//! one runs the resumable project pass. None crops, straightens, removes a
//! distraction, keeps or rejects a photograph: phases 12, 23 and 24 own those
//! actions.
//!
//! The conversion functions in this module are also the boundary between the
//! frozen Rust vocabulary and the web view. They send the backend's answers about
//! exonerations, flag thresholds and actionable crop hints rather than asking the
//! interface to recreate them.

use aura_core::contract::composition::{
    CompositionFlags, CompositionReason, CompositionResult, CompositionService, CropHint, ImageId,
    JointCut,
};
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{PhotoId, Priority, ProjectId};

use crate::commands::IpcResult;
use crate::contract::ipc::{
    AnalyseCompositionInput, CompositionCropHintDto, CompositionDto, CompositionJointCutDto,
    CompositionPassDto, CompositionReasonDto, CompositionStatusDto, CropRectDto,
    DismissCompositionFlagInput, FlaggedCompositionInput, IpcError,
};
use crate::state::AppState;

/// What the Composition panel's project header shows.
///
/// # Errors
///
/// `AURA-DB-3006` when the stored judgements cannot be read.
pub fn composition_status(state: &AppState, project_id: &str) -> IpcResult<CompositionStatusDto> {
    let project = parse_project(project_id)?;
    let outline = state.composition().outline(project)?;
    Ok(CompositionStatusDto {
        photos: outline.photos,
        scored: outline.scored,
        coverage: outline.coverage,
        keypoint_aware: outline.keypoint_aware,
        flag_counts: outline.flag_histogram.to_vec(),
        flag_names: CompositionFlags::ALL
            .iter()
            .map(|flag| flag.as_str().unwrap_or("").to_string())
            .collect(),
        mean_score: outline.mean_score,
        mean_abs_tilt: outline.mean_abs_tilt,
        intentional_ratio: outline.intentional_ratio,
        hinted: outline.hinted,
        reviewed: outline.reviewed,
        violating_at_most: outline.violating_at_most(),
        unruled_scenes: outline.unruled_scenes,
        model_ver: outline.model_ver,
        analysis_ver: outline.analysis_ver,
        rules_ver: outline.rules_ver,
    })
}

/// One photograph's framing judgement for the Explain panel.
///
/// `None` means nobody has analysed the frame. It is not a clean judgement and
/// the card renders it separately.
///
/// # Errors
///
/// `AURA-DB-3006` when the judgement cannot be read.
pub fn image_composition(state: &AppState, photo_id: &str) -> IpcResult<Option<CompositionDto>> {
    let image = parse_photo(photo_id)?;
    Ok(state.composition().of_image(image)?.as_ref().map(dto))
}

/// Frames carrying any of the requested composition flags, worst score first.
///
/// This is a review queue, not a cull. Unknown slugs contribute no bits and an
/// entirely unknown request therefore returns an empty queue rather than every
/// photograph in the wedding.
///
/// # Errors
///
/// `AURA-DB-3006` when the set cannot be read.
pub fn flagged_composition(
    state: &AppState,
    input: &FlaggedCompositionInput,
) -> IpcResult<Vec<String>> {
    let project = parse_project(&input.project_id)?;
    let mut flags = CompositionFlags::EMPTY;
    for slug in &input.flags {
        flags = flags.union(CompositionFlags::from_str_or_empty(slug));
    }
    let limit = input.limit.unwrap_or(200).clamp(1, 5_000) as usize;
    Ok(state
        .composition()
        .flagged(project, flags, limit)?
        .into_iter()
        .map(|id| id.to_db())
        .collect())
}

/// Record that one composition mark is wrong.
///
/// The backend refuses non-violation flags, combinations and marks that are not
/// currently present. A successful dismissal changes the explanation and review
/// queue only; it does not crop the photograph or decide its delivery.
///
/// # Errors
///
/// `AURA-ML-5044` when the dismissal is not a single present violation, or
/// `AURA-DB-3006` when the row cannot be updated.
pub fn dismiss_composition_flag(
    state: &AppState,
    input: &DismissCompositionFlagInput,
) -> IpcResult<CompositionDto> {
    let image = parse_photo(&input.photo_id)?;
    let flag = CompositionFlags::from_str_or_empty(&input.flag);
    let composition = state.composition();
    composition.dismiss(image, flag)?;
    composition
        .of_image(image)?
        .as_ref()
        .map(dto)
        .ok_or_else(|| disappeared("the composition judgement disappeared between two reads"))
}

/// Analyse every photograph in a project that has no current-version judgement.
///
/// Completed rows are durable. A cancelled pass reports `cancelled: true`; its
/// next run resumes from the catalog query rather than recomputing those rows.
///
/// # Errors
///
/// `AURA-ML-5046` when the rule table cannot load, or the typed error raised while
/// building the preview/inference services or reading the pending set.
pub fn analyse_composition(
    state: &AppState,
    input: &AnalyseCompositionInput,
) -> IpcResult<CompositionPassDto> {
    let project = parse_project(&input.project_id)?;
    let pass = state.composition_pass(&input.project_id)?;
    let cancel = CancelToken::new();
    if let Some(id) = input.cancel_id.as_deref() {
        state.register_job(id, cancel.clone());
    }
    let report = pass.run(&project, Priority::AiBatch, &cancel, &NullProgress);
    if let Some(id) = input.cancel_id.as_deref() {
        state.finish_job(id);
    }
    let report = report?;
    Ok(CompositionPassDto {
        scored: saturating_u32(report.scored),
        failed: saturating_u32(report.failed),
        keypoint_subjects: saturating_u32(report.keypoint_subjects),
        cut: saturating_u32(report.cut),
        intentional_tilts: saturating_u32(report.intentional_tilts),
        horizons: saturating_u32(report.horizons),
        mean_abs_tilt: report.mean_abs_tilt(),
        flag_counts: report.flag_histogram.to_vec(),
        flag_names: CompositionFlags::ALL
            .iter()
            .map(|flag| flag.as_str().unwrap_or("").to_string())
            .collect(),
        mean_score: report.mean_score,
        hinted: saturating_u32(report.hinted),
        unruled_scenes: report.unruled,
        elapsed_ms: report.elapsed_ms,
        cancelled: report.cancelled,
    })
}

// -- wire shapes ------------------------------------------------------------

fn dto(result: &CompositionResult) -> CompositionDto {
    CompositionDto {
        photo_id: result.image_id.to_db(),
        tilt_deg: result.tilt_deg,
        tilt_intentional: result.tilt_intentional,
        horizon_conf: result.horizon_conf,
        horizon_source: result.horizon_source.as_str().to_string(),
        headroom: result.headroom,
        thirds_offset: result.thirds_offset,
        balance: result.balance,
        negative_space: result.negative_space,
        joint_cuts: result.joint_cuts.iter().map(joint_cut_dto).collect(),
        head_crop: result.head_crop,
        edge_intrusions: result
            .edge_intrusions
            .iter()
            .copied()
            .map(crop_dto)
            .collect(),
        clutter: result.clutter,
        bright_blobs: result.bright_blobs.iter().copied().map(crop_dto).collect(),
        head_merge: result.head_merge,
        colour_competition: result.colour_competition,
        aesthetic: result.aesthetic,
        composition_score: result.composition_score,
        crop_suggestion_hint: result.crop_suggestion_hint.map(crop_hint_dto),
        scene: result.scene.as_str().to_string(),
        relative_composition: result.relative_composition,
        keypoint_subjects: result.keypoint_subjects,
        flags: result
            .flags
            .set_flags()
            .into_iter()
            .filter_map(|flag| flag.as_str().map(str::to_string))
            .collect(),
        has_violation: result.has_violation(),
        reasons: result.reasons.iter().map(reason_dto).collect(),
        confidence: result.confidence,
        user_reviewed: result.user_reviewed,
        model_ver: result.model_ver,
        analysis_ver: result.analysis_ver,
        rules_ver: result.rules_ver,
    }
}

fn joint_cut_dto(cut: &JointCut) -> CompositionJointCutDto {
    CompositionJointCutDto {
        joint: cut.joint.as_str().to_string(),
        edge: cut.edge.as_str().to_string(),
        at_joint: cut.at_joint,
        severity: cut.severity,
        flagged: cut.is_flagged(),
        area: crop_dto(cut.area),
    }
}

fn reason_dto(reason: &CompositionReason) -> CompositionReasonDto {
    CompositionReasonDto {
        code: reason.code.as_str().to_string(),
        text: reason.text.clone(),
        weight: reason.weight,
        exoneration: reason.code.is_exoneration(),
        evidence: reason.evidence.map(crop_dto),
    }
}

fn crop_hint_dto(hint: CropHint) -> CompositionCropHintDto {
    CompositionCropHintDto {
        region: crop_dto(hint.region),
        safe_margin: hint.safe_margin,
        straighten_deg: hint.straighten_deg,
        confidence: hint.confidence,
        actionable: hint.is_actionable(),
    }
}

fn crop_dto(crop: aura_core::CropRect) -> CropRectDto {
    CropRectDto {
        x: crop.x,
        y: crop.y,
        w: crop.w,
        h: crop.h,
    }
}

fn parse_project(text: &str) -> IpcResult<ProjectId> {
    ProjectId::from_db(text).map_err(|_| bad_id("project"))
}

fn parse_photo(text: &str) -> IpcResult<ImageId> {
    PhotoId::from_db(text).map_err(|_| bad_id("photo"))
}

fn bad_id(kind: &str) -> IpcError {
    IpcError::from(aura_core::errors::db::statement_failed(
        format!("{kind} id was not a valid AURA id"),
        &std::io::Error::from(std::io::ErrorKind::InvalidInput),
    ))
}

fn disappeared(detail: &str) -> IpcError {
    IpcError::from(aura_core::errors::db::statement_failed(
        detail,
        &std::io::Error::from(std::io::ErrorKind::NotFound),
    ))
}

fn saturating_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
