//! The gallery consistency command surface. PHASE-25.
//!
//! Nine commands. Five read - the project header, the node tree, one node's strip, one
//! photograph's delta and the outlier queue - one runs the pass, and three record what the
//! photographer decided. ADR-0052 records the shape and what is deliberately absent from it.
//!
//! # What this surface does that no earlier command surface does
//!
//! **It answers about a wedding rather than about a photograph.** Every develop panel since phase
//! 14 answers "what is happening to this frame"; this one answers "what is happening to these four
//! hundred frames, and which of them does not belong". Two consequences follow.
//!
//! The first is that **both denominators are on the wire**. `gallery_status` carries `nodes` and
//! `anchoredNodes`, and a project at 100 % coverage with 20 % anchored has had almost nothing done
//! to it - an unanchored node produces a zero delta for every frame in it, and a zero delta is
//! still a row. Phase 05's rule, at the point where a green number and an untouched gallery look
//! identical.
//!
//! The second is that **the spreads are sent rather than the reduction**. A panel that received
//! "77 % reduced" could not tell 500 K down to 115 K from 20 K down to 4.6 K.
//!
//! # What is not here, and cannot be added without an ADR
//!
//! No strength, no damping, no way to raise a bound, no pixels, no apply and no node editing. See
//! the header of the DTO block in `contract::ipc` and section 8 of ADR-0052.

use aura_brain_gallery::api::Gallery;
use aura_core::contract::gallery::{
    GalleryCode, GalleryOverride, GalleryReason, GalleryService, NodeTarget, NormalisationDelta,
    Outlier, SceneNode,
};
use aura_core::contract::ids::NodeId;
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{PhotoId, ProjectId};

use crate::commands::IpcResult;
use crate::contract::ipc::{
    DisableGalleryInput, GalleryDeltaDto, GalleryOutlierDto, GalleryOverrideInput, GalleryPassDto,
    GalleryPassInput, GalleryReasonDto, GalleryStatusDto, IpcError, NodeTargetDto, PinAnchorInput,
    SceneNodeDto,
};
use crate::state::AppState;

/// What the Consistency panel's project header shows.
///
/// # Errors
///
/// `AURA-DB-3006` when the stored rows cannot be read.
pub fn gallery_status(state: &AppState, project_id: &str) -> IpcResult<GalleryStatusDto> {
    let project = parse_project(project_id)?;
    let gallery = state.gallery();
    let outline = gallery.outline(project)?;
    Ok(GalleryStatusDto {
        photos: outline.photos,
        normalised: outline.normalised,
        coverage: outline.coverage,
        nodes: outline.nodes,
        anchored_nodes: outline.anchored_nodes,
        split_nodes: outline.split_nodes,
        pinned_anchors: outline.pinned_anchors,
        bounded: outline.bounded,
        mood_preserved: outline.mood_preserved,
        user_edited: outline.user_edited,
        outliers: outline.outliers,
        skin_targeted: outline.skin_targeted,
        identities: outline.identities,
        spread_before_cct: outline.spread_before_cct,
        spread_after_cct: outline.spread_after_cct,
        spread_before_ev: outline.spread_before_ev,
        spread_after_ev: outline.spread_after_ev,
        worst_skin_spread: outline.worst_skin_spread,
        untargeted_scenes: outline.untargeted_scenes.clone(),
        // On the wire rather than inferred: a panel that guessed from `skinTargeted == 0` would
        // eventually say "everybody's skin is consistent across this wedding" for a build that
        // cannot look at skin.
        skin_field_available: aura_brain_gallery::SKIN_FIELD_AVAILABLE,
        policy_ver: outline.policy_ver,
    })
}

/// Every node of a project's tree, in capture order of their first frame.
///
/// # Errors
///
/// `AURA-DB-3006` when the nodes cannot be read.
pub fn gallery_nodes(state: &AppState, project_id: &str) -> IpcResult<Vec<SceneNodeDto>> {
    let project = parse_project(project_id)?;
    let nodes = state.gallery().nodes(project)?;
    Ok(nodes.iter().map(to_node_dto).collect())
}

/// One node's deltas, in capture order. What a timeline strip draws.
///
/// A separate command from [`gallery_nodes`] rather than a field on it: a wedding has forty nodes
/// and four thousand frames, and a panel that listed the tree would otherwise pull every delta in
/// the project to draw a header.
///
/// # Errors
///
/// `AURA-DB-3006` when the deltas cannot be read.
pub fn gallery_node_strip(state: &AppState, node_id: &str) -> IpcResult<Vec<GalleryDeltaDto>> {
    let node = parse_node(node_id)?;
    let deltas = state.gallery().deltas_in(node)?;
    Ok(deltas.iter().map(to_delta_dto).collect())
}

