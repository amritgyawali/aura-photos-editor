//! The local light command surface. PHASE-19.
//!
//! Six commands. Three read - the project's coverage, one photograph's plan and the
//! low-confidence review queue - one runs the resumable pass, and two record what the
//! photographer decided about a frame the pass had already decided.
//!
//! # What this module does that the tone one does not
//!
//! **It writes a recipe's `masks[]` rather than three global fields.** Phase 15's pass carries
//! three numbers across through `aura_recipe::schema::merge`; this one carries a list of
//! masks. The merge treats an array as atomic - `schema`'s own note says a mask list is
//! edited as a whole, and a per-element path would break the moment somebody reordered two
//! masks - so a photographer who has touched `masks` owns the whole list and an automated pass
//! refuses it entirely. That is stricter than the tone case and it is the right side to err
//! on: half of somebody's local edits replaced by half of AURA's is not a state anybody asked
//! for.
//!
//! # What is not here, and cannot be added without an ADR
//!
//! **No command returns a mask.** Phase 18 owns masks and phase 19 reads them through
//! `MaskField`; nothing on this surface can return an alpha, a matte or a grid, and
//! `LocalPlanDto` has no field that could hold one. What the panel gets instead is the
//! *evidence rectangles* the reasons carry and the shaping zones by name, which is what a
//! photographer needs and carries no image data.
//!
//! **No command retouches.** There is no blur radius, no smoothing strength and no texture
//! parameter anywhere on this surface. Section 2.2 puts skin texture in phase 20, and the
//! boundary is structural rather than remembered.
//!
//! **No command normalises a gallery.** Nothing here reads a second photograph. That is
//! phase 25.

use aura_core::contract::local::{
    LocalCode, LocalLightPlan, LocalOp, LocalOverride, LocalReason, LocalService, MaskKind,
};
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{PhotoId, Priority, ProjectId};
use aura_recipe::{schema, EditSource, Mask, MaskParams, Recipe};

use crate::commands::IpcResult;
use crate::contract::ipc::{
    AcceptLocalInput, CropRectDto, FaceLightDto, GateDto, IpcError, LocalPassDto, LocalPlanDto,
    LocalReasonDto, LocalReviewInput, LocalStatusDto, SculptLocalInput, SetLocalStrengthDto,
    SetLocalStrengthInput, ShapingZoneDto,
};
use crate::develop_commands::{load_or_neutral, recipe_dto};
use crate::state::AppState;

/// What the Local panel's project header shows.
///
/// # Errors
///
/// `AURA-DB-3006` when the stored plans cannot be read.
pub fn local_status(state: &AppState, project_id: &str) -> IpcResult<LocalStatusDto> {
    let project = parse_project(project_id)?;
    let outline = state.local().outline(project)?;
    Ok(LocalStatusDto {
        photos: outline.photos,
        planned: outline.planned,
        coverage: outline.coverage,
        acted_on: outline.acted_on,
        mask_covered: outline.mask_covered,
        op_counts: outline.op_histogram.to_vec(),
        op_names: LocalOp::PRIORITY
            .iter()
            .map(|op| op.as_str().to_string())
            .collect(),
        gated_counts: outline.gated_histogram.to_vec(),
        gated_names: MaskKind::ALL
            .iter()
            .map(|kind| kind.as_str().to_string())
            .collect(),
        mean_budget_used: outline.mean_budget_used,
        shine_reduced: outline.shine_reduced,
        mean_shine_ev: outline.mean_shine_ev,
        group_solved: outline.group_solved,
        needs_review: outline.needs_review,
        user_edited: outline.user_edited,
        unpolicied_scenes: outline.unpolicied_scenes.clone(),
        model_ver: u32::from(outline.model_ver),
        analysis_ver: u32::from(outline.analysis_ver),
        policy_ver: u32::from(outline.policy_ver),
        shaping_ver: u32::from(outline.shaping_ver),
    })
}

