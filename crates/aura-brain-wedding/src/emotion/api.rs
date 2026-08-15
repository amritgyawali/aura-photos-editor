//! `Emotion`: the one implementation of the frozen `EmotionService`, and the pass.
//!
//! Two halves, the shape phases 06 to 09 all settled on. [`Emotion`] answers questions
//! about readings that already exist and is what every later phase holds. [`EmotionPass`]
//! walks a project and produces them, and is what the job graph holds.
//!
//! ## The order inside `run_ids`, which is the design
//!
//! 1. **Read** every pending frame: two heads, one decode, per frame.
//! 2. **Store** each reading immediately, with a neutral peak proximity and no reaction.
//! 3. **Curve** every moment in the project, once the readings exist. A frame's place on
//!    its moment's curve depends on its siblings, and half a moment's siblings have not
//!    been read while step 1 is running.
//! 4. **Link** reactions across the whole project, which needs every frame's gaze.
//! 5. **Re-score** the frames the last two steps changed, from stored readings.
//!
//! Steps 3 to 5 are arithmetic over stored numbers and open no file. That is what makes a
//! `weights_ver` bump cheap - the most common re-run in this phase is an edit to
//! `emotion_weights.toml`, and it costs one pass over the catalog rather than two model
//! calls per frame.
//!
//! Storing in step 2 rather than at the end is invariant 5: kill the process at 90 % and
//! the readings for the first 90 % are on disk. What is lost is the peaks and links for
//! this run, and those are recomputed from stored readings in seconds.
//!
//! ## No silent failure, invariant 9
//!
//! A frame whose proxy will not decode is counted, coded `AURA-ML-5040` and reported. The
//! run continues, and **no row is written** - so the next pass tries again, which is right
//! for a transient decode failure and harmless for a permanent one. A written-but-empty
//! reading would read as "nothing happening in this photograph" to phase 12, which is a
//! deletion dressed as a measurement.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::contract::emotion::{
    EmotionOutline, EmotionService, ImageEmotion, ImageId, Interaction, MomentPeak, Preference,
    ReactionLink, Timestamp,
};
use aura_core::contract::people::PeopleService;
use aura_core::contract::scene::StoryService;
use aura_core::progress::{CancelToken, ProgressSink, ProgressUpdate};
use aura_core::{AuraError, AuraResult, IdentityId, MomentId, PhotoId, Priority, ProjectId, Role};
use aura_infer::contract::infer::InferService;
use aura_preview::contract::service::{PreviewService, Priority as PreviewPriority};

use crate::emotion::analyse::{Analyser, FrameContext, ANALYSIS_VER, EMOTION_LEVEL};
use crate::emotion::expression::MODEL_VER;
use crate::emotion::peak::{self, Frame};
use crate::emotion::reaction::{self, Candidate};
use crate::emotion::store::EmotionStore;
use crate::emotion::weights::EmotionWeights;
use crate::errors;

/// The telemetry stage name, matching section 11's `emotion.scored` event.
pub const STAGE: &str = "emotion.scored";

/// The peak telemetry stage, matching section 11's `emotion.peaks` event.
pub const PEAK_STAGE: &str = "emotion.peaks";

/// The reaction telemetry stage, matching section 11's `emotion.reactions` event.
pub const REACTION_STAGE: &str = "emotion.reactions";

/// What one pass over a project did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EmotionReport {
    /// Photographs scored.
    pub scored: usize,
    /// Photographs that could not be scored, each logged with a code.
    pub failed: usize,
    /// Faces read.
    pub faces: usize,
    /// Moments whose curve was built.
    pub moments: usize,
    /// Moments whose peak separated itself.
    pub peaked: usize,
    /// Reaction links written.
    pub links: usize,
    /// How many frames carry each interaction, in `Interaction::ALL` order.
    pub interaction_histogram: [u32; Interaction::COUNT],
    /// Mean emotion score over the frames this pass scored.
    pub mean_score: f32,
    /// Mean peak margin over resolved peaks.
    pub mean_margin: f32,
    /// Mean reaction bonus over the links written.
    pub mean_bonus: f32,
    /// Wall clock for the whole pass.
    pub elapsed_ms: u64,
    /// True when the pass stopped early because it was cancelled.
    pub cancelled: bool,
}

