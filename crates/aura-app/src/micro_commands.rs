//! The micro-retouch command surface. PHASE-21.
//!
//! Seven commands. Three read - the project coverage, one photograph's plan and the review
//! queue - one reads the **disclosure list**, one runs the resumable pass, and two record what
//! the studio decided.
//!
//! # What this module does that the retouch one does not
//!
//! **It publishes what was composited.** `micro_composites` exists on the frozen service and on
//! this surface because a borrow that can only be found by opening four hundred plans one at a
//! time is a borrow nobody finds. The same view backs the panel, the delivery report and phase
//! 27's QC agent, so no two of them can disagree about what happened.
//!
//! **Its override cannot change a magnitude.** `set_micro_matrix` carries three sets of switches
//! and nothing else. Phase 20's override could set a per-identity strength; this one cannot set
//! anything numeric at all, because every ceiling here is a product decision that
//! `docs/retouch-ethics.md` makes a promise about. A surface that could raise one would turn that
//! document into a description of the defaults.
//!
//! # What is not here, and cannot be added without an ADR
//!
//! **No command reshapes, slims, whitens or recolours.** There is no field on this surface that
//! could express any of them, and `crates/aura-retouch/tests/boundaries.rs` fails the build if
//! the words appear in the crate.
//!
//! **No command can request a borrow.** Borrowing is decided by the solver against five
//! conditions, four of which are properties of the photograph rather than of a preference. What a
//! studio may do is switch it off.
//!
//! **No command returns a mask or a crop of somebody's face.** What the panel gets is rectangles
//! and numbers; the pixels it draws are the ones it already has on screen.

use aura_core::contract::micro::{
    ClothingIssue, MicroCode, MicroOp, MicroOverride, MicroPlan, MicroService, NaturalnessReport,
    OpFamily,
};
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{PhotoId, Priority, ProjectId};
use aura_recipe::{schema, EditSource, Recipe, RetouchOp as RecipeRetouchOp};

use crate::commands::IpcResult;
use crate::contract::ipc::{
    AcceptMicroInput, CropRectDto, IpcError, MicroCompositeDto, MicroMatrixDto, MicroOpDto,
    MicroPassDto, MicroPassInput, MicroPlanDto, MicroReasonDto, MicroReviewInput, MicroStatusDto,
    NaturalnessReportDto, SetMicroMatrixInput,
};
use crate::develop_commands::load_or_neutral;
use crate::state::AppState;

/// What the Micro-Retouch panel's project header shows.
///
/// # Errors
///
/// `AURA-DB-3006` when the stored plans cannot be read.
pub fn micro_status(state: &AppState, project_id: &str) -> IpcResult<MicroStatusDto> {
    let project = parse_project(project_id)?;
    let outline = state.micro(&project)?.outline(project)?;
    Ok(MicroStatusDto {
        photos: outline.photos,
        planned: outline.planned,
        coverage: outline.coverage,
        acted_on: outline.acted_on,
        region_covered: outline.region_covered,
        op_counts: outline.op_histogram.to_vec(),
        operators: operator_names(),
        borrows: outline.borrows,
        withdrawn_counts: outline.withdrawn_histogram.to_vec(),
        families: family_names(),
        resolved: outline.resolved,
        mean_catchlight_ratio: outline.mean_catchlight_ratio,
        mean_hair_energy_ratio: outline.mean_hair_energy_ratio,
        needs_review: outline.needs_review,
        user_edited: outline.user_edited,
        unlisted_scenes: outline.unlisted_scenes.clone(),
        model_ver: u32::from(outline.model_ver),
        analysis_ver: u32::from(outline.analysis_ver),
        matrix_ver: u32::from(outline.matrix_ver),
    })
}

/// One photograph's plan, or `None` when it has not been planned.
///
/// # Errors
///
/// `AURA-DB-3006` when the plan cannot be read.
pub fn image_micro(state: &AppState, photo_id: &str) -> IpcResult<Option<MicroPlanDto>> {
    let photo = parse_photo(photo_id)?;
    let project = state.project_of(photo)?;
    Ok(state
        .micro(&project)?
        .of_image(photo)?
        .as_ref()
        .map(plan_dto))
}

/// Every frame in the project that composited pixels from another.
///
/// **The disclosure call.** Empty is the ordinary answer and it is a real one: a gallery with no
/// borrowed pixels in it should be able to say so without a photographer inferring it from the
/// absence of a warning.
///
/// # Errors
///
/// `AURA-DB-3006` when the plans cannot be read.
pub fn micro_composites(state: &AppState, project_id: &str) -> IpcResult<Vec<MicroCompositeDto>> {
    let project = parse_project(project_id)?;
    let composites = state.micro(&project)?.composites(project)?;
    Ok(composites
        .into_iter()
        .map(|(photo, sources)| MicroCompositeDto {
            photo_id: photo.to_db(),
            source_photo_ids: sources.iter().map(PhotoId::to_db).collect(),
        })
        .collect())
}

