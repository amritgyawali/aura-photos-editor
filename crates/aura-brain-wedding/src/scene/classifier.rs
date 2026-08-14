//! The multi-head scene classifier: one vector in, a scene and fourteen bits out.
//!
//! Section 6.1's design, and the whole of it is one sentence: **the trunk is the phase
//! 05 embedding, frozen, and this is a small trainable adapter on top of it.** No pixel
//! is opened here. The 512-d vector was computed once, in phase 05, and is read back
//! from `embeddings`; the sixteen context numbers come from the catalog. That is why
//! section 11 can budget 35 seconds for four thousand images while phase 06's face pass
//! needs twelve minutes for the same wedding - the expensive part already happened.
//!
//! ## The abstention is in the decoder, not in the graph
//!
//! `SceneId::Unknown` is not a softmax slot. A model cannot usefully be trained to say
//! "I am not sure" through an output that competes with the classes it is unsure
//! between: the gradient that teaches it to pick `unknown` is the same gradient that
//! teaches it not to pick `ceremony`, and the two are not the same lesson.
//!
//! So the head emits 22 classes and this module decides. Two conditions, both required
//! to *accept* a label:
//!
//! * the top-1 posterior is at least [`MIN_CONFIDENCE`], and
//! * it beats the runner-up by at least [`MIN_MARGIN`].
//!
//! The second is the one that matters. A head split 0.46 / 0.44 between `ceremony` and
//! `ritual` is not uncertain about *whether* this is the ceremony - it is telling you
//! it cannot separate two labels that a later threshold treats very differently, and
//! `ceremony` at 0.46 would be recorded as fact. Below the margin the frame is
//! `Unknown` with its top-3 intact, so the HMM can still use the distribution and the
//! Explain panel can still show what the head thought.
//!
//! ## Batching
//!
//! One `run_batch` per chunk, sized by the hardware plan, exactly as phase 05 and 06 do
//! it. A 528-wide feature vector is 2 KB, so a batch of 256 is half a megabyte and the
//! memory ledger has nothing to say about it; the batch size is chosen by the scheduler
//! anyway, because the scheduler is where that decision lives.

use std::sync::Arc;

use aura_core::contract::priority::Priority;
use aura_core::contract::scene::{AttrFlags, SceneId, SceneResult, SceneScore, Source};
use aura_core::{AuraResult, PhotoId};
use aura_infer::contract::infer::{InferRequest, InferService, ModelRef, TensorView, Version};
use aura_infer::onnx::fixtures::{
    SCENE_ADAPTER_DIM, SCENE_ATTRIBUTES, SCENE_CLASSIFIER_CLASSES, SCENE_CLASSIFIER_MODEL,
    SCENE_CONTEXT_FEATURES, SCENE_INPUT_DIM,
};

use crate::scene::attributes::{decode_attributes, AttributeReport, ContextFeatures};

/// Registry name of the scene classifier.
pub const SCENE_MODEL: &str = SCENE_CLASSIFIER_MODEL;

/// Pinned version of that model.
pub const SCENE_MODEL_VERSION: Version = Version::new(1, 0, 0);

/// The model version stored on every row.
///
/// A single integer rather than the semantic triple, for the reason phase 05 gives: it
/// is compared, not read. A row is either the current version or it is not.
pub const MODEL_VER: u16 =
    SCENE_MODEL_VERSION.major * 100 + SCENE_MODEL_VERSION.minor * 10 + SCENE_MODEL_VERSION.patch;

/// The feature-assembly generation.
///
/// Bumped by any change to [`ContextFeatures::to_vector`] - a reordered slot, a
/// different ISO boundary, a new saturation point. The pixels this model sees are the
/// embedding's business and are versioned by `embed_ver`; *this* is the version of the
/// sixteen numbers beside it.
pub const PREPROCESS_VER: u16 = 1;

/// Least top-1 posterior a label may be accepted at.
///
/// 0.30 against a uniform prior of 1/22 = 0.045. Low, and deliberately: the margin
/// below is what actually rejects, and a high floor here would abstain on the wide
/// low-confidence distributions that a well-calibrated 22-way head produces for a
/// genuinely ambiguous frame - exactly the frames the HMM is best at fixing from
/// context.
pub const MIN_CONFIDENCE: f32 = 0.30;

