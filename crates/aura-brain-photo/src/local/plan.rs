//! One decoded frame in, one local light plan out.
//!
//! PHASE-19 section 3's data flow, as one function, and section 8's implementation order, in
//! that order. The pixels are read **once** - [`crate::local::measure::FrameMeasure`] carries
//! every statistic the rest of the phase needs - and each face is decomposed at most once.
//! Section 11 budgets 80 ms per image and that is only reachable because of it.
//!
//! ## The order, and why it is this order
//!
//! 1. **Gate on masks first.** Every operation here is local, so the first question is which
//!    of them can run at all. Doing this first means the expensive work is never done for an
//!    operation that was going to be gated anyway.
//! 2. **Face lighting.** The highest-priority operation and the one section 1 exists for.
//!    Solved jointly across every face in the frame.
//! 3. **The paired subject/background move.** After the faces, because the mean luminance it
//!    holds is measured before any local work and the faces' own contribution has to be in it.
//! 4. **Shine.** Before the shaping, because a hot spot found on a forehead is a region the
//!    shaping should not lift again.
//! 5. **Frequency separation and shaping.** The most expensive step and the lowest priority,
//!    so it runs last and is the first thing the governor gives up.
//! 6. **The governor.** Every operation has now said what it would cost; the allowance is
//!    allocated in priority order and the deltas are scaled to what they were allowed.
//! 7. **Reasons and confidence.** Assembled from what happened rather than from what was
//!    intended, which is why they are last.
//!
//! ## The placeholder, said once and plainly
//!
//! The shipped `local_light_targets` head is **untrained**, for the reason every model since
//! phase 05 has been: section 8's step 1 is "extract local-adjustment behaviour from expert
//! edits (difference maps between baseline-graded and final images)", and there is no corpus
//! of expert edit pairs in this repository.
//!
//! The consequence is handled structurally rather than promised. [`TARGET_HEAD_TRAINED`] is
//! false, so [`Analyser::learned_targets`] returns `None` and the learned targets are
//! **never consulted**. What runs instead is the reference model: phase 15's own per-scene
//! luminance bands, which are a real product decision with a written reason per row, and the
//! plan carries [`aura_core::contract::local::LocalCode::TargetHeadUnavailable`] so nobody
//! mistakes the one for the other. This is condition C2 of
//! `docs/progress/PHASE-19-EXIT.md`.

use aura_core::contract::composition::CompositionResult;
use aura_core::contract::error::AuraError;
use aura_core::contract::integrity::CropRect;
use aura_core::contract::local::{
    BackgroundBalanceDelta, FaceLightDelta, ImageId, LocalCode, LocalLightPlan, LocalOp,
    LocalReason, MaskField, MaskKind, ShineReduction, SubjectEnhanceDelta, MAX_SHAPED_FACES,
    MIN_SHAPEABLE_FACE,
};
use aura_core::contract::people::FaceRef;
use aura_core::{IdentityId, SceneId};
use aura_raw::contract::pixels::{PixelBuffer, PixelLevel};

use crate::local::dodgeburn::{self, SHAPING_VER};
use crate::local::governor::{self, Ledger};
use crate::local::measure::FrameMeasure;
use crate::local::policy::{PolicyTable, ScenePolicy, UNPOLICIED_CONFIDENCE_PENALTY};
use crate::local::{background, face_light, freqsep, shine, subject};

/// Which build's arithmetic produced a plan.
///
/// Bumped on any change to a measurement, a trigger, a cap, a cost model or the way the
/// confidence is combined. It is written into `local_light_plan.analysis_ver`, and two plans
/// made under different values of it are not comparable: `AURA-ML-5066` exists so that
/// comparison never happens silently.
pub const ANALYSIS_VER: u16 = 1;

