//! The frozen `MicroService`, and the pass that fills it.
//!
//! Two halves, the shape phases 06 to 20 all settled on. [`Micro`] answers questions about plans
//! that already exist and is what every later phase holds. [`MicroPass`] walks a project and
//! produces them, and is what the job graph holds.
//!
//! ## Resumability, invariant 5
//!
//! **The work remaining is a query** - [`crate::micro::store::MicroStore::pending`] - rather than
//! a journal. Kill the process at 10 %, 50 % or 90 % and the next run asks the catalog which
//! photographs have no plan at these three versions. A `matrix_ver` bump therefore heals itself.
//!
//! ## Three-tier compute, invariant 3
//!
//! Section 11's last budget is written about a thousand images **at export**, so this is not an
//! import-time pass. [`MicroPass::run`] accepts a whole project because the gate and the fixtures
//! need it, and [`MicroPass::run_ids`] is the path the job graph uses, with phase 12's keepers.
//!
//! ## The sibling window, and why it is bounded
//!
//! A borrow needs another photograph decoded. That is the only place in this phase where the cost
//! of one frame depends on another, so it is bounded twice: at most [`MicroPass::MAX_SIBLINGS`]
//! siblings are offered per frame, and they are fetched **only for frames that already have a
//! blown sheet on them**. A wedding with no glasses in it therefore decodes exactly one proxy per
//! frame, which is what keeps section 11's 250 ms budget reachable.
//!
//! ## No silent failure, invariant 9
//!
//! A frame whose proxy will not decode is counted, coded `AURA-ML-5103` and reported. The run
//! continues and **no row is written**, so the next pass tries again - a written-but-empty plan
//! would read to phases 25, 27 and 28 as "AURA decided this face needed nothing".

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use aura_core::clock::Clock;
use aura_core::contract::micro::{
    ClothingIssue, ImageId, MicroCode, MicroField, MicroOutline, MicroOverride, MicroPlan,
    MicroRegion, MicroService, OpFamily,
};
use aura_core::contract::moment::MomentService;
use aura_core::contract::people::PeopleService;
use aura_core::contract::scene::StoryService;
use aura_core::contract::tone::ToneService;
use aura_core::progress::{CancelToken, ProgressSink, ProgressUpdate};
use aura_core::{AuraError, AuraResult, PhotoId, Priority, ProjectId, SceneId};
use aura_preview::contract::service::{PreviewService, Priority as PreviewPriority};

use crate::errors;
use crate::micro::guard;
use crate::micro::matrix::MicroTable;
use crate::micro::ops::{Analyser, MicroFrame, Sibling, ANALYSIS_VER, MICRO_LEVEL, MODEL_VER};
use crate::micro::store::MicroStore;

/// The telemetry stage name, matching section 11's `micro.applied` event.
pub const STAGE: &str = "micro.applied";

/// The skip telemetry stage, matching section 11's `micro.skipped` event.
pub const SKIPPED_STAGE: &str = "micro.skipped";

/// The borrow telemetry stage, matching section 11's `micro.borrow` event.
pub const BORROW_STAGE: &str = "micro.borrow";

/// What one pass over a project did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MicroPassReport {
    /// Photographs planned.
    pub planned: usize,
    /// Photographs that could not be planned, each logged with a code.
    pub failed: usize,
    /// Photographs where at least one operation ran.
    pub acted_on: usize,
    /// Photographs where at least one usable region arrived.
    pub region_covered: usize,
    /// Operations of each kind, in `MicroOp::NAMES` order.
    pub ops: [usize; 5],
    /// Frames that borrowed pixels from a sibling. **The disclosure count.**
    pub borrows: usize,
    /// Mean alignment score over the borrows that happened.
    pub mean_alignment: f32,
    /// Families withdrawn, in `OpFamily::ALL` order.
    pub withdrawn: [usize; OpFamily::COUNT],
    /// Frames where a family gave up strength to reach its bound.
    pub resolved: usize,
    /// Frames below the review threshold.
    pub low_confidence: usize,
    /// Scenes planned against the neutral row.
    pub unlisted_scenes: Vec<String>,
    /// Milliseconds for the pass.
    pub elapsed_ms: u64,
    /// True when the pass stopped early because it was cancelled.
    pub cancelled: bool,
}

