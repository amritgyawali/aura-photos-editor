//! The frozen `CompositionService`, and the pass that fills it.
//!
//! Two halves, the same shape phases 06 to 10 all settled on. [`Composition`] answers
//! questions about judgements that already exist and is what every later phase holds.
//! [`CompositionPass`] walks a project and produces them, and is what the job graph holds.
//!
//! Splitting them is not tidiness. The service is `Send + Sync` and cheap to clone into
//! four call sites; the pass owns two models, a preview service and a rule table, and
//! there should be exactly one of it.
//!
//! ## Resumability, invariant 5
//!
//! **The work remaining is a query** - [`CompositionStore::pending`] - not a journal. Kill
//! the process at 10 %, 50 % or 90 % and the next run asks the catalog which photographs
//! have no judgement at this version. A `rules_ver` bump therefore heals itself: the rows
//! made under the old table are *pending* by definition, and the background pass picks
//! them up without anybody triggering anything. That matters more in this phase than in
//! phase 09, because a rule table is a product decision a release changes deliberately.
//!
//! ## No silent failure, invariant 9
//!
//! A frame whose proxy will not decode is counted, coded `AURA-ML-5045` and reported. The
//! run continues, and **no row is written** - so the next pass tries again, which is right
//! for a transient decode failure and harmless for a permanent one. A written-but-empty
//! judgement would read as "this photograph is well framed" to phases 12 and 29, and both
//! of them act on that.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::contract::composition::{
    CompositionFlags, CompositionOutline, CompositionResult, CompositionService, CropHint, ImageId,
};
use aura_core::contract::people::{ImageSubjects, PeopleService};
use aura_core::contract::scene::StoryService;
use aura_core::progress::{CancelToken, ProgressSink, ProgressUpdate};
use aura_core::{AuraError, AuraResult, MomentId, PhotoId, Priority, ProjectId, SceneId};
use aura_infer::contract::infer::InferService;
use aura_preview::contract::service::{PreviewService, Priority as PreviewPriority};

use crate::composition::analyse::{Analyser, FrameContext, ANALYSIS_VER, COMPOSITION_LEVEL};
use crate::composition::keypoints::MODEL_VER;
use crate::composition::rules::RuleTable;
use crate::composition::store::{CompositionStore, Provenance};
use crate::errors;

/// The telemetry stage name, matching section 11's `composition.scored` event.
pub const STAGE: &str = "composition.scored";

/// The tilt telemetry stage, matching section 11's `composition.tilt` event.
pub const TILT_STAGE: &str = "composition.tilt";

/// The unruled-scene telemetry stage.
///
/// Not in section 11's two events. It is here for the reason phase 09's
/// `integrity.camera_uncalibrated` is: section 12's first failure mode is rules that punish
/// creative work, and a wedding judged entirely on neutral bands is the quiet version of
/// that. A number nobody emits is a failure mode nobody notices.
pub const UNRULED_STAGE: &str = "composition.unruled";

/// What one pass over a project did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PassReport {
    /// Photographs judged.
    pub scored: usize,
    /// Photographs that could not be judged, each logged with a code.
    pub failed: usize,
    /// People the crop audit had keypoints for.
    pub keypoint_subjects: usize,
    /// Frames with at least one flagged cut.
    pub cut: usize,
    /// Frames whose tilt was read as deliberate.
    pub intentional_tilts: usize,
    /// Frames with a measurable horizon.
    pub horizons: usize,
    /// Sum of absolute tilt over frames with a measurable horizon.
    pub tilt_total_deg: f32,
    /// How many frames carry each flag, in `CompositionFlags::ALL` order.
    pub flag_histogram: [u32; CompositionFlags::COUNT],
    /// Mean composition score over the frames this pass judged.
    pub mean_score: f32,
    /// Frames with a crop hint for phase 23.
    pub hinted: usize,
    /// Scenes judged on the neutral rule row, by slug.
    pub unruled: Vec<String>,
    /// Wall clock for the whole pass.
    pub elapsed_ms: u64,
    /// True when the pass stopped early because it was cancelled.
    pub cancelled: bool,
}

impl PassReport {
    /// Milliseconds per judged photograph, or zero when nothing was judged.
    #[must_use]
    pub fn ms_per_image(&self) -> f64 {
        if self.scored == 0 {
            return 0.0;
        }
        self.elapsed_ms as f64 / self.scored as f64
    }