/// The pixel rung the local pass reads.
///
/// Tier 2, the 2048 px proxy, and here the choice *is* about detail. A face's mid-frequency
/// band is the whole basis of section 6.3, and a mid-frequency band measured on a 512 px
/// thumbnail is the low-frequency band of a face measured on a proxy - the separation would
/// silently be between form and form.
///
/// It is also the rung phases 06, 09, 11 and 15 already read, so the cost is a cache hit
/// rather than a decode.
pub const LOCAL_LEVEL: PixelLevel = PixelLevel::Proxy2048;

/// The version stamped on every `local_light_plan.model_ver`.
///
/// One head, so one number. Phases 09, 11 and 15 all fold their heads into a single version
/// for the same reason: no consumer of a plan cares *which* head moved, only that the numbers
/// are not comparable across the move.
pub const MODEL_VER: u16 = 100;

/// Whether the pinned local-target weights have passed the section 10.1 gate.
///
/// A compile-time release assertion rather than an inference-success check, exactly as phase
/// 11's `KEYPOINT_HEAD_TRAINED` and phase 15's `WB_HEAD_TRAINED` are. While this is false the
/// learned targets are never consulted at all and the reference model runs instead.
pub const TARGET_HEAD_TRAINED: bool = false;

/// Everything that is known about a frame before its pixels are read.
///
/// All of it comes through a frozen service - `PeopleService` for the faces,
/// `StoryService` for the scene, `IntegrityService` for the noise, `CompositionService` for
/// what is behind the subject and `ToneService` for the band. This phase measures none of
/// them again, which is the rule ten phases have now written and the reason
/// `aura-brain-photo` still depends on no other brain crate.
#[derive(Debug, Clone, Default)]
pub struct FrameContext {
    /// What the photograph is of. Invariant 7.
    pub scene: SceneId,
    /// The faces phase 06 found, in prominence order.
    pub faces: Vec<FaceRef>,
    /// The masks phase 18 generated.
    ///
    /// **Empty on a build with no phase 18**, which is the honest state of this repository.
    /// Every operation is then gated and the plan says so.
    pub masks: Vec<MaskField>,
    /// How noisy phase 09 measured this frame to be, `0..1`.
    pub noise: f32,
    /// The scene's shadow-lift scale from phase 15's exposure target table.
    pub shadow_scale: f32,
    /// The scene's subject luminance band centre from phase 15's table, `0..1`.
    ///
    /// The same number phase 15's global exposure was set against, so a face phase 15 already
    /// put in the band arrives here needing nothing.
    pub band: f32,
    /// What phase 11 found behind the subject.
    pub composition: Option<CompositionResult>,
    /// Whether local light sculpting is switched on for this project.
    ///
    /// The kill switch hard rule 8 requires. False produces a plan that does nothing and says
    /// [`LocalCode::LocalDisabled`], rather than no plan at all - because a frame with no plan
    /// and a frame the photographer switched off look identical in a coverage report.
    pub enabled: bool,
}

impl FrameContext {
    /// A context with the defaults a frame nobody has analysed would have.
    #[must_use]
    pub fn new(scene: SceneId) -> Self {
        Self {
            scene,
            faces: Vec::new(),
            masks: Vec::new(),
            noise: 0.0,
            shadow_scale: 1.0,
            band: 0.48,
            composition: None,
            enabled: true,
        }
    }

    /// The field of one kind, for one identity when it names one.
    #[must_use]
    pub fn mask(&self, kind: MaskKind, identity: Option<IdentityId>) -> Option<&MaskField> {
        self.masks
            .iter()
            .find(|m| m.kind == kind && (identity.is_none() || m.identity == identity))
            .or_else(|| self.masks.iter().find(|m| m.kind == kind))
    }
}

/// One frame's answer.
#[derive(Debug, Clone)]
pub struct FrameOutcome {
    /// The plan.
    pub plan: LocalLightPlan,
    /// Anything worth telling a support engineer. Never fatal; a warning here is a gated
    /// operation rather than a failure.
    pub warnings: Vec<AuraError>,
}

