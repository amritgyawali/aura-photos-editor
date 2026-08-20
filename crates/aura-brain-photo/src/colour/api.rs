//! The frozen `ColourService`, and the pass that fills it.
//!
//! Two halves, the shape phases 06 to 15 all settled on. [`Colour`] answers questions about
//! grades that already exist and is what every later phase holds. [`ColourPass`] walks a
//! project and produces them, and is what the job graph holds.
//!
//! ## Resumability, invariant 5
//!
//! **The work remaining is a query** - [`crate::colour::store::ColourStore::pending`] - not a
//! journal. Kill the process at 10 %, 50 % or 90 % and the next run asks the catalog which
//! photographs have no grade at this version. An `intent_ver` bump therefore heals itself.
//!
//! ## One pass, not two
//!
//! Phase 15's pass improves on a second run because its skin loci accumulate across a
//! wedding. Nothing here does: a grade is a function of one frame's own pixels, its scene and
//! phase 09's calibration, all of which are available the first time the frame is read. So
//! there is no `refine` step and no reason for one - and saying that plainly is worth more
//! than leaving a reader to notice its absence.
//!
//! ## No silent failure, invariant 9
//!
//! A frame whose proxy will not decode is counted, coded `AURA-ML-5068` and reported. The run
//! continues, and **no row is written** - so the next pass tries again. A written-but-neutral
//! grade would read to phases 17, 25 and 27 as "AURA decided this photograph needed nothing",
//! and all three act on that.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::contract::colour::{
    ColourCode, ColourDecision, ColourOutline, ColourOverride, ColourService, ImageId, VariantKind,
    REVIEW_BELOW,
};
use aura_core::contract::people::{ImageSubjects, PeopleService};
use aura_core::contract::scene::StoryService;
use aura_core::contract::style::{StyleAdvice, StyleQuery, StyleService};
use aura_core::progress::{CancelToken, ProgressSink, ProgressUpdate};
use aura_core::{AttrFlags, AuraError, AuraResult, PhotoId, Priority, ProjectId, SceneId};
use aura_infer::contract::infer::InferService;
use aura_preview::contract::service::{PreviewService, Priority as PreviewPriority};

use crate::colour::analyse::{Analyser, FrameContext, ANALYSIS_VER, COLOUR_LEVEL, MODEL_VER};
use crate::colour::intent::IntentTable;
use crate::colour::store::ColourStore;
use crate::errors;
use crate::integrity::calibration::CalibrationTable;

/// The telemetry stage name, matching section 11's `colour.decided` event.
pub const STAGE: &str = "colour.decided";

/// The skin guard telemetry stage, matching section 11's `colour.skin_guard_triggered`.
pub const SKIN_GUARD_STAGE: &str = "colour.skin_guard_triggered";

/// The clipping guard telemetry stage, matching section 11's `colour.clip_guard_resolve`.
pub const CLIP_GUARD_STAGE: &str = "colour.clip_guard_resolve";

/// The untargeted-scene telemetry stage.
///
/// Not in section 11's three events, and here for the reason phase 15's `tone.untargeted` is:
/// a wedding graded entirely against neutral intents is a quiet version of section 12's
/// fourth failure mode.
pub const UNTARGETED_STAGE: &str = "colour.untargeted";

/// What EXIF said about one frame, for this phase's purposes.
///
/// Read from the catalog by the caller rather than from the file by this crate: phase 01 owns
/// EXIF, and opening an original to re-read an ISO tag would break invariant 1's spirit and
/// section 11's budget at once.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FrameExif {
    /// Camera make, as written.
    pub make: String,
    /// Camera model, as written.
    pub model: String,
    /// ISO. Zero when EXIF had none, which is treated as base ISO.
    pub iso: u32,
    /// The original's pixel count in megapixels, for the uncalibrated fallback.
    pub megapixels: f32,
}

