//! The mask command surface. PHASE-18.
//!
//! Eight commands: four read - the project's status, one photograph's regions, one region as a
//! drawable plane, and what an operation may do through it - one produces regions on demand, one
//! edits a region by hand, one drops that edit, and one lists the vocabulary.
//!
//! # The three things this module cannot do, and could not be made to
//!
//! **It cannot apply a mask to a photograph.** Section 2.2 of the phase document puts every use
//! of a mask in phases 19 to 24, and there is no `apply_mask` here to be tempted by. What this
//! surface hands out is a region and a ceiling; what a later phase does with them is that
//! phase's decision, recorded in that phase's rows.
//!
//! **It cannot return a photograph.** `MaskOverlayDto` carries a width, a height and a plane of
//! alpha bytes - derived geometry about a region. There is no field anywhere on this surface
//! that could hold the pixels of the frame, which is what makes "the mask surface ships no
//! imagery" a property of nine struct definitions rather than a promise about the code that
//! fills them. The photograph itself reaches the panel through the preview surface, which is
//! phase 02's and has its own consent rules.
//!
//! **It cannot clear `userEdited` by accident.** `edit_mask` sets it and takes no argument that
//! unsets it; `regenerate_mask` is a separate command with its own confirmation in the panel.
//! ADR-0037 decision 7, and the store enforces it a second time - the flag is inside the
//! `DELETE`'s own `WHERE`.
//!
//! # Why `allowance` crosses the wire
//!
//! Because the panel needs it and must not compute it. Two implementations of a gating rule is
//! two answers to "may this mask carry skin smoothing", and the one written in TypeScript is the
//! one nobody tests against a fixture. ADR-0038 decision 3.

use std::sync::Arc;

use aura_core::contract::ids::MaskId;
use aura_core::{PhotoId, ProjectId};
use aura_render::contract::render::RenderLevel;
use aura_vision::contract::mask::{
    Mask, MaskKind, MaskOp, MaskPayload, MaskService, ALL_KINDS, OVERLAY_MAX_EDGE,
};
use aura_vision::mask::quality::{self, Operation};
use aura_vision::mask::{errors as mask_errors, store};

use crate::commands::IpcResult;
use crate::contract::ipc::{
    EditMaskInput, EnsureMasksInput, IpcError, MaskAllowanceDto, MaskDto, MaskOpDto,
    MaskOverlayDto, MaskReasonDto, MaskStatusDto,
};
use crate::state::AppState;

/// What the mask panel's project header shows.
///
/// # Errors
///
/// `AURA-DB-3006` when the masks cannot be read.
pub fn mask_status(state: &AppState, project_id: &str) -> IpcResult<MaskStatusDto> {
    let project = parse_project(project_id)?;
    let outline = state.masks().outline(project)?;
    Ok(MaskStatusDto {
        selected: outline.selected,
        masked: outline.masked,
        masks: outline.masks,
        user_edited: outline.user_edited,
        low_quality: outline.low_quality,
        mean_confidence: outline.mean_confidence,
        mean_edge_quality: outline.mean_edge_quality,
        payload_bytes: outline.payload_bytes,
        bytes_per_image: outline.bytes_per_image(),
        model_ver: u32::from(outline.model_ver),
        analysis_ver: u32::from(outline.analysis_ver),
        head_trained: outline.head_trained,
    })
}

/// Every region stored for one photograph, in the frozen class order.
///
/// An empty list means nobody has masked this frame yet. It is not the same as a frame with no
/// regions in it, and the panel renders the two differently - the first offers **Find regions**
/// and the second says the photograph has no people or sky in it.
///
/// # Errors
///
/// `AURA-DB-3006` when the masks cannot be read.
pub fn image_masks(state: &AppState, image_id: &str) -> IpcResult<Vec<MaskDto>> {
    let image = parse_photo(image_id)?;
    let masks = state.masks().masks(image);
    Ok(masks.iter().map(|m| mask_dto(state, m)).collect())
}

/// Produce the named regions for one photograph if they are not already stored.
///
/// Idempotent: a class already stored at the current versions is returned rather than
/// recomputed, and a class a photographer has edited is returned untouched.
///
/// # Errors
///
/// `AURA-ML-5078` when the pixels cannot be read, `AURA-DB-3006` when the store cannot be
/// written.
pub fn ensure_masks(state: &AppState, input: &EnsureMasksInput) -> IpcResult<Vec<MaskDto>> {
    let image = parse_photo(&input.image_id)?;
    let mut kinds = Vec::with_capacity(input.kinds.len());
    for slug in &input.kinds {
        let Some(kind) = MaskKind::parse(slug) else {
            return Err(IpcError::from(mask_errors::edit_refused(format!(
                "unknown mask kind `{slug}`"
            ))));
        };
        kinds.push(kind);
    }
    let masks = state.masks_for(&input.project_id)?.ensure(image, &kinds)?;
    Ok(masks.iter().map(|m| mask_dto(state, m)).collect())
}

