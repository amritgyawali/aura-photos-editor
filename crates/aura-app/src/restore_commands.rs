//! The restoration command surface. PHASE-22.
//!
//! Seven commands. Four read - the project coverage, one photograph's plan, the identity
//! refusals and the review queue - one runs the resumable pass, and two record what the
//! photographer decided. ADR-0048 records the shape and what is deliberately absent from it.
//!
//! # What this module does that the micro-retouch one does not
//!
//! **Its override carries a number.** Phase 21's matrix was switches all the way down and
//! ADR-0046 could say there was no strength field anywhere. Here there is a genuine four-way
//! choice, because "how much noise reduction" is a question a photographer legitimately has an
//! opinion about and "how much may AURA whiten teeth" is not. The line is between *which of four*
//! and *how far each goes*: a tier is on the wire and `DenoiseSpec`'s three amounts are not,
//! because they are what the tier becomes under one sensor at one ISO.
//!
//! **It publishes what was declined.** `restore_identity_refusals` exists on the frozen service
//! and on this surface because section 10.1 gates identity preservation at 100 %, and a gate that
//! can only be checked by opening four hundred plans one at a time is a gate nobody checks. The
//! same view backs the panel, the delivery report and phase 27's QC agent, so no two of them can
//! disagree about which faces AURA declined to change.
//!
//! **It names the cameras it could not measure.** `RestoreStatusDto::unmeasured_cameras` is the
//! field on this surface most likely to be deleted by somebody tidying up, and it is the one a
//! photographer can act on: every noise model in this build is synthetic, so a studio that sees
//! its main body on that list knows why its dance-floor frames are capped at `standard`.
//!
//! # What is not here, and cannot be added without an ADR
//!
//! **No command returns pixels.** Section 9's SFE row asks for a 100 % zoom preview and the panel
//! renders it from the pixels it already has, through phase 14's `render`. What this surface adds
//! is the *parameters* that render was made with, so a before-and-after is the same region asked
//! for twice. Phase 13's rule: evidence can never be a pixel.
//!
//! **No command can upscale or reconstruct.** Section 2.2 puts both out of scope for V1, there is
//! no field here that could express either, and `crates/aura-restore/tests/boundaries.rs` fails
//! the build if the words appear in the crate.
//!
//! **No command can schedule restoration onto the interactive path.** `RestorePassInput::when` is
//! `export` or `background`, `RestoreWhen` has no third variant, and `graph::plan` refuses
//! independently. Three layers, and they fail differently.

use aura_core::contract::restore::{
    DenoiseTier, RecoveredFace, RestoreCode, RestoreOverride, RestorePlan, RestoreService,
    RestoreWhen,
};
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{PhotoId, Priority, ProjectId};

use crate::commands::IpcResult;
use crate::contract::ipc::{
    AcceptRestoreInput, ArtefactReportDto, CropRectDto, IpcError, RestoreFaceDto,
    RestoreIdentityRefusalDto, RestorePassDto, RestorePassInput, RestorePlanDto, RestoreReasonDto,
    RestoreRefusalDto, RestoreReviewInput, RestoreStatusDto, SetRestoreOverrideInput,
};
use crate::state::AppState;

/// What the Restore panel's project header shows.
///
/// # Errors
///
/// `AURA-DB-3006` when the stored plans cannot be read, and `AURA-ML-5111` when the profile
/// tables will not load.
pub fn restore_status(state: &AppState, project_id: &str) -> IpcResult<RestoreStatusDto> {
    let project = parse_project(project_id)?;
    let outline = state.restore(&project)?.outline(project)?;
    Ok(RestoreStatusDto {
        photos: outline.photos,
        planned: outline.planned,
        coverage: outline.coverage,
        acted_on: outline.acted_on,
        region_covered: outline.region_covered,
        tiers: outline.tier_histogram.to_vec(),
        tier_names: DenoiseTier::ALL
            .iter()
            .map(|tier| tier.as_str().to_string())
            .collect(),
        sharpened: outline.sharpened,
        sharpen_refusals: outline
            .sharpen_refusals
            .iter()
            .map(|(code, count)| RestoreRefusalDto {
                code: code.as_str().to_string(),
                text: code.user_text().to_string(),
                count: *count,
            })
            .collect(),
        faces_recovered: outline.faces_recovered,
        faces_skipped_identity: outline.faces_skipped_identity,
        worst_identity_drift: outline.worst_identity_drift,
        mean_texture_retention: outline.mean_texture_retention,
        mean_ringing: outline.mean_ringing,
        reduced: outline.reduced,
        needs_review: outline.needs_review,
        user_edited: outline.user_edited,
        unmeasured_cameras: outline.unmeasured_cameras.clone(),
        unlisted_scenes: outline.unlisted_scenes.clone(),
        versions: vec![outline.model_ver, outline.analysis_ver, outline.profile_ver],
    })
}

