//! The frozen `GalleryService`, and the pass that fills it.
//!
//! Two halves, the shape phases 06 to 24 all settled on. [`Gallery`] answers questions about a tree
//! that already exists and is what every later phase holds. [`ConsistencyPass`] builds one, and is
//! what the job graph holds.
//!
//! ## Resumability, invariant 5, and why this phase's is different
//!
//! Every phase from 06 to 24 makes the work remaining a *query over photographs*: which frames have
//! no row at these versions. This one cannot, and the reason is the shape of the thing rather than
//! a shortcut. **A delta is a statement about a node**, and a node whose first half was solved
//! against one target and whose second half was solved against another has a target that describes
//! neither. So resumability here is at the level of the pass: [`crate::store::GalleryStore::is_current`]
//! asks whether the stored tree came from this build's arithmetic and this policy table, and a run
//! that was killed half way through answers `false` and starts again.
//!
//! That is cheaper than it sounds and it is why section 11's budget is 60 s for a thousand images:
//! the pass reads numbers other phases already stored and opens no pixels of its own except through
//! the skin field.
//!
//! ## What survives a re-pass
//!
//! Everything a photographer said. [`crate::store::GalleryStore::take_decisions`] reads the pins,
//! the rejections, the overrides and the per-frame switches out before the tree is cleared, and
//! `restore_decisions` puts them back afterwards - onto whichever node the photograph now belongs
//! to, because the tree may have been re-shaped by a change point that was not there last time.
//!
//! ## Three tiers, invariant 3
//!
//! No pixels are opened for the tone half at all: it reads phase 15's and phase 16's stored rows.
//! The skin half opens a 2048 px proxy through the field, on the frames that have an
//! identity-scoped skin region - which in this build is none of them.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::contract::colour::ColourService;
use aura_core::contract::gallery::{
    GalleryCode, GalleryOutline, GalleryOverride, GalleryService, ImageId, NormalisationDelta,
    Outlier, SceneNode, SkinTarget, MAX_D_CCT_K, MAX_D_CONTRAST, MAX_D_EXPOSURE_EV,
    MAX_D_SATURATION, MAX_D_TINT,
};
use aura_core::contract::ids::NodeId;
use aura_core::contract::scene::StoryService;
use aura_core::contract::tone::ToneService;
use aura_core::progress::{CancelToken, ProgressSink, ProgressUpdate};
use aura_core::{AuraError, AuraResult, IdentityId, Priority, ProjectId};

use crate::policy::Consistency;
use crate::skin_consistency::{self, SkinField, TargetBuilder};
use crate::store::{Decisions, GalleryStore, NodeWrite};
use crate::tree::{Frame, RawNode};
use crate::{anchors, changepoint, errors, normalise, outlier, stats, tree};

/// The one implementation of the frozen service.
///
/// Holds a store and the policy table the stored rows were bounded by, and nothing else. It answers
/// questions; [`ConsistencyPass`] is what produces the rows it answers from.
#[derive(Debug, Clone)]
pub struct Gallery {
    store: GalleryStore,
    policy: Consistency,
}

impl Gallery {
    /// Wrap a catalog with the bundled policy table.
    #[must_use]
    pub fn new(catalog: Arc<Catalog>, clock: Arc<dyn Clock>) -> Self {
        Self {
            store: GalleryStore::new(catalog, clock),
            policy: Consistency::default(),
        }
    }

    /// Wrap a catalog with a studio's own table.
    #[must_use]
    pub fn with_policy(catalog: Arc<Catalog>, clock: Arc<dyn Clock>, policy: Consistency) -> Self {
        Self {
            store: GalleryStore::new(catalog, clock),
            policy,
        }
    }

    /// The store underneath, for the gate and the budget test.
    #[must_use]
    pub fn store(&self) -> &GalleryStore {
        &self.store
    }

    /// The policy table the stored rows were bounded by.
    #[must_use]
    pub fn policy(&self) -> &Consistency {
        &self.policy
    }
}

