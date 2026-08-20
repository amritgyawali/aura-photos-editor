//! The frozen `LocalService`, and the pass that fills it.
//!
//! Two halves, the shape phases 06 to 15 all settled on. [`Local`] answers questions about
//! plans that already exist and is what every later phase holds. [`LocalPass`] walks a project
//! and produces them, and is what the job graph holds.
//!
//! ## Resumability, invariant 5
//!
//! **The work remaining is a query** - [`crate::local::store::LocalStore::pending`] - not a
//! journal. Kill the process at 10 %, 50 % or 90 % and the next run asks the catalog which
//! photographs have no plan at these four versions. A `policy_ver` or `shaping_ver` bump
//! therefore heals itself: the rows made under the old table are pending by definition.
//!
//! ## Three-tier compute, invariant 3
//!
//! This pass is the first in `aura-brain-photo` that should not run over a whole wedding.
//! Section 11's third budget is "**1,000 selected images** total <= 90 s", in those words: the
//! subject of local light sculpting is the gallery, not the import. [`LocalPass::run`] still
//! accepts a whole project - the CLI gate and the fixtures need it - but
//! [`LocalPass::run_ids`] is the path the job graph uses, with phase 12's keepers.
//!
//! ## No silent failure, invariant 9
//!
//! A frame whose proxy will not decode is counted, coded `AURA-ML-5086` and reported. The run
//! continues, and **no row is written** - so the next pass tries again. A written-but-empty
//! plan would read to phases 20, 25 and 27 as "AURA decided this photograph needed nothing
//! locally", and all three act on that.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use aura_core::clock::Clock;
use aura_core::contract::composition::CompositionService;
use aura_core::contract::integrity::IntegrityService;
use aura_core::contract::local::{
    ImageId, LocalCode, LocalLightPlan, LocalOp, LocalOutline, LocalOverride, LocalService,
    MaskField,
};
use aura_core::contract::people::PeopleService;
use aura_core::contract::scene::StoryService;
use aura_core::progress::{CancelToken, ProgressSink, ProgressUpdate};
use aura_core::{AuraError, AuraResult, PhotoId, Priority, ProjectId, SceneId};
use aura_preview::contract::service::{PreviewService, Priority as PreviewPriority};

use crate::errors;
use crate::local::plan::{Analyser, FrameContext, ANALYSIS_VER, LOCAL_LEVEL, MODEL_VER};
use crate::local::policy::PolicyTable;
use crate::local::store::LocalStore;
use crate::local::SHAPING_VER;
use crate::tone::targets::TargetTable;

/// The telemetry stage name, matching section 11's `local.applied` event.
pub const STAGE: &str = "local.applied";

/// The gating telemetry stage, matching section 11's `local.gated` event.
pub const GATED_STAGE: &str = "local.gated";

/// The shine telemetry stage, matching section 11's `local.shine_reduced` event.
pub const SHINE_STAGE: &str = "local.shine_reduced";

/// What one pass over a project did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PassReport {
    /// Photographs planned.
    pub planned: usize,
    /// Photographs that could not be planned, each logged with a code.
    pub failed: usize,
    /// Photographs where at least one operation ran.
    pub acted_on: usize,
    /// How many frames each operation ran on, in [`LocalOp::PRIORITY`] order.
    pub op_histogram: [u32; LocalOp::COUNT],
    /// How many operations were gated, over the whole pass.
    pub gated: usize,
    /// Frames where every mask an operation wanted arrived.
    pub fully_masked: usize,
    /// Frames where faces were solved jointly.
    pub group_solved: usize,
    /// Frames where shine was reduced.
    pub shine_reduced: usize,
    /// Frames below the review threshold.
    pub low_confidence: usize,
    /// Mean fraction of the allowance spent, over frames that spent any.
    pub mean_budget_used: f32,
    /// Scenes planned against the neutral row.
    pub unpolicied_scenes: Vec<String>,
    /// Milliseconds for the pass.
    pub elapsed_ms: u64,
    /// True when the pass stopped early because it was cancelled.
    pub cancelled: bool,
}

/// The frozen `LocalService` over one catalog.
#[derive(Debug)]
pub struct Local {
    store: Arc<LocalStore>,
}

impl Local {
    /// Wrap a store.
    #[must_use]
    pub fn new(store: Arc<LocalStore>) -> Self {
        Self { store }
    }

