//! The portrait retouch command surface. PHASE-20.
//!
//! Seven commands. Three read - the project coverage, one photograph plan and the review
//! queue - one runs the resumable pass, two record what the photographer decided about a frame,
//! and one adds or clears a protected feature.
//!
//! # What this module does that the local one does not
//!
//! **It writes `recipe.retouch[]`.** Phase 19 carries a list of masks; this carries a list of
//! retouch operations. The merge treats an array as atomic, so a photographer who has touched
//! `retouch` owns the whole list and an automated pass refuses it entirely - which is the same
//! rule and the same side to err on.
//!
//! **It writes a row about a person.** `set_protection` is the first command in this product
//! whose subject is somebody rather than a photograph, and its rectangle is projected into
//! face-normalised coordinates before it is stored, so that protecting a mole on one frame
//! protects it on every frame of that person.
//!
//! # What is not here, and cannot be added without an ADR
//!
//! **No command reshapes, slims or lightens.** There is no field on this surface that could
//! express any of them, and `crates/aura-retouch/tests/boundaries.rs` fails the build if the
//! words appear in the crate. Section 11 of `docs/plan/CLAUDE.md` forbids them permanently.
//!
//! **No command can clear a tattoo protection.** `set_protection` refuses it with
//! `AURA-ML-5097`, the store refuses it again, and migration 21 carries a trigger that aborts
//! the delete. Three layers, because section 10.1 gates tattoo removal at zero rather than at a
//! small number.
//!
//! **No command returns a mask or a crop of somebody skin.** What the panel gets is rectangles
//! and numbers; the pixels it draws are the ones it already has on screen.

use aura_core::contract::composition::Box2;
use aura_core::contract::people::PeopleService;
use aura_core::contract::retouch::{
    ProtectedFeature, ProtectedKind, ProtectedSource, RetouchOp, RetouchOverride, RetouchPlan,
    RetouchPreset, RetouchService,
};
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{IdentityId, PhotoId, Priority, ProjectId};
use aura_recipe::{schema, EditSource, Recipe, RetouchOp as RecipeRetouchOp};

use crate::commands::IpcResult;
use crate::contract::ipc::{
    AcceptRetouchInput, CropRectDto, IdentityStrengthDto, IpcError, ProtectedFeatureDto,
    RetouchOpDto, RetouchPassDto, RetouchPassInput, RetouchPlanDto, RetouchReasonDto,
    RetouchReviewInput, RetouchStatusDto, SetProtectionInput, SetRetouchDto, SetRetouchInput,
    TextureReportDto,
};
use crate::develop_commands::{load_or_neutral, recipe_dto};
use crate::state::AppState;

/// What the Retouch panel project header shows.
///
/// # Errors
///
/// `AURA-DB-3006` when the stored plans cannot be read.
pub fn retouch_status(state: &AppState, project_id: &str) -> IpcResult<RetouchStatusDto> {
    let project = parse_project(project_id)?;
    let outline = state.retouch(&project).outline(project)?;
    Ok(RetouchStatusDto {
        photos: outline.photos,
        planned: outline.planned,
        coverage: outline.coverage,
        acted_on: outline.acted_on,
        mask_covered: outline.mask_covered,
        blemishes_removed: outline.blemishes_removed,
        anomalies_left: outline.anomalies_left,
        protected_counts: outline.protected_histogram.to_vec(),
        protected_kinds: ProtectedKind::ALL
            .iter()
            .map(|kind| kind.as_str().to_string())
            .collect(),
        texture_resolved: outline.texture_resolved,
        texture_withdrawn: outline.texture_withdrawn,
        mean_band_ratio: outline.mean_band_ratio,
        mean_strength: outline.mean_strength,
        max_identity_spread: outline.max_identity_spread,
        preset_counts: outline.preset_histogram.to_vec(),
        preset_names: RetouchPreset::ALL
            .iter()
            .map(|preset| preset.as_str().to_string())
            .collect(),
        needs_review: outline.needs_review,
        user_edited: outline.user_edited,
        unpreset_scenes: outline.unpreset_scenes.clone(),
        model_ver: u32::from(outline.model_ver),
        analysis_ver: u32::from(outline.analysis_ver),
        preset_ver: u32::from(outline.preset_ver),
    })
}