/// One frame in, one plan out.
#[derive(Debug, Clone)]
pub struct Analyser {
    policy: PolicyTable,
}

impl Analyser {
    /// Build an analyser over a policy table.
    #[must_use]
    pub fn new(policy: PolicyTable) -> Self {
        Self { policy }
    }

    /// The table this analyser reads.
    #[must_use]
    pub fn policy(&self) -> &PolicyTable {
        &self.policy
    }

    /// The learned per-scene targets, when a trained head exists.
    ///
    /// Always `None` on this build. See the module header: an untrained head is not consulted
    /// at all rather than consulted and ignored, because a head that is called and discarded
    /// is a head somebody eventually stops discarding.
    #[must_use]
    pub fn learned_targets(&self, _scene: SceneId) -> Option<[f32; LocalOp::COUNT]> {
        // Deliberately `None` in both arms. When a trained head ships, this is the one place
        // that changes; until then the branch documents that the decision is about training
        // rather than about availability.
        None
    }

    /// Plan one photograph.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn analyse(&self, pixels: &PixelBuffer, image: ImageId, ctx: &FrameContext) -> FrameOutcome {
        let policy = self.policy.get(ctx.scene);
        let unpolicied = !policy.measured;
        let mut reasons: Vec<LocalReason> = Vec::new();
        let mut warnings: Vec<AuraError> = Vec::new();
        let mut gated: Vec<(LocalOp, MaskKind)> = Vec::new();

        if !ctx.enabled {
            return FrameOutcome {
                plan: LocalLightPlan::nothing(
                    image,
                    ctx.scene,
                    LocalReason::plain(LocalCode::LocalDisabled, 0.0),
                ),
                warnings,
            };
        }

        let Some(rgb) = pixels.as_srgb8() else {
            return FrameOutcome {
                plan: LocalLightPlan::nothing(
                    image,
                    ctx.scene,
                    LocalReason::plain(LocalCode::MaskUnavailable, -0.20),
                ),
                warnings,
            };
        };
        let frame = FrameMeasure::of(rgb, pixels.width as usize, pixels.height as usize);

        // 1. Which operations can run at all.
        let mut scale = [0.0f32; LocalOp::COUNT];
        for op in LocalOp::PRIORITY {
            let kind = op.requires();
            if policy.declines(op) {
                continue;
            }
            let Some(field) = ctx.mask(kind, None) else {
                gated.push((op, kind));
                continue;
            };
            if let Err(err) = crate::local::guard::check_mask(field) {
                warnings.push(err);
                gated.push((op, kind));
                continue;
            }
            let quality = field.strength_scale();
            if quality <= 0.0 {
                gated.push((op, kind));
                warnings.push(crate::errors::mask_unusable(
                    kind.as_str(),
                    "the confidence is below the floor, so the operation was skipped",
                ));
                continue;
            }
            if let Some(slot) = scale.get_mut(op.rank()) {
                *slot = quality;
            }
        }
        if !gated.is_empty() {
            reasons.push(LocalReason::plain(LocalCode::MaskUnavailable, -0.25));
        }
        if scale
            .iter()
            .any(|s| *s > 0.0 && *s < 1.0)
        {
            reasons.push(LocalReason::plain(LocalCode::MaskWeak, -0.08));
        }

        // 2. Face lighting.
        let short_side = pixels.width.min(pixels.height).max(1) as f32;
        let caps = face_light::Caps::from_noise(
            ctx.noise,
            ctx.shadow_scale,
            policy.max_face_lift_ev,
        );
        let face_inputs = Self::face_inputs(&frame, ctx, at(&scale, LocalOp::FaceLight));
        let solved = face_light::solve(
            &face_inputs,
            ctx.band,
            &caps,
            policy.strength(LocalOp::FaceLight),
        );
        let face_area: f32 = ctx.faces.iter().map(|f| f.area_frac).sum::<f32>().min(1.0);

