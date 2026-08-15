//! The frozen `IntegrityService`, and the pass that fills it.
//!
//! Two halves, the same shape phases 06, 07 and 08 all settled on. [`Integrity`] answers
//! questions about verdicts that already exist and is what every later phase holds.
//! [`IntegrityPass`] walks a project and produces them, and is what the job graph holds.
//!
//! Splitting them is not tidiness. The service is `Send + Sync` and cheap to clone into
//! nine call sites; the pass owns two models, a preview service and a calibration table,
//! and there should be exactly one of it.
//!
//! ## Resumability, invariant 5
//!
//! **The work remaining is a query** - [`IntegrityStore::pending`] - not a journal. Kill
//! the process at 10 %, 50 % or 90 % and the next run asks the catalog which photographs
//! have no verdict at this version. A `calib_ver` bump therefore heals itself: the rows
//! made under the old table are *pending* by definition, and the background pass picks
//! them up without anybody triggering anything.
//!
//! ## No silent failure, invariant 9
//!
//! A frame whose proxy will not decode is counted, coded `AURA-ML-5035` and reported.
//! The run continues, and **no row is written** - so the next pass tries again, which is
//! right for a transient decode failure and harmless for a permanent one. The same choice
//! `aura_people::scan` makes, for the same reason, and the reason it matters more here is
//! that a written-but-empty verdict would read as "nothing wrong with this photograph" to
//! phase 12.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::contract::integrity::{
    ImageId, IntegrityFlags, IntegrityOutline, IntegrityResult, IntegrityService,
};
use aura_core::contract::people::PeopleService;
use aura_core::contract::scene::StoryService;
use aura_core::progress::{CancelToken, ProgressSink, ProgressUpdate};
use aura_core::{AuraError, AuraResult, MomentId, PhotoId, Priority, ProjectId, SceneId};
use aura_infer::contract::infer::InferService;
use aura_preview::contract::service::{PreviewService, Priority as PreviewPriority};

use crate::errors;
use crate::integrity::analyse::{Analyser, FrameContext, FrameExif, ANALYSIS_VER, INTEGRITY_LEVEL};
use crate::integrity::calibration::CalibrationTable;
use crate::integrity::focus::MODEL_VER;
use crate::integrity::store::{IntegrityStore, Provenance};

/// The telemetry stage name, matching section 11's `integrity.scored` event.
pub const STAGE: &str = "integrity.scored";

/// The eye telemetry stage, matching section 11's `integrity.eyes` event.
pub const EYE_STAGE: &str = "integrity.eyes";

/// The uncalibrated-body telemetry stage, matching section 11's third event.
pub const UNCALIBRATED_STAGE: &str = "integrity.camera_uncalibrated";

/// What one pass over a project did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PassReport {
    /// Photographs analysed.
    pub scored: usize,
    /// Photographs that could not be analysed, each logged with a code.
    pub failed: usize,
    /// Faces whose eyes were judged.
    pub faces: usize,
    /// Faces whose eyes were closed.
    pub closed: usize,
    /// Closures the intent rules justified.
    pub closed_ok: usize,
    /// Faces squinting.
    pub squint: usize,
    /// How many frames carry each flag, in `IntegrityFlags::ALL` order.
    pub flag_histogram: [u32; IntegrityFlags::COUNT],
    /// Mean technical score over the frames this pass scored.
    pub mean_score: f32,
    /// Bodies with no calibration row, by make and model.
    pub uncalibrated: Vec<String>,
    /// Wall clock for the whole pass.
    pub elapsed_ms: u64,
    /// True when the pass stopped early because it was cancelled.
    pub cancelled: bool,
}

impl PassReport {
    /// Milliseconds per analysed photograph, or zero when nothing was analysed.
    #[must_use]
    pub fn ms_per_image(&self) -> f64 {
        if self.scored == 0 {
            return 0.0;
        }
        self.elapsed_ms as f64 / self.scored as f64
    }

    /// How many frames carry at least one defect flag.
    ///
    /// An upper bound - one frame can be soft *and* noisy - and documented as such
    /// wherever it is shown, for the reason `IntegrityOutline::defective_at_most` gives.
    #[must_use]
    pub fn defective_at_most(&self) -> u32 {
        IntegrityFlags::ALL
            .iter()
            .enumerate()
            .filter(|(_, flag)| IntegrityFlags::DEFECTS.contains(**flag))
            .filter_map(|(index, _)| self.flag_histogram.get(index).copied())
            .sum()
    }
}

