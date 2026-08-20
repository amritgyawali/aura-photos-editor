//! The frozen `ToneService`, and the pass that fills it.
//!
//! Two halves, the shape phases 06 to 12 all settled on. [`Tone`] answers questions about
//! estimates that already exist and is what every later phase holds. [`TonePass`] walks a
//! project and produces them, and is what the job graph holds.
//!
//! ## Resumability, invariant 5
//!
//! **The work remaining is a query** - [`crate::tone::store::ToneStore::pending`] - not a
//! journal. Kill the process at 10 %, 50 % or 90 % and the next run asks the catalog which
//! photographs have no estimate at this version. A `targets_ver` bump therefore heals itself:
//! the rows made under the old table are pending by definition.
//!
//! ## The one thing about this pass that is not like the others: it improves on a second run
//!
//! Section 6.2 scores an illuminant hypothesis by what it does to known skin, and section 6.3
//! measures known skin by dividing out the illuminant. Each needs the other, and
//! `skin_locus`'s header sets out the resolution: samples are contributed from frames whose
//! colour was already confident, and the loci they build constrain the frames analysed after
//! them.
//!
//! This pass therefore does three things in order, and all three are visible in
//! [`PassReport`]:
//!
//! 1. **It refreshes the loci as it goes**, every [`LOCUS_REFRESH`] frames, so a wedding's
//!    later frames are constrained by its earlier ones.
//! 2. **It counts the frames that got no constraint** - [`PassReport::unconstrained`] - which
//!    is the honest measure of how much of the wedding was decided without knowing what
//!    anybody's skin looks like.
//! 3. **It re-analyses those frames once**, at the end, but only when the run actually
//!    produced new loci. [`TonePass::refine`] is that step. Without the guard a wedding with
//!    no faces in it would re-analyse itself completely on every run for no benefit; with it,
//!    a wedding with faces converges in one pass and a wedding without one does no extra work
//!    at all.
//!
//! ## No silent failure, invariant 9
//!
//! A frame whose proxy will not decode is counted, coded `AURA-ML-5062` and reported. The run
//! continues, and **no row is written** - so the next pass tries again. A written-but-neutral
//! estimate would read to phases 16, 17, 25 and 27 as "AURA decided this photograph needed
//! nothing", and all four act on that.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::contract::people::{ImageSubjects, PeopleService};
use aura_core::contract::scene::StoryService;
use aura_core::contract::style::{StyleAdvice, StyleQuery, StyleService};
use aura_core::contract::tone::{
    ImageId, ReferenceFrame, SkinLocus, ToneEstimate, ToneOutline, ToneOverride, ToneService,
};
use aura_core::progress::{CancelToken, ProgressSink, ProgressUpdate};
use aura_core::{
    AttrFlags, AuraError, AuraResult, IdentityId, PhotoId, Priority, ProjectId, SceneId, SegmentId,
};
use aura_infer::contract::infer::InferService;
use aura_preview::contract::service::{PreviewService, Priority as PreviewPriority};

use crate::errors;
use crate::integrity::calibration::CalibrationTable;
use crate::tone::analyse::{Analyser, FrameContext, ANALYSIS_VER, MODEL_VER, TONE_LEVEL};
use crate::tone::illuminant::AsShot;
use crate::tone::reference::{self, Candidate};
use crate::tone::skin_locus::LocusBuilder;
use crate::tone::store::ToneStore;
use crate::tone::targets::TargetTable;

/// The telemetry stage name, matching section 11's `tone.estimated` event.
pub const STAGE: &str = "tone.estimated";

/// The low-confidence telemetry stage, matching section 11's `tone.low_confidence` event.
pub const LOW_CONFIDENCE_STAGE: &str = "tone.low_confidence";

/// The override telemetry stage, matching section 11's `tone.user_override` event.
pub const OVERRIDE_STAGE: &str = "tone.user_override";

