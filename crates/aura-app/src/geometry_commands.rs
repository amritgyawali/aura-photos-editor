//! The geometry command surface. PHASE-23.
//!
//! Six commands. Three read - the project's coverage, one photograph's plan and the review
//! queue - one runs the resumable pass, and two record what the photographer decided about a
//! frame the pass had already decided.
//!
//! # What this module does that the local one does not
//!
//! **It writes three recipe blocks rather than a mask list**: `lens`, `geometry.crop` and
//! `geometry.rotate`, plus `geometry.perspective` when a keystone survived. Each is a scalar
//! path, so `schema::merge` protects them individually - a photographer who dragged a crop owns
//! `geometry.crop` and still receives a lens correction, which is the right granularity and is
//! *finer* than phase 19 could manage with its atomic mask array.
//!
//! # What is not here, and cannot be added without an ADR
//!
//! **No command returns a pixel.** `GeometryPlanDto` has no field that could hold one, and the
//! evidence a reason carries is a rectangle - the face that would have been cut - rather than
//! a crop of it.
//!
//! **No command returns the lens profile table.** A profile is an input to a decision. What
//! reaches the wire is the name of the profile that matched and `lensSynthetic`, which says
//! whether anybody measured it. A photographer is never told a lens was profiled when it was
//! invented.
//!
//! **No command fills a corner.** A keystone opens two and a rotation opens four; they are
//! cropped away. Section 2.2 puts filling in phase 24 and there is no parameter here for it.
//!
//! **No command chooses which crop an album uses.** That is phase 29, and `variant` is how it
//! will ask.

use aura_core::contract::geometry::{
    Aspect, CropPurpose, CropVariant, GeometryCode, GeometryOverride, GeometryPlan, GeometryReason,
    GeometryService, ProtectedKind, ProtectedRegion,
};
use aura_core::contract::composition::CompositionService;
use aura_core::contract::integrity::CropRect;
use aura_core::contract::people::PeopleService;
use aura_core::contract::scene::StoryService;
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{PhotoId, ProjectId};
use aura_geometry::api::GeometryPass;
use aura_geometry::plan::GeometryInput;
use aura_recipe::{schema, EditSource, LensCoefficients, Perspective, Recipe};

use crate::commands::IpcResult;
use crate::contract::ipc::{
    AcceptGeometryInput, CropRectDto, CropSafetyDto, CropVariantDto, GeometryPassDto,
    GeometryPlanDto, GeometryReasonDto, GeometryReviewInput, GeometryStatusDto, IpcError,
    PlanGeometryInput, SetFramingDto, SetFramingInput,
};
use crate::develop_commands::{load_or_neutral, recipe_dto};
use crate::state::AppState;

/// What the Geometry panel's project header shows.
///
/// # Errors
///
/// `AURA-ML-5093` when the crop rules or the lens profiles will not load, `AURA-DB-3006` when
/// the plans cannot be read.
pub fn geometry_status(state: &AppState, project_id: &str) -> IpcResult<GeometryStatusDto> {
    let project = parse_project(project_id)?;
    let service = state.geometry()?;
    let outline = service.outline(project)?;
    let profiles = service.planner().profiles();
    Ok(GeometryStatusDto {
        photos: outline.photos,
        planned: outline.planned,
        coverage: outline.coverage,
        kept_original: outline.kept_original,
        profile_covered: outline.profile_covered,
        levelled: outline.levelled,
        mean_rotate_deg: outline.mean_rotate_deg,
        keystoned: outline.keystoned,
        variant_counts: outline.variant_histogram.to_vec(),
        variant_names: CropPurpose::ALL
            .iter()
            .map(|purpose| purpose.title().to_string())
            .collect(),
        refused_counts: outline.refused_histogram.to_vec(),
        refused_names: GeometryCode::REFUSALS
            .iter()
            .map(|code| code.as_str().to_string())
            .collect(),
        missing_profiles: outline.missing_profiles.clone(),
        unpolicied_scenes: service
            .store()
            .unpolicied(&project)
            .unwrap_or_default(),
        needs_review: outline.needs_review,
        user_edited: outline.user_edited,
        profiles_synthetic: profiles.is_synthetic(),
        profiles_known: u32::try_from(profiles.len()).unwrap_or(u32::MAX),
        profile_ver: u32::from(outline.profile_ver),
        analysis_ver: u32::from(outline.analysis_ver),
        rules_ver: u32::from(outline.rules_ver),
    })
}