/// The frozen service. What every later phase holds.
pub struct Integrity {
    store: Arc<IntegrityStore>,
}

impl fmt::Debug for Integrity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Integrity").finish_non_exhaustive()
    }
}

impl Integrity {
    /// Wrap one catalog's integrity tables.
    #[must_use]
    pub fn new(store: Arc<IntegrityStore>) -> Self {
        Self { store }
    }

    /// Open one over a catalog directly.
    #[must_use]
    pub fn over(catalog: Arc<Catalog>, clock: Arc<dyn Clock>) -> Self {
        Self::new(Arc::new(IntegrityStore::new(catalog, clock)))
    }

    /// The store underneath, for the pass and the gate.
    #[must_use]
    pub fn store(&self) -> &Arc<IntegrityStore> {
        &self.store
    }

    /// Which versions this build would produce.
    ///
    /// `(model_ver, analysis_ver, calib_ver)`, and the third needs a loaded table - so a
    /// caller that has one passes it, and a caller that does not gets the shipped one.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5036` when the embedded calibration table will not load.
    pub fn current_versions() -> Result<(u16, u16, u16), AuraError> {
        Ok((MODEL_VER, ANALYSIS_VER, CalibrationTable::embedded()?.version()))
    }
}

impl IntegrityService for Integrity {
    fn outline(&self, project: ProjectId) -> AuraResult<IntegrityOutline> {
        let outline = self.store.outline(project)?;
        // The version check, reported rather than enforced. `AURA-ML-5033` is degraded:
        // stale verdicts keep working while the background pass replaces them, and the
        // outline reports the lowest version present so that a caller about to draw a
        // conclusion over a mixed set finds out before it draws it.
        if let Ok((model, analysis, calib)) = Integrity::current_versions() {
            if outline.scored > 0
                && (outline.model_ver, outline.analysis_ver, outline.calib_ver)
                    != (model, analysis, calib)
            {
                let stale = errors::integrity_version_mismatch(
                    (outline.model_ver, outline.analysis_ver, outline.calib_ver),
                    (model, analysis, calib),
                    outline.scored as usize,
                );
                tracing::warn!(
                    target: "integrity.version",
                    code = %stale.code,
                    "{}", stale.detail
                );
            }
        }
        Ok(outline)
    }

    fn of_image(&self, image: ImageId) -> AuraResult<Option<IntegrityResult>> {
        self.store.get(image)
    }

    fn flagged(
        &self,
        project: ProjectId,
        flags: IntegrityFlags,
        limit: usize,
    ) -> AuraResult<Vec<ImageId>> {
        self.store.flagged(project, flags, limit)
    }

    fn within_moment(&self, moment: MomentId) -> AuraResult<Vec<(ImageId, f32)>> {
        self.store.relative_sharpness(moment)
    }

    fn dismiss(&self, image: ImageId, flag: IntegrityFlags) -> Result<(), AuraError> {
        self.store.dismiss(image, flag)
    }
}

/// The project walk.
pub struct IntegrityPass {
    previews: Arc<dyn PreviewService>,
    analyser: Analyser,
    store: Arc<IntegrityStore>,
    people: Option<Arc<dyn PeopleService>>,
    story: Option<Arc<dyn StoryService>>,
    clock: Arc<dyn Clock>,
}

impl fmt::Debug for IntegrityPass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IntegrityPass")
            .field("calib_ver", &self.analyser.calibration().version())
            .field("people", &self.people.is_some())
            .field("story", &self.story.is_some())
            .finish_non_exhaustive()
    }
}