/// One photograph plan, or `None` when it has not been planned.
///
/// # Errors
///
/// `AURA-DB-3006` when the plan cannot be read.
pub fn image_retouch(state: &AppState, photo_id: &str) -> IpcResult<Option<RetouchPlanDto>> {
    let photo = parse_photo(photo_id)?;
    let project = state.project_of(photo)?;
    Ok(state
        .retouch(&project)
        .of_image(photo)?
        .as_ref()
        .map(plan_dto))
}

/// Everything protected on one person, across the project.
///
/// # Errors
///
/// `AURA-DB-3006` when the features cannot be read.
pub fn protected_features(
    state: &AppState,
    project_id: &str,
    identity_id: &str,
) -> IpcResult<Vec<ProtectedFeatureDto>> {
    let project = parse_project(project_id)?;
    let identity = parse_identity(identity_id)?;
    Ok(state
        .retouch(&project)
        .protected(identity)?
        .iter()
        .map(protected_dto)
        .collect())
}

/// The frames whose retouch is worth a photographer attention, weakest first.
///
/// # Errors
///
/// `AURA-DB-3006` when the plans cannot be read.
pub fn retouch_review_queue(
    state: &AppState,
    input: &RetouchReviewInput,
) -> IpcResult<Vec<String>> {
    let project = parse_project(&input.project_id)?;
    let limit = input.limit.unwrap_or(200).clamp(1, 5_000) as usize;
    Ok(state
        .retouch(&project)
        .needs_review(project, limit)?
        .into_iter()
        .map(|id| id.to_db())
        .collect())
}

/// Record that the photographer has looked at one plan and agrees.
///
/// # Errors
///
/// `AURA-ML-5097` when the photograph has no plan.
pub fn accept_retouch(state: &AppState, input: &AcceptRetouchInput) -> IpcResult<RetouchPlanDto> {
    let photo = parse_photo(&input.photo_id)?;
    let project = state.project_of(photo)?;
    let retouch = state.retouch(&project);
    retouch.accept(photo)?;
    retouch
        .of_image(photo)?
        .as_ref()
        .map(plan_dto)
        .ok_or_else(|| {
            IpcError::from(aura_retouch::errors::retouch_edit_refused(
                "the plan disappeared between the write and the read",
            ))
        })
}

/// Record what the photographer set instead, and write it into the recipe.
///
/// # Errors
///
/// `AURA-ML-5097` when the photograph has no plan, the preset is not one of the four, or the
/// strength is outside `0..1`; `AURA-RENDER-8002` when the merged recipe will not validate.
pub fn set_retouch(state: &AppState, input: &SetRetouchInput) -> IpcResult<SetRetouchDto> {
    let project = parse_project(&input.project_id)?;
    let photo = parse_photo(&input.photo_id)?;

    let mut values = RetouchOverride::default();
    if let Some(text) = input.preset.as_deref() {
        let Some(preset) = RetouchPreset::parse(text) else {
            return Err(IpcError::from(aura_retouch::errors::retouch_edit_refused(
                format!("`{text}` is not one of the four retouch presets"),
            )));
        };
        values.preset = Some(preset);
    }
    if let Some(identity) = input.identity_id.as_deref() {
        let identity = parse_identity(identity)?;
        let Some(strength) = input.strength else {
            return Err(IpcError::from(aura_retouch::errors::retouch_edit_refused(
                "a person was named with no strength to set",
            )));
        };
        values.identity_strength = Some((identity, strength));
    }

    let retouch = state.retouch(&project);
    retouch.set_override(photo, values)?;

    let Some(plan) = retouch.of_image(photo)? else {
        return Err(IpcError::from(aura_retouch::errors::retouch_edit_refused(
            "the plan disappeared between the write and the read",
        )));
    };

    let base = load_or_neutral(state, photo)?;
    let proposal = with_retouch(&base, &plan);
    let (merged, report) = schema::merge(&base, &proposal, EditSource::User)?;
    let merged = merged.clamped();
    schema::Validation::check(&merged)?;
    state
        .recipe_store()
        .save(&project, &photo, &merged, &report.changed, "Retouch")?;

    Ok(SetRetouchDto {
        plan: plan_dto(&plan),
        recipe: recipe_dto(&input.photo_id, &merged),
        changed: report.changed.clone(),
        protected: merged.provenance.user_edited_fields.clone(),
    })
}

