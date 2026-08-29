//! The frozen `GeometryService`, and the pass that fills it.
//!
//! Two halves, the shape phases 06 to 22 all settled on. [`Geometry`] answers questions about
//! plans that already exist and is what phases 27, 29 and 30 hold. [`GeometryPass`] walks a
//! project and produces them, and is what the job graph holds.
//!
//! ## Resumability, invariant 5
//!
//! **The work remaining is a query** - [`crate::store::GeometryStore::pending`] - rather than a
//! journal. Kill the process at 10 %, 50 % or 90 % and the next run asks the catalog which
//! photographs have no plan at these two versions. A `profile_ver` bump therefore heals itself,
//! and a hand-framed photograph is never in the answer.
//!
//! ## Three-tier compute, invariant 3
//!
//! The *decision* is made on a 2048 px proxy like every phase since 06. The *resample* runs at
//! full resolution, at export, on the frames being delivered - which is why section 11's
//! resampling budget is written about a 45 MP frame and this pass's budget is written about a
//! proxy.
//!
//! ## The two input ports nothing fills on this build
//!
//! `ProtectedContent::Hands` and `ProtectedContent::JoinedHands` come from phase 11's keypoints,
//! whose head is a placeholder. `ProtectedContent::MomentKey` would come from a region describing
//! what phase 08 says a moment is *of*, and phase 08 records a key **frame** rather than a key
//! region - so there is nothing to read. Both are gaps in what is protected rather than
//! permissions, and the mitigation is structural rather than promised: the scenes where hands and
//! ritual objects matter most - `rings`, `ceremony`, `ritual`, `kiss`, `family_portrait` - are the
//! scenes `crop_rules.toml` switches automatic cropping off for entirely.
//!
//! ## No silent failure, invariant 9
//!
//! A frame whose proxy will not decode is counted, coded `AURA-ML-5110` and reported. The run
//! continues and **no row is written**, so the next pass tries again - a written-but-empty plan
//! would read to phases 27, 29 and 30 as "AURA decided this photograph needed no framing work",
//! which is a different and much worse statement than "AURA has not looked at this photograph
//! yet".

use std::fmt;
use std::sync::Arc;

use aura_core::clock::Clock;
use aura_core::contract::composition::{Box2, CompositionService, HorizonSource};
use aura_core::contract::geometry::{
    CropVariant, GeometryCode, GeometryOutline, GeometryOverride, GeometryPlan, GeometryService,
    ImageId, ProtectedContent, ProtectedRegion,
};
use aura_core::contract::people::PeopleService;
use aura_core::contract::scene::StoryService;
use aura_core::progress::{CancelToken, ProgressSink, ProgressUpdate};
use aura_core::{AuraError, AuraResult, PhotoId, Priority, ProjectId, SceneId};
use aura_preview::contract::service::{PreviewService, Priority as PreviewPriority};
use aura_raw::contract::pixels::{PixelBuffer, PixelData};

use crate::decide::{Analyser, GeometryFrame, REVIEW_BELOW};
use crate::errors;
use crate::profiles::LensExif;
use crate::store::GeometryStore;
use crate::straighten::Horizon;

/// The telemetry stage name, matching section 11's `geometry.applied` event.
pub const STAGE: &str = "geometry.applied";

/// The crop refusal telemetry stage, matching section 11's `geometry.crop_refused` event.
pub const REFUSAL_STAGE: &str = "geometry.crop_refused";

/// The missing-profile telemetry stage, matching section 11's `geometry.lens_profile_missing`.
pub const LENS_STAGE: &str = "geometry.lens_profile_missing";