/// One photograph's plan, or `None` when it has not been planned.
///
/// # Errors
///
/// `AURA-DB-3006` when the plan cannot be read.
pub fn image_local(state: &AppState, photo_id: &str) -> IpcResult<Option<LocalPlanDto>> {
    let photo = parse_photo(photo_id)?;
    Ok(state.local().of_image(photo)?.as_ref().map(plan_dto))
}

/// The frames whose local work is worth a photographer's attention, weakest first.
///
/// # Errors
///
/// `AURA-DB-3006` when the plans cannot be read.
pub fn local_review_queue(state: &AppState, input: &LocalReviewInput) -> IpcResult<Vec<String>> {
    let project = parse_project(&input.project_id)?;
    let limit = input.limit.unwrap_or(200).clamp(1, 5_000) as usize;
    Ok(state
        .local()
        .needs_review(project, limit)?
        .into_iter()
        .map(|id| id.to_db())
        .collect())
}

/// Record that the photographer has looked at one plan and agrees.
///
/// # Errors
///
/// `AURA-ML-5067` when the photograph has no plan.
pub fn accept_local(state: &AppState, input: &AcceptLocalInput) -> IpcResult<LocalPlanDto> {
    let photo = parse_photo(&input.photo_id)?;
    let local = state.local();
    local.accept(photo)?;
    local
        .of_image(photo)?
        .as_ref()
        .map(plan_dto)
        .ok_or_else(|| {
            IpcError::from(aura_brain_photo::errors::local_edit_refused(
                "the plan disappeared between the write and the read",
            ))
        })
}

/// Record what the photographer set instead, and write it into the recipe.
///
/// # Errors
///
/// `AURA-ML-5067` when the photograph has no plan, the operation is not one of the six, or the
/// strength is outside `0..1`; `AURA-RENDER-8002` when the merged recipe will not validate.
pub fn set_local_strength(
    state: &AppState,
    input: &SetLocalStrengthInput,
) -> IpcResult<SetLocalStrengthDto> {
    let project = parse_project(&input.project_id)?;
    let photo = parse_photo(&input.photo_id)?;
    let Some(op) = LocalOp::parse(&input.operation) else {
        return Err(IpcError::from(
            aura_brain_photo::errors::local_edit_refused(format!(
                "`{}` is not one of the six local operations",
                input.operation
            )),
        ));
    };

    let local = state.local();
    local.set_override(photo, LocalOverride::one(op, input.strength))?;

    let Some(plan) = local.of_image(photo)? else {
        return Err(IpcError::from(
            aura_brain_photo::errors::local_edit_refused(
                "the plan disappeared between the write and the read",
            ),
        ));
    };

    let base = load_or_neutral(state, photo)?;
    let proposal = with_masks(&base, &plan, Some((op, input.strength)));
    let (merged, report) = schema::merge(&base, &proposal, EditSource::User)?;
    let merged = merged.clamped();
    schema::Validation::check(&merged)?;
    state
        .recipe_store()
        .save(&project, &photo, &merged, &report.changed, "Local light")?;

    Ok(SetLocalStrengthDto {
        plan: plan_dto(&plan),
        recipe: recipe_dto(&input.photo_id, &merged),
        changed: report.changed.clone(),
        protected: merged.provenance.user_edited_fields.clone(),
    })
}

/// Run the resumable local light pass, then write what it decided into the recipes.
///
/// # Errors
///
/// `AURA-ML-5069` when the policy table will not load, `AURA-ML-5063` when phase 15's exposure
/// targets will not, or whatever building the preview service raised. Per-photograph failures
/// are counted rather than returned.
pub fn sculpt_local(state: &AppState, input: &SculptLocalInput) -> IpcResult<LocalPassDto> {
    let project = parse_project(&input.project_id)?;
    let pass = state.local_pass(&input.project_id)?;
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

    Ok(LocalPassDto {
        planned: saturating_u32(report.planned),
        failed: saturating_u32(report.failed),
        acted_on: saturating_u32(report.acted_on),
        op_counts: report.op_histogram.to_vec(),
        gated: saturating_u32(report.gated),
        fully_masked: saturating_u32(report.fully_masked),
        group_solved: saturating_u32(report.group_solved),
        shine_reduced: saturating_u32(report.shine_reduced),
        low_confidence: saturating_u32(report.low_confidence),
        mean_budget_used: report.mean_budget_used,
        unpolicied_scenes: report.unpolicied_scenes.clone(),
        recipes_written: written,
        recipes_protected: protected,
        elapsed_ms: report.elapsed_ms,
        cancelled: report.cancelled,
    })
}

