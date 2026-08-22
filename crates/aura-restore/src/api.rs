//! The frozen `RestoreService`, and the pass that fills it.
//!
//! Two halves, the shape phases 06 to 21 all settled on. [`Restore`] answers questions about
//! plans that already exist and is what every later phase holds. [`RestorePass`] walks a project
//! and produces them, and is what the job graph holds.
//!
//! ## Resumability, invariant 5
//!
//! **The work remaining is a query** - [`crate::store::RestoreStore::pending`] - rather than a
//! journal. Kill the process at 10 %, 50 % or 90 % and the next run asks the catalog which
//! photographs have no plan at these three versions. A `profile_ver` bump therefore heals itself.
//!
//! ## Three-tier compute, invariant 3, and the one thing this phase does differently
//!
//! The *decision* is made on a 2048 px proxy like every phase since 06. The *operators* run at
//! full resolution, at export, on the frames being delivered - which is why section 11's budgets
//! are written about a 45 MP denoise while this pass's budget is written about a proxy.
//!
//! Section 6.4 adds a constraint no earlier phase had: "Restoration never runs on the interactive
//! path". [`RestorePass::run`] takes a [`RestoreWhen`], there is no third variant, and the render
//! graph refuses independently through `RenderPurpose::skip_heavy`. Two layers, neither of which
//! is a check somebody could forget to write.
//!
//! ## No silent failure, invariant 9
//!
//! A frame whose proxy will not decode is counted, coded `AURA-ML-5103` and reported. The run
//! continues and **no row is written**, so the next pass tries again - a written-but-empty plan
//! would read to phases 25, 27 and 28 as "AURA decided this photograph needed nothing", which is
//! a different and much worse statement than "AURA has not looked at this photograph yet".

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use aura_core::clock::Clock;
use aura_core::contract::composition::Box2;
use aura_core::contract::integrity::{IntegrityService, MotionKind};
use aura_core::contract::people::PeopleService;
use aura_core::contract::restore::{
    DenoiseTier, ImageId, RestoreCode, RestoreField, RestoreOutline, RestoreOverride, RestorePlan,
    RestoreReason, RestoreService, RestoreWhen,
};
use aura_core::contract::scene::StoryService;
use aura_core::progress::{CancelToken, ProgressSink, ProgressUpdate};
use aura_core::{AuraError, AuraResult, PhotoId, Priority, ProjectId, SceneId};
use aura_preview::contract::service::{PreviewService, Priority as PreviewPriority};
use aura_raw::contract::pixels::{PixelBuffer, PixelData};

use crate::decide::{Analyser, RestoreFrame, ANALYSIS_VER, MODEL_VER, RESTORE_LEVEL};
use crate::errors;
use crate::face_recovery::{FaceCandidate, IdentityProbe};
use crate::schedule::Capacity;

/// The telemetry stage name, matching section 11's `restore.applied` event.
pub const STAGE: &str = "restore.applied";

/// The self-check telemetry stage, matching section 11's `restore.selfcheck` event.
pub const SELFCHECK_STAGE: &str = "restore.selfcheck";

/// The identity telemetry stage, matching section 11's `restore.identity_guard` event.
pub const IDENTITY_STAGE: &str = "restore.identity_guard";

/// What one pass over a project did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RestorePassReport {
    /// Photographs planned.
    pub planned: usize,
    /// Photographs that could not be planned, each logged with a code.
    pub failed: usize,
    /// Photographs where at least one operation ran.
    pub acted_on: usize,
    /// Photographs where at least one usable region arrived.
    pub region_covered: usize,
    /// How many frames got each tier, in `DenoiseTier::ALL` order.
    pub tiers: [usize; DenoiseTier::COUNT],
    /// Frames that were sharpened.
    pub sharpened: usize,
    /// Faces recovered.
    pub faces_recovered: usize,
    /// Faces skipped because the embedding moved too far. **The guarantee's counter.**
    pub identity_refusals: usize,
    /// Frames where the self-check reduced or withdrew something.
    pub reduced: usize,
    /// Frames below the review threshold.
    pub low_confidence: usize,
    /// Camera bodies denoised against a synthetic noise model.
    pub unmeasured_cameras: Vec<String>,
    /// Scenes planned against the neutral row.
    pub unlisted_scenes: Vec<String>,
    /// Milliseconds for the pass.
    pub elapsed_ms: u64,
    /// True when the pass stopped early because it was cancelled.
    pub cancelled: bool,
}