/// What one pass over a project did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PassReport {
    /// Photographs graded.
    pub decided: usize,
    /// Photographs that could not be graded, each logged with a code.
    pub failed: usize,
    /// Frames where there was skin to protect and it was measured.
    pub skin_measured: usize,
    /// Frames where the skin guard had to intervene.
    pub skin_guard_triggered: usize,
    /// Frames where every colour operation was withdrawn to keep skin where it was.
    pub skin_guard_withdrew: usize,
    /// Sum of the attenuations the guard applied, for the mean.
    pub attenuation_total: f64,
    /// Frames where the clipping guard re-solved.
    pub clip_guard_resolved: usize,
    /// How many times each parameter was reduced, by name.
    ///
    /// Section 11's `colour.clip_guard_resolve {param_histogram}`. A `BTreeMap` because the
    /// order reaches a log line and a report.
    pub clip_param_histogram: BTreeMap<String, u32>,
    /// Frames whose grade was scaled back by the subtlety cap.
    pub subtlety_capped: usize,
    /// Frames below the review threshold.
    pub low_confidence: usize,
    /// Sum of the contrasts, for the mean.
    pub contrast_total: f64,
    /// Sum of the shadow lifts, for the mean.
    pub shadow_total: f64,
    /// Sum of the subtlety figures, for the mean.
    pub subtlety_total: f64,
    /// The worst skin hue shift anywhere in the run, in degrees.
    pub worst_skin_hue_shift: f32,
    /// Scenes graded against the neutral intent row, by slug.
    pub untargeted: Vec<String>,
    /// Wall clock for the whole pass.
    pub elapsed_ms: u64,
    /// True when the pass stopped early because it was cancelled.
    pub cancelled: bool,
}

impl PassReport {
    /// Milliseconds per graded photograph, or zero when nothing was graded.
    #[must_use]
    pub fn ms_per_image(&self) -> f64 {
        if self.decided == 0 {
            return 0.0;
        }
        self.elapsed_ms as f64 / self.decided as f64
    }

    /// Mean contrast over graded frames.
    #[must_use]
    pub fn mean_contrast(&self) -> f32 {
        if self.decided == 0 {
            return 0.0;
        }
        (self.contrast_total / self.decided as f64) as f32
    }

    /// Mean shadow lift over graded frames.
    #[must_use]
    pub fn mean_shadow_lift(&self) -> f32 {
        if self.decided == 0 {
            return 0.0;
        }
        (self.shadow_total / self.decided as f64) as f32
    }

    /// Mean subtlety over graded frames.
    #[must_use]
    pub fn mean_subtlety(&self) -> f32 {
        if self.decided == 0 {
            return 0.0;
        }
        (self.subtlety_total / self.decided as f64) as f32
    }

    /// Mean attenuation over the frames the guard acted on.
    #[must_use]
    pub fn mean_attenuation(&self) -> f32 {
        if self.skin_guard_triggered == 0 {
            return 1.0;
        }
        (self.attenuation_total / self.skin_guard_triggered as f64) as f32
    }

    /// Fraction of graded frames where the guarantee was actually checked.
    ///
    /// **The number that matters when it is low.** A wedding at 100 % coverage and 3 %
    /// skin-measured has had its headline guarantee verified on almost nothing, which is what
    /// a wedding with an untrained face detector looks like.
    #[must_use]
    pub fn skin_measured_ratio(&self) -> f32 {
        if self.decided == 0 {
            return 0.0;
        }
        self.skin_measured as f32 / self.decided as f32
    }
}

/// The frozen service. What every later phase holds.
pub struct Colour {
    store: Arc<ColourStore>,
}

impl fmt::Debug for Colour {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Colour").finish_non_exhaustive()
    }
}

impl Colour {
    /// Wrap one catalog's colour table.
    #[must_use]
    pub fn new(store: Arc<ColourStore>) -> Self {
        Self { store }
    }

