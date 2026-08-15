//! The moments half of the command surface.
//!
//! Nine commands, and the same rules the other halves follow plus one this half adds.
//!
//! **Nothing here blocks for long, except the one command that says it does.**
//! `moment_status`, `list_moments`, `moment_of_image` and `moment_duplicates` read the
//! catalog. `group_moments` is a whole-project pass; the UI calls it from a job rather
//! than from a click handler, and it is cancellable.
//!
//! **The grid tells the truth about what is missing.** An empty moments view on an
//! unembedded project is a support ticket, so `MomentStatusDto` carries the groupable
//! count separately from the photograph count - an ungrouped frame with no embedding is
//! a phase 05 gap and reporting it as a phase 08 failure would send somebody looking in
//! the wrong place.
//!
//! **The photographer outranks the automation, and both sides of a split are locked.**
//! ADR-0018 decision 2, which is ADR-0016 decision 2 one phase on: a split is one
//! statement about two moments, and locking one side would let the next re-grouping put
//! them back together from the other.
//!
//! **Nothing here can reject a photograph.** Five commands change a grouping, one moves
//! a hint, three are reads. `set_keep_hint` is the closest this surface comes to a
//! culling decision and it deliberately is not one: it moves where phase 12 starts and
//! changes nothing about what phase 12 may look at. Section 2.2, kept structural.

use aura_core::contract::moment::{DuplicateSet, ImageId, Moment, MomentOutline, MomentService};
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{MomentId, PhotoId};

use crate::commands::IpcResult;
use crate::contract::ipc::{
    DuplicateSetDto, GroupMomentsInput, IpcError, LockMomentInput, MergeMomentsInput, MomentDto,
    MomentEditDto, MomentFrameDto, MomentHandleDto, MomentListDto, MomentStatusDto, MomentsInput,
    SetKeepHintInput, SplitMomentInput,
};
use crate::state::AppState;

/// What the moments view opens with.
///
/// Section 11 budgets 60 ms for an expand or collapse; this is the whole list, which the
/// grid virtualises exactly as it virtualises frames. A wedding has a few hundred to a
/// few thousand moments, so the list is bounded by the project rather than by a
/// constant - the same shape `list_images` has.
///
/// # Errors
///
/// `AURA-DB-3006` when the moments cannot be read, `AURA-ML-5031` when the shipped
/// grouping thresholds will not load.
pub fn list_moments(state: &AppState, input: &MomentsInput) -> IpcResult<MomentListDto> {
    let project = parse_project(&input.project_id)?;
    let moments = state.moments()?;
    let outline = moments.outline(project)?;

    let wanted = input
        .segment_id
        .as_deref()
        .and_then(|text| aura_core::SegmentId::from_db(text).ok());

    Ok(MomentListDto {
        moments: outline
            .moments
            .iter()
            .filter(|moment| wanted.is_none_or(|segment| moment.segment_id == segment))
            .map(moment_dto)
            .collect(),
        coverage: outline.coverage,
        embed_ver: outline.embed_ver,
        group_ver: outline.group_ver,
        profile_ver: outline.profile_ver,
    })
}

/// What the moments view's header shows.
///
/// # Errors
///
/// `AURA-DB-3006`, or `AURA-ML-5031` when the thresholds will not load.
pub fn moment_status(state: &AppState, project_id: &str) -> IpcResult<MomentStatusDto> {
    let project = parse_project(project_id)?;
    let store = state.moment_store();
    let moments = state.moments()?;
    let (photos, groupable, grouped, count, locked) = store.coverage(&project)?;
    let outline = moments.outline(project)?;
    let [identical, near_identical, variant] = outline.duplicate_counts();

    Ok(MomentStatusDto {
        photos,
        groupable,
        grouped,
        coverage: outline.coverage,
        moments: count,
        locked,
        mean_size: outline.mean_size(),
        bursts: outline
            .moments
            .iter()
            .map(|moment| i64::try_from(moment.bursts.len()).unwrap_or(0))
            .sum(),
        duplicates: [
            i64::try_from(identical).unwrap_or(0),
            i64::try_from(near_identical).unwrap_or(0),
            i64::try_from(variant).unwrap_or(0),
        ],
        median_interval_ms: state.moment_median_interval(&project)?,
        // Recomputed from the stored result rather than remembered from the pass, for
        // `story_status`'s reason: a flag is a property of the grouping that is in the
        // catalog, not of a run that has finished.
        implausible: is_implausible(&outline),
        embed_ver: outline.embed_ver,
        group_ver: outline.group_ver,
        profile_ver: outline.profile_ver,
    })
}

