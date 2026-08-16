//! One decoded frame in, one framing judgement out.
//!
//! The shape phases 05, 06, 09 and 10 all argued for, a fifth time: **the pixels are
//! decoded once**. From one 2048 px proxy come the luminance plane, the orientation
//! histograms, every person crop, the analysis grid, the colour statistics and both
//! models' inputs. Nothing here opens a file twice, and section 11's 30 ms budget - the
//! tightest per-image budget in the wedding brain - is only reachable because of it.
//!
//! ## One batched model call per frame, never one per person
//!
//! A group photograph has thirty people. Thirty calls into the runtime would discard the
//! memory ledger and the batch scheduler phase 03 built, which is the mistake
//! `aura_vision::face::FacePipeline::analyse` documents at its step 3 and which phases 09
//! and 10 both copy deliberately. Section 9 gives PERF "share keypoint inference with
//! Phase 06"; batching is what makes that sharing possible at all.
//!
//! ## What is measured against what
//!
//! | Measurement | Reads | Conditioned by |
//! |---|---|---|
//! | tilt | the whole luminance plane | `tilt_tolerance_deg`, `allows_dutch` |
//! | crop audit | derived person boxes and their keypoints | `joint_cut_weight`, `head_crop_allowed` |
//! | headroom | the highest crown | `headroom_min`, `headroom_max` |
//! | placement | the subject box against the thirds | `thirds_weight`, `centre_preferred` |
//! | background | everything outside the subject box | `clutter_tolerance` |
//! | aesthetic | the twelve measurements above | `aesthetic_weight`, the scene one-hot |
//!
//! Every row's third column is this phase's rule table. Invariant 7 - no threshold is
//! global - is not a thing this module remembers to do; none of the analysers takes a
//! constant it could have used instead.
//!
//! ## The order matters in exactly one place
//!
//! The crop audit runs **before** placement, because a cut head is a negative headroom and
//! the placement stage has to be told the depth of the cut rather than discovering a crown
//! at `y = 0` and calling the headroom zero. Everything else is independent and could run
//! in any order.

use std::sync::Arc;

use aura_core::contract::composition::{Box2, CompositionFlags, CompositionResult};
use aura_core::contract::people::ImageSubjects;
use aura_core::{AuraResult, PhotoId, Priority, SceneId};
use aura_infer::contract::infer::InferService;
use aura_raw::contract::pixels::{PixelBuffer, PixelLevel};
use aura_vision::embed::descriptors::{luma_plane, LumaPlane};

use crate::composition::aesthetic::{Aesthetic, AestheticHead};
use crate::composition::background;
use crate::composition::crop_audit;
use crate::composition::flags::{self, Evidence};
use crate::composition::horizon::{self, HorizonInput};
use crate::composition::keypoints::{self, Pose, PoseHead, ReferencePose, MODEL_VER};
use crate::composition::placement;
use crate::composition::rules::{RuleTable, SceneRule, UNRULED_CONFIDENCE_PENALTY};
use crate::composition::score::{self, SceneCalibration};

/// Which build's arithmetic produced a judgement.
///
/// Bumped on any change to a threshold, a sub-score curve, a flag rule or the way the
/// composite is combined - which is to say on almost any change in this module tree that
/// is not a comment. It is written into `image_composition.analysis_ver`, and two
/// judgements made under different values of it are not comparable: `AURA-ML-5043` exists
/// so that comparison never happens silently.
pub const ANALYSIS_VER: u16 = 1;

/// The pixel rung the composition pass reads.
///
/// Tier 2, the 2048 px proxy - the same rung phases 06, 09 and 10 use, and the reason is
/// this phase's rather than inherited. Composition is a question about *arrangement*, and
/// arrangement survives downsampling: a horizon is as measurable at 2048 px as at 61 MP,
/// and a wrist near the frame edge is as locatable. The one measurement that would benefit
/// from more pixels is the head merge, and even there the structure being looked for is a
/// pole rather than a hair.
///
/// The proxy is already built and cached by phase 02 and already read by three other
/// passes, so the cost here is a cache read rather than a decode.
pub const COMPOSITION_LEVEL: PixelLevel = PixelLevel::Proxy2048;