/// One photograph's plan, or `None` when it has not been planned.
///
/// # Errors
///
/// `AURA-ML-5093` when the tables will not load, `AURA-DB-3006` when the plan cannot be read.
pub fn image_geometry(state: &AppState, photo_id: &str) -> IpcResult<Option<GeometryPlanDto>> {
    let photo = parse_photo(photo_id)?;
    let service = state.geometry()?;
    let synthetic = service.planner().profiles().is_synthetic();
    Ok(service
        .of_image(photo)?
        .as_ref()
        .map(|plan| plan_dto(plan, synthetic)))
}

/// The frames whose geometry is worth a photographer's attention, weakest first.
///
/// # Errors
///
/// `AURA-DB-3006` when the plans cannot be read.
pub fn geometry_review_queue(
    state: &AppState,
    input: &GeometryReviewInput,
) -> IpcResult<Vec<String>> {
    let project = parse_project(&input.project_id)?;
    let limit = input.limit.unwrap_or(200).clamp(1, 5_000) as usize;
    Ok(state
        .geometry()?
        .needs_review(project, limit)?
        .into_iter()
        .map(|id| id.to_db())
        .collect())
}

/// Record that the photographer has looked at one plan and agrees.
///
/// # Errors
///
/// `AURA-ML-5091` when the photograph has no plan.
pub fn accept_geometry(
    state: &AppState,
    input: &AcceptGeometryInput,
) -> IpcResult<GeometryPlanDto> {
    let photo = parse_photo(&input.photo_id)?;
    let service = state.geometry()?;
    service.accept(photo)?;
    let synthetic = service.planner().profiles().is_synthetic();
    let Some(plan) = service.of_image(photo)? else {
        return Err(IpcError::from(aura_geometry::errors::framing_refused(
            "the plan disappeared between the write and the read",
        )));
    };
    Ok(plan_dto(&plan, synthetic))
}

/// Record the framing the photographer chose, and write it into the recipe.
///
/// Reverting is this command with the whole frame and zero degrees. See `SetFramingInput`.
///
/// # Errors
///
/// `AURA-ML-5091` when the rectangle is degenerate, leaves the frame, or the angle is outside
/// `-45..45`; `AURA-RENDER-8002` when the merged recipe is invalid.
pub fn set_framing(state: &AppState, input: &SetFramingInput) -> IpcResult<SetFramingDto> {
    let project = parse_project(&input.project_id)?;
    let photo = parse_photo(&input.photo_id)?;
    let service = state.geometry()?;
    let synthetic = service.planner().profiles().is_synthetic();

    let rect = CropRect {
        x: input.rect.x,
        y: input.rect.y,
        w: input.rect.w,
        h: input.rect.h,
    };
    service.set_framing(GeometryOverride {
        image_id: photo,
        rect,
        rotate_deg: input.rotate_deg,
        aspect: Aspect::from_str_or_original(&input.aspect),
    })?;

    let Some(plan) = service.of_image(photo)? else {
        return Err(IpcError::from(aura_geometry::errors::framing_refused(
            "the plan disappeared between the write and the read",
        )));
    };

    let base = load_or_neutral(state, photo)?;
    let proposal = into_recipe(&base, &plan);
    let (merged, report) = schema::merge(&base, &proposal, EditSource::User)?;
    let merged = merged.clamped();
    schema::Validation::check(&merged)?;
    state
        .recipe_store()
        .save(&project, &photo, &merged, &report.changed, "Framing")?;

    Ok(SetFramingDto {
        plan: plan_dto(&plan, synthetic),
        recipe: recipe_dto(&input.photo_id, &merged),
        changed: report.changed.clone(),
        protected: merged.provenance.user_edited_fields.clone(),
    })
}