    /// Open one over a catalog directly.
    #[must_use]
    pub fn over(catalog: Arc<Catalog>, clock: Arc<dyn Clock>) -> Self {
        Self::new(Arc::new(ColourStore::new(catalog, clock)))
    }

    /// The store underneath, for the pass and the gate.
    #[must_use]
    pub fn store(&self) -> &Arc<ColourStore> {
        &self.store
    }

    /// Which versions this build would produce.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5070` when the embedded intent table will not load.
    pub fn current_versions() -> Result<(u16, u16, u16), AuraError> {
        Ok((MODEL_VER, ANALYSIS_VER, IntentTable::embedded()?.version()))
    }
}

impl ColourService for Colour {
    fn outline(&self, project: ProjectId) -> AuraResult<ColourOutline> {
        let mut outline = self.store.outline(project)?;
        if let Ok(table) = IntentTable::embedded() {
            outline.untargeted_scenes = table.untargeted();
        }
        // The version check, reported rather than enforced. `AURA-ML-5071` is degraded: stale
        // grades keep working while the background pass replaces them, and the outline
        // reports the lowest version present so a caller about to draw a conclusion over a
        // mixed set finds out before it draws it.
        if let Ok((model, analysis, intent)) = Colour::current_versions() {
            if outline.decided > 0
                && (outline.model_ver, outline.analysis_ver, outline.intent_ver)
                    != (model, analysis, intent)
            {
                let stale = errors::colour_version_mismatch(
                    (outline.model_ver, outline.analysis_ver, outline.intent_ver),
                    (model, analysis, intent),
                    outline.decided as usize,
                );
                tracing::warn!(
                    target: "colour.version",
                    code = %stale.code,
                    "{}", stale.detail
                );
            }
        }
        Ok(outline)
    }

    fn of_image(&self, image: ImageId) -> AuraResult<Option<ColourDecision>> {
        self.store.get(image)
    }

    fn needs_review(&self, project: ProjectId, limit: usize) -> AuraResult<Vec<ImageId>> {
        self.store.needs_review(project, limit)
    }

    fn accept(&self, image: ImageId) -> Result<(), AuraError> {
        self.store.accept(image)
    }

    fn set_override(&self, image: ImageId, values: ColourOverride) -> Result<(), AuraError> {
        self.store.set_override(image, &values)
    }

    fn select_variant(&self, image: ImageId, kind: VariantKind) -> Result<(), AuraError> {
        self.store.select_variant(image, kind)
    }
}

/// The project walk.
pub struct ColourPass {
    previews: Arc<dyn PreviewService>,
    analyser: Analyser,
    store: Arc<ColourStore>,
    calibration: CalibrationTable,
    people: Option<Arc<dyn PeopleService>>,
    story: Option<Arc<dyn StoryService>>,
    style: Option<Arc<dyn StyleService>>,
    project: Option<ProjectId>,
    exif: BTreeMap<PhotoId, FrameExif>,
    clock: Arc<dyn Clock>,
}

impl fmt::Debug for ColourPass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ColourPass")
            .field("intent_ver", &self.analyser.intents().version())
            .field("people", &self.people.is_some())
            .field("story", &self.story.is_some())
            .field("style", &self.style.is_some())
            .finish_non_exhaustive()
    }
}