/// The frames whose micro-retouch is worth a photographer's attention, weakest first.
///
/// # Errors
///
/// `AURA-DB-3006` when the plans cannot be read.
pub fn micro_review_queue(state: &AppState, input: &MicroReviewInput) -> IpcResult<Vec<String>> {
    let project = parse_project(&input.project_id)?;
    let limit = input.limit.unwrap_or(50).clamp(1, 500) as usize;
    Ok(state
        .micro(&project)?
        .needs_review(project, limit)?
        .iter()
        .map(PhotoId::to_db)
        .collect())
}

/// Which operations this project permits.
///
/// # Errors
///
/// `AURA-DB-3006` when the matrix cannot be read.
pub fn micro_matrix(state: &AppState, project_id: &str) -> IpcResult<MicroMatrixDto> {
    let project = parse_project(project_id)?;
    let values = state.micro(&project)?.matrix(project)?;
    Ok(matrix_dto(&values))
}

/// Record which operations this project permits.
///
/// # Errors
///
/// `AURA-ML-5104` when the override sets nothing, or when a list is the wrong length.
pub fn set_micro_matrix(
    state: &AppState,
    input: &SetMicroMatrixInput,
) -> IpcResult<MicroMatrixDto> {
    let project = parse_project(&input.project_id)?;
    let allowed = match &input.allowed {
        None => None,
        Some(values) => Some(exactly::<5>(values, "allowed")?),
    };
    let clothing = match &input.clothing {
        None => None,
        Some(values) => Some(exactly::<{ ClothingIssue::COUNT }>(values, "clothing")?),
    };
    let service = state.micro(&project)?;
    service.set_matrix(
        project,
        MicroOverride {
            allowed,
            clothing,
            borrowing: input.borrowing,
        },
    )?;
    Ok(matrix_dto(&service.matrix(project)?))
}

/// Record that the photographer has looked at one plan and agrees.
///
/// # Errors
///
/// `AURA-ML-5104` when the photograph has no plan.
pub fn accept_micro(state: &AppState, input: &AcceptMicroInput) -> IpcResult<()> {
    let photo = parse_photo(&input.photo_id)?;
    let project = state.project_of(photo)?;
    state.micro(&project)?.accept(photo)?;
    Ok(())
}

/// Run the resumable micro-retouch pass over a project.
///
/// # Errors
///
/// `AURA-DB-3006` when the pending set cannot be read.
pub fn micro_pass(state: &AppState, input: &MicroPassInput) -> IpcResult<MicroPassDto> {
    let project = parse_project(&input.project_id)?;
    let priority = match input.priority.as_deref() {
        Some("visible") => Priority::Visible,
        Some("interactive") => Priority::Interactive,
        Some("background") => Priority::Background,
        _ => Priority::AiBatch,
    };
    let cancel = CancelToken::new();
    let pass = state
        .micro_pass(&input.project_id)?
        .enabled(input.enabled.unwrap_or(true));
    let report = pass.run(&project, priority, &cancel, &NullProgress)?;
    let _ = write_recipes(state, &project, &cancel)?;
    Ok(MicroPassDto {
        planned: saturating_u32(report.planned),
        failed: saturating_u32(report.failed),
        acted_on: saturating_u32(report.acted_on),
        region_covered: saturating_u32(report.region_covered),
        ops: report.ops.iter().copied().map(saturating_u32).collect(),
        borrows: saturating_u32(report.borrows),
        mean_alignment: report.mean_alignment,
        withdrawn: report
            .withdrawn
            .iter()
            .copied()
            .map(saturating_u32)
            .collect(),
        resolved: saturating_u32(report.resolved),
        low_confidence: saturating_u32(report.low_confidence),
        unlisted_scenes: report.unlisted_scenes.clone(),
        elapsed_ms: report.elapsed_ms,
        cancelled: report.cancelled,
    })
}

// -- carrying the decision into the recipe ----------------------------------