/// Run the resumable geometry pass, then write what it decided into the recipes.
///
/// **The pass plans from what other phases already measured**, and on a frame nobody has
/// analysed that is nothing at all: no tilt, no faces, no distractions. Such a frame is planned
/// and delivered as shot, which is the right answer and is why the pass never fails on it.
///
/// # Errors
///
/// `AURA-ML-5093` when the crop rules or the lens profiles will not load. Per-photograph
/// failures are counted rather than returned.
pub fn plan_geometry(state: &AppState, input: &PlanGeometryInput) -> IpcResult<GeometryPassDto> {
    let project = parse_project(&input.project_id)?;
    let service = state.geometry()?;
    let started = state.clock().monotonic_ms();
    let cancel = CancelToken::new();
    let limit = input.limit.map_or(usize::MAX, |value| value as usize);

    let mut seen = 0usize;
    let pass = GeometryPass::new(&service, &NullProgress, &cancel);
    let outcome = pass.run(project, |image| {
        if seen >= limit {
            cancel.cancel();
            return None;
        }
        seen += 1;
        Some(build_input(state, image))
    })?;

    // The recipes, written through the merge exactly as phase 19's pass writes its masks.
    let mut written = 0u32;
    let mut protected = 0u32;
    for image in service.store().planned(&project).unwrap_or_default() {
        let Ok(Some(plan)) = service.of_image(image) else {
            continue;
        };
        if plan.is_identity() {
            continue;
        }
        let Ok(base) = load_or_neutral(state, image) else {
            continue;
        };
        let proposal = into_recipe(&base, &plan);
        let Ok((merged, report)) = schema::merge(&base, &proposal, EditSource::Ai) else {
            continue;
        };
        let merged = merged.clamped();
        if schema::Validation::check(&merged).is_err() {
            continue;
        }
        if state
            .recipe_store()
            .save(&project, &image, &merged, &report.changed, "Geometry")
            .is_ok()
        {
            written += 1;
        }
        protected += u32::try_from(merged.provenance.user_edited_fields.len()).unwrap_or(0);
    }

    let outline = service.outline(project)?;
    Ok(GeometryPassDto {
        planned: outcome.planned,
        failed: outcome.failed,
        kept_original: outline.kept_original,
        recipes_written: written,
        recipes_protected: protected,
        elapsed_ms: state.clock().monotonic_ms().saturating_sub(started),
        cancelled: outcome.cancelled,
    })
}

// ---------------------------------------------------------------------------
// Building one frame's input from what the other phases already measured
// ---------------------------------------------------------------------------

/// Gather phase 06's faces and phase 11's framing for one photograph.
///
/// **Every input is optional and an absent one is not a failure.** A frame nobody has analysed
/// is planned with no tilt, no regions and no distractions, and is delivered exactly as it was
/// shot - which is what a phase with a seventy-per-cent restraint target should do with a
/// photograph it knows nothing about.
fn build_input(state: &AppState, image: PhotoId) -> GeometryInput {
    let mut input = GeometryInput::bare(image, aura_core::SceneId::Unknown);

    if let Ok(story) = state.story() {
        if let Ok(Some(scene)) = story.scene(image) {
            input.scene = scene.scene;
        }
    }

    {
        let composition = state.composition();
        if let Ok(Some(result)) = composition.of_image(image) {
            input.scene = result.scene;
            input.tilt_deg = result.tilt_deg;
            input.horizon_conf = result.horizon_conf;
            input.tilt_intentional = result.tilt_intentional;
            input.distractions = result
                .bright_blobs
                .iter()
                .chain(result.edge_intrusions.iter())
                .copied()
                .collect();
            if let Some(hint) = result.crop_suggestion_hint {
                input.subject = Some(hint.region);
                input.regions.push(ProtectedRegion {
                    kind: ProtectedKind::KeyContent,
                    identity: None,
                    rect: hint.region,
                    primary: true,
                });
            }
        }
    }

    if let Ok(subjects) = state.people().subjects(image) {
        for face in &subjects.faces {
            input.regions.push(ProtectedRegion {
                kind: ProtectedKind::Face,
                identity: face.identity_id,
                rect: face.bbox,
                primary: subjects.dominant.is_some() && subjects.dominant == face.identity_id,
            });
        }
    }

    input
}

// ---------------------------------------------------------------------------
// Turning a plan into a recipe
// ---------------------------------------------------------------------------

/// The recipe a plan proposes, ready for `schema::merge`.
///
/// Three blocks and a fourth when a keystone survived. Nothing here writes; the merge does, and
/// it is what honours `user_edited_fields`.
fn into_recipe(base: &Recipe, plan: &GeometryPlan) -> Recipe {
    let mut out = base.clone();
    out.lens.distortion = plan.lens.corrects_distortion();
    out.lens.ca = plan.lens.corrects_ca();
    // The decision is a fraction and the recipe is an integer instruction. Clamped before the
    // cast, so the truncation the lint warns about cannot reach a value outside 0..100.
    out.lens.vignette = vignette_percent(plan.lens.vignette);
    plan.lens.profile_id.clone_into(&mut out.lens.profile);
    out.lens.coefficients = if plan.lens.is_identity() {
        None
    } else {
        Some(LensCoefficients {
            k1: plan.lens.distortion[0],
            k2: plan.lens.distortion[1],
            k3: plan.lens.distortion[2],
            ca_red: plan.lens.ca[0],
            ca_blue: plan.lens.ca[1],
        })
    };

    let primary = plan.primary();
    out.geometry.rotate = plan.rotate_deg;
    // The recipe's crop is `[left, top, right, bottom]`; the contract's rectangle is
    // `[x, y, w, h]`. Two conventions for the same rectangle, and this is the only place in
    // the product they meet.
    out.geometry.crop = [
        primary.rect.x,
        primary.rect.y,
        primary.rect.x + primary.rect.w,
        primary.rect.y + primary.rect.h,
    ];
    out.geometry.perspective = plan.keystone.map(|keystone| Perspective {
        vertical: keystone.vertical,
        horizontal: keystone.horizontal,
        rotate: 0.0,
        scale: keystone.scale,
    });
    out
}