/// The frozen `RestoreService` over one catalog.
#[derive(Debug)]
pub struct Restore {
    store: Arc<crate::store::RestoreStore>,
    versions: (u16, u16, u16),
}

impl Restore {
    /// Wrap a store.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5105` when the embedded tables will not load.
    pub fn new(store: Arc<crate::store::RestoreStore>) -> Result<Self, AuraError> {
        let analyser = Analyser::embedded(Capacity::default())?;
        Ok(Self {
            versions: analyser.versions(),
            store,
        })
    }

    /// The three versions this build produces.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5105` when the embedded tables will not load.
    pub fn current_versions() -> Result<(u16, u16, u16), AuraError> {
        Ok(Analyser::embedded(Capacity::default())?.versions())
    }

    /// The store underneath, for the panel's own reads and for the gate.
    #[must_use]
    pub fn store(&self) -> &Arc<crate::store::RestoreStore> {
        &self.store
    }
}

impl RestoreService for Restore {
    fn outline(&self, project: ProjectId) -> AuraResult<RestoreOutline> {
        let mut outline = self.store.outline(&project, self.versions)?;
        if let Ok(Some((stored, rows))) = self.store.stored_versions(&project) {
            if stored != self.versions && rows > 0 {
                // Reported rather than raised: a version drift is a background re-check rather
                // than a failure, and the caller decides whether to surface it.
                tracing::info!(
                    target: "restore.outline",
                    code = %errors::ML_RESTORE_VERSION_MISMATCH.0,
                    "{}",
                    errors::restore_version_mismatch(stored, self.versions, rows).detail
                );
                outline.model_ver = stored.0;
                outline.analysis_ver = stored.1;
                outline.profile_ver = stored.2;
            }
        }
        Ok(outline)
    }

    fn of_image(&self, image: ImageId) -> AuraResult<Option<RestorePlan>> {
        self.store.get(image)
    }

    fn identity_refusals(&self, project: ProjectId, limit: usize) -> AuraResult<Vec<ImageId>> {
        self.store.identity_refusals(&project, limit)
    }

    fn needs_review(&self, project: ProjectId, limit: usize) -> AuraResult<Vec<ImageId>> {
        self.store.needs_review(&project, limit)
    }

    fn accept(&self, image: ImageId) -> Result<(), AuraError> {
        self.store.accept(image)
    }

    fn set_override(&self, image: ImageId, values: RestoreOverride) -> Result<(), AuraError> {
        self.store.set_override(image, &values)
    }
}

/// The resumable project walk.
pub struct RestorePass {
    previews: Arc<dyn PreviewService>,
    store: Arc<crate::store::RestoreStore>,
    clock: Arc<dyn Clock>,
    analyser: Analyser,
    integrity: Option<Arc<dyn IntegrityService>>,
    people: Option<Arc<dyn PeopleService>>,
    story: Option<Arc<dyn StoryService>>,
    probe: Option<Arc<dyn IdentityProbe>>,
    regions: BTreeMap<PhotoId, Vec<RestoreField>>,
    output_long_edge: u32,
    enabled: bool,
}

impl fmt::Debug for RestorePass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RestorePass")
            .field("regions", &self.regions.len())
            .field("output_long_edge", &self.output_long_edge)
            .field("enabled", &self.enabled)
            .finish_non_exhaustive()
    }
}

impl RestorePass {
    /// How many frames ahead the pass asks the preview service to decode.
    pub const PREFETCH_WINDOW: usize = 4;

