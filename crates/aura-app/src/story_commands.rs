//! The story half of the command surface.
//!
//! Nine commands, and the same rules the other halves follow plus one this half adds.
//!
//! **Nothing here blocks for long, except the two commands that say they do.**
//! `story_status`, `story_outline`, `image_scene`, `scene_profiles` and the four edit
//! commands read the catalog. `classify_scenes` is a batch pass and `segment_story` is
//! a whole-project pass; the UI calls both from a job rather than from a click handler,
//! and the first is resumable so a cancel costs nothing.
//!
//! **The timeline tells the truth about what is missing.** An empty chapter strip on an
//! unclassified project is a support ticket, so `StoryStatusDto` carries coverage, the
//! penalty the segmentation settled on, whether it fell back to time gaps, and both
//! version numbers - rather than making the UI infer any of it from silence.
//!
//! **The photographer outranks the automation, and the lock is set on both sides of an
//! edit.** ADR-0016 decision 2. `move_chapter_boundary` locks the chapter before the
//! boundary *and* the one after it, because a boundary is shared and locking one side
//! would let the next re-analysis move it back from the other.
//!
//! **Nothing here can change a photograph.** Four commands change a chapter, one
//! changes a label, four are reads. Invariant 1 stays structural.

use std::sync::Arc;

use aura_brain_wedding::scene::taxonomy::Taxonomy;
use aura_brain_wedding::story::changepoint;
use aura_core::contract::scene::{
    ChapterId, SceneId, SceneProfile, Segment, StoryOutline, StoryService,
};
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{PhotoId, SegmentId};

use crate::commands::IpcResult;
use crate::contract::ipc::{
    ChapterDto, ChapterHandleDto, ClassifyScenesInput, IpcError, MergeChaptersInput,
    MoveBoundaryInput, SceneDto, SceneProfileDto, SceneScoreDto, SetChapterInput,
    SplitChapterInput, StoryOutlineDto, StoryStatusDto,
};
use crate::state::AppState;

/// What the timeline opens with.
///
/// Section 11 budgets 200 ms. This is one indexed read of `segments` plus one of
/// `v_scene_coverage`, and a wedding has at most twenty chapters by construction.
///
/// # Errors
///
/// `AURA-DB-3006` when the segments cannot be read, `AURA-ML-5024` when the shipped
/// scene profiles or ritual taxonomies will not load.
pub fn story_outline(state: &AppState, project_id: &str) -> IpcResult<StoryOutlineDto> {
    let project = parse_project(project_id)?;
    let story = state.story()?;
    let outline = story.outline(project)?;
    Ok(outline_dto(&outline))
}

/// What the Story panel's header shows.
///
/// # Errors
///
/// `AURA-DB-3006`, or `AURA-ML-5024` when the config will not load.
pub fn story_status(state: &AppState, project_id: &str) -> IpcResult<StoryStatusDto> {
    let project = parse_project(project_id)?;
    let segments_store = state.story_store();
    let story = state.story()?;
    let (photos, classified) = segments_store.coverage(&project)?;
    let segments = segments_store.load_segments(&project)?;
    let taxonomy = story.taxonomy();

    Ok(StoryStatusDto {
        photos,
        classified,
        coverage: coverage_fraction(classified, photos),
        chapters: u32::try_from(segments.len()).unwrap_or(u32::MAX),
        needs_review: u32::try_from(
            segments
                .iter()
                .filter(|segment| segment.needs_review())
                .count(),
        )
        .unwrap_or(u32::MAX),
        locked: u32::try_from(
            segments
                .iter()
                .filter(|segment| segment.user_locked)
                .count(),
        )
        .unwrap_or(u32::MAX),
        // Read back from the stored chapters rather than recomputed. A penalty is a
        // property of the pass that produced these boundaries, and recomputing it would
        // report what a pass run *now* would choose, which is a different number about a
        // different timeline.
        penalty: state.story_penalty(&project)?,
        gaps_only: state.story_penalty(&project)? <= 0.0 && !segments.is_empty(),
        scene_ver: aura_brain_wedding::MODEL_VER,
        taxonomy_ver: taxonomy.version(),
        rituals_known: u32::try_from(taxonomy.len()).unwrap_or(u32::MAX),
    })
}

/// What one photograph is of, for the Explain panel.
///
/// Acceptance criterion 2: "every image carries a scene label, attributes and
/// confidence, visible in the Explain panel".
///
/// # Errors
///
/// `AURA-DB-3006` when the row cannot be read.
pub fn image_scene(state: &AppState, photo_id: &str) -> IpcResult<Option<SceneDto>> {
    let photo = parse_photo(photo_id)?;
    let story = state.story()?;
    let Some(result) = story.scene(photo)? else {
        return Ok(None);
    };

    // The ritual slug is stored, not the id, so it is read back through a second
    // lookup rather than resolved from `SceneResult::ritual` - which is deliberately
    // `None` on a stored row. See `StoryStore::load_scene`.
    let ritual = state.story_ritual_slug(photo)?;

    Ok(Some(SceneDto {
        photo_id: photo.to_db(),
        scene: result.scene.as_str().to_string(),
        scene_title: scene_title(result.scene),
        scene_conf: result.scene_conf,
        top3: result
            .top3
            .iter()
            .map(|score| SceneScoreDto {
                scene: score.scene.as_str().to_string(),
                score: score.score,
            })
            .collect(),
        attributes: result
            .attributes
            .names()
            .into_iter()
            .map(ToString::to_string)
            .collect(),
        attributes_measured: result.attributes_measured,
        ritual,
        ritual_conf: result.ritual_conf,
        source: result.source.as_str().to_string(),
        model_ver: result.model_ver,
    }))
}