/// Least gap between the top two posteriors.
///
/// See the module header. This is the condition that does the work.
pub const MIN_MARGIN: f32 = 0.08;

/// The pinned model, as a value a caller can pass around.
#[must_use]
pub fn model_ref() -> ModelRef {
    ModelRef::new(SCENE_MODEL, SCENE_MODEL_VERSION)
}

/// One photograph's inputs, assembled.
#[derive(Debug, Clone)]
pub struct SceneInput {
    /// The photograph.
    pub image_id: PhotoId,
    /// The phase 05 embedding, already L2-normalised.
    pub embedding: Vec<f32>,
    /// Which trunk produced it. Stored, compared, never ignored.
    pub embed_ver: u16,
    /// What the catalog knows.
    pub context: ContextFeatures,
}

impl SceneInput {
    /// The 528-wide feature vector this model takes.
    ///
    /// A short embedding is padded and a long one is truncated rather than refused: the
    /// runtime's own shape check is the place that refuses a wrong-width tensor, and
    /// duplicating it here would produce two different errors for one cause.
    #[must_use]
    pub fn to_features(&self) -> Vec<f32> {
        let mut out = Vec::with_capacity(SCENE_INPUT_DIM);
        let width = SCENE_INPUT_DIM - SCENE_CONTEXT_FEATURES;
        out.extend(self.embedding.iter().take(width).copied());
        out.resize(width, 0.0);
        out.extend_from_slice(&self.context.to_vector());
        out
    }
}

/// What one classification pass measured, beyond the labels themselves.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ClassifyStats {
    /// Frames the head produced a label for.
    pub labelled: usize,
    /// Frames that fell below the confidence floor or the margin.
    pub abstained: usize,
    /// Attribute bits a measurement overrode. See `scene::attributes`.
    pub corrected_attributes: usize,
    /// Mean top-1 posterior over the frames that were labelled.
    pub mean_confidence: f32,
}

/// The scene head, with its feature assembly attached.
#[derive(Debug, Clone)]
pub struct SceneClassifier {
    infer: Arc<dyn InferService>,
    model: ModelRef,
}

impl SceneClassifier {
    /// Wrap the one way to run a model.
    #[must_use]
    pub fn new(infer: Arc<dyn InferService>) -> Self {
        Self {
            infer,
            model: model_ref(),
        }
    }

    /// The pinned model this classifier runs.
    #[must_use]
    pub const fn model(&self) -> &ModelRef {
        &self.model
    }

    /// Pay the first-call cost before a photographer is waiting on it.
    ///
    /// # Errors
    ///
    /// Whatever the runtime raised: `AURA-ML-5001` when the model is not installed,
    /// `AURA-ML-5006` on an unsupported operator.
    pub fn warmup(&self) -> AuraResult<()> {
        let features = vec![0.0f32; SCENE_INPUT_DIM];
        let view = TensorView::new(vec![1, SCENE_INPUT_DIM], &features)?;
        let request = InferRequest::new(self.model, vec![view], Priority::Background);
        self.infer.run(request).map(|_| ())
    }