impl ColourPass {
    /// Assemble a pass.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5070` when the embedded intent table will not load, and `AURA-ML-5036` when
    /// the camera calibration table will not.
    pub fn new(
        previews: Arc<dyn PreviewService>,
        infer: Arc<dyn InferService>,
        store: Arc<ColourStore>,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, AuraError> {
        Ok(Self {
            previews,
            analyser: Analyser::new(infer)?,
            store,
            calibration: CalibrationTable::embedded()?,
            people: None,
            story: None,
            style: None,
            project: None,
            exif: BTreeMap::new(),
            clock,
        })
    }

    /// Read who is in each frame through phase 06's frozen service.
    ///
    /// Optional, and the degradation is documented rather than silent: **every frame becomes
    /// faceless**. The subject contrast is measured over the centre half of the frame instead
    /// of over the faces, no skin is sampled, and the guarantee in section 6.3 is not checked
    /// on a single photograph. `ColourOutline::skin_measured` reports zero and the panel says
    /// so, rather than the product quietly claiming a guarantee it did not verify.
    #[must_use]
    pub fn with_people(mut self, people: Arc<dyn PeopleService>) -> Self {
        self.people = Some(people);
        self
    }

    /// Read the scene through phase 07's frozen service.
    ///
    /// Optional. Without it every frame is graded against the neutral intent row, which is
    /// invariant 7 degraded rather than broken: the intents are still a table's rather than a
    /// constant's. The confidence drops and the decision records `unknown`.
    #[must_use]
    pub fn with_story(mut self, story: Arc<dyn StoryService>) -> Self {
        self.story = Some(story);
        self
    }

    /// Read the photographer's own look through phase 17's frozen service.
    ///
    /// Optional, and the degradation is the whole safety property of a residual design:
    /// **without it every frame is graded exactly as phase 16 decided on its own.** There is no
    /// state of this system in which a missing profile makes a photograph worse than switching
    /// the feature off would.
    ///
    /// The advice is applied to the solved grade and **before** the clipping guard and the skin
    /// guard, so a personal style that would move somebody's skin is a personal style the guard
    /// withdraws. ADR-0035 decision 1, and phase 16's exit report wrote the rule before this
    /// phase existed.
    #[must_use]
    pub fn with_style(mut self, style: Arc<dyn StyleService>, project: ProjectId) -> Self {
        self.style = Some(style);
        self.project = Some(project);
        self
    }

    /// Supply the EXIF the calibration lookup needs.
    #[must_use]
    pub fn with_exif(mut self, exif: BTreeMap<PhotoId, FrameExif>) -> Self {
        self.exif = exif;
        self
    }

    /// Use a different intent table.
    #[must_use]
    pub fn with_intents(mut self, intents: IntentTable) -> Self {
        self.analyser = self.analyser.with_intents(intents);
        self
    }

    /// The analyser underneath.
    #[must_use]
    pub const fn analyser(&self) -> &Analyser {
        &self.analyser
    }

    /// Load the head before a photographer is waiting on it.
    ///
    /// # Errors
    ///
    /// Whatever the loader raised.
    pub fn warmup(&self) -> AuraResult<()> {
        self.analyser.warmup()
    }

    /// Grade every photograph in a project that has no current grade.
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
            self.analyser.intents().version(),
        )?;
        let report = self.run_ids(project, &pending, prio, cancel, progress)?;
        self.emit(&report);
        Ok(report)
    }

    /// Grade a specific list of photographs.
    ///
    /// The incremental path: after a second card is imported, the caller passes the new ids
    /// and nothing else is touched.
    ///
    /// # Errors
    ///
    /// As [`ColourPass::run`].
    #[allow(clippy::too_many_lines)]
    pub fn run_ids(
        &self,
        project: &ProjectId,
        ids: &[PhotoId],
        prio: Priority,
        cancel: &CancelToken,
        progress: &dyn ProgressSink,
    ) -> AuraResult<PassReport> {
        // The preview service decodes on its own threads, so the next frame's proxy is being
        // built while this one is graded. Four rather than forty: a 2048 px proxy is 12 MB.
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
        let mut untargeted: BTreeSet<String> = BTreeSet::new();

        for (position, id) in ids.iter().enumerate() {
            if cancel.is_cancelled() {
                report.cancelled = true;
                break;
            }
            if let Some(window) =
                ids.get(position + 1..(position + 1 + PREFETCH_WINDOW).min(ids.len()))
            {
                self.previews.prefetch(window, COLOUR_LEVEL);
            }

            let buffer = match self.previews.get(*id, COLOUR_LEVEL, preview_priority) {
                Ok(buffer) => buffer,
                Err(err) => {
                    let coded = errors::colour_failed(&id.to_db(), &err.detail);
                    tracing::warn!(
                        target: "colour.pass",
                        photo = %id,
                        code = %coded.code,
                        "{}", coded.detail
                    );
                    report.failed += 1;
                    continue;
                }
            };

            let context = self.context_for(*id);
            let outcome = match self.analyser.analyse(*id, &buffer, &context, prio) {
                Ok(outcome) => outcome,
                Err(err) => {
                    tracing::warn!(
                        target: "colour.pass",
                        photo = %id,
                        code = %err.code,
                        "{}", err.detail
                    );
                    report.failed += 1;
                    continue;
                }
            };

            // Drop the decoded proxy before writing. Twelve megabytes held across a write is
            // how a 4,000-image pass ends up with a resident set nobody predicted.
            drop(buffer);

            match self
                .store
                .put(project, &outcome.decision, &outcome.reason_numbers)
            {
                Ok(()) => Self::tally(&mut report, &outcome.decision, &mut untargeted),
                Err(err) => {
                    tracing::warn!(
                        target: "colour.pass",
                        photo = %id,
                        code = %err.code,
                        "could not store a colour decision"
                    );
                    report.failed += 1;
                }
            }

            progress.report(ProgressUpdate {
                stage: "colour.pass",
                done: (report.decided + report.failed) as u64,
                total,
                current: None,
            });
        }

        report.untargeted = untargeted.into_iter().collect();
        report.elapsed_ms = self.clock.monotonic_ms().saturating_sub(started);
        Ok(report)
    }

    /// Add one decision to the running totals.
    fn tally(
        report: &mut PassReport,
        decision: &ColourDecision,
        untargeted: &mut BTreeSet<String>,
    ) {
        report.decided += 1;
        report.contrast_total += f64::from(decision.contrast);
        report.shadow_total += f64::from(decision.shadows);
        report.subtlety_total += f64::from(decision.subtlety);
        if decision.skin_measured {
            report.skin_measured += 1;
            report.worst_skin_hue_shift = report
                .worst_skin_hue_shift
                .max(decision.skin_guard.max_hue_shift_deg);
        }
        if decision.skin_guard.intervened() {
            report.skin_guard_triggered += 1;
            report.attenuation_total += f64::from(decision.skin_guard.attenuation);
        }
        if decision.confidence < REVIEW_BELOW {
            report.low_confidence += 1;
        }
        for reason in &decision.reasons {
            match reason.code {
                ColourCode::SkinGuardWithdrew => report.skin_guard_withdrew += 1,
                ColourCode::ClippingGuardResolved => {
                    report.clip_guard_resolved += 1;
                    // The parameter name is inside the sentence the reason carries, which is
                    // the one place it exists - the code is the same whichever parameter
                    // moved, because the remedy is the same.
                    for name in ["white point", "contrast", "tone curve"] {
                        if reason.text.contains(name) {
                            *report
                                .clip_param_histogram
                                .entry(name.to_string())
                                .or_insert(0) += 1;
                        }
                    }
                }
                ColourCode::SubtletyCapped => report.subtlety_capped += 1,
                ColourCode::NoIntentRow => {
                    untargeted.insert(decision.scene.as_str().to_string());
                }
                _ => {}
            }
        }
    }

    /// Section 11's three events, plus the fourth this module argues for.
    fn emit(&self, report: &PassReport) {
        tracing::info!(
            target: STAGE,
            images = report.decided,
            ms = report.elapsed_ms,
            mean_contrast = report.mean_contrast(),
            mean_shadow_lift = report.mean_shadow_lift(),
            mean_subtlety = report.mean_subtlety(),
            skin_measured_ratio = report.skin_measured_ratio(),
            worst_skin_hue_shift = report.worst_skin_hue_shift,
            failed = report.failed,
            ep = self.analyser.ep(),
            "colour pass finished"
        );
        tracing::info!(
            target: SKIN_GUARD_STAGE,
            count = report.skin_guard_triggered,
            withdrew = report.skin_guard_withdrew,
            mean_attenuation = report.mean_attenuation(),
            "skin guard"
        );
        tracing::info!(
            target: CLIP_GUARD_STAGE,
            count = report.clip_guard_resolved,
            param_histogram = ?report.clip_param_histogram,
            "clipping guard"
        );
        for scene in &report.untargeted {
            tracing::warn!(
                target: UNTARGETED_STAGE,
                scene = %scene,
                "a scene has no tone intent row"
            );
        }
    }

    /// Assemble one frame's context from the two frozen services and the EXIF map.
    fn context_for(&self, image: PhotoId) -> FrameContext {
        let scene_result = match &self.story {
            Some(story) => match story.scene(image) {
                Ok(scene) => scene,
                Err(err) => {
                    tracing::warn!(
                        target: "colour.context",
                        photo = %image,
                        service = "story",
                        code = %err.code,
                        "the scene service failed; the neutral grading intent was used"
                    );
                    None
                }
            },
            None => None,
        };
        let scene = scene_result.as_ref().map_or(SceneId::Unknown, |r| r.scene);
        let attrs = scene_result
            .as_ref()
            .map_or(AttrFlags::EMPTY, |r| r.attributes);
        let subjects = self.subjects_of(image);
        let exif = self.exif.get(&image).cloned().unwrap_or_default();
        let calibration = self
            .calibration
            .get(&exif.make, &exif.model, exif.megapixels);

        FrameContext {
            scene,
            scene_known: scene_result.is_some(),
            attrs,
            subjects,
            shadow_headroom_ev: calibration.shadow_headroom_at(exif.iso),
            style: self.style_for(scene),
        }
    }

    /// What the photographer's own look says about a frame of this kind.
    ///
    /// A failure of the style service is a **warning and a neutral answer**, never a failed
    /// frame: the grade phase 16 decided is a complete, guarded, deliverable answer and losing
    /// the personal lean on it is a degradation rather than a defect. Invariant 9's fallback
    /// path, and the same shape `subjects_of` and the scene lookup above already have.
    fn style_for(&self, scene: SceneId) -> Option<StyleAdvice> {
        let (style, project) = (self.style.as_ref()?, self.project?);
        let query = StyleQuery {
            project: Some(project),
            scene,
            // The lighting axis comes from phase 15's stored estimate, which this pass does not
            // hold. Until it does, every frame buckets as `LightingBucket::Unknown` and the
            // resolution falls back to the scene group - which is a real and honest degradation
            // and is exactly what `FallbackLevel::Group` is for. Wiring `ToneService` in here is
            // a small change and it belongs with whichever phase needs the light rather than
            // with the one that added the hook.
            ..StyleQuery::default()
        };
        match style.advise(&query) {
            Ok(advice) => Some(advice),
            Err(err) => {
                tracing::warn!(
                    target: "colour.context",
                    service = "style",
                    code = %err.code,
                    "the style service failed; this frame was graded without a personal look"
                );
                None
            }
        }
    }

    /// Who is in one photograph, or nobody.
    fn subjects_of(&self, image: PhotoId) -> ImageSubjects {
        match &self.people {
            Some(people) => match people.subjects(image) {
                Ok(subjects) => subjects,
                Err(err) => {
                    tracing::warn!(
                        target: "colour.context",
                        photo = %image,
                        service = "people",
                        code = %err.code,
                        "the people service failed; the skin guarantee was not checked"
                    );
                    ImageSubjects::default()
                }
            },
            None => ImageSubjects::default(),
        }
    }
}