/// Every scene's tolerances, with the sentence explaining them.
///
/// ADR-0016 decision 4: the rationale is on the wire because "why is my dance floor
/// being judged this way" is a question photographers ask and invariant 2 says has to
/// have an answer.
///
/// # Errors
///
/// `AURA-ML-5024` when the shipped profiles will not load.
pub fn scene_profiles(state: &AppState, project_id: &str) -> IpcResult<Vec<SceneProfileDto>> {
    let project = parse_project(project_id)?;
    let story = state.story()?;
    let registry = story.profiles();

    // Per-project overrides win over the shipped defaults, exactly as the classification
    // pass sees them. Reading the shipped table here and the stored one there would be
    // two answers to the same question.
    let stored = state.story_store().load_profiles(&project)?;

    let mut out: Vec<SceneProfileDto> = registry
        .scenes()
        .into_iter()
        .map(|scene| {
            let profile = stored
                .iter()
                .find(|(row, _)| row.scene == scene)
                .map_or_else(|| registry.get(scene), |(row, _)| row.clone());
            let rationale = stored
                .iter()
                .find(|(row, _)| row.scene == scene)
                .map_or_else(
                    || registry.rationale(scene).unwrap_or_default().to_string(),
                    |(_, text)| text.clone(),
                );
            profile_dto(&profile, &rationale)
        })
        .collect();
    out.sort_by(|left, right| left.scene.cmp(&right.scene));
    Ok(out)
}

/// Classify every photograph that has an embedding and no current scene row.
///
/// Resumable: a killed pass restarts and does the rest. Invariant 5.
///
/// # Errors
///
/// `AURA-DB-3006` on a catalog failure, `AURA-ML-5011` on cancellation, or whatever
/// the runtime raised.
pub fn classify_scenes(state: &AppState, input: &ClassifyScenesInput) -> IpcResult<StoryStatusDto> {
    let project = parse_project(&input.project_id)?;
    let story = state.story()?;
    let embeddings = state.embedding_store();
    let classifier = state.scene_classifier()?;
    // The ritual head is optional: a wedding with chapters and no rite names is a
    // degradation, not a failure, and a build whose ritual model is missing must still
    // classify scenes. Invariant 6.
    let rituals = state.ritual_classifier().ok();

    let context = state.story_pass_context(&project)?;
    aura_brain_wedding::classify_project(
        &story,
        &embeddings,
        &classifier,
        rituals.as_ref(),
        &project,
        &context,
        &NullProgress,
        &CancelToken::new(),
    )?;
    story_status(state, &input.project_id)
}

/// Build the chapters from the labelled frames.
///
/// # Errors
///
/// `AURA-DB-3006` on a catalog failure.
pub fn segment_story(state: &AppState, project_id: &str) -> IpcResult<StoryOutlineDto> {
    let project = parse_project(project_id)?;
    let story = state.story()?;
    let embeddings = state.story_embeddings(&project)?;
    story.segment_with_embeddings(&project, &embeddings)?;
    let outline = story.outline(project)?;
    Ok(outline_dto(&outline))
}

/// Rename a chapter, or change which chapter it is. Locks it.
///
/// # Errors
///
/// `AURA-ML-5025` when the chapter is unknown.
pub fn set_chapter(state: &AppState, input: &SetChapterInput) -> IpcResult<ChapterHandleDto> {
    let segment = parse_segment(&input.segment_id)?;
    let chapter = ChapterId::from_str_or_other(&input.chapter);
    let story = state.story()?;
    story.set_chapter(segment, chapter, input.label.as_deref())?;
    Ok(ChapterHandleDto {
        id: segment.to_db(),
    })
}

/// Move the boundary between a chapter and the one after it. Locks both.
///
/// # Errors
///
/// `AURA-ML-5025` when the chapter is unknown, is the last one, or when the move would
/// empty either side.
pub fn move_chapter_boundary(
    state: &AppState,
    input: &MoveBoundaryInput,
) -> IpcResult<ChapterHandleDto> {
    let segment = parse_segment(&input.segment_id)?;
    let story = state.story()?;
    story.move_boundary(segment, input.new_end_ms)?;
    Ok(ChapterHandleDto {
        id: segment.to_db(),
    })
}

/// Split a chapter in two at a photograph. Returns the new later half.
///
/// # Errors
///
/// `AURA-ML-5025` when the chapter is unknown, the photograph is not in it, or the
/// split would leave a side empty.
pub fn split_chapter(state: &AppState, input: &SplitChapterInput) -> IpcResult<ChapterHandleDto> {
    let segment = parse_segment(&input.segment_id)?;
    let photo = parse_photo(&input.photo_id)?;
    let story = state.story()?;
    let created = story.split_segment(segment, photo)?;
    Ok(ChapterHandleDto {
        id: created.to_db(),
    })
}

