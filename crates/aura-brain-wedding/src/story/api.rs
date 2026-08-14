//! `Story`: the one implementation of the frozen `StoryService`.
//!
//! Everything a later phase can do with a scene or a chapter goes through this type,
//! which is what makes invariant 7 enforceable in one place instead of trusted in ten.
//! Phase 05 wrote the rule for `SimilarityIndex`, phase 06 for `PeopleService`, and it
//! is the same rule a third time: **no phase may keep its own scene classifier or its
//! own idea of where the ceremony was.** A second answer to "is this the ceremony" is
//! two thresholds that disagree.
//!
//! ## The two passes, and why they are separate
//!
//! [`Story::classify`] labels frames. [`Story::segment`] builds chapters. They are two
//! entry points rather than one, because they have different costs, different
//! resumability and different triggers:
//!
//! * classification is per frame, resumable at any point, and re-runs only what is
//!   missing at the current version - section 11 budgets 35 s for four thousand frames;
//! * segmentation is per project, takes every labelled frame at once, and re-runs from
//!   scratch every time - section 11 budgets 2 s.
//!
//! Merging them would mean a killed classification pass left a wedding with no chapters
//! at all, rather than with chapters over the frames it had reached.
//!
//! ## The order inside `segment`, which is the design
//!
//! 1. **Read** every labelled frame in timeline order.
//! 2. **Smooth** the posteriors with the HMM. Before segmentation, not after; see
//!    `story::hmm` and ADR-0015 section 6.
//! 3. **Detect** change points over the fused signal, with the penalty searched.
//! 4. **Merge** runs that are too short or too thin.
//! 5. **Choose** a key frame per run.
//! 6. **Write**, preserving every chapter the photographer has locked.
//!
//! Step 2 before step 3 is the part that is easy to get wrong, and it is the same shape
//! of mistake as phase 06's "replay decisions before building the co-occurrence graph":
//! a conclusion drawn from unsmoothed labels is a conclusion about noise, and by the
//! time it is a row in `segments` no later step can undo it.

use std::collections::BTreeMap;
use std::sync::Arc;

use aura_core::clock::Clock;
use aura_core::contract::scene::{
    ChapterId, ImageId, SceneId, SceneProfile, SceneResult, SceneScore, Segment, StoryOutline,
    StoryService, Timestamp, NEEDS_REVIEW_BELOW,
};
use aura_core::{AuraError, AuraResult, PhotoId, ProjectId, SegmentId};

use crate::errors;
use crate::scene::classifier::{self, MODEL_VER, PREPROCESS_VER};
use crate::scene::profile::SceneProfileRegistry;
use crate::scene::taxonomy::Taxonomy;
use crate::story::changepoint::{self, Frame, Run};
use crate::story::hmm::{self, Observation, Smoothed};
use crate::story::keyframe::{self, Candidate};
use crate::story::segment::{SceneRow, StoryStore};

/// The subject-focus score used when phase 06's face pass has not run.
///
/// Zero, not a neutral 0.5. A key-frame filter that treated an unscanned wedding as
/// "everything is in focus" would pick a frame at random and present it as a considered
/// choice; zero sends the filter to its documented [`keyframe::Relaxation::Any`] step,
/// which says so.
pub const NO_SUBJECT_FOCUS: f32 = 0.0;

/// What one segmentation pass did.
#[derive(Debug, Clone, PartialEq)]
pub struct StoryReport {
    /// Chapters written.
    pub chapters: usize,
    /// Chapters the photographer had locked, which were preserved unchanged.
    pub locked: usize,
    /// Frames placed in a chapter.
    pub frames: usize,
    /// Frames whose scene the HMM moved to a different chapter.
    pub smoothed: usize,
    /// Boundaries forced by a time gap above twenty minutes.
    pub hard_boundaries: usize,
    /// Runs the merge pass absorbed.
    pub merged: usize,
    /// The PELT penalty the search settled on.
    pub penalty: f32,
    /// True when the search never landed in the band and gaps alone were used.
    pub fell_back: bool,
    /// Chapters below the review threshold.
    pub needs_review: usize,
    /// Fraction of the project carrying a scene label, `0..1`.
    pub coverage: f32,
    /// Wall clock.
    pub elapsed_ms: u64,
}

