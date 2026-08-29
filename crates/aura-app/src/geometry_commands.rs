//! The geometry command surface. PHASE-23.
//!
//! Nine commands. Four read - the project coverage, one photograph's plan, its safe variants and
//! the review queue - one runs the resumable pass, three record what the photographer decided,
//! and one is the panel's own legend. ADR-0048 records the shape and what is deliberately absent
//! from it.
//!
//! # What this module does that the restoration one does not
//!
//! **Its revert is a command rather than a field.** Section 13's fifth acceptance criterion is
//! that the original framing is always one click away, and `revert_geometry` takes a photograph
//! and nothing else. A revert expressed as one boolean inside a payload with six other optional
//! fields is a revert every caller has to assemble correctly, and the failure mode - a rectangle
//! that stays `user_edited` and is never revisited - is silent.
//!
//! **It publishes what it refused.** `GeometryStatusDto::crop_refusals` is the histogram behind
//! "AURA cropped almost nothing in this wedding", which has six causes and five of them are
//! somebody else's placeholder. The same shape phase 22 gave `sharpen_refusals`, for the same
//! reason.
//!
//! **It carries both safety numbers.** `faces_checked` beside `faces_cut`. Over a wedding whose
//! detector found nothing, zero faces cut is arithmetic; the denominator is how a caller finds
//! that out, and on this build it is zero everywhere.
//!
//! # What is not here, and cannot be added without an ADR
//!
//! **No command returns pixels.** The panel renders its preview through phase 14's `render` from
//! the pixels it already has; what this surface adds is the rectangle that render should be made
//! with. Phase 13's rule: evidence can never be a pixel.
//!
//! **No command can widen a safety rule.** There is no field on any input here that says faces may
//! be cut, and no per-project setting that lowers the resolution floor. What a photographer *can*
//! do is crop one photograph of their own as tightly as they like: that is one frame they are
//! looking at rather than a rule applied to four hundred they are not, it is stored with
//! `user_edited` set and never re-cropped, and there is nowhere on this surface to say that
//! cutting faces is acceptable in general.
//!
//! **No command can scale, fill or upscale.** Section 2.2 puts generative fill in phase 24 and
//! panoramas out of scope; there is no field here that could express either, migration 23 has no
//! column for one, and `crates/aura-geometry/tests/boundaries.rs` fails the build if the words
//! appear in the crate.

use aura_core::contract::composition::Box2;
use aura_core::contract::geometry::{
    AspectRatio, GeometryCode, GeometryOverride, GeometryPlan, GeometryService,
};
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{PhotoId, Priority, ProjectId};
use rusqlite::OptionalExtension;

use crate::commands::IpcResult;
use crate::contract::ipc::{
    AcceptGeometryInput, CropRectDto, CropSafetyDto, CropVariantDto, GeometryLensMissDto,
    GeometryPassDto, GeometryPassInput, GeometryPlanDto, GeometryReasonDto, GeometryRefusalDto,
    GeometryReviewInput, GeometryStatusDto, IpcError, KeystoneDto, ProtectedRegionDto,
    RevertGeometryInput, SetGeometryOverrideInput,
};
use crate::state::AppState;

