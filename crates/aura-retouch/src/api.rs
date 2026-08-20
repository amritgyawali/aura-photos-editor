//! The frozen `RetouchService`, and the pass that fills it.
//!
//! Two halves, the shape phases 06 to 19 all settled on. [`Retouch`] answers questions about
//! plans that already exist and is what every later phase holds. [`RetouchPass`] walks a
//! project and produces them, and is what the job graph holds.
//!
//! ## Resumability, invariant 5
//!
//! **The work remaining is a query** - [`crate::store::RetouchStore::pending`] - rather than a
//! journal. Kill the process at 10 %, 50 % or 90 % and the next run asks the catalog which
//! photographs have no plan at these three versions. A `preset_ver` bump therefore heals
//! itself.
//!
//! ## Three-tier compute, invariant 3
//!
//! Section 11 third budget is written about **a thousand selected images at export**, so this
//! pass is not an import-time pass. [`RetouchPass::run`] accepts a whole project because the
//! gate and the fixtures need it, and [`RetouchPass::run_ids`] is the path the job graph uses,
//! with phase 12 keepers.
//!
//! ## The two passes, and why permanence needs the second
//!
//! Section 6.1 cross-frame evidence cannot be decided one frame at a time: a mark is permanent
//! because it was in the same place on the same face across hours. So [`RetouchPass::run_ids`]
//! collects observations while it plans, and [`RetouchPass::settle_protected`] turns a project
//! worth of them into protect rows afterwards. A second pass over the planned frames then picks
//! the new rows up, and the store keeps a photographer own rows out of both.
//!
//! ## No silent failure, invariant 9
//!
//! A frame whose proxy will not decode is counted, coded `AURA-ML-5092` and reported. The run
//! continues and **no row is written**, so the next pass tries again - a written-but-empty plan
//! would read to phases 21, 25 and 27 as "AURA decided this skin needed nothing".

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use aura_core::clock::Clock;
use aura_core::contract::local::MaskField;
use aura_core::contract::people::{PeopleService, Role};
use aura_core::contract::retouch::{
    ImageId, ProtectedFeature, RetouchCode, RetouchOutline, RetouchOverride, RetouchPlan,
    RetouchPreset, RetouchService,
};
use aura_core::contract::scene::StoryService;
use aura_core::progress::{CancelToken, ProgressSink, ProgressUpdate};
use aura_core::{AuraError, AuraResult, IdentityId, MaskId, PhotoId, Priority, ProjectId, SceneId};
use aura_preview::contract::service::{PreviewService, Priority as PreviewPriority};

use crate::errors;
use crate::ops::{Analyser, FrameContext, ANALYSIS_VER, MODEL_VER, RETOUCH_LEVEL};
use crate::permanent::{self, Observation};
use crate::presets::PresetTable;
use crate::store::RetouchStore;
use crate::strength::{self, IdentityStats};

/// The telemetry stage name, matching section 11 `retouch.applied` event.
pub const STAGE: &str = "retouch.applied";

/// The texture telemetry stage, matching section 11 `retouch.texture_guard` event.
pub const TEXTURE_STAGE: &str = "retouch.texture_guard";

/// The protect telemetry stage, matching section 11 `retouch.protected` event.
pub const PROTECTED_STAGE: &str = "retouch.protected";

/// What one pass over a project did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PassReport {
    /// Photographs planned.
    pub planned: usize,
    /// Photographs that could not be planned, each logged with a code.
    pub failed: usize,
    /// Photographs where at least one operation ran.
    pub acted_on: usize,
    /// Photographs where a skin mask arrived.
    pub mask_covered: usize,
    /// Blemishes removed across the pass.
    pub blemishes: usize,
    /// Frames where the texture guard reduced a strength.
    pub texture_resolved: usize,
    /// Frames where it withdrew the retouch entirely.
    pub texture_withdrawn: usize,
    /// Features protected by cross-frame evidence.
    pub protected: usize,
    /// Frames below the review threshold.
    pub low_confidence: usize,
    /// Mean band ratio over frames that were retouched.
    pub mean_band_ratio: f32,
    /// Scenes planned against the neutral row.
    pub unpreset_scenes: Vec<String>,
    /// Milliseconds for the pass.
    pub elapsed_ms: u64,
    /// True when the pass stopped early because it was cancelled.
    pub cancelled: bool,
}

/// The frozen `RetouchService` over one catalog.
#[derive(Debug)]
pub struct Retouch {
    store: Arc<RetouchStore>,
    project: ProjectId,
}