// -- carrying the decision into the recipe ----------------------------------

/// Write every current plan into its photograph's recipe, as the product.
fn write_recipes(
    state: &AppState,
    project: &ProjectId,
    cancel: &CancelToken,
) -> Result<(u32, u32), IpcError> {
    let local = state.local();
    let store = state.recipe_store();
    let mut written = 0_u32;
    let mut protected = 0_u32;

    for photo in state.local_store().planned(project)? {
        if cancel.is_cancelled() {
            break;
        }
        let Some(plan) = local.of_image(photo)? else {
            continue;
        };
        // A frame the photographer has taken over keeps their strengths.
        if plan.user_edited {
            continue;
        }
        let base = load_or_neutral(state, photo)?;
        let proposal = with_masks(&base, &plan, None);
        let (merged, report) = schema::merge(&base, &proposal, EditSource::Ai)?;
        protected = protected.saturating_add(saturating_u32(report.refused.len()));
        if report.changed.is_empty() {
            continue;
        }
        let merged = merged.clamped();
        schema::Validation::check(&merged)?;
        store.save(project, &photo, &merged, &report.changed, "Local light")?;
        written = written.saturating_add(1);
    }
    Ok((written, protected))
}

/// The base recipe with this plan's masks.
///
/// **The only function in this module that touches a recipe's parameters**, and it can reach
/// exactly one field: `masks`. There is no path from here to the global exposure, to the
/// curve or to a retouch operator, which is what keeps phases 15, 16 and 20's boundaries
/// structural.
///
/// `override_one` is the photographer's own strength for one operation, when they have just
/// set one. It scales that operation's parameters and leaves the other five where the product
/// put them.
fn with_masks(
    base: &Recipe,
    plan: &LocalLightPlan,
    override_one: Option<(LocalOp, f32)>,
) -> Recipe {
    let mut out = base.clone();
    let scale_of = |op: LocalOp| -> f32 {
        match override_one {
            Some((chosen, strength)) if chosen == op => strength.clamp(0.0, 1.0),
            _ => 1.0,
        }
    };

    let mut masks: Vec<Mask> = Vec::new();

    // One face mask per lit face. The target names the identity when phase 06 knows one, and
    // the *ordinal* when it does not - because a frame with four unnamed guests needs four
    // distinguishable masks and `face` alone would collapse them.
    let face_scale = scale_of(LocalOp::FaceLight);
    for (index, (identity, delta)) in plan.face_light.iter().enumerate() {
        if delta.is_noop() || face_scale <= 0.0 {
            continue;
        }
        masks.push(Mask {
            id: format!("l_face_{index}"),
            kind: aura_recipe::MaskKind::Face,
            target: Some(identity.as_ref().map_or_else(
                || format!("face:{index}"),
                |id| format!("identity:{}", id.to_db()),
            )),
            invert_of: None,
            feather: delta.feather,
            params: MaskParams {
                exposure: Some(delta.exposure_ev * face_scale),
                shadows: Some(scaled(delta.shadows, face_scale)),
                highlights: Some(scaled(delta.highlights, face_scale)),
                ..MaskParams::default()
            },
        });
    }

    // The paired subject and background move, as two masks that are written together or not
    // at all. Section 6.2's rule, kept by the shape of this block.
    let subject_scale = scale_of(LocalOp::SubjectEnhance);
    let background_scale = scale_of(LocalOp::BackgroundBalance);
    if !plan.subject.is_noop() || !plan.background.is_noop() {
        masks.push(Mask {
            id: "l_subject".to_string(),
            kind: aura_recipe::MaskKind::Subject,
            target: None,
            invert_of: None,
            feather: 0.60,
            params: MaskParams {
                clarity: Some(scaled(plan.subject.clarity, subject_scale)),
                texture: Some(scaled(plan.subject.texture, subject_scale)),
                contrast: Some(scaled(plan.subject.contrast, subject_scale)),
                ..MaskParams::default()
            },
        });
        masks.push(Mask {
            id: "l_background".to_string(),
            kind: aura_recipe::MaskKind::Background,
            target: None,
            invert_of: Some("l_subject".to_string()),
            feather: plan.background.feather,
            params: MaskParams {
                exposure: Some(plan.background.exposure_ev * background_scale),
                highlights: Some(scaled(plan.background.highlights, background_scale)),
                saturation: Some(scaled(plan.background.saturation, background_scale)),
                ..MaskParams::default()
            },
        });
    }

    // Shine, as a skin mask carrying a negative exposure and nothing else. The type could
    // express a clarity or a texture here; the code never writes one, and `docs/local-light.md`
    // says why in the product's own words.
    let shine_scale = scale_of(LocalOp::ShineControl);
    if let Some(shine) = &plan.shine {
        if !shine.is_empty() && shine_scale > 0.0 {
            masks.push(Mask {
                id: "l_shine".to_string(),
                kind: aura_recipe::MaskKind::Skin,
                target: Some("specular".to_string()),
                invert_of: None,
                feather: 0.45,
                params: MaskParams {
                    exposure: Some(shine.reduction_ev * shine_scale),
                    ..MaskParams::default()
                },
            });
        }
    }

    out.masks = masks;
    out.provenance.scene = Some(plan.scene.as_str().to_string());
    out.provenance.confidence = plan.confidence;
    out.provenance.source = if override_one.is_some() {
        EditSource::User
    } else {
        EditSource::Ai
    };
    out
}

