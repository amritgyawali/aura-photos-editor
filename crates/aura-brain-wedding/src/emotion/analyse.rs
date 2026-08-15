//! One decoded frame in, one emotion reading out.
//!
//! The shape phases 05, 06 and 09 all argued for, a fourth time: **the pixels are decoded
//! once**. From one 2048 px proxy come every aligned face crop and the interaction head's
//! four-plane tensor, and nothing here opens a file twice.
//!
//! ## Two model calls per frame, never one per face
//!
//! A group photograph has thirty faces. Thirty calls into the runtime would discard the
//! memory ledger and the batch scheduler phase 03 built - the mistake
//! `FacePipeline::analyse` documents at its step 3, which `Analyser::analyse` in
//! `aura-brain-photo` copies and this copies again. The expression head takes one batched
//! call over every face; the interaction head takes one call over the frame.
//!
//! ## What arrives from where
//!
//! | Reading | Comes from | Conditioned by |
//! |---|---|---|
//! | eight expression channels | the expression head, on an aligned crop | nothing; the scene weights are applied later |
//! | gaze | phase 06's two eye landmarks and the face box | the scene, for the officiant case |
//! | interactions | the interaction head, on the frame plus a person prior | nothing |
//! | mutual gaze | the face boxes and the turn geometry | nothing |
//! | the score | all of the above | the scene's weights and the tradition's multipliers |
//!
//! Only the last row is scene-conditioned, and that is the design. **A reading is not
//! conditioned; a judgement is.** Invariant 7 is about thresholds, and the only threshold
//! in this phase is the weighting that turns readings into one number - so it is the only
//! thing that has to know what the photograph is of. Storing the readings unconditioned
//! is also what lets `emotion_weights.toml` be re-tuned with a `weights_ver` bump instead
//! of a re-run of two heads.

use std::collections::BTreeMap;
use std::sync::Arc;

use aura_core::contract::emotion::{
    EmotionReason, FaceExpression, ImageEmotion, Interaction, MomentPeak,
};
use aura_core::contract::people::{FaceRef, ImageSubjects};
use aura_core::contract::scene::Source;
use aura_core::{AuraResult, IdentityId, PhotoId, Priority, SceneId};
use aura_infer::contract::infer::InferService;
use aura_raw::contract::pixels::{PixelBuffer, PixelLevel};

use crate::emotion::expression::{self, ExpressionHead, MODEL_VER};
use crate::emotion::gaze;
use crate::emotion::interaction::{self, InteractionHead};
use crate::emotion::score::{self, Inputs};
use crate::emotion::weights::EmotionWeights;

/// Which build's arithmetic produced a reading.
///
/// Bumped on any change to the gaze geometry, the peak curve, the reaction rules or the
/// feature construction - which is to say on almost any change in this module or its
/// siblings that is not a comment. It is written into `image_interaction.analysis_ver`,
/// and two readings made under different values of it are not comparable:
/// `AURA-ML-5038` exists so that comparison never happens silently.
pub const ANALYSIS_VER: u16 = 1;

/// The pixel rung the emotion pass reads.
///
/// Tier 2, the 2048 px proxy - the same rung phases 06 and 09 read, and the same cache
/// entry. Invariant 3 puts medium analysis here and this is medium analysis: a 384 px
/// thumbnail cannot answer whether a smile reaches the eyes, because at that size an eye
/// is nine pixels across.
///
/// Tier 3 was considered and rejected for phase 09's reason plus one of this phase's own:
/// an expression is a shape rather than a texture, so full sensor resolution would cost
/// eight times as much to measure the same thing - and the interaction head resamples to
/// 160 px regardless.
pub const EMOTION_LEVEL: PixelLevel = PixelLevel::Proxy2048;

/// Everything outside the pixels that one frame's reading depends on.
///
/// Assembled by the caller from the frozen services - `StoryService` for the scene and
/// the tradition, `PeopleService` for the subjects and the couple, `MomentService` for
/// the grouping. This module takes no dependency on the crates that implement them, so
/// the only way a scene or an identity can reach the analyser is through this struct.
#[derive(Debug, Clone, Default)]
pub struct FrameContext {
    /// What phase 07 says the photograph is of.
    pub scene: SceneId,
    /// True when a scene label was actually present.
    ///
    /// Distinguished from `scene == SceneId::Unknown` for phase 09's reason: a
    /// classified-as-unknown frame has been looked at and an unclassified one has not,
    /// and only the second should cost the reading confidence.
    pub scene_known: bool,
    /// The wedding's tradition, from phase 07's ritual head, as a taxonomy slug.
    pub tradition: Option<String>,
    /// Who is in the frame, from phase 06.
    pub subjects: ImageSubjects,
    /// The identities phase 06 calls primary, for the partner-gaze case.
    pub couple: Vec<IdentityId>,
    /// True when a face pass has actually run over this project.
    ///
    /// Not `subjects.faces.is_empty()`: a detail shot of the rings has no faces and has
    /// been scanned, and a wedding with no face pass has no faces and has not. The
    /// confidence penalty is 0.25 for the second and nothing for the first.
    pub faces_scanned: bool,
}

