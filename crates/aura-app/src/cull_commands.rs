//! The culling command surface.
//!
//! Seven commands, and the boundary is the most consequential one in the product. Three
//! read the stored decision, three change it, and one runs the cull. **None of them
//! deletes, moves, exports, uploads or edits a photograph**: phase 14 edits the survivors,
//! phase 27 swaps in runner-ups, phase 29 builds albums and phase 30 delivers. There is no
//! command here that could take a rejected frame off a disk, and there is nowhere in
//! migration 12 to record that one had been.
//!
//! ## What crosses the boundary, and what deliberately does not
//!
//! Every field of `Selected`, `Rejected` and `CoverageReport` crosses, plus the backend's
//! own answers about them: whether a reason is a keep, whether it was a veto, whether a
//! guarantee is satisfied, whether a keeper is protected from the slider. The interface
//! draws those; it does not keep a second copy of the vocabulary.
//!
//! That matters more here than on any earlier surface. A web view that decided for itself
//! what `covered_weak` meant would be a web view that could tell a photographer their
//! gallery was complete when the engine had said it was not.
//!
//! What does not cross is the four sub-scores of a *rejected* frame. They are in the
//! catalog and the Explain panel of phase 13 will show them; a rejection drawer that led
//! with "technical 0.31" would be inviting an argument about a number instead of an
//! argument about a photograph, and section 12's fourth failure mode is users distrusting
//! automation.

use std::time::Instant;

use aura_core::contract::cull::{
    Coverage, CoverageReport, CullCode, CullMode, CullReason, CullService, Decision, ImageId,
    Rejected, Selected, SelectionResult,
};
use aura_core::{ChapterId, PhotoId, ProjectId};

use crate::commands::IpcResult;
use crate::contract::ipc::{
    ChapterCountDto, CoverageReportDto, CoverageRuleDto, CullPassDto, CullProjectInput,
    CullReasonDto, CullStatusDto, DecisionDto, IdentityCoverageDto, IpcError,
    OverrideDecisionInput, RejectedDto, ResizeGalleryInput, SelectedDto, SelectionDto,
    SetCullModeInput,
};
use crate::state::AppState;

/// What the culling view's header shows.
///
/// # Errors
///
/// `AURA-DB-3006` when the stored selection cannot be read.
pub fn cull_status(state: &AppState, project_id: &str) -> IpcResult<CullStatusDto> {
    let project = parse_project(project_id)?;
    let outline = state.cull()?.outline(project)?;
    Ok(CullStatusDto {
        photos: outline.photos,
        eligible: outline.eligible,
        selected: outline.selected,
        coverage: outline.coverage,
        emotion_aware: outline.emotion_aware,
        composition_aware: outline.composition_aware,
        grouped: outline.grouped,
        covered: outline.covered,
        covered_weak: outline.covered_weak,
        missing: outline.missing,
        user_kept: outline.user_kept,
        user_rejected: outline.user_rejected,
        mode: outline.mode.as_str().to_string(),
        deterministic_hash: format!("{:016x}", outline.deterministic_hash),
        model_ver: outline.model_ver,
        analysis_ver: outline.analysis_ver,
        calibration_ver: outline.calibration_ver,
    })
}

/// The stored gallery, or `null` when the project has never been culled.
///
/// `null` means **nobody has decided**. It is not an empty gallery, and no caller may
/// render it as one - phase 30 in particular must never read it as "deliver nothing".
///
/// # Errors
///
/// `AURA-DB-3006` when the selection cannot be read.
pub fn gallery(state: &AppState, project_id: &str) -> IpcResult<Option<SelectionDto>> {
    let project = parse_project(project_id)?;
    Ok(state
        .cull()?
        .selection(project)?
        .as_ref()
        .map(selection_dto))
}

/// What was decided about one photograph, and why.
///
/// # Errors
///
/// `AURA-DB-3006` when the decision cannot be read.
pub fn image_decision(state: &AppState, photo_id: &str) -> IpcResult<Option<DecisionDto>> {
    let image = parse_photo(photo_id)?;
    Ok(state.cull()?.of_image(image)?.as_ref().map(decision_dto))
}