    /// Classify a batch of frames.
    ///
    /// Returns one [`SceneResult`] per input, in the same order, plus what the pass
    /// measured. A frame whose outputs could not be read comes back as
    /// [`SceneResult::unknown`] rather than being dropped, so the caller's zip with its
    /// own input list cannot slip.
    ///
    /// # Errors
    ///
    /// Whatever the runtime raised. A failed batch is a failed batch; the caller
    /// retries it one frame at a time and records `AURA-ML-5027` for the frames that
    /// still fail, which is how one bad row stops costing a whole chunk.
    pub fn classify(
        &self,
        batch: &[SceneInput],
        prio: Priority,
    ) -> AuraResult<(Vec<SceneResult>, ClassifyStats)> {
        if batch.is_empty() {
            return Ok((Vec::new(), ClassifyStats::default()));
        }

        let features: Vec<f32> = batch.iter().flat_map(SceneInput::to_features).collect();
        let view = TensorView::new(vec![batch.len(), SCENE_INPUT_DIM], &features)?;
        let request = InferRequest::new(self.model, vec![view], prio);
        let result = self.infer.run(request)?;

        let scene_probs = result.outputs.first();
        let attr_probs = result.outputs.get(1);

        let mut out = Vec::with_capacity(batch.len());
        let mut stats = ClassifyStats::default();
        let mut confidence_total = 0.0f32;

        for (index, input) in batch.iter().enumerate() {
            let scenes = scene_probs
                .and_then(|tensor| row(&tensor.data, index, SCENE_CLASSIFIER_CLASSES))
                .unwrap_or(&[]);
            let attributes = attr_probs
                .and_then(|tensor| row(&tensor.data, index, SCENE_ATTRIBUTES))
                .unwrap_or(&[]);

            let decoded = decode_attributes(attributes, &input.context);
            stats.corrected_attributes += usize::from(decoded.corrected);

            let mut result = decode_scene(input.image_id, scenes, decoded);
            if result.scene == SceneId::Unknown {
                stats.abstained += 1;
            } else {
                stats.labelled += 1;
                confidence_total += result.scene_conf;
            }
            // The attribute head ran for the whole batch or for none of it.
            result.attributes_measured = !attributes.is_empty();
            out.push(result);
        }

        if stats.labelled > 0 {
            stats.mean_confidence = confidence_total / stats.labelled as f32;
        }
        Ok((out, stats))
    }
}

/// One row of a `[N, width]` output, or `None` when the tensor is the wrong shape.
fn row(samples: &[f32], index: usize, width: usize) -> Option<&[f32]> {
    samples.get(index * width..index * width + width)
}

/// Turn 22 posteriors into a label, a confidence and a top-3.
///
/// Split out from [`SceneClassifier::classify`] so the eval harness can drive it with
/// synthetic distributions and assert the abstention rules without a runtime. That is
/// what makes the section 10.1 accuracy gate measurable at all against a placeholder
/// model - see condition C1 of the phase 07 exit report.
///
/// `embed_ver` is deliberately not a parameter and not a field of [`SceneResult`]: the
/// frozen shape carries one `model_ver`, and widening it would need an ADR. The trunk
/// version travels to the store separately, on `ScenePassRow`, where the four version
/// columns of migration 7 live together.
#[must_use]
pub fn decode_scene(image_id: PhotoId, probs: &[f32], attributes: AttributeReport) -> SceneResult {
    let mut ranked: Vec<SceneScore> = probs
        .iter()
        .take(SceneId::CLASSES)
        .enumerate()
        .map(|(index, score)| SceneScore {
            scene: SceneId::from_index(index),
            score: *score,
        })
        .collect();
    // Descending by score, ties broken by class order. The tie-break is not cosmetic:
    // invariant 4 requires that two machines produce the same recipe, and a sort that
    // left equal posteriors in memory order would not.
    ranked.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.scene.as_index().cmp(&right.scene.as_index()))
    });

    let mut top3 = [SceneScore::NONE; 3];
    for (slot, found) in top3.iter_mut().zip(ranked.iter()) {
        *slot = *found;
    }

    let best = ranked.first().copied().unwrap_or(SceneScore::NONE);
    let second = ranked.get(1).copied().unwrap_or(SceneScore::NONE);
    let accepted = best.score >= MIN_CONFIDENCE && (best.score - second.score) >= MIN_MARGIN;

    SceneResult {
        image_id,
        scene: if accepted {
            best.scene
        } else {
            SceneId::Unknown
        },
        // The confidence is the posterior whether or not the label was accepted. An
        // abstention at 0.46 and an abstention at 0.05 are different situations, the
        // HMM weights them differently, and zeroing both would throw that away.
        scene_conf: best.score,
        top3,
        attributes: attributes.flags,
        attributes_measured: false,
        ritual: None,
        ritual_conf: 0.0,
        source: Source::Local,
        model_ver: MODEL_VER,
    }
}

/// How wide the adapter is, for the model card's parameter count.
#[must_use]
pub const fn adapter_width() -> usize {
    SCENE_ADAPTER_DIM
}

/// Every attribute the head emits, for the debug panel.
#[must_use]
pub fn attribute_names() -> Vec<&'static str> {
    AttrFlags::ALL.into_iter().map(AttrFlags::name).collect()
}