/// Add or clear one protected feature.
///
/// The rectangle arrives in **frame** coordinates, because that is what a photographer drew on a
/// photograph, and it is projected into face-normalised coordinates here - through that frame
/// own eye landmarks - because that is what makes the protection follow the person rather than
/// the photograph.
///
/// # Errors
///
/// `AURA-ML-5097` when the kind is unknown, when the face has no landmarks to project through,
/// or when the feature is absolute and the caller asked to clear it.
pub fn set_protection(
    state: &AppState,
    input: &SetProtectionInput,
) -> IpcResult<Vec<ProtectedFeatureDto>> {
    let project = parse_project(&input.project_id)?;
    let identity = parse_identity(&input.identity_id)?;
    let photo = parse_photo(&input.photo_id)?;

    let Some(kind) = ProtectedKind::parse(&input.kind) else {
        return Err(IpcError::from(aura_retouch::errors::retouch_edit_refused(
            format!("`{}` is not a kind of permanent feature", input.kind),
        )));
    };

    // The face this rectangle is on, and the landmarks that make the projection possible. A
    // face with no landmarks is refused rather than guessed: phase 09 rule is that
    // `[[0,0],[0,0]]` means unknown, and a protect row written in a coordinate system nobody can
    // reproduce protects a random part of every other photograph of that person.
    let subjects = state.people().subjects(photo)?;
    let Some(face) = subjects
        .faces
        .iter()
        .find(|face| face.identity_id == Some(identity) && face.has_eyes())
    else {
        return Err(IpcError::from(aura_retouch::errors::retouch_edit_refused(
            "that person has no face with eye landmarks on that photograph, so a protected \
             region cannot be placed on their face",
        )));
    };

    let frame_area = Box2 {
        x: input.area.x,
        y: input.area.y,
        w: input.area.w,
        h: input.area.h,
    };
    let Some(area) = aura_retouch::permanent::to_face_frame(frame_area, face) else {
        return Err(IpcError::from(aura_retouch::errors::retouch_edit_refused(
            "that region could not be projected onto the face",
        )));
    };

    let feature = ProtectedFeature {
        identity,
        kind,
        area,
        confidence: 1.0,
        source: ProtectedSource::User,
        frames: 1,
        span_minutes: 0.0,
        first_seen: photo,
    };

    let retouch = state.retouch(&project);
    retouch.set_protection(feature, input.protect)?;
    Ok(retouch
        .protected(identity)?
        .iter()
        .map(protected_dto)
        .collect())
}

/// Run the resumable retouch pass, then write what it decided into the recipes.
///
/// # Errors
///
/// `AURA-ML-5099` when the preset table will not load, or whatever building the preview service
/// raised. Per-photograph failures are counted rather than returned.
pub fn retouch_pass(state: &AppState, input: &RetouchPassInput) -> IpcResult<RetouchPassDto> {
    let project = parse_project(&input.project_id)?;
    let preset = match input.preset.as_deref() {
        Some(text) => match RetouchPreset::parse(text) {
            Some(preset) => preset,
            None => {
                return Err(IpcError::from(aura_retouch::errors::retouch_edit_refused(
                    format!("`{text}` is not one of the four retouch presets"),
                )))
            }
        },
        None => RetouchPreset::default(),
    };

    let pass = state.retouch_pass(&input.project_id)?.with_preset(preset);
    let cancel = CancelToken::new();
    if let Some(id) = input.cancel_id.as_deref() {
        state.register_job(id, cancel.clone());
    }
    let report = if input.photo_ids.is_empty() {
        pass.run(&project, Priority::AiBatch, &cancel, &NullProgress)
    } else {
        let ids: Vec<PhotoId> = input
            .photo_ids
            .iter()
            .filter_map(|id| PhotoId::from_db(id).ok())
            .collect();
        pass.run_ids(&project, &ids, Priority::AiBatch, &cancel, &NullProgress)
    };
    if let Some(id) = input.cancel_id.as_deref() {
        state.finish_job(id);
    }
    let report = report?;

    let (written, protected) = write_recipes(state, &project, &cancel)?;

    Ok(RetouchPassDto {
        planned: saturating_u32(report.planned),
        failed: saturating_u32(report.failed),
        acted_on: saturating_u32(report.acted_on),
        mask_covered: saturating_u32(report.mask_covered),
        blemishes: saturating_u32(report.blemishes),
        texture_resolved: saturating_u32(report.texture_resolved),
        texture_withdrawn: saturating_u32(report.texture_withdrawn),
        protected: saturating_u32(report.protected),
        low_confidence: saturating_u32(report.low_confidence),
        mean_band_ratio: report.mean_band_ratio,
        unpreset_scenes: report.unpreset_scenes.clone(),
        recipes_written: written,
        recipes_protected: protected,
        elapsed_ms: report.elapsed_ms,
        cancelled: report.cancelled,
    })
}

