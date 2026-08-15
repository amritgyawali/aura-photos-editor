//! The emotion half of the command surface.
//!
//! Seven commands, and the shape of the seven is the argument. Five are reads. The two
//! that change anything are both the photographer telling the product it is wrong -
//! [`prefer_frame`] records a pairwise comparison, [`set_moment_peak`] overrides a peak
//! choice - and neither of them decides a photograph's fate.
//!
//! **There is no command here that keeps, delivers or builds a gallery.** Section 2.2
//! puts every one of those in phase 12 and phase 29, and this is the surface where
//! crossing that line would be easiest: an ordering by `emotion_score` *looks* exactly
//! like a shortlist. [`ranked_by_emotion`] comes closest and deliberately stops short -
//! it returns the ordering this phase's headline promises, and it says nothing about
//! which frames a client sees.
//!
//! **Nothing here blocks for long, except the one command that says it does.**
//! `emotion_status`, `image_emotion`, `moment_peak`, `reactions_of` and
//! `ranked_by_emotion` read the catalog. `score_emotion` is a whole-project pass that
//! decodes a proxy per frame and runs two heads; the UI calls it from a job rather than
//! from a click handler, and it is cancellable.
//!
//! **The panel is told which reasons are caveats and which faces read as crying.**
//! `EmotionReasonDto::caveat` and `FaceExpressionDto::reads_as_crying` are computed here
//! and sent, rather than derived in the interface from a slug list and a threshold. The
//! tear gate in particular is `aura_core::contract::emotion::TEARS_CERTAIN`, and section
//! 12's fourth failure mode is what happens when a second copy of it drifts.

use aura_core::contract::emotion::{
    EmotionReason, EmotionService, FaceExpression, ImageEmotion, ImageId, Interaction, MomentPeak,
    Preference, ReactionLink,
};
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{MomentId, PhotoId, Priority, ProjectId};

use crate::commands::IpcResult;
use crate::contract::ipc::{
    CropRectDto, EmotionDto, EmotionPassDto, EmotionReasonDto, EmotionStatusDto, FaceExpressionDto,
    InteractionDto, IpcError, MomentPeakDto, PreferInput, RankedByEmotionDto, RankedInput,
    ReactionLinkDto, ScoreEmotionInput, SetPeakInput,
};
use crate::state::AppState;

/// What the Emotion panel's header shows.
///
/// # Errors
///
/// `AURA-DB-3006` when the readings cannot be read.
pub fn emotion_status(state: &AppState, project_id: &str) -> IpcResult<EmotionStatusDto> {
    let project = parse_project(project_id)?;
    let outline = state.emotion()?.outline(project)?;
    Ok(EmotionStatusDto {
        photos: outline.photos,
        scored: outline.scored,
        coverage: outline.coverage,
        face_aware: outline.face_aware,
        moments: outline.moments,
        peaked: outline.peaked,
        peak_rate: outline.peak_rate(),
        links: outline.links,
        interaction_counts: outline.interaction_histogram.to_vec(),
        interaction_names: Interaction::ALL
            .iter()
            .map(|kind| kind.as_str().to_string())
            .collect(),
        mean_score: outline.mean_score,
        mean_margin: outline.mean_margin,
        preferences: outline.preferences,
        model_ver: outline.model_ver,
        analysis_ver: outline.analysis_ver,
        weights_ver: outline.weights_ver,
    })
}

/// One photograph's reading, for the Emotion card.
///
/// `None` means **nobody has looked**, and the card says so rather than drawing a flat
/// reading. Migration 10's sixth property, carried out to the interface.
///
/// # Errors
///
/// `AURA-DB-3006` when the reading cannot be read.
pub fn image_emotion(state: &AppState, photo_id: &str) -> IpcResult<Option<EmotionDto>> {
    let image = parse_photo(photo_id)?;
    Ok(state.emotion()?.of_image(image)?.as_ref().map(dto))
}

/// One moment's peak, for the browser's indicator.
///
/// # Errors
///
/// `AURA-DB-3006` when the peak cannot be read.
pub fn moment_peak(state: &AppState, moment_id: &str) -> IpcResult<Option<MomentPeakDto>> {
    let moment = MomentId::from_db(moment_id).map_err(|_| bad_id("moment"))?;
    Ok(state.emotion()?.peak_of(moment)?.as_ref().map(peak_dto))
}