impl EmotionReport {
    /// Milliseconds per scored photograph, or zero when nothing was scored.
    #[must_use]
    pub fn ms_per_image(&self) -> f64 {
        if self.scored == 0 {
            return 0.0;
        }
        self.elapsed_ms as f64 / self.scored as f64
    }
}

/// The frozen service. What every later phase holds.
pub struct Emotion {
    store: Arc<EmotionStore>,
    weights: Arc<EmotionWeights>,
}

impl fmt::Debug for Emotion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Emotion")
            .field("weights_ver", &self.weights.version())
            .finish_non_exhaustive()
    }
}

impl Emotion {
    /// Wrap one catalog's emotion tables.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5039` when the shipped weight table would not load. Propagated rather
    /// than swallowed: a build whose emotion weights are broken must not open a project,
    /// for the reason `Moments::new` gives about grouping thresholds.
    pub fn new(store: Arc<EmotionStore>) -> Result<Self, AuraError> {
        Ok(Self {
            store,
            weights: Arc::new(EmotionWeights::embedded()?),
        })
    }

    /// Open one over a catalog directly.
    ///
    /// # Errors
    ///
    /// As [`Emotion::new`].
    pub fn over(catalog: Arc<Catalog>, clock: Arc<dyn Clock>) -> Result<Self, AuraError> {
        Self::new(Arc::new(EmotionStore::new(catalog, clock)))
    }

    /// Use a different weight table. The per-installation override path.
    #[must_use]
    pub fn with_weights(mut self, weights: Arc<EmotionWeights>) -> Self {
        self.weights = weights;
        self
    }

    /// The store underneath, for the pass and the gate.
    #[must_use]
    pub fn store(&self) -> &Arc<EmotionStore> {
        &self.store
    }

    /// The weight table in force.
    #[must_use]
    pub fn weights(&self) -> &Arc<EmotionWeights> {
        &self.weights
    }

    /// Which versions this build would produce.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5039` when the embedded weight table will not load.
    pub fn current_versions() -> Result<(u16, u16, u16), AuraError> {
        Ok((
            MODEL_VER,
            ANALYSIS_VER,
            EmotionWeights::embedded()?.version(),
        ))
    }

    /// Report any version drift, once per distinct stale tuple.
    ///
    /// Nothing is compared across versions; this is only how the photographer finds out
    /// that a re-score is coming.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the versions cannot be read.
    pub fn check_versions(&self, project: &ProjectId) -> AuraResult<usize> {
        let current = (MODEL_VER, ANALYSIS_VER, self.weights.version());
        let mut stale = 0usize;
        for (model, analysis, weights, count) in self.store.versions(project)? {
            if (model, analysis, weights) == current {
                continue;
            }
            stale += count;
            let err = errors::emotion_version_mismatch((model, analysis, weights), current, count);
            tracing::warn!(
                target: "emotion.version",
                code = %err.code,
                stored_model_ver = model,
                stored_analysis_ver = analysis,
                stored_weights_ver = weights,
                "{}",
                err.detail
            );
        }
        Ok(stale)
    }
}

impl EmotionService for Emotion {
    fn outline(&self, project: ProjectId) -> AuraResult<EmotionOutline> {
        let outline = self.store.outline(project)?;
        // The version check, reported rather than enforced. `AURA-ML-5038` is degraded:
        // stale scores keep working while the background pass replaces them, and the
        // outline reports the lowest version present so a caller about to draw a
        // conclusion over a mixed set finds out before it draws it.
        if outline.scored > 0 {
            let current = (MODEL_VER, ANALYSIS_VER, self.weights.version());
            if (outline.model_ver, outline.analysis_ver, outline.weights_ver) != current {
                let stale = errors::emotion_version_mismatch(
                    (outline.model_ver, outline.analysis_ver, outline.weights_ver),
                    current,
                    outline.scored as usize,
                );
                tracing::warn!(
                    target: "emotion.version",
                    code = %stale.code,
                    "{}", stale.detail
                );
            }
        }
        Ok(outline)
    }