/// One photograph's delta, or `None` when it has not been placed in a node.
///
/// `None` is not a zero delta, and a panel that rendered it as one would turn a gap in coverage
/// into a decision.
///
/// # Errors
///
/// `AURA-DB-3006` when the delta cannot be read.
pub fn image_gallery(state: &AppState, photo_id: &str) -> IpcResult<Option<GalleryDeltaDto>> {
    let photo = parse_photo(photo_id)?;
    let delta = state.gallery().delta(photo)?;
    Ok(delta.as_ref().map(to_delta_dto))
}

/// Every frame still out of line, worst first. What phase 27's QC queue reads.
///
/// # Errors
///
/// `AURA-DB-3006` when the outliers cannot be read.
pub fn gallery_outliers(
    state: &AppState,
    project_id: &str,
    limit: usize,
) -> IpcResult<Vec<GalleryOutlierDto>> {
    let project = parse_project(project_id)?;
    let outliers = state.gallery().outliers(project, limit.clamp(1, 500))?;
    Ok(outliers.iter().map(to_outlier_dto).collect())
}

/// Run the consistency pass over a project.
///
/// Runs to completion and returns what it did rather than a job id. ADR-0052 section 5: a node
/// half-solved against one target and half against another has a target that describes neither, so
/// there is no partial state a reader could make sense of and therefore no honest progress to poll.
///
/// # Errors
///
/// `AURA-ML-5123` when the pass cannot complete, `AURA-ML-5129` when the policy table will not
/// load, `AURA-DB-3006` when a statement fails.
pub fn gallery_pass(state: &AppState, input: GalleryPassInput) -> IpcResult<GalleryPassDto> {
    let project = parse_project(&input.project_id)?;
    let frames = state.gallery_frames(&project)?;
    let pass = state.gallery_pass();
    let cancel = CancelToken::new();
    let report = pass.run(project, &frames, None, &NullProgress, &cancel)?;
    Ok(GalleryPassDto {
        nodes: report.nodes,
        anchored: report.anchored,
        split: report.split,
        normalised: report.normalised,
        outliers: report.outliers,
        skin_targets: report.skin_targets,
        spread_before_cct: report.spread_before.0,
        spread_after_cct: report.spread_after.0,
        spread_before_ev: report.spread_before.1,
        spread_after_ev: report.spread_after.1,
        decisions_kept: u32::try_from(report.decisions_kept).unwrap_or(u32::MAX),
        cancelled: false,
        elapsed_ms: report.elapsed_ms,
    })
}

/// Pin or reject one photograph as an anchor of its node.
///
/// Both survive a re-analysis. Section 6.1: "pinned anchors are authoritative, which gives
/// professionals direct control over the look of a scene."
///
/// # Errors
///
/// `AURA-ML-5124` when the node is unknown or the photograph is not in it.
pub fn pin_gallery_anchor(state: &AppState, input: PinAnchorInput) -> IpcResult<()> {
    let node = parse_node(&input.node_id)?;
    let photo = parse_photo(&input.photo_id)?;
    let gallery = state.gallery();
    if input.pinned {
        gallery.pin_anchor(node, photo)?;
    } else {
        gallery.reject_anchor(node, photo)?;
    }
    Ok(())
}

/// Record what the photographer set instead, on one frame.
///
/// **This records the disagreement; it does not move a pixel.** The pixels move when the develop
/// panel renders the frame and `aura_recipe::schema::merge` writes the same values, which is the
/// only function in the workspace permitted to write a recipe. Two writes rather than one,
/// deliberately - phase 15's rule, fourth application.
///
/// # Errors
///
/// `AURA-ML-5125` when the photograph has no delta, the override is empty, or a value is outside
/// its bound. **Refused rather than clamped**: a frame that needs to move further than the bound is
/// a frame whose per-frame estimate is wrong, and phase 15's own override is where that is fixed.
pub fn set_gallery_override(state: &AppState, input: GalleryOverrideInput) -> IpcResult<()> {
    let photo = parse_photo(&input.photo_id)?;
    state.gallery().set_override(
        photo,
        GalleryOverride {
            d_cct: input.d_cct,
            d_tint: input.d_tint,
            d_exposure: input.d_exposure,
            d_contrast: input.d_contrast,
            d_saturation: input.d_saturation,
        },
    )?;
    Ok(())
}

/// Switch the consistency pass off for one photograph, or back on.
///
/// Invariant 8's kill switch at the grain a photographer actually wants: one frame in a gallery
/// that should not be matched to anything.
///
/// # Errors
///
/// `AURA-ML-5125` when the photograph has no delta.
pub fn disable_gallery(state: &AppState, input: DisableGalleryInput) -> IpcResult<()> {
    let photo = parse_photo(&input.photo_id)?;
    state.gallery().set_enabled(photo, !input.disabled)?;
    Ok(())
}