#[allow(clippy::cast_possible_truncation)]
fn scaled(value: i16, scale: f32) -> i16 {
    (f32::from(value) * scale).round().clamp(-100.0, 100.0) as i16
}

// -- projections ------------------------------------------------------------

fn plan_dto(plan: &LocalLightPlan) -> LocalPlanDto {
    LocalPlanDto {
        photo_id: plan.image_id.to_db(),
        strengths: plan.strengths.to_vec(),
        operations: LocalOp::PRIORITY
            .iter()
            .map(|op| op.as_str().to_string())
            .collect(),
        faces: plan
            .face_light
            .iter()
            .map(|(identity, delta)| FaceLightDto {
                identity_id: identity.as_ref().map(aura_core::IdentityId::to_db),
                exposure_ev: delta.exposure_ev,
                shadows: i32::from(delta.shadows),
                highlights: i32::from(delta.highlights),
                luma_before: delta.luma_before,
                luma_target: delta.luma_target,
                luma_after: delta.luma_after,
                noise_cap_ev: delta.noise_cap_ev,
                mask_scale: delta.mask_scale,
            })
            .collect(),
        subject_clarity: i32::from(plan.subject.clarity),
        subject_texture: i32::from(plan.subject.texture),
        subject_contrast: i32::from(plan.subject.contrast),
        background_ev: plan.background.exposure_ev,
        background_saturation: i32::from(plan.background.saturation),
        competition_ratio: plan.background.competition_ratio,
        chroma_energy: plan.background.chroma_energy,
        mean_luma_before: plan.background.mean_luma_before,
        mean_luma_after: plan.background.mean_luma_after,
        shine_regions: plan
            .shine
            .as_ref()
            .map_or(0, |s| saturating_u32(s.regions.len())),
        shine_ev: plan.shine.as_ref().map_or(0.0, |s| s.reduction_ev),
        shine_boxes: plan.shine.as_ref().map_or_else(Vec::new, |s| {
            s.regions.iter().map(|rect| crop_dto(*rect)).collect()
        }),
        shaping: plan.dodge_burn.as_ref().map_or_else(Vec::new, |maps| {
            maps.faces
                .iter()
                .map(|face| {
                    face.zones
                        .iter()
                        .map(|zone| ShapingZoneDto {
                            zone: zone.zone.as_str().to_string(),
                            cx: zone.centre[0],
                            cy: zone.centre[1],
                            radius: zone.radius,
                            gain_ev: zone.gain_ev,
                        })
                        .collect()
                })
                .collect()
        }),
        face_spread: plan.inter_face_spread(),
        group_fair: plan.group_is_fair(),
        budget_used: plan.total_budget_used,
        gated: plan
            .gated_by_mask_quality
            .iter()
            .map(|(op, kind)| GateDto {
                operation: op.as_str().to_string(),
                mask_kind: kind.as_str().to_string(),
            })
            .collect(),
        reasons: plan.reasons.iter().map(reason_dto).collect(),
        confidence: plan.confidence,
        scene: plan.scene.as_str().to_string(),
        user_edited: plan.user_edited,
        reviewed: plan.reviewed,
        needs_review: plan.needs_review(),
        model_ver: u32::from(plan.model_ver),
        analysis_ver: u32::from(plan.analysis_ver),
        policy_ver: u32::from(plan.policy_ver),
        shaping_ver: u32::from(plan.shaping_ver),
    }
}