/// What one pass over a project did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GeometryPassReport {
    /// Photographs planned.
    pub planned: usize,
    /// Photographs that could not be planned, each logged with a code.
    pub failed: usize,
    /// Photographs where at least one pixel moves.
    pub acted_on: usize,
    /// Photographs delivered at the framing they were shot at.
    ///
    /// **Section 10.1's conservatism gate**, as a counter: at least seventy per cent of a wedding.
    pub kept_original: usize,
    /// Frames that were straightened.
    pub straightened: usize,
    /// Frames that got a keystone correction.
    pub keystoned: usize,
    /// Safe aspect variants generated across the pass.
    pub variants: usize,
    /// Frames corrected through a lens profile nobody measured.
    ///
    /// **Every corrected frame on this build.** See `assets/lens_profiles/ATTRIBUTION.md`.
    pub reference_profiles: usize,
    /// Frames below the review threshold.
    pub low_confidence: usize,
    /// The lenses nothing could be found for, most frequent first.
    pub lenses_missing: Vec<String>,
    /// Scenes planned against the neutral row.
    pub unlisted_scenes: Vec<String>,
    /// How many crop candidates were refused, by code.
    pub refusals: Vec<(GeometryCode, usize)>,
    /// Milliseconds for the pass.
    pub elapsed_ms: u64,
    /// True when the pass stopped early because it was cancelled.
    pub cancelled: bool,
}

impl GeometryPassReport {
    /// The share of planned frames that kept their original framing, `0..1`.
    ///
    /// One over an empty pass, which is the reading that does not fail a conservatism gate over a
    /// project nobody has run.
    #[must_use]
    pub fn conservatism(&self) -> f32 {
        if self.planned == 0 {
            return 1.0;
        }
        (self.kept_original as f64 / self.planned as f64) as f32
    }
}

/// The frozen `GeometryService` over one catalog.
#[derive(Debug)]
pub struct Geometry {
    store: Arc<GeometryStore>,
}

impl Geometry {
    /// Wrap a store.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5112` when the embedded tables will not load. Checked here rather than lazily,
    /// because a service that constructed successfully and then refused every question is a
    /// service whose failure a caller finds out about one photograph at a time.
    pub fn new(store: Arc<GeometryStore>) -> Result<Self, AuraError> {
        let _ = Analyser::embedded()?;
        Ok(Self { store })
    }

    /// The store underneath, for the gate.
    #[must_use]
    pub fn store(&self) -> &Arc<GeometryStore> {
        &self.store
    }
}

impl GeometryService for Geometry {
    fn outline(&self, project: ProjectId) -> AuraResult<GeometryOutline> {
        self.store.outline(project)
    }

    fn of_image(&self, image: ImageId) -> AuraResult<Option<GeometryPlan>> {
        self.store.get(image)
    }

    fn review_queue(&self, project: ProjectId, limit: usize) -> AuraResult<Vec<ImageId>> {
        self.store.review_queue(project, limit)
    }

    fn variants(&self, image: ImageId) -> AuraResult<Vec<CropVariant>> {
        // Every safe variant, including the delivered one. Phase 29 lays out albums from this and
        // wants the whole set to choose between; filtering the delivered rectangle out here would
        // make "use the frame as delivered" the one layout it could not express.
        Ok(self
            .store
            .crops_of(image)?
            .into_iter()
            .filter(|variant| variant.safe)
            .collect())
    }

    fn accept(&self, image: ImageId) -> Result<GeometryPlan, AuraError> {
        self.store.accept(image)
    }

    fn set_override(
        &self,
        image: ImageId,
        change: &GeometryOverride,
    ) -> Result<GeometryPlan, AuraError> {
        self.store.set_override(image, change)
    }
}

/// The resumable project walk.
pub struct GeometryPass {
    analyser: Analyser,
    store: Arc<GeometryStore>,
    previews: Arc<dyn PreviewService>,
    clock: Arc<dyn Clock>,
    people: Option<Arc<dyn PeopleService>>,
    story: Option<Arc<dyn StoryService>>,
    composition: Option<Arc<dyn CompositionService>>,
    enabled: bool,
}

impl fmt::Debug for GeometryPass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GeometryPass")
            .field("enabled", &self.enabled)
            .field("has_people", &self.people.is_some())
            .field("has_story", &self.story.is_some())
            .field("has_composition", &self.composition.is_some())
            .finish()
    }
}

impl GeometryPass {
    /// How many proxies ahead of the cursor the pass asks phase 02 to warm.
    ///
    /// Four, matching the passes since phase 09. The decision is a few milliseconds and the decode
    /// is most of the wall clock, so the window only has to cover one decode's worth of lookahead.
    pub const PREFETCH_WINDOW: usize = 4;

