//! The multi-camera matching command surface. PHASE-26.
//!
//! Ten commands. Six read - the project header, the per-camera report, the corrections, the
//! fingerprints, one camera's matched pairs and the shooter habits - one runs the pass, and three
//! record what the photographer decided. ADR-0054 records the shape and what is deliberately absent
//! from it.
//!
//! # What this surface does that no earlier command surface does
//!
//! **It answers about a camera rather than about a photograph or a wedding.** Phase 25's surface
//! answers "what is happening to these four hundred frames"; this one answers "what is happening to
//! everything that came out of this body, and on what evidence". Two consequences follow.
//!
//! The first is that **the evidence travels beside every number**. `camera_transforms` sends
//! `source`, `blend`, `evidencePairs`, `heldoutPairs` and both held-out distances, because a body
//! corrected by 300 K from twenty pairs of its own ceremony and a body corrected by 300 K from a
//! bundled brand setting are the same arithmetic and completely different claims. `camera_reports`
//! is the same facts rendered into sentences, and it leads with the evidence rather than with the
//! correction.
//!
//! The second is that **`baselinesMeasured` is on the wire**. It is false in this build. A panel
//! that had to infer it would eventually present a fabricated brand fallback as a laboratory
//! measurement - phase 24 put `detectorTrained` on the wire for the same reason and phase 25
//! `skinFieldAvailable`.
//!
//! # What is not here, and cannot be added without an ADR
//!
//! No strength, no share, no way to raise a bound, no pixels, no apply, and no way to ask for a
//! correction larger than the frozen contract's ceilings. See the header of the DTO block in
//! `contract::ipc` and section 8 of ADR-0054.

use aura_brain_gallery::camera::{report, CameraMatching};
use aura_core::contract::camera::{
    CameraCode, CameraMatchService, CameraOverride, CameraReason, CameraTransform, FlashState,
    MatchedPair, ShooterBias,
};
use aura_core::contract::moment::CameraId;
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::ProjectId;

use crate::commands::IpcResult;
use crate::contract::ipc::{
    CameraFingerprintDto, CameraOverrideInput, CameraPassDto, CameraPassInput, CameraReasonDto,
    CameraReportDto, CameraStatusDto, CameraTransformDto, DisableCameraInput, IpcError,
    MatchedPairDto, SetCameraReferenceInput, ShooterBiasDto,
};
use crate::state::AppState;

/// What the Camera Match panel's project header shows.
///
/// # Errors
///
/// `AURA-DB-3006` when the stored rows cannot be read.
pub fn camera_status(state: &AppState, project_id: &str) -> IpcResult<CameraStatusDto> {
    let project = parse_project(project_id)?;
    let matching = state.camera_matching();
    let outline = matching.outline(project)?;
    Ok(CameraStatusDto {
        photos: outline.photos,
        matched: outline.matched,
        coverage: outline.coverage,
        cameras: outline.cameras,
        fingerprinted: outline.fingerprinted,
        solved_from_pairs: outline.solved_from_pairs,
        blended: outline.blended,
        baseline_only: outline.baseline_only,
        pairs: outline.pairs,
        pairs_rejected: outline.pairs_rejected,
        heldout_pairs: outline.heldout_pairs,
        flash_separated: outline.flash_separated,
        shooters_measured: outline.shooters_measured,
        shooters_capped: outline.shooters_capped,
        disabled: outline.disabled,
        user_edited: outline.user_edited,
        skin_de00_before: outline.skin_de00_before,
        skin_de00_after: outline.skin_de00_after,
        worst_skin_de00: outline.worst_skin_de00,
        reference_id: outline.reference.as_ref().map(|id| id.as_str().to_string()),
        reference_source: outline.reference_source.as_str().to_string(),
        unknown_brands: outline.unknown_brands.clone(),
        // On the wire rather than inferred, for the reason phase 24 put `detectorTrained` there: a
        // panel that guessed would eventually render a fabricated brand fallback as a measurement.
        baselines_measured: matching.library().any_measured(),
        skin_field_available: aura_brain_gallery::SKIN_FIELD_AVAILABLE,
        policy_ver: outline.policy_ver,
    })
}