    /// Build a pass over one catalog.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5105` when the embedded tables will not load.
    pub fn new(
        previews: Arc<dyn PreviewService>,
        store: Arc<crate::store::RestoreStore>,
        clock: Arc<dyn Clock>,
        capacity: Capacity,
    ) -> AuraResult<Self> {
        Ok(Self {
            previews,
            store,
            clock,
            analyser: Analyser::embedded(capacity)?,
            integrity: None,
            people: None,
            story: None,
            probe: None,
            regions: BTreeMap::new(),
            output_long_edge: 2048,
            enabled: true,
        })
    }

    /// Supply phase 09's verdicts, for the measured noise, the motion and the focus.
    ///
    /// **The only route to a tier above `Off`.** Section 6.1 requires the tier to come from the
    /// measured sigma relative to the scene's tolerance, and phase 09 is the one place that
    /// number exists. Without it every frame records `restore_no_noise_reading` and nothing is
    /// denoised, which is the conservative direction and the only correct one.
    #[must_use]
    pub fn with_integrity(mut self, integrity: Arc<dyn IntegrityService>) -> Self {
        self.integrity = Some(integrity);
        self
    }

    /// Supply the people service, for the faces face recovery would act on.
    #[must_use]
    pub fn with_people(mut self, people: Arc<dyn PeopleService>) -> Self {
        self.people = Some(people);
        self
    }

    /// Supply the story service, for the scene. Invariant 7.
    #[must_use]
    pub fn with_story(mut self, story: Arc<dyn StoryService>) -> Self {
        self.story = Some(story);
        self
    }

    /// Supply phase 06's recogniser, for the identity constraint.
    ///
    /// **Without it no face is recovered anywhere in the pass.** A guarantee that cannot be
    /// measured is a guarantee that cannot be kept, so the absence of a probe is a refusal rather
    /// than a permission, and every face records
    /// [`RestoreCode::RecoveryHeadUntrained`].
    #[must_use]
    pub fn with_identity_probe(mut self, probe: Arc<dyn IdentityProbe>) -> Self {
        self.probe = Some(probe);
        self
    }

    /// Supply the regions phase 18 generated.
    ///
    /// **The input port, and the only one.** Taken as a map rather than read per frame, because
    /// this crate has no dependency on phase 18 and must not acquire one. An empty map - the
    /// state of this build - refuses every deconvolution and the plans say
    /// [`RestoreCode::SharpenNoRegions`].
    #[must_use]
    pub fn with_regions(mut self, regions: BTreeMap<PhotoId, Vec<RestoreField>>) -> Self {
        self.regions = regions;
        self
    }

    /// Tell the pass how large the delivery is, for the output-size modifier.
    #[must_use]
    pub const fn with_output_long_edge(mut self, long_edge: u32) -> Self {
        self.output_long_edge = long_edge;
        self
    }

    /// Switch the whole stage off.
    ///
    /// Hard rule 8's kill switch. A disabled pass still writes a plan per frame - one that does
    /// nothing - because a frame with no plan and a frame the studio switched off look identical
    /// in a coverage report.
    #[must_use]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// The analyser underneath, for the gate.
    #[must_use]
    pub const fn analyser(&self) -> &Analyser {
        &self.analyser
    }

    /// Plan everything in a project that is not planned at these versions.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the pending set cannot be read. Per-photograph failures are counted in
    /// the report rather than returned: one unreadable frame must not end a pass.
    pub fn run(
        &self,
        project: &ProjectId,
        when: RestoreWhen,
        prio: Priority,
        cancel: &CancelToken,
        progress: &dyn ProgressSink,
    ) -> AuraResult<RestorePassReport> {
        let pending = self.store.pending(project, self.analyser.versions())?;
        let report = self.run_ids(project, &pending, when, prio, cancel, progress)?;
        Self::emit(&report);
        Ok(report)
    }