/// Every frame that reacts to this one, for the pair viewer.
///
/// # Errors
///
/// `AURA-DB-3006` when the links cannot be read.
pub fn reactions_of(state: &AppState, photo_id: &str) -> IpcResult<Vec<ReactionLinkDto>> {
    let image = parse_photo(photo_id)?;
    Ok(state
        .emotion()?
        .reactions_to(image)?
        .iter()
        .map(link_dto)
        .collect())
}

/// A project's frames ordered by emotion score, strongest first.
///
/// **An ordering, not a selection.** See this module's header.
///
/// # Errors
///
/// `AURA-DB-3006` when the readings cannot be read.
pub fn ranked_by_emotion(
    state: &AppState,
    input: &RankedInput,
) -> IpcResult<Vec<RankedByEmotionDto>> {
    let project = parse_project(&input.project_id)?;
    let limit = input.limit.unwrap_or(200).clamp(1, 5_000) as usize;
    Ok(state
        .emotion()?
        .ranked(project, limit)?
        .into_iter()
        .map(|(photo, score)| RankedByEmotionDto {
            photo_id: photo.to_db(),
            emotion_score: score,
        })
        .collect())
}

/// Record that the photographer would deliver one of two frames.
///
/// **Collected, not applied.** This build ships the ranker coefficients that
/// `ml/models/emotion/train_ranker.py` produced; phase 30's learning loop is where a
/// photographer's own comparisons start moving them. A ranker that refitted itself while
/// somebody was culling would reorder the grid under their hands.
///
/// # Errors
///
/// `AURA-ML-5041` when either photograph is unknown, when they are the same photograph,
/// or when they belong to different weddings.
pub fn prefer_frame(state: &AppState, input: &PreferInput) -> IpcResult<()> {
    let winner = parse_photo(&input.winner_id)?;
    let loser = parse_photo(&input.loser_id)?;
    state.emotion()?.prefer(Preference { winner, loser })?;
    Ok(())
}

/// Record that the photographer disagrees with a moment's peak frame.
///
/// Unbeatable: `moment_peak.user_chosen` is checked inside the statement a re-analysis
/// would overwrite it with. It changes which frame the browser leads with and it changes
/// nothing about what is delivered.
///
/// # Errors
///
/// `AURA-ML-5041` when the moment is unknown or the photograph is not in it.
pub fn set_moment_peak(state: &AppState, input: &SetPeakInput) -> IpcResult<MomentPeakDto> {
    let moment = MomentId::from_db(&input.moment_id).map_err(|_| bad_id("moment"))?;
    let image = parse_photo(&input.photo_id)?;
    let emotion = state.emotion()?;
    emotion.set_peak(moment, image)?;
    emotion
        .peak_of(moment)?
        .as_ref()
        .map(peak_dto)
        .ok_or_else(|| {
            IpcError::from(aura_core::errors::db::statement_failed(
                "the peak disappeared between two reads",
                &std::io::Error::from(std::io::ErrorKind::NotFound),
            ))
        })
}

/// Score every photograph in a project that has no current reading.
///
/// The one command here that is a pass rather than a read. Resumable: the work remaining
/// is a query, so a killed run continues where it stopped, and a `weights_ver` bump makes
/// every affected row pending without anybody triggering anything.
///
/// # Errors
///
/// `AURA-ML-5039` when the weight table will not load - the one thing in phase 10 that
/// halts - or `AURA-DB-3006` when the pending set cannot be read. Per-photograph failures
/// are counted in the report rather than returned.
pub fn score_emotion(state: &AppState, input: &ScoreEmotionInput) -> IpcResult<EmotionPassDto> {
    let project = parse_project(&input.project_id)?;
    let pass = state.emotion_pass(&input.project_id)?;
    // The token is registered under the caller's id so `cancel_job` can reach it. A pass
    // that decodes four thousand proxies and runs two heads over each is the one command
    // on this surface a photographer will want to stop.
    let cancel = CancelToken::new();
    if let Some(id) = input.cancel_id.as_deref() {
        state.register_job(id, cancel.clone());
    }
    let report = pass.run(&project, Priority::AiBatch, &cancel, &NullProgress);
    if let Some(id) = input.cancel_id.as_deref() {
        state.finish_job(id);
    }
    let report = report?;
    Ok(EmotionPassDto {
        scored: u32::try_from(report.scored).unwrap_or(u32::MAX),
        failed: u32::try_from(report.failed).unwrap_or(u32::MAX),
        faces: u32::try_from(report.faces).unwrap_or(u32::MAX),
        moments: u32::try_from(report.moments).unwrap_or(u32::MAX),
        peaked: u32::try_from(report.peaked).unwrap_or(u32::MAX),
        links: u32::try_from(report.links).unwrap_or(u32::MAX),
        mean_score: report.mean_score,
        elapsed_ms: report.elapsed_ms,
        cancelled: report.cancelled,
    })
}