/// Write every current plan into its photograph's recipe.
///
/// **Phase 14's rule, for the fifth phase running.** This module decides nothing: the plans
/// already exist, and `aura_recipe::schema::merge` is the only function in the workspace that
/// writes one recipe into another. A photographer who has touched `retouch` owns the whole list
/// and an automated pass refuses it entirely - the merge treats an array as atomic, which is the
/// same rule and the same side to err on that phases 19 and 20 chose.
fn write_recipes(
    state: &AppState,
    project: &ProjectId,
    cancel: &CancelToken,
) -> Result<(u32, u32), IpcError> {
    let micro = state.micro(project)?;
    let store = state.recipe_store();
    let mut written = 0_u32;
    let mut protected = 0_u32;

    for photo in state.micro_store().planned(project)? {
        if cancel.is_cancelled() {
            break;
        }
        let Some(plan) = micro.of_image(photo)? else {
            continue;
        };
        // A frame the photographer has taken over keeps their settings.
        if plan.user_edited {
            continue;
        }
        let base = load_or_neutral(state, photo)?;
        let proposal = with_micro(&base, &plan);
        let (merged, report) = schema::merge(&base, &proposal, EditSource::Ai)?;
        protected = protected.saturating_add(saturating_u32(report.refused.len()));
        if report.changed.is_empty() {
            continue;
        }
        let merged = merged.clamped();
        schema::Validation::check(&merged)?;
        store.save(project, &photo, &merged, &report.changed, "Micro-retouch")?;
        written = written.saturating_add(1);
    }
    Ok((written, protected))
}

/// The base recipe with this plan's micro operations appended.
///
/// **The only function in this module that touches a recipe's parameters**, and it can reach
/// exactly one field: `retouch`. Phase 14 provisioned that array for "phases 20 and 21" in the
/// frozen schema, so this phase appends rather than claiming a field of its own.
///
/// The operations phase 20 wrote are kept and this phase's are added after them, which is also the
/// order the renderer applies them in: skin first, then the small fixes on top. Rebuilding the
/// list from the micro plan alone would silently drop a frame's blemish work.
///
/// `borrowed_from` is filled for a glare repair and is `None` everywhere else. That is the third
/// of the five places a borrow is disclosed - see `docs/retouch-ethics.md` section 5 - and it is
/// the one that survives the catalog being lost.
fn with_micro(base: &Recipe, plan: &MicroPlan) -> Recipe {
    use aura_core::contract::micro::MicroRegion;
    let mut out = base.clone();
    out.retouch
        .retain(|op| !MicroOp::NAMES.contains(&op.op.as_str()));
    for op in &plan.ops {
        out.retouch.push(RecipeRetouchOp {
            op: op.as_str().to_string(),
            strength: op.strength().clamp(0.0, 1.0),
            // Not this phase's guarantee. Phase 20's texture floor belongs to phase 20's
            // operations; a micro operation carries zero here rather than a number it does not
            // hold itself to.
            protect_texture: 0.0,
            mask: Some(
                match op {
                    MicroOp::Flyaway { .. } => MicroRegion::Hair,
                    MicroOp::Teeth { .. } => MicroRegion::Teeth,
                    MicroOp::Eyes { .. } => MicroRegion::Iris,
                    MicroOp::Clothing { .. } => MicroRegion::Clothing,
                    MicroOp::Glare { .. } => MicroRegion::Eyes,
                }
                .as_mask_str()
                .to_string(),
            ),
            borrowed_from: op.borrowed_from().map(|id| id.to_db()),
        });
    }
    out
}

fn saturating_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

// ---------------------------------------------------------------------------
// Shapes
// ---------------------------------------------------------------------------

fn plan_dto(plan: &MicroPlan) -> MicroPlanDto {
    MicroPlanDto {
        photo_id: plan.image_id.to_db(),
        ops: plan.ops.iter().map(op_dto).collect(),
        naturalness: naturalness_dto(&plan.naturalness),
        allowed: plan.allowed.to_vec(),
        operators: operator_names(),
        reasons: plan.reasons.iter().map(reason_dto).collect(),
        confidence: plan.confidence,
        scene: plan.scene.as_str().to_string(),
        budget_used: plan.budget_used,
        borrowed_from: plan.borrowed_from().iter().map(PhotoId::to_db).collect(),
        user_edited: plan.user_edited,
        reviewed: plan.reviewed,
        model_ver: u32::from(plan.model_ver),
        analysis_ver: u32::from(plan.analysis_ver),
        matrix_ver: u32::from(plan.matrix_ver),
    }
}