impl GalleryService for Gallery {
    fn outline(&self, project: ProjectId) -> AuraResult<GalleryOutline> {
        let mut outline = self.store.outline(project, self.policy.untargeted())?;
        // The two spreads cannot be recovered from a table of deltas - a disabled frame is in the
        // before and not the after - so a pass records them and an outline read without one carries
        // zeroes and a reduction of zero. `GalleryOutline::cct_spread_reduction` returns 0.0 on a
        // zero baseline rather than 1.0, which is what stops "we have not measured" reading as
        // "we removed all of it".
        if outline.spread_before_cct <= f32::EPSILON {
            outline.spread_before_cct = 0.0;
            outline.spread_after_cct = 0.0;
        }
        Ok(outline)
    }

    fn nodes(&self, project: ProjectId) -> AuraResult<Vec<SceneNode>> {
        self.store.nodes(project)
    }

    fn node(&self, node: NodeId) -> AuraResult<Option<SceneNode>> {
        self.store.node(node)
    }

    fn node_of(&self, image: ImageId) -> AuraResult<Option<NodeId>> {
        self.store.node_of(image)
    }

    fn delta(&self, image: ImageId) -> AuraResult<Option<NormalisationDelta>> {
        self.store.delta(image)
    }

    fn deltas_in(&self, node: NodeId) -> AuraResult<Vec<NormalisationDelta>> {
        self.store.deltas_in(node)
    }

    fn outliers(&self, project: ProjectId, limit: usize) -> AuraResult<Vec<Outlier>> {
        self.store.outliers(project, limit)
    }

    fn skin_target(&self, identity: IdentityId) -> AuraResult<Option<SkinTarget>> {
        self.store.skin_target(identity)
    }

    fn skin_targets(&self, project: ProjectId) -> AuraResult<Vec<SkinTarget>> {
        self.store.skin_targets(project)
    }

    fn pin_anchor(&self, node: NodeId, image: ImageId) -> Result<(), AuraError> {
        self.store.set_anchor(node, image, true)
    }

    fn reject_anchor(&self, node: NodeId, image: ImageId) -> Result<(), AuraError> {
        self.store.set_anchor(node, image, false)
    }

    fn set_override(&self, image: ImageId, values: GalleryOverride) -> Result<(), AuraError> {
        if values.is_empty() {
            return Err(aura_core::errors::ml::gallery_override_refused(
                "an override that sets nothing would take a frame out of automation without \
                 changing anything about it",
            ));
        }
        if !values.within_bounds() {
            return Err(aura_core::errors::ml::gallery_override_refused(format!(
                "a value is outside its bound: cct <= {MAX_D_CCT_K}, tint <= {MAX_D_TINT}, \
                 exposure <= {MAX_D_EXPOSURE_EV}, contrast <= {MAX_D_CONTRAST}, \
                 saturation <= {MAX_D_SATURATION}"
            )));
        }
        self.store.set_override(
            image,
            [
                values.d_exposure,
                values.d_cct,
                values.d_tint,
                values.d_contrast,
                values.d_saturation,
            ],
        )
    }

    fn set_enabled(&self, image: ImageId, enabled: bool) -> Result<(), AuraError> {
        self.store.set_enabled(image, enabled)
    }
}

/// Everything one pass needs that is not in the catalog.
///
/// The three services are the *only* way this crate asks another phase anything, which is what keeps
/// `aura-brain-photo` and `aura-brain-wedding` out of `Cargo.toml`. The skin field is optional and
/// its absence is a reason code rather than a fallback.
pub struct Context<'a> {
    /// Phase 07's chapters and scenes.
    pub story: &'a dyn StoryService,
    /// Phase 15's per-frame answer about the light.
    pub tone: &'a dyn ToneService,
    /// Phase 16's per-frame grade, or `None` when the project has not been graded.
    pub colour: Option<&'a dyn ColourService>,
    /// The route to a per-frame skin reading, or `None`.
    ///
    /// `None` is `GalleryCode::SkinMaskAbsent` on every frame, which is what this build does.
    pub skin: Option<&'a dyn SkinField>,
}