    fn of_image(&self, image: ImageId) -> AuraResult<Option<ImageEmotion>> {
        self.store.get(image)
    }

    fn peak_of(&self, moment: MomentId) -> AuraResult<Option<MomentPeak>> {
        self.store.peak_of(moment)
    }

    fn reactions_to(&self, action: ImageId) -> AuraResult<Vec<ReactionLink>> {
        self.store.reactions_to(action)
    }

    fn ranked(&self, project: ProjectId, limit: usize) -> AuraResult<Vec<(ImageId, f32)>> {
        self.store.ranked(project, limit)
    }

    fn prefer(&self, choice: Preference) -> Result<(), AuraError> {
        self.store.record_preference(choice)
    }

    fn set_peak(&self, moment: MomentId, image: ImageId) -> Result<(), AuraError> {
        self.store.set_peak(moment, image)
    }
}

/// The project walk.
pub struct EmotionPass {
    previews: Arc<dyn PreviewService>,
    analyser: Analyser,
    store: Arc<EmotionStore>,
    people: Option<Arc<dyn PeopleService>>,
    story: Option<Arc<dyn StoryService>>,
    tradition: Option<String>,
    clock: Arc<dyn Clock>,
}

impl fmt::Debug for EmotionPass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EmotionPass")
            .field("weights_ver", &self.analyser.weights().version())
            .field("people", &self.people.is_some())
            .field("story", &self.story.is_some())
            .finish_non_exhaustive()
    }
}