impl IntegrityPass {
    /// Assemble a pass.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5036` when the embedded calibration table will not load.
    pub fn new(
        previews: Arc<dyn PreviewService>,
        infer: Arc<dyn InferService>,
        store: Arc<IntegrityStore>,
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
    /// Optional, and the degradation when it is absent is documented rather than
    /// silent: every frame is judged as subjectless, `NO_SUBJECT_DETECTED` is set, the
    /// confidence drops by 0.12 and `IntegrityOutline::subject_aware` reports zero. A
    /// wedding analysed without a face pass is a wedding judged on frame-wide sharpness,
    /// which is the ordinary global measure this phase exists to replace - so the product
    /// says so instead of quietly doing it.
    #[must_use]
    pub fn with_people(mut self, people: Arc<dyn PeopleService>) -> Self {
        self.people = Some(people);
        self
    }

    /// Read the scene and its profile through phase 07's frozen service.
    ///
    /// Optional. Without it every frame is judged on `SceneProfile::neutral`, which is
    /// invariant 7 degraded rather than broken: the thresholds are still a function of a
    /// profile, and the profile is the documented substitute. The confidence drops by
    /// 0.10 and `IntegrityResult::scene` records `unknown`, so nobody later mistakes a
    /// neutral judgement for a scene-conditioned one.
    #[must_use]
    pub fn with_story(mut self, story: Arc<dyn StoryService>) -> Self {
        self.story = Some(story);
        self
    }

    /// Use a different calibration table.
    #[must_use]
    pub fn with_calibration(mut self, table: CalibrationTable) -> Self {
        self.analyser = self.analyser.with_calibration(table);
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

    /// Analyse every photograph in a project that has no current verdict.
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
            self.analyser.calibration().version(),
        )?;
        self.run_ids(project, &pending, prio, cancel, progress)
    }

    /// Analyse a specific list of photographs.
    ///
    /// The incremental path: after a second card is imported, the caller passes the new
    /// ids and nothing else is touched.
    ///
    /// # Errors
    ///
    /// As [`IntegrityPass::run`].
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
        // being built while this one is being analysed. Four rather than forty: a 2048 px
        // proxy is 12 MB and a large window would hold a hundred of them to save a few
        // milliseconds. The same window `aura_people::scan` uses.
        const PREFETCH_WINDOW: usize = 4;

        let started = self.clock.monotonic_ms();
        let mut report = PassReport::default();
        let total = ids.len() as u64;
        let exif = self.store.exif_table(project)?;
        let couple = self.couple(project);
        let preview_priority = match prio {
            Priority::Visible => PreviewPriority::Visible,
            Priority::Interactive => PreviewPriority::Interactive,
            Priority::AiBatch => PreviewPriority::AiBatch,
            Priority::Background => PreviewPriority::Background,
        };
        let mut score_total = 0.0f64;
        let mut uncalibrated: BTreeMap<String, u32> = BTreeMap::new();

        for (position, id) in ids.iter().enumerate() {
            if cancel.is_cancelled() {
                report.cancelled = true;
                break;
            }
            if let Some(window) =
                ids.get(position + 1..(position + 1 + PREFETCH_WINDOW).min(ids.len()))
            {
                self.previews.prefetch(window, INTEGRITY_LEVEL);
            }

            let buffer = match self.previews.get(*id, INTEGRITY_LEVEL, preview_priority) {
                Ok(buffer) => buffer,
                Err(err) => {
                    let coded = errors::integrity_failed(&id.to_db(), &err.detail);
                    tracing::warn!(
                        target: "integrity.pass",
                        photo = %id,
                        code = %coded.code,
                        "{}", coded.detail
                    );
                    report.failed += 1;
                    continue;
                }
            };

            let context = self.context_for(*id, &exif, &couple);
            let result = match self.analyser.analyse(*id, &buffer, &context, prio) {
                Ok(result) => result,
                Err(err) => {
                    let coded = errors::integrity_failed(&id.to_db(), &err.detail);
                    tracing::warn!(
                        target: "integrity.pass",
                        photo = %id,
                        code = %coded.code,
                        "{}", coded.detail
                    );
                    report.failed += 1;
                    continue;
                }
            };

            // Drop the decoded proxy before writing. Twelve megabytes held across a
            // transaction is how a 4,000-image pass ends up with a resident set nobody
            // predicted; `aura_people::scan` learned this in phase 06.
            drop(buffer);

            let calibration = self.analyser.calibration().get(
                &context.exif.make,
                &context.exif.model,
                context.exif.megapixels,
            );
            if !calibration.measured {
                let body = format!("{} {}", context.exif.make, context.exif.model);
                let body = body.trim().to_string();
                if !body.is_empty() {
                    *uncalibrated.entry(body).or_insert(0) += 1;
                }
            }

            for (index, flag) in IntegrityFlags::ALL.into_iter().enumerate() {
                if result.flags.contains(flag) {
                    if let Some(slot) = report.flag_histogram.get_mut(index) {
                        *slot += 1;
                    }
                }
            }
            report.faces += result.eyes.len();
            report.closed += result
                .eyes
                .iter()
                .filter(|eye| eye.state.is_closed())
                .count();
            report.closed_ok += result
                .eyes
                .iter()
                .filter(|eye| eye.state.is_closed() && eye.intentional)
                .count();
            report.squint += result
                .eyes
                .iter()
                .filter(|eye| {
                    matches!(
                        eye.state,
                        aura_core::contract::integrity::EyeOpenness::Squint
                    )
                })
                .count();
            score_total += f64::from(result.technical_score);

            let provenance = Provenance {
                uncalibrated: !calibration.measured,
            };
            match self.store.put(project, &result, &provenance) {
                Ok(()) => report.scored += 1,
                Err(err) => {
                    tracing::warn!(
                        target: "integrity.pass",
                        photo = %id,
                        code = %err.code,
                        "could not store a technical verdict"
                    );
                    report.failed += 1;
                }
            }

            progress.report(ProgressUpdate {
                stage: "integrity.pass",
                done: (report.scored + report.failed) as u64,
                total,
                current: None,
            });
        }