/// The per-frame pipeline.
#[derive(Debug, Clone)]
pub struct Analyser {
    expression: ExpressionHead,
    interaction: InteractionHead,
    weights: Arc<EmotionWeights>,
    /// When set, the fixture reader replaces the expression head. The gates and
    /// `tests/eval/emotion_eval.rs` use it; nothing in the product does.
    reference_expressions: bool,
}

impl Analyser {
    /// Assemble the pipeline over one inference service, with the shipped weights.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5039` when the embedded weight table will not load, which is a build bug
    /// rather than a deployment one.
    pub fn new(infer: Arc<dyn InferService>) -> Result<Self, aura_core::AuraError> {
        Ok(Self {
            expression: ExpressionHead::new(Arc::clone(&infer)),
            interaction: InteractionHead::new(infer),
            weights: Arc::new(EmotionWeights::embedded()?),
            reference_expressions: false,
        })
    }

    /// Use a different weight table. The installation override path, and QA's sweeps.
    #[must_use]
    pub fn with_weights(mut self, weights: Arc<EmotionWeights>) -> Self {
        self.weights = weights;
        self
    }

    /// Read expressions from the fixture marker instead of from the head.
    ///
    /// **This is the only way to state an expression accuracy at all** while the shipped
    /// head is a placeholder, and it is the same device phase 06 used for detection
    /// recall and phase 09 for blink F1. What it measures is everything around the head -
    /// the alignment, the gaze geometry, the prominence weighting, the peak curve, the
    /// reaction rules, the nine features, the ranker and the calibration - against an
    /// answer known by construction. What it does not measure is the weights, and the
    /// phase 10 exit report says so where the number is reported.
    ///
    /// Explicit at the call site rather than a feature flag, so no product path can reach
    /// it by accident.
    #[must_use]
    pub const fn with_reference_expressions(mut self) -> Self {
        self.reference_expressions = true;
        self
    }

    /// The weight table in force.
    #[must_use]
    pub fn weights(&self) -> &Arc<EmotionWeights> {
        &self.weights
    }

    /// Load both heads before a photographer is waiting on them.
    ///
    /// # Errors
    ///
    /// Whatever the loader raised. A warmup failure is a real failure: it is how the
    /// product learns a model pack is broken before a wedding depends on it.
    pub fn warmup(&self) -> AuraResult<()> {
        self.expression.warmup()?;
        self.interaction.warmup()
    }

    /// Read one frame.
    ///
    /// The peak proximity and the reaction bonus are **not** known here - both depend on
    /// frames this one has not seen - so they arrive as neutral and the project pass
    /// fills them in afterwards. `ImageEmotion::peak_proximity` is
    /// `peak::NEUTRAL_PROXIMITY` on the way out of this function, and the pass overwrites
    /// it once the moment curves exist.
    ///
    /// # Errors
    ///
    /// Whatever either head raised.
    pub fn analyse(
        &self,
        image: PhotoId,
        buffer: &PixelBuffer,
        context: &FrameContext,
        prio: Priority,
    ) -> AuraResult<ImageEmotion> {
        let width = buffer.width as usize;
        let height = buffer.height as usize;
        let rgb = buffer.as_srgb8().unwrap_or(&[]);

        let faces = self.read_faces(rgb, width, height, context, prio)?;
        let boxes: Vec<FaceRef> = context.subjects.faces.clone();
        let had_prior = !boxes.is_empty();
        let strengths = if rgb.is_empty() {
            [0.0f32; aura_infer::onnx::fixtures::INTERACTION_CLASSES]
        } else {
            let tensor = interaction::frame_tensor(rgb, width, height, &boxes);
            self.interaction.run(&tensor, prio)?
        };
        let interactions = interaction::detected(&strengths);
        let mutual = gaze::mutual(&boxes);

        Ok(self.assemble(
            image,
            context,
            faces,
            interactions,
            mutual,
            had_prior,
            &strengths,
        ))
    }