    /// The four versions this build produces.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5087` when the embedded policy table will not load.
    pub fn current_versions() -> Result<(u16, u16, u16, u16), AuraError> {
        Ok((
            MODEL_VER,
            ANALYSIS_VER,
            PolicyTable::embedded()?.version(),
            SHAPING_VER,
        ))
    }

    /// The store underneath, for the panel's own reads and for the gate.
    #[must_use]
    pub fn store(&self) -> &Arc<LocalStore> {
        &self.store
    }
}

impl LocalService for Local {
    fn outline(&self, project: ProjectId) -> AuraResult<LocalOutline> {
        let unpolicied = PolicyTable::embedded()
            .map(|table| table.unpolicied())
            .unwrap_or_default();
        let outline = self.store.outline(&project, unpolicied)?;
        // The version check, reported rather than enforced. `AURA-ML-5084` is degraded: stale
        // plans keep working while the background pass replaces them, and the outline reports
        // what is stored so a caller about to draw a conclusion over a mixed set finds out
        // before it draws it.
        if let Ok(current) = Self::current_versions() {
            let stored = (
                outline.model_ver,
                outline.analysis_ver,
                outline.policy_ver,
                outline.shaping_ver,
            );
            if outline.planned > 0 && stored != current {
                let stale =
                    errors::local_version_mismatch(stored, current, outline.planned as usize);
                tracing::warn!(
                    target: "local.version",
                    code = %stale.code,
                    "{}", stale.detail
                );
            }
        }
        Ok(outline)
    }

    fn of_image(&self, image: ImageId) -> AuraResult<Option<LocalLightPlan>> {
        self.store.get(image)
    }

    fn needs_review(&self, project: ProjectId, limit: usize) -> AuraResult<Vec<ImageId>> {
        self.store.needs_review(&project, limit)
    }

    fn op_counts(&self, project: ProjectId) -> AuraResult<BTreeMap<LocalOp, u32>> {
        self.store.op_counts(&project)
    }

    fn accept(&self, image: ImageId) -> Result<(), AuraError> {
        self.store.accept(image)
    }

    fn set_override(&self, image: ImageId, values: LocalOverride) -> Result<(), AuraError> {
        self.store.set_override(image, values)
    }
}

/// The resumable project walk.
pub struct LocalPass {
    previews: Arc<dyn PreviewService>,
    analyser: Analyser,
    store: Arc<LocalStore>,
    targets: TargetTable,
    people: Option<Arc<dyn PeopleService>>,
    story: Option<Arc<dyn StoryService>>,
    integrity: Option<Arc<dyn IntegrityService>>,
    composition: Option<Arc<dyn CompositionService>>,
    masks: BTreeMap<PhotoId, Vec<MaskField>>,
    enabled: bool,
    clock: Arc<dyn Clock>,
}

impl fmt::Debug for LocalPass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LocalPass")
            .field("policy_ver", &self.analyser.policy().version())
            .field("people", &self.people.is_some())
            .field("story", &self.story.is_some())
            .field("integrity", &self.integrity.is_some())
            .field("composition", &self.composition.is_some())
            .field("mask_frames", &self.masks.len())
            .field("enabled", &self.enabled)
            .finish_non_exhaustive()
    }
}