/// What one classification pass did.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ScenePassReport {
    /// Frames classified in this pass.
    pub classified: usize,
    /// Frames the head refused to label, which keep their top-3.
    pub abstained: usize,
    /// Frames that could not be classified at all. `AURA-ML-5027`.
    pub failed: usize,
    /// Rituals named above the floor and the margin.
    pub rituals: usize,
    /// Attribute bits a measurement overrode.
    pub corrected_attributes: usize,
    /// Mean top-1 posterior over the frames that were labelled.
    pub mean_confidence: f32,
    /// Fraction of the project carrying a scene label after this pass.
    pub coverage: f32,
    /// Wall clock.
    pub elapsed_ms: u64,
}

/// The scenes-and-story service.
#[derive(Debug)]
pub struct Story {
    store: Arc<StoryStore>,
    clock: Arc<dyn Clock>,
    profiles: Arc<SceneProfileRegistry>,
    taxonomy: Arc<Taxonomy>,
}

impl Story {
    /// Assemble the service over a store.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5024` when the shipped config would not load. Propagated rather than
    /// swallowed: this is the constructor the product uses, and a build whose threshold
    /// table is broken must not open a project.
    pub fn new(store: Arc<StoryStore>, clock: Arc<dyn Clock>) -> Result<Self, AuraError> {
        Ok(Self {
            store,
            clock,
            profiles: Arc::new(SceneProfileRegistry::embedded()?),
            taxonomy: Arc::new(Taxonomy::embedded()?),
        })
    }

    /// Use a different profile registry. The per-project override path, and how QA
    /// tunes section 6.3 without a rebuild.
    #[must_use]
    pub fn with_profiles(mut self, profiles: Arc<SceneProfileRegistry>) -> Self {
        self.profiles = profiles;
        self
    }

    /// Use a different ritual taxonomy.
    #[must_use]
    pub fn with_taxonomy(mut self, taxonomy: Arc<Taxonomy>) -> Self {
        self.taxonomy = taxonomy;
        self
    }

    /// The store underneath.
    #[must_use]
    pub fn store(&self) -> &Arc<StoryStore> {
        &self.store
    }

    /// The profile registry in force.
    #[must_use]
    pub fn profiles(&self) -> &Arc<SceneProfileRegistry> {
        &self.profiles
    }

    /// The taxonomy in force.
    #[must_use]
    pub fn taxonomy(&self) -> &Arc<Taxonomy> {
        &self.taxonomy
    }

    /// Copy the shipped profiles into a project, so an override has something to
    /// override.
    ///
    /// Idempotent, and `user_edited = 1` rows are never touched - the rule is in the
    /// statement, in `StoryStore::put_profiles`.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the write fails.
    pub fn install_profiles(&self, project: &ProjectId) -> AuraResult<usize> {
        let rows: Vec<(SceneProfile, String)> = self
            .profiles
            .scenes()
            .into_iter()
            .map(|scene| {
                (
                    self.profiles.get(scene),
                    self.profiles
                        .rationale(scene)
                        .unwrap_or("shipped default")
                        .to_string(),
                )
            })
            .collect();
        self.store
            .put_profiles(project, &rows, self.profiles.version())
    }

    /// Report any version drift, once per distinct stale tuple.
    ///
    /// Section 12's fourth failure mode. Nothing is compared across versions: the reader
    /// filters to the current `model_ver` before it computes anything, and this is only
    /// how the photographer finds out that a re-classification is coming.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the versions cannot be read.
    pub fn check_versions(&self, project: &ProjectId) -> AuraResult<usize> {
        let current = (
            MODEL_VER,
            PREPROCESS_VER,
            self.taxonomy.version(),
            TRUNK_VER,
        );
        let mut stale = 0usize;
        for (stored, rows) in self.store.scene_versions(project)? {
            if stored == current {
                continue;
            }
            stale += rows;
            let mismatch = errors::scene_version_mismatch(stored, current, rows);
            tracing::warn!(
                target: "scene.version",
                code = %mismatch.code,
                "{}", mismatch.detail
            );
        }
        Ok(stale)
    }