/// What the Geometry panel's project header shows.
///
/// # Errors
///
/// `AURA-DB-3006` when the stored plans cannot be read, and `AURA-ML-5112` when the lens profile
/// or crop rule tables will not load.
pub fn geometry_status(state: &AppState, project_id: &str) -> IpcResult<GeometryStatusDto> {
    let project = parse_project(project_id)?;
    let outline = state.geometry(&project)?.outline(project)?;
    Ok(GeometryStatusDto {
        photos: count64(outline.photos),
        planned: count64(outline.planned),
        coverage: outline.coverage,
        acted_on: count64(outline.acted_on),
        kept_original: count64(outline.kept_original),
        conservatism: outline.conservatism(),
        straightened: count64(outline.straightened),
        mean_rotation_deg: outline.mean_rotation_deg,
        keystoned: count64(outline.keystoned),
        cropped: count64(outline.cropped),
        variants: count64(outline.variants),
        crop_refusals: outline
            .crop_refusals
            .iter()
            .map(|(code, count)| refusal_dto(*code, count64(*count)))
            .collect(),
        lens_sources: outline.lens_sources.iter().copied().map(count64).collect(),
        lens_source_names: aura_core::contract::geometry::LensSource::ALL
            .iter()
            .map(|source| source.as_str().to_string())
            .collect(),
        lenses_missing: outline
            .lenses_missing
            .iter()
            .map(|(lens, count)| GeometryLensMissDto {
                lens: lens.clone(),
                count: count64(*count),
            })
            .collect(),
        faces_checked: count64(outline.faces_checked),
        faces_cut: count64(outline.faces_cut),
        user_edited: count64(outline.user_edited),
        pending_review: count64(outline.pending_review),
    })
}

/// One photograph's geometry plan, or `None` when it has not been planned.
///
/// # Errors
///
/// `AURA-DB-3006` when the plan cannot be read.
pub fn image_geometry(state: &AppState, photo_id: &str) -> IpcResult<Option<GeometryPlanDto>> {
    let photo = parse_photo(photo_id)?;
    let project = project_of(state, photo)?;
    let plan = state.geometry(&project)?.of_image(photo)?;
    Ok(plan.as_ref().map(|plan| to_dto(state, plan)))
}

/// Every safe crop variant for one photograph.
///
/// **Separate from `image_geometry` because phase 29 wants this and nothing else.** An album
/// layout that had to decode a whole plan to take one list out of it is a layout pass parsing
/// reason codes it never renders.
///
/// # Errors
///
/// `AURA-DB-3006` when the variants cannot be read.
pub fn geometry_variants(state: &AppState, photo_id: &str) -> IpcResult<Vec<CropVariantDto>> {
    let photo = parse_photo(photo_id)?;
    let project = project_of(state, photo)?;
    let service = state.geometry(&project)?;
    // The plan rather than `GeometryService::variants`, because the ordinal is the thing a caller
    // acts on - `primary_crop` indexes it, and `set_geometry_override` names it - and `variants`
    // returns the safe ones only. Numbering a filtered list from zero would hand phase 29 an
    // ordinal that addresses a different rectangle from the one it asked about, which is the worst
    // failure this surface could have: silently correct-looking and wrong.
    let Some(plan) = service.of_image(photo)? else {
        return Ok(Vec::new());
    };
    let frame_aspect = frame_aspect_of(state, photo);
    Ok(plan
        .crops
        .iter()
        .enumerate()
        .filter(|(_, variant)| variant.safe)
        .map(|(ordinal, variant)| variant_dto(ordinal, variant, frame_aspect))
        .collect())
}

/// The frames whose geometry is worth a photographer's attention, least confident first.
///
/// A queue, not a shortlist: nothing here says which frames to keep.
///
/// # Errors
///
/// `AURA-DB-3006` when the plans cannot be read.
pub fn geometry_review_queue(
    state: &AppState,
    input: &GeometryReviewInput,
) -> IpcResult<Vec<String>> {
    let project = parse_project(&input.project_id)?;
    let limit = input.limit.unwrap_or(50).clamp(1, 500) as usize;
    let ids = state.geometry(&project)?.review_queue(project, limit)?;
    Ok(ids.into_iter().map(|id| id.to_db()).collect())
}

/// Record that a photographer has looked at one plan and agrees.
///
/// Sets `reviewed` and does **not** set `user_edited`: agreeing with a proposal is not making one,
/// and a later re-analysis with a measured lens profile should still improve a frame somebody
/// merely approved. Phase 15's distinction, inherited.
///
/// # Errors
///
/// `AURA-ML-5111` when the photograph has no plan.
pub fn accept_geometry(state: &AppState, input: &AcceptGeometryInput) -> IpcResult<()> {
    let photo = parse_photo(&input.photo_id)?;
    let project = project_of(state, photo)?;
    state.geometry(&project)?.accept(photo)?;
    Ok(())
}

