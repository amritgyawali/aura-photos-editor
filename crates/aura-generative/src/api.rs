//! The frozen `CleanupService`, and the resumable pass that fills it.
//!
//! Two halves, the shape phases 06 to 23 all settled on. [`Cleanup`] answers questions about
//! proposals that already exist and is what every later phase holds. [`CleanupPass`] walks a
//! project and produces them, and is what the job graph holds.
//!
//! ## Resumability, invariant 5
//!
//! **The work remaining is a query** - [`crate::store::CleanupStore::pending`] - rather than a
//! journal. Kill the process at 10 %, 50 % or 90 % and the next run asks the catalog which
//! photographs have not been examined at these three versions. A `policy_ver` bump therefore heals
//! itself, and a photograph a photographer switched cleanup off for is excluded rather than
//! re-proposed.
//!
//! ## Three-tier compute, invariant 3
//!
//! The decision is made on a 2048 px proxy, like every phase since 06. The *removal* is applied at
//! full resolution, at export, on the frames being delivered - which is why section 11's budgets
//! are written about a 45 MP fill while this pass's budget is written about a proxy.
//!
//! ## Nothing here applies anything
//!
//! [`CleanupPass::run`] writes proposals and refusals. It never writes a recipe, never replaces a
//! pixel in a delivered file and never marks anything applied. ADR-0049 section 9: removals are
//! proposals, `aura-app` merges an accepted one through `aura_recipe::schema::merge`, and there is
//! no code path from this pass to a written recipe.
//!
//! ## Every input is optional and every absence is visible
//!
//! Phase 19's rule - a phase that consumes another phase's output owns no fallback for it - in its
//! strictest form yet. Without [`CleanupPass::with_masks`] the coverage is [`Coverage::Absent`] and
//! **every candidate is blocked**, which is the correct behaviour and is condition C1 of the exit
//! report. Without composition there are no salience regions and the pass proposes nothing at all;
//! without moments there are no siblings and a borrow is impossible. None of the three is
//! substituted for.
//!
//! [`Coverage::Absent`]: crate::denylist::Coverage::Absent

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use aura_core::clock::Clock;
use aura_core::contract::cleanup::{
    Box2, CleanupCode, CleanupDisclosure, CleanupOutline, CleanupOverride, CleanupProposal,
    CleanupService, ImageId, SafetyCheck,
};
use aura_core::contract::composition::CompositionService;
use aura_core::contract::moment::MomentService;
use aura_core::contract::people::PeopleService;
use aura_core::contract::scene::StoryService;
use aura_core::progress::{CancelToken, ProgressSink, ProgressUpdate};
use aura_core::{AuraError, AuraResult, PhotoId, Priority, ProjectId, SceneId};
use aura_preview::contract::service::{PreviewService, Priority as PreviewPriority};
use aura_raw::contract::pixels::{PixelBuffer, PixelData, PixelLevel};

use crate::denylist::{Coverage, Protected};
use crate::detect::{self, Frame};
use crate::errors;
use crate::judgement::EditorialJudge;
use crate::pixels::Image;
use crate::policy::Policy;
use crate::queue::{self, Context};
use crate::source::{Sibling, Sources};

/// The telemetry stage name, matching section 11's `cleanup.proposed` event.
pub const STAGE: &str = "cleanup.proposed";

/// The telemetry stage for a refusal, matching section 11's `cleanup.blocked` event.
pub const BLOCKED_STAGE: &str = "cleanup.blocked";

/// The telemetry stage for a self-check revert, matching section 11's `cleanup.reverted` event.
pub const REVERTED_STAGE: &str = "cleanup.reverted";

/// The most sibling frames one photograph's borrow search will try.
///
/// Section 11 budgets 700 ms per borrow and a moment can be forty frames long. Four is what fits,
/// and the four are the *nearest in time*, which are also the ones most likely to align - a frame
/// eight seconds later is a different camera position and usually a different subject.
pub const MAX_SIBLINGS: usize = 4;