fn op_dto(op: &MicroOp) -> MicroOpDto {
    use aura_core::contract::micro::GlareMethod;
    let (luma_ev, yellow_reduce) = match op {
        MicroOp::Teeth {
            luma,
            yellow_reduce,
            ..
        } => (*luma, *yellow_reduce),
        _ => (0.0, 0.0),
    };
    let (sclera, iris_clarity) = match op {
        MicroOp::Eyes {
            sclera,
            iris_clarity,
            ..
        } => (*sclera, *iris_clarity),
        _ => (0.0, 0.0),
    };
    let (method, alignment) = match op {
        MicroOp::Glare { method, .. } => match method {
            GlareMethod::Reduce { .. } => (Some("reduce".to_string()), 0.0),
            GlareMethod::BorrowFrom { alignment, .. } => (Some("borrow".to_string()), *alignment),
        },
        _ => (None, 0.0),
    };
    MicroOpDto {
        kind: op.as_str().to_string(),
        strength: op.strength(),
        region: op.region().map(|r| CropRectDto {
            x: r.x,
            y: r.y,
            w: r.w,
            h: r.h,
        }),
        identity_id: match op {
            MicroOp::Teeth { identity, .. } | MicroOp::Eyes { identity, .. } => {
                Some(identity.to_db())
            }
            _ => None,
        },
        luma_ev,
        yellow_reduce,
        sclera,
        iris_clarity,
        clothing_kind: match op {
            MicroOp::Clothing { kind, .. } => Some(kind.as_str().to_string()),
            _ => None,
        },
        method,
        borrowed_from: op.borrowed_from().map(|id| id.to_db()),
        alignment,
    }
}

fn naturalness_dto(report: &NaturalnessReport) -> NaturalnessReportDto {
    NaturalnessReportDto {
        catchlight_ratio: report.catchlight_ratio,
        hair_energy_ratio: report.hair_energy_ratio,
        teeth_excursion: report.teeth_excursion,
        measured_on: report.measured_on,
        resolves: report.resolves,
        withdrawn: report.withdrawn.to_vec(),
        families: family_names(),
    }
}

fn reason_dto(reason: &aura_core::contract::micro::MicroReason) -> MicroReasonDto {
    MicroReasonDto {
        code: reason.code.as_str().to_string(),
        text: if reason.text.is_empty() {
            reason.code.user_text().to_string()
        } else {
            reason.text.clone()
        },
        weight: reason.weight,
        doubt: reason.is_doubt(),
        evidence: reason.evidence.map(|r| CropRectDto {
            x: r.x,
            y: r.y,
            w: r.w,
            h: r.h,
        }),
    }
}

fn matrix_dto(values: &MicroOverride) -> MicroMatrixDto {
    MicroMatrixDto {
        allowed: values.allowed.unwrap_or([true; 5]).to_vec(),
        operators: operator_names(),
        clothing: values
            .clothing
            .unwrap_or([true, true, true, false, false])
            .to_vec(),
        clothing_kinds: ClothingIssue::ALL
            .iter()
            .map(|kind| kind.as_str().to_string())
            .collect(),
        clothing_opt_in: ClothingIssue::ALL
            .iter()
            .map(|kind| kind.is_opt_in_only())
            .collect(),
        borrowing: values.borrowing.unwrap_or(true),
    }
}

fn operator_names() -> Vec<String> {
    MicroOp::NAMES
        .iter()
        .map(|name| (*name).to_string())
        .collect()
}

fn family_names() -> Vec<String> {
    OpFamily::ALL
        .iter()
        .map(|family| family.as_str().to_string())
        .collect()
}

/// Read a list of switches of exactly the expected length.
///
/// A wrong-length list is refused rather than padded. A panel that sent four switches for five
/// operations has a bug, and silently defaulting the fifth is how a studio ends up with an
/// operation running that they believe they switched off.
fn exactly<const N: usize>(values: &[bool], field: &str) -> IpcResult<[bool; N]> {
    if values.len() != N {
        return Err(IpcError::from(aura_retouch::errors::micro_edit_refused(
            format!(
                "`{field}` has {} entries and needs exactly {N}",
                values.len()
            ),
        )));
    }
    let mut out = [false; N];
    for (slot, value) in out.iter_mut().zip(values.iter()) {
        *slot = *value;
    }
    Ok(out)
}

fn parse_project(id: &str) -> IpcResult<ProjectId> {
    ProjectId::from_db(id).map_err(|_| {
        IpcError::from(aura_retouch::errors::micro_edit_refused(format!(
            "`{id}` is not a project id"
        )))
    })
}

fn parse_photo(id: &str) -> IpcResult<PhotoId> {
    PhotoId::from_db(id).map_err(|_| {
        IpcError::from(aura_retouch::errors::micro_edit_refused(format!(
            "`{id}` is not a photograph id"
        )))
    })
}

/// Every reason code this phase can emit, for the panel's own legend.
///
/// Assembled from the frozen enum rather than written out, so it cannot go stale and there is no
/// way to render a code no deciding path can produce - phase 13's rule.
#[must_use]
pub fn micro_reason_codes() -> Vec<MicroReasonDto> {
    MicroCode::ALL
        .iter()
        .map(|code| MicroReasonDto {
            code: code.as_str().to_string(),
            text: code.user_text().to_string(),
            weight: 0.0,
            doubt: code.is_doubt(),
            evidence: None,
        })
        .collect()
}