fn reason_dto(reason: &LocalReason) -> LocalReasonDto {
    LocalReasonDto {
        code: reason.code.as_str().to_string(),
        text: reason.text.clone(),
        weight: reason.weight,
        operation: reason.code.operation().map(|op| op.as_str().to_string()),
        withdrawal: reason.code.is_withdrawal(),
        evidence: reason.evidence.map(crop_dto),
    }
}

fn crop_dto(rect: aura_core::CropRect) -> CropRectDto {
    CropRectDto {
        x: rect.x,
        y: rect.y,
        w: rect.w,
        h: rect.h,
    }
}

// -- parsing ----------------------------------------------------------------

fn saturating_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn parse_project(id: &str) -> Result<ProjectId, IpcError> {
    ProjectId::from_db(id).map_err(|_| bad_id("project", id))
}

fn parse_photo(id: &str) -> Result<PhotoId, IpcError> {
    PhotoId::from_db(id).map_err(|_| bad_id("photo", id))
}

fn bad_id(kind: &str, id: &str) -> IpcError {
    IpcError::from(aura_core::errors::db::statement_failed(
        format!("not a {kind} id: {id}"),
        &std::io::Error::from(std::io::ErrorKind::InvalidInput),
    ))
}

/// The reason code a plan carries when nothing could run.
///
/// Exposed so a caller can tell "phase 18 is not installed" from "there was nothing to do"
/// without parsing sentences.
#[must_use]
pub const fn gated_code() -> LocalCode {
    LocalCode::MaskUnavailable
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::local::{FaceLightDelta, LocalReason};
    use aura_core::SceneId;
    use aura_recipe::fixtures as recipe_fixtures;

    fn neutral() -> Recipe {
        recipe_fixtures::neutral(&"a".repeat(64), "Test Camera")
    }

    fn photo() -> PhotoId {
        PhotoId::from_db("pht_00000000-0000-4000-8000-000000000019").expect("a photo id")
    }

    fn lit_plan() -> LocalLightPlan {
        let mut plan = LocalLightPlan::nothing(
            photo(),
            SceneId::FamilyPortrait,
            LocalReason::plain(LocalCode::FaceLit, 0.0),
        );
        plan.face_light = vec![(
            None,
            FaceLightDelta {
                exposure_ev: 0.25,
                shadows: 30,
                highlights: -10,
                feather: 0.5,
                luma_before: 0.30,
                luma_target: 0.48,
                luma_after: 0.42,
                noise_cap_ev: 0.9,
                mask_scale: 1.0,
            },
        )];
        plan
    }

    #[test]
    fn a_plan_becomes_masks_and_touches_nothing_else() {
        let base = neutral();
        let out = with_masks(&base, &lit_plan(), None);
        assert_eq!(out.masks.len(), 1);
        assert!((out.global.exposure - base.global.exposure).abs() < f32::EPSILON);
        assert_eq!(out.global.temperature, base.global.temperature);
        assert!(
            out.retouch.is_empty(),
            "phase 19 must not write a retouch op"
        );
    }

    #[test]
    fn a_face_mask_carries_all_three_fields_of_the_split() {
        let out = with_masks(&neutral(), &lit_plan(), None);
        let mask = out.masks.first().expect("one face mask");
        assert_eq!(mask.kind, aura_recipe::MaskKind::Face);
        assert!(mask.params.exposure.is_some());
        assert!(mask.params.shadows.is_some());
        assert!(
            mask.params.highlights.is_some_and(|value| value <= 0),
            "a face lift pushed the highlights up"
        );
    }

    #[test]
    fn an_override_scales_only_the_operation_it_names() {
        let plan = lit_plan();
        let full = with_masks(&neutral(), &plan, None);
        let half = with_masks(&neutral(), &plan, Some((LocalOp::FaceLight, 0.5)));
        let a = full.masks.first().expect("a mask").params.exposure;
        let b = half.masks.first().expect("a mask").params.exposure;
        assert!(b < a, "the override did not reach the mask");

        // Turning the shaping off leaves the face lighting where it was.
        let other = with_masks(&neutral(), &plan, Some((LocalOp::DodgeBurnLow, 0.0)));
        assert_eq!(other.masks.first().expect("a mask").params.exposure, a);
    }

    #[test]
    fn an_empty_plan_writes_no_masks_at_all() {
        let plan = LocalLightPlan::nothing(
            photo(),
            SceneId::Venue,
            LocalReason::plain(LocalCode::MaskUnavailable, -0.25),
        );
        let out = with_masks(&neutral(), &plan, None);
        assert!(out.masks.is_empty());
    }

    #[test]
    fn a_persons_mask_list_survives_an_automated_pass() {
        // The property this module exists to keep. `masks` is an atomic path, so a person who
        // has edited it owns the whole list and the pass is refused - which is stricter than
        // the tone case and is the right side to err on.
        let base = neutral();
        let mut mine = base.clone();
        mine.masks = vec![Mask {
            id: "mine".to_string(),
            kind: aura_recipe::MaskKind::Linear,
            target: None,
            invert_of: None,
            feather: 0.5,
            params: MaskParams {
                exposure: Some(-0.3),
                ..MaskParams::default()
            },
        }];
        let (base, _) = schema::merge(&base, &mine, EditSource::User).expect("user merge");
        assert!(base
            .provenance
            .user_edited_fields
            .contains(&"masks".to_string()));

        let theirs = with_masks(&base, &lit_plan(), None);
        let (merged, report) = schema::merge(&base, &theirs, EditSource::Ai).expect("ai merge");
        assert_eq!(
            merged.masks.len(),
            1,
            "the person's own mask list was replaced"
        );
        assert_eq!(merged.masks.first().map(|m| m.id.as_str()), Some("mine"));
        assert!(report.refused.contains(&"masks".to_string()));
    }

    #[test]
    fn the_paired_operations_are_written_together_or_not_at_all() {
        let mut plan = lit_plan();
        plan.subject = aura_core::contract::local::SubjectEnhanceDelta {
            clarity: 12,
            texture: 6,
            contrast: 4,
            paired_background_ev: -0.3,
            competition_ratio: 1.4,
            mask_scale: 1.0,
        };
        plan.background = aura_core::contract::local::BackgroundBalanceDelta {
            exposure_ev: -0.3,
            highlights: -20,
            saturation: -10,
            feather: 0.8,
            competition_ratio: 1.4,
            chroma_energy: 0.05,
            bright_blobs: 1,
            mean_luma_before: 0.5,
            mean_luma_after: 0.49,
            mask_scale: 1.0,
        };
        let out = with_masks(&neutral(), &plan, None);
        let ids: Vec<&str> = out.masks.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&"l_subject"), "{ids:?}");
        assert!(ids.contains(&"l_background"), "{ids:?}");
    }
}