/// What one pass did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CleanupPassReport {
    /// Photographs examined.
    pub examined: u32,
    /// Photographs carrying at least one proposal.
    pub with_proposals: u32,
    /// Proposals produced.
    pub proposals: u32,
    /// Candidates the safety engine and its downstream checks refused, by check.
    pub blocked: [u32; SafetyCheck::COUNT],
    /// Removals the self-check undid before anybody saw them.
    pub reverted: u32,
    /// Photographs whose six protected kinds could all be looked for.
    pub mask_complete: u32,
    /// Cloud editorial judgements made.
    pub judged: u32,
    /// Judgements that declined a removal.
    pub declined: u32,
    /// Photographs that could not be examined.
    pub failed: u32,
    /// True when the run was cancelled part way.
    pub cancelled: bool,
    /// How long it took.
    pub elapsed_ms: u64,
}

impl CleanupPassReport {
    /// Every refusal, however it was reached.
    #[must_use]
    pub fn blocked_total(&self) -> u32 {
        self.blocked.iter().copied().sum()
    }
}

/// The frozen service. What every later phase holds.
#[derive(Debug)]
pub struct Cleanup {
    store: Arc<crate::store::CleanupStore>,
    policy_ver: u16,
}

impl Cleanup {
    /// Wrap one store.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5119` when the shipped policy table will not load, because the version on every
    /// stored row comes from it and a service that could not say which policy a proposal was made
    /// under could not detect a drift.
    pub fn new(store: Arc<crate::store::CleanupStore>) -> Result<Self, AuraError> {
        let policy = Policy::shipped()?;
        Ok(Self {
            store,
            policy_ver: policy.version,
        })
    }

    /// The three versions this build produces.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5119` when the shipped table will not load.
    pub fn current_versions() -> Result<(u16, u16, u16), AuraError> {
        let policy = Policy::shipped()?;
        Ok((crate::DETECTOR_VER, crate::ANALYSIS_VER, policy.version))
    }

    /// The store underneath, for the gate and the budget test.
    #[must_use]
    pub fn store(&self) -> &Arc<crate::store::CleanupStore> {
        &self.store
    }

    /// Which policy table this service is reading.
    #[must_use]
    pub const fn policy_ver(&self) -> u16 {
        self.policy_ver
    }
}

impl CleanupService for Cleanup {
    fn proposals(&self, image: ImageId) -> AuraResult<Vec<CleanupProposal>> {
        self.store.proposals(image)
    }

    fn blocked(&self, image: ImageId) -> AuraResult<Vec<(Box2, SafetyCheck, CleanupCode)>> {
        self.store.blocked(image)
    }

    fn disclosures(&self, project: ProjectId) -> AuraResult<Vec<CleanupDisclosure>> {
        self.store.disclosures(project)
    }

    /// Record a photographer's decision.
    ///
    /// The three things a person may say, and there is nothing else on this surface: accept one,
    /// reject one, or switch cleanup off for the whole photograph. No strength, no size and no
    /// description - which is what makes `docs/generative-policy.md` a promise about the product
    /// rather than a description of its defaults.
    fn decide(&self, image: ImageId, choice: &CleanupOverride) -> AuraResult<()> {
        let mut did_something = false;
        if let Some(disabled) = choice.disable_for_image {
            // The scene is only used when the row does not exist yet, and a photographer switching
            // cleanup off for an unexamined photograph is a real case: they can see the frame in
            // the grid before the pass has reached it.
            self.store
                .set_disabled(&ProjectId::default(), image, SceneId::Unknown, disabled)?;
            did_something = true;
        }
        if let (Some(proposal), Some(accept)) = (choice.proposal_id, choice.accept) {
            self.store.decide(image, proposal, accept)?;
            did_something = true;
        }
        if !did_something {
            return Err(aura_core::errors::ml::cleanup_override_refused(
                "the override named no proposal and asked for nothing",
            ));
        }
        Ok(())
    }

    fn outline(&self, project: ProjectId) -> AuraResult<CleanupOutline> {
        self.store.outline(project)
    }
}

/// The resumable pass. What the job graph holds.
pub struct CleanupPass {
    store: Arc<crate::store::CleanupStore>,
    previews: Arc<dyn PreviewService>,
    clock: Arc<dyn Clock>,
    policy: Policy,
    composition: Option<Arc<dyn CompositionService>>,
    people: Option<Arc<dyn PeopleService>>,
    moments: Option<Arc<dyn MomentService>>,
    story: Option<Arc<dyn StoryService>>,
    /// The protected regions, by photograph, from phase 18's `MaskService`.
    ///
    /// Handed in rather than read through the trait, because `MaskService::masks` returns payloads
    /// rather than bounding boxes and turning twenty alpha planes into six rectangles is the
    /// caller's job, done once, where the mask crate's own types are in scope. `aura-app` fills
    /// this; the default is empty, which means [`Coverage::Absent`] on every frame.
    coverage: BTreeMap<PhotoId, Coverage>,
    judge: Option<Arc<dyn EditorialJudge>>,
    studio_opted_in: bool,
    enabled: bool,
}