// -- carrying the decision into the recipe ----------------------------------

/// Write every current plan into its photograph recipe, as the product.
fn write_recipes(
    state: &AppState,
    project: &ProjectId,
    cancel: &CancelToken,
) -> Result<(u32, u32), IpcError> {
    let retouch = state.retouch(project);
    let store = state.recipe_store();
    let mut written = 0_u32;
    let mut protected = 0_u32;

    for photo in state.retouch_store().planned(project)? {
        if cancel.is_cancelled() {
            break;
        }
        let Some(plan) = retouch.of_image(photo)? else {
            continue;
        };
        // A frame the photographer has taken over keeps their settings.
        if plan.user_edited {
            continue;
        }
        let base = load_or_neutral(state, photo)?;
        let proposal = with_retouch(&base, &plan);
        let (merged, report) = schema::merge(&base, &proposal, EditSource::Ai)?;
        protected = protected.saturating_add(saturating_u32(report.refused.len()));
        if report.changed.is_empty() {
            continue;
        }
        let merged = merged.clamped();
        schema::Validation::check(&merged)?;
        store.save(project, &photo, &merged, &report.changed, "Retouch")?;
        written = written.saturating_add(1);
    }
    Ok((written, protected))
}

/// The base recipe with this plan retouch operations.
///
/// **The only function in this module that touches a recipe parameters**, and it can reach
/// exactly one field: `retouch`. There is no path from here to the global exposure, to the curve
/// or to a mask, which is what keeps phases 15, 16, 19 and 20 boundaries structural rather than
/// remembered.
///
/// `protect_texture` on every operation is the preset floor, carried onto the wire so a renderer
/// that executes the recipe on another machine holds the same guarantee this plan was measured
/// against.
fn with_retouch(base: &Recipe, plan: &RetouchPlan) -> Recipe {
    let mut out = base.clone();
    out.retouch = plan
        .ops
        .iter()
        .map(|op| RecipeRetouchOp {
            op: op.as_str().to_string(),
            strength: op.strength().clamp(0.0, 1.0),
            protect_texture: plan.texture_report.floor,
            mask: match op {
                RetouchOp::ToneEvening { mask, .. } => Some(mask.to_db()),
                _ => Some("skin".to_string()),
            },
            // Phase 20 never composites. The field exists because phase 21's glare repair does,
            // and `None` here is a statement rather than a default: nothing in this phase reads
            // another photograph.
            borrowed_from: None,
        })
        .collect();
    out
}

// -- the wire shapes ---------------------------------------------------------

fn plan_dto(plan: &RetouchPlan) -> RetouchPlanDto {
    RetouchPlanDto {
        photo_id: plan.image_id.to_db(),
        ops: plan.ops.iter().map(op_dto).collect(),
        identity_strengths: plan
            .per_identity_strength
            .iter()
            .map(|(identity, strength)| IdentityStrengthDto {
                identity_id: identity.to_db(),
                strength: *strength,
            })
            .collect(),
        protected: plan.protected.iter().map(protected_dto).collect(),
        texture: TextureReportDto {
            band_ratio: plan.texture_report.band_ratio,
            floor: plan.texture_report.floor,
            passed: plan.texture_report.passed,
            measured_on: plan.texture_report.measured_on,
            resolves: plan.texture_report.resolves,
            withdrawn: plan.texture_report.withdrawn,
        },
        preset: plan.preset.as_str().to_string(),
        reasons: plan
            .reasons
            .iter()
            .map(|reason| RetouchReasonDto {
                code: reason.code.as_str().to_string(),
                text: reason.text.clone(),
                weight: reason.weight,
                withdrawal: reason.code.is_withdrawal(),
                evidence: reason.evidence.map(crop_dto),
            })
            .collect(),
        confidence: plan.confidence,
        scene: plan.scene.as_str().to_string(),
        budget_used: plan.budget_used,
        user_edited: plan.user_edited,
        reviewed: plan.reviewed,
        needs_review: plan.needs_review(),
        model_ver: u32::from(plan.model_ver),
        analysis_ver: u32::from(plan.analysis_ver),
        preset_ver: u32::from(plan.preset_ver),
    }
}