/// One photograph's restoration plan, or `None` when it has not been planned.
///
/// # Errors
///
/// `AURA-DB-3006` when the plan cannot be read.
pub fn image_restore(state: &AppState, photo_id: &str) -> IpcResult<Option<RestorePlanDto>> {
    let photo = parse_photo(photo_id)?;
    let project = project_of(state, photo)?;
    let plan = state.restore(&project)?.of_image(photo)?;
    Ok(plan.map(|plan| to_dto(&plan)))
}

/// Every frame in the project whose face recovery was declined, worst first.
///
/// **The guarantee's own list.** See the module header.
///
/// # Errors
///
/// `AURA-DB-3006` when the faces cannot be read.
pub fn restore_identity_refusals(
    state: &AppState,
    input: &RestoreReviewInput,
) -> IpcResult<Vec<RestoreIdentityRefusalDto>> {
    let project = parse_project(&input.project_id)?;
    let limit = input.limit.unwrap_or(50).clamp(1, 500) as usize;
    let service = state.restore(&project)?;
    let ids = service.identity_refusals(project, limit)?;

    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        // The per-frame detail comes from the plan rather than from a second query, so the number
        // the panel shows beside a frame is the number stored on that frame's own rows.
        let Some(plan) = service.of_image(id)? else {
            continue;
        };
        let declined: Vec<&RecoveredFace> = plan
            .recovered
            .iter()
            .filter(|face| face.skipped_because == Some(RestoreCode::IdentityDriftSkipped))
            .collect();
        if declined.is_empty() {
            continue;
        }
        out.push(RestoreIdentityRefusalDto {
            photo_id: id.to_db(),
            worst_drift: declined
                .iter()
                .map(|face| face.identity_drift)
                .fold(0.0_f32, f32::max),
            faces: count(declined.len()),
        });
    }
    Ok(out)
}

/// The frames whose restoration is worth a photographer's attention, weakest first.
///
/// # Errors
///
/// `AURA-DB-3006` when the plans cannot be read.
pub fn restore_review_queue(
    state: &AppState,
    input: &RestoreReviewInput,
) -> IpcResult<Vec<String>> {
    let project = parse_project(&input.project_id)?;
    let limit = input.limit.unwrap_or(50).clamp(1, 500) as usize;
    let ids = state.restore(&project)?.needs_review(project, limit)?;
    Ok(ids.into_iter().map(|id| id.to_db()).collect())
}

/// Record that a photographer has looked at one plan and agrees.
///
/// Sets `reviewed` and does **not** set `user_edited`: accepting a suggestion is not authoring
/// one, and phase 30's learning loop needs to tell them apart.
///
/// # Errors
///
/// `AURA-ML-5110` when the photograph has no plan.
pub fn accept_restore(state: &AppState, input: &AcceptRestoreInput) -> IpcResult<()> {
    let photo = parse_photo(&input.photo_id)?;
    let project = project_of(state, photo)?;
    state.restore(&project)?.accept(photo)?;
    Ok(())
}