impl fmt::Debug for CleanupPass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CleanupPass")
            .field("policy_rows", &self.policy.len())
            .field("policy_ver", &self.policy.version)
            .field("coverage_known", &self.coverage.len())
            .field("has_composition", &self.composition.is_some())
            .field("has_moments", &self.moments.is_some())
            .field("has_judge", &self.judge.is_some())
            .field("enabled", &self.enabled)
            .finish()
    }
}

impl CleanupPass {
    /// How many photographs ahead the preview service is asked to warm.
    const PREFETCH_WINDOW: usize = 8;

    /// Build a pass over one catalog.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5119` when `cleanup_policy.toml` will not load. Halting rather than falling back
    /// on a default, because tidying every wedding to a table nobody approved is far worse than
    /// tidying nothing.
    pub fn new(
        store: Arc<crate::store::CleanupStore>,
        previews: Arc<dyn PreviewService>,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, AuraError> {
        Ok(Self {
            store,
            previews,
            clock,
            policy: Policy::shipped()?,
            composition: None,
            people: None,
            moments: None,
            story: None,
            coverage: BTreeMap::new(),
            judge: None,
            studio_opted_in: false,
            enabled: true,
        })
    }

    /// Where the salience regions come from. Phase 11.
    #[must_use]
    pub fn with_composition(mut self, composition: Arc<dyn CompositionService>) -> Self {
        self.composition = Some(composition);
        self
    }

    /// Where the subject boxes come from. Phase 06.
    #[must_use]
    pub fn with_people(mut self, people: Arc<dyn PeopleService>) -> Self {
        self.people = Some(people);
        self
    }

    /// Where the sibling frames come from. Phase 08, and the only route to them.
    #[must_use]
    pub fn with_moments(mut self, moments: Arc<dyn MomentService>) -> Self {
        self.moments = Some(moments);
        self
    }

    /// Where the scene comes from. Phase 07, and what chooses the policy row.
    #[must_use]
    pub fn with_story(mut self, story: Arc<dyn StoryService>) -> Self {
        self.story = Some(story);
        self
    }

    /// The protected regions this pass may intersect against. Phase 18.
    ///
    /// **Without this every candidate is blocked**, and that is the correct behaviour rather than
    /// a gap to work around. See the module header and ADR-0049 section 3.
    #[must_use]
    pub fn with_masks(mut self, coverage: BTreeMap<PhotoId, Coverage>) -> Self {
        self.coverage = coverage;
        self
    }

    /// The cloud editorial judge, when one is configured. Phase 04.
    #[must_use]
    pub fn with_judge(mut self, judge: Arc<dyn EditorialJudge>) -> Self {
        self.judge = Some(judge);
        self
    }

    /// Whether the studio has opted the diffusion tier in. Off at installation.
    #[must_use]
    pub const fn with_studio_opt_in(mut self, opted_in: bool) -> Self {
        self.studio_opted_in = opted_in;
        self
    }

    /// The kill switch. Section 12's rollback path: a feature flag that turns the phase off.
    ///
    /// A disabled pass still **examines** every photograph and writes a row saying so, with no
    /// proposals. That is deliberate: a photograph nobody looked at and a photograph with nothing
    /// to tidy are delivered identically, and the outline is the only place the difference shows.
    #[must_use]
    pub const fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// The three versions this pass writes.
    #[must_use]
    pub const fn versions(&self) -> (u16, u16, u16) {
        (crate::DETECTOR_VER, crate::ANALYSIS_VER, self.policy.version)
    }

    /// Examine everything in a project that has not been examined at these versions.
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
    ) -> AuraResult<CleanupPassReport> {
        let pending = self.store.pending(project, self.versions())?;
        let report = self.run_ids(project, &pending, prio, cancel, progress)?;
        Self::emit(&report);
        Ok(report)
    }

