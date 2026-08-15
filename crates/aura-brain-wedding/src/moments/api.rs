//! `Moments`: the one implementation of the frozen `MomentService`, and the pass.
//!
//! Everything a later phase can do with a moment, a burst or a duplicate class goes
//! through this type. Phase 05 wrote the rule for `SimilarityIndex`, phase 06 for
//! `PeopleService`, phase 07 for `StoryService`, and this is the fourth: **no phase may
//! keep its own grouping.** Two answers to "are these the same shot" is two culling
//! decisions that disagree, and phase 12's coverage guarantee is written against this
//! one.
//!
//! ## The order inside `group`, which is the design
//!
//! 1. **Read** every frame that has an embedding and a timeline time, in that order.
//! 2. **Subtract** the frames the photographer has locked into a moment. Not "preserve
//!    them afterwards": remove them from the problem, so the pass cannot produce a
//!    grouping that contradicts a decision already made.
//! 3. **Estimate** the cadence, per body.
//! 4. **Propose** candidate edges inside the adaptive window, and score them.
//! 5. **Union-find**, then split anything over the scene's size cap.
//! 6. **Merge** across cameras, to a fixed point.
//! 7. **Assemble** each group into a moment: bursts, duplicate sets, diversity,
//!    confidence, reasons.
//! 8. **Write**, in one transaction, preserving every locked moment.
//!
//! Step 2 before step 3 is the part that is easy to get wrong, and it is the same shape
//! of mistake as phase 06's "replay decisions before building the co-occurrence graph"
//! and phase 07's "smooth before you segment": a conclusion drawn over frames the
//! photographer has already assigned is a conclusion that will be thrown away, and by
//! the time it is a row in `moments` it has already cost the work.
//!
//! ## What this pass does not open
//!
//! A file. Grouping reads `embeddings`, `descriptors`, `photo`, `faces` and
//! `image_scenes`, and section 11's six-second budget for four thousand images is only
//! sane because of that. A phase that recomputes a difference hash is opening an image
//! that phase 05 already opened - which is phase 05's fifth inherited rule, and this is
//! the first phase to lean on it heavily.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use aura_core::clock::Clock;
use aura_core::contract::moment::{
    DuplicateSet, ImageId, Moment, MomentEdit, MomentOutline, MomentService, Timestamp,
};
use aura_core::progress::{CancelToken, ProgressSink, ProgressUpdate};
use aura_core::{AuraError, AuraResult, MomentId, PhotoId, ProjectId};

use crate::errors;
use crate::moments::cadence::Cadence;
use crate::moments::duplicate::FaceOverlap;
use crate::moments::graph::{self, Frame, MomentProfileRegistry};
use crate::moments::merge::{self, MomentSpan};
use crate::moments::moment::{self, Assembly, MomentRow, MomentStore};

/// How many times the cross-camera merge pass runs before it stops.
///
/// Each pass merges any moment at most once, so a three-body wedding needs two passes
/// to fold all three together. Four is one more than any wedding this product has been
/// designed for and is a termination guarantee rather than a tuning parameter.
pub const MAX_MERGE_PASSES: usize = 4;

/// What one grouping pass did.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GroupReport {
    /// Frames the pass considered.
    pub frames: usize,
    /// Frames excluded because the photographer had locked them into a moment.
    pub locked_frames: usize,
    /// Moments written.
    pub moments: usize,
    /// Moments the photographer had locked, which were preserved unchanged.
    pub locked_moments: usize,
    /// Bursts across every moment.
    pub bursts: usize,
    /// Candidate edges scored.
    pub candidates: usize,
    /// Cross-camera merges the evidence supported.
    pub cross_camera_merges: usize,
    /// Duplicate sets, by class: identical, near identical, variant.
    pub duplicates: [usize; 3],
    /// Mean frames per moment.
    pub mean_size: f32,
    /// Median inter-frame interval across the wedding, in milliseconds.
    pub median_interval_ms: i64,
    /// Fraction of the project's groupable frames that are in a moment, `0..1`.
    pub coverage: f32,
    /// Frames that could not be grouped at all. `AURA-ML-5032`.
    pub ungrouped: usize,
    /// Set when the result is outside the plausible band. `AURA-ML-5030`.
    pub implausible: bool,
    /// Wall clock.
    pub elapsed_ms: u64,
}