    /// A pass over one catalog.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5112` when the embedded tables will not load.
    pub fn new(
        store: Arc<GeometryStore>,
        previews: Arc<dyn PreviewService>,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, AuraError> {
        Ok(Self {
            analyser: Analyser::embedded()?,
            store,
            previews,
            clock,
            people: None,
            story: None,
            composition: None,
            enabled: true,
        })
    }

    /// Supply phase 06's faces.
    ///
    /// **Without it nothing is protected anywhere in the pass**, `CropSafetyReport::considered` is
    /// zero on every frame and the safety gate is arithmetic rather than evidence. It is a
    /// builder rather than a constructor argument for the reason every port since phase 19 has
    /// been one: the absence has to be visible in the report rather than assumed by the caller.
    #[must_use]
    pub fn with_people(mut self, people: Arc<dyn PeopleService>) -> Self {
        self.people = Some(people);
        self
    }

    /// Supply phase 07's scenes, for invariant 7.
    ///
    /// Without it every frame is planned under the neutral row, which forbids automatic cropping
    /// entirely - the conservative outcome, and the report says how many frames it happened to.
    #[must_use]
    pub fn with_story(mut self, story: Arc<dyn StoryService>) -> Self {
        self.story = Some(story);
        self
    }

    /// Supply phase 11's horizon and crop hint.
    ///
    /// Without it no frame in the pass is straightened - `Horizon::default` has `present = false`,
    /// which is [`GeometryCode::HorizonAbsent`] - and the crop objective falls back on the frame's
    /// own energy centroid for a subject. Both are refusals rather than guesses.
    #[must_use]
    pub fn with_composition(mut self, composition: Arc<dyn CompositionService>) -> Self {
        self.composition = Some(composition);
        self
    }

    /// Switch the whole stage off.
    ///
    /// A disabled pass still writes a plan per frame - one that does nothing - because a frame
    /// with no plan and a frame the studio switched off look identical in a coverage report.
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
        prio: Priority,
        cancel: &CancelToken,
        progress: &dyn ProgressSink,
    ) -> AuraResult<GeometryPassReport> {
        let pending = self.store.pending(project, self.analyser.versions())?;
        let report = self.run_ids(project, &pending, prio, cancel, progress)?;
        Self::emit(&report);
        Ok(report)
    }