    /// Plan a specific list of photographs.
    ///
    /// **The path the job graph uses**, with phase 12's keepers. Invariant 3.
    ///
    /// # Errors
    ///
    /// As [`RestorePass::run`].
    #[allow(clippy::too_many_lines)]
    pub fn run_ids(
        &self,
        project: &ProjectId,
        ids: &[PhotoId],
        when: RestoreWhen,
        prio: Priority,
        cancel: &CancelToken,
        progress: &dyn ProgressSink,
    ) -> AuraResult<RestorePassReport> {
        let started = self.clock.monotonic_ms();
        let mut report = RestorePassReport::default();
        let total = ids.len() as u64;
        let preview_priority = match prio {
            Priority::Visible => PreviewPriority::Visible,
            Priority::Interactive => PreviewPriority::Interactive,
            Priority::AiBatch => PreviewPriority::AiBatch,
            Priority::Background => PreviewPriority::Background,
        };
        let during_export = when == RestoreWhen::Export;
        let mut cameras: Vec<String> = Vec::new();

        for (position, id) in ids.iter().enumerate() {
            if cancel.is_cancelled() {
                report.cancelled = true;
                break;
            }
            if let Some(window) =
                ids.get(position + 1..(position + 1 + Self::PREFETCH_WINDOW).min(ids.len()))
            {
                self.previews.prefetch(window, level());
            }

            let buffer = match self.previews.get(*id, level(), preview_priority) {
                Ok(buffer) => buffer,
                Err(err) => {
                    let coded = errors::restore_failed(&id.to_db(), &err.detail);
                    tracing::warn!(
                        target: "restore.pass",
                        photo = %id,
                        code = %coded.code,
                        "{}", coded.detail
                    );
                    report.failed += 1;
                    continue;
                }
            };

            // The kill switch writes a plan that does nothing rather than no plan. See
            // `RestorePass::enabled`.
            if !self.enabled {
                let plan = RestorePlan::nothing(
                    *id,
                    self.scene_of(*id),
                    RestoreReason::plain(RestoreCode::ScheduledOffInteractive, 0.0),
                );
                if self.store.put(project, &plan).is_ok() {
                    report.planned += 1;
                    if let Some(slot) = report.tiers.first_mut() {
                        *slot += 1;
                    }
                }
                continue;
            }

            let Some(frame) = self.frame_for(*id, &buffer) else {
                let coded = errors::restore_failed(
                    &id.to_db(),
                    "the proxy carries tiles, which this pass never asks for",
                );
                tracing::warn!(
                    target: "restore.pass",
                    photo = %id,
                    code = %coded.code,
                    "{}", coded.detail
                );
                report.failed += 1;
                continue;
            };

            let probe = self.probe.as_deref();
            let (plan, outcome) = match self.analyser.plan(&frame, probe, during_export) {
                Ok(result) => result,
                Err(err) => {
                    tracing::warn!(
                        target: "restore.pass",
                        photo = %id,
                        code = %err.code,
                        "{}", err.detail
                    );
                    report.failed += 1;
                    continue;
                }
            };
            if let Err(err) = self.store.put(project, &plan) {
                tracing::warn!(
                    target: "restore.pass",
                    photo = %id,
                    code = %err.code,
                    "{}", err.detail
                );
                report.failed += 1;
                continue;
            }

            report.planned += 1;
            if !plan.is_noop() {
                report.acted_on += 1;
            }
            if outcome.region_covered {
                report.region_covered += 1;
            }
            if let Some(slot) = report.tiers.get_mut(plan.denoise.rank() as usize) {
                *slot += 1;
            }
            if plan.sharpen.is_some() {
                report.sharpened += 1;
            }
            report.faces_recovered += plan.faces_recovered();
            report.identity_refusals += plan.faces_skipped_for_identity();
            if outcome.reduced {
                report.reduced += 1;
            }
            if plan.needs_review() {
                report.low_confidence += 1;
            }
            if outcome.unmeasured_camera
                && plan.denoise != DenoiseTier::Off
                && !cameras.contains(&outcome.camera)
            {
                cameras.push(outcome.camera.clone());
            }

            // The two warnings that describe the product working, raised where they happen so a
            // photographer can find them rather than inferring them from a histogram.
            if plan.faces_skipped_for_identity() > 0 {
                let worst = plan
                    .recovered
                    .iter()
                    .filter(|face| face.skipped_because == Some(RestoreCode::IdentityDriftSkipped))
                    .map(|face| face.identity_drift)
                    .fold(0.0_f32, f32::max);
                let coded = errors::identity_declined(
                    &id.to_db(),
                    worst,
                    aura_core::contract::restore::MAX_IDENTITY_DRIFT,
                    plan.recovered.iter().map(|f| f.resolves).max().unwrap_or(0),
                );
                tracing::info!(
                    target: IDENTITY_STAGE,
                    photo = %id,
                    code = %coded.code,
                    "{}", coded.detail
                );
            }
            if outcome.reduced {
                let report_ref = plan
                    .selfcheck
                    .unwrap_or(aura_core::contract::restore::ArtefactReport::UNTOUCHED);
                let (what, measured, bound) = if report_ref.denoise_reduced {
                    (
                        "texture retention",
                        report_ref.texture_retention,
                        aura_core::contract::restore::MIN_TEXTURE_RETENTION,
                    )
                } else {
                    (
                        "ringing",
                        report_ref.ringing,
                        aura_core::contract::restore::MAX_RINGING,
                    )
                };
                let coded = errors::selfcheck_reduced(
                    &id.to_db(),
                    what,
                    measured,
                    bound,
                    plan.sharpen.is_none() && report_ref.sharpen_reduced,
                );
                tracing::info!(
                    target: SELFCHECK_STAGE,
                    photo = %id,
                    code = %coded.code,
                    "{}", coded.detail
                );
            }

            progress.report(ProgressUpdate {
                stage: STAGE,
                done: position as u64 + 1,
                total,
                current: None,
            });
        }

        cameras.sort();
        report.unmeasured_cameras = cameras;
        report.unlisted_scenes = self.analyser.profiles().unlisted().to_vec();
        report.elapsed_ms = self.clock.monotonic_ms().saturating_sub(started);
        Ok(report)
    }