impl fmt::Debug for Context<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Context")
            .field("colour", &self.colour.is_some())
            .field("skin", &self.skin.is_some())
            .finish_non_exhaustive()
    }
}

/// What one pass did.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PassReport {
    /// Nodes built.
    pub nodes: u32,
    /// Nodes with a usable target.
    pub anchored: u32,
    /// Nodes a change point split.
    pub split: u32,
    /// Frames with a delta.
    pub normalised: u32,
    /// Frames still out of line.
    pub outliers: u32,
    /// Identities with a gallery skin target.
    pub skin_targets: u32,
    /// The within-node spread before, as (kelvin, stops).
    pub spread_before: (f32, f32),
    /// The within-node spread after.
    pub spread_after: (f32, f32),
    /// The photographer decisions that were carried across.
    pub decisions_kept: usize,
    /// How long it took, in milliseconds.
    pub elapsed_ms: u64,
}

impl PassReport {
    /// The share of the temperature spread that normalising removed, `0..1`.
    #[must_use]
    pub fn cct_reduction(&self) -> f32 {
        if self.spread_before.0 <= f32::EPSILON {
            return 0.0;
        }
        (1.0 - self.spread_after.0 / self.spread_before.0).clamp(0.0, 1.0)
    }

    /// The share of the exposure spread that normalising removed, `0..1`.
    #[must_use]
    pub fn ev_reduction(&self) -> f32 {
        if self.spread_before.1 <= f32::EPSILON {
            return 0.0;
        }
        (1.0 - self.spread_after.1 / self.spread_before.1).clamp(0.0, 1.0)
    }
}

/// Builds one project's tree and solves every frame in it.
#[derive(Debug, Clone)]
pub struct ConsistencyPass {
    store: GalleryStore,
    policy: Consistency,
    clock: Arc<dyn Clock>,
}

impl ConsistencyPass {
    /// A pass over one catalog, with the bundled policy table.
    #[must_use]
    pub fn new(catalog: Arc<Catalog>, clock: Arc<dyn Clock>) -> Self {
        Self {
            store: GalleryStore::new(catalog, Arc::clone(&clock)),
            policy: Consistency::default(),
            clock,
        }
    }

    /// A pass with a studio's own table.
    #[must_use]
    pub fn with_policy(catalog: Arc<Catalog>, clock: Arc<dyn Clock>, policy: Consistency) -> Self {
        Self {
            store: GalleryStore::new(catalog, Arc::clone(&clock)),
            policy,
            clock,
        }
    }

    /// The policy table this pass bounds movement by.
    #[must_use]
    pub fn policy(&self) -> &Consistency {
        &self.policy
    }