/// Record what a photographer chose for one photograph.
///
/// **A tier and two switches, and no other number.** See the module header and ADR-0048 section 3.
///
/// # Errors
///
/// `AURA-ML-5110` when the override sets nothing, when the tier is not one of the four, or when
/// the photograph has no plan.
pub fn set_restore_override(
    state: &AppState,
    input: &SetRestoreOverrideInput,
) -> IpcResult<RestorePlanDto> {
    let photo = parse_photo(&input.photo_id)?;
    let project = project_of(state, photo)?;

    let denoise = match input.denoise.as_deref() {
        None => None,
        Some(text) => Some(DenoiseTier::parse(text).ok_or_else(|| {
            // Named rather than defaulted. A caller that sent `strongest` meant something, and
            // silently reading it as `strong` is how a photographer ends up with a tier they did
            // not choose.
            IpcError::from(aura_restore::errors::restore_edit_refused(format!(
                "`{text}` is not one of off, light, standard or strong"
            )))
        })?),
    };

    state.restore(&project)?.set_override(
        photo,
        RestoreOverride {
            denoise,
            sharpen: input.sharpen,
            face_recovery: input.face_recovery,
        },
    )?;

    // The plan as it now stands, so the panel renders what was stored rather than what it asked
    // for. Phase 15's rule: a row with `user_edited = 1` still carries AURA's own numbers, and a
    // panel that echoed the request would hide the disagreement phase 30's learning loop reads.
    let plan = state.restore(&project)?.of_image(photo)?.ok_or_else(|| {
        IpcError::from(aura_restore::errors::restore_edit_refused(
            "the plan disappeared between the write and the read",
        ))
    })?;
    Ok(to_dto(&plan))
}

/// Run the resumable restoration pass.
///
/// # Errors
///
/// `AURA-DB-3006` when the pending set cannot be read, and `AURA-ML-5111` when the tables will
/// not load. Per-photograph failures are counted in the report rather than returned.
pub fn restore_pass(state: &AppState, input: &RestorePassInput) -> IpcResult<RestorePassDto> {
    let project = parse_project(&input.project_id)?;
    let priority = match input.priority.as_deref() {
        Some("visible") => Priority::Visible,
        Some("interactive") => Priority::Interactive,
        Some("background") => Priority::Background,
        _ => Priority::AiBatch,
    };
    // There is no interactive occasion, and an unrecognised value reads as the background pass
    // rather than as the export one: a caller that meant `export` and typed something else should
    // get the slower, safer answer.
    let when = input
        .when
        .as_deref()
        .and_then(RestoreWhen::parse)
        .unwrap_or(RestoreWhen::Background);

    let mut pass = state
        .restore_pass(&input.project_id)?
        .enabled(input.enabled.unwrap_or(true));
    if let Some(long_edge) = input.output_long_edge {
        pass = pass.with_output_long_edge(long_edge);
    }

    let cancel = CancelToken::new();
    let progress = NullProgress;
    let report = pass.run(&project, when, priority, &cancel, &progress)?;

    Ok(RestorePassDto {
        planned: count(report.planned),
        failed: count(report.failed),
        acted_on: count(report.acted_on),
        region_covered: count(report.region_covered),
        tiers: report.tiers.iter().copied().map(count).collect(),
        sharpened: count(report.sharpened),
        faces_recovered: count(report.faces_recovered),
        faces_skipped_identity: count(report.identity_refusals),
        reduced: count(report.reduced),
        low_confidence: count(report.low_confidence),
        unmeasured_cameras: report.unmeasured_cameras.clone(),
        unlisted_scenes: report.unlisted_scenes.clone(),
        elapsed_ms: report.elapsed_ms,
        cancelled: report.cancelled,
    })
}