/// One region as a plane the panel can draw over a preview.
///
/// # Errors
///
/// `AURA-ML-5082` when the mask is not in the store, `AURA-DB-3006` when it cannot be read.
pub fn mask_overlay(state: &AppState, mask_id: &str) -> IpcResult<MaskOverlayDto> {
    let id = parse_mask(mask_id)?;
    let Some(mask) = state.mask_store().mask(id)? else {
        return Err(IpcError::from(mask_errors::edit_refused(format!(
            "no mask {mask_id}"
        ))));
    };
    // The overlay level is chosen here rather than by the caller: a panel that could ask for a
    // full-resolution plane is a panel that will, on a 60-megapixel frame, once.
    let (w, h) = overlay_size(&mask);
    let plane = store::decode(&mask.payload).resize_bilinear(w, h);
    let bytes: Vec<u8> = plane.a.iter().map(|v| quantise(*v)).collect();
    Ok(MaskOverlayDto {
        id: mask.id.to_db(),
        width: plane.w,
        height: plane.h,
        alpha_base64: crate::preview_commands::base64(&bytes),
        level: RenderLevel::Screen(w, h).as_str().to_string(),
    })
}

/// What one operation may do through one region.
///
/// # Errors
///
/// `AURA-ML-5082` when the mask is not in the store or the operation is not one of the five.
pub fn mask_allowance(
    state: &AppState,
    mask_id: &str,
    operation: &str,
) -> IpcResult<MaskAllowanceDto> {
    let id = parse_mask(mask_id)?;
    let Some(mask) = state.mask_store().mask(id)? else {
        return Err(IpcError::from(mask_errors::edit_refused(format!(
            "no mask {mask_id}"
        ))));
    };
    let op = match operation {
        "local_tone" => Operation::LocalTone,
        "skin_smooth" => Operation::SkinSmooth,
        "micro_retouch" => Operation::MicroRetouch,
        "restoration" => Operation::Restoration,
        "generative_cleanup" => Operation::GenerativeCleanup,
        other => {
            return Err(IpcError::from(mask_errors::edit_refused(format!(
                "unknown operation `{other}`"
            ))))
        }
    };
    let allowance = quality::allowance(&mask, op);
    Ok(MaskAllowanceDto {
        mask_id: mask.id.to_db(),
        operation: op.as_str().to_string(),
        ceiling: allowance.ceiling,
        permitted: allowance.permitted,
        reasons: allowance
            .reasons
            .iter()
            .map(|r| MaskReasonDto {
                code: r.as_str().to_string(),
                text: r.sentence().to_string(),
            })
            .collect(),
    })
}

/// Apply a composition to one region and keep the result as the photographer's own.
///
/// # Errors
///
/// `AURA-ML-5082` when the mask is not in the store or the program is not valid,
/// `AURA-DB-3006` when the row cannot be written.
pub fn edit_mask(state: &AppState, input: &EditMaskInput) -> IpcResult<MaskDto> {
    let id = parse_mask(&input.mask_id)?;
    let Some(existing) = state.mask_store().mask(id)? else {
        return Err(IpcError::from(mask_errors::edit_refused(format!(
            "no mask {}",
            input.mask_id
        ))));
    };

    let mut ops = Vec::with_capacity(input.ops.len() + 1);
    // The mask being edited is always the bottom of the stack. A program that had to push it
    // itself is a program that can forget to, and forgetting produces a mask made only of the
    // brush stroke - which is a region a photographer did not ask for.
    ops.push(MaskOp::Source { id });
    for step in &input.ops {
        ops.push(decode_op(step)?);
    }

    let composed = state.masks().compose(&ops);
    if composed.payload.is_empty() {
        // `compose` is total and returns the empty mask when a program does not mean what it
        // says. Writing it would replace a real region with nothing.
        return Err(IpcError::from(mask_errors::edit_refused(
            "the composition produced an empty region",
        )));
    }

    let edited = Mask {
        id: existing.id,
        image_id: existing.image_id,
        kind: existing.kind,
        identity: existing.identity,
        payload: composed.payload,
        feather: input.feather.unwrap_or(existing.feather).clamp(0.0, 1.0),
        // A photographer's own region is not something AURA is uncertain about. The allowance
        // on a hand-edited mask is one, and this is where that becomes true.
        confidence: 1.0,
        edge_quality: existing.edge_quality.max(0.5),
        edge: existing.edge,
        reasons: vec![aura_vision::contract::mask::MaskReason::UserEdited],
        user_edited: true,
        model_ver: existing.model_ver,
    };
    state.mask_store().save_edit(&edited)?;
    Ok(mask_dto(state, &edited))
}