    /// Run over a project's frames.
    ///
    /// The frames are assembled by the caller from [`Context`], because a pass that queried the
    /// three services itself would make every test need three service implementations - and because
    /// `aura-app` already has all three in one place. [`collect_frames`] is the assembly, and it
    /// lives beside this rather than inside it so a caller may substitute its own.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5123` when the pass cannot complete, `AURA-DB-3006` when a statement fails, and
    /// `AURA-ML-5127` when the stored rows came from a different build and `force` is false.
    // One function, deliberately. Every step reads what the step before it produced - the tree feeds
    // the split, the split feeds the anchors, the anchors feed the target, the target feeds every
    // frame - and splitting it would mean passing eight intermediate collections between four
    // private functions that have no other caller. The modules are where this phase is decomposed.
    #[allow(clippy::too_many_lines)]
    pub fn run(
        &self,
        project: ProjectId,
        frames: &[Frame],
        skin: Option<&dyn SkinField>,
        progress: &dyn ProgressSink,
        cancel: &CancelToken,
    ) -> AuraResult<PassReport> {
        let started = self.clock.monotonic_ms();
        let mut report = PassReport::default();

        let decisions = self.store.take_decisions(project)?;
        report.decisions_kept = decisions.len();

        // Every frame a photographer touched keeps what they said, whatever the tree does next.
        let frames = apply_decisions(frames, &decisions);

        let raw = tree::build(&frames);
        let mut nodes: Vec<RawNode> = Vec::new();
        for node in &raw {
            if cancel.is_cancelled() {
                return Err(errors::pass_failed("cancelled while building the tree"));
            }
            nodes.extend(changepoint::split(node, self.policy.split_sigma));
        }
        report.nodes = nodes.len().min(u32::MAX as usize) as u32;
        report.split = nodes
            .iter()
            .filter(|node| node.parent.is_some())
            .count()
            .min(u32::MAX as usize) as u32;

        // The skin targets are accumulated over the whole project before any frame is corrected,
        // because a target is a gallery-level fact and a per-node one would give the same person a
        // different appearance in each chapter - which is the drift this phase exists to remove.
        let mut builder = TargetBuilder::new();
        if let Some(field) = skin {
            for frame in &frames {
                for reading in field.readings(frame.image) {
                    builder.add(reading);
                }
            }
        }
        let targets = builder.finish(crate::ANALYSIS_VER);
        let mut targets: BTreeMap<IdentityId, SkinTarget> = targets;
        report.skin_targets = targets.len().min(u32::MAX as usize) as u32;

        let mut writes: Vec<NodeWrite> = Vec::with_capacity(nodes.len());
        let mut before_cct: Vec<f32> = Vec::new();
        let mut after_cct: Vec<f32> = Vec::new();
        let mut before_ev: Vec<f32> = Vec::new();
        let mut after_ev: Vec<f32> = Vec::new();
        let mut corrections: BTreeMap<IdentityId, BTreeMap<ImageId, _>> = BTreeMap::new();

        let total = nodes.len().max(1);
        for (index, node) in nodes.iter().enumerate() {
            if cancel.is_cancelled() {
                return Err(errors::pass_failed("cancelled while solving"));
            }
            progress.report(ProgressUpdate {
                stage: crate::STAGE,
                done: index as u64,
                total: total as u64,
                current: None,
            });

            let scene_policy = self.policy.scene(node.scene);
            let anchored = anchors::select(
                node,
                scene_policy,
                &decisions.pinned,
                &decisions.rejected,
                self.policy.target_anchors,
            );
            let confidence = anchors::target_confidence(&anchored);
            let usable = anchored.is_anchored();
            if usable {
                report.anchored += 1;
            }

            // The spread this node contributes to the headline gate. Measured over the frames that
            // could be normalised, so a node of untouched intentional light does not report a
            // reduction it never made.
            let movable: Vec<&Frame> = node
                .frames
                .iter()
                .filter(|frame| frame.blocked_by().is_none() && frame.has_tone())
                .collect();
            if movable.len() >= 2 {
                let ccts: Vec<f32> = movable.iter().filter_map(|f| f.cct_k).collect();
                let evs: Vec<f32> = movable
                    .iter()
                    .filter_map(|f| f.subject_luma)
                    .map(|luma| normalise::stops_between(0.45, luma))
                    .collect();
                before_cct.push(stats::mean_abs_deviation(&ccts));
                before_ev.push(stats::mean_abs_deviation(&evs));
            }

            let mut deltas = Vec::with_capacity(node.frames.len());
            let mut outliers = Vec::new();
            let mut moved_cct: Vec<f32> = Vec::new();
            let mut moved_ev: Vec<f32> = Vec::new();

            for frame in &node.frames {
                let mut solved = match anchored.target {
                    Some(target) if usable => normalise::solve(
                        frame,
                        node.id,
                        &target,
                        scene_policy,
                        &self.policy,
                        confidence,
                    ),
                    _ => normalise::Solved {
                        delta: unanchored_delta(frame, node.id, scene_policy.damping),
                        residual: outlier::zero_residual(),
                    },
                };

                // The skin half. Its three outcomes are three codes and never one, because "could
                // not look", "looked and did not know" and "looked, knew and moved something" are
                // different facts. Phase 24's rule.
                let readings = skin
                    .map(|field| field.readings(frame.image))
                    .unwrap_or_default();
                let mine = readings
                    .iter()
                    .find(|reading| targets.contains_key(&reading.identity))
                    .copied()
                    .or_else(|| readings.first().copied());
                let correction = match (mine, scene_policy.correct_skin) {
                    (Some(reading), true) => targets
                        .get(&reading.identity)
                        .and_then(|target| skin_consistency::correct(&reading, target)),
                    _ => None,
                };
                let code = skin_consistency::code_for(
                    !readings.is_empty(),
                    mine.is_some_and(|r| targets.contains_key(&r.identity)),
                    correction.is_some(),
                    correction
                        .as_ref()
                        .is_some_and(skin_consistency::is_skin_outlier),
                );
                if let (Some(reading), Some(correction)) = (mine, correction) {
                    corrections
                        .entry(reading.identity)
                        .or_default()
                        .insert(frame.image, correction);
                }
                normalise::with_skin(&mut solved, correction, code);

                if let Some(found) = outlier::detect(
                    &solved,
                    node.id,
                    &self.policy,
                    !usable && !anchored.anchors.is_empty(),
                ) {
                    outliers.push(found);
                }

                if !solved.delta.is_zero() {
                    moved_cct.push(solved.delta.from_cct_k + solved.delta.d_cct);
                    if let Some(luma) = frame.subject_luma {
                        moved_ev
                            .push(normalise::stops_between(0.45, luma) + solved.delta.d_exposure);
                    }
                } else if frame.blocked_by().is_none() {
                    if let Some(cct) = frame.cct_k {
                        moved_cct.push(cct);
                    }
                    if let Some(luma) = frame.subject_luma {
                        moved_ev.push(normalise::stops_between(0.45, luma));
                    }
                }

                deltas.push(solved.delta);
            }

            if moved_cct.len() >= 2 {
                after_cct.push(stats::mean_abs_deviation(&moved_cct));
            }
            if moved_ev.len() >= 2 {
                after_ev.push(stats::mean_abs_deviation(&moved_ev));
            }

            report.normalised += deltas.len().min(u32::MAX as usize) as u32;
            report.outliers += outliers.len().min(u32::MAX as usize) as u32;

            writes.push(NodeWrite {
                node: SceneNode {
                    id: node.id,
                    parent: node.parent,
                    segment_id: node.segment,
                    label: node.label(chapter_label(node)),
                    image_ids: node.image_ids(),
                    anchors: anchored.anchors.iter().map(|c| c.image).collect(),
                    target: if usable { anchored.target } else { None },
                    scene: node.scene,
                    reasons: node
                        .reasons
                        .iter()
                        .chain(anchored.reasons.iter())
                        .copied()
                        .map(aura_core::contract::gallery::GalleryReason::of)
                        .collect(),
                    analysis_ver: crate::ANALYSIS_VER,
                    policy_ver: self.policy.version,
                },
                anchors: anchored
                    .anchors
                    .iter()
                    .map(|c| (c.image, c.quality))
                    .collect(),
                deltas,
                outliers,
                first_ts: format_ms(node.first_ts()),
            });
        }

        // Section 6.3's promise is about the corrected gallery, so every target's `spread_after` is
        // re-measured with the corrections applied rather than predicted from the caps.
        if let Some(field) = skin {
            for (identity, target) in &mut targets {
                let readings: Vec<_> = frames
                    .iter()
                    .flat_map(|frame| field.readings(frame.image))
                    .filter(|reading| reading.identity == *identity)
                    .collect();
                let empty = BTreeMap::new();
                let applied = corrections.get(identity).unwrap_or(&empty);
                skin_consistency::measure_after(target, &readings, applied);
            }
        }

        self.store.write_pass(project, &writes, &targets)?;
        self.store.restore_decisions(project, &decisions)?;

        report.spread_before = (mean(&before_cct), mean(&before_ev));
        report.spread_after = (mean(&after_cct), mean(&after_ev));
        report.elapsed_ms = self.clock.monotonic_ms().saturating_sub(started);
        progress.report(ProgressUpdate {
            stage: crate::STAGE,
            done: total as u64,
            total: total as u64,
            current: None,
        });
        Ok(report)
    }