    /// Everything one frame needs, from the services that are attached.
    fn frame_for(&self, id: PhotoId, buffer: &PixelBuffer) -> Option<RestoreFrame> {
        let (pixels, width, height) = to_linear(buffer)?;
        let scene = self.scene_of(id);
        let verdict = self
            .integrity
            .as_ref()
            .and_then(|integrity| integrity.of_image(id).ok())
            .flatten();
        let subjects = self
            .people
            .as_ref()
            .and_then(|people| people.subjects(id).ok());

        // The faces, each with **its own** measured sharpness rather than the frame's. Section
        // 6.3's band is about how soft one face is, and phase 06 measures exactly that -
        // `FaceRef::sharpness` is documented as "not the frame's sharpness: a frame can be tack
        // sharp on the cake and soft on the bride". Using the frame's number here would put a
        // sharp face in a soft frame into the band and a soft face in a sharp frame outside it,
        // which is the wrong answer in both directions.
        let faces: Vec<FaceCandidate> = subjects
            .as_ref()
            .map(|s| {
                s.faces
                    .iter()
                    .map(|face| FaceCandidate {
                        identity: face.identity_id,
                        bounds: Box2 {
                            x: face.bbox.x,
                            y: face.bbox.y,
                            w: face.bbox.w,
                            h: face.bbox.h,
                        },
                        sharpness: face.sharpness,
                    })
                    .collect()
            })
            .unwrap_or_default();

        let (make, model) = self.camera_of(buffer);
        Some(RestoreFrame {
            image_id: id,
            pixels,
            width,
            height,
            scene,
            make,
            model,
            iso: iso_of(buffer),
            noise_sigma_rel: verdict.as_ref().map(|v| v.noise_sigma_rel),
            motion: verdict.as_ref().map_or(MotionKind::None, |v| v.motion),
            motion_severity: verdict.as_ref().map_or(0.0, |v| v.motion_severity),
            focus_offset: verdict.as_ref().map_or(0.0, |v| v.focus_offset),
            prominence: subjects.as_ref().map_or(0.0, |s| {
                s.faces
                    .iter()
                    .map(|face| face.area_frac)
                    .fold(0.0_f32, f32::max)
            }),
            output_long_edge: self.output_long_edge,
            regions: self.regions.get(&id).cloned().unwrap_or_default(),
            faces,
        })
    }