        // 3. The paired subject/background move.
        let subject_stats = ctx
            .mask(MaskKind::Subject, None)
            .filter(|f| f.is_usable())
            .map(|f| frame.region(f));
        let background_stats = background::stats(&frame, ctx.mask(MaskKind::Background, None));
        let blobs = background::bright_blobs(ctx.composition.as_ref());
        let paired = match (subject_stats, background_stats) {
            (Some(subject_region), Some(background_region)) => {
                let competition =
                    subject::Competition::measure(subject_region, background_region, blobs);
                let mask_scale = at(&scale, LocalOp::SubjectEnhance)
                    .min(at(&scale, LocalOp::BackgroundBalance));
                if !competition.is_competing() {
                    reasons.push(LocalReason::plain(LocalCode::NoCompetitionMeasured, 0.0));
                }
                subject::pair(
                    &competition,
                    policy.strength(LocalOp::SubjectEnhance),
                    policy.strength(LocalOp::BackgroundBalance),
                    mask_scale,
                    frame.mean_luma,
                )
            }
            _ => None,
        };

        // 4. Shine.
        let shine_result =
            Self::find_shine(&frame, ctx, &policy, at(&scale, LocalOp::ShineControl));
        match &shine_result {
            ShineOutcome::None => reasons.push(LocalReason::plain(LocalCode::NoShineFound, 0.0)),
            ShineOutcome::TooLarge(rect) => reasons.push(LocalReason::plain_at(
                LocalCode::ShineTooLargeToBeSheen,
                0.0,
                *rect,
            )),
            ShineOutcome::Reduced(_) => {}
        }

        // 5. Shaping.
        let (maps, shaping_notes) = Self::shape(&frame, ctx, &policy, &scale, short_side);
        reasons.extend(shaping_notes);

        // 6. The governor.
        let mut face_deltas = solved.deltas.clone();
        let mut subject_delta = paired.map_or(SubjectEnhanceDelta::NONE, |p| p.subject);
        let mut background_delta = paired.map_or(BackgroundBalanceDelta::NONE, |p| p.background);
        let mut shine_delta = match shine_result {
            ShineOutcome::Reduced(reduction) => Some(reduction),
            _ => None,
        };
        let mut maps = maps;

        let costs = [
            governor::face_cost(&face_deltas, face_area),
            governor::subject_cost(&subject_delta, subject_area(&frame, ctx)),
            governor::background_cost(&background_delta, background_delta_area(&frame, ctx)),
            shine_delta.as_ref().map_or(0.0, governor::shine_cost),
            maps.as_ref()
                .map_or(0.0, |m| governor::shaping_cost(m, face_area, true)),
            maps.as_ref()
                .map_or(0.0, |m| governor::shaping_cost(m, face_area, false)),
        ];
        let ledger = governor::allocate(costs, policy.budget);
        apply_ledger(
            &ledger,
            &mut face_deltas,
            &mut subject_delta,
            &mut background_delta,
            &mut shine_delta,
            &mut maps,
        );
        if ledger.exhausted {
            reasons.push(LocalReason::plain(LocalCode::BudgetExhausted, -0.05));
        }

        // 7. Reasons and confidence.
        Self::explain(
            &mut reasons,
            &solved,
            &face_deltas,
            &subject_delta,
            &background_delta,
            shine_delta.as_ref(),
            paired.is_some_and(|p| p.held_mean_luma),
            paired.is_some_and(|p| p.chroma_led),
        );
        if unpolicied {
            reasons.push(LocalReason::plain(LocalCode::SceneStrengthLimited, 0.0));
            warnings.push(crate::errors::scene_unpolicied(ctx.scene.as_str()));
        }
        if !TARGET_HEAD_TRAINED {
            reasons.push(LocalReason::plain(LocalCode::TargetHeadUnavailable, 0.0));
        }
        if reasons.is_empty() {
            reasons.push(LocalReason::plain(LocalCode::FaceAlreadyInBand, 0.0));
        }
        // Invariant 2 and migration 16's own CHECK: at most eight reasons. Keep the doubts
        // first, because a truncated list that dropped the doubts would read as a confident
        // plan.
        reasons.sort_by(|a, b| a.weight.total_cmp(&b.weight));
        reasons.truncate(8);