    /// Whether a project's stored tree came from this build and this policy table.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn is_current(&self, project: ProjectId) -> AuraResult<bool> {
        self.store
            .is_current(project, (crate::ANALYSIS_VER, self.policy.version))
    }

    /// Raise `AURA-ML-5127` when the stored rows came from a different build.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5127` when the versions differ, `AURA-DB-3006` when the query fails.
    pub fn check_versions(&self, project: ProjectId) -> AuraResult<()> {
        let stored = self.store.stored_versions(project)?;
        crate::store::check_versions(stored, (crate::ANALYSIS_VER, self.policy.version))
    }

    /// The priority a consistency pass runs at.
    ///
    /// Background, like every analysis pass since phase 06. Nothing a photographer is looking at
    /// depends on it finishing, and section 11's 60 s budget is a wall-clock figure for a thousand
    /// images rather than an interactive one.
    #[must_use]
    pub const fn priority() -> Priority {
        Priority::Background
    }
}

/// Apply a photographer's stored decisions to the frames a pass is about to solve.
///
/// The overrides are not applied here: an override is what a photographer set *instead of* a
/// delta, so the pass still solves the frame - marking it `user_edited` so the solver produces
/// five zeroes - and `restore_decisions` writes their values back over the row afterwards. That
/// keeps AURA's own numbers on the row beside the photographer's, which is what lets a review
/// queue show a disagreement and phase 30's learning loop read one. Phase 15's rule, fourth
/// application.
#[must_use]
pub fn apply_decisions(frames: &[Frame], decisions: &Decisions) -> Vec<Frame> {
    frames
        .iter()
        .map(|frame| {
            let mut frame = frame.clone();
            if decisions.overrides.contains_key(&frame.image) {
                frame.user_edited = true;
            }
            if decisions.disabled.contains(&frame.image) {
                frame.enabled = false;
            }
            frame
        })
        .collect()
}