    /// Mean absolute tilt over frames with a measurable horizon.
    ///
    /// The denominator is deliberately not `scored`. Averaging in the zero an unmeasurable
    /// frame carries would make a wedding shot entirely indoors look perfectly level, which
    /// is the opposite of what section 11's `mean_abs_deg` is watched for.
    #[must_use]
    pub fn mean_abs_tilt(&self) -> f32 {
        if self.horizons == 0 {
            return 0.0;
        }
        self.tilt_total_deg / self.horizons as f32
    }

    /// How many frames carry at least one violation flag.
    ///
    /// An upper bound - one frame can be tilted *and* cluttered - and documented as such
    /// wherever it is shown.
    #[must_use]
    pub fn violating_at_most(&self) -> u32 {
        CompositionFlags::ALL
            .iter()
            .enumerate()
            .filter(|(_, flag)| CompositionFlags::DEFECTS.contains(**flag))
            .filter_map(|(index, _)| self.flag_histogram.get(index).copied())
            .sum()
    }
}

/// The frozen service. What every later phase holds.
pub struct Composition {
    store: Arc<CompositionStore>,
}

impl fmt::Debug for Composition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Composition").finish_non_exhaustive()
    }
}

impl Composition {
    /// Wrap one catalog's composition table.
    #[must_use]
    pub fn new(store: Arc<CompositionStore>) -> Self {
        Self { store }
    }

    /// Open one over a catalog directly.
    #[must_use]
    pub fn over(catalog: Arc<Catalog>, clock: Arc<dyn Clock>) -> Self {
        Self::new(Arc::new(CompositionStore::new(catalog, clock)))
    }

    /// The store underneath, for the pass and the gate.
    #[must_use]
    pub fn store(&self) -> &Arc<CompositionStore> {
        &self.store
    }

    /// Which versions this build would produce.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5046` when the embedded rule table will not load.
    pub fn current_versions() -> Result<(u16, u16, u16), AuraError> {
        Ok((MODEL_VER, ANALYSIS_VER, RuleTable::embedded()?.version()))
    }
}

impl CompositionService for Composition {
    fn outline(&self, project: ProjectId) -> AuraResult<CompositionOutline> {
        let outline = self.store.outline(project)?;
        // The version check, reported rather than enforced. `AURA-ML-5043` is degraded:
        // stale judgements keep working while the background pass replaces them, and the
        // outline reports the lowest version present so that a caller about to draw a
        // conclusion over a mixed set finds out before it draws it.
        if let Ok((model, analysis, rules)) = Composition::current_versions() {
            if outline.scored > 0
                && (outline.model_ver, outline.analysis_ver, outline.rules_ver)
                    != (model, analysis, rules)
            {
                let stale = errors::composition_version_mismatch(
                    (outline.model_ver, outline.analysis_ver, outline.rules_ver),
                    (model, analysis, rules),
                    outline.scored as usize,
                );
                tracing::warn!(
                    target: "composition.version",
                    code = %stale.code,
                    "{}", stale.detail
                );
            }
        }
        Ok(outline)
    }

    fn of_image(&self, image: ImageId) -> AuraResult<Option<CompositionResult>> {
        self.store.get(image)
    }

    fn flagged(
        &self,
        project: ProjectId,
        flags: CompositionFlags,
        limit: usize,
    ) -> AuraResult<Vec<ImageId>> {
        self.store.flagged(project, flags, limit)
    }

    fn within_moment(&self, moment: MomentId) -> AuraResult<Vec<(ImageId, f32)>> {
        self.store.relative_composition(moment)
    }

    fn crop_hint(&self, image: ImageId) -> AuraResult<Option<CropHint>> {
        self.store.crop_hint(image)
    }

    fn dismiss(&self, image: ImageId, flag: CompositionFlags) -> Result<(), AuraError> {
        self.store.dismiss(image, flag)
    }
}

/// The project walk.
pub struct CompositionPass {
    previews: Arc<dyn PreviewService>,
    analyser: Analyser,
    store: Arc<CompositionStore>,
    people: Option<Arc<dyn PeopleService>>,
    story: Option<Arc<dyn StoryService>>,
    clock: Arc<dyn Clock>,
}

impl fmt::Debug for CompositionPass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CompositionPass")
            .field("rules_ver", &self.analyser.rules().version())
            .field("people", &self.people.is_some())
            .field("story", &self.story.is_some())
            .finish_non_exhaustive()
    }
}