/// Give a photograph back the framing it was shot at.
///
/// **The one click of section 13.** Clears the crop, the rotation, the keystone and `user_edited`
/// together, so automation resumes on the frame rather than stopping on it forever.
///
/// # Errors
///
/// `AURA-ML-5111` when the photograph has no plan.
pub fn revert_geometry(
    state: &AppState,
    input: &RevertGeometryInput,
) -> IpcResult<GeometryPlanDto> {
    let photo = parse_photo(&input.photo_id)?;
    let project = project_of(state, photo)?;
    let plan = state
        .geometry(&project)?
        .set_override(photo, &GeometryOverride::reverted())?;
    Ok(to_dto(state, &plan))
}

/// Record the framing a photographer chose for one photograph.
///
/// **No field here widens a safety rule.** See the module header and ADR-0048 section 3.
///
/// # Errors
///
/// `AURA-ML-5111` when the photograph has no plan, when the rectangle is not inside the frame,
/// when the aspect is not one of the five this phase knows, and when the change asks for
/// nothing.
pub fn set_geometry_override(
    state: &AppState,
    input: &SetGeometryOverrideInput,
) -> IpcResult<GeometryPlanDto> {
    let photo = parse_photo(&input.photo_id)?;
    let project = project_of(state, photo)?;

    let aspect = match input.aspect.as_deref() {
        None => None,
        Some(text) => {
            // Named rather than defaulted. `AspectRatio::from_str_or_original` reads an unknown
            // string as the whole frame, which is the right reading for a stored row written by an
            // older build and the wrong one for a request: a caller that sent `3:2` meant
            // something, and quietly delivering the original framing is a crop nobody chose.
            let parsed = AspectRatio::from_str_or_original(text);
            if parsed.as_str() != text {
                return Err(IpcError::from(
                    aura_geometry::errors::geometry_edit_refused(format!(
                        "`{text}` is not one of original, 4:5, 5:4, 1:1 or 16:9"
                    )),
                ));
            }
            Some(parsed)
        }
    };

    let change = GeometryOverride {
        crop: input.crop.as_ref().map(|rect| Box2 {
            x: rect.x,
            y: rect.y,
            w: rect.w,
            h: rect.h,
        }),
        aspect,
        rotate_deg: input.rotate_deg,
        distortion: input.distortion,
        vignette: input.vignette,
        ca: input.ca,
        revert: false,
    };
    if change.is_empty() {
        return Err(IpcError::from(
            aura_geometry::errors::geometry_edit_refused(
                "that change asks for nothing; use the revert to restore the original framing",
            ),
        ));
    }

    let plan = state.geometry(&project)?.set_override(photo, &change)?;
    // The plan as it now stands rather than as it was asked for. Phase 15's rule: a row with
    // `user_edited = 1` still carries AURA's own numbers, and a panel that echoed the request
    // would hide the disagreement phase 30's learning loop reads.
    Ok(to_dto(state, &plan))
}