/// A zero delta for a frame in a node nothing could be anchored to.
///
/// **Not the same row as a frame that needed nothing.** Five zeroes and
/// `GalleryCode::NodeUnanchored`, which is a different code, a different sentence and a different
/// runbook from `GalleryCode::AlreadyConsistent`. ADR-0051 section 3 and the contract's own header.
#[must_use]
pub fn unanchored_delta(frame: &Frame, node: NodeId, damping: f32) -> NormalisationDelta {
    NormalisationDelta {
        image_id: frame.image,
        node_id: node,
        d_exposure: 0.0,
        d_cct: 0.0,
        d_tint: 0.0,
        d_contrast: 0.0,
        d_saturation: 0.0,
        skin_correction: None,
        from_exposure_ev: frame.exposure_ev.unwrap_or(0.0),
        from_cct_k: frame.cct_k.unwrap_or(0.0),
        from_tint: frame.tint.unwrap_or(0.0),
        damping,
        bounded_by: None,
        reasons: vec![aura_core::contract::gallery::GalleryReason::of(
            frame.blocked_by().unwrap_or(GalleryCode::NodeUnanchored),
        )],
        // Zero, and deliberately: the product has no opinion about this frame at all, and any other
        // number would be an opinion about a target that does not exist.
        confidence: 0.0,
        user_edited: frame.user_edited,
        analysis_ver: crate::ANALYSIS_VER,
        policy_ver: 0,
    }
}