/// The moment one photograph is in, for the grid and the Explain panel.
///
/// # Errors
///
/// `AURA-DB-3006` when the moment cannot be read.
pub fn moment_of_image(state: &AppState, photo_id: &str) -> IpcResult<Option<MomentDto>> {
    let photo = parse_photo(photo_id)?;
    let moments = state.moments()?;
    Ok(moments.moment_of(photo)?.as_ref().map(moment_dto))
}

/// The duplicate sets inside one moment, for the review panel.
///
/// Only sets that constrain delivery come back - `identical` and `near_identical`. The
/// alternatives phase 12 chooses between are `MomentDto::frames` grouped by `burstIx`;
/// see ADR-0017 section 7.
///
/// # Errors
///
/// `AURA-DB-3006` when the sets cannot be read.
pub fn moment_duplicates(state: &AppState, moment_id: &str) -> IpcResult<Vec<DuplicateSetDto>> {
    let moment = parse_moment(moment_id)?;
    let moments = state.moments()?;
    Ok(moments
        .duplicates(moment)?
        .iter()
        .map(duplicate_dto)
        .collect())
}

/// Group every photograph that has an embedding into moments.
///
/// Replaces the previous grouping and preserves every moment the photographer has
/// locked - the frames inside a locked moment are removed from the pass's input rather
/// than reconciled afterwards.
///
/// # Errors
///
/// `AURA-DB-3006` on a catalog failure, `AURA-ML-5011` on cancellation.
pub fn group_moments(state: &AppState, input: &GroupMomentsInput) -> IpcResult<MomentStatusDto> {
    let project = parse_project(&input.project_id)?;
    let moments = state.moments()?;
    let context = state.moment_pass_context()?;
    moments.group(&project, &context, &NullProgress, &CancelToken::new())?;
    moment_status(state, &input.project_id)
}

/// Break a moment in two at a photograph. Locks both halves.
///
/// # Errors
///
/// `AURA-ML-5029` when the moment is unknown, the photograph is not in it, or the split
/// would leave a side empty.
pub fn split_moment(state: &AppState, input: &SplitMomentInput) -> IpcResult<MomentHandleDto> {
    let moment = parse_moment(&input.moment_id)?;
    let photo = parse_photo(&input.photo_id)?;
    let moments = state.moments()?;
    let created = moments.split(moment, photo)?;
    Ok(MomentHandleDto {
        id: created.to_db(),
    })
}

/// Fold two moments into one. Locks the survivor.
///
/// # Errors
///
/// `AURA-ML-5029` when either is unknown, or they belong to different weddings or
/// chapters.
pub fn merge_moments(state: &AppState, input: &MergeMomentsInput) -> IpcResult<MomentHandleDto> {
    let a = parse_moment(&input.moment_id_a)?;
    let b = parse_moment(&input.moment_id_b)?;
    let moments = state.moments()?;
    let survivor = moments.merge(a, b)?;
    Ok(MomentHandleDto {
        id: survivor.to_db(),
    })
}

/// Pin a moment against re-analysis, or release it.
///
/// # Errors
///
/// `AURA-ML-5029` when the moment is unknown.
pub fn lock_moment(state: &AppState, input: &LockMomentInput) -> IpcResult<MomentHandleDto> {
    let moment = parse_moment(&input.moment_id)?;
    let moments = state.moments()?;
    moments.set_locked(moment, input.locked)?;
    Ok(MomentHandleDto { id: moment.to_db() })
}

/// Record which frame of a duplicate set the photographer would keep.
///
/// Moves the hint. Culls nothing.
///
/// # Errors
///
/// `AURA-ML-5029` when the moment is unknown or the photograph is not in a set.
pub fn set_keep_hint(state: &AppState, input: &SetKeepHintInput) -> IpcResult<MomentHandleDto> {
    let moment = parse_moment(&input.moment_id)?;
    let photo = parse_photo(&input.photo_id)?;
    let moments = state.moments()?;
    moments.set_keep_hint(moment, photo)?;
    Ok(MomentHandleDto { id: moment.to_db() })
}

/// Undo the last grouping edit in a project.
///
/// # Errors
///
/// `AURA-ML-5029` when there is nothing to undo.
pub fn undo_moment_edit(state: &AppState, project_id: &str) -> IpcResult<MomentEditDto> {
    let project = parse_project(project_id)?;
    let moments = state.moments()?;
    let edit = moments.undo(project)?;
    Ok(MomentEditDto {
        action: edit.action,
        moment_id: edit.moment.to_db(),
        other_id: edit.other.map(|id| id.to_db()),
        photo_id: edit.image.map(|id| id.to_db()),
        moment_size: edit.moment_size,
    })
}