/// The untargeted-scene telemetry stage.
///
/// Not in section 11's three events, and here for the reason phase 11's
/// `composition.unruled` is: section 12's fourth failure mode is auto-exposure fighting the
/// photographer's intent, and a wedding exposed entirely against neutral bands is the quiet
/// version of that.
pub const UNTARGETED_STAGE: &str = "tone.untargeted";

/// How many frames the pass analyses before refreshing the loci.
///
/// Sixty-four. Small enough that a wedding's second chapter is constrained by its first;
/// large enough that the rebuild - a weighted mean and a trim over a few hundred samples per
/// person - happens sixty times in a 4,000-image pass rather than four thousand.
pub const LOCUS_REFRESH: usize = 64;

/// What EXIF said about one frame, for this phase's purposes.
///
/// Read from the catalog by the caller rather than from the file by this crate: phase 01 owns
/// EXIF, it is already in `exif`, and opening the original to re-read a white-balance tag
/// would break invariant 1's spirit and section 11's budget at once.
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
    /// What the camera decided about the colour of the light.
    pub as_shot: AsShot,
}

/// What one pass over a project did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PassReport {
    /// Photographs estimated.
    pub estimated: usize,
    /// Photographs that could not be estimated, each logged with a code.
    pub failed: usize,
    /// Frames whose exposure was anchored on a face.
    pub face_anchored: usize,
    /// Frames where at least one person's locus constrained the colour.
    pub constrained: usize,
    /// Frames where nobody's locus was available.
    ///
    /// The honest measure of how much of the wedding was decided without knowing what
    /// anybody's skin looks like. See the module header.
    pub unconstrained: usize,
    /// Frames carrying more than one illuminant.
    pub mixed_light: usize,
    /// Frames where a saturated light was preserved.
    pub coloured_light: usize,
    /// Frames below the review threshold.
    pub low_confidence: usize,
    /// How many low-confidence frames each scene contributed, by slug.
    ///
    /// Section 11's `tone.low_confidence {scene_histogram}`. A `BTreeMap` because the order
    /// reaches a log line and a report.
    pub low_confidence_by_scene: BTreeMap<String, u32>,
    /// Sum of the exposure offsets, for the mean.
    pub ev_total: f64,
    /// Sum of the temperatures, for the mean.
    pub cct_total: f64,
    /// Identities that ended the pass with a usable locus.
    pub loci: usize,
    /// Segments that ended the pass with enough anchors.
    pub segments_anchored: usize,
    /// Segments in the project.
    pub segments: usize,
    /// Frames re-analysed by [`TonePass::refine`].
    pub refined: usize,
    /// Scenes estimated against the neutral target row, by slug.
    pub untargeted: Vec<String>,
    /// Wall clock for the whole pass.
    pub elapsed_ms: u64,
    /// True when the pass stopped early because it was cancelled.
    pub cancelled: bool,
}

impl PassReport {
    /// Milliseconds per estimated photograph, or zero when nothing was estimated.
    #[must_use]
    pub fn ms_per_image(&self) -> f64 {
        if self.estimated == 0 {
            return 0.0;
        }
        self.elapsed_ms as f64 / self.estimated as f64
    }

    /// Mean exposure offset over estimated frames.
    #[must_use]
    pub fn mean_ev(&self) -> f32 {
        if self.estimated == 0 {
            return 0.0;
        }
        (self.ev_total / self.estimated as f64) as f32
    }

    /// Mean colour temperature over estimated frames.
    #[must_use]
    pub fn mean_cct(&self) -> f32 {
        if self.estimated == 0 {
            return 0.0;
        }
        (self.cct_total / self.estimated as f64) as f32
    }

    /// Fraction of estimated frames carrying more than one illuminant.
    #[must_use]
    pub fn mixed_light_ratio(&self) -> f32 {
        if self.estimated == 0 {
            return 0.0;
        }
        self.mixed_light as f32 / self.estimated as f32
    }