    /// Turn readings into a scored reading.
    ///
    /// Split out from [`Analyser::analyse`] so the project pass can re-score a frame
    /// after the peak curve and the reaction links exist without running either head
    /// again. Re-scoring a whole wedding is then arithmetic over stored numbers, which is
    /// what makes a `weights_ver` bump cheap.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn assemble(
        &self,
        image: PhotoId,
        context: &FrameContext,
        faces: Vec<FaceExpression>,
        interactions: Vec<(Interaction, f32)>,
        mutual_gaze: bool,
        had_prior: bool,
        strengths: &[f32; aura_infer::onnx::fixtures::INTERACTION_CLASSES],
    ) -> ImageEmotion {
        self.rescore(
            image,
            context,
            faces,
            interactions,
            mutual_gaze,
            crate::emotion::peak::NEUTRAL_PROXIMITY,
            0.0,
            interaction::confidence(strengths, had_prior),
        )
    }

    /// Score one frame's readings against the scene's weights.
    ///
    /// The one place the weight table, the ranker and the calibration meet. Every caller
    /// - the per-frame pass, the re-score after the peaks are known, and the eval
    ///   harness - goes through here, so there is exactly one arithmetic path from
    ///   readings to a number.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn rescore(
        &self,
        image: PhotoId,
        context: &FrameContext,
        faces: Vec<FaceExpression>,
        interactions: Vec<(Interaction, f32)>,
        mutual_gaze: bool,
        peak_proximity: f32,
        reaction_bonus: f32,
        interaction_confidence: f32,
    ) -> ImageEmotion {
        let weights = self
            .weights
            .for_scene(context.scene, context.tradition.as_deref());
        let prominence = context.subjects.prominence.clone();
        let inputs = Inputs {
            faces: &faces,
            prominence: &prominence,
            interactions: &interactions,
            mutual_gaze,
            peak_proximity,
            reaction_bonus,
            scene: context.scene,
            scene_known: context.scene_known,
            faces_scanned: context.faces_scanned,
        };

        let features = score::features(&inputs, &weights);
        let ranker = self.weights.ranker();
        let calibration = self.weights.calibration(context.scene);
        let emotion_score = score::score(&features, &ranker, &calibration);
        let peak_resolved =
            (peak_proximity - crate::emotion::peak::NEUTRAL_PROXIMITY).abs() > f32::EPSILON;
        let mut reasons = score::reasons(&inputs, &features, &ranker, peak_resolved);
        reasons.truncate(ImageEmotion::MAX_REASONS);

        // The frame confidence takes the interaction head's confidence into account only
        // when there are no faces. With faces, the expression head is the reader that
        // matters and the interaction strength is one feature of nine.
        let mut confidence = score::confidence(&inputs, peak_resolved);
        if faces.is_empty() {
            confidence = confidence.min(interaction_confidence.max(0.05));
        }

        ImageEmotion {
            image_id: image,
            faces,
            interactions,
            mutual_gaze,
            peak_proximity,
            reaction_of: None,
            emotion_score,
            // Zero until a cloud `MomentSignificance` answer raises it. Section 7's
            // offline fallback, which is also the online default: the call is only made
            // for ambiguous moments.
            narrative_weight: 0.0,
            scene: context.scene,
            reasons,
            confidence,
            source: Source::Local,
            model_ver: MODEL_VER,
            analysis_ver: ANALYSIS_VER,
            weights_ver: self.weights.version(),
        }
    }

    /// Read every face in the frame.
    ///
    /// Faces with no usable landmarks are **skipped rather than guessed at**, which is
    /// phase 09's rule and matters more here: a crop built from collapsed landmarks is a
    /// rectangle of somebody's shoulder, and eight confident readings about a shoulder is
    /// worse than no reading at all. Such a face gets a `FaceExpression::unread` row, so
    /// the panel can show it greyed rather than pretending it is not there.
    fn read_faces(
        &self,
        rgb: &[u8],
        width: usize,
        height: usize,
        context: &FrameContext,
        prio: Priority,
    ) -> AuraResult<Vec<FaceExpression>> {
        let all = &context.subjects.faces;
        if all.is_empty() || rgb.is_empty() {
            return Ok(Vec::new());
        }

        let mut readable = Vec::new();
        let mut tensors = Vec::new();
        for (index, face) in all.iter().enumerate() {
            if let Some(tensor) = expression::crop_for(rgb, width, height, face) {
                readable.push(index);
                tensors.push(tensor);
            }
        }

        let readings = if self.reference_expressions {
            tensors
                .iter()
                .map(|tensor| crate::emotion::fixtures::ReferenceExpressionReader::read(tensor))
                .collect()
        } else {
            self.expression.run_batch(&tensors, prio)?
        };

        let mut out = Vec::with_capacity(all.len());
        for (index, face) in all.iter().enumerate() {
            let Some(slot) = readable.iter().position(|candidate| *candidate == index) else {
                out.push(FaceExpression::unread(
                    face.face_id,
                    face.bbox.expanded(0.20),
                ));
                continue;
            };
            let channels = readings
                .get(slot)
                .copied()
                .unwrap_or([0.0; aura_infer::onnx::fixtures::EXPRESSION_CHANNELS]);
            let target = gaze::target_for(face, all, &context.couple, context.scene);
            out.push(expression::assemble(face, channels, target));
        }
        Ok(out)
    }
}

/// Where a moment's peak leaves a frame, for the pass's second sweep.
#[derive(Debug, Clone, Default)]
pub struct PeakEffects {
    /// Per-frame proximity, `0..1`.
    pub proximity: BTreeMap<PhotoId, f32>,
    /// The peaks themselves, in moment order.
    pub peaks: Vec<MomentPeak>,
    /// Moments whose peak did not separate itself.
    pub unresolved: usize,
}

/// One reason list truncated to the contract's cap, for callers assembling by hand.
#[must_use]
pub fn capped(mut reasons: Vec<EmotionReason>) -> Vec<EmotionReason> {
    reasons.truncate(ImageEmotion::MAX_REASONS);
    reasons
}