// -- shaping -----------------------------------------------------------------

/// One moment on the wire.
fn moment_dto(moment: &Moment) -> MomentDto {
    let suppressed = moment.suppressed_candidates();
    let burst_of = |image: ImageId| -> u32 {
        moment
            .bursts
            .iter()
            .position(|burst| burst.contains(&image))
            .map_or(0, |index| u32::try_from(index).unwrap_or(0))
    };

    // The cover is the first set's keep hint when the moment has a set, and the first
    // frame otherwise. Not a decision about what a client sees - phase 12 owns that -
    // only about which of a stack's frames is on top of the pile.
    let cover = moment
        .duplicate_sets
        .first()
        .map(|set| set.keep_hint)
        .or_else(|| moment.image_ids.first().copied());

    MomentDto {
        moment_id: moment.id.to_db(),
        segment_id: (moment.segment_id != aura_brain_wedding::moments::moment::UNPLACED)
            .then(|| moment.segment_id.to_db()),
        cover: cover.map(|id| id.to_db()).unwrap_or_default(),
        frames: moment
            .image_ids
            .iter()
            .enumerate()
            .map(|(position, image)| MomentFrameDto {
                photo_id: image.to_db(),
                position: u32::try_from(position).unwrap_or(0),
                burst_ix: burst_of(*image),
                suppressed: suppressed.contains(image),
            })
            .collect(),
        frame_count: u32::try_from(moment.len()).unwrap_or(u32::MAX),
        burst_count: u32::try_from(moment.bursts.len()).unwrap_or(u32::MAX),
        camera_count: u32::try_from(moment.cameras.len()).unwrap_or(u32::MAX),
        start_ms: moment.start_ts,
        end_ms: moment.end_ts,
        duration_s: moment.duration_ms() / 1_000,
        diversity: moment.diversity,
        suggested_keepers: moment.suggested_keepers(),
        confidence: moment.confidence,
        reasons: moment.reasons.clone(),
        user_locked: moment.user_locked,
        duplicate_sets: u32::try_from(moment.duplicate_sets.len()).unwrap_or(u32::MAX),
    }
}

/// One duplicate set on the wire.
fn duplicate_dto(set: &DuplicateSet) -> DuplicateSetDto {
    DuplicateSetDto {
        kind: set.kind.as_str().to_string(),
        photo_ids: set.image_ids.iter().map(PhotoId::to_db).collect(),
        keep_hint: set.keep_hint.to_db(),
        confidence: set.confidence,
        reasons: set.reasons.clone(),
        user_chosen: set.user_chosen,
        caps_gallery: set.kind.caps_gallery_to_one(),
    }
}

/// True when the grouping's mean size is outside the plausible band.
///
/// The same test `Moments::group` applies, applied to the stored result rather than to
/// a run - so the Problems panel can still say so after a restart.
fn is_implausible(outline: &MomentOutline) -> bool {
    if outline.is_empty() {
        return false;
    }
    let (low, high) = aura_brain_wedding::moments::graph::PLAUSIBLE_MEAN_SIZE;
    let mean = outline.mean_size();
    mean < low || mean > high
}

/// Parse a project id, or refuse with a coded error rather than a panic.
fn parse_project(text: &str) -> IpcResult<aura_core::ProjectId> {
    aura_core::ProjectId::from_db(text).map_err(|_| {
        IpcError::from(aura_core::errors::db::statement_failed(
            "project id was not a valid AURA id",
            &std::io::Error::from(std::io::ErrorKind::InvalidInput),
        ))
    })
}

/// Parse a photograph id.
fn parse_photo(text: &str) -> IpcResult<PhotoId> {
    PhotoId::from_db(text).map_err(|_| {
        IpcError::from(aura_core::errors::db::statement_failed(
            "photo id was not a valid AURA id",
            &std::io::Error::from(std::io::ErrorKind::InvalidInput),
        ))
    })
}

/// Parse a moment id.
fn parse_moment(text: &str) -> IpcResult<MomentId> {
    MomentId::from_db(text).map_err(|_| {
        IpcError::from(aura_brain_wedding::errors::moment_edit_refused(
            "moment lookup",
            "moment id was not a valid AURA id",
        ))
    })
}