/// Everything outside the pixels that one frame's judgement depends on.
///
/// Assembled by the caller from the frozen services - `StoryService` for the scene and
/// `PeopleService` for the faces. This crate takes no dependency on the crates that
/// implement them, so the only way a scene or a face can reach an analyser is through this
/// struct.
#[derive(Debug, Clone, Default)]
pub struct FrameContext {
    /// What phase 07 says the photograph is of.
    pub scene: SceneId,
    /// Who is in the frame, from phase 06.
    pub subjects: ImageSubjects,
    /// True when a scene label was actually present.
    ///
    /// Distinguished from `scene == SceneId::Unknown` for phase 09's reason: a
    /// classified-as-unknown frame has been looked at and an unclassified one has not, and
    /// only the second should cost the judgement confidence twice.
    pub scene_known: bool,
    /// What the camera's gravity sensor said, when it recorded one.
    ///
    /// `None` on every path in this build. Phase 01's schema has no column for it and no
    /// body in the calibration table writes one; the field exists because section 6.1 asks
    /// for the cross-check and the shape of the code should not have to change when a body
    /// that records it arrives. Condition C4 of the phase 11 exit report.
    pub gravity_deg: Option<f32>,
}

impl FrameContext {
    /// Every face box in the frame, largest first.
    #[must_use]
    pub fn face_boxes(&self) -> Vec<Box2> {
        let mut out: Vec<Box2> = self.subjects.faces.iter().map(|face| face.bbox).collect();
        out.sort_by(|left, right| (right.w * right.h).total_cmp(&(left.w * left.h)));
        out
    }
}

/// The per-frame pipeline.
#[derive(Debug, Clone)]
pub struct Analyser {
    pose_head: PoseHead,
    aesthetic_head: AestheticHead,
    rules: RuleTable,
    scores: SceneCalibration,
    /// When set, the fixture reader replaces the keypoint head. The gates and
    /// `tests/eval/composition_eval.rs` use it; nothing in the product does.
    reference_poses: bool,
}

/// The poses available to one judgement and whether the keypoint-dependent checks were
/// degraded.
#[derive(Debug, Default)]
struct LocatedPoses {
    poses: Vec<Pose>,
    degraded: bool,
}

impl Analyser {
    /// Assemble the pipeline over one inference service, with the shipped rules.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5046` when the embedded rule table will not load, which is a build bug
    /// rather than a deployment one.
    pub fn new(infer: Arc<dyn InferService>) -> Result<Self, aura_core::AuraError> {
        Ok(Self {
            pose_head: PoseHead::new(Arc::clone(&infer)),
            aesthetic_head: AestheticHead::new(infer),
            rules: RuleTable::embedded()?,
            scores: SceneCalibration::shipped(),
            reference_poses: false,
        })
    }

    /// Use a different rule table. The installation override path, and QA's sweeps.
    #[must_use]
    pub fn with_rules(mut self, rules: RuleTable) -> Self {
        self.rules = rules;
        self
    }

    /// Install fitted per-scene score calibration.
    #[must_use]
    pub fn with_score_calibration(mut self, scores: SceneCalibration) -> Self {
        self.scores = scores;
        self
    }

    /// Read poses from the fixture markers instead of from the head.
    ///
    /// **This is the only way to state a limb-crop F1 at all** while the shipped head is a
    /// placeholder, and it is the same device phases 06 and 09 used. Explicit at the call
    /// site rather than a feature flag, so no product path can reach it by accident.
    #[must_use]
    pub const fn with_reference_poses(mut self) -> Self {
        self.reference_poses = true;
        self
    }

    /// The rule table in force.
    #[must_use]
    pub const fn rules(&self) -> &RuleTable {
        &self.rules
    }