/// The chapter name a node's label is built from.
///
/// The scene's own chapter title, from phase 07's own `ChapterId::title`. Phase 07 owns what a
/// chapter is called and a photographer may have renamed it, so a caller with the segment in hand
/// should pass its label instead - which is what `aura-app` does. This is the fallback for a caller
/// that has only the frames, and it goes through the chapter rather than inventing a name per scene
/// because a node of a wedding is a part of a chapter and "Getting Ready (2 of 3)" is what a
/// photographer expects to read.
#[must_use]
pub fn chapter_label(node: &RawNode) -> &'static str {
    node.scene.chapter().title()
}

fn mean(values: &[f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f32>() / values.len() as f32
}

/// A node's first capture time as text, for the column the node list is ordered by.
///
/// Milliseconds since the project's own epoch, zero-padded so a text sort is a time sort. The
/// column is `TEXT` because every timestamp in this catalog is, and a mixed-type ordering column
/// would sort differently in `SQLite` than in Rust.
#[must_use]
pub fn format_ms(ms: i64) -> String {
    format!("{:016}", ms.max(0))
}

/// Assemble one project's frames from the three services.
///
/// The one place this crate reads another phase's output, and it reads all of it through the frozen
/// traits. A frame with no scene, no segment or no tone estimate is still returned - with its
/// fields `None` - because a frame that vanished here would be a gap in coverage nobody could see.
///
/// # Errors
///
/// Whatever the three services raised.
pub fn collect_frames(
    project: ProjectId,
    images: &[(ImageId, i64)],
    context: &Context<'_>,
) -> AuraResult<Vec<Frame>> {
    collect_frames_with_camera(project, images, context, &crate::camera::Field::empty())
}

/// The same, with phase 26's camera corrections folded in first.
///
/// **This is PHASE-26 section 6.4's ordering, and it is a data dependency rather than a
/// convention.** Every camera transform is applied to a frame's readings *before* the frame reaches
/// [`tree::build`], so this pass's nodes, its change points, its anchors and its targets are all
/// computed over numbers that are already comparable across bodies. Reversing the two produces a
/// gallery in which every node's target is the average of two brands' colour science and every
/// frame is normalised toward a look neither camera can produce.
///
/// An empty field is exactly what this function did before phase 26 existed, which is what makes
/// that phase additive rather than a change to this one.
///
/// # Errors
///
/// Whatever the three frozen services return.
pub fn collect_frames_with_camera(
    project: ProjectId,
    images: &[(ImageId, i64)],
    context: &Context<'_>,
    camera: &crate::camera::Field,
) -> AuraResult<Vec<Frame>> {
    let story = context.story.outline(project)?;
    let mut frames = Vec::with_capacity(images.len());
    for (image, timeline_ms) in images {
        let segment = context.story.segment_of(*image)?;
        let Some(segment) = segment else {
            // No chapter, no node. `GalleryCode::SegmentAbsent` is what the outline reports, and
            // the frame is left out of the tree rather than put in a node of unrelated rooms.
            continue;
        };
        let scene = context
            .story
            .scene(*image)?
            .map_or(segment.dominant_scene, |result| result.scene);
        let estimate = context.tone.of_image(*image)?;
        let decision = match context.colour {
            Some(colour) => colour.of_image(*image)?,
            None => None,
        };
        let dominant = estimate.as_ref().and_then(|e| {
            e.dominant_on_subject
                .and_then(|index| e.illuminants.get(index).copied())
        });
        frames.push(Frame {
            image: *image,
            segment: segment.id,
            scene,
            timeline_ms: *timeline_ms,
            cct_k: estimate.as_ref().map(|e| e.temperature_k),
            tint: estimate.as_ref().map(|e| e.tint),
            exposure_ev: estimate.as_ref().map(|e| e.exposure_ev),
            subject_luma: estimate.as_ref().map(|e| e.subject_luma_target),
            wb_conf: estimate.as_ref().map_or(0.0, |e| e.wb_conf),
            exposure_conf: estimate.as_ref().map_or(0.0, |e| e.exposure_conf),
            mixed_light: estimate.as_ref().is_some_and(|e| e.mixed_light),
            // Read straight off phase 15's own `IlluminantKind::is_intentional`. This phase does not
            // get a second opinion about whether a purple dance floor is meant to be purple.
            intentional_light: dominant.is_some_and(|light| light.kind.is_intentional()),
            mood: dominant.map_or(0.0, |light| {
                if light.kind.is_intentional() {
                    light.weight.clamp(0.0, 1.0)
                } else {
                    0.0
                }
            }),
            contrast: decision.as_ref().map(|d| d.contrast),
            saturation: decision.as_ref().map(|d| d.saturation),
            signature: decision.as_ref().map(signature_of),
            identities: BTreeMap::new(),
            user_edited: false,
            enabled: true,
        });
    }
    let _ = story;
    // PHASE-26 section 6.4. The one place a camera correction reaches this pass, and it is here -
    // at the end of assembly and before anything is grouped - rather than inside the loop above,
    // so that a reader of that loop is never in doubt about whether a field is being consulted per
    // frame in a way that could differ between frames of one body.
    camera.apply_to_gallery_frames(&mut frames);
    Ok(frames)
}

/// The eight-number colour character of one graded frame.
///
/// Assembled from phase 16's own stored readings rather than re-measured, which is the whole reason
/// `ColourDecision::bands` exists on the row: this crate opens no pixels for the grade half.
#[must_use]
pub fn signature_of(decision: &aura_core::contract::colour::ColourDecision) -> [f32; 8] {
    use aura_core::contract::colour::HslBand;
    let shadow = decision.shadows;
    let highlight = decision.highlights;
    // The hue and chroma of the two ends come from the HSL band shifts phase 16 solved: the warm
    // bands describe what happened to the shadows of a wedding frame and the cool ones the
    // highlights, which is a simplification and is stated as one. What matters for this phase is
    // that two frames graded alike produce the same eight numbers and two graded differently do
    // not - a signature is compared, never applied.
    let warm = average_shift(
        &decision.hsl,
        &[HslBand::Red, HslBand::Orange, HslBand::Yellow],
    );
    let cool = average_shift(
        &decision.hsl,
        &[HslBand::Blue, HslBand::Aqua, HslBand::Purple],
    );
    stats::GradeSignature::new(
        warm.0,
        warm.1,
        cool.0,
        cool.1,
        (warm.2 / 100.0).abs().clamp(0.0, 1.0),
        (cool.2 / 100.0).abs().clamp(0.0, 1.0),
        (decision.contrast / 100.0).clamp(-1.0, 1.0),
        ((shadow - highlight) / 200.0 + 0.5).clamp(0.0, 1.0),
    )
    .values
}

fn average_shift(
    hsl: &aura_core::contract::colour::HslAdjustments,
    bands: &[aura_core::contract::colour::HslBand],
) -> (f32, f32, f32) {
    let mut hues = Vec::new();
    let mut sats = Vec::new();
    for band in bands {
        let index = *band as usize;
        if let Some(shift) = hsl.bands.get(index) {
            hues.push(shift.h);
            sats.push(shift.s);
        }
    }
    let hue = stats::median(&hues).unwrap_or(0.0);
    let spread = stats::mean_abs_deviation(&hues);
    let sat = stats::median(&sats).unwrap_or(0.0);
    (hue, spread, sat)
}

/// Every identity in a project that has a gallery skin target, for the panel's header.
#[must_use]
pub fn targeted_identities(targets: &[SkinTarget]) -> BTreeSet<IdentityId> {
    targets
        .iter()
        .filter(|target| target.is_usable())
        .map(|target| target.identity)
        .collect()
}