        let strengths = [
            if face_deltas.iter().any(|d| !d.is_noop()) {
                policy.strength(LocalOp::FaceLight) * ledger.allowed(LocalOp::FaceLight)
            } else {
                0.0
            },
            if subject_delta.is_noop() {
                0.0
            } else {
                policy.strength(LocalOp::SubjectEnhance) * ledger.allowed(LocalOp::SubjectEnhance)
            },
            if background_delta.is_noop() {
                0.0
            } else {
                policy.strength(LocalOp::BackgroundBalance)
                    * ledger.allowed(LocalOp::BackgroundBalance)
            },
            if shine_delta.is_some() {
                policy.strength(LocalOp::ShineControl) * ledger.allowed(LocalOp::ShineControl)
            } else {
                0.0
            },
            maps.as_ref().map_or(0.0, |m| {
                if m.faces.iter().any(|f| !f.zones.is_empty()) {
                    policy.strength(LocalOp::DodgeBurnLow) * ledger.allowed(LocalOp::DodgeBurnLow)
                } else {
                    0.0
                }
            }),
            maps.as_ref().map_or(0.0, |m| {
                if m.faces.iter().any(|f| f.evening > 0.0) {
                    policy.strength(LocalOp::DodgeBurnMid) * ledger.allowed(LocalOp::DodgeBurnMid)
                } else {
                    0.0
                }
            }),
        ];