/// The frozen `MicroService` over one catalog.
#[derive(Debug)]
pub struct Micro {
    store: Arc<MicroStore>,
    table: MicroTable,
}

impl Micro {
    /// Wrap a store.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5105` when the embedded matrix will not load.
    pub fn new(store: Arc<MicroStore>) -> Result<Self, AuraError> {
        Ok(Self {
            store,
            table: MicroTable::embedded()?,
        })
    }

    /// The three versions this build produces.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5105` when the embedded matrix will not load.
    pub fn current_versions() -> Result<(u16, u16, u16), AuraError> {
        Ok((MODEL_VER, ANALYSIS_VER, MicroTable::embedded()?.version()))
    }

    /// The store underneath, for the panel's own reads and for the gate.
    #[must_use]
    pub fn store(&self) -> &Arc<MicroStore> {
        &self.store
    }

    /// The matrix defaults this build ships.
    #[must_use]
    pub fn defaults(&self) -> ([bool; 5], [bool; ClothingIssue::COUNT], bool) {
        (
            self.table.defaults(),
            self.table.clothing_defaults(),
            self.table.borrowing_default(),
        )
    }
}

impl MicroService for Micro {
    fn outline(&self, project: ProjectId) -> AuraResult<MicroOutline> {
        let outline = self.store.outline(&project, self.table.unlisted())?;
        // Reported rather than enforced. `AURA-ML-5102` is degraded: stale plans keep working
        // while the background pass replaces them, and a caller about to draw a conclusion over a
        // mixed set finds out before it draws it.
        let current = (MODEL_VER, ANALYSIS_VER, self.table.version());
        if outline.planned > 0 && !outline.versions_agree(current) {
            let stale = errors::micro_version_mismatch(
                (outline.model_ver, outline.analysis_ver, outline.matrix_ver),
                current,
                outline.planned as usize,
            );
            tracing::info!(
                target: "micro.outline",
                code = %stale.code,
                "{}", stale.detail
            );
        }
        Ok(outline)
    }

    fn of_image(&self, image: ImageId) -> AuraResult<Option<MicroPlan>> {
        self.store.of_image(image)
    }

    fn composites(&self, project: ProjectId) -> AuraResult<BTreeMap<ImageId, Vec<ImageId>>> {
        self.store.composites(&project)
    }

    fn needs_review(&self, project: ProjectId, limit: usize) -> AuraResult<Vec<ImageId>> {
        self.store.needs_review(&project, limit)
    }

    fn matrix(&self, project: ProjectId) -> AuraResult<MicroOverride> {
        self.store.matrix(&project, self.defaults())
    }

    fn accept(&self, image: ImageId) -> Result<(), AuraError> {
        self.store.accept(image)
    }

    fn set_matrix(&self, project: ProjectId, values: MicroOverride) -> Result<(), AuraError> {
        guard::check_override(&values)?;
        self.store
            .set_matrix(&project, &values, self.defaults(), self.table.version())
    }
}

/// The resumable project walk.
pub struct MicroPass {
    store: Arc<MicroStore>,
    previews: Arc<dyn PreviewService>,
    clock: Arc<dyn Clock>,
    analyser: Analyser,
    people: Option<Arc<dyn PeopleService>>,
    story: Option<Arc<dyn StoryService>>,
    moments: Option<Arc<dyn MomentService>>,
    tone: Option<Arc<dyn ToneService>>,
    regions: BTreeMap<PhotoId, BTreeMap<MicroRegion, MicroField>>,
    neutrals: BTreeMap<PhotoId, [f32; 2]>,
    enabled: bool,
}

impl fmt::Debug for MicroPass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MicroPass")
            .field("regions", &self.regions.len())
            .field("neutrals", &self.neutrals.len())
            .field("enabled", &self.enabled)
            .finish_non_exhaustive()
    }
}

impl MicroPass {
    /// How many frames ahead the pass asks the preview service to prepare.
    pub const PREFETCH_WINDOW: usize = 8;

    /// The most siblings offered to one borrow search.
    ///
    /// Four. Each one is a decoded proxy and a correlation search, and a fifth candidate has
    /// almost never been the one that wins: a burst's nearest neighbours are its best donors,
    /// because the head has moved least.
    pub const MAX_SIBLINGS: usize = 4;