impl Retouch {
    /// Wrap a store.
    ///
    /// The project is carried because two of the frozen methods - the identity strengths and
    /// the override that writes one - are project-scoped, and a service that had to be handed a
    /// project on every call would let a caller mix two weddings in one answer.
    #[must_use]
    pub fn new(store: Arc<RetouchStore>, project: ProjectId) -> Self {
        Self { store, project }
    }

    /// The three versions this build produces.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5093` when the embedded preset table will not load.
    pub fn current_versions() -> Result<(u16, u16, u16), AuraError> {
        Ok((MODEL_VER, ANALYSIS_VER, PresetTable::embedded()?.version()))
    }

    /// The store underneath, for the panel own reads and for the gate.
    #[must_use]
    pub fn store(&self) -> &Arc<RetouchStore> {
        &self.store
    }
}

impl RetouchService for Retouch {
    fn outline(&self, project: ProjectId) -> AuraResult<RetouchOutline> {
        let unpreset = PresetTable::embedded()
            .map(|table| table.unpreset())
            .unwrap_or_default();
        let outline = self.store.outline(&project, unpreset)?;
        // Reported rather than enforced. `AURA-ML-5090` is degraded: stale plans keep working
        // while the background pass replaces them, and a caller about to draw a conclusion over
        // a mixed set finds out before it draws it.
        if let Ok(current) = Self::current_versions() {
            let stored = (outline.model_ver, outline.analysis_ver, outline.preset_ver);
            if outline.planned > 0 && stored != current {
                let stale =
                    errors::retouch_version_mismatch(stored, current, outline.planned as usize);
                tracing::warn!(target: "retouch.version", code = %stale.code, "{}", stale.detail);
            }
        }
        Ok(outline)
    }

    fn of_image(&self, image: ImageId) -> AuraResult<Option<RetouchPlan>> {
        self.store.get(image)
    }

    fn protected(&self, identity: IdentityId) -> AuraResult<Vec<ProtectedFeature>> {
        self.store.protected(identity)
    }

    fn needs_review(&self, project: ProjectId, limit: usize) -> AuraResult<Vec<ImageId>> {
        self.store.needs_review(&project, limit)
    }

    fn identity_strengths(&self, project: ProjectId) -> AuraResult<BTreeMap<IdentityId, f32>> {
        self.store.identity_strengths(&project)
    }

    fn accept(&self, image: ImageId) -> Result<(), AuraError> {
        self.store.accept(image)
    }

    fn set_override(&self, image: ImageId, values: RetouchOverride) -> Result<(), AuraError> {
        self.store.set_override(image, &self.project, &values)
    }

    fn set_protection(&self, feature: ProtectedFeature, protect: bool) -> Result<(), AuraError> {
        self.store.set_protection(&self.project, &feature, protect)
    }
}

/// The resumable project walk.
pub struct RetouchPass {
    previews: Arc<dyn PreviewService>,
    analyser: Analyser,
    store: Arc<RetouchStore>,
    people: Option<Arc<dyn PeopleService>>,
    story: Option<Arc<dyn StoryService>>,
    masks: BTreeMap<PhotoId, (MaskId, MaskField)>,
    evened: BTreeMap<PhotoId, Vec<Option<IdentityId>>>,
    minutes: BTreeMap<PhotoId, f32>,
    preset: RetouchPreset,
    enabled: bool,
    clock: Arc<dyn Clock>,
}

impl fmt::Debug for RetouchPass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RetouchPass")
            .field("preset_ver", &self.analyser.presets().version())
            .field("people", &self.people.is_some())
            .field("story", &self.story.is_some())
            .field("mask_frames", &self.masks.len())
            .field("preset", &self.preset)
            .field("enabled", &self.enabled)
            .finish_non_exhaustive()
    }
}

impl RetouchPass {
    /// How many frames ahead the preview service is asked to decode.
    const PREFETCH_WINDOW: usize = 4;