        let confidence = confidence_of(&reasons, unpolicied, &gated);
        let plan = LocalLightPlan {
            image_id: image,
            face_light: identities(ctx).zip(face_deltas).collect(),
            subject: subject_delta,
            background: background_delta,
            dodge_burn: maps,
            shine: shine_delta,
            total_budget_used: ledger.spent,
            gated_by_mask_quality: gated,
            reasons,
            confidence,
            scene: ctx.scene,
            strengths,
            user_edited: false,
            reviewed: false,
            model_ver: MODEL_VER,
            analysis_ver: ANALYSIS_VER,
            policy_ver: self.policy.version(),
            shaping_ver: SHAPING_VER,
        };
        FrameOutcome { plan, warnings }
    }

    /// Read each face's own statistics.
    fn face_inputs(
        frame: &FrameMeasure,
        ctx: &FrameContext,
        mask_scale: f32,
    ) -> Vec<face_light::FaceInput> {
        ctx.faces
            .iter()
            .map(|face| {
                let stats = frame.rect(face.bbox);
                face_light::FaceInput {
                    luma: stats.mean_luma,
                    p95_luma: stats.p95_luma,
                    side: face.bbox.w.min(face.bbox.h),
                    prominence: face.quality.max(face.area_frac),
                    mask_scale,
                }
            })
            .collect()
    }

    /// Look for sheen on the frame's own faces.
    fn find_shine(
        frame: &FrameMeasure,
        ctx: &FrameContext,
        policy: &ScenePolicy,
        mask_scale: f32,
    ) -> ShineOutcome {
        if policy.declines(LocalOp::ShineControl) || mask_scale <= 0.0 {
            return ShineOutcome::None;
        }
        let skin = ctx.mask(MaskKind::Skin, None);
        let mut spots = Vec::new();
        let mut identities = Vec::new();
        let mut too_large: Option<CropRect> = None;
        let mut p95 = 0.0f32;
        for face in &ctx.faces {
            match shine::find(frame, face.bbox, skin) {
                shine::Found::Spots(found) => {
                    p95 = p95.max(frame.rect(face.bbox).p95_luma);
                    for spot in found {
                        spots.push(spot);
                        identities.push(face.identity_id);
                    }
                }
                shine::Found::TooLarge => too_large = too_large.or(Some(face.bbox)),
                shine::Found::Nothing => {}
            }
        }
        if spots.is_empty() {
            return too_large.map_or(ShineOutcome::None, ShineOutcome::TooLarge);
        }
        spots.truncate(ShineReduction::MAX_REGIONS);
        identities.truncate(spots.len());
        shine::reduce(
            &spots,
            identities,
            p95,
            policy.strength(LocalOp::ShineControl),
            mask_scale,
        )
        .map_or(ShineOutcome::None, ShineOutcome::Reduced)
    }

    /// Decompose and shape the frame's largest faces.
    fn shape(
        frame: &FrameMeasure,
        ctx: &FrameContext,
        policy: &ScenePolicy,
        scale: &[f32; LocalOp::COUNT],
        short_side: f32,
    ) -> (
        Option<aura_core::contract::local::DodgeBurnMaps>,
        Vec<LocalReason>,
    ) {
        let mut notes = Vec::new();
        let low_scale = at(scale, LocalOp::DodgeBurnLow);
        let mid_scale = at(scale, LocalOp::DodgeBurnMid);
        if policy.declines(LocalOp::DodgeBurnLow) && policy.declines(LocalOp::DodgeBurnMid) {
            notes.push(LocalReason::plain(LocalCode::SceneDeclinesShaping, 0.0));
            return (None, notes);
        }
        if low_scale <= 0.0 && mid_scale <= 0.0 {
            return (None, notes);
        }
        let mut shaped = Vec::new();
        let mut too_small = false;
        for face in ctx.faces.iter().take(MAX_SHAPED_FACES) {
            if face.bbox.w.min(face.bbox.h) < MIN_SHAPEABLE_FACE {
                too_small = true;
                continue;
            }
            if !face.has_eyes() {
                // No landmarks, no geometry to shape against. The face is still lit.
                notes.push(LocalReason::plain_at(
                    LocalCode::LandmarksUnavailable,
                    -0.05,
                    face.bbox,
                ));
                continue;
            }
            let crop = frame.crop_luma(face.bbox);
            let bands = freqsep::separate(&crop);
            if bands.is_empty() {
                continue;
            }
            shaped.push(dodgeburn::shape_face(
                face.identity_id,
                face.bbox,
                &bands,
                policy.strength(LocalOp::DodgeBurnLow) * low_scale,
                policy.strength(LocalOp::DodgeBurnMid) * mid_scale,
                short_side,
            ));
        }
        if too_small {
            notes.push(LocalReason::plain(LocalCode::FaceTooSmallToShape, 0.0));
        }
        let maps = dodgeburn::collect(shaped);
        if let Some(maps) = &maps {
            if maps.faces.iter().any(|f| !f.zones.is_empty()) {
                notes.push(LocalReason::plain(LocalCode::FormShaped, 0.0));
            }
            if maps.faces.iter().any(|f| f.evening > 0.0) {
                notes.push(LocalReason::plain(LocalCode::MidFrequencyEvened, 0.0));
            }
            if maps
                .faces
                .iter()
                .any(|f| f.evening > 0.0 && f.evening < 1.0)
            {
                notes.push(LocalReason::plain(LocalCode::TextureProtected, 0.0));
            }
        }
        (maps, notes)
    }

    /// Say what happened, in the product's voice.
    #[allow(clippy::too_many_arguments)]
    fn explain(
        reasons: &mut Vec<LocalReason>,
        solved: &face_light::Solved,
        deltas: &[FaceLightDelta],
        subject: &SubjectEnhanceDelta,
        background: &BackgroundBalanceDelta,
        shine: Option<&ShineReduction>,
        held_mean: bool,
        chroma_led: bool,
    ) {
        if deltas.iter().any(|d| !d.is_noop()) {
            reasons.push(LocalReason::plain(LocalCode::FaceLit, 0.0));
        } else if !deltas.is_empty() {
            reasons.push(LocalReason::plain(LocalCode::FaceAlreadyInBand, 0.0));
        }
        if solved.joint {
            reasons.push(LocalReason::plain(LocalCode::GroupSolvedJointly, 0.0));
        }
        if solved.spread_capped {
            reasons.push(LocalReason::plain(LocalCode::GroupSpreadCapped, 0.0));
        }
        if solved.noise_capped {
            reasons.push(LocalReason::plain(LocalCode::LiftCappedByNoise, 0.0));
        }
        if solved.highlight_capped {
            reasons.push(LocalReason::plain(LocalCode::LiftCappedByHighlights, 0.0));
        }
        if !subject.is_noop() {
            reasons.push(LocalReason::plain(LocalCode::SubjectSeparated, 0.0));
            reasons.push(LocalReason::plain(LocalCode::SubjectBackgroundPaired, 0.0));
        }
        if background.exposure_ev < 0.0 {
            reasons.push(LocalReason::plain(LocalCode::BackgroundLumaReduced, 0.0));
        }
        if background.saturation < 0 && chroma_led {
            reasons.push(LocalReason::plain(LocalCode::BackgroundChromaReduced, 0.0));
        }
        if background.bright_blobs > 0 && !background.is_noop() {
            reasons.push(LocalReason::plain(LocalCode::BrightBlobCalmed, 0.0));
        }
        if held_mean {
            reasons.push(LocalReason::plain(LocalCode::PairingHeldMeanLuma, 0.0));
        }
        if let Some(shine) = shine {
            let evidence = shine.regions.first().copied();
            reasons.push(match evidence {
                Some(rect) => LocalReason::plain_at(LocalCode::ShineReduced, 0.0, rect),
                None => LocalReason::plain(LocalCode::ShineReduced, 0.0),
            });
        }
    }
}