impl CompositionPass {
    /// Assemble a pass.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5046` when the embedded rule table will not load.
    pub fn new(
        previews: Arc<dyn PreviewService>,
        infer: Arc<dyn InferService>,
        store: Arc<CompositionStore>,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, AuraError> {
        Ok(Self {
            previews,
            analyser: Analyser::new(infer)?,
            store,
            people: None,
            story: None,
            clock,
        })
    }

    /// Read who is in each frame through phase 06's frozen service.
    ///
    /// Optional, and the degradation when it is absent is documented rather than silent:
    /// every frame is judged with no people in it, `NO_GEOMETRY` is set, six of the sixteen
    /// flags cannot fire, the confidence drops by 0.12 and
    /// `CompositionOutline::keypoint_aware` reports zero. A wedding judged without a face
    /// pass has had its horizon measured and its cropping ignored, so the product says so
    /// instead of quietly doing it.
    #[must_use]
    pub fn with_people(mut self, people: Arc<dyn PeopleService>) -> Self {
        self.people = Some(people);
        self
    }

    /// Read the scene through phase 07's frozen service.
    ///
    /// Optional. Without it every frame is judged on the neutral rule row, which is
    /// invariant 7 degraded rather than broken: the bands are still a table's rather than a
    /// constant's, and the neutral row is the documented substitute. The confidence drops
    /// and `CompositionResult::scene` records `unknown`, so nobody later mistakes a neutral
    /// judgement for a scene-conditioned one.
    #[must_use]
    pub fn with_story(mut self, story: Arc<dyn StoryService>) -> Self {
        self.story = Some(story);
        self
    }

    /// Use a different rule table.
    #[must_use]
    pub fn with_rules(mut self, rules: RuleTable) -> Self {
        self.analyser = self.analyser.with_rules(rules);
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

    /// Judge every photograph in a project that has no current judgement.
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
    ) -> AuraResult<PassReport> {
        let pending = self.store.pending(
            project,
            MODEL_VER,
            ANALYSIS_VER,
            self.analyser.rules().version(),
        )?;
        self.run_ids(project, &pending, prio, cancel, progress)
    }

    /// Judge a specific list of photographs.
    ///
    /// The incremental path: after a second card is imported, the caller passes the new ids
    /// and nothing else is touched.
    ///
    /// # Errors
    ///
    /// As [`CompositionPass::run`].
    #[allow(clippy::too_many_lines)]
    pub fn run_ids(
        &self,
        project: &ProjectId,
        ids: &[PhotoId],
        prio: Priority,
        cancel: &CancelToken,
        progress: &dyn ProgressSink,
    ) -> AuraResult<PassReport> {
        // The preview service decodes on its own threads, so the next frame's proxy is
        // being built while this one is being judged. Four rather than forty, for the reason
        // `aura_people::scan` gives: a 2048 px proxy is 12 MB.
        const PREFETCH_WINDOW: usize = 4;

        let started = self.clock.monotonic_ms();
        let mut report = PassReport::default();
        let total = ids.len() as u64;
        let preview_priority = match prio {
            Priority::Visible => PreviewPriority::Visible,
            Priority::Interactive => PreviewPriority::Interactive,
            Priority::AiBatch => PreviewPriority::AiBatch,
            Priority::Background => PreviewPriority::Background,
        };
        let mut score_total = 0.0f64;
        let mut unruled: BTreeMap<String, u32> = BTreeMap::new();

        for (position, id) in ids.iter().enumerate() {
            if cancel.is_cancelled() {
                report.cancelled = true;
                break;
            }
            if let Some(window) =
                ids.get(position + 1..(position + 1 + PREFETCH_WINDOW).min(ids.len()))
            {
                self.previews.prefetch(window, COMPOSITION_LEVEL);
            }

            let buffer = match self.previews.get(*id, COMPOSITION_LEVEL, preview_priority) {
                Ok(buffer) => buffer,
                Err(err) => {
                    let coded = errors::composition_failed(&id.to_db(), &err.detail);
                    tracing::warn!(
                        target: "composition.pass",
                        photo = %id,
                        code = %coded.code,
                        "{}", coded.detail
                    );
                    report.failed += 1;
                    continue;
                }
            };

            let context = self.context_for(*id);
            let result = match self.analyser.analyse(*id, &buffer, &context, prio) {
                Ok(result) => result,
                Err(err) => {
                    let coded = errors::composition_failed(&id.to_db(), &err.detail);
                    tracing::warn!(
                        target: "composition.pass",
                        photo = %id,
                        code = %coded.code,
                        "{}", coded.detail
                    );
                    report.failed += 1;
                    continue;
                }
            };

            // Drop the decoded proxy before writing. Twelve megabytes held across a write
            // is how a 4,000-image pass ends up with a resident set nobody predicted.
            drop(buffer);

            let rule = self.analyser.rules().get(context.scene);
            let provenance = Provenance {
                unruled: !rule.measured,
            };
            match self.store.put(project, &result, &provenance) {
                Ok(()) => {
                    report.scored += 1;
                    score_total += f64::from(result.composition_score);
                    if !rule.measured {
                        *unruled
                            .entry(context.scene.as_str().to_string())
                            .or_insert(0) += 1;
                    }
                    for (index, flag) in CompositionFlags::ALL.into_iter().enumerate() {
                        if result.flags.contains(flag) {
                            if let Some(slot) = report.flag_histogram.get_mut(index) {
                                *slot += 1;
                            }
                        }
                    }
                    report.keypoint_subjects += result.keypoint_subjects as usize;
                    if result
                        .joint_cuts
                        .iter()
                        .any(aura_core::JointCut::is_flagged)
                    {
                        report.cut += 1;
                    }
                    if result.tilt_intentional {
                        report.intentional_tilts += 1;
                    }
                    if result.horizon_source.is_measured() {
                        report.horizons += 1;
                        report.tilt_total_deg += result.tilt_deg.abs();
                    }
                    if result.crop_suggestion_hint.is_some() {
                        report.hinted += 1;
                    }
                }
                Err(err) => {
                    tracing::warn!(
                        target: "composition.pass",
                        photo = %id,
                        code = %err.code,
                        "could not store a framing judgement"
                    );
                    report.failed += 1;
                }
            }

            progress.report(ProgressUpdate {
                stage: "composition.pass",
                done: (report.scored + report.failed) as u64,
                total,
                current: None,
            });
        }

        // The within-moment ranks, once, at the end. A frame's rank depends on its siblings,
        // and half a moment's siblings have not been judged yet while the pass is running -
        // so computing it per frame would store a number that was true for four
        // milliseconds.
        if report.scored > 0 {
            if let Err(err) = self.store.refresh_relative_composition(project) {
                tracing::warn!(
                    target: "composition.pass",
                    code = %err.code,
                    "could not refresh the within-moment ranks; every other judgement stands"
                );
            }
        }

        report.unruled = unruled.keys().cloned().collect();
        report.mean_score = if report.scored == 0 {
            0.0
        } else {
            (score_total / report.scored as f64) as f32
        };
        report.elapsed_ms = self.clock.monotonic_ms().saturating_sub(started);

        // Section 11's two events, plus the third this module argues for.
        tracing::info!(
            target: STAGE,
            images = report.scored,
            ms = report.elapsed_ms,
            mean_score = report.mean_score,
            flag_histogram = ?report.flag_histogram,
            failed = report.failed,
            ep = self.analyser.ep(),
            "composition pass finished"
        );
        tracing::info!(
            target: TILT_STAGE,
            mean_abs_deg = report.mean_abs_tilt(),
            intentional_ratio = if report.horizons == 0 {
                0.0
            } else {
                report.intentional_tilts as f32 / report.horizons as f32
            },
            "tilt"
        );
        for (scene, frames) in &unruled {
            let coded = errors::scene_unruled(scene);
            tracing::warn!(
                target: UNRULED_STAGE,
                scene = %scene,
                frames = frames,
                code = %coded.code,
                "a scene has no composition rule row"
            );
        }

        Ok(report)
    }

    /// Assemble one frame's context from the two frozen services.
    fn context_for(&self, image: PhotoId) -> FrameContext {
        let scene_result = match &self.story {
            Some(story) => match story.scene(image) {
                Ok(scene) => scene,
                Err(err) => {
                    tracing::warn!(
                        target: "composition.context",
                        photo = %image,
                        service = "story",
                        code = %err.code,
                        "the scene service failed; neutral composition rules were used"
                    );
                    None
                }
            },
            None => None,
        };
        let scene = scene_result.as_ref().map_or(SceneId::Unknown, |r| r.scene);
        let subjects = match &self.people {
            Some(people) => match people.subjects(image) {
                Ok(subjects) => subjects,
                Err(err) => {
                    tracing::warn!(
                        target: "composition.context",
                        photo = %image,
                        service = "people",
                        code = %err.code,
                        "the people service failed; subject-dependent composition claims were withheld"
                    );
                    ImageSubjects::default()
                }
            },
            None => ImageSubjects::default(),
        };

        FrameContext {
            scene,
            subjects,
            scene_known: scene_result.is_some(),
            // No camera in the calibration table records a gravity vector and phase 01's
            // schema has no column for one. `None` rather than a guess, which is what makes
            // `HorizonSource::Gravity` a claim nothing in this build can make falsely.
            gravity_deg: None,
        }
    }
}