impl EmotionPass {
    /// Assemble a pass.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5039` when the embedded weight table will not load.
    pub fn new(
        previews: Arc<dyn PreviewService>,
        infer: Arc<dyn InferService>,
        store: Arc<EmotionStore>,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, AuraError> {
        Ok(Self {
            previews,
            analyser: Analyser::new(infer)?,
            store,
            people: None,
            story: None,
            tradition: None,
            clock,
        })
    }

    /// Read who is in each frame through phase 06's frozen service.
    ///
    /// Optional, and the degradation is documented rather than silent: every frame is
    /// read on interactions alone, `EmotionCode::NoFaces` is in the reasons, the
    /// confidence drops by 0.25 and `EmotionOutline::face_aware` reports zero. Seven of
    /// the nine ranker features come from faces, so a wedding ranked without a face pass
    /// is very nearly a wedding ranked on nothing - and the product says so rather than
    /// quietly producing an ordering.
    #[must_use]
    pub fn with_people(mut self, people: Arc<dyn PeopleService>) -> Self {
        self.people = Some(people);
        self
    }

    /// Read the scene through phase 07's frozen service.
    ///
    /// Optional. Without it every frame is weighted by `[default]`, which is invariant 7
    /// degraded rather than broken: the weights are still a table lookup and the default
    /// is the documented substitute. The confidence drops by 0.10 and
    /// `ImageEmotion::scene` records `unknown`, so nobody later mistakes a default
    /// weighting for a scene-conditioned one.
    #[must_use]
    pub fn with_story(mut self, story: Arc<dyn StoryService>) -> Self {
        self.story = Some(story);
        self
    }

    /// Set the wedding's tradition, for the cultural multipliers.
    ///
    /// A slug from `crates/aura-brain-wedding/config/rituals/`. Absent means no
    /// multipliers - which is the neutral case and **not** a Western default, because
    /// `[default]` in the weight table is set from the balanced fixture set.
    #[must_use]
    pub fn with_tradition(mut self, tradition: impl Into<String>) -> Self {
        self.tradition = Some(tradition.into());
        self
    }

    /// Use a different weight table.
    #[must_use]
    pub fn with_weights(mut self, weights: Arc<EmotionWeights>) -> Self {
        self.analyser = self.analyser.clone().with_weights(weights);
        self
    }

    /// Read expressions from the fixture marker. Tests and gates only.
    #[must_use]
    pub fn with_reference_expressions(mut self) -> Self {
        self.analyser = self.analyser.clone().with_reference_expressions();
        self
    }

    /// The analyser underneath.
    #[must_use]
    pub const fn analyser(&self) -> &Analyser {
        &self.analyser
    }

    /// Load both heads before a photographer is waiting on them.
    ///
    /// # Errors
    ///
    /// Whatever the loader raised.
    pub fn warmup(&self) -> AuraResult<()> {
        self.analyser.warmup()
    }

    /// Score every photograph in a project that has no current reading.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the pending set cannot be read. Per-photograph failures are
    /// counted in the report rather than returned: one unreadable frame must not end a
    /// 4,000-image pass.
    pub fn run(
        &self,
        project: &ProjectId,
        prio: Priority,
        cancel: &CancelToken,
        progress: &dyn ProgressSink,
    ) -> AuraResult<EmotionReport> {
        let pending = self.store.pending(
            project,
            MODEL_VER,
            ANALYSIS_VER,
            self.analyser.weights().version(),
        )?;
        self.run_ids(project, &pending, prio, cancel, progress)
    }

    /// Score a specific list of photographs, then rebuild the peaks and the links.
    ///
    /// # Errors
    ///
    /// As [`EmotionPass::run`].
    #[allow(clippy::too_many_lines)]
    pub fn run_ids(
        &self,
        project: &ProjectId,
        ids: &[PhotoId],
        prio: Priority,
        cancel: &CancelToken,
        progress: &dyn ProgressSink,
    ) -> AuraResult<EmotionReport> {
        // The same four-frame window `aura_people::scan` and `IntegrityPass::run_ids` use.
        // A 2048 px proxy is 12 MB and a large window would hold a hundred of them to
        // save a few milliseconds.
        const PREFETCH_WINDOW: usize = 4;

        let started = self.clock.monotonic_ms();
        let mut report = EmotionReport::default();
        let total = ids.len() as u64;
        let couple = self.couple(project);
        let preview_priority = match prio {
            Priority::Visible => PreviewPriority::Visible,
            Priority::Interactive => PreviewPriority::Interactive,
            Priority::AiBatch => PreviewPriority::AiBatch,
            Priority::Background => PreviewPriority::Background,
        };
        let mut score_total = 0.0f64;

        for (position, id) in ids.iter().enumerate() {
            if cancel.is_cancelled() {
                report.cancelled = true;
                break;
            }
            if let Some(window) =
                ids.get(position + 1..(position + 1 + PREFETCH_WINDOW).min(ids.len()))
            {
                self.previews.prefetch(window, EMOTION_LEVEL);
            }

            let buffer = match self.previews.get(*id, EMOTION_LEVEL, preview_priority) {
                Ok(buffer) => buffer,
                Err(err) => {
                    Self::report_failure(*id, &err.detail);
                    report.failed += 1;
                    continue;
                }
            };

            let context = self.context_for(*id, &couple);
            let result = match self.analyser.analyse(*id, &buffer, &context, prio) {
                Ok(result) => result,
                Err(err) => {
                    Self::report_failure(*id, &err.detail);
                    report.failed += 1;
                    continue;
                }
            };

            // Drop the decoded proxy before writing. Twelve megabytes held across a
            // transaction is how a 4,000-image pass ends up with a resident set nobody
            // predicted; phase 06 learned this and phase 09 wrote it down.
            drop(buffer);

            report.faces += result.faces.len();
            for (kind, _) in &result.interactions {
                if let Some(slot) = report.interaction_histogram.get_mut(kind.as_index()) {
                    *slot += 1;
                }
            }
            score_total += f64::from(result.emotion_score);

            match self.store.put(project, &result) {
                Ok(()) => report.scored += 1,
                Err(err) => {
                    tracing::warn!(
                        target: "emotion.pass",
                        photo = %id,
                        code = %err.code,
                        "could not store an emotion reading"
                    );
                    report.failed += 1;
                }
            }

            progress.report(ProgressUpdate {
                stage: "emotion.pass",
                done: (report.scored + report.failed) as u64,
                total,
                current: None,
            });
        }

        // Steps 3 to 5. Arithmetic over stored readings; no file is opened.
        if report.scored > 0 && !report.cancelled {
            let peaks = self.rebuild_peaks(project)?;
            report.moments = peaks.0;
            report.peaked = peaks.1;
            report.mean_margin = peaks.2;

            let links = self.rebuild_links(project, &couple)?;
            report.links = links.0;
            report.mean_bonus = links.1;
        }

        report.mean_score = if report.scored == 0 {
            0.0
        } else {
            (score_total / report.scored as f64) as f32
        };
        report.elapsed_ms = self.clock.monotonic_ms().saturating_sub(started);

        // Section 11's four events, minus `emotion.cloud_used`, which `aura-cloud`'s
        // audit trail already emits for every provider call.
        tracing::info!(
            target: STAGE,
            images = report.scored,
            ms = report.elapsed_ms,
            mean_score = report.mean_score,
            interaction_histogram = ?report.interaction_histogram,
            failed = report.failed,
            "emotion pass finished"
        );
        tracing::info!(
            target: PEAK_STAGE,
            moments = report.moments,
            peaked = report.peaked,
            mean_margin = report.mean_margin,
            "moment peaks"
        );
        tracing::info!(
            target: REACTION_STAGE,
            links = report.links,
            mean_bonus = report.mean_bonus,
            "reaction links"
        );

        Ok(report)
    }

    /// Build every moment's curve and store the peaks and the proximities.
    ///
    /// Returns `(moments, peaked, mean_margin)`.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the moments or the readings cannot be read.
    pub fn rebuild_peaks(&self, project: &ProjectId) -> AuraResult<(usize, usize, f32)> {
        let moments = self.store.moments_of(project)?;
        let mut proximities: BTreeMap<PhotoId, f32> = BTreeMap::new();
        let mut resolved = 0usize;
        let mut margin_total = 0.0f64;

        for (moment, frames) in &moments {
            let mut curve_frames = Vec::with_capacity(frames.len());
            for image in frames {
                let Some(reading) = self.store.get(*image)? else {
                    continue;
                };
                curve_frames.push(Frame {
                    image: *image,
                    faces: reading.faces,
                    interactions: reading.interactions,
                    scene: reading.scene,
                });
            }
            if curve_frames.is_empty() {
                continue;
            }
            let Some((peak, curve)) = peak::peak_of(*moment, &curve_frames) else {
                continue;
            };
            for (index, frame) in curve_frames.iter().enumerate() {
                proximities.insert(frame.image, curve.proximity(index));
            }
            if peak.is_resolved() {
                resolved += 1;
                margin_total += f64::from(peak.margin);
            } else {
                let err = errors::peak_unresolved(&moment.to_db(), curve_frames.len(), peak.margin);
                tracing::info!(target: PEAK_STAGE, code = %err.code, "{}", err.detail);
            }
            self.store.put_peak(&peak)?;
        }

        self.store.set_proximities(&proximities)?;
        let mean = if resolved == 0 {
            0.0
        } else {
            (margin_total / resolved as f64) as f32
        };
        Ok((moments.len(), resolved, mean))
    }

    /// Find and store every reaction link in a project.
    ///
    /// Returns `(links, mean_bonus)`.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the readings cannot be read.
    pub fn rebuild_links(
        &self,
        project: &ProjectId,
        _couple: &[IdentityId],
    ) -> AuraResult<(usize, f32)> {
        let candidates = self.candidates(project)?;
        let roles = self.roles(project);

        // The window scan is over a time-sorted list, so each action reaches forward and
        // backward until the gap exceeds the window and then stops. A full cross product
        // over four thousand frames is sixteen million pairs; this is about forty per
        // action.
        let mut links: Vec<ReactionLink> = Vec::new();
        for (index, action) in candidates.iter().enumerate() {
            if action.strength < reaction::ACTION_FLOOR {
                continue;
            }
            let mut window: Vec<Candidate> = Vec::new();
            for other in candidates.iter().skip(index + 1) {
                if other.ts.saturating_sub(action.ts) > ReactionLink::WINDOW_MS {
                    break;
                }
                window.push(other.clone());
            }
            for other in candidates.iter().take(index).rev() {
                if action.ts.saturating_sub(other.ts) > ReactionLink::WINDOW_MS {
                    break;
                }
                window.push(other.clone());
            }
            links.extend(reaction::link(action, &window, &roles));
        }

        let resolved = reaction::resolve(links);
        let written = self.store.put_links(project, &resolved, ANALYSIS_VER)?;
        let bonus_total: f64 = resolved.iter().map(|link| f64::from(link.bonus)).sum();
        let mean = if resolved.is_empty() {
            0.0
        } else {
            (bonus_total / resolved.len() as f64) as f32
        };

        // Re-score the frames the links changed. Only those: a wedding with forty links
        // does not need four thousand re-scores.
        let touched: BTreeSet<PhotoId> = resolved.iter().map(|link| link.reaction).collect();
        for image in touched {
            self.rescore_one(project, image)?;
        }

        Ok((written, mean))
    }

    /// Re-score one frame from stored readings, after its proximity or bonus changed.
    ///
    /// Opens no file and runs no head. This is what makes step 5 cheap.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the reading cannot be read or written.
    pub fn rescore_one(&self, project: &ProjectId, image: ImageId) -> AuraResult<()> {
        let Some(stored) = self.store.get(image)? else {
            return Ok(());
        };
        // The bonus this frame earns as somebody's *reaction*, which is the action's view
        // of the link read back. A frame that is nobody's reaction earns nothing here,
        // and a frame that other frames react to earns nothing either - that is the
        // action's own significance, and it is already in its expression and interaction
        // readings.
        let bonus = stored
            .reaction_of
            .and_then(|action| self.store.reactions_to(action).ok())
            .into_iter()
            .flatten()
            .find(|link| link.reaction == image)
            .map_or(0.0, |link| link.bonus);

        let couple = self.couple(project);
        let mut context = self.context_for(image, &couple);
        context.scene = stored.scene;
        let mut rescored = self.analyser.rescore(
            image,
            &context,
            stored.faces.clone(),
            stored.interactions.clone(),
            stored.mutual_gaze,
            stored.peak_proximity,
            bonus,
            stored.confidence,
        );
        rescored.reaction_of = stored.reaction_of;
        rescored.narrative_weight = stored.narrative_weight;
        rescored.source = stored.source;
        self.store.put(project, &rescored)
    }

    /// Every scored frame in the project, in timeline order, as reaction candidates.
    fn candidates(&self, project: &ProjectId) -> AuraResult<Vec<Candidate>> {
        let rows = self.frame_rows(project)?;
        let mut out = Vec::with_capacity(rows.len());
        for (image, ts, moment, camera) in rows {
            let Some(reading) = self.store.get(image)? else {
                continue;
            };
            let strength = reading
                .faces
                .iter()
                .map(aura_core::contract::emotion::FaceExpression::intensity)
                .fold(0.0f32, f32::max)
                .max(
                    reading
                        .interactions
                        .first()
                        .map_or(0.0, |(_, value)| *value),
                );
            out.push(Candidate {
                image,
                ts,
                moment,
                camera,
                faces: reading.faces,
                interactions: reading.interactions,
                strength,
            });
        }
        Ok(out)
    }

    /// Timeline time, moment and camera for every photograph in a project.
    ///
    /// One query rather than three per frame. `moment_images` is read back as opaque ids;
    /// see `EmotionStore`'s header for why that is not a second grouping.
    fn frame_rows(
        &self,
        project: &ProjectId,
    ) -> AuraResult<Vec<(PhotoId, Timestamp, Option<MomentId>, String)>> {
        let key = project.to_db();
        self.store.catalog().read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT p.photo_id, p.timeline_time, mi.moment_id, COALESCE(p.camera_id, '')
                       FROM photo p
                       JOIN image_interaction e ON e.photo_id = p.photo_id
                       LEFT JOIN moment_images mi ON mi.photo_id = p.photo_id
                      WHERE p.project_id = ?1
                      ORDER BY p.timeline_time, p.photo_id",
                )
                .map_err(|e| {
                    aura_core::errors::db::statement_failed("could not read the frame rows", &e)
                })?;
            let mut out = Vec::new();
            let mut cursor = statement.query(rusqlite::params![key]).map_err(|e| {
                aura_core::errors::db::statement_failed("could not read the frame rows", &e)
            })?;
            while let Some(row) = cursor.next().map_err(|e| {
                aura_core::errors::db::statement_failed("could not read the frame rows", &e)
            })? {
                let photo_text: String = row.get(0).map_err(|e| {
                    aura_core::errors::db::statement_failed("could not read a photo id", &e)
                })?;
                let Ok(image) = PhotoId::from_db(&photo_text) else {
                    continue;
                };
                let ts = row
                    .get::<_, Option<String>>(1)
                    .ok()
                    .flatten()
                    .and_then(|text| aura_index::store::parse_rfc3339_ms(&text))
                    .unwrap_or(0);
                let moment = row
                    .get::<_, Option<String>>(2)
                    .ok()
                    .flatten()
                    .and_then(|text| MomentId::from_db(&text).ok());
                let camera: String = row.get(3).unwrap_or_default();
                out.push((image, ts, moment, camera));
            }
            Ok(out)
        })
    }

    /// Every identity's role, for the reaction bonus.
    ///
    /// Read once per pass. An error is not fatal and is not logged as one: a wedding with
    /// no identities simply gets the guest weight everywhere, which is
    /// `reaction::role_weight`'s documented behaviour for an unassigned face.
    fn roles(&self, project: &ProjectId) -> BTreeMap<IdentityId, Role> {
        let key = project.to_db();
        self.store
            .catalog()
            .read(move |conn| {
                let mut out = BTreeMap::new();
                let Ok(mut statement) =
                    conn.prepare("SELECT id, role FROM identities WHERE project_id = ?1")
                else {
                    return Ok(out);
                };
                let Ok(mut cursor) = statement.query(rusqlite::params![key]) else {
                    return Ok(out);
                };
                while let Ok(Some(row)) = cursor.next() {
                    let (Ok(id), Ok(role)) = (row.get::<_, String>(0), row.get::<_, String>(1))
                    else {
                        continue;
                    };
                    if let Ok(identity) = IdentityId::from_db(&id) {
                        out.insert(identity, Role::from_str_or_unknown(&role));
                    }
                }
                Ok(out)
            })
            .unwrap_or_default()
    }

    /// Assemble one frame's context from the two frozen services.
    fn context_for(&self, image: PhotoId, couple: &[IdentityId]) -> FrameContext {
        let scene_result = self
            .story
            .as_ref()
            .and_then(|story| story.scene(image).ok().flatten());
        let scene = scene_result
            .as_ref()
            .map_or(aura_core::SceneId::Unknown, |result| result.scene);
        let subjects = self
            .people
            .as_ref()
            .and_then(|people| people.subjects(image).ok())
            .unwrap_or_default();

        FrameContext {
            scene,
            scene_known: scene_result.is_some(),
            tradition: self.tradition.clone(),
            subjects,
            couple: couple.to_vec(),
            faces_scanned: self.people.is_some(),
        }
    }

    /// The identities phase 06 calls primary, for the partner-gaze case.
    fn couple(&self, project: &ProjectId) -> Vec<IdentityId> {
        self.people
            .as_ref()
            .and_then(|people| people.hierarchy(*project).ok())
            .map(|hierarchy| hierarchy.primary)
            .unwrap_or_default()
    }

    /// Log one frame's failure, coded.
    fn report_failure(id: PhotoId, why: &str) {
        let coded = errors::emotion_failed(&id.to_db(), why);
        tracing::warn!(
            target: "emotion.pass",
            photo = %id,
            code = %coded.code,
            "{}", coded.detail
        );
    }
}