    /// This frame's scene, or the neutral one.
    fn scene_of(&self, id: PhotoId) -> SceneId {
        self.story
            .as_ref()
            .and_then(|story| story.scene(id).ok())
            .flatten()
            .map_or(SceneId::Unknown, |result| result.scene)
    }

    /// The camera this frame came from.
    ///
    /// The proxy carries no EXIF, so this is the reference model on every frame in this build
    /// until a caller threads the camera through. It is a method rather than a constant so that
    /// the place to thread it is obvious.
    #[allow(clippy::unused_self)]
    fn camera_of(&self, _buffer: &PixelBuffer) -> (String, String) {
        (String::new(), String::new())
    }

    /// Emit one pass's telemetry.
    fn emit(report: &RestorePassReport) {
        tracing::info!(
            target: STAGE,
            planned = report.planned,
            acted_on = report.acted_on,
            sharpened = report.sharpened,
            ms = report.elapsed_ms,
            "restoration pass finished"
        );
        tracing::info!(
            target: SELFCHECK_STAGE,
            reduced_count = report.reduced,
            "restoration self-check"
        );
        tracing::info!(
            target: IDENTITY_STAGE,
            skipped_count = report.identity_refusals,
            faces_recovered = report.faces_recovered,
            "restoration identity guard"
        );
    }
}

/// The render level this pass reads.
#[must_use]
pub fn level() -> aura_raw::contract::pixels::PixelLevel {
    aura_raw::contract::pixels::PixelLevel::Proxy2048
}

/// The frame's ISO, from the buffer.
///
/// The proxy carries no EXIF, so this is the base ISO on every frame in this build. It is a
/// function rather than a literal at the call site for the reason [`RestorePass::camera_of`] is a
/// method: the place a caller threads the real value through should be obvious.
#[must_use]
pub const fn iso_of(_buffer: &PixelBuffer) -> u32 {
    100
}

/// Read a proxy buffer as linear RGB.
///
/// `None` when the buffer carries tiles, which this pass never asks for: tiling is an export path
/// and this is a decision path.
#[must_use]
pub fn to_linear(buffer: &PixelBuffer) -> Option<(Vec<f32>, usize, usize)> {
    let width = buffer.width as usize;
    let height = buffer.height as usize;
    if width == 0 || height == 0 {
        return None;
    }
    let mut rgb = Vec::with_capacity(width * height * 3);
    match &buffer.data {
        PixelData::Srgb8(bytes) => {
            for value in bytes.iter().take(width * height * 3) {
                rgb.push(aura_raw::colour::curve::srgb_decode(
                    f32::from(*value) / 255.0,
                ));
            }
        }
        PixelData::Linear16(values) => {
            for value in values.iter().take(width * height * 3) {
                rgb.push(aura_raw::colour::curve::linear_u16_to_scene(*value));
            }
        }
        PixelData::Tiled(_) => return None,
    }
    if rgb.len() < width * height * 3 {
        rgb.resize(width * height * 3, 0.0);
    }
    Some((rgb, width, height))
}

/// The versions this build produces, without loading a catalog.
///
/// # Errors
///
/// `AURA-ML-5105` when the embedded tables will not load.
pub fn versions() -> Result<(u16, u16, u16), AuraError> {
    let _ = (MODEL_VER, ANALYSIS_VER, RESTORE_LEVEL);
    Restore::current_versions()
}