/// Run the resumable geometry pass.
///
/// # Errors
///
/// `AURA-DB-3006` when the pending set cannot be read, and `AURA-ML-5112` when the tables will not
/// load. Per-photograph failures are counted in the report rather than returned.
pub fn geometry_pass(state: &AppState, input: &GeometryPassInput) -> IpcResult<GeometryPassDto> {
    let project = parse_project(&input.project_id)?;
    let priority = match input.priority.as_deref() {
        Some("visible") => Priority::Visible,
        Some("interactive") => Priority::Interactive,
        Some("background") => Priority::Background,
        _ => Priority::AiBatch,
    };

    let pass = state
        .geometry_pass(&input.project_id)?
        .enabled(input.enabled.unwrap_or(true));

    let cancel = CancelToken::new();
    let progress = NullProgress;
    let report = pass.run(&project, priority, &cancel, &progress)?;

    Ok(GeometryPassDto {
        planned: count(report.planned),
        failed: count(report.failed),
        acted_on: count(report.acted_on),
        kept_original: count(report.kept_original),
        conservatism: report.conservatism(),
        straightened: count(report.straightened),
        keystoned: count(report.keystoned),
        variants: count(report.variants),
        reference_profiles: count(report.reference_profiles),
        low_confidence: count(report.low_confidence),
        lenses_missing: report.lenses_missing.clone(),
        unlisted_scenes: report.unlisted_scenes.clone(),
        crop_refusals: report
            .refusals
            .iter()
            .map(|(code, count_of)| refusal_dto(*code, count(*count_of)))
            .collect(),
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
pub fn geometry_reason_codes() -> Vec<GeometryReasonDto> {
    GeometryCode::ALL
        .iter()
        .map(|code| GeometryReasonDto {
            code: code.as_str().to_string(),
            text: code.user_text().to_string(),
            weight: 0.0,
            refusal: code.is_refusal(),
            safety: code.is_safety_refusal(),
            area: None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Conversion
// ---------------------------------------------------------------------------

/// One count as the wire's integer, saturating rather than truncating.
///
/// Phase 22's note applies unchanged: every caller counts photographs or regions in one project,
/// so the saturation is unreachable, and it is written as a saturation anyway because a cast that
/// wrapped would turn a very large wedding into a small number - and a small number is the one
/// failure a coverage report cannot show.
fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn count64(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn rect_dto(area: Box2) -> CropRectDto {
    CropRectDto {
        x: area.x,
        y: area.y,
        w: area.w,
        h: area.h,
    }
}

fn refusal_dto(code: GeometryCode, count: u32) -> GeometryRefusalDto {
    GeometryRefusalDto {
        code: code.as_str().to_string(),
        text: code.user_text().to_string(),
        safety: code.is_safety_refusal(),
        count,
    }
}

/// The photograph's width divided by its height, read from the catalog.
///
/// **Read rather than derived.** A normalised rectangle carries no aspect of its own, and
/// `CropVariant::long_edge_fraction` needs the frame's to say which of a variant's two sides is
/// its long edge - so a 4:5 crop of a landscape frame and of a portrait frame keep very different
/// shares. Guessing one is how a resolution figure comes out plausible and wrong on half a
/// wedding. A photograph whose dimensions the catalog does not carry gets `None`, and the wire
/// then carries no share at all rather than a made-up one.
fn frame_aspect_of(state: &AppState, photo: PhotoId) -> Option<f32> {
    let key = photo.to_db();
    let size: Option<(i64, i64)> = state
        .catalog()
        .read(move |conn| {
            conn.query_row(
                "SELECT width_px, height_px FROM photo WHERE photo_id = ?1",
                rusqlite::params![key],
                |row| Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, Option<i64>>(1)?)),
            )
            .optional()
            .map(|found| found.and_then(|(w, h)| w.zip(h)))
            .map_err(|e| aura_core::errors::db::statement_failed("photo size", &e))
        })
        .unwrap_or(None);
    let (width, height) = size?;
    if width <= 0 || height <= 0 {
        return None;
    }
    Some((width as f32) / (height as f32))
}

fn variant_dto(
    ordinal: usize,
    variant: &aura_core::contract::geometry::CropVariant,
    frame_aspect: Option<f32>,
) -> CropVariantDto {
    CropVariantDto {
        ordinal: count(ordinal),
        aspect: variant.aspect.as_str().to_string(),
        purpose: variant.purpose.as_str().to_string(),
        rect: rect_dto(variant.rect),
        score: variant.score,
        safe: variant.safe,
        refusal: None,
        long_edge_fraction: frame_aspect.map(|aspect| variant.long_edge_fraction(aspect)),
    }
}

fn to_dto(state: &AppState, plan: &GeometryPlan) -> GeometryPlanDto {
    let frame_aspect = frame_aspect_of(state, plan.image_id);
    GeometryPlanDto {
        photo_id: plan.image_id.to_db(),
        scene: plan.scene.as_str().to_string(),
        lens_source: plan.lens.source.as_str().to_string(),
        lens_profile: plan.lens.profile_id.clone(),
        lens_distortion: plan.lens.distortion,
        lens_vignette: plan.lens.vignette,
        lens_ca: plan.lens.ca,
        lens_measured: plan.lens.source.is_measured(),
        rotate_deg: plan.rotate_deg,
        rotate_conf: plan.rotate_conf,
        keystone: plan.keystone.map(|k| KeystoneDto {
            vertical: k.vertical,
            horizontal: k.horizontal,
            stretch: k.stretch,
            convergence: k.convergence,
        }),
        crops: plan
            .crops
            .iter()
            .enumerate()
            .map(|(ordinal, variant)| {
                let mut dto = variant_dto(ordinal, variant, frame_aspect);
                // A refused variant carries the code that refused it, derived exactly as
                // `GeometryStore::refusal_for` derives the value it writes into
                // `geometry_crop.refusal` - the plan's most specific safety refusal, and
                // `geometry_variant_unsafe` when it carries none. Deriving it a second way here
                // would put a different sentence in the panel from the one in the row, and the
                // row is what a support case reads.
                if !variant.safe {
                    let code = plan
                        .reasons
                        .iter()
                        .map(|reason| reason.code)
                        .find(|code| {
                            code.is_safety_refusal() && *code != GeometryCode::VariantUnsafe
                        })
                        .unwrap_or(GeometryCode::VariantUnsafe);
                    dto.refusal = Some(code.as_str().to_string());
                }
                dto
            })
            .collect(),
        primary_crop: count(plan.primary_crop),
        safety: CropSafetyDto {
            faces_intact: plan.safety.faces_intact,
            resolution_ok: plan.safety.resolution_ok,
            content_kept: plan.safety.content_kept,
            considered: plan.safety.considered,
            at_risk: plan.safety.at_risk,
            long_edge_fraction: plan.safety.long_edge_fraction,
            regions: plan
                .safety
                .regions
                .iter()
                .map(|region| ProtectedRegionDto {
                    kind: region.kind.as_str().to_string(),
                    text: region.kind.user_text().to_string(),
                    area: rect_dto(region.area),
                    identity_id: region.identity.map(|id| id.to_db()),
                })
                .collect(),
        },
        reasons: plan
            .reasons
            .iter()
            .map(|reason| GeometryReasonDto {
                code: reason.code.as_str().to_string(),
                text: reason.code.user_text().to_string(),
                weight: reason.weight,
                refusal: reason.code.is_refusal(),
                safety: reason.code.is_safety_refusal(),
                area: reason.evidence.map(rect_dto),
            })
            .collect(),
        confidence: plan.confidence,
        kept_original: plan.is_identity(),
        user_edited: plan.user_edited,
        reviewed: plan.reviewed,
        versions: vec![plan.analysis_ver, plan.profile_ver],
    }
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

fn parse_project(id: &str) -> IpcResult<ProjectId> {
    ProjectId::from_db(id).map_err(|_| {
        IpcError::from(aura_geometry::errors::geometry_edit_refused(format!(
            "`{id}` is not a project id"
        )))
    })
}

fn parse_photo(id: &str) -> IpcResult<PhotoId> {
    PhotoId::from_db(id).map_err(|_| {
        IpcError::from(aura_geometry::errors::geometry_edit_refused(format!(
            "`{id}` is not a photograph id"
        )))
    })
}

/// Which project a photograph belongs to.
fn project_of(state: &AppState, photo: PhotoId) -> IpcResult<ProjectId> {
    Ok(state.project_of(photo)?)
}