impl LocalPass {
    /// Assemble a pass.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5087` when the embedded policy table will not load, and `AURA-ML-5063` when
    /// phase 15's exposure target table will not - the bands this phase lights faces toward
    /// are phase 15's, and inventing a substitute for them would be a second answer to what a
    /// well-lit face looks like.
    pub fn new(
        previews: Arc<dyn PreviewService>,
        store: Arc<LocalStore>,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, AuraError> {
        Ok(Self {
            previews,
            analyser: Analyser::new(PolicyTable::embedded()?),
            store,
            targets: TargetTable::embedded()?,
            people: None,
            story: None,
            integrity: None,
            composition: None,
            masks: BTreeMap::new(),
            enabled: true,
            clock,
        })
    }

    /// Read who is in each frame through phase 06's frozen service.
    ///
    /// Optional, and the degradation is documented rather than silent: **every frame becomes
    /// faceless**. Four of the six operations need a face, so a pass with no people service
    /// balances backgrounds and does nothing else.
    #[must_use]
    pub fn with_people(mut self, people: Arc<dyn PeopleService>) -> Self {
        self.people = Some(people);
        self
    }

    /// Read the scene through phase 07's frozen service.
    ///
    /// Optional. Without it every frame is planned against the neutral policy row, which is
    /// invariant 7 degraded rather than broken - and the neutral row switches shaping off, so
    /// a wedding with no story does no dodge and burn at all.
    #[must_use]
    pub fn with_story(mut self, story: Arc<dyn StoryService>) -> Self {
        self.story = Some(story);
        self
    }

    /// Read the noise through phase 09's frozen service.
    ///
    /// Optional, and this one costs the most when it is absent: without it every frame is
    /// treated as clean, so the dynamic noise cap of section 6.1 is the scene's ceiling alone.
    /// A high-ISO reception then gets lifted as far as the policy allows and the grain comes
    /// with it, which is section 12's fourth failure mode.
    #[must_use]
    pub fn with_integrity(mut self, integrity: Arc<dyn IntegrityService>) -> Self {
        self.integrity = Some(integrity);
        self
    }

    /// Read what is behind the subject through phase 11's frozen service.
    ///
    /// Optional. Without it the bright-blob trigger never fires, and the paired operation runs
    /// on the luminance ratio and the chroma energy alone.
    #[must_use]
    pub fn with_composition(mut self, composition: Arc<dyn CompositionService>) -> Self {
        self.composition = Some(composition);
        self
    }

    /// Supply the masks phase 18 generated.
    ///
    /// **The input port, and the only one.** Taken as a map rather than read per frame,
    /// because this crate has no dependency on phase 18 and must not acquire one: a mask is
    /// phase 18's answer and this phase is a consumer of it.
    ///
    /// An empty map - the state of this build - gates every operation and the plans say so.
    #[must_use]
    pub fn with_masks(mut self, masks: BTreeMap<PhotoId, Vec<MaskField>>) -> Self {
        self.masks = masks;
        self
    }

    /// Switch the whole stage off.
    ///
    /// Hard rule 8's kill switch. A disabled pass still writes a plan per frame - one that
    /// does nothing and says [`LocalCode::LocalDisabled`] - because a frame with no plan and a
    /// frame the photographer switched off look identical in a coverage report.
    #[must_use]
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Use a different policy table.
    #[must_use]
    pub fn with_policy(mut self, policy: PolicyTable) -> Self {
        self.analyser = Analyser::new(policy);
        self
    }

    /// Use a different exposure target table.
    #[must_use]
    pub fn with_targets(mut self, targets: TargetTable) -> Self {
        self.targets = targets;
        self
    }

    /// The analyser underneath.
    #[must_use]
    pub const fn analyser(&self) -> &Analyser {
        &self.analyser
    }

    /// Plan every photograph in a project that has no current plan.
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
            (
                MODEL_VER,
                ANALYSIS_VER,
                self.analyser.policy().version(),
                SHAPING_VER,
            ),
        )?;
        let report = self.run_ids(project, &pending, prio, cancel, progress)?;
        Self::emit(&report);
        Ok(report)
    }

    /// Plan a specific list of photographs.
    ///
    /// **The path the job graph uses**, with phase 12's keepers. Invariant 3: expensive work
    /// only on survivors, and this is the phase where that stops being an optimisation and
    /// becomes the design - section 11's third budget is written about a thousand selected
    /// images rather than about a wedding.
    ///
    /// # Errors
    ///
    /// As [`LocalPass::run`].
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
        // built while this one is being planned.
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
        let mut unpolicied: BTreeSet<String> = BTreeSet::new();
        let mut budget_sum = 0.0f32;
        let mut budget_frames = 0usize;

        for (position, id) in ids.iter().enumerate() {
            if cancel.is_cancelled() {
                report.cancelled = true;
                break;
            }
            if let Some(window) =
                ids.get(position + 1..(position + 1 + PREFETCH_WINDOW).min(ids.len()))
            {
                self.previews.prefetch(window, LOCAL_LEVEL);
            }

            let buffer = match self.previews.get(*id, LOCAL_LEVEL, preview_priority) {
                Ok(buffer) => buffer,
                Err(err) => {
                    let coded = errors::local_failed(&id.to_db(), &err.detail);
                    tracing::warn!(
                        target: "local.pass",
                        photo = %id,
                        code = %coded.code,
                        "{}", coded.detail
                    );
                    report.failed += 1;
                    continue;
                }
            };

            let context = self.context_for(*id);
            let outcome = self.analyser.analyse(&buffer, *id, &context);
            for warning in &outcome.warnings {
                tracing::debug!(
                    target: "local.pass",
                    photo = %id,
                    code = %warning.code,
                    "{}", warning.detail
                );
            }
            if let Err(err) = self.store.put(project, &outcome.plan) {
                tracing::warn!(
                    target: "local.pass",
                    photo = %id,
                    code = %err.code,
                    "{}", err.detail
                );
                report.failed += 1;
                continue;
            }

            report.planned += 1;
            let plan = &outcome.plan;
            if plan.active_operations() > 0 {
                report.acted_on += 1;
            }
            for (index, op) in LocalOp::PRIORITY.iter().enumerate() {
                if plan.strength(*op) > 0.0 {
                    if let Some(slot) = report.op_histogram.get_mut(index) {
                        *slot += 1;
                    }
                }
            }
            report.gated += plan.gated_by_mask_quality.len();
            if plan.gated_by_mask_quality.is_empty() {
                report.fully_masked += 1;
            }
            if plan
                .reasons
                .iter()
                .any(|r| r.code == LocalCode::GroupSolvedJointly)
            {
                report.group_solved += 1;
            }
            if plan.shine.is_some() {
                report.shine_reduced += 1;
            }
            if plan.needs_review() {
                report.low_confidence += 1;
            }
            if plan.total_budget_used > 0.0 {
                budget_sum += plan.total_budget_used;
                budget_frames += 1;
            }
            if plan
                .reasons
                .iter()
                .any(|r| r.code == LocalCode::SceneStrengthLimited)
            {
                unpolicied.insert(plan.scene.as_str().to_string());
            }

            progress.report(ProgressUpdate {
                stage: STAGE,
                done: position as u64 + 1,
                total,
                current: None,
            });
        }

        if budget_frames > 0 {
            report.mean_budget_used = budget_sum / budget_frames as f32;
        }
        report.unpolicied_scenes = unpolicied.into_iter().collect();
        report.elapsed_ms = self.clock.monotonic_ms().saturating_sub(started);
        Ok(report)
    }

    /// Everything known about one frame, read through the frozen services.
    fn context_for(&self, id: PhotoId) -> FrameContext {
        let scene = self
            .story
            .as_ref()
            .and_then(|story| story.scene(id).ok().flatten())
            .map_or(SceneId::Unknown, |result| result.scene);
        let target = self.targets.get(scene);
        let faces = self
            .people
            .as_ref()
            .and_then(|people| people.subjects(id).ok())
            .map(|subjects| subjects.faces)
            .unwrap_or_default();
        let noise = self
            .integrity
            .as_ref()
            .and_then(|integrity| integrity.of_image(id).ok().flatten())
            // `noise_sigma_rel` is relative to the *scene's* own tolerance - one is exactly
            // what this kind of photograph accepts - which is the number the cap wants: a
            // dance floor at 0.9 and a family portrait at 0.9 are both at the edge of what
            // their own scene will take, and a shared absolute figure would not be.
            .map_or(0.0, |result| result.noise_sigma_rel.clamp(0.0, 1.0));
        let composition = self
            .composition
            .as_ref()
            .and_then(|composition| composition.of_image(id).ok().flatten());
        FrameContext {
            scene,
            faces,
            masks: self.masks.get(&id).cloned().unwrap_or_default(),
            noise,
            shadow_scale: target.shadow_lift_scale,
            band: target.luma_target(),
            composition,
            enabled: self.enabled,
        }
    }

    /// Section 11's three telemetry events, once per pass.
    fn emit(report: &PassReport) {
        tracing::info!(
            target: "telemetry",
            event = STAGE,
            images = report.planned,
            acted_on = report.acted_on,
            mean_budget_used = report.mean_budget_used,
            ms = report.elapsed_ms,
            "local light pass finished"
        );
        if report.gated > 0 {
            tracing::info!(
                target: "telemetry",
                event = GATED_STAGE,
                count = report.gated,
                fully_masked = report.fully_masked,
                "operations gated by mask quality"
            );
        }
        if report.shine_reduced > 0 {
            tracing::info!(
                target: "telemetry",
                event = SHINE_STAGE,
                count = report.shine_reduced,
                "shine reduced"
            );
        }
    }
}