// ---------------------------------------------------------------------------
// The wire shapes
// ---------------------------------------------------------------------------

fn plan_dto(plan: &GeometryPlan, synthetic: bool) -> GeometryPlanDto {
    GeometryPlanDto {
        photo_id: plan.image_id.to_db(),
        scene: plan.scene.as_str().to_string(),
        lens_source: plan.lens.source.as_str().to_string(),
        lens_id: plan.lens.lens_id.clone(),
        lens_profile: plan.lens.profile_id.clone(),
        lens_synthetic: synthetic && plan.lens.profile_id.is_some(),
        distortion: plan.lens.distortion.to_vec(),
        vignette: plan.lens.vignette,
        ca: plan.lens.ca.to_vec(),
        rotate_deg: plan.rotate_deg,
        rotate_conf: plan.rotate_conf,
        keystone_vertical: plan.keystone.map(|k| k.vertical),
        keystone_horizontal: plan.keystone.map(|k| k.horizontal),
        keystone_stretch: plan.keystone.map(|k| k.stretch),
        keystone_verticals: u32::from(plan.keystone.map_or(0, |k| k.verticals)),
        crops: plan.crops.iter().map(crop_dto).collect(),
        primary_crop: u32::try_from(plan.primary_crop).unwrap_or(0),
        kept_original: plan.kept_original_framing(),
        safety: CropSafetyDto {
            faces_intact: plan.safety.faces_intact,
            resolution_ok: plan.safety.resolution_ok,
            content_kept: plan.safety.content_kept,
            faces_checked: plan.safety.faces_checked,
            hands_checked: plan.safety.hands_checked,
            is_evidence: plan.safety.is_evidence(),
            refused: plan.safety.refused.to_vec(),
            refused_names: GeometryCode::REFUSALS
                .iter()
                .map(|code| code.as_str().to_string())
                .collect(),
        },
        reasons: plan.reasons.iter().map(reason_dto).collect(),
        confidence: plan.confidence,
        profile_ver: u32::from(plan.profile_ver),
        analysis_ver: u32::from(plan.analysis_ver),
        rules_ver: u32::from(plan.rules_ver),
        user_edited: plan.user_edited,
    }
}

fn crop_dto(variant: &CropVariant) -> CropVariantDto {
    CropVariantDto {
        purpose: variant.purpose.as_str().to_string(),
        title: variant.purpose.title().to_string(),
        aspect: variant.aspect.as_str().to_string(),
        rect: CropRectDto {
            x: variant.rect.x,
            y: variant.rect.y,
            w: variant.rect.w,
            h: variant.rect.h,
        },
        score: variant.score,
        safe: variant.safe,
    }
}

fn reason_dto(reason: &GeometryReason) -> GeometryReasonDto {
    GeometryReasonDto {
        code: reason.code.as_str().to_string(),
        text: reason.text.clone(),
        weight: reason.weight,
        restraint: reason.code.is_restraint(),
        evidence: reason.evidence.map(|rect| CropRectDto {
            x: rect.x,
            y: rect.y,
            w: rect.w,
            h: rect.h,
        }),
    }
}

/// The recipe's `0..100` integer for a `0..1` decision.
///
/// The decision is a fraction and the recipe is an instruction. Clamped before the conversion
/// rather than after it, so there is no value of the input for which this truncates.
fn vignette_percent(strength: f32) -> i16 {
    // Integer arithmetic after the clamp: `0..=100` has an exact `i16`, and the only way to
    // reach the fallback is a NaN strength, which is a value the contract's `0..1` bound
    // already excludes.
    let scaled = (strength * 100.0).round().clamp(0.0, 100.0);
    (0..=100i16)
        .find(|step| f32::from(*step) >= scaled - 0.5)
        .unwrap_or(0)
}

fn parse_project(id: &str) -> IpcResult<ProjectId> {
    ProjectId::from_db(id).map_err(|_| bad_id("project", id))
}

fn parse_photo(id: &str) -> IpcResult<PhotoId> {
    PhotoId::from_db(id).map_err(|_| bad_id("photo", id))
}

fn bad_id(kind: &str, id: &str) -> IpcError {
    IpcError::from(aura_core::errors::db::statement_failed(
        format!("not a {kind} id: {id}"),
        &std::io::Error::from(std::io::ErrorKind::InvalidInput),
    ))
}