/// The grouping service.
#[derive(Debug)]
pub struct Moments {
    store: Arc<MomentStore>,
    clock: Arc<dyn Clock>,
    profiles: Arc<MomentProfileRegistry>,
}

impl Moments {
    /// Assemble the service over a store.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5031` when the shipped threshold table would not load. Propagated
    /// rather than swallowed: a build whose grouping thresholds are broken must not
    /// open a project, for the reason `Story::new` gives about scene profiles.
    pub fn new(store: Arc<MomentStore>, clock: Arc<dyn Clock>) -> Result<Self, AuraError> {
        Ok(Self {
            store,
            clock,
            profiles: Arc::new(MomentProfileRegistry::embedded()?),
        })
    }

    /// Use a different threshold table. The per-installation override path, and how
    /// MLR tunes section 6.2 without a rebuild.
    #[must_use]
    pub fn with_profiles(mut self, profiles: Arc<MomentProfileRegistry>) -> Self {
        self.profiles = profiles;
        self
    }

    /// The store underneath.
    #[must_use]
    pub fn store(&self) -> &Arc<MomentStore> {
        &self.store
    }

    /// The threshold table in force.
    #[must_use]
    pub fn profiles(&self) -> &Arc<MomentProfileRegistry> {
        &self.profiles
    }

    /// Report any version drift, once per distinct stale tuple.
    ///
    /// Nothing is compared across versions; this is only how the photographer finds out
    /// that a regrouping is coming.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the versions cannot be read.
    pub fn check_versions(&self, project: &ProjectId, embed_ver: u16) -> AuraResult<usize> {
        let current = (embed_ver, graph::GROUP_VER, self.profiles.version());
        let mut stale = 0usize;
        for (stored_embed, stored_group, stored_profile, count) in self.store.versions(project)? {
            let stored = (stored_embed, stored_group, stored_profile);
            if stored == current {
                continue;
            }
            stale += count;
            let err = errors::moment_version_mismatch(stored, current, count);
            tracing::warn!(
                target: "moments.version",
                code = %err.code,
                stored_embed_ver = stored.0,
                stored_group_ver = stored.1,
                stored_profile_ver = stored.2,
                "{}",
                err.detail
            );
        }
        Ok(stale)
    }