/// Drop a photographer's edit so the next pass regenerates the region.
///
/// The one command that clears `userEdited`, and it does it by deleting the row rather than by
/// flipping a flag - a regenerated mask is a new measurement, not an old one with its provenance
/// rewritten.
///
/// # Errors
///
/// `AURA-DB-3006` when the row cannot be deleted.
pub fn regenerate_mask(state: &AppState, mask_id: &str) -> IpcResult<bool> {
    let id = parse_mask(mask_id)?;
    Ok(state.mask_store().regenerate(id)?)
}

/// The twenty class slugs, in the frozen iteration order.
///
/// On the wire so the panel's list order comes from the contract rather than from a hand-kept
/// copy in TypeScript - and so a build that adds a class does not need a UI change to show it.
#[must_use]
pub fn mask_kinds() -> Vec<String> {
    ALL_KINDS.iter().map(|k| k.as_str().to_string()).collect()
}

// ---------------------------------------------------------------------------
// Conversion
// ---------------------------------------------------------------------------

fn mask_dto(state: &AppState, mask: &Mask) -> MaskDto {
    let (form, width, height) = match &mask.payload {
        MaskPayload::Rle { w, h, .. } => ("rle", *w, *h),
        MaskPayload::Alpha8 { w, h, .. } => ("alpha8", *w, *h),
    };
    MaskDto {
        id: mask.id.to_db(),
        image_id: mask.image_id.to_db(),
        kind: mask.kind.as_str().to_string(),
        identity_id: mask.identity.map(|id| id.to_db()),
        identity_name: mask.identity.and_then(|id| state.identity_name(id)),
        form: form.to_string(),
        width,
        height,
        bytes: mask.byte_len() as u64,
        feather: mask.feather,
        confidence: mask.confidence,
        edge_quality: mask.edge_quality,
        edge: mask.edge.as_str().to_string(),
        allowance: mask.allowance(),
        allows_aggressive: mask.allows_aggressive(),
        reasons: mask
            .reasons
            .iter()
            .map(|r| MaskReasonDto {
                code: r.as_str().to_string(),
                text: r.sentence().to_string(),
            })
            .collect(),
        user_edited: mask.user_edited,
        model_ver: u32::from(mask.model_ver),
    }
}

/// An alpha in `0.0 ..= 1.0` as a byte, half-up and saturating.
///
/// Written out rather than cast, because `as u8` on an `f32` saturates silently on one end and
/// truncates on the other - and an overlay whose 0.999 came back as 254 would show a hairline
/// where the region is solid.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn quantise(value: f32) -> u8 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    if value >= 1.0 {
        return 255;
    }
    (value * 255.0 + 0.5) as u8
}

/// The plane size an overlay is sent at: the stored aspect, capped on the long edge.
fn overlay_size(mask: &Mask) -> (u32, u32) {
    let (w, h) = mask.payload.dimensions();
    let long = w.max(h);
    if long == 0 {
        return (0, 0);
    }
    if long <= OVERLAY_MAX_EDGE {
        return (w, h);
    }
    let scale = f64::from(OVERLAY_MAX_EDGE) / f64::from(long);
    (scaled(w, scale), scaled(h, scale))
}

/// One edge scaled and rounded, never below one pixel.
///
/// The bound is checked before the cast rather than after it, so no value that could truncate
/// ever reaches one: `OVERLAY_MAX_EDGE` is the ceiling and a scale that produced more than it
/// would be a scale computed from the wrong long edge.
fn scaled(edge: u32, scale: f64) -> u32 {
    let value = (f64::from(edge) * scale).round();
    if value <= 1.0 {
        return 1;
    }
    if value >= f64::from(OVERLAY_MAX_EDGE) {
        return OVERLAY_MAX_EDGE;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        (value as u32).max(1)
    }
}