    /// Assemble a pass.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5093` when the embedded preset table will not load.
    pub fn new(
        previews: Arc<dyn PreviewService>,
        store: Arc<RetouchStore>,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, AuraError> {
        Ok(Self {
            previews,
            analyser: Analyser::new()?,
            store,
            people: None,
            story: None,
            masks: BTreeMap::new(),
            evened: BTreeMap::new(),
            minutes: BTreeMap::new(),
            preset: RetouchPreset::default(),
            enabled: true,
            clock,
        })
    }

    /// Read who is in each frame through phase 06 frozen service.
    ///
    /// Optional, and the degradation is documented rather than silent: without it there are no
    /// faces, so nothing is retouched at all and every plan says so.
    #[must_use]
    pub fn with_people(mut self, people: Arc<dyn PeopleService>) -> Self {
        self.people = Some(people);
        self
    }

    /// Read the scene through phase 07 frozen service.
    ///
    /// Optional. Without it every frame is planned against the neutral preset row, which is
    /// invariant 7 degraded rather than broken - and the neutral row is the most conservative in
    /// the table, so a wedding with no story is retouched very gently.
    #[must_use]
    pub fn with_story(mut self, story: Arc<dyn StoryService>) -> Self {
        self.story = Some(story);
        self
    }

    /// Supply the skin masks phase 18 generated.
    ///
    /// **The input port, and the only one.** Taken as a map rather than read per frame, because
    /// this crate has no dependency on phase 18 and must not acquire one. An empty map - the
    /// state of this build - withdraws every operation and the plans say
    /// [`RetouchCode::MaskUnavailable`].
    #[must_use]
    pub fn with_masks(mut self, masks: BTreeMap<PhotoId, (MaskId, MaskField)>) -> Self {
        self.masks = masks;
        self
    }

    /// Supply the faces phase 19 has already evened.
    ///
    /// Phase 19 wrote the rule - "phase 20 retouches skin this phase has already evened and must
    /// not do it twice" - and `idx_local_evened` is the index it exists for. Without this map
    /// the two phases can even the same face twice, which is a portrait that gets flattened.
    #[must_use]
    pub fn with_evened(mut self, evened: BTreeMap<PhotoId, Vec<Option<IdentityId>>>) -> Self {
        self.evened = evened;
        self
    }

    /// Supply the capture time of each frame, in minutes from the start of the project.
    ///
    /// Needed by the cross-frame permanence test and by nothing else. Absent, every observation
    /// lands at minute zero, the span is zero, and **nothing is ever called permanent** - which
    /// is the conservative direction: a mark nobody protected is a mark that stays on the
    /// photograph, because an unprotected mark still has to pass the temporary floor.
    #[must_use]
    pub fn with_minutes(mut self, minutes: BTreeMap<PhotoId, f32>) -> Self {
        self.minutes = minutes;
        self
    }

    /// Choose the preset for this pass.
    #[must_use]
    pub fn with_preset(mut self, preset: RetouchPreset) -> Self {
        self.preset = preset;
        self
    }

    /// Switch the whole stage off.
    ///
    /// Hard rule 8 kill switch. A disabled pass still writes a plan per frame - one that does
    /// nothing - because a frame with no plan and a frame the photographer switched off look
    /// identical in a coverage report.
    #[must_use]
    pub fn enabled(mut self, enabled: bool) -> Self {
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
    /// `AURA-DB-3006` when the pending set cannot be read. Per-photograph failures are counted
    /// in the report rather than returned: one unreadable frame must not end a pass.
    pub fn run(
        &self,
        project: &ProjectId,
        prio: Priority,
        cancel: &CancelToken,
        progress: &dyn ProgressSink,
    ) -> AuraResult<PassReport> {
        let pending = self.store.pending(
            project,
            (MODEL_VER, ANALYSIS_VER, self.analyser.presets().version()),
        )?;
        let report = self.run_ids(project, &pending, prio, cancel, progress)?;
        Self::emit(&report);
        Ok(report)
    }

    /// Plan a specific list of photographs.
    ///
    /// **The path the job graph uses**, with phase 12 keepers. Invariant 3.
    ///
    /// # Errors
    ///
    /// As [`RetouchPass::run`].
    #[allow(clippy::too_many_lines)]
    pub fn run_ids(
        &self,
        project: &ProjectId,
        ids: &[PhotoId],
        prio: Priority,
        cancel: &CancelToken,
        progress: &dyn ProgressSink,
    ) -> AuraResult<PassReport> {
        let started = self.clock.monotonic_ms();
        let mut report = PassReport::default();
        let total = ids.len() as u64;
        let preview_priority = match prio {
            Priority::Visible => PreviewPriority::Visible,
            Priority::Interactive => PreviewPriority::Interactive,
            Priority::AiBatch => PreviewPriority::AiBatch,
            Priority::Background => PreviewPriority::Background,
        };

        let strengths = self.identity_strengths(project, ids)?;
        let mut unpreset: BTreeSet<String> = BTreeSet::new();
        let mut observations: Vec<Observation> = Vec::new();
        let mut ratio_sum = 0.0f32;
        let mut ratio_frames = 0usize;

        for (position, id) in ids.iter().enumerate() {
            if cancel.is_cancelled() {
                report.cancelled = true;
                break;
            }
            if let Some(window) =
                ids.get(position + 1..(position + 1 + Self::PREFETCH_WINDOW).min(ids.len()))
            {
                self.previews.prefetch(window, RETOUCH_LEVEL);
            }

            let buffer = match self.previews.get(*id, RETOUCH_LEVEL, preview_priority) {
                Ok(buffer) => buffer,
                Err(err) => {
                    let coded = errors::retouch_failed(&id.to_db(), &err.detail);
                    tracing::warn!(
                        target: "retouch.pass",
                        photo = %id,
                        code = %coded.code,
                        "{}", coded.detail
                    );
                    report.failed += 1;
                    continue;
                }
            };

            let context = self.context_for(*id, &strengths);
            let outcome = match self.analyser.analyse(*id, &buffer, &context) {
                Ok(outcome) => outcome,
                Err(err) => {
                    tracing::warn!(
                        target: "retouch.pass",
                        photo = %id,
                        code = %err.code,
                        "{}", err.detail
                    );
                    report.failed += 1;
                    continue;
                }
            };
            for warning in &outcome.warnings {
                tracing::debug!(
                    target: "retouch.pass",
                    photo = %id,
                    code = %warning.code,
                    "{}", warning.detail
                );
            }
            if let Err(err) = crate::guard::check_plan(&outcome.plan) {
                tracing::warn!(
                    target: "retouch.pass",
                    photo = %id,
                    code = %err.code,
                    "{}", err.detail
                );
                report.failed += 1;
                continue;
            }
            if let Err(err) = self.store.put(project, &outcome.plan) {
                tracing::warn!(
                    target: "retouch.pass",
                    photo = %id,
                    code = %err.code,
                    "{}", err.detail
                );
                report.failed += 1;
                continue;
            }

            let minute = self.minutes.get(id).copied().unwrap_or(0.0);
            observations.extend(outcome.observations.into_iter().map(|mut observation| {
                observation.minute = minute;
                observation
            }));

            let plan = &outcome.plan;
            report.planned += 1;
            if !plan.is_noop() {
                report.acted_on += 1;
            }
            report.blemishes += plan.count_of("blemish");
            if plan.texture_report.measured_on > 0 && !plan.is_noop() {
                ratio_sum += plan.texture_report.band_ratio;
                ratio_frames += 1;
            }
            if plan.texture_report.resolves > 0 {
                report.texture_resolved += 1;
            }
            if plan.texture_report.withdrawn {
                report.texture_withdrawn += 1;
            }
            if plan.needs_review() {
                report.low_confidence += 1;
            }
            if !plan
                .reasons
                .iter()
                .any(|reason| reason.code == RetouchCode::MaskUnavailable)
            {
                report.mask_covered += 1;
            }
            if plan
                .reasons
                .iter()
                .any(|reason| reason.code == RetouchCode::SceneLimited)
            {
                unpreset.insert(plan.scene.as_str().to_string());
            }

            progress.report(ProgressUpdate {
                stage: STAGE,
                done: position as u64 + 1,
                total,
                current: None,
            });
        }

        report.protected = self.settle_protected(project, &observations)?;
        report.mean_band_ratio = if ratio_frames == 0 {
            0.0
        } else {
            ratio_sum / ratio_frames as f32
        };
        report.unpreset_scenes = unpreset.into_iter().collect();
        report.elapsed_ms = self.clock.monotonic_ms().saturating_sub(started);
        Ok(report)
    }

    /// Turn a project worth of observations into protect rows.
    ///
    /// Returns how many features were protected. Separate from the planning loop because
    /// permanence is a property of a person across a gallery, and no single frame can decide it.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the write fails.
    pub fn settle_protected(
        &self,
        project: &ProjectId,
        observations: &[Observation],
    ) -> AuraResult<usize> {
        let features = permanent::accumulate(observations);
        if features.is_empty() {
            return Ok(0);
        }
        self.store.replace_protected(project, &features)?;
        Ok(features.len())
    }

    /// The gallery-constant strength for every identity in the frames about to be planned.
    ///
    /// Computed once per pass and stored, which is what makes it a constant: every frame in the
    /// wedding reads the same number, and section 10.1 cross-frame consistency gate is
    /// satisfied by construction. See ADR-0041 section 6.
    fn identity_strengths(
        &self,
        project: &ProjectId,
        ids: &[PhotoId],
    ) -> AuraResult<BTreeMap<IdentityId, f32>> {
        let Some(people) = self.people.as_ref() else {
            return Ok(BTreeMap::new());
        };

        // Roles come from phase 06 hierarchy rather than from a role column, because that
        // is the shape `PeopleService` freezes: the couple are `primary`, close family are
        // `secondary`, and everybody else is a guest until a photographer says otherwise.
        let hierarchy = people.hierarchy(*project).ok();

        let mut sizes: BTreeMap<IdentityId, Vec<f32>> = BTreeMap::new();
        let mut scenes: BTreeMap<IdentityId, BTreeMap<String, u32>> = BTreeMap::new();

        for id in ids {
            let Ok(subjects) = people.subjects(*id) else {
                continue;
            };
            let scene = self.scene_of(*id);
            for face in &subjects.faces {
                let Some(identity) = face.identity_id else {
                    continue;
                };
                sizes
                    .entry(identity)
                    .or_default()
                    .push(face.area_frac.sqrt());
                *scenes
                    .entry(identity)
                    .or_default()
                    .entry(scene.as_str().to_string())
                    .or_default() += 1;
            }
        }

        let mut out = BTreeMap::new();
        let preset_ver = self.analyser.presets().version();
        for (identity, mut values) in sizes {
            values.sort_by(f32::total_cmp);
            let median = values.get(values.len() / 2).copied().unwrap_or(0.0);
            let dominant = scenes
                .get(&identity)
                .and_then(|counts| {
                    counts
                        .iter()
                        .max_by_key(|(name, count)| (**count, (*name).clone()))
                        .map(|(name, _)| SceneId::from_str_or_unknown(name))
                })
                .unwrap_or(SceneId::Unknown);
            let role = hierarchy.as_ref().map_or(Role::Unknown, |h| {
                if h.primary.contains(&identity) {
                    Role::Couple
                } else if h.secondary.contains(&identity) {
                    Role::FamilyClose
                } else {
                    Role::Guest
                }
            });
            let stats = IdentityStats {
                identity,
                role,
                median_face_frac: median,
                dominant_scene: dominant,
                frames: values.len() as u32,
            };
            let assigned = strength::assign(&stats, self.analyser.presets(), self.preset);
            self.store
                .put_identity(project, &stats, assigned, preset_ver)?;
            out.insert(identity, assigned);
        }

        // Whatever a photographer set outranks whatever was just computed, and the store is
        // what settles that: `put_identity` does not overwrite a `user_edited` row, so reading
        // the table back is what makes the override effective everywhere.
        self.store.identity_strengths(project)
    }

    /// Everything known about one frame before its pixels are read.
    fn context_for(&self, id: PhotoId, strengths: &BTreeMap<IdentityId, f32>) -> FrameContext {
        let scene = self.scene_of(id);
        let faces = self
            .people
            .as_ref()
            .and_then(|people| people.subjects(id).ok())
            .map(|subjects| subjects.faces)
            .unwrap_or_default();
        let protected = faces
            .iter()
            .filter_map(|face| face.identity_id)
            .flat_map(|identity| {
                self.store
                    .protected(identity)
                    .unwrap_or_default()
                    .into_iter()
            })
            .collect();

        FrameContext {
            scene,
            faces,
            skin: self.masks.get(&id).cloned(),
            identity_strength: strengths.clone(),
            protected,
            evened_by_local: self.evened.get(&id).cloned().unwrap_or_default(),
            preset: self.preset,
            enabled: self.enabled,
        }
    }

    fn scene_of(&self, id: PhotoId) -> SceneId {
        self.story
            .as_ref()
            .and_then(|story| story.scene(id).ok().flatten())
            .map_or(SceneId::Unknown, |result| result.scene)
    }

    /// Section 11 three telemetry events, emitted once per pass.
    fn emit(report: &PassReport) {
        tracing::info!(
            target: STAGE,
            images = report.planned,
            ops = report.blemishes,
            acted_on = report.acted_on,
            ms = report.elapsed_ms,
            "retouch pass complete"
        );
        tracing::info!(
            target: TEXTURE_STAGE,
            triggered = report.texture_resolved + report.texture_withdrawn,
            withdrawn = report.texture_withdrawn,
            mean_band_ratio = report.mean_band_ratio,
            "texture guard"
        );
        tracing::info!(
            target: PROTECTED_STAGE,
            count = report.protected,
            "features protected"
        );
    }
}