    /// Examine a specific list of photographs.
    ///
    /// **The path the job graph uses**, with phase 12's keepers. Invariant 3: expensive work only
    /// on survivors, and a distraction in a rejected frame is not a distraction.
    ///
    /// # Errors
    ///
    /// As [`CleanupPass::run`].
    #[allow(clippy::too_many_lines)]
    pub fn run_ids(
        &self,
        project: &ProjectId,
        ids: &[PhotoId],
        prio: Priority,
        cancel: &CancelToken,
        progress: &dyn ProgressSink,
    ) -> AuraResult<CleanupPassReport> {
        let started = self.clock.monotonic_ms();
        let mut report = CleanupPassReport::default();
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

            let scene = self.scene_of(*id);
            let Some(policy) = self.policy.scene(scene) else {
                // Invariant 7: no threshold is global, so a scene with no row is left alone rather
                // than tidied to a default nobody wrote down.
                let coded = errors::scene_missing(scene.as_str());
                tracing::warn!(
                    target: "cleanup.pass",
                    photo = %id,
                    code = %coded.code,
                    "{}", coded.detail
                );
                report.failed += 1;
                continue;
            };

            let buffer = match self.previews.get(*id, level(), preview_priority) {
                Ok(buffer) => buffer,
                Err(err) => {
                    let coded = errors::item_failed(format!(
                        "the proxy for {} could not be read: {}",
                        id.to_db(),
                        err.detail
                    ));
                    tracing::warn!(
                        target: "cleanup.pass",
                        photo = %id,
                        code = %coded.code,
                        "{}", coded.detail
                    );
                    report.failed += 1;
                    continue;
                }
            };
            let Some(target) = to_image(&buffer) else {
                let coded = errors::item_failed(
                    "the proxy carries tiles, which this pass never asks for".to_string(),
                );
                tracing::warn!(
                    target: "cleanup.pass",
                    photo = %id,
                    code = %coded.code,
                    "{}", coded.detail
                );
                report.failed += 1;
                continue;
            };

            let coverage = self
                .coverage
                .get(id)
                .cloned()
                .unwrap_or(Coverage::Absent);
            if !coverage.is_known() {
                // Raised where it happens rather than inferred from a histogram, so a photographer
                // reading the problems panel learns that AURA could not tell where people are
                // rather than that it found nothing to tidy.
                let coded = errors::protection_unknown(format!(
                    "no protected regions are available for {}",
                    id.to_db()
                ));
                tracing::debug!(
                    target: "cleanup.pass",
                    photo = %id,
                    code = %coded.code,
                    "{}", coded.detail
                );
            }

            let candidates = if self.enabled {
                detect::candidates(&self.frame_for(*id))
            } else {
                Vec::new()
            };

            let siblings = self.siblings_for(*id, preview_priority);
            let sibling_refs: Vec<Sibling<'_>> = siblings
                .iter()
                .map(|(id, image)| Sibling { id: *id, image })
                .collect();

            let context = Context {
                image: *id,
                scene,
                policy,
                coverage: &coverage,
                sources: Sources {
                    target: &target,
                    siblings: &sibling_refs,
                    studio_opted_in: self.studio_opted_in,
                },
                detector_ver: crate::DETECTOR_VER,
                analysis_ver: crate::ANALYSIS_VER,
                policy_ver: self.policy.version,
                // Nothing in this build is calibrated, so every band is raised one further toward
                // review. Phase 13's `uncalibrated_raises`, read here rather than assumed.
                calibrated: false,
            };
            let plan = queue::plan(&context, &candidates, self.judge.as_deref());

            if let Err(err) = self.store.put(project, *id, scene, &plan, self.versions()) {
                tracing::warn!(
                    target: "cleanup.pass",
                    photo = %id,
                    code = %err.code,
                    "{}", err.detail
                );
                report.failed += 1;
                continue;
            }

            report.examined += 1;
            report.proposals += u32::try_from(plan.prepared.len()).unwrap_or(0);
            if plan.has_proposals() {
                report.with_proposals += 1;
            }
            if plan.mask_complete {
                report.mask_complete += 1;
            }
            report.reverted += plan.reverted;
            report.judged += plan.judged;
            report.declined += plan.declined;
            for block in &plan.blocked {
                if let Some(slot) = report
                    .blocked
                    .get_mut(SafetyCheck::ALL.iter().position(|c| *c == block.check).unwrap_or(4))
                {
                    *slot += 1;
                }
            }
            if plan.reverted > 0 {
                let coded = errors::self_check_reverted(format!(
                    "{} removal(s) on {} were undone before anybody saw them",
                    plan.reverted,
                    id.to_db()
                ));
                tracing::info!(
                    target: REVERTED_STAGE,
                    photo = %id,
                    code = %coded.code,
                    "{}", coded.detail
                );
            }

            progress.report(ProgressUpdate {
                stage: STAGE,
                done: (position + 1) as u64,
                total,
                current: None,
            });
        }