/// Run or re-run the cull over a project.
///
/// # Errors
///
/// `AURA-ML-5051` or `AURA-ML-5052` when a configuration table will not load, and whatever
/// the gatherer or the store raised.
pub fn cull_project(state: &AppState, input: &CullProjectInput) -> IpcResult<CullPassDto> {
    let project = parse_project(&input.project_id)?;
    let service = state.cull()?;
    let mode = match input.mode.as_deref() {
        Some(text) => CullMode::from_str_or_balanced(text),
        None => service.outline(project)?.mode,
    };
    // `elapsed_ms` is reported to the panel and never read by the engine; the selection's
    // own hash is computed over inputs and configuration, so no gallery can differ because
    // one run was slower than another.
    let started = Instant::now(); // DETERMINISM: telemetry only, never an input to a decision
    let (_, report) = service.run(project, mode, input.target)?;
    let elapsed = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    Ok(CullPassDto {
        photos: saturating_u32(report.photos),
        eligible: saturating_u32(report.eligible),
        selected: saturating_u32(report.selected),
        veto_counts: report.vetoes.to_vec(),
        veto_names: CullCode::VETOES
            .iter()
            .map(|code| code.as_str().to_string())
            .collect(),
        swaps: saturating_u32(report.swaps),
        coverage_added: saturating_u32(report.coverage_added),
        diversity_dropped: saturating_u32(report.diversity_dropped),
        size_added: saturating_u32(report.size_added),
        size_trimmed: saturating_u32(report.size_trimmed),
        peaks_rejected: saturating_u32(report.peaks_rejected),
        coverage_weak: saturating_u32(report.coverage_weak),
        coverage_missing: saturating_u32(report.coverage_missing),
        unweighted_scenes: report.unweighted_scenes,
        elapsed_ms: elapsed,
    })
}

/// Move the size slider.
///
/// The returned gallery may exceed the requested target: the coverage guard runs last and
/// a guarantee outranks a slider. Section 6.4, and the panel shows both numbers so the
/// difference is visible rather than surprising.
///
/// # Errors
///
/// `AURA-ML-5049` when the project has not been culled yet.
pub fn resize_gallery(state: &AppState, input: &ResizeGalleryInput) -> IpcResult<SelectionDto> {
    let project = parse_project(&input.project_id)?;
    let result = state.cull()?.resize(project, input.target)?;
    Ok(selection_dto(&result))
}

/// Switch autonomy mode.
///
/// # Errors
///
/// `AURA-ML-5049` when the project has not been culled yet.
pub fn set_cull_mode(state: &AppState, input: &SetCullModeInput) -> IpcResult<SelectionDto> {
    let project = parse_project(&input.project_id)?;
    let mode = CullMode::from_str_or_balanced(&input.mode);
    let result = state.cull()?.set_mode(project, mode)?;
    Ok(selection_dto(&result))
}

/// Keep or remove one photograph by hand, or withdraw an earlier choice.
///
/// Unbeatable and re-applied onto every fresh selection. A forced reject **can** leave a
/// guarantee short: when that happens the coverage report degrades the rule and writes a
/// warning naming the override, because silently overruling the photographer and silently
/// losing the vows are both failures and only one of them is recoverable by saying so.
///
/// # Errors
///
/// `AURA-ML-5049` when the photograph is unknown, its project has not been culled, or a
/// `clear` was asked for where no choice was recorded.
pub fn override_decision(
    state: &AppState,
    input: &OverrideDecisionInput,
) -> IpcResult<DecisionDto> {
    let image = parse_photo(&input.photo_id)?;
    let service = state.cull()?;
    match input.action.as_str() {
        "keep" => service.override_decision(image, true)?,
        "reject" => service.override_decision(image, false)?,
        "clear" => service.clear_override(image)?,
        other => {
            return Err(IpcError::from(aura_core::errors::db::statement_failed(
                format!("`{other}` is not keep, reject or clear"),
                &std::io::Error::from(std::io::ErrorKind::InvalidInput),
            )))
        }
    }
    service
        .of_image(image)?
        .as_ref()
        .map(decision_dto)
        .ok_or_else(|| disappeared("the decision disappeared between two reads"))
}