    /// Group one project's photographs into moments.
    ///
    /// `subject_focus` and `face_overlap` come from `PeopleService` when a face pass
    /// has run, and are closures rather than data so that a wedding with no face pass
    /// costs nothing to grouping - the whole-project face read that would otherwise be
    /// needed simply never happens.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` on a catalog failure, or `AURA-ML-5011` on cancellation.
    #[allow(clippy::too_many_lines)]
    pub fn group(
        &self,
        project: &ProjectId,
        context: &PassContext,
        progress: &dyn ProgressSink,
        cancel: &CancelToken,
    ) -> AuraResult<GroupReport> {
        let started = self.clock.monotonic_us();
        let mut report = GroupReport::default();

        let (mut frames, cameras) = self.store.load_frames(project)?;
        let locked = self.store.locked_frames(project)?;
        report.locked_frames = frames
            .iter()
            .filter(|frame| locked.contains(&frame.image))
            .count();
        frames.retain(|frame| !locked.contains(&frame.image));
        for frame in &mut frames {
            frame.subject_focus = (context.subject_focus)(frame.image);
        }
        report.frames = frames.len();
        let total = u64::try_from(report.frames).unwrap_or(u64::MAX);
        progress.report(ProgressUpdate {
            stage: "moments.load",
            done: total,
            total,
            current: None,
        });

        if frames.is_empty() {
            report.elapsed_ms = self.elapsed_ms(started);
            return Ok(report);
        }
        stop_if_cancelled(cancel, "loading frames")?;

        let shots: Vec<_> = frames.iter().map(Frame::shot).collect();
        let cadence = Cadence::estimate(&shots);
        report.median_interval_ms = cadence.median_ms();

        let edges = graph::candidates(&frames, &cadence, &self.profiles);
        report.candidates = edges.len();
        stop_if_cancelled(cancel, "scoring candidate pairs")?;

        let grouped = graph::group(&frames, &edges, &self.profiles);
        // Which groups the over-large pass had to break up. A group whose members are
        // not a contiguous run of one union-find component came out of `split_oversized`,
        // and its confidence says so.
        let reclustered = reclustered_groups(&grouped, &edges, frames.len());

        // Cross-camera merging, to a fixed point. Each pass merges any moment at most
        // once, so a three-body wedding needs two.
        let mut groups = grouped;
        let mut merge_evidence: BTreeMap<usize, (f32, f32)> = BTreeMap::new();
        for _ in 0..MAX_MERGE_PASSES {
            let spans: Vec<MomentSpan> = groups
                .iter()
                .map(|members| MomentSpan::of(&frames, members))
                .collect();
            let merges = merge::cross_camera(&frames, &spans);
            if merges.is_empty() {
                break;
            }
            report.cross_camera_merges += merges.len();
            for found in &merges {
                if let Some(group) = groups.get(found.left).and_then(|g| g.first()) {
                    merge_evidence.insert(*group, (found.overlap, found.medoid_distance));
                }
            }
            groups = merge::apply(&groups, &merges, &frames);
            stop_if_cancelled(cancel, "merging across cameras")?;
        }

        let by_index: BTreeMap<PhotoId, usize> = frames
            .iter()
            .enumerate()
            .map(|(index, frame)| (frame.image, index))
            .collect();

        let mut rows: Vec<MomentRow> = Vec::with_capacity(groups.len());
        for members in &groups {
            let anchor = members.first().copied().unwrap_or(0);
            let weakest = weakest_internal_edge(members, &edges);
            let all_drive = every_link_has_drive_evidence(members, &edges);
            let segment = (context.segment_of)(frames.get(anchor).map_or(0, |frame| frame.ts));

            let assembly = Assembly {
                profiles: &self.profiles,
                segment,
                embed_ver: context.embed_ver,
                weakest_edge: weakest,
                all_drive,
                reclustered: reclustered.contains(&anchor),
                cross_camera: merge_evidence.get(&anchor).copied(),
            };
            let faces = |left: usize, right: usize| match (frames.get(left), frames.get(right)) {
                (Some(a), Some(b)) => (context.face_overlap)(a.image, b.image),
                _ => FaceOverlap::default(),
            };
            let same_file = |left: usize, right: usize| match (frames.get(left), frames.get(right))
            {
                (Some(a), Some(b)) => (context.same_file)(a.image, b.image),
                _ => false,
            };
            rows.push(moment::assemble(
                &frames, members, &cadence, &cameras, assembly, &faces, &same_file,
            ));
            stop_if_cancelled(cancel, "assembling moments")?;
        }
        drop(by_index);

        report.moments = self.store.put_moments(project, &rows)?;
        report.bursts = rows.iter().map(|row| row.moment.bursts.len()).sum();
        for row in &rows {
            for set in &row.moment.duplicate_sets {
                let slot = match set.kind {
                    aura_core::contract::moment::DuplicateKind::Identical => 0,
                    aura_core::contract::moment::DuplicateKind::NearIdentical => 1,
                    aura_core::contract::moment::DuplicateKind::Variant => 2,
                };
                if let Some(counter) = report.duplicates.get_mut(slot) {
                    *counter += 1;
                }
            }
        }

        let (photos, groupable, grouped_frames, moments, locked_moments) =
            self.store.coverage(project)?;
        report.locked_moments = usize::try_from(locked_moments).unwrap_or(0);
        report.coverage = fraction(grouped_frames, groupable.max(0));
        report.ungrouped = usize::try_from((groupable - grouped_frames).max(0)).unwrap_or(0);
        report.mean_size = if moments > 0 {
            grouped_frames as f32 / moments as f32
        } else {
            0.0
        };
        let _ = photos;

        // `AURA-ML-5030`: a moment count nobody would recognise as this wedding. The
        // moments are written either way - a grouping nobody trusts is still better
        // than a grid of three thousand loose files, and every stack can be split by
        // hand - and this is how the Problems panel gets to say so.
        let (low, high) = graph::PLAUSIBLE_MEAN_SIZE;
        if report.moments > 0 && (report.mean_size < low || report.mean_size > high) {
            report.implausible = true;
            let err = errors::grouping_implausible(report.frames, report.moments, report.mean_size);
            tracing::warn!(target: "moments.built", code = %err.code, "{}", err.detail);
        }
        if report.ungrouped > 0 {
            let err = errors::moment_failed(
                &format!("{} frames", report.ungrouped),
                "no embedding or no timeline time",
            );
            tracing::info!(target: "moments.built", code = %err.code, "{}", err.detail);
        }

        report.elapsed_ms = self.elapsed_ms(started);
        tracing::info!(
            target: "moments.built",
            images = report.frames,
            moments = report.moments,
            bursts = report.bursts,
            mean_size = report.mean_size,
            ms = report.elapsed_ms,
            "grouped a wedding"
        );
        Ok(report)
    }