        report.elapsed_ms = self.clock.monotonic_ms().saturating_sub(started);
        Ok(report)
    }

    /// The scene one photograph is, or `Unknown` when phase 07 has not classified it.
    fn scene_of(&self, image: ImageId) -> SceneId {
        self.story
            .as_ref()
            .and_then(|story| story.scene(image).ok().flatten())
            .map_or(SceneId::Unknown, |result| result.scene)
    }

    /// What the detector is given about one photograph.
    ///
    /// Every field is a measurement an earlier phase already made. This pass opens the proxy for
    /// the *removal*, never for the detection - invariant 3, and phase 05's rule that descriptors
    /// are computed once.
    fn frame_for(&self, image: ImageId) -> Frame {
        let mut frame = Frame::default();
        if let Some(composition) = self.composition.as_ref() {
            if let Ok(Some(result)) = composition.of_image(image) {
                // Bright regions brighter than the subject, and things entering at an edge. Those
                // are phase 11's two measurements of "something here is pulling the eye and is not
                // the subject", which is the definition of unexplained salience.
                for blob in &result.bright_blobs {
                    frame.salient.push((*blob, 0.85));
                }
                for intrusion in &result.edge_intrusions {
                    frame.salient.push((*intrusion, 0.70));
                }
            }
        }
        if let Some(people) = self.people.as_ref() {
            if let Ok(subjects) = people.subjects(image) {
                for face in &subjects.faces {
                    frame.subjects.push(face.bbox);
                }
            }
        }
        frame
    }

    /// The frames of the same moment whose pixels a borrow may come from.
    ///
    /// Phase 08's `MomentService` is the only thing that decides what "the same moment" means, and
    /// a list assembled from timestamps here would be the second answer to "what was shot once"
    /// that phase 08's rule forbids.
    ///
    /// The nearest [`MAX_SIBLINGS`] frames in the moment's own order, excluding this one. A frame
    /// whose proxy will not decode is skipped rather than failing the photograph: a borrow is a
    /// preference, and losing one candidate source costs a fill rather than a removal.
    fn siblings_for(&self, image: ImageId, priority: PreviewPriority) -> Vec<(PhotoId, Image)> {
        let Some(moments) = self.moments.as_ref() else {
            return Vec::new();
        };
        let Ok(Some(moment)) = moments.moment_of(image) else {
            return Vec::new();
        };
        let Some(position) = moment.image_ids.iter().position(|id| *id == image) else {
            return Vec::new();
        };

        // Nearest first, alternating either side, so a frame at the start of a burst still gets
        // four candidates and they are still its closest neighbours.
        let mut order: Vec<PhotoId> = Vec::new();
        for step in 1..moment.image_ids.len() {
            if let Some(id) = position.checked_sub(step).and_then(|i| moment.image_ids.get(i)) {
                order.push(*id);
            }
            if let Some(id) = moment.image_ids.get(position + step) {
                order.push(*id);
            }
            if order.len() >= MAX_SIBLINGS {
                break;
            }
        }
        order.truncate(MAX_SIBLINGS);

        order
            .into_iter()
            .filter_map(|id| {
                let buffer = self.previews.get(id, level(), priority).ok()?;
                let image = to_image(&buffer)?;
                Some((id, image))
            })
            .collect()
    }

    /// Section 11's telemetry, emitted once per pass.
    fn emit(report: &CleanupPassReport) {
        tracing::info!(
            target: STAGE,
            examined = report.examined,
            proposals = report.proposals,
            with_proposals = report.with_proposals,
            mask_complete = report.mask_complete,
            elapsed_ms = report.elapsed_ms,
            "cleanup pass finished"
        );
        tracing::info!(
            target: BLOCKED_STAGE,
            size_cap = report.blocked.first().copied().unwrap_or(0),
            denylist = report.blocked.get(1).copied().unwrap_or(0),
            identity = report.blocked.get(2).copied().unwrap_or(0),
            structure = report.blocked.get(3).copied().unwrap_or(0),
            confidence = report.blocked.get(4).copied().unwrap_or(0),
            "cleanup refusals"
        );
    }
}