fn decode_op(step: &MaskOpDto) -> Result<MaskOp, IpcError> {
    match step.op.as_str() {
        "source" => {
            let Some(text) = step.mask_id.as_deref() else {
                return Err(IpcError::from(mask_errors::edit_refused(
                    "a source step named no mask",
                )));
            };
            Ok(MaskOp::Source {
                id: parse_mask(text)?,
            })
        }
        "plane" => {
            let (Some(w), Some(h), Some(alpha)) =
                (step.width, step.height, step.alpha_base64.as_deref())
            else {
                return Err(IpcError::from(mask_errors::edit_refused(
                    "a plane step was missing its dimensions or its alpha",
                )));
            };
            let bytes = crate::preview_commands::from_base64(alpha).ok_or_else(|| {
                IpcError::from(mask_errors::edit_refused(
                    "a plane step was not valid base64",
                ))
            })?;
            if bytes.len() as u64 != u64::from(w) * u64::from(h) {
                return Err(IpcError::from(mask_errors::edit_refused(format!(
                    "a plane step is {}x{} but carried {} bytes",
                    w,
                    h,
                    bytes.len()
                ))));
            }
            Ok(MaskOp::Plane {
                payload: MaskPayload::Alpha8 { w, h, alpha: bytes },
                // A stroke has no class of its own; it becomes part of whatever it is composed
                // with, and `compose` keeps the class of the deeper operand.
                kind: MaskKind::Subject,
            })
        }
        "union" => Ok(MaskOp::Union),
        "intersect" => Ok(MaskOp::Intersect),
        "subtract" => Ok(MaskOp::Subtract),
        "invert" => Ok(MaskOp::Invert),
        "feather" => Ok(MaskOp::Feather {
            amount: step.amount.unwrap_or(0.0),
        }),
        "grow" => Ok(MaskOp::Grow {
            radius: step.radius.unwrap_or(1),
        }),
        "shrink" => Ok(MaskOp::Shrink {
            radius: step.radius.unwrap_or(1),
        }),
        other => Err(IpcError::from(mask_errors::edit_refused(format!(
            "unknown mask operation `{other}`"
        )))),
    }
}

fn parse_project(text: &str) -> Result<ProjectId, IpcError> {
    ProjectId::from_db(text)
        .map_err(|_| IpcError::from(mask_errors::edit_refused(format!("bad project id {text}"))))
}

fn parse_photo(text: &str) -> Result<PhotoId, IpcError> {
    PhotoId::from_db(text)
        .map_err(|_| IpcError::from(mask_errors::edit_refused(format!("bad photo id {text}"))))
}

fn parse_mask(text: &str) -> Result<MaskId, IpcError> {
    MaskId::from_db(text)
        .map_err(|_| IpcError::from(mask_errors::edit_refused(format!("bad mask id {text}"))))
}

/// The service, so a caller that already has one does not build a second.
#[must_use]
pub fn service(state: &AppState) -> Arc<dyn MaskService> {
    state.masks()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_vision::contract::mask::EdgeQuality;

    fn mask(kind: MaskKind, w: u32, h: u32) -> Mask {
        Mask {
            id: MaskId::new(),
            image_id: PhotoId::new(),
            kind,
            identity: None,
            payload: MaskPayload::Alpha8 {
                w,
                h,
                alpha: vec![128; (w * h) as usize],
            },
            feather: 0.0,
            confidence: 0.8,
            edge_quality: 0.8,
            edge: EdgeQuality::Soft,
            reasons: vec![aura_vision::contract::mask::MaskReason::Derived],
            user_edited: false,
            model_ver: 1,
        }
    }

    #[test]
    fn an_overlay_is_capped_on_its_long_edge() {
        let big = mask(MaskKind::Subject, 4000, 3000);
        let (w, h) = overlay_size(&big);
        assert_eq!(w, OVERLAY_MAX_EDGE);
        assert!(h < OVERLAY_MAX_EDGE);
    }

    #[test]
    fn a_small_overlay_is_not_upscaled() {
        let small = mask(MaskKind::Sky, 192, 128);
        assert_eq!(overlay_size(&small), (192, 128));
    }

    #[test]
    fn the_kind_list_is_the_frozen_order() {
        let kinds = mask_kinds();
        assert_eq!(kinds.len(), ALL_KINDS.len());
        assert_eq!(kinds.first().map(String::as_str), Some("skin"));
    }

    #[test]
    fn an_unknown_operation_is_refused_rather_than_ignored() {
        let step = MaskOpDto {
            op: "sharpen".to_string(),
            mask_id: None,
            width: None,
            height: None,
            alpha_base64: None,
            amount: None,
            radius: None,
        };
        assert!(decode_op(&step).is_err());
    }

    #[test]
    fn a_plane_step_whose_bytes_do_not_match_its_size_is_refused() {
        let step = MaskOpDto {
            op: "plane".to_string(),
            mask_id: None,
            width: Some(4),
            height: Some(4),
            alpha_base64: Some(crate::preview_commands::base64(&[0_u8; 3])),
            amount: None,
            radius: None,
        };
        assert!(decode_op(&step).is_err());
    }
}