    /// Milliseconds since a monotonic start.
    fn elapsed_ms(&self, started: u64) -> u64 {
        self.clock.monotonic_us().saturating_sub(started) / 1_000
    }

    /// The project a moment belongs to, for the edit commands' scoping check.
    fn project_of(&self, moment: MomentId) -> AuraResult<Option<ProjectId>> {
        let key = moment.to_db();
        self.store.catalog().read(move |conn| {
            let found: Result<String, rusqlite::Error> = conn.query_row(
                "SELECT project_id FROM moments WHERE id = ?1",
                rusqlite::params![key],
                |row| row.get(0),
            );
            match found {
                Ok(text) => Ok(ProjectId::from_db(&text).ok()),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(err) => Err(aura_core::errors::db::statement_failed(
                    "could not read a moment's project",
                    &err,
                )),
            }
        })
    }
}

/// What the pass needs that the catalog cannot give it.
///
/// Every field is a closure rather than a map, so that evidence a wedding does not have
/// costs grouping nothing: a project with no face pass never pays for the whole-project
/// face read a data argument would require, and one that has been embedded but not
/// classified never reads `segments`.
///
/// Owned boxes rather than borrowed references, because the caller that assembles this
/// is a command handler that reads four tables and returns - and a borrowed context
/// would tie the pass's lifetime to four local variables in a function that has already
/// finished.
pub struct PassContext {
    /// Which phase 05 embedding produced the vectors.
    pub embed_ver: u16,
    /// Phase 06's prominence-weighted face sharpness for a frame, or zero.
    ///
    /// Zero, not a neutral 0.5, for `story::api::NO_SUBJECT_FOCUS`'s reason: a keep hint
    /// chosen as if every face were sharp is a hint chosen at random and presented as a
    /// considered choice. Zero sends `duplicate::keep_hint` to edge energy alone and
    /// makes it say so in the set's reasons.
    pub subject_focus: Box<dyn Fn(ImageId) -> f32 + Send + Sync>,
    /// Phase 06's face-box overlap between two frames, or an unmeasured default.
    pub face_overlap: Box<dyn Fn(ImageId, ImageId) -> FaceOverlap + Send + Sync>,
    /// Phase 01's answer to "are these two the same file", by content hash.
    pub same_file: Box<dyn Fn(ImageId, ImageId) -> bool + Send + Sync>,
    /// Phase 07's chapter at a moment in time, or `None` when the wedding has not been
    /// segmented.
    pub segment_of: Box<dyn Fn(Timestamp) -> Option<aura_core::SegmentId> + Send + Sync>,
}

impl std::fmt::Debug for PassContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PassContext")
            .field("embed_ver", &self.embed_ver)
            .finish_non_exhaustive()
    }
}

impl PassContext {
    /// The context a wedding with no face pass and no segmentation gets.
    ///
    /// Every closure returns the documented neutral, and the moments come out with
    /// reasons that say which evidence was missing rather than pretending it was there.
    #[must_use]
    pub fn bare(embed_ver: u16) -> Self {
        Self {
            embed_ver,
            subject_focus: Box::new(|_| 0.0),
            face_overlap: Box::new(|_, _| FaceOverlap {
                iou: 0.0,
                measured: false,
            }),
            same_file: Box::new(|_, _| false),
            segment_of: Box::new(|_| None),
        }
    }

    /// Replace the "are these the same file" answer.
    ///
    /// Phase 01 already knows: two `photo_file` rows with the same BLAKE3 digest are the
    /// same bytes. Section 6.3's first duplicate class is reported here for completeness
    /// rather than detected here, and this is how the answer arrives.
    #[must_use]
    pub fn with_same_file(
        mut self,
        f: impl Fn(ImageId, ImageId) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.same_file = Box::new(f);
        self
    }

    /// Replace the "which chapter is this" answer.
    #[must_use]
    pub fn with_segments(
        mut self,
        f: impl Fn(Timestamp) -> Option<aura_core::SegmentId> + Send + Sync + 'static,
    ) -> Self {
        self.segment_of = Box::new(f);
        self
    }