/// Every body's correction, by body and then by flash state.
///
/// # Errors
///
/// `AURA-DB-3006` when the transforms cannot be read.
pub fn camera_transforms(state: &AppState, project_id: &str) -> IpcResult<Vec<CameraTransformDto>> {
    let project = parse_project(project_id)?;
    Ok(state
        .camera_matching()
        .transforms(project)?
        .iter()
        .map(to_transform_dto)
        .collect())
}

/// Every body's colour response, by body and then by flash state.
///
/// # Errors
///
/// `AURA-DB-3006` when the fingerprints cannot be read.
pub fn camera_fingerprints(
    state: &AppState,
    project_id: &str,
) -> IpcResult<Vec<CameraFingerprintDto>> {
    let project = parse_project(project_id)?;
    Ok(state
        .camera_matching()
        .fingerprints(project)?
        .into_iter()
        .map(|print| CameraFingerprintDto {
            camera_id: print.camera_id.as_str().to_string(),
            flash: print.flash.as_str().to_string(),
            brand: print.brand.as_str().to_string(),
            skin_chroma: print.skin_chroma,
            white_point: print.white_point,
            highlight_rolloff: print.highlight_rolloff,
            subject_luma: print.subject_luma,
            samples: print.samples,
            confidence: print.confidence,
            reasons: print.reasons.iter().map(to_reason_dto).collect(),
        })
        .collect())
}

/// The per-camera report, worst evidence first.
///
/// **The one command that answers section 13's third acceptance criterion** - "the per-camera report
/// explains what was corrected and on what evidence" - and it is a command rather than a panel
/// concern so that the CLI gate, the exit report and the window all render the same sentences.
///
/// # Errors
///
/// `AURA-DB-3006` when the rows cannot be read.
pub fn camera_reports(state: &AppState, project_id: &str) -> IpcResult<Vec<CameraReportDto>> {
    let project = parse_project(project_id)?;
    let matching = state.camera_matching();
    let transforms = matching.transforms(project)?;
    let fingerprints = matching.fingerprints(project)?;
    let bias = matching.shooter_bias(project)?;
    let reference = matching.reference(project)?;
    let labels: Vec<(CameraId, String)> = bias
        .iter()
        .map(|row| (row.camera_id.clone(), row.shooter.clone()))
        .collect();
    Ok(report::of_project(
        &transforms,
        &fingerprints,
        &bias,
        reference.as_ref(),
        &labels,
    )
    .into_iter()
    .map(|row| CameraReportDto {
        camera_id: row.camera_id.as_str().to_string(),
        flash: row.flash.as_str().to_string(),
        shooter: row.shooter.clone(),
        is_reference: row.is_reference,
        headline: row.headline().to_string(),
        evidence: row.evidence.clone(),
        corrections: row.corrections.clone(),
        withdrawals: row.withdrawals.clone(),
        skin_de00_after: row.skin_de00_after,
        meets_promise: row.meets_promise,
        magnitude: row.magnitude,
        confidence: row.confidence,
    })
    .collect())
}

/// The matched pairs behind one body's correction, best first.
///
/// What the matched-pair viewer shows. `limit` bounds the answer because the viewer is a page, and
/// **rejected pairs come back too**: "both cameras shot the whole ceremony and AURA still used a
/// brand baseline" is answered by a list of candidate pairs whose backgrounds disagreed, and by
/// nothing else.
///
/// # Errors
///
/// `AURA-DB-3006` when the pairs cannot be read.
pub fn camera_pairs(
    state: &AppState,
    project_id: &str,
    camera_id: &str,
    limit: usize,
) -> IpcResult<Vec<MatchedPairDto>> {
    let project = parse_project(project_id)?;
    let camera = CameraId::new(camera_id);
    Ok(state
        .camera_matching()
        .pairs(project, &camera, limit.clamp(1, 200))?
        .iter()
        .map(to_pair_dto)
        .collect())
}