    /// Fraction of estimated frames whose exposure was anchored on a face.
    ///
    /// **The number that matters when it is low.** Section 1's whole argument is that
    /// subject-referred exposure is the improvement; a wedding at 3 % has been exposed the way
    /// every other editor exposes it.
    #[must_use]
    pub fn face_anchored_ratio(&self) -> f32 {
        if self.estimated == 0 {
            return 0.0;
        }
        self.face_anchored as f32 / self.estimated as f32
    }
}

/// The frozen service. What every later phase holds.
pub struct Tone {
    store: Arc<ToneStore>,
}

impl fmt::Debug for Tone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tone").finish_non_exhaustive()
    }
}

impl Tone {
    /// Wrap one catalog's tone tables.
    #[must_use]
    pub fn new(store: Arc<ToneStore>) -> Self {
        Self { store }
    }

    /// Open one over a catalog directly.
    #[must_use]
    pub fn over(catalog: Arc<Catalog>, clock: Arc<dyn Clock>) -> Self {
        Self::new(Arc::new(ToneStore::new(catalog, clock)))
    }

    /// The store underneath, for the pass and the gate.
    #[must_use]
    pub fn store(&self) -> &Arc<ToneStore> {
        &self.store
    }

    /// Which versions this build would produce.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5063` when the embedded target table will not load.
    pub fn current_versions() -> Result<(u16, u16, u16), AuraError> {
        Ok((MODEL_VER, ANALYSIS_VER, TargetTable::embedded()?.version()))
    }
}

impl ToneService for Tone {
    fn outline(&self, project: ProjectId) -> AuraResult<ToneOutline> {
        let mut outline = self.store.outline(project)?;
        if let Ok(table) = TargetTable::embedded() {
            outline.untargeted_scenes = table.untargeted();
        }
        // The version check, reported rather than enforced. `AURA-ML-5060` is degraded: stale
        // estimates keep working while the background pass replaces them, and the outline
        // reports the lowest version present so a caller about to draw a conclusion over a
        // mixed set finds out before it draws it.
        if let Ok((model, analysis, targets)) = Tone::current_versions() {
            if outline.estimated > 0
                && (outline.model_ver, outline.analysis_ver, outline.targets_ver)
                    != (model, analysis, targets)
            {
                let stale = errors::tone_version_mismatch(
                    (outline.model_ver, outline.analysis_ver, outline.targets_ver),
                    (model, analysis, targets),
                    outline.estimated as usize,
                );
                tracing::warn!(
                    target: "tone.version",
                    code = %stale.code,
                    "{}", stale.detail
                );
            }
        }
        Ok(outline)
    }

    fn of_image(&self, image: ImageId) -> AuraResult<Option<ToneEstimate>> {
        self.store.get(image)
    }

    fn needs_review(&self, project: ProjectId, limit: usize) -> AuraResult<Vec<ImageId>> {
        self.store.needs_review(project, limit)
    }

    fn reference_frames(&self, segment: SegmentId) -> AuraResult<Vec<ReferenceFrame>> {
        self.store.reference_frames(segment)
    }

    fn skin_locus(&self, identity: IdentityId) -> AuraResult<Option<SkinLocus>> {
        self.store.locus(identity)
    }

    fn skin_loci(&self, project: ProjectId) -> AuraResult<BTreeMap<IdentityId, SkinLocus>> {
        self.store.loci(project)
    }

    fn accept(&self, image: ImageId) -> Result<(), AuraError> {
        self.store.accept(image)
    }

    fn set_override(&self, image: ImageId, values: ToneOverride) -> Result<(), AuraError> {
        // Section 11's third telemetry event, emitted here rather than in the store because
        // the store is also what a re-analysis writes through and a re-analysis is not an
        // override.
        for (param, delta) in [
            ("exposure", values.exposure_ev),
            ("temperature", values.temperature_k),
            ("tint", values.tint),
        ] {
            if let Some(value) = delta {
                tracing::info!(target: OVERRIDE_STAGE, param = param, delta = value, "override");
            }
        }
        self.store.set_override(image, values)
    }
}