    /// Replace phase 06's two face answers together.
    ///
    /// Together rather than separately, because a build that has one and not the other
    /// would produce a keep hint chosen on face sharpness inside a set that was decided
    /// without looking at where the faces were - which is two different levels of
    /// evidence in one claim.
    #[must_use]
    pub fn with_faces(
        mut self,
        focus: impl Fn(ImageId) -> f32 + Send + Sync + 'static,
        overlap: impl Fn(ImageId, ImageId) -> FaceOverlap + Send + Sync + 'static,
    ) -> Self {
        self.subject_focus = Box::new(focus);
        self.face_overlap = Box::new(overlap);
        self
    }
}

impl MomentService for Moments {
    fn outline(&self, project: ProjectId) -> AuraResult<MomentOutline> {
        let moments = self.store.load_moments(&project)?;
        let (_, groupable, grouped, _, locked) = self.store.coverage(&project)?;
        let versions = self.store.versions(&project)?;
        // The lowest tuple, so a mixed project reports the oldest thing in it rather
        // than the newest - which is the honest direction, exactly as
        // `IndexStats::model_ver` reports the lowest of a mixed index.
        let (embed_ver, group_ver, profile_ver) = versions
            .first()
            .map_or((0, graph::GROUP_VER, self.profiles.version()), |row| {
                (row.0, row.1, row.2)
            });

        Ok(MomentOutline {
            moments,
            coverage: fraction(grouped, groupable),
            locked: u32::try_from(locked).unwrap_or(0),
            embed_ver,
            group_ver,
            profile_ver,
        })
    }

    fn moment_of(&self, image: ImageId) -> AuraResult<Option<Moment>> {
        self.store.moment_of(image)
    }

    fn duplicates(&self, moment: MomentId) -> AuraResult<Vec<DuplicateSet>> {
        self.store.duplicates(moment)
    }

    fn split(&self, moment: MomentId, at: ImageId) -> Result<MomentId, AuraError> {
        let Some(existing) = self.store.load_moment(moment)? else {
            return Err(errors::moment_edit_refused(
                "split",
                "that moment is not in this catalog",
            ));
        };
        let Some(project) = self.project_of(moment)? else {
            return Err(errors::moment_edit_refused(
                "split",
                "that moment belongs to no project",
            ));
        };
        let plan = merge::plan_split(&existing.image_ids, at)?;

        // The two halves' spans, taken from the moment that is being split rather than
        // re-read: the frames have not moved in time, only in which moment they are in.
        let times = self.frame_times(&existing)?;
        let keep_span = span_of(&plan.keep, &times);
        let moved_span = span_of(&plan.moved, &times);

        let created = self
            .store
            .apply_split(moment, &plan.moved, keep_span, moved_span)?;
        self.store.record_edit(
            &project,
            &MomentEdit {
                action: "split".to_string(),
                moment,
                other: Some(created),
                image: Some(at),
                moment_size: u32::try_from(existing.len()).unwrap_or(0),
            },
        )?;
        Ok(created)
    }

    fn merge(&self, a: MomentId, b: MomentId) -> Result<MomentId, AuraError> {
        if a == b {
            return Err(errors::moment_edit_refused(
                "merge",
                "a moment cannot be merged with itself",
            ));
        }
        let (Some(left), Some(right)) = (self.store.load_moment(a)?, self.store.load_moment(b)?)
        else {
            return Err(errors::moment_edit_refused(
                "merge",
                "one of those moments is not in this catalog",
            ));
        };
        if !self.store.same_home(a, b)? {
            return Err(errors::moment_edit_refused(
                "merge",
                "those two moments are in different weddings or different chapters",
            ));
        }
        let Some(project) = self.project_of(a)? else {
            return Err(errors::moment_edit_refused(
                "merge",
                "that moment belongs to no project",
            ));
        };

        let left_times = self.frame_times(&left)?;
        let right_times = self.frame_times(&right)?;
        let pairs_left: Vec<(ImageId, Timestamp)> = left
            .image_ids
            .iter()
            .map(|image| (*image, left_times.get(image).copied().unwrap_or(0)))
            .collect();
        let pairs_right: Vec<(ImageId, Timestamp)> = right
            .image_ids
            .iter()
            .map(|image| (*image, right_times.get(image).copied().unwrap_or(0)))
            .collect();
        merge::plan_merge(&pairs_left, &pairs_right)?;

        let span = (
            left.start_ts.min(right.start_ts),
            left.end_ts.max(right.end_ts),
        );
        self.store.apply_merge(a, b, span)?;
        self.store.record_edit(
            &project,
            &MomentEdit {
                action: "merge".to_string(),
                moment: a,
                other: Some(b),
                image: None,
                moment_size: u32::try_from(left.len() + right.len()).unwrap_or(0),
            },
        )?;
        Ok(a)
    }