    /// Build a pass over a store and a preview service.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5105` when the embedded matrix will not load.
    pub fn new(
        store: Arc<MicroStore>,
        previews: Arc<dyn PreviewService>,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, AuraError> {
        Ok(Self {
            store,
            previews,
            clock,
            analyser: Analyser::new()?,
            people: None,
            story: None,
            moments: None,
            regions: BTreeMap::new(),
            tone: None,
            neutrals: BTreeMap::new(),
            enabled: true,
        })
    }

    /// Supply the people service, for faces and identities.
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

    /// Supply the moment service, for the sibling frames a borrow may draw on.
    ///
    /// **The only route to a sibling.** Without it no borrow is possible anywhere in the pass and
    /// every blown sheet is reduced from its own frame instead - which is a worse repair and a
    /// perfectly safe one.
    #[must_use]
    pub fn with_moments(mut self, moments: Arc<dyn MomentService>) -> Self {
        self.moments = Some(moments);
        self
    }

    /// Supply the regions phase 18 generated.
    ///
    /// **The input port, and the only one.** Taken as a map rather than read per frame, because
    /// this crate has no dependency on phase 18 and must not acquire one. An empty map - the
    /// state of this build - skips every operation and the plans say
    /// [`MicroCode::RegionUnavailable`].
    #[must_use]
    pub fn with_regions(
        mut self,
        regions: BTreeMap<PhotoId, BTreeMap<MicroRegion, MicroField>>,
    ) -> Self {
        self.regions = regions;
        self
    }

    /// Supply phase 15's illuminant estimate per frame, in CIE `u'v'`.
    ///
    /// Absent, every colour move is skipped and the plans say [`MicroCode::NoIlluminant`]. That is
    /// the conservative direction and it is the only correct one: a locus expressed relative to a
    /// neutral has no meaning without the neutral.
    #[must_use]
    pub fn with_neutrals(mut self, neutrals: BTreeMap<PhotoId, [f32; 2]>) -> Self {
        self.neutrals = neutrals;
        self
    }

    /// Attach phase 15's estimator, so each frame's neutral is asked for rather than supplied.
    ///
    /// **Phase 15's rule, kept**: `ToneService` is the only way to ask what colour the light was,
    /// and this phase measures teeth and sclera as distances from that answer. Without it the
    /// colour half of both operators does not run and every plan records
    /// [`aura_core::contract::micro::MicroCode::NoIlluminant`] - which is the conservative
    /// direction, and is exactly what a caller that attaches nothing still gets.
    ///
    /// A neutral supplied through [`MicroPass::with_neutrals`] wins over the service, because a
    /// caller that handed one in has already decided; the service is what fills the gaps.
    #[must_use]
    pub fn with_tone(mut self, tone: Arc<dyn ToneService>) -> Self {
        self.tone = Some(tone);
        self
    }

    /// Switch the whole stage off.
    ///
    /// Hard rule 8's kill switch. A disabled pass still writes a plan per frame - one that does
    /// nothing - because a frame with no plan and a frame the studio switched off look identical
    /// in a coverage report.
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
    /// `AURA-DB-3006` when the pending set cannot be read. Per-photograph failures are counted in
    /// the report rather than returned: one unreadable frame must not end a pass.
    pub fn run(
        &self,
        project: &ProjectId,
        prio: Priority,
        cancel: &CancelToken,
        progress: &dyn ProgressSink,
    ) -> AuraResult<MicroPassReport> {
        let pending = self.store.pending(
            project,
            (MODEL_VER, ANALYSIS_VER, self.analyser.table().version()),
        )?;
        let report = self.run_ids(project, &pending, prio, cancel, progress)?;
        Self::emit(&report);
        Ok(report)
    }

    /// Plan a specific list of photographs.
    ///
    /// **The path the job graph uses**, with phase 12's keepers. Invariant 3.
    ///
    /// # Errors
    ///
    /// As [`MicroPass::run`].
    #[allow(clippy::too_many_lines)]
    pub fn run_ids(
        &self,
        project: &ProjectId,
        ids: &[PhotoId],
        prio: Priority,
        cancel: &CancelToken,
        progress: &dyn ProgressSink,
    ) -> AuraResult<MicroPassReport> {
        let started = self.clock.monotonic_ms();
        let mut report = MicroPassReport::default();
        let total = ids.len() as u64;
        let preview_priority = match prio {
            Priority::Visible => PreviewPriority::Visible,
            Priority::Interactive => PreviewPriority::Interactive,
            Priority::AiBatch => PreviewPriority::AiBatch,
            Priority::Background => PreviewPriority::Background,
        };

        let matrix = self
            .store
            .matrix(project, self.analyser.table().defaults_triple())?;
        let mut alignment_sum = 0.0f32;
        let mut alignment_count = 0usize;

        for (position, id) in ids.iter().enumerate() {
            if cancel.is_cancelled() {
                report.cancelled = true;
                break;
            }
            if let Some(window) =
                ids.get(position + 1..(position + 1 + Self::PREFETCH_WINDOW).min(ids.len()))
            {
                self.previews.prefetch(window, MICRO_LEVEL);
            }

            let buffer = match self.previews.get(*id, MICRO_LEVEL, preview_priority) {
                Ok(buffer) => buffer,
                Err(err) => {
                    let coded = errors::micro_failed(&id.to_db(), &err.detail);
                    tracing::warn!(
                        target: "micro.pass",
                        photo = %id,
                        code = %coded.code,
                        "{}", coded.detail
                    );
                    report.failed += 1;
                    continue;
                }
            };

            let context = self.frame_for(*id, &matrix, preview_priority);
            let outcome = match self.analyser.analyse(*id, &buffer, &context) {
                Ok(outcome) => outcome,
                Err(err) => {
                    tracing::warn!(
                        target: "micro.pass",
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
                    target: "micro.pass",
                    photo = %id,
                    code = %warning.code,
                    "{}", warning.detail
                );
            }
            if let Err(err) = guard::check_plan(&outcome.plan) {
                tracing::warn!(
                    target: "micro.pass",
                    photo = %id,
                    code = %err.code,
                    "{}", err.detail
                );
                report.failed += 1;
                continue;
            }
            if let Err(err) = self.store.put(project, &outcome.plan) {
                tracing::warn!(
                    target: "micro.pass",
                    photo = %id,
                    code = %err.code,
                    "{}", err.detail
                );
                report.failed += 1;
                continue;
            }

            let plan = &outcome.plan;
            report.planned += 1;
            if !plan.is_noop() {
                report.acted_on += 1;
            }
            for (index, name) in aura_core::contract::micro::MicroOp::NAMES
                .iter()
                .enumerate()
            {
                if let Some(slot) = report.ops.get_mut(index) {
                    *slot += plan.count_of(name);
                }
            }
            if plan.is_composite() {
                report.borrows += 1;
                for op in &plan.ops {
                    if let aura_core::contract::micro::MicroOp::Glare {
                        method:
                            aura_core::contract::micro::GlareMethod::BorrowFrom { alignment, .. },
                        ..
                    } = op
                    {
                        alignment_sum += *alignment;
                        alignment_count += 1;
                    }
                }
            }
            for (index, family) in OpFamily::ALL.iter().enumerate() {
                if plan.naturalness.is_withdrawn(*family) {
                    if let Some(slot) = report.withdrawn.get_mut(index) {
                        *slot += 1;
                    }
                }
            }
            if plan.naturalness.resolves > 0 {
                report.resolved += 1;
            }
            if plan.needs_review() {
                report.low_confidence += 1;
            }
            if !plan.reasons.iter().any(|reason| {
                reason.code == MicroCode::RegionUnavailable
                    || reason.code == MicroCode::RegionDoubtful
            }) {
                report.region_covered += 1;
            }

            progress.report(ProgressUpdate {
                stage: STAGE,
                done: position as u64 + 1,
                total,
                current: None,
            });
        }

        report.mean_alignment = if alignment_count == 0 {
            0.0
        } else {
            alignment_sum / alignment_count as f32
        };
        report.unlisted_scenes = self.analyser.table().unlisted();
        report.elapsed_ms = self.clock.monotonic_ms().saturating_sub(started);
        Ok(report)
    }

    /// Everything known about one frame before its pixels are read.
    fn frame_for(&self, id: PhotoId, matrix: &MicroOverride, prio: PreviewPriority) -> MicroFrame {
        let scene = self.scene_of(id);
        let faces = self
            .people
            .as_ref()
            .and_then(|people| people.subjects(id).ok())
            .map(|subjects| subjects.faces)
            .unwrap_or_default();
        let regions = self.regions.get(&id).cloned().unwrap_or_default();
        let defaults = self.analyser.table().defaults_triple();
        let allowed = matrix.allowed.unwrap_or(defaults.0);
        let borrowing = matrix.borrowing.unwrap_or(defaults.2);

        // The sibling fetch is deliberately conditional. See the module header: a wedding with no
        // glasses in it must decode exactly one proxy per frame.
        let siblings = if borrowing && allowed.get(4).copied().unwrap_or(false) {
            self.siblings_for(id, prio)
        } else {
            Vec::new()
        };

        MicroFrame {
            scene,
            faces,
            regions,
            neutral: self.neutral_for(id),
            allowed,
            clothing: matrix.clothing.unwrap_or(defaults.1),
            borrowing,
            siblings,
            enabled: self.enabled,
        }
    }

    /// This frame's own neutral in CIE `u'v'`, supplied or asked for.
    ///
    /// The *subject's* illuminant rather than the strongest one, because that is the light the
    /// teeth in this frame were under - a candle behind somebody's shoulder can outweigh the
    /// window on their face, and phase 15 stores which one governs the subject exactly so this
    /// question has an answer. `None` when nobody has estimated the frame, which skips every
    /// colour move rather than inventing an origin to measure against.
    fn neutral_for(&self, id: PhotoId) -> Option<[f32; 2]> {
        if let Some(supplied) = self.neutrals.get(&id).copied() {
            return Some(supplied);
        }
        let estimate = self.tone.as_ref()?.of_image(id).ok().flatten()?;
        let index = estimate.dominant_on_subject.unwrap_or(0);
        estimate
            .illuminants
            .get(index)
            .or_else(|| estimate.illuminants.first())
            .map(|light| light.uv)
    }

    /// The frames of the same moment that could donate pixels to this one.
    ///
    /// Ordered by how close they are in the moment's own sequence, nearest first, so the donor
    /// search sees the frames whose heads have moved least before the ones that have moved most -
    /// and so two machines offer the same list. Invariant 4.
    fn siblings_for(&self, id: PhotoId, prio: PreviewPriority) -> Vec<Sibling> {
        let Some(moments) = self.moments.as_ref() else {
            return Vec::new();
        };
        let Ok(Some(moment)) = moments.moment_of(id) else {
            return Vec::new();
        };
        let Some(position) = moment.image_ids.iter().position(|other| *other == id) else {
            return Vec::new();
        };

        let mut ordered: Vec<(usize, PhotoId)> = moment
            .image_ids
            .iter()
            .enumerate()
            .filter(|(_, other)| **other != id)
            .map(|(index, other)| (index.abs_diff(position), *other))
            .collect();
        ordered.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

        let mut out = Vec::new();
        for (_, other) in ordered.into_iter().take(Self::MAX_SIBLINGS) {
            let Ok(pixels) = self.previews.get(other, MICRO_LEVEL, prio) else {
                continue;
            };
            let faces = self
                .people
                .as_ref()
                .and_then(|people| people.subjects(other).ok())
                .map(|subjects| subjects.faces)
                .unwrap_or_default();
            out.push(Sibling {
                image: other,
                pixels: (*pixels).clone(),
                faces,
            });
        }
        out
    }

    fn scene_of(&self, id: PhotoId) -> SceneId {
        self.story
            .as_ref()
            .and_then(|story| story.scene(id).ok().flatten())
            .map_or(SceneId::Unknown, |result| result.scene)
    }

    /// Section 11's three telemetry events, emitted once per pass.
    fn emit(report: &MicroPassReport) {
        tracing::info!(
            target: STAGE,
            images = report.planned,
            flyaway = report.ops[0],
            teeth = report.ops[1],
            eyes = report.ops[2],
            clothing = report.ops[3],
            glare = report.ops[4],
            ms = report.elapsed_ms,
            "micro pass complete"
        );
        tracing::info!(
            target: SKIPPED_STAGE,
            no_region = report.planned.saturating_sub(report.region_covered),
            withdrawn_hair = report.withdrawn[0],
            withdrawn_teeth = report.withdrawn[1],
            withdrawn_eyes = report.withdrawn[2],
            resolved = report.resolved,
            "micro operations skipped"
        );
        // The disclosure event. Section 11 names it and it is the one number in this phase a
        // studio may be asked about by a client.
        tracing::info!(
            target: BORROW_STAGE,
            count = report.borrows,
            alignment_score = report.mean_alignment,
            "cross-frame borrows"
        );
    }
}