    /// Load both heads before a photographer is waiting on them.
    ///
    /// # Errors
    ///
    /// Whatever the loader raised. A warmup failure is a real failure: it is how the
    /// product learns a model pack is broken before a wedding depends on it.
    pub fn warmup(&self) -> AuraResult<()> {
        if self.pose_head.is_trained() {
            self.pose_head.warmup()?;
        }
        if self.aesthetic_head.is_trained() {
            self.aesthetic_head.warmup()?;
        }
        Ok(())
    }

    /// Which execution provider the hardware plan selected.
    #[must_use]
    pub fn ep(&self) -> &'static str {
        self.pose_head.ep()
    }

    /// Judge one frame.
    ///
    /// # Errors
    ///
    /// * `AURA-ML-5007` when the buffer is not 8-bit sRGB or a model's tensors disagree
    ///   with the manifest.
    /// * `AURA-ML-5011` when the work was cancelled.
    ///
    /// A person whose crop is unusable is **not** an error: they are skipped, they
    /// contribute no cuts, and the frame is judged without them. One bad box must not fail
    /// a frame, let alone a wedding. Neither head failing is an error either - both
    /// degrade, and both degradations are visible in the reasons.
    #[allow(clippy::too_many_lines)]
    pub fn analyse(
        &self,
        image_id: PhotoId,
        buffer: &PixelBuffer,
        context: &FrameContext,
        prio: Priority,
    ) -> AuraResult<CompositionResult> {
        let Some(rgb) = buffer.as_srgb8() else {
            return Err(aura_core::errors::ml::shape_mismatch(
                "an 8-bit sRGB buffer",
                buffer.data.as_str(),
            ));
        };
        let width = buffer.width as usize;
        let height = buffer.height as usize;
        if width == 0 || height == 0 || rgb.len() < width * height * 3 {
            return Err(aura_core::errors::ml::shape_mismatch(
                &format!("{width}x{height}x3 samples"),
                &format!("{} samples", rgb.len()),
            ));
        }

        // 1. The one derived plane, and the one rule row. Everything below reads these.
        let plane = luma_plane(rgb, width, height);
        let rule = self.rules.get(context.scene);
        let faces = context.face_boxes();

        // 2. Poses. One batched call over every derivable person box.
        let located = self.locate(rgb, width, height, &faces, prio, image_id);
        let poses = &located.poses;
        let subjectless = faces.is_empty() && poses.is_empty();

        // 3. What the frame edge cut. Before placement, because a cut head is a negative
        //    headroom and placement has to be told the depth.
        let audit = crop_audit::analyse(poses, &faces, &rule);

        // 4. Placement, balance and empty space.
        let placement = placement::analyse(&plane, poses, &faces, &rule, audit.head_crop_depth);

        // 5. The horizon, which needs the subject centre for the intentional-tilt rule.
        let horizon = horizon::analyse(
            &plane,
            &HorizonInput {
                gravity_deg: context.gravity_deg,
                subject_centre: placement.subject_centre,
                allows_dutch: rule.allows_dutch,
            },
        );

        // 6. The background.
        let background = background::analyse(
            &plane,
            rgb,
            width,
            height,
            placement.subject_box,
            &faces,
            &rule,
        );

        // 7. Taste, over the twelve measurements above.
        let features = crate::composition::aesthetic::features(
            &horizon,
            &placement,
            &background,
            &rule,
            context.scene,
        );
        let aesthetic = self.taste(&features, prio, image_id);

        // 8. Flags and reasons.
        let verdict = flags::decide(&Evidence {
            horizon: &horizon,
            audit: &audit,
            placement: &placement,
            background: &background,
            aesthetic,
            rule: &rule,
            subjectless,
            keypoints_degraded: located.degraded,
        });

        // 9. The composite.
        let sub = score::sub_scores(&horizon, &audit, &placement, &background, &rule);
        let composition_score = score::compose(
            sub,
            score::Weights::for_rule(&rule),
            aesthetic,
            &self.scores.get(context.scene),
        );

        let confidence = confidence_of(
            context,
            &rule,
            &horizon,
            subjectless,
            located.degraded,
            aesthetic,
        );
        let hint = placement::crop_hint(
            &placement,
            horizon.tilt_deg,
            horizon.confidence,
            confidence,
            audit.cuts.iter().any(aura_core::JointCut::is_flagged),
        );

        Ok(CompositionResult {
            image_id,
            tilt_deg: horizon.tilt_deg,
            tilt_intentional: horizon.intentional,
            horizon_conf: horizon.confidence,
            horizon_source: horizon.source,
            headroom: placement.headroom,
            thirds_offset: placement.thirds_offset,
            balance: placement.balance,
            negative_space: placement.negative_space,
            joint_cuts: audit.cuts.clone(),
            head_crop: audit.head_crop,
            edge_intrusions: audit.intrusions.clone(),
            clutter: background.clutter,
            bright_blobs: background.bright_blobs.clone(),
            head_merge: background.head_merge,
            colour_competition: background.colour_competition,
            aesthetic: aesthetic.value,
            composition_score,
            crop_suggestion_hint: hint,
            scene: context.scene,
            // Filled by the store once the frame's moment is known. 0.5 - neither the best
            // nor the worst of no siblings - until then.
            relative_composition: 0.5,
            keypoint_subjects: u32::try_from(audit.audited).unwrap_or(u32::MAX),
            flags: verdict.flags,
            reasons: verdict.reasons,
            confidence,
            user_reviewed: false,
            model_ver: MODEL_VER,
            analysis_ver: ANALYSIS_VER,
            rules_ver: self.rules.version(),
        })
    }

    /// Locate every derivable person, in one batched call.
    ///
    /// A head that will not run costs the whole crop audit, so the frame is judged with no
    /// poses rather than failing. That degradation is visible: `keypoint_subjects` is zero,
    /// `CompositionOutline::keypoint_aware` falls, and no cut is claimed - which is the
    /// direction that under-reports violations instead of inventing them.
    fn locate(
        &self,
        rgb: &[u8],
        width: usize,
        height: usize,
        faces: &[Box2],
        prio: Priority,
        image_id: PhotoId,
    ) -> LocatedPoses {
        if faces.is_empty() {
            return LocatedPoses::default();
        }
        let boxes: Vec<(Box2, Box2)> = faces
            .iter()
            .map(|face| (keypoints::person_box(*face), *face))
            .collect();

        if self.reference_poses {
            let poses: Vec<Pose> = boxes
                .into_iter()
                .map(|(person, face)| ReferencePose::read(rgb, width, height, person, face))
                .collect();
            let degraded = poses.iter().any(|pose| pose.located() == 0);
            return LocatedPoses { poses, degraded };
        }

        if !self.pose_head.is_trained() {
            tracing::debug!(
                target: "composition.keypoints",
                photo = %image_id,
                model = keypoints::KEYPOINT_MODEL,
                "the pinned keypoint head is an untrained placeholder; crop claims were withheld"
            );
            return LocatedPoses {
                poses: Vec::new(),
                degraded: true,
            };
        }

        let crops: Vec<Vec<f32>> = boxes
            .iter()
            .map(|(person, _)| keypoints::preprocess_person(rgb, width, height, *person))
            .collect();
        let expected = boxes.len();
        match self.pose_head.run_batch(&crops, prio) {
            Ok(raw) => {
                let complete = raw.len() == expected;
                let poses: Vec<Pose> = raw
                    .into_iter()
                    .zip(boxes)
                    .map(|(values, (person, face))| keypoints::decode(&values, person, face))
                    .collect();
                let degraded = !complete
                    || poses.len() != expected
                    || poses.iter().any(|pose| pose.located() == 0);
                if degraded {
                    tracing::warn!(
                        target: "composition.keypoints",
                        photo = %image_id,
                        expected,
                        returned = poses.len(),
                        "the keypoint batch was incomplete; affected crop claims were withheld"
                    );
                }
                LocatedPoses { poses, degraded }
            }
            Err(err) => {
                tracing::warn!(
                    target: "composition.keypoints",
                    photo = %image_id,
                    code = %err.code,
                    "the keypoint head did not run; no crop audit was performed on this frame"
                );
                LocatedPoses {
                    poses: Vec::new(),
                    degraded: true,
                }
            }
        }
    }

    /// Score the feature vector, falling back to the reference model.
    ///
    /// The fallback is not a silent one: [`Aesthetic::learned`] is false, the judgement
    /// carries `CompositionCode::AestheticUnavailable`, and the confidence drops. A caller
    /// must be able to tell "the head said 0.5" from "there was no head".
    fn taste(&self, features: &[f32], prio: Priority, image_id: PhotoId) -> Aesthetic {
        if !self.aesthetic_head.is_trained() {
            tracing::debug!(
                target: "composition.aesthetic",
                photo = %image_id,
                model = keypoints::AESTHETIC_MODEL,
                "the pinned aesthetic head is an untrained placeholder; geometry alone decided"
            );
            return Aesthetic::from_features(features);
        }
        match self.aesthetic_head.run_batch(&[features.to_vec()], prio) {
            Ok(values) => match values.first().copied() {
                Some(value) => Aesthetic {
                    value,
                    learned: true,
                },
                None => Aesthetic::from_features(features),
            },
            Err(err) => {
                tracing::debug!(
                    target: "composition.aesthetic",
                    photo = %image_id,
                    code = %err.code,
                    "the aesthetic head did not run; geometry alone decided"
                );
                Aesthetic::from_features(features)
            }
        }
    }
}