    fn set_locked(&self, moment: MomentId, locked: bool) -> Result<(), AuraError> {
        let Some(existing) = self.store.load_moment(moment)? else {
            return Err(errors::moment_edit_refused(
                if locked { "lock" } else { "unlock" },
                "that moment is not in this catalog",
            ));
        };
        let Some(project) = self.project_of(moment)? else {
            return Err(errors::moment_edit_refused(
                "lock",
                "that moment belongs to no project",
            ));
        };
        self.store.set_locked(moment, locked)?;
        self.store.record_edit(
            &project,
            &MomentEdit {
                action: if locked { "lock" } else { "unlock" }.to_string(),
                moment,
                other: None,
                image: None,
                moment_size: u32::try_from(existing.len()).unwrap_or(0),
            },
        )?;
        Ok(())
    }

    fn set_keep_hint(&self, moment: MomentId, keep: ImageId) -> Result<(), AuraError> {
        let Some(existing) = self.store.load_moment(moment)? else {
            return Err(errors::moment_edit_refused(
                "keep_hint",
                "that moment is not in this catalog",
            ));
        };
        let Some(project) = self.project_of(moment)? else {
            return Err(errors::moment_edit_refused(
                "keep_hint",
                "that moment belongs to no project",
            ));
        };
        self.store.set_keep_hint(moment, keep)?;
        self.store.record_edit(
            &project,
            &MomentEdit {
                action: "keep_hint".to_string(),
                moment,
                other: None,
                image: Some(keep),
                moment_size: u32::try_from(existing.len()).unwrap_or(0),
            },
        )?;
        Ok(())
    }

    fn undo(&self, project: ProjectId) -> Result<MomentEdit, AuraError> {
        let Some((edit_id, edit)) = self.store.last_edit(&project)? else {
            return Err(errors::moment_edit_refused(
                "undo",
                "there is no grouping edit to undo in this wedding",
            ));
        };

        match edit.action.as_str() {
            // A split is undone by merging the two halves back and clearing the lock,
            // which returns exactly the moment that was there before - the frames never
            // moved in time, so the merged order is the original order.
            "split" => {
                if let Some(other) = edit.other {
                    let (Some(left), Some(right)) = (
                        self.store.load_moment(edit.moment)?,
                        self.store.load_moment(other)?,
                    ) else {
                        return Err(errors::moment_edit_refused(
                            "undo",
                            "one half of that split is no longer in the catalog",
                        ));
                    };
                    let span = (
                        left.start_ts.min(right.start_ts),
                        left.end_ts.max(right.end_ts),
                    );
                    self.store.apply_merge(edit.moment, other, span)?;
                }
                self.store.set_locked(edit.moment, false)?;
            }
            // Three actions, one reversal, and they share an arm rather than repeating
            // it three times because the reversal really is the same operation:
            //
            // * **merge** cannot be undone by splitting. The absorbed moment's identity
            //   is gone and re-deriving the boundary would be a guess dressed as a
            //   restoration. What can honestly be offered is releasing the lock, after
            //   which the next grouping pass reconsiders those frames from scratch.
            //   `docs/runbooks/AURA-ML-5029.md` says so in as many words.
            // * **lock** was a decision to pin, so undoing it is releasing.
            // * **keep_hint** returns to the automatic choice on the next pass, and
            //   releasing the lock is what lets that pass reach the moment at all.
            "merge" | "lock" | "keep_hint" => self.store.set_locked(edit.moment, false)?,
            "unlock" => self.store.set_locked(edit.moment, true)?,
            other => {
                return Err(errors::moment_edit_refused(
                    "undo",
                    &format!("`{other}` is not an action this build knows how to undo"),
                ))
            }
        }

        self.store.mark_undone(&edit_id)?;
        Ok(edit)
    }
}