/// Every gallery reason code, for the panel's filter chips.
///
/// The slug is what a filter matches on and the sentence is what a person reads; neither is stored,
/// which is what makes `docs/gallery-consistency.md` translatable and a stored row cost a code.
/// # Errors
///
/// Never. The signature matches every other legend command on this surface so the window can call
/// them all the same way.
pub fn gallery_reason_codes(_state: &AppState) -> IpcResult<Vec<GalleryReasonDto>> {
    Ok(GalleryCode::ALL
        .into_iter()
        .map(|code| GalleryReasonDto {
            code: code.as_str().to_string(),
            text: code.user_text().to_string(),
            withdraws: code.withdraws(),
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Wire conversions
// ---------------------------------------------------------------------------

fn to_node_dto(node: &SceneNode) -> SceneNodeDto {
    SceneNodeDto {
        node_id: node.id.to_db(),
        parent_id: node.parent.map(|id| id.to_db()),
        segment_id: node.segment_id.to_db(),
        label: node.label.clone(),
        scene: node.scene.as_str().to_string(),
        image_count: u32::try_from(node.image_ids.len()).unwrap_or(u32::MAX),
        anchors: node.anchors.iter().map(PhotoId::to_db).collect(),
        target: node.target.as_ref().map(to_target_dto),
        reasons: node.reasons.iter().map(to_reason_dto).collect(),
    }
}

fn to_target_dto(target: &NodeTarget) -> NodeTargetDto {
    NodeTargetDto {
        cct_k: target.cct_k,
        cct_tol: target.cct_tol,
        tint: target.tint,
        tint_tol: target.tint_tol,
        subject_luma: target.subject_luma,
        luma_tol: target.luma_tol,
        contrast: target.contrast,
        saturation: target.saturation,
        anchor_count: target.anchor_count,
        cohesion: target.cohesion,
    }
}

fn to_delta_dto(delta: &NormalisationDelta) -> GalleryDeltaDto {
    let skin = delta.skin_correction;
    GalleryDeltaDto {
        photo_id: delta.image_id.to_db(),
        node_id: delta.node_id.to_db(),
        d_exposure: delta.d_exposure,
        d_cct: delta.d_cct,
        d_tint: delta.d_tint,
        d_contrast: delta.d_contrast,
        d_saturation: delta.d_saturation,
        from_exposure_ev: delta.from_exposure_ev,
        from_cct_k: delta.from_cct_k,
        from_tint: delta.from_tint,
        damping: delta.damping,
        bounded_by: delta.bounded_by.map(|bound| bound.as_str().to_string()),
        magnitude: delta.magnitude(),
        skin_identity: skin.map(|correction| correction.identity.to_db()),
        skin_de00_before: skin.map(|correction| correction.de00_before),
        skin_de00_after: skin.map(|correction| correction.de00_after),
        confidence: delta.confidence,
        reasons: delta.reasons.iter().map(to_reason_dto).collect(),
        user_edited: delta.user_edited,
    }
}

fn to_outlier_dto(outlier: &Outlier) -> GalleryOutlierDto {
    GalleryOutlierDto {
        photo_id: outlier.image_id.to_db(),
        node_id: outlier.node_id.to_db(),
        // Assembled here rather than in the panel, so this and phase 27's QC ticket say the same
        // thing about the same frame.
        description: outlier.describe(),
        residual_cct: outlier.residual_cct,
        residual_tint: outlier.residual_tint,
        residual_exposure: outlier.residual_exposure,
        residual_skin_de00: outlier.residual_skin_de00,
        deviation: outlier.deviation,
        reasons: outlier.reasons.iter().map(to_reason_dto).collect(),
    }
}

fn to_reason_dto(reason: &GalleryReason) -> GalleryReasonDto {
    GalleryReasonDto {
        code: reason.code.as_str().to_string(),
        text: reason.text.clone(),
        withdraws: reason.code.withdraws(),
    }
}

/// An id the window sent that this build cannot parse.
///
/// `AURA-ML-5125` rather than a generic bad-request, because every id on this surface arrives from
/// a row this same surface handed out - so an unparseable one is the panel and the catalog
/// disagreeing about the tree rather than a person typing something wrong.
fn refused(detail: impl Into<String>) -> IpcError {
    IpcError::from(aura_core::errors::ml::gallery_override_refused(detail))
}

fn parse_project(id: &str) -> IpcResult<ProjectId> {
    ProjectId::from_db(id).map_err(|_| refused(format!("`{id}` is not a project id")))
}

fn parse_photo(id: &str) -> IpcResult<PhotoId> {
    PhotoId::from_db(id).map_err(|_| refused(format!("`{id}` is not a photograph id")))
}

fn parse_node(id: &str) -> IpcResult<NodeId> {
    NodeId::from_db(id).map_err(|_| refused(format!("`{id}` is not a node id")))
}

/// The service, for a caller that already has a state and wants the trait rather than the wire.
#[must_use]
pub fn service(state: &AppState) -> std::sync::Arc<Gallery> {
    state.gallery()
}