/// The render level this pass reads. Invariant 3.
#[must_use]
pub const fn level() -> PixelLevel {
    PixelLevel::Proxy2048
}

/// Read a proxy buffer as a linear image.
///
/// `None` when the buffer carries tiles, which this pass never asks for: tiling is an export path
/// and this is a decision path.
#[must_use]
pub fn to_image(buffer: &PixelBuffer) -> Option<Image> {
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
    Some(Image {
        w: width,
        h: height,
        rgb,
    })
}

/// Turn phase 18's masks into the six rectangles the denylist intersects against.
///
/// **The one place the two vocabularies meet**, and it is a function rather than a method so that
/// its incompleteness is visible: [`Protected::phase18_kind`] returns `None` for a ring and for a
/// cake, so the resolved set can never be complete on a build with phase 18's twenty classes. What
/// comes back is [`Coverage::partial`], and a candidate that clears everything askable still comes
/// back `Unknown`.
///
/// `bounds` is `(kind slug, bounding box)` for every mask a caller resolved, in any order.
#[must_use]
pub fn coverage_from_masks(bounds: &[(String, Box2)]) -> Coverage {
    let mut regions: Vec<(Protected, Box2)> = Vec::new();
    let mut resolved = [false; Protected::COUNT];
    for kind in Protected::ALL {
        let Some(slug) = kind.phase18_kind() else {
            continue;
        };
        // A kind is resolved when the segmenter produced that class at all - including when it
        // produced it empty, which is what "looked and found nothing" means.
        if let Some(slot) = resolved.get_mut(kind.index()) {
            *slot = bounds.iter().any(|(name, _)| name == slug);
        }
        for (name, region) in bounds {
            if name == slug {
                regions.push((kind, *region));
            }
        }
    }
    Coverage::partial(regions, resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Box2 {
        Box2 { x, y, w, h }
    }

    #[test]
    fn a_build_with_every_phase_18_class_still_cannot_prove_a_ring_is_absent() {
        // The finding this phase made about phase 18's vocabulary, as a test. Even handed every
        // class phase 18 can produce, the coverage is incomplete, so a candidate that clears
        // everything askable is still `Unknown` rather than `Clear`.
        let bounds = vec![
            ("face".to_string(), rect(0.4, 0.2, 0.1, 0.1)),
            ("skin".to_string(), rect(0.4, 0.2, 0.2, 0.4)),
            ("dress".to_string(), rect(0.35, 0.4, 0.3, 0.5)),
        ];
        let coverage = coverage_from_masks(&bounds);
        assert!(coverage.is_known());
        assert!(!coverage.is_complete());
        assert_eq!(
            coverage.unresolved(),
            vec![Protected::Rings, Protected::Cake]
        );
    }

    #[test]
    fn hands_are_resolved_by_the_skin_class_because_every_hand_is_skin() {
        let bounds = vec![("skin".to_string(), rect(0.1, 0.1, 0.2, 0.2))];
        let coverage = coverage_from_masks(&bounds);
        let unresolved = coverage.unresolved();
        assert!(!unresolved.contains(&Protected::Hands));
        assert!(!unresolved.contains(&Protected::Skin));
        assert!(unresolved.contains(&Protected::Face));
    }

    #[test]
    fn no_masks_at_all_resolves_nothing() {
        let coverage = coverage_from_masks(&[]);
        assert_eq!(coverage.unresolved().len(), Protected::COUNT);
        assert!(!coverage.is_complete());
    }

    #[test]
    fn the_shipped_versions_load() {
        let (detector, analysis, policy) =
            Cleanup::current_versions().expect("the shipped policy table must load");
        assert_eq!(detector, crate::DETECTOR_VER);
        assert_eq!(analysis, crate::ANALYSIS_VER);
        assert!(policy > 0, "the shipped policy table must carry a version");
    }

    #[test]
    fn the_pass_reads_a_proxy_and_never_a_tile() {
        // Invariant 3, as a property of the level rather than as a comment: a tiled buffer is an
        // export path and this is a decision path.
        assert_eq!(level(), PixelLevel::Proxy2048);
    }
}