impl Moments {
    /// Timeline times for one moment's frames.
    fn frame_times(&self, moment: &Moment) -> AuraResult<BTreeMap<ImageId, Timestamp>> {
        let ids: Vec<String> = moment.image_ids.iter().map(PhotoId::to_db).collect();
        self.store.catalog().read(move |conn| {
            let mut out = BTreeMap::new();
            let mut statement = conn
                .prepare("SELECT timeline_time FROM photo WHERE photo_id = ?1")
                .map_err(|e| {
                    aura_core::errors::db::statement_failed("could not read a frame time", &e)
                })?;
            for id in &ids {
                let Ok(photo) = PhotoId::from_db(id) else {
                    continue;
                };
                let text: Option<String> = statement
                    .query_row(rusqlite::params![id], |row| row.get(0))
                    .ok()
                    .flatten();
                if let Some(ms) = text
                    .as_deref()
                    .and_then(aura_index::store::parse_rfc3339_ms)
                {
                    out.insert(photo, ms);
                }
            }
            Ok(out)
        })
    }
}

/// The `(start, end)` of a list of frames.
fn span_of(images: &[ImageId], times: &BTreeMap<ImageId, Timestamp>) -> (Timestamp, Timestamp) {
    let found: Vec<Timestamp> = images
        .iter()
        .filter_map(|image| times.get(image).copied())
        .collect();
    (
        found.iter().copied().min().unwrap_or(0),
        found.iter().copied().max().unwrap_or(0),
    )
}

/// Stop the pass if the photographer cancelled it.
///
/// Checked between stages rather than inside them: the stages are each a bounded pass
/// over the frame list, and a wedding's whole grouping is seconds. Invariant 5 is
/// satisfied by the write being one transaction - a cancelled pass leaves the previous
/// grouping intact rather than half of a new one.
fn stop_if_cancelled(cancel: &CancelToken, during: &str) -> AuraResult<()> {
    if cancel.is_cancelled() {
        return Err(aura_core::errors::ml::cancelled(format!(
            "grouping was stopped while {during}"
        )));
    }
    Ok(())
}

/// A count as a fraction, guarding the zero case.
///
/// Per-mille integer arithmetic then one lossless widening, the same shape the index,
/// people and story statuses use.
fn fraction(part: i64, whole: i64) -> f32 {
    if whole <= 0 {
        return 0.0;
    }
    let per_mille = (part.saturating_mul(1_000) / whole).clamp(0, 1_000);
    f32::from(u16::try_from(per_mille).unwrap_or(1_000)) / 1_000.0
}

/// The weakest edge score inside a group, or `None` when it has no recorded edge.
fn weakest_internal_edge(members: &[usize], edges: &[graph::Edge]) -> Option<f32> {
    let inside: BTreeSet<usize> = members.iter().copied().collect();
    edges
        .iter()
        .filter(|edge| inside.contains(&edge.left) && inside.contains(&edge.right))
        .map(|edge| edge.score)
        .fold(None, |best: Option<f32>, score| {
            Some(best.map_or(score, |current| current.min(score)))
        })
}

/// True when every internal edge carried camera drive evidence.
fn every_link_has_drive_evidence(members: &[usize], edges: &[graph::Edge]) -> bool {
    if members.len() < 2 {
        return false;
    }
    let inside: BTreeSet<usize> = members.iter().copied().collect();
    let mut any = false;
    for edge in edges
        .iter()
        .filter(|edge| inside.contains(&edge.left) && inside.contains(&edge.right))
    {
        any = true;
        if !edge.drive_evidence {
            return false;
        }
    }
    any
}

/// The anchors of groups the over-large pass had to break up.
///
/// Detected rather than tracked: a group produced by `split_oversized` is one whose
/// union-find component in the *unrestricted* graph is larger than itself. Recomputing
/// that is cheap and keeps `graph::group` returning a plain partition, which is what
/// the eval harness and the fixtures compare against.
fn reclustered_groups(
    groups: &[Vec<usize>],
    edges: &[graph::Edge],
    frames: usize,
) -> BTreeSet<usize> {
    let mut sets = graph::UnionFind::new(frames);
    for edge in edges {
        sets.union(edge.left, edge.right);
    }
    let components = sets.groups();
    let mut size_of: BTreeMap<usize, usize> = BTreeMap::new();
    for component in &components {
        for member in component {
            size_of.insert(*member, component.len());
        }
    }
    groups
        .iter()
        .filter_map(|group| {
            let anchor = group.first().copied()?;
            let whole = size_of.get(&anchor).copied().unwrap_or(group.len());
            (whole > group.len()).then_some(anchor)
        })
        .collect()
}