/// Every measured exposure habit in a project.
///
/// # Errors
///
/// `AURA-DB-3006` when the rows cannot be read.
pub fn camera_shooter_bias(state: &AppState, project_id: &str) -> IpcResult<Vec<ShooterBiasDto>> {
    let project = parse_project(project_id)?;
    Ok(state
        .camera_matching()
        .shooter_bias(project)?
        .iter()
        .map(to_bias_dto)
        .collect())
}

/// Match a project's cameras to each other.
///
/// # Errors
///
/// `AURA-ML-5130` when the pass cannot run; `AURA-ML-5133` when a studio's policy table is refused;
/// `AURA-DB-3006` when a statement fails.
pub fn camera_pass(state: &AppState, input: CameraPassInput) -> IpcResult<CameraPassDto> {
    let project = parse_project(&input.project_id)?;
    let frames = state.camera_frames(&project)?;
    let pass = state.camera_pass();
    // No skin loci: `SKIN_FIELD_AVAILABLE` is false in this build, so section 6.2's hard constraint
    // admits every candidate and says so through `CameraCode::FingerprintThin`. A constraint that
    // never fired and a constraint that never ran are different facts.
    let report = pass.run(project, &frames, &[], &NullProgress, &CancelToken::new())?;
    let outline = state.camera_matching().outline(project)?;
    Ok(CameraPassDto {
        cameras: report.cameras,
        reference_id: report.reference.as_ref().map(|id| id.as_str().to_string()),
        reference_source: report.reference_source.as_str().to_string(),
        pairs: report.pairs,
        pairs_rejected: report.pairs_rejected,
        heldout_pairs: report.heldout_pairs,
        solved: report.solved,
        blended: report.blended,
        baseline_only: report.baseline_only,
        heldout_failures: report.heldout_failures,
        distance_before: report.distance_before,
        distance_after: report.distance_after,
        signature_before: report.signature_before,
        signature_after: report.signature_after,
        worst_skin_de00: report.worst_skin_de00,
        shooters_measured: report.shooters_measured,
        shooters_capped: report.shooters_capped,
        summary: report::summary(&outline),
    })
}

/// Choose the body everything else is matched to, and re-solve against it.
///
/// # Errors
///
/// `AURA-ML-5131` when the body is unknown to the project or shot no measurable photographs.
pub fn set_camera_reference(state: &AppState, input: SetCameraReferenceInput) -> IpcResult<()> {
    let project = parse_project(&input.project_id)?;
    let camera = CameraId::new(input.camera_id);
    state.camera_matching().set_reference(project, &camera)?;
    Ok(())
}

/// Switch matching off for one body, or back on.
///
/// # Errors
///
/// `AURA-ML-5131` when the body is unknown to the project.
pub fn disable_camera(state: &AppState, input: DisableCameraInput) -> IpcResult<()> {
    let project = parse_project(&input.project_id)?;
    let camera = CameraId::new(input.camera_id);
    state
        .camera_matching()
        .set_enabled(project, &camera, !input.disabled)?;
    Ok(())
}

/// Record what the photographer set instead, for one body in one flash state.
///
/// # Errors
///
/// `AURA-ML-5131` when the body has no transform, when the override is empty, or when a value is
/// outside its documented bound. **Refused rather than clamped**: a camera that needs to move
/// further is a camera whose per-frame estimates are wrong.
pub fn set_camera_override(state: &AppState, input: CameraOverrideInput) -> IpcResult<()> {
    let project = parse_project(&input.project_id)?;
    let camera = CameraId::new(input.camera_id);
    let flash = FlashState::from_str_or_ambient(&input.flash);
    let values = CameraOverride {
        d_cct: input.d_cct,
        d_tint: input.d_tint,
        d_exposure: input.d_exposure,
        d_saturation: input.d_saturation,
    };
    state
        .camera_matching()
        .set_override(project, &camera, flash, values)?;
    Ok(())
}