// -- wire shapes ------------------------------------------------------------

fn selection_dto(result: &SelectionResult) -> SelectionDto {
    SelectionDto {
        selected: result.selected.iter().map(selected_dto).collect(),
        rejected: result.rejected.iter().map(rejected_dto).collect(),
        coverage: coverage_dto(&result.coverage),
        target_count: result.target_count,
        actual_count: result.actual_count,
        mode: result.mode.as_str().to_string(),
        deterministic_hash: format!("{:016x}", result.deterministic_hash),
        model_ver: result.model_ver,
        analysis_ver: result.analysis_ver,
        calibration_ver: result.calibration_ver,
    }
}

fn selected_dto(keep: &Selected) -> SelectedDto {
    SelectedDto {
        photo_id: keep.image_id.to_db(),
        moment_id: keep.moment_id.map(|id| id.to_db()),
        keep_score: keep.keep_score,
        confidence: keep.confidence,
        reasons: keep.reasons.iter().map(reason_dto).collect(),
        runner_up: keep.runner_up.map(|id| id.to_db()),
        coverage_role: keep.coverage_role.map(|rule| rule.as_str().to_string()),
        protected: keep.is_protected(),
    }
}

fn rejected_dto(drop: &Rejected) -> RejectedDto {
    RejectedDto {
        photo_id: drop.image_id.to_db(),
        moment_id: drop.moment_id.map(|id| id.to_db()),
        keep_score: drop.keep_score,
        reasons: drop.reasons.iter().map(reason_dto).collect(),
        kept_instead: drop.kept_instead.map(|id| id.to_db()),
        was_peak: drop.was_peak(),
        vetoed: drop.is_vetoed(),
    }
}

fn reason_dto(reason: &CullReason) -> CullReasonDto {
    CullReasonDto {
        code: reason.code.as_str().to_string(),
        text: reason.text.clone(),
        weight: reason.weight,
        keep: reason.code.is_keep(),
        veto: reason.code.is_veto(),
    }
}

fn coverage_dto(report: &CoverageReport) -> CoverageReportDto {
    CoverageReportDto {
        must_haves: report
            .must_haves
            .iter()
            .map(|(rule, state)| CoverageRuleDto {
                rule: rule.as_str().to_string(),
                title: rule.title().to_string(),
                state: state.as_str().to_string(),
                satisfied: state.is_satisfied(),
            })
            .collect(),
        identity_coverage: report
            .identity_coverage
            .iter()
            .map(|(id, frames)| IdentityCoverageDto {
                identity_id: id.to_db(),
                frames: *frames,
            })
            .collect(),
        chapters: report
            .chapter_counts
            .iter()
            .map(|(chapter, delivered, target)| ChapterCountDto {
                chapter: chapter.as_str().to_string(),
                title: chapter_title(*chapter).to_string(),
                delivered: *delivered,
                target: *target,
            })
            .collect(),
        warnings: report.warnings.clone(),
    }
}

fn decision_dto(decision: &Decision) -> DecisionDto {
    match decision {
        Decision::Keep(keep) => DecisionDto {
            kept: true,
            selected: Some(selected_dto(keep)),
            rejected: None,
        },
        Decision::Drop(drop) => DecisionDto {
            kept: false,
            selected: None,
            rejected: Some(rejected_dto(drop)),
        },
    }
}

/// The words the chapter panel shows.
///
/// `ChapterId::title` already exists in the frozen contract; this exists because the DTO
/// carries the title rather than making the web view keep a second table of nine strings
/// that a tenth chapter would silently break.
fn chapter_title(chapter: ChapterId) -> &'static str {
    chapter.title()
}

/// Assert at compile time that nothing here can express a deletion.
///
/// A const rather than a test: `Coverage` has three states and none of them is "removed",
/// and if a fourth is ever added this line is where somebody has to think about whether the
/// culling surface should be able to say it.
const _: () = assert!(Coverage::ALL.len() == 3);

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