    /// Fraction of a project carrying a scene label.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the view cannot be read.
    pub fn coverage(&self, project: &ProjectId) -> AuraResult<f32> {
        let (photos, classified) = self.store.coverage(project)?;
        Ok(ratio(classified, photos))
    }

    /// Build the wedding's chapters from its labelled frames.
    ///
    /// See this module's header for why the six steps are in this order.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` on a catalog failure.
    pub fn segment(&self, project: &ProjectId) -> AuraResult<StoryReport> {
        let started = self.clock.monotonic_ms();
        let coverage = self.coverage(project)?;

        // 1. Read, in timeline order.
        let rows = self.store.load_scenes(project, MODEL_VER)?;
        if rows.is_empty() {
            return Ok(StoryReport {
                chapters: 0,
                locked: 0,
                frames: 0,
                smoothed: 0,
                hard_boundaries: 0,
                merged: 0,
                penalty: 0.0,
                fell_back: false,
                needs_review: 0,
                coverage,
                elapsed_ms: self.clock.monotonic_ms().saturating_sub(started),
            });
        }

        // 2. Smooth.
        let day_start = rows.first().map_or(0, |row| row.ts);
        let observations: Vec<Observation> = rows
            .iter()
            .map(|row| Observation {
                top3: row.top3,
                offset_ms: row.ts.saturating_sub(day_start),
            })
            .collect();
        let (smoothed, smooth_report) = hmm::smooth(&observations);

        // 3 and 4. Detect and merge.
        //
        // The embeddings are deliberately not loaded here. The change signal's first
        // term needs them, and holding four thousand 512-d vectors is 8 MB - affordable
        // - but the caller already has them in `aura_index` and passing them in would
        // add a parameter to a method whose other three callers do not have them. So
        // `segment_with_embeddings` is the full path and this is the convenience one,
        // which runs on the chapter-disagreement and time-gap terms alone. Both are
        // documented, both are tested, and `SegmentReport` says which ran.
        let frames = Self::frames_from(&rows, &smoothed, &BTreeMap::new());
        let (runs, detect) = changepoint::segment(&frames);

        self.write_runs(
            project,
            &rows,
            &frames,
            &runs,
            &detect,
            coverage,
            started,
            smooth_report.corrected,
        )
    }

    /// Segment with the phase 05 embeddings, which is the full signal.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` on a catalog failure.
    pub fn segment_with_embeddings(
        &self,
        project: &ProjectId,
        embeddings: &BTreeMap<PhotoId, Vec<f32>>,
    ) -> AuraResult<StoryReport> {
        let started = self.clock.monotonic_ms();
        let coverage = self.coverage(project)?;
        let rows = self.store.load_scenes(project, MODEL_VER)?;
        if rows.is_empty() {
            return self.segment(project);
        }

        let day_start = rows.first().map_or(0, |row| row.ts);
        let observations: Vec<Observation> = rows
            .iter()
            .map(|row| Observation {
                top3: row.top3,
                offset_ms: row.ts.saturating_sub(day_start),
            })
            .collect();
        let (smoothed, smooth_report) = hmm::smooth(&observations);
        let frames = Self::frames_from(&rows, &smoothed, embeddings);
        let (runs, detect) = changepoint::segment(&frames);

        self.write_runs(
            project,
            &rows,
            &frames,
            &runs,
            &detect,
            coverage,
            started,
            smooth_report.corrected,
        )
    }