/// What the shine search concluded.
#[derive(Debug, Clone, PartialEq)]
enum ShineOutcome {
    None,
    TooLarge(CropRect),
    Reduced(ShineReduction),
}

/// One entry of the per-operation scale array.
///
/// A function rather than an index, because `clippy::indexing_slicing` is denied crate-wide
/// and a panic in a decision path is not something a comment can make safe.
fn at(scale: &[f32; LocalOp::COUNT], op: LocalOp) -> f32 {
    scale.get(op.rank()).copied().unwrap_or(0.0)
}

/// The identities, in the order the faces were solved.
fn identities(ctx: &FrameContext) -> impl Iterator<Item = Option<IdentityId>> + '_ {
    ctx.faces.iter().map(|f| f.identity_id)
}

/// The subject region's area, or zero when there is no subject mask.
fn subject_area(frame: &FrameMeasure, ctx: &FrameContext) -> f32 {
    ctx.mask(MaskKind::Subject, None)
        .filter(|f| f.is_usable())
        .map_or(0.0, |f| frame.region(f).area)
}

/// The background region's area, or zero when there is no background mask.
fn background_delta_area(frame: &FrameMeasure, ctx: &FrameContext) -> f32 {
    ctx.mask(MaskKind::Background, None)
        .filter(|f| f.is_usable())
        .map_or(0.0, |f| frame.region(f).area)
}