/// Fold two adjacent chapters into one. Returns the survivor.
///
/// # Errors
///
/// `AURA-ML-5025` when either is unknown, they belong to different weddings, or they
/// are not adjacent.
pub fn merge_chapters(state: &AppState, input: &MergeChaptersInput) -> IpcResult<ChapterHandleDto> {
    let a = parse_segment(&input.segment_id_a)?;
    let b = parse_segment(&input.segment_id_b)?;
    let story = state.story()?;
    let survivor = story.merge_segments(a, b)?;
    Ok(ChapterHandleDto {
        id: survivor.to_db(),
    })
}

// -- shaping -----------------------------------------------------------------

/// One chapter on the wire.
fn chapter_dto(segment: &Segment, ordinal: usize) -> ChapterDto {
    ChapterDto {
        segment_id: segment.id.to_db(),
        ordinal: u32::try_from(ordinal).unwrap_or(u32::MAX),
        chapter: segment.chapter.as_str().to_string(),
        label: None,
        title: segment.chapter.title().to_string(),
        start_ms: segment.start_ts,
        end_ms: segment.end_ts,
        duration_minutes: segment.duration_ms() / 60_000,
        dominant_scene: segment.dominant_scene.as_str().to_string(),
        confidence: segment.confidence,
        key_frame: segment.key_frame.to_db(),
        image_count: segment.image_count,
        reasons: segment.reasons.clone(),
        user_locked: segment.user_locked,
        needs_review: segment.needs_review(),
    }
}

/// A whole outline on the wire.
fn outline_dto(outline: &StoryOutline) -> StoryOutlineDto {
    StoryOutlineDto {
        chapters: outline
            .segments
            .iter()
            .enumerate()
            .map(|(ordinal, segment)| chapter_dto(segment, ordinal))
            .collect(),
        coverage: outline.coverage,
        needs_review: outline.needs_review.iter().map(SegmentId::to_db).collect(),
        scene_ver: outline.scene_ver,
        taxonomy_ver: outline.taxonomy_ver,
    }
}

/// One profile on the wire.
fn profile_dto(profile: &SceneProfile, rationale: &str) -> SceneProfileDto {
    SceneProfileDto {
        scene: profile.scene.as_str().to_string(),
        title: scene_title(profile.scene),
        keeper_min: profile.keeper_rate.0,
        keeper_max: profile.keeper_rate.1,
        max_acceptable_noise: profile.max_acceptable_noise,
        max_acceptable_blur: profile.max_acceptable_blur,
        subject_focus_weight: profile.subject_focus_weight,
        emotion_weight: profile.emotion_weight,
        composition_weight: profile.composition_weight,
        editing_intent: profile.editing_intent.as_str().to_string(),
        must_cover: profile.must_cover,
        rationale: rationale.trim().to_string(),
    }
}

/// A photographer-facing name for a scene.
///
/// Derived from the slug rather than authored, because twenty-two hand-written titles
/// would be twenty-two things to keep in step with the enum, and the slugs are already
/// readable. `getting_ready_bride` becomes "Getting Ready Bride".
fn scene_title(scene: SceneId) -> String {
    scene
        .as_str()
        .split('_')
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().collect::<String>() + chars.as_str()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A count as a fraction, guarding the zero-photograph case.
///
/// Per-mille integer arithmetic then one lossless widening, the same shape the index
/// and people statuses use: a direct `i64` to float cast is a precision loss the
/// compiler is right to object to even when the numbers are small.
fn coverage_fraction(part: i64, whole: i64) -> f32 {
    if whole <= 0 {
        return 0.0;
    }
    let per_mille = (part.saturating_mul(1_000) / whole).clamp(0, 1_000);
    f32::from(u16::try_from(per_mille).unwrap_or(1_000)) / 1_000.0
}

/// The chapter band this build segments into, for the panel's empty state.
#[must_use]
pub const fn chapter_band() -> (usize, usize) {
    changepoint::CHAPTER_BAND
}

/// Every rite this build knows, for the panel's ritual filter.
///
/// # Errors
///
/// `AURA-ML-5024` when the shipped taxonomy will not load.
pub fn known_rituals() -> IpcResult<Vec<String>> {
    let taxonomy = Taxonomy::embedded()?;
    Ok(taxonomy
        .all()
        .into_iter()
        .map(|rite| rite.slug.clone())
        .collect())
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

/// Parse a chapter id.
fn parse_segment(text: &str) -> IpcResult<SegmentId> {
    SegmentId::from_db(text).map_err(|_| {
        IpcError::from(aura_brain_wedding::errors::edit_refused(
            "chapter lookup",
            "chapter id was not a valid AURA id",
        ))
    })
}

/// The story service, for callers that hold their own.
#[must_use]
pub fn story_of(state: &AppState) -> Option<Arc<aura_brain_wedding::Story>> {
    state.story().ok()
}