/// How sure the whole judgement is.
///
/// Invariant 2, and the number is built by subtraction rather than by a model: it starts at
/// 0.95 - never 1.0, because a framing judgement from a 2048 px proxy is an inference about
/// an original nobody opened - and loses something for each input that was missing.
///
/// | Missing | Cost | Why |
/// |---|---|---|
/// | scene rule row | `UNRULED_CONFIDENCE_PENALTY` | every band was the neutral row's |
/// | scene label | 0.10 | nothing was conditioned on anything |
/// | any subject | 0.12 | six of the sixteen flags could not be evaluated |
/// | keypoint audit | 0.08 | limb and joint crop claims were withheld |
/// | a measurable horizon | 0.05 | the tilt figure is an absence rather than a zero |
/// | the learned head | 0.06 | the aesthetic reading is the reference model's |
fn confidence_of(
    context: &FrameContext,
    rule: &SceneRule,
    horizon: &horizon::HorizonResult,
    subjectless: bool,
    keypoints_degraded: bool,
    aesthetic: Aesthetic,
) -> f32 {
    let mut confidence = 0.95f32;
    if !rule.measured {
        confidence -= UNRULED_CONFIDENCE_PENALTY;
    }
    if !context.scene_known {
        confidence -= 0.10;
    }
    if subjectless {
        confidence -= 0.12;
    }
    if keypoints_degraded {
        confidence -= 0.08;
    }
    if !horizon.is_measured() {
        confidence -= 0.05;
    }
    if !aesthetic.learned {
        confidence -= 0.06;
    }
    confidence.clamp(0.05, 0.95)
}

/// The flags a frame with no subject cannot carry.
///
/// Used by the store's read path and by the eval harness to assert the withdrawal is total:
/// a frame with `NO_GEOMETRY` set must not also claim a headroom problem or a head merge,
/// because there was no head. Written once here rather than as six `assert!`s in three
/// places.
#[must_use]
pub const fn subject_only_flags() -> CompositionFlags {
    CompositionFlags::EMPTY
        .union(CompositionFlags::HEADROOM_TIGHT)
        .union(CompositionFlags::HEADROOM_EXCESSIVE)
        .union(CompositionFlags::HEAD_CROPPED)
        .union(CompositionFlags::JOINT_CUT)
        .union(CompositionFlags::LIMB_CUT)
        .union(CompositionFlags::HEAD_MERGE)
}

/// The luminance plane one frame produced, for callers that want to measure twice.
///
/// Exposed because `tests/eval/composition_eval.rs` builds frames and needs the same plane
/// the analyser saw; nothing in the product calls it.
#[must_use]
pub fn plane_of(rgb: &[u8], width: usize, height: usize) -> LumaPlane {
    luma_plane(rgb, width, height)
}