/// Scale every delta by what the governor allowed it.
fn apply_ledger(
    ledger: &Ledger,
    faces: &mut [FaceLightDelta],
    subject: &mut SubjectEnhanceDelta,
    background: &mut BackgroundBalanceDelta,
    shine: &mut Option<ShineReduction>,
    maps: &mut Option<aura_core::contract::local::DodgeBurnMaps>,
) {
    use crate::local::measure::apply_ev;

    let face_scale = ledger.allowed(LocalOp::FaceLight);
    if face_scale < 1.0 {
        for delta in faces.iter_mut() {
            delta.exposure_ev *= face_scale;
            delta.shadows = (f32::from(delta.shadows) * face_scale).round() as i16;
            delta.highlights = (f32::from(delta.highlights) * face_scale).round() as i16;
            let ev = crate::local::measure::ev_between(delta.luma_before, delta.luma_after)
                * face_scale;
            delta.luma_after = apply_ev(delta.luma_before, ev);
        }
    }

    let subject_scale = ledger.allowed(LocalOp::SubjectEnhance);
    if subject_scale < 1.0 {
        subject.clarity = (f32::from(subject.clarity) * subject_scale).round() as i16;
        subject.texture = (f32::from(subject.texture) * subject_scale).round() as i16;
        subject.contrast = (f32::from(subject.contrast) * subject_scale).round() as i16;
    }

    let background_scale = ledger.allowed(LocalOp::BackgroundBalance);
    if background_scale < 1.0 {
        background.exposure_ev *= background_scale;
        background.highlights = (f32::from(background.highlights) * background_scale).round() as i16;
        background.saturation = (f32::from(background.saturation) * background_scale).round() as i16;
        background.mean_luma_after = background.mean_luma_before
            + (background.mean_luma_after - background.mean_luma_before) * background_scale;
        // The two halves are one decision. Scaling the background without the subject is
        // exactly the un-paired operation section 6.2 forbids.
        subject.paired_background_ev = background.exposure_ev;
    }

    let shine_scale = ledger.allowed(LocalOp::ShineControl);
    if shine_scale < 1.0 {
        if let Some(reduction) = shine {
            reduction.reduction_ev *= shine_scale;
            reduction.peak_after = apply_ev(reduction.peak_before, reduction.reduction_ev);
            if reduction.reduction_ev >= -1e-3 {
                *shine = None;
            }
        }
    }

    let low_scale = ledger.allowed(LocalOp::DodgeBurnLow);
    let mid_scale = ledger.allowed(LocalOp::DodgeBurnMid);
    if let Some(maps) = maps {
        if low_scale < 1.0 {
            for face in &mut maps.faces {
                for zone in &mut face.zones {
                    zone.gain_ev *= low_scale;
                }
                face.zones.retain(|z| z.gain_ev.abs() > 1e-4);
                face.low_freq = dodgeburn::grid(face.region, &face.zones);
            }
        }
        if mid_scale < 1.0 {
            for face in &mut maps.faces {
                face.evening *= mid_scale;
                for value in &mut face.mid_freq {
                    *value = (f32::from(*value) * mid_scale).round() as i8;
                }
                face.band_energy_after = face.band_energy_before
                    + (face.band_energy_after - face.band_energy_before) * mid_scale;
            }
        }
    }
    if maps
        .as_ref()
        .is_some_and(|m| m.faces.iter().all(|f| f.zones.is_empty() && f.evening <= 0.0))
    {
        *maps = None;
    }
}

/// How much the plan trusts itself.
///
/// One minus the doubts, floored. The doubts are the reason weights, which is the shape phases
/// 09, 11 and 15 all use: a confidence assembled from a separate set of numbers is a
/// confidence that can disagree with the sentences underneath it.
fn confidence_of(
    reasons: &[LocalReason],
    unpolicied: bool,
    gated: &[(LocalOp, MaskKind)],
) -> f32 {
    let doubts: f32 = reasons.iter().filter(|r| r.is_doubt()).map(|r| r.weight).sum();
    let mut confidence = 1.0 + doubts;
    if unpolicied {
        confidence -= UNPOLICIED_CONFIDENCE_PENALTY;
    }
    // Each gated operation costs a little more, because a plan that could not run four of its
    // six operations has described very little of what it wanted to do.
    confidence -= 0.04 * gated.len() as f32;
    confidence.clamp(0.05, 1.0)
}