fn op_dto(op: &RetouchOp) -> RetouchOpDto {
    let (method, identity, luma, chroma) = match op {
        RetouchOp::Blemish { method, .. } => (Some(method.as_str().to_string()), None, 0.0, 0.0),
        RetouchOp::UnderEye {
            identity,
            luma,
            chroma,
        } => (None, Some(identity.to_db()), *luma, *chroma),
        _ => (None, None, 0.0, 0.0),
    };
    RetouchOpDto {
        kind: op.as_str().to_string(),
        strength: op.strength(),
        area: op.area().map(crop_dto),
        method,
        identity_id: identity,
        luma_ev: luma,
        chroma,
    }
}

fn protected_dto(feature: &ProtectedFeature) -> ProtectedFeatureDto {
    ProtectedFeatureDto {
        identity_id: feature.identity.to_db(),
        kind: feature.kind.as_str().to_string(),
        area: crop_dto(feature.area),
        confidence: feature.confidence,
        source: feature.source.as_str().to_string(),
        frames: feature.frames,
        span_minutes: feature.span_minutes,
        first_seen_photo: feature.first_seen.to_db(),
        absolute: feature.is_absolute(),
    }
}

fn crop_dto(area: Box2) -> CropRectDto {
    CropRectDto {
        x: area.x,
        y: area.y,
        w: area.w,
        h: area.h,
    }
}

fn parse_project(id: &str) -> Result<ProjectId, IpcError> {
    ProjectId::from_db(id).map_err(|_| {
        IpcError::from(aura_retouch::errors::retouch_edit_refused(format!(
            "`{id}` is not a project id"
        )))
    })
}

fn parse_photo(id: &str) -> Result<PhotoId, IpcError> {
    PhotoId::from_db(id).map_err(|_| {
        IpcError::from(aura_retouch::errors::retouch_edit_refused(format!(
            "`{id}` is not a photograph id"
        )))
    })
}

fn parse_identity(id: &str) -> Result<IdentityId, IpcError> {
    IdentityId::from_db(id).map_err(|_| {
        IpcError::from(aura_retouch::errors::retouch_edit_refused(format!(
            "`{id}` is not a person id"
        )))
    })
}

fn saturating_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::retouch::{FreqBand, InpaintMethod, RetouchCode, RetouchReason};
    use aura_core::{MaskId, SceneId};

    fn photo() -> PhotoId {
        PhotoId::from_db("pht_00000000-0000-4000-8000-000000000020").expect("a photo id")
    }

    fn plan() -> RetouchPlan {
        let mut plan = RetouchPlan::nothing(
            photo(),
            SceneId::CouplePortrait,
            RetouchReason::plain(RetouchCode::BlemishRemoved, 0.1),
        );
        plan.ops = vec![
            RetouchOp::Blemish {
                area: Box2 {
                    x: 0.4,
                    y: 0.4,
                    w: 0.02,
                    h: 0.02,
                },
                method: InpaintMethod::Patch,
                strength: 0.7,
            },
            RetouchOp::ToneEvening {
                mask: MaskId::from_db("msk_00000000-0000-4000-8000-000000000020")
                    .expect("a mask id"),
                strength: 0.3,
                band: FreqBand::Mid,
            },
        ];
        plan
    }

    #[test]
    fn a_plan_reaches_the_wire_with_its_operations_and_its_texture_report() {
        let dto = plan_dto(&plan());
        assert_eq!(dto.ops.len(), 2);
        assert_eq!(dto.ops[0].kind, "blemish");
        assert_eq!(dto.ops[0].method.as_deref(), Some("patch"));
        assert!(dto.ops[0].area.is_some());
        assert_eq!(dto.ops[1].kind, "tone_evening");
        assert!(dto.ops[1].area.is_none());
        assert!(dto.texture.passed);
    }

    #[test]
    fn the_recipe_carries_the_texture_floor_onto_the_wire() {
        // A renderer executing this recipe on another machine has to hold the same guarantee the
        // plan was measured against, so the floor travels with the operations rather than being
        // re-derived from a preset name.
        let base = aura_recipe::fixtures::neutral("hash", "bench");
        let out = with_retouch(&base, &plan());
        assert_eq!(out.retouch.len(), 2);
        for op in &out.retouch {
            assert!((op.protect_texture - 0.90).abs() < 1e-6, "{op:?}");
            assert!(op.mask.is_some());
        }
    }

    #[test]
    fn writing_a_retouch_touches_nothing_else_in_the_recipe() {
        let base = aura_recipe::fixtures::neutral("hash", "bench");
        let out = with_retouch(&base, &plan());
        assert_eq!(out.global, base.global);
        assert_eq!(out.masks, base.masks);
        assert_eq!(out.restoration, base.restoration);
    }
}