// -- shapes ------------------------------------------------------------------

fn dto(result: &ImageEmotion) -> EmotionDto {
    EmotionDto {
        photo_id: result.image_id.to_db(),
        faces: result.faces.iter().map(face_dto).collect(),
        channel_names: FaceExpression::CHANNEL_NAMES
            .iter()
            .map(|name| (*name).to_string())
            .collect(),
        interactions: result
            .interactions
            .iter()
            .map(|(kind, strength)| InteractionDto {
                kind: kind.as_str().to_string(),
                title: kind.title().to_string(),
                strength: *strength,
                milestone: kind.is_milestone(),
            })
            .collect(),
        mutual_gaze: result.mutual_gaze,
        peak_proximity: result.peak_proximity,
        reaction_of: result.reaction_of.map(|id| id.to_db()),
        emotion_score: result.emotion_score,
        narrative_weight: result.narrative_weight,
        scene: result.scene.as_str().to_string(),
        reasons: result.reasons.iter().map(reason_dto).collect(),
        confidence: result.confidence,
        source: result.source.as_str().to_string(),
        model_ver: result.model_ver,
        analysis_ver: result.analysis_ver,
        weights_ver: result.weights_ver,
    }
}

fn face_dto(face: &FaceExpression) -> FaceExpressionDto {
    FaceExpressionDto {
        face_id: face.face_id.to_db(),
        identity_id: face.identity.map(|id| id.to_db()),
        channels: face.channels().to_vec(),
        gaze: face.gaze.as_str().to_string(),
        confidence: face.confidence,
        reads_as_crying: face.reads_as_crying(),
        posed_smile: face.posed_smile(),
        crop: crop_dto(face.crop),
    }
}

fn reason_dto(reason: &EmotionReason) -> EmotionReasonDto {
    EmotionReasonDto {
        code: reason.code.as_str().to_string(),
        text: reason.text.clone(),
        weight: reason.weight,
        caveat: reason.code.is_caveat(),
        evidence: reason.evidence.map(crop_dto),
    }
}

fn peak_dto(peak: &MomentPeak) -> MomentPeakDto {
    MomentPeakDto {
        moment_id: peak.moment_id.to_db(),
        photo_id: peak.image_id.to_db(),
        index: peak.index,
        frames: peak.frames,
        margin: peak.margin,
        kind: peak.kind.as_str().to_string(),
        resolved: peak.is_resolved(),
        confidence: peak.confidence,
        reasons: peak.reasons.iter().map(reason_dto).collect(),
        user_chosen: peak.user_chosen,
    }
}

fn link_dto(link: &ReactionLink) -> ReactionLinkDto {
    ReactionLinkDto {
        action: link.action.to_db(),
        reaction: link.reaction.to_db(),
        gap_ms: link.gap_ms,
        bonus: link.bonus,
        confidence: link.confidence,
        reasons: link.reasons.iter().map(reason_dto).collect(),
    }
}

fn crop_dto(crop: aura_core::contract::integrity::CropRect) -> CropRectDto {
    CropRectDto {
        x: crop.x,
        y: crop.y,
        w: crop.w,
        h: crop.h,
    }
}

fn parse_project(text: &str) -> IpcResult<ProjectId> {
    ProjectId::from_db(text).map_err(|_| bad_id("project"))
}

fn parse_photo(text: &str) -> IpcResult<ImageId> {
    PhotoId::from_db(text).map_err(|_| bad_id("photo"))
}

/// The refusal every id parse here shares.
///
/// The same shape the other command modules use, gathered into one function because this
/// surface parses three kinds of id and three copies of a five-line closure is three
/// places for the message to drift.
fn bad_id(kind: &str) -> IpcError {
    IpcError::from(aura_core::errors::db::statement_failed(
        format!("{kind} id was not a valid AURA id"),
        &std::io::Error::from(std::io::ErrorKind::InvalidInput),
    ))
}