/// Every reason code this phase can emit, for the panel's own legend.
///
/// Assembled from the contract's own enum rather than from a list here, so a code added to the
/// vocabulary cannot go missing from the panel. Phase 13's rule about the reason registry, in the
/// small.
#[must_use]
pub fn restore_reason_codes() -> Vec<RestoreReasonDto> {
    RestoreCode::ALL
        .iter()
        .map(|code| RestoreReasonDto {
            code: code.as_str().to_string(),
            text: code.user_text().to_string(),
            subject: code.subject().as_str().to_string(),
            weight: 0.0,
            restraint: code.is_restraint(),
            area: None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Conversion
// ---------------------------------------------------------------------------

/// One count as the wire's integer, saturating rather than truncating.
///
/// Every caller is a count of photographs or of faces in one project, so the saturation is
/// unreachable. It is written as a saturation anyway because the alternative is a cast that wraps
/// a four-billion-frame wedding into a small number, and a small number is the one failure a
/// coverage report cannot show.
fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn to_dto(plan: &RestorePlan) -> RestorePlanDto {
    let spec = plan.denoise_spec.as_ref();
    let sharpen = plan.sharpen.as_ref();
    RestorePlanDto {
        photo_id: plan.image_id.to_db(),
        denoise: plan.denoise.as_str().to_string(),
        denoise_luminance: spec.map(|s| s.luminance),
        denoise_colour: spec.map(|s| s.colour),
        denoise_sigma: spec.map(|s| s.sigma),
        denoise_camera: spec.map(|s| s.camera.clone()),
        denoise_measured: spec.is_some_and(|s| s.measured_model),
        sharpen_kernel: sharpen.map(|s| s.kernel_sigma),
        sharpen_amount: sharpen.map_or(0.0, |s| s.amount),
        sharpen_skin_attenuation: sharpen.map_or(0.0, |s| s.skin_attenuation),
        sharpen_coverage: sharpen.map_or(0.0, |s| s.mask.coverage),
        face_recovery: plan.face_recovery.unwrap_or(0.0),
        faces: plan.recovered.iter().map(face_dto).collect(),
        faces_recovered: count(plan.faces_recovered()),
        faces_skipped_identity: count(plan.faces_skipped_for_identity()),
        selfcheck: plan.selfcheck.map(|report| ArtefactReportDto {
            texture_retention: report.texture_retention,
            ringing: report.ringing,
            identity_drift: report.identity_drift,
            measured_on: report.measured_on,
            resolves: report.resolves,
            denoise_reduced: report.denoise_reduced,
            sharpen_reduced: report.sharpen_reduced,
            face_skipped: report.face_skipped,
        }),
        run_where: plan.run_where.as_str().to_string(),
        run_when: plan.when.as_str().to_string(),
        region_covered: plan.region_covered,
        reasons: plan
            .reasons
            .iter()
            .map(|reason| RestoreReasonDto {
                code: reason.code.as_str().to_string(),
                text: reason.text.clone(),
                subject: reason.subject().as_str().to_string(),
                weight: reason.weight,
                restraint: reason.is_restraint(),
                area: reason.evidence.map(|area| CropRectDto {
                    x: area.x,
                    y: area.y,
                    w: area.w,
                    h: area.h,
                }),
            })
            .collect(),
        confidence: plan.confidence,
        scene: plan.scene.as_str().to_string(),
        user_edited: plan.user_edited,
        reviewed: plan.reviewed,
    }
}

fn face_dto(face: &RecoveredFace) -> RestoreFaceDto {
    RestoreFaceDto {
        identity_id: face.identity.map(|id| id.to_db()),
        area: CropRectDto {
            x: face.bounds.x,
            y: face.bounds.y,
            w: face.bounds.w,
            h: face.bounds.h,
        },
        sharpness: face.sharpness,
        strength: face.strength,
        identity_drift: face.identity_drift,
        resolves: face.resolves,
        skipped: face.skipped,
        skipped_because: face.skipped_because.map(|code| code.as_str().to_string()),
    }
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

fn parse_project(id: &str) -> IpcResult<ProjectId> {
    ProjectId::from_db(id).map_err(|_| {
        IpcError::from(aura_restore::errors::restore_edit_refused(format!(
            "`{id}` is not a project id"
        )))
    })
}

fn parse_photo(id: &str) -> IpcResult<PhotoId> {
    PhotoId::from_db(id).map_err(|_| {
        IpcError::from(aura_restore::errors::restore_edit_refused(format!(
            "`{id}` is not a photograph id"
        )))
    })
}

/// Which project a photograph belongs to.
///
/// The service is project-scoped for the reason phases 20 and 21 are - some of its frozen methods
/// are about a project rather than about a photograph - so a per-photograph command has to
/// resolve one.
fn project_of(state: &AppState, photo: PhotoId) -> IpcResult<ProjectId> {
    Ok(state.project_of(photo)?)
}