    /// Plan a specific list of photographs.
    ///
    /// **The path the job graph uses**, with phase 12's keepers. Invariant 3: geometry is decided
    /// for the frames that are being delivered rather than for every frame in the wedding.
    ///
    /// # Errors
    ///
    /// As [`GeometryPass::run`].
    #[allow(clippy::too_many_lines)]
    pub fn run_ids(
        &self,
        project: &ProjectId,
        ids: &[PhotoId],
        prio: Priority,
        cancel: &CancelToken,
        progress: &dyn ProgressSink,
    ) -> AuraResult<GeometryPassReport> {
        let started = self.clock.monotonic_ms();
        let mut report = GeometryPassReport::default();
        let total = ids.len() as u64;
        let preview_priority = match prio {
            Priority::Visible => PreviewPriority::Visible,
            Priority::Interactive => PreviewPriority::Interactive,
            Priority::AiBatch => PreviewPriority::AiBatch,
            Priority::Background => PreviewPriority::Background,
        };

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
                    let coded = errors::geometry_failed(&id.to_db(), &err.detail);
                    tracing::warn!(
                        target: "geometry.pass",
                        photo = %id,
                        code = %coded.code,
                        "{}", coded.detail
                    );
                    report.failed += 1;
                    continue;
                }
            };

            if !self.enabled {
                let mut plan = GeometryPlan::untouched(*id, self.scene_of(*id));
                let (analysis, profile) = self.analyser.versions();
                plan.analysis_ver = analysis;
                plan.profile_ver = profile;
                if self.store.put(project, &plan).is_ok() {
                    report.planned += 1;
                    report.kept_original += 1;
                }
                continue;
            }

            let Some(frame) = self.frame_for(*id, &buffer) else {
                let coded = errors::geometry_failed(
                    &id.to_db(),
                    "the proxy carries tiles, which this pass never asks for",
                );
                tracing::warn!(
                    target: "geometry.pass",
                    photo = %id,
                    code = %coded.code,
                    "{}", coded.detail
                );
                report.failed += 1;
                continue;
            };

            let (plan, outcome) = match self.analyser.plan(&frame) {
                Ok(result) => result,
                Err(err) => {
                    tracing::warn!(
                        target: "geometry.pass",
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
                    target: "geometry.pass",
                    photo = %id,
                    code = %err.code,
                    "{}", err.detail
                );
                report.failed += 1;
                continue;
            }

            report.planned += 1;
            if outcome.acted {
                report.acted_on += 1;
            }
            if outcome.kept_original {
                report.kept_original += 1;
            }
            if plan.rotate_deg.abs() > f32::EPSILON {
                report.straightened += 1;
            }
            if plan.keystone.is_some() {
                report.keystoned += 1;
            }
            report.variants += outcome.variants;
            if outcome.reference_profile {
                report.reference_profiles += 1;
            }
            if plan.confidence < REVIEW_BELOW {
                report.low_confidence += 1;
            }
            if let Some(lens) = outcome.lens_missing {
                if !report.lenses_missing.contains(&lens) {
                    let coded = errors::crop_refused(&id.to_db(), "lens", &lens);
                    tracing::info!(
                        target: LENS_STAGE,
                        photo = %id,
                        lens = %lens,
                        code = %coded.code,
                        "no lens profile for {lens}"
                    );
                    report.lenses_missing.push(lens);
                }
            }
            if outcome.unlisted_scene {
                let name = plan.scene.as_str().to_string();
                if !report.unlisted_scenes.contains(&name) {
                    report.unlisted_scenes.push(name);
                }
            }
            for code in outcome.refusals {
                match report.refusals.iter_mut().find(|(seen, _)| *seen == code) {
                    Some((_, count)) => *count += 1,
                    None => report.refusals.push((code, 1)),
                }
            }

            // The two warnings that describe the product working, raised where they happen so a
            // photographer can find them rather than inferring them from a histogram.
            if let Some(code) = plan
                .reasons
                .iter()
                .map(|reason| reason.code)
                .find(|code| code.is_safety_refusal())
            {
                let coded = errors::crop_refused(&id.to_db(), "original", code.as_str());
                tracing::info!(
                    target: REFUSAL_STAGE,
                    photo = %id,
                    code = %coded.code,
                    "{}", coded.detail
                );
            }
            if matches!(
                plan.reasons.iter().map(|r| r.code).find(|code| matches!(
                    code,
                    GeometryCode::RotationReduced | GeometryCode::RotationRefused
                )),
                Some(_)
            ) {
                let coded = errors::straighten_refused(
                    &id.to_db(),
                    frame.horizon.tilt_deg,
                    plan.rotate_deg,
                    if plan.rotate_deg.abs() > f32::EPSILON {
                        "the full angle would have cropped into somebody"
                    } else {
                        "no angle kept everything in frame"
                    },
                );
                tracing::info!(
                    target: STAGE,
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

        report.elapsed_ms = self.clock.monotonic_ms().saturating_sub(started);
        Ok(report)
    }

    /// Section 11's telemetry, emitted once per pass.
    fn emit(report: &GeometryPassReport) {
        tracing::info!(
            target: STAGE,
            rotate_count = report.straightened,
            keystone_count = report.keystoned,
            crop_count = report.planned - report.kept_original,
            kept_original = report.kept_original,
            conservatism = report.conservatism(),
            reference_profiles = report.reference_profiles,
            "geometry pass finished"
        );
        for (code, count) in &report.refusals {
            tracing::info!(target: REFUSAL_STAGE, code = %code, count = count, "crop refused");
        }
        for lens in &report.lenses_missing {
            tracing::info!(target: LENS_STAGE, lens_id = %lens, "no lens profile");
        }
    }

    /// The scene a photograph was classified as, or `Unknown`.
    fn scene_of(&self, id: PhotoId) -> SceneId {
        self.story
            .as_ref()
            .and_then(|story| story.scene(id).ok())
            .flatten()
            .map_or(SceneId::Unknown, |result| result.scene)
    }

    /// Assemble one frame from the proxy and the frozen services.
    fn frame_for(&self, id: PhotoId, buffer: &PixelBuffer) -> Option<GeometryFrame> {
        let (rgb, width, height) = to_linear(buffer)?;
        let scene = self.scene_of(id);

        // Phase 06's faces, as protected regions. The primary identities are the ones phase 06
        // resolved to somebody; everybody else is a `Face`, which is protected just as hard - the
        // distinction is which sentence the panel says, not which rule applies.
        let protected: Vec<ProtectedRegion> = self
            .people
            .as_ref()
            .and_then(|people| people.subjects(id).ok())
            .map(|subjects| {
                subjects
                    .faces
                    .iter()
                    .map(|face| ProtectedRegion {
                        kind: if face.identity_id.is_some() {
                            ProtectedContent::PrimaryFace
                        } else {
                            ProtectedContent::Face
                        },
                        area: Box2 {
                            x: face.bbox.x,
                            y: face.bbox.y,
                            w: face.bbox.w,
                            h: face.bbox.h,
                        },
                        identity: face.identity_id,
                    })
                    .collect()
            })
            .unwrap_or_default();

        let composition = self
            .composition
            .as_ref()
            .and_then(|service| service.of_image(id).ok())
            .flatten();
        let horizon = composition.as_ref().map_or(Horizon::default(), |result| Horizon {
            tilt_deg: result.tilt_deg,
            confidence: result.horizon_conf,
            intentional: result.tilt_intentional,
            // A horizon phase 11 could not find is one this phase must not act on, and
            // `HorizonSource::None` is how phase 11 says so. Reading `tilt_deg` without this
            // check would turn "nobody measured it" into "it is exactly level".
            present: result.horizon_source != HorizonSource::None,
        });
        let hint = composition
            .as_ref()
            .and_then(|result| result.crop_suggestion_hint)
            .map(|hint| hint.region);

        // The proxy's own dimensions rather than the original's, and that is exact enough for
        // the one thing they are used for: `rotation_crop` depends on the frame's aspect RATIO and
        // on nothing else, and phase 02's pyramid preserves the ratio to within a pixel of
        // rounding. A field named for the full size, filled from the proxy, would be a lie in a
        // struct; the field is documented as the size the geometry is reasoned about at.
        let (full_width, full_height) = (buffer.width.max(1), buffer.height.max(1));

        Some(GeometryFrame {
            image_id: id,
            scene,
            rgb,
            width,
            height,
            full_width,
            full_height,
            // The proxy carries no EXIF, so on this build every frame reaches the lens resolver
            // with an empty name and no focal length - which is `LensProfileMissing` and then the
            // estimator. It is a function of the buffer rather than a literal at the call site so
            // that the place a caller threads the real value through is obvious.
            lens: lens_exif(buffer),
            horizon,
            protected,
            hint,
            user_edited: false,
        })
    }
}

/// The preview level this pass decides on.
#[must_use]
pub fn level() -> aura_raw::contract::pixels::PixelLevel {
    aura_raw::contract::pixels::PixelLevel::Proxy2048
}

/// What a proxy says about the lens it was shot on.
///
/// **Nothing, on this build.** A phase 02 proxy carries pixels and dimensions rather than EXIF, so
/// every frame reaches [`crate::profiles::resolve_lens`] unnamed and falls through to the
/// estimator. Written as a function so that the one place a real EXIF block would be threaded in
/// is named rather than implied - and so that the day it is, no call site changes.
#[must_use]
pub fn lens_exif(buffer: &PixelBuffer) -> LensExif {
    let _ = buffer;
    LensExif::default()
}

/// The proxy as interleaved linear RGB.
///
/// `None` for a tiled buffer, which this pass never asks for: a tiled proxy is what the export
/// path streams and the decision path always takes the whole 2048 px frame, because every
/// measurement in this phase is a property of the frame rather than of a region of it.
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

/// The two versions this build produces, without loading a catalog.
///
/// # Errors
///
/// `AURA-ML-5112` when the embedded tables will not load.
pub fn versions() -> Result<(u16, u16), AuraError> {
    Ok(Analyser::embedded()?.versions())
}