    /// Assemble the segmenter's view of the timeline.
    fn frames_from(
        rows: &[SceneRow],
        smoothed: &[Smoothed],
        embeddings: &BTreeMap<PhotoId, Vec<f32>>,
    ) -> Vec<Frame> {
        rows.iter()
            .zip(smoothed.iter())
            .map(|(row, decided)| Frame {
                ts: row.ts,
                embedding: embeddings.get(&row.photo_id).cloned().unwrap_or_default(),
                smoothed: *decided,
            })
            .collect()
    }

    /// Steps 5 and 6: key frames, then the write.
    #[allow(clippy::too_many_arguments)]
    fn write_runs(
        &self,
        project: &ProjectId,
        rows: &[SceneRow],
        frames: &[Frame],
        runs: &[Run],
        detect: &changepoint::SegmentReport,
        coverage: f32,
        started: u64,
        smoothed: usize,
    ) -> AuraResult<StoryReport> {
        let mut segments = Vec::with_capacity(runs.len());
        let mut members: BTreeMap<SegmentId, Vec<PhotoId>> = BTreeMap::new();
        let mut needs_review = 0usize;
        let mut placed = 0usize;

        for run in runs {
            let (chapter, support) = changepoint::dominant(frames, *run);
            let slice: Vec<&SceneRow> = rows.iter().skip(run.start).take(run.len()).collect();
            let Some(first) = slice.first() else {
                continue;
            };
            let last = slice.last().copied().unwrap_or(first);

            // 5. Key frame. Subject focus and primary presence are phase 06's to
            // answer and this crate does not link `aura-people`, so the fallback path
            // is what runs until a caller supplies them - see `NO_SUBJECT_FOCUS`.
            let candidates: Vec<Candidate> = slice
                .iter()
                .map(|row| Candidate {
                    image_id: row.photo_id,
                    embedding: frames
                        .iter()
                        .find(|frame| frame.ts == row.ts)
                        .map(|frame| frame.embedding.clone())
                        .unwrap_or_default(),
                    subject_focus: NO_SUBJECT_FOCUS,
                    has_primary: false,
                })
                .collect();
            let key = keyframe::choose(&candidates);

            let dominant_scene = dominant_scene(&slice);
            let confidence = chapter_confidence(&slice, support, detect.fell_back);
            let reasons = chapter_reasons(
                &slice,
                chapter,
                dominant_scene,
                support,
                detect,
                key.as_ref(),
            );

            let id = SegmentId::new();
            members.insert(id, slice.iter().map(|row| row.photo_id).collect());
            placed += slice.len();

            let segment = Segment {
                id,
                chapter,
                start_ts: first.ts,
                end_ts: last.ts,
                dominant_scene,
                confidence,
                key_frame: key.as_ref().map_or(first.photo_id, |found| found.image_id),
                image_count: u32::try_from(slice.len()).unwrap_or(u32::MAX),
                reasons,
                user_locked: false,
            };
            if segment.needs_review() {
                needs_review += 1;
            }
            segments.push(segment);
        }

        if detect.fell_back {
            let refusal = errors::segmentation_implausible(
                detect.at_low,
                detect.at_high,
                changepoint::CHAPTER_BAND,
            );
            tracing::warn!(
                target: "story.segmented",
                code = %refusal.code,
                "{}", refusal.detail
            );
        }

        // 6. Write.
        let (written, locked) =
            self.store
                .put_segments(project, &segments, &members, detect.penalty, MODEL_VER)?;

        // Section 11's `story.segmented`.
        tracing::info!(
            target: "story.segmented",
            segments = written,
            boundary_penalty = detect.penalty,
            chapters = segments.len(),
            hard_boundaries = detect.hard_boundaries,
            merged = detect.merged,
            ms = self.clock.monotonic_ms().saturating_sub(started),
            "the wedding was read as a story"
        );

        Ok(StoryReport {
            chapters: segments.len(),
            locked,
            frames: placed,
            smoothed,
            hard_boundaries: detect.hard_boundaries,
            merged: detect.merged,
            penalty: detect.penalty,
            fell_back: detect.fell_back,
            needs_review,
            coverage,
            elapsed_ms: self.clock.monotonic_ms().saturating_sub(started),
        })
    }