        // The within-moment ranks, once, at the end. A frame's rank depends on its
        // siblings, and half a moment's siblings have not been analysed yet while the
        // pass is running - so computing it per frame would store a number that was true
        // for about four milliseconds.
        if report.scored > 0 {
            if let Err(err) = self.store.refresh_relative_sharpness(project) {
                tracing::warn!(
                    target: "integrity.pass",
                    code = %err.code,
                    "could not refresh the within-moment ranks; every other verdict stands"
                );
            }
        }

        report.uncalibrated = uncalibrated.keys().cloned().collect();
        report.mean_score = if report.scored == 0 {
            0.0
        } else {
            (score_total / report.scored as f64) as f32
        };
        report.elapsed_ms = self.clock.monotonic_ms().saturating_sub(started);

        // Section 11's three events.
        tracing::info!(
            target: STAGE,
            images = report.scored,
            ms = report.elapsed_ms,
            mean_score = report.mean_score,
            flag_histogram = ?report.flag_histogram,
            failed = report.failed,
            ep = self.analyser.ep(),
            "integrity pass finished"
        );
        tracing::info!(
            target: EYE_STAGE,
            faces = report.faces,
            closed = report.closed,
            closed_ok = report.closed_ok,
            squint = report.squint,
            "eye states"
        );
        for (body, frames) in &uncalibrated {
            let coded = errors::camera_uncalibrated(body, "");
            tracing::warn!(
                target: UNCALIBRATED_STAGE,
                body = %body,
                frames = frames,
                code = %coded.code,
                "a camera body has no calibration row"
            );
        }

        Ok(report)
    }

    /// Assemble one frame's context from the two frozen services.
    fn context_for(
        &self,
        image: PhotoId,
        exif: &BTreeMap<PhotoId, FrameExif>,
        couple: &[aura_core::IdentityId],
    ) -> FrameContext {
        let scene_result = self
            .story
            .as_ref()
            .and_then(|story| story.scene(image).ok().flatten());
        let scene = scene_result.as_ref().map_or(SceneId::Unknown, |r| r.scene);
        let profile = self.story.as_ref().map_or_else(
            || aura_core::SceneProfile::neutral(scene),
            |story| story.profile(scene),
        );
        let subjects = self
            .people
            .as_ref()
            .and_then(|people| people.subjects(image).ok())
            .unwrap_or_default();

        FrameContext {
            scene,
            profile,
            subjects,
            couple: couple.to_vec(),
            exif: exif.get(&image).cloned().unwrap_or_default(),
            scene_known: scene_result.is_some(),
        }
    }

    /// The identities phase 06 calls primary, for the both-partners intent rule.
    ///
    /// Read once per pass rather than per frame. An error here is not fatal and is not
    /// logged as one: a wedding with no confirmed couple simply cannot satisfy intent
    /// rule 4, and the other three still apply.
    fn couple(&self, project: &ProjectId) -> Vec<aura_core::IdentityId> {
        self.people
            .as_ref()
            .and_then(|people| people.hierarchy(*project).ok())
            .map(|hierarchy| hierarchy.primary)
            .unwrap_or_default()
    }

}