/// The project walk.
pub struct TonePass {
    previews: Arc<dyn PreviewService>,
    analyser: Analyser,
    store: Arc<ToneStore>,
    calibration: CalibrationTable,
    people: Option<Arc<dyn PeopleService>>,
    story: Option<Arc<dyn StoryService>>,
    style: Option<Arc<dyn StyleService>>,
    style_project: Option<ProjectId>,
    exif: BTreeMap<PhotoId, FrameExif>,
    clock: Arc<dyn Clock>,
}

impl fmt::Debug for TonePass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TonePass")
            .field("targets_ver", &self.analyser.targets().version())
            .field("people", &self.people.is_some())
            .field("story", &self.story.is_some())
            .field("style", &self.style.is_some())
            .finish_non_exhaustive()
    }
}

impl TonePass {
    /// Assemble a pass.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5063` when the embedded target table will not load, and `AURA-ML-5036` when
    /// the camera calibration table will not.
    pub fn new(
        previews: Arc<dyn PreviewService>,
        infer: Arc<dyn InferService>,
        store: Arc<ToneStore>,
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
            style_project: None,
            exif: BTreeMap::new(),
            clock,
        })
    }

    /// Read who is in each frame through phase 06's frozen service.
    ///
    /// Optional, and the degradation when it is absent is documented rather than silent:
    /// **every frame becomes faceless**. No exposure is anchored on a subject, no skin patch
    /// is sampled, no locus is ever built and the hard constraint never applies. That is the
    /// whole of section 1's improvement removed, so `ToneOutline::face_anchored` reports zero
    /// and the panel says so rather than the product quietly reverting to average-referred
    /// exposure.
    #[must_use]
    pub fn with_people(mut self, people: Arc<dyn PeopleService>) -> Self {
        self.people = Some(people);
        self
    }

    /// Read the scene through phase 07's frozen service.
    ///
    /// Optional. Without it every frame is estimated against the neutral target row, which is
    /// invariant 7 degraded rather than broken: the bands are still a table's rather than a
    /// constant's. The confidence drops and the estimate records `unknown`.
    #[must_use]
    pub fn with_story(mut self, story: Arc<dyn StoryService>) -> Self {
        self.story = Some(story);
        self
    }

    /// Read the photographer's own look through phase 17's frozen service.
    ///
    /// Optional, and without it every frame is exposed and balanced exactly as phase 15
    /// decided on its own - which is the safety property of a residual design and is the same
    /// degradation `ColourPass::with_style` documents.
    ///
    /// The shift is applied to the solved exposure and white balance and then phase 15's own
    /// clipping bound and skin-locus constraint re-run on it. ADR-0035 decision 1.
    #[must_use]
    pub fn with_style(mut self, style: Arc<dyn StyleService>, project: ProjectId) -> Self {
        self.style = Some(style);
        self.style_project = Some(project);
        self
    }

    /// Supply the EXIF the analyser needs.
    ///
    /// Taken as a map rather than read per frame, for the reason `FrameExif`'s doc comment
    /// gives: phase 01 owns EXIF and this crate must not open an original.
    #[must_use]
    pub fn with_exif(mut self, exif: BTreeMap<PhotoId, FrameExif>) -> Self {
        self.exif = exif;
        self
    }

    /// Use a different target table.
    #[must_use]
    pub fn with_targets(mut self, targets: TargetTable) -> Self {
        self.analyser = self.analyser.with_targets(targets);
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

    /// Estimate every photograph in a project that has no current estimate, then anchor the
    /// chapters.
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
            self.analyser.targets().version(),
        )?;
        let mut report = self.run_ids(project, &pending, prio, cancel, progress)?;
        if !report.cancelled {
            self.refine(project, &mut report, prio, cancel, progress)?;
            self.anchor_segments(project, &mut report)?;
        }
        self.emit(&report);
        Ok(report)
    }

    /// Estimate a specific list of photographs.
    ///
    /// The incremental path: after a second card is imported, the caller passes the new ids
    /// and nothing else is touched.
    ///
    /// # Errors
    ///
    /// As [`TonePass::run`].
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
        // built while this one is being estimated. Four rather than forty, for the reason
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

        let mut loci = self.store.loci(*project)?;
        let started_with = loci.len();
        let mut builder = LocusBuilder::new();
        let mut untargeted: BTreeSet<String> = BTreeSet::new();

        for (position, id) in ids.iter().enumerate() {
            if cancel.is_cancelled() {
                report.cancelled = true;
                break;
            }
            if let Some(window) =
                ids.get(position + 1..(position + 1 + PREFETCH_WINDOW).min(ids.len()))
            {
                self.previews.prefetch(window, TONE_LEVEL);
            }

            let buffer = match self.previews.get(*id, TONE_LEVEL, preview_priority) {
                Ok(buffer) => buffer,
                Err(err) => {
                    let coded = errors::tone_failed(&id.to_db(), &err.detail);
                    tracing::warn!(
                        target: "tone.pass",
                        photo = %id,
                        code = %coded.code,
                        "{}", coded.detail
                    );
                    report.failed += 1;
                    continue;
                }
            };

            let context = self.context_for(*id);
            let outcome = match self.analyser.analyse(*id, &buffer, &context, &loci, prio) {
                Ok(outcome) => outcome,
                Err(err) => {
                    tracing::warn!(
                        target: "tone.pass",
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

            for (identity, sample) in &outcome.samples {
                builder.add(*identity, *sample);
            }

            match self
                .store
                .put(project, &outcome.estimate, &outcome.reason_numbers)
            {
                Ok(()) => Self::tally(&mut report, &outcome.estimate, &mut untargeted),
                Err(err) => {
                    tracing::warn!(
                        target: "tone.pass",
                        photo = %id,
                        code = %err.code,
                        "could not store a tone estimate"
                    );
                    report.failed += 1;
                }
            }

            // Refresh the loci as the pass goes, so a wedding's later frames are constrained
            // by its earlier ones. See the module header.
            if position % LOCUS_REFRESH == LOCUS_REFRESH - 1 {
                merge_loci(&mut loci, &builder.finish(ANALYSIS_VER));
            }

            progress.report(ProgressUpdate {
                stage: "tone.pass",
                done: (report.estimated + report.failed) as u64,
                total,
                current: None,
            });
        }

        let fitted = builder.finish(ANALYSIS_VER);
        merge_loci(&mut loci, &fitted);
        for locus in fitted.values() {
            if let Err(err) = self.store.put_locus(project, locus) {
                tracing::warn!(
                    target: "tone.pass",
                    code = %err.code,
                    "could not store a skin locus; every estimate stands"
                );
            }
        }
        report.loci = loci.values().filter(|locus| locus.is_usable()).count();
        if report.loci == 0 && report.estimated > 0 {
            let coded = errors::skin_locus_unavailable(&project.to_db(), builder.identities());
            tracing::warn!(
                target: "tone.pass",
                code = %coded.code,
                "{}", coded.detail
            );
        }
        // Whether the run learned anything new. `refine` is gated on it, so a wedding with no
        // faces does no extra work at all.
        report.refined = usize::from(report.loci > started_with);
        report.untargeted = untargeted.into_iter().collect();
        report.elapsed_ms = self.clock.monotonic_ms().saturating_sub(started);
        Ok(report)
    }

    /// Re-analyse the frames that were decided before anybody had a locus.
    ///
    /// Runs **only when the pass produced a locus it did not start with**, which is the guard
    /// the module header describes: without it a wedding with no faces would re-analyse itself
    /// completely on every run for no benefit.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the unconstrained set cannot be read.
    pub fn refine(
        &self,
        project: &ProjectId,
        report: &mut PassReport,
        prio: Priority,
        cancel: &CancelToken,
        progress: &dyn ProgressSink,
    ) -> AuraResult<()> {
        if report.refined == 0 || report.unconstrained == 0 {
            report.refined = 0;
            return Ok(());
        }
        let loci = self.store.loci(*project)?;
        if loci.is_empty() {
            report.refined = 0;
            return Ok(());
        }
        // Which frames were decided without a constraint, and contain somebody who now has
        // one. Recomputed from the catalog rather than remembered from the walk, so that a
        // pass killed and resumed refines the same set.
        let mut wanted: Vec<PhotoId> = Vec::new();
        for id in self.store.all_photos(project)? {
            let Ok(Some(estimate)) = self.store.get(id) else {
                continue;
            };
            if estimate.constrained_identities > 0 || estimate.user_edited {
                continue;
            }
            let subjects = self.subjects_of(id);
            let now_constrained = subjects
                .faces
                .iter()
                .filter_map(|face| face.identity_id)
                .any(|identity| loci.get(&identity).is_some_and(SkinLocus::is_usable));
            if now_constrained {
                wanted.push(id);
            }
        }
        if wanted.is_empty() {
            report.refined = 0;
            return Ok(());
        }
        let before = report.estimated;
        let second = self.run_ids(project, &wanted, prio, cancel, progress)?;
        report.refined = second.estimated;
        report.constrained += second.constrained;
        report.unconstrained = report.unconstrained.saturating_sub(second.constrained);
        report.estimated = before;
        Ok(())
    }

    /// Choose every segment's anchors.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the segments or the estimates cannot be read.
    pub fn anchor_segments(&self, project: &ProjectId, report: &mut PassReport) -> AuraResult<()> {
        let segments = self.store.segments_of(project)?;
        report.segments = segments.len();
        let primary: BTreeSet<IdentityId> = match &self.people {
            Some(people) => people
                .hierarchy(*project)
                .map(|hierarchy| hierarchy.primary.into_iter().collect())
                .unwrap_or_default(),
            None => BTreeSet::new(),
        };

        for segment in segments {
            let photos = self.store.photos_in_segment(segment)?;
            let mut candidates = Vec::with_capacity(photos.len());
            for photo in photos {
                let Ok(Some(estimate)) = self.store.get(photo) else {
                    continue;
                };
                let target = self.analyser.target_for(estimate.scene);
                let subject_after = crate::tone::exposure::linear_to_perceptual(
                    crate::tone::exposure::perceptual_to_linear(estimate.subject_luma_before)
                        * estimate.exposure_ev.exp2(),
                );
                let subjects = self.subjects_of(photo);
                let has_primary = if primary.is_empty() {
                    // With no hierarchy there is nothing to require. Falling back to "any
                    // identified face" rather than to "every frame" keeps the gate meaning
                    // something: a chapter of empty rooms should still get no anchors.
                    subjects.faces.iter().any(|face| face.identity_id.is_some())
                } else {
                    subjects
                        .faces
                        .iter()
                        .filter_map(|face| face.identity_id)
                        .any(|identity| primary.contains(&identity))
                };
                candidates.push(Candidate {
                    image_id: photo,
                    wb_conf: estimate.wb_conf,
                    exposure_conf: estimate.exposure_conf,
                    temperature_k: estimate.temperature_k,
                    tint: estimate.tint,
                    subject_luma: subject_after,
                    subject_in_band: estimate.face_anchored && target.in_band(subject_after),
                    has_primary,
                    mixed_light: estimate.mixed_light,
                    coloured_light: estimate.coloured_light,
                });
            }
            let chosen = reference::select(segment, &candidates, ANALYSIS_VER);
            if !chosen.is_empty() {
                report.segments_anchored += 1;
            }
            self.store
                .put_reference_frames(segment, &chosen, ANALYSIS_VER)?;
        }
        Ok(())
    }

    /// Add one estimate to the running totals.
    fn tally(report: &mut PassReport, estimate: &ToneEstimate, untargeted: &mut BTreeSet<String>) {
        report.estimated += 1;
        report.ev_total += f64::from(estimate.exposure_ev);
        report.cct_total += f64::from(estimate.temperature_k);
        if estimate.face_anchored {
            report.face_anchored += 1;
        }
        if estimate.constrained_identities > 0 {
            report.constrained += 1;
        } else {
            report.unconstrained += 1;
        }
        if estimate.mixed_light {
            report.mixed_light += 1;
        }
        if estimate.coloured_light {
            report.coloured_light += 1;
        }
        if estimate.wb_conf < aura_core::contract::tone::REVIEW_WB_BELOW {
            report.low_confidence += 1;
            *report
                .low_confidence_by_scene
                .entry(estimate.scene.as_str().to_string())
                .or_insert(0) += 1;
        }
        if estimate
            .reasons
            .iter()
            .any(|reason| reason.code == aura_core::ToneCode::NoTargetRow)
        {
            untargeted.insert(estimate.scene.as_str().to_string());
        }
    }

    /// Section 11's three events, plus the fourth this module argues for.
    fn emit(&self, report: &PassReport) {
        tracing::info!(
            target: STAGE,
            images = report.estimated,
            ms = report.elapsed_ms,
            mean_ev = report.mean_ev(),
            mean_cct = report.mean_cct(),
            mixed_light_ratio = report.mixed_light_ratio(),
            face_anchored_ratio = report.face_anchored_ratio(),
            failed = report.failed,
            ep = self.analyser.ep(),
            "tone pass finished"
        );
        tracing::info!(
            target: LOW_CONFIDENCE_STAGE,
            count = report.low_confidence,
            scene_histogram = ?report.low_confidence_by_scene,
            "low confidence"
        );
        for scene in &report.untargeted {
            let coded = errors::scene_untargeted(scene);
            tracing::warn!(
                target: UNTARGETED_STAGE,
                scene = %scene,
                code = %coded.code,
                "a scene has no exposure target row"
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
                        target: "tone.context",
                        photo = %image,
                        service = "story",
                        code = %err.code,
                        "the scene service failed; the neutral exposure band was used"
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
            as_shot: exif.as_shot,
            shadow_headroom_ev: calibration.shadow_headroom_at(exif.iso),
            style: self.style_for(scene),
        }
    }

    /// What the photographer's own look says about a frame of this kind.
    ///
    /// A failed style lookup is a **warning and a neutral answer**, never a failed frame:
    /// phase 15's own estimate is complete, constrained and deliverable, and losing the
    /// personal lean on it is a degradation. Invariant 9's fallback path.
    ///
    /// The lighting axis is left at its default here. This pass is the one that *decides* what
    /// colour the light was, so it cannot also condition on the answer - which means a style
    /// resolves at the scene group's level on the first pass over a project. That is
    /// `FallbackLevel::Group`, it is recorded on every decision, and it is honest.
    fn style_for(&self, scene: SceneId) -> Option<StyleAdvice> {
        let (style, project) = (self.style.as_ref()?, self.style_project?);
        let query = StyleQuery {
            project: Some(project),
            scene,
            ..StyleQuery::default()
        };
        match style.advise(&query) {
            Ok(advice) => Some(advice),
            Err(err) => {
                tracing::warn!(
                    target: "tone.context",
                    service = "style",
                    code = %err.code,
                    "the style service failed; this frame was exposed without a personal look"
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
                        target: "tone.context",
                        photo = %image,
                        service = "people",
                        code = %err.code,
                        "the people service failed; the exposure was not anchored on a subject"
                    );
                    ImageSubjects::default()
                }
            },
            None => ImageSubjects::default(),
        }
    }
}

/// Fold newly fitted loci into the working map.
///
/// A newly fitted locus always wins: it was built from a superset of the samples the stored
/// one was, because the builder accumulates across the whole run.
fn merge_loci(
    into: &mut BTreeMap<IdentityId, SkinLocus>,
    fitted: &BTreeMap<IdentityId, SkinLocus>,
) {
    for (identity, locus) in fitted {
        into.insert(*identity, *locus);
    }
}