    /// The chapter that would hold a moment, for a boundary edit.
    fn segments_of(&self, project: &ProjectId) -> AuraResult<Vec<Segment>> {
        self.store.load_segments(project)
    }
}

/// A run's most common scene, which is not always its chapter's namesake.
///
/// A `Ceremony` chapter whose frames are mostly `vows` reports `vows`, and that is the
/// useful answer: the chapter says where in the story this is and the dominant scene
/// says what the photographs are of.
fn dominant_scene(rows: &[&SceneRow]) -> SceneId {
    let mut counts: BTreeMap<SceneId, u32> = BTreeMap::new();
    for row in rows {
        if row.scene == SceneId::Unknown {
            continue;
        }
        *counts.entry(row.scene).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .max_by(|left, right| {
            left.1
                .cmp(&right.1)
                // Ties to the earlier scene in wedding order, so two machines agree.
                .then_with(|| right.0.as_index().cmp(&left.0.as_index()))
        })
        .map_or(SceneId::Unknown, |(scene, _)| scene)
}

/// How sure a chapter's label is.
///
/// The geometric mean of two things that must both hold: the share of frames on the
/// dominant chapter, and the mean posterior support for it. A geometric mean rather
/// than an arithmetic one, for the reason phase 06's quality gate gives - it refuses to
/// let one strong term hide a weak one. A chapter where 95 % of frames agree at 0.20
/// support is not a confident chapter, and an arithmetic mean would call it 0.58.
///
/// The gap-only fallback caps it below the review threshold, because a chapter whose
/// boundary came from a clock rather than from evidence should be looked at.
fn chapter_confidence(rows: &[&SceneRow], support: f32, fell_back: bool) -> f32 {
    if rows.is_empty() {
        return 0.0;
    }
    let labelled = rows
        .iter()
        .filter(|row| row.scene != SceneId::Unknown)
        .count();
    let share = labelled as f32 / rows.len() as f32;
    let combined = (share.clamp(0.0, 1.0) * support.clamp(0.0, 1.0)).sqrt();
    if fell_back {
        combined.min(NEEDS_REVIEW_BELOW - 0.01)
    } else {
        combined.clamp(0.0, 0.98)
    }
}

/// Why this chapter is this chapter. Invariant 2, at most `Segment::MAX_REASONS`.
fn chapter_reasons(
    rows: &[&SceneRow],
    chapter: ChapterId,
    dominant: SceneId,
    support: f32,
    detect: &changepoint::SegmentReport,
    key: Option<&keyframe::KeyFrame>,
) -> Vec<String> {
    let mut out = Vec::with_capacity(Segment::MAX_REASONS);
    let labelled = rows
        .iter()
        .filter(|row| row.scene != SceneId::Unknown)
        .count();

    out.push(format!(
        "{} of {} frames carry a scene label, most often {}",
        labelled,
        rows.len(),
        dominant.as_str()
    ));
    out.push(format!(
        "the smoothed timeline puts these frames in {} at {:.2} mean support",
        chapter.title(),
        support
    ));
    if detect.fell_back {
        out.push("chapter boundaries came from time gaps only".to_string());
    } else {
        out.push(format!(
            "boundaries were detected at penalty {:.2}, which put this wedding inside the {}-{} \
             chapter band",
            detect.penalty,
            changepoint::CHAPTER_BAND.0,
            changepoint::CHAPTER_BAND.1
        ));
    }
    if let Some(found) = key {
        out.push(format!(
            "the cover frame is the medoid of {} eligible frames ({})",
            found.eligible,
            found.relaxed_to.as_str()
        ));
    }
    if let Some(ritual) = rows.iter().find_map(|row| row.ritual.as_ref()) {
        out.push(format!("a named rite appears in this chapter: {ritual}"));
    }
    out.truncate(Segment::MAX_REASONS);
    out
}

/// The phase 05 trunk version this build's scene head reads vectors from.
///
/// The fourth of migration 7's version columns. It is a **constant of this build**
/// rather than a reading of the catalog, and that is the point: it records which trunk
/// the scene head was trained against, so a project whose embeddings have moved on
/// reports `AURA-ML-5022` instead of quietly feeding the head a vector from a model it
/// has never seen.
///
/// `aura-vision` owns the authoritative `MODEL_VER` and this crate deliberately does
/// not link it - the scene head takes a vector, not pixels, and pulling in the decode
/// path to read one integer would make a lightweight crate heavy. The two are kept in
/// step by `crates/aura-brain-wedding/tests/versions.rs`, which fails the build if they
/// diverge.
pub const TRUNK_VER: u16 = 100;

/// A count as a fraction, guarding the zero-photograph case.
fn ratio(part: i64, whole: i64) -> f32 {
    if whole <= 0 {
        return 0.0;
    }
    let per_mille = (part.saturating_mul(1_000) / whole).clamp(0, 1_000);
    f32::from(u16::try_from(per_mille).unwrap_or(1_000)) / 1_000.0
}

impl StoryService for Story {
    fn outline(&self, project: ProjectId) -> AuraResult<StoryOutline> {
        let segments = self.store.load_segments(&project)?;
        let needs_review = segments
            .iter()
            .filter(|segment| segment.needs_review())
            .map(|segment| segment.id)
            .collect();
        let (photos, classified) = self.store.coverage(&project)?;
        Ok(StoryOutline {
            segments,
            coverage: ratio(classified, photos),
            needs_review,
            scene_ver: MODEL_VER,
            taxonomy_ver: self.taxonomy.version(),
        })
    }

    fn scene(&self, image: ImageId) -> AuraResult<Option<SceneResult>> {
        self.store.load_scene(image)
    }

    fn segment_of(&self, image: ImageId) -> AuraResult<Option<Segment>> {
        self.store.segment_of(image)
    }

    fn profile(&self, scene: SceneId) -> SceneProfile {
        self.profiles.get(scene)
    }

    fn set_chapter(
        &self,
        segment: SegmentId,
        chapter: ChapterId,
        label: Option<&str>,
    ) -> Result<(), AuraError> {
        let changed = self.store.set_chapter(segment, chapter, label)?;
        if changed == 0 {
            return Err(errors::edit_refused(
                "rename",
                "that chapter does not exist",
            ));
        }
        tracing::info!(
            target: "story.user_edit",
            action = "rename",
            segment = %segment,
            to_label = chapter.as_str(),
            "a chapter was renamed by the photographer"
        );
        Ok(())
    }

    fn move_boundary(&self, segment: SegmentId, new_end: Timestamp) -> Result<(), AuraError> {
        let project = self.owner_of(segment)?;
        let segments = self.segments_of(&project)?;
        let position = segments
            .iter()
            .position(|found| found.id == segment)
            .ok_or_else(|| errors::edit_refused("boundary", "that chapter does not exist"))?;
        let this = segments
            .get(position)
            .ok_or_else(|| errors::edit_refused("boundary", "that chapter does not exist"))?;
        let next = segments.get(position + 1).ok_or_else(|| {
            errors::edit_refused(
                "boundary",
                "that is the last chapter, so there is no boundary after it",
            )
        })?;

        if new_end <= this.start_ts {
            return Err(errors::edit_refused(
                "boundary",
                "the new boundary would leave the earlier chapter with no photographs",
            ));
        }
        if new_end >= next.end_ts {
            return Err(errors::edit_refused(
                "boundary",
                "the new boundary would leave the later chapter with no photographs",
            ));
        }

        // Both sides are locked. A boundary is shared, so moving it is one decision
        // about two chapters, and locking only one of them would let the next
        // re-analysis move it back from the other side.
        let moved = self.reassign_across(&project, this, next, new_end)?;
        self.store.lock(this.id)?;
        self.store.lock(next.id)?;

        tracing::info!(
            target: "story.user_edit",
            action = "boundary",
            segment = %segment,
            frames_moved = moved,
            "a chapter boundary was moved by the photographer"
        );
        Ok(())
    }

    fn split_segment(&self, segment: SegmentId, at: ImageId) -> Result<SegmentId, AuraError> {
        let project = self.owner_of(segment)?;
        let segments = self.segments_of(&project)?;
        let position = segments
            .iter()
            .position(|found| found.id == segment)
            .ok_or_else(|| errors::edit_refused("split", "that chapter does not exist"))?;
        let this = segments
            .get(position)
            .ok_or_else(|| errors::edit_refused("split", "that chapter does not exist"))?;

        let members = self.store.load_scenes(&project, MODEL_VER)?;
        let inside: Vec<&SceneRow> = members
            .iter()
            .filter(|row| row.ts >= this.start_ts && row.ts <= this.end_ts)
            .collect();
        let cut = inside
            .iter()
            .position(|row| row.photo_id == at)
            .ok_or_else(|| {
                errors::edit_refused("split", "that photograph is not in this chapter")
            })?;
        if cut == 0 {
            return Err(errors::edit_refused(
                "split",
                "splitting at the first photograph would leave the earlier chapter empty",
            ));
        }
        let Some(pivot) = inside.get(cut) else {
            return Err(errors::edit_refused(
                "split",
                "that photograph is not in this chapter",
            ));
        };

        let later: Vec<&SceneRow> = inside.iter().skip(cut).copied().collect();
        let created = SegmentId::new();
        let new_segment = Segment {
            id: created,
            chapter: this.chapter,
            start_ts: pivot.ts,
            end_ts: this.end_ts,
            dominant_scene: dominant_scene(&later),
            confidence: this.confidence,
            key_frame: pivot.photo_id,
            image_count: u32::try_from(later.len()).unwrap_or(u32::MAX),
            reasons: vec![format!(
                "split from an earlier chapter by the photographer at {}",
                pivot.photo_id
            )],
            user_locked: true,
        };
        self.store
            .insert_segment(&project, &new_segment, position + 1, MODEL_VER)?;

        let moves: Vec<(PhotoId, SegmentId)> =
            later.iter().map(|row| (row.photo_id, created)).collect();
        let earlier_end = inside
            .get(cut.saturating_sub(1))
            .map_or(this.start_ts, |row| row.ts);
        self.store.reassign(
            &project,
            &moves,
            &[(
                this.id,
                this.start_ts,
                earlier_end,
                u32::try_from(cut).unwrap_or(0),
            )],
        )?;
        self.store.lock(this.id)?;

        tracing::info!(
            target: "story.user_edit",
            action = "split",
            segment = %segment,
            frames_moved = later.len(),
            "a chapter was split by the photographer"
        );
        Ok(created)
    }

    fn merge_segments(&self, a: SegmentId, b: SegmentId) -> Result<SegmentId, AuraError> {
        if a == b {
            return Err(errors::edit_refused(
                "merge",
                "a chapter cannot be merged with itself",
            ));
        }
        let project = self.owner_of(a)?;
        let other = self.owner_of(b)?;
        if project != other {
            // The same boundary phase 06 guards with `AURA-SEC-9004`, one level down:
            // refused rather than repaired.
            return Err(errors::edit_refused(
                "merge",
                "those two chapters belong to different weddings",
            ));
        }

        let segments = self.segments_of(&project)?;
        let left = segments
            .iter()
            .position(|found| found.id == a)
            .ok_or_else(|| errors::edit_refused("merge", "that chapter does not exist"))?;
        let right = segments
            .iter()
            .position(|found| found.id == b)
            .ok_or_else(|| errors::edit_refused("merge", "that chapter does not exist"))?;
        if left.abs_diff(right) != 1 {
            return Err(errors::edit_refused(
                "merge",
                "those chapters are not adjacent; merging them would silently absorb the ones \
                 between, and no dialog asked about those",
            ));
        }

        let (first, second) = if left < right { (a, b) } else { (b, a) };
        let (Some(head), Some(tail)) = (
            segments.iter().find(|found| found.id == first),
            segments.iter().find(|found| found.id == second),
        ) else {
            return Err(errors::edit_refused("merge", "that chapter does not exist"));
        };

        let moved = self.store.absorb(second, first)?;
        self.store.reassign(
            &project,
            &[],
            &[(
                first,
                head.start_ts,
                tail.end_ts,
                head.image_count.saturating_add(tail.image_count),
            )],
        )?;

        tracing::info!(
            target: "story.user_edit",
            action = "merge",
            segment = %first,
            frames_moved = moved,
            "two chapters were merged by the photographer"
        );
        Ok(first)
    }
}

impl Story {
    /// Which project owns a chapter, or a refusal.
    fn owner_of(&self, segment: SegmentId) -> Result<ProjectId, AuraError> {
        self.store
            .project_of(segment)?
            .ok_or_else(|| errors::edit_refused("chapter lookup", "that chapter does not exist"))
    }

    /// Move every frame after a new boundary from one chapter into the next.
    fn reassign_across(
        &self,
        project: &ProjectId,
        this: &Segment,
        next: &Segment,
        new_end: Timestamp,
    ) -> AuraResult<usize> {
        let rows = self.store.load_scenes(project, MODEL_VER)?;
        let span: Vec<&SceneRow> = rows
            .iter()
            .filter(|row| row.ts >= this.start_ts && row.ts <= next.end_ts)
            .collect();

        let mut moves = Vec::new();
        let mut before = 0u32;
        let mut after = 0u32;
        for row in &span {
            if row.ts <= new_end {
                before = before.saturating_add(1);
                moves.push((row.photo_id, this.id));
            } else {
                after = after.saturating_add(1);
                moves.push((row.photo_id, next.id));
            }
        }
        let count = moves.len();
        self.store.reassign(
            project,
            &moves,
            &[
                (this.id, this.start_ts, new_end, before),
                (next.id, new_end.saturating_add(1), next.end_ts, after),
            ],
        )?;
        Ok(count)
    }
}

/// The chapter that would hold one scene's frames when there is no story yet.
///
/// Used by the debug panel and by tests, and it is here rather than in `SceneId` so
/// that `aura-core` keeps no opinion about segmentation.
#[must_use]
pub fn provisional_chapter(scenes: &[SceneScore]) -> ChapterId {
    scenes
        .iter()
        .max_by(|left, right| left.score.total_cmp(&right.score))
        .map_or(ChapterId::Other, |best| best.scene.chapter())
}

/// The classification pass. Exposed on [`Story`] for symmetry with `segment`.
impl Story {
    /// Which photographs still need a scene.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn pending(&self, project: &ProjectId) -> AuraResult<Vec<PhotoId>> {
        self.store.pending(project, MODEL_VER)
    }

    /// Store one batch of classified frames.
    ///
    /// The runner in `crate::scene::pass` assembles the inputs, calls the classifier and
    /// hands the results here. Split that way so the storage rules - the `source <>
    /// 'user'` guard, the four version columns, the top-3 encoding - live in one place
    /// and the pass does not have to know about any of them.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the write fails.
    pub fn record(
        &self,
        project: &ProjectId,
        rows: &[crate::story::segment::ScenePassRow],
    ) -> AuraResult<usize> {
        self.store.put_scenes(project, rows)
    }

    /// The scene versions this build writes: model, preprocess, taxonomy, trunk.
    #[must_use]
    pub fn versions(&self) -> (u16, u16, u16, u16) {
        (
            MODEL_VER,
            PREPROCESS_VER,
            self.taxonomy.version(),
            TRUNK_VER,
        )
    }

    /// The abstention rules this build classifies with, for the debug panel.
    #[must_use]
    pub const fn thresholds() -> (f32, f32) {
        (classifier::MIN_CONFIDENCE, classifier::MIN_MARGIN)
    }
}