/// Every reason code this build can emit, with its sentence.
///
/// What a filter chip list is built from, and what `docs/camera-matching.md` is checked against.
/// The panel never hard-codes a slug.
#[must_use]
pub fn camera_reason_codes() -> Vec<CameraReasonDto> {
    CameraCode::ALL
        .into_iter()
        .map(|code| CameraReasonDto {
            code: code.as_str().to_string(),
            text: code.user_text().to_string(),
            withdraws: code.withdraws(),
        })
        .collect()
}

fn to_transform_dto(transform: &CameraTransform) -> CameraTransformDto {
    CameraTransformDto {
        camera_id: transform.camera_id.as_str().to_string(),
        flash: transform.flash.as_str().to_string(),
        reference_id: transform.reference.as_str().to_string(),
        d_cct: transform.d_cct,
        d_tint: transform.d_tint,
        d_exposure: transform.d_exposure,
        d_saturation: transform.d_saturation,
        channel_gain: transform.channel_gain,
        contrast_shape: transform.contrast_shape,
        skin_de00_before: transform.skin_correction.de00_before,
        skin_de00_after: transform.skin_correction.de00_after,
        skin_capped: transform.skin_correction.capped,
        skin_locus_valid: transform.skin_correction.locus_valid,
        source: transform.source.as_str().to_string(),
        blend: transform.blend,
        evidence_pairs: transform.evidence_pairs,
        heldout_pairs: transform.heldout_pairs,
        heldout_before: transform.heldout_before.total(),
        heldout_after: transform.heldout_after.total(),
        // `None` is the third state and it is on the wire as one: "we could not check" is not
        // "we checked and it failed", and collapsing them is how a product claims a verification
        // it did not do.
        heldout_improved: transform.heldout_improved(),
        bounded_by: transform.bounded.map(|bound| bound.as_str().to_string()),
        magnitude: transform.magnitude(),
        confidence: transform.confidence,
        enabled: transform.enabled,
        user_edited: transform.user_edited,
        reasons: transform.reasons.iter().map(to_reason_dto).collect(),
    }
}

fn to_pair_dto(pair: &MatchedPair) -> MatchedPairDto {
    MatchedPairDto {
        pair_id: pair.id.to_db(),
        left_photo_id: pair.left.to_db(),
        right_photo_id: pair.right.to_db(),
        flash: pair.flash.as_str().to_string(),
        gap_ms: pair.gap_ms,
        subject_similarity: pair.subject_similarity,
        background_agreement: pair.background_agreement,
        verified: pair.verified,
        held_out: pair.held_out,
    }
}

fn to_bias_dto(row: &ShooterBias) -> ShooterBiasDto {
    ShooterBiasDto {
        shooter: row.shooter.clone(),
        camera_id: row.camera_id.as_str().to_string(),
        scene: row.scene.as_str().to_string(),
        measured_ev: row.measured_ev,
        applied_ev: row.applied_ev,
        frames: row.frames,
        capped: row.capped,
        reasons: row.reasons.iter().map(to_reason_dto).collect(),
    }
}

fn to_reason_dto(reason: &CameraReason) -> CameraReasonDto {
    CameraReasonDto {
        code: reason.code.as_str().to_string(),
        text: reason.text.clone(),
        withdraws: reason.code.withdraws(),
    }
}

/// An id the window sent that this build cannot parse.
///
/// `AURA-ML-5131` rather than a generic bad-request, because every id on this surface arrives from a
/// row this same surface handed out - so an unparseable one is the panel and the catalog disagreeing
/// rather than a person typing something wrong.
fn refused(detail: impl Into<String>) -> IpcError {
    IpcError::from(aura_core::errors::ml::camera_decision_refused(detail))
}

fn parse_project(id: &str) -> IpcResult<ProjectId> {
    ProjectId::from_db(id).map_err(|_| refused(format!("`{id}` is not a project id")))
}

/// The service, for a caller that already has a state and wants the trait rather than the wire.
#[must_use]
pub fn service(state: &AppState) -> std::sync::Arc<CameraMatching> {
    state.camera_matching()
}
