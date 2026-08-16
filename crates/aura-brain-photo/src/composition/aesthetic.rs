//! Taste, scoped honestly.
//!
//! PHASE-11 section 6.3, whose title in the phase document is "learned aesthetics,
//! honestly scoped" - and the honesty is the design rather than a caveat attached to it.
//!
//! ## What the head is given, and what it is therefore able to learn
//!
//! Twelve geometric measurements and a scene one-hot. Not pixels. Section 6.3:
//!
//! > "Feature inputs include the geometric measures, so the model learns *how much* each
//! > violation matters rather than re-deriving geometry."
//!
//! That sentence is the whole architecture. The horizon is already measured to a tenth of
//! a degree by [`super::horizon`]; a small convolutional head trained on four thousand
//! pairwise comparisons would rediscover a worse version of it and spend most of its
//! capacity doing so. What photographer choices actually teach - the only thing they can
//! teach from that much data - is the *exchange rate* between violations, per scene: how
//! many degrees of tilt is one cut wrist worth in a couple portrait, and is the answer
//! different on a dance floor.
//!
//! ## The cap is applied here, not left to phase 12 to remember
//!
//! Section 6.3's last line: "aesthetic score is capped in influence: Phase 12 weights it
//! below technical integrity and emotion, because taste should break ties, not override
//! substance." A cap that lives in the consuming phase is a cap that is forgotten by the
//! second consuming phase. [`super::score::AESTHETIC_CAP`] binds inside the composite, and
//! [`crate::composition::rules::AESTHETIC_WEIGHT_CAP`] binds what a rule file may ask for.
//! Two caps rather than one, because they answer different questions: how much taste may
//! move *this score*, and how much a product manager may ask for in *this scene*.
//!
//! ## The placeholder, said plainly
//!
//! The shipped `aesthetic_head` weights are untrained. Its output over a real frame is a
//! random projection of thirty-five numbers. Section 10.1's agreement gate of 0.78 is
//! therefore measured against [`Aesthetic::from_features`] - the deterministic linear
//! reference model below - rather than against the head, and the phase 11 exit report says
//! so where the number is reported. Condition C1.
//!
//! The reference model exists for a second reason that outlives the placeholder: it is
//! what the head is *compared against*. A learned ranker that cannot beat a twelve-term
//! linear model on held-out photographer pairs has learned nothing worth shipping, and
//! `ml/models/composition/eval_composition.py` makes that comparison the gate.

use std::sync::Arc;

use aura_core::{AuraResult, Priority, SceneId};
use aura_infer::contract::infer::{InferService, ModelRef, TensorView};
use aura_infer::onnx::fixtures::{AESTHETIC_FEATURES, AESTHETIC_GEOMETRY, AESTHETIC_SCENES};

use crate::composition::background::Background;
use crate::composition::horizon::HorizonResult;
use crate::composition::keypoints::{AESTHETIC_MODEL, AESTHETIC_MODEL_VERSION};
use crate::composition::placement::Placement;
use crate::composition::rules::SceneRule;

/// Whether the pinned aesthetic weights have passed the photographer-pair gate.
///
/// This is deliberately a compile-time release assertion rather than an inference-success
/// check. The deterministic fixture currently registered as `aesthetic_head` is a valid
/// ONNX graph, so it can execute successfully, but successful execution does not make its
/// random weights learned. A release may set this to `true` only when the pinned model card
/// records a passing held-out evaluation for these exact weights.
pub const AESTHETIC_HEAD_TRAINED: bool = false;

/// The twelve geometric slots, in the order the feature vector carries them.
///
/// The order **is** the model's input order, so renumbering it relabels a trained head -
/// the same rule `FaceExpression::channels` and `Interaction::ALL` state. It is written as
/// an enum rather than as a comment above a `Vec` push sequence because the training
/// script, the reference model and the eval harness all index it, and three copies of an
/// order is two chances to get one wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    /// Absolute tilt, over twelve degrees, clamped.
    Tilt,
    /// How sure the tilt is.
    HorizonConfidence,
    /// Headroom over 0.40 of the frame, clamped.
    Headroom,
    /// How far outside the scene's headroom band, over 0.20, clamped. Zero inside it.
    HeadroomDeviation,
    /// Distance to the nearest power point, doubled.
    ThirdsOffset,
    /// Distance to the centre, doubled.
    CentreOffset,
    /// Balance, as measured.
    Balance,
    /// Negative space, as measured.
    NegativeSpace,
    /// Clutter relative to the scene, halved and clamped.
    Clutter,
    /// Total area of bright blobs, over a tenth of the frame, clamped.
    BrightBlobArea,
    /// One when something grows out of a head.
    HeadMerge,
    /// Colour competition, as measured.
    ColourCompetition,
}

impl Feature {
    /// Every feature, in the model's input order.
    pub const ALL: [Self; AESTHETIC_GEOMETRY] = [
        Self::Tilt,
        Self::HorizonConfidence,
        Self::Headroom,
        Self::HeadroomDeviation,
        Self::ThirdsOffset,
        Self::CentreOffset,
        Self::Balance,
        Self::NegativeSpace,
        Self::Clutter,
        Self::BrightBlobArea,
        Self::HeadMerge,
        Self::ColourCompetition,
    ];

    /// The slug the training script and the eval harness use.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tilt => "tilt",
            Self::HorizonConfidence => "horizon_confidence",
            Self::Headroom => "headroom",
            Self::HeadroomDeviation => "headroom_deviation",
            Self::ThirdsOffset => "thirds_offset",
            Self::CentreOffset => "centre_offset",
            Self::Balance => "balance",
            Self::NegativeSpace => "negative_space",
            Self::Clutter => "clutter",
            Self::BrightBlobArea => "bright_blob_area",
            Self::HeadMerge => "head_merge",
            Self::ColourCompetition => "colour_competition",
        }
    }

    /// How this feature moves the reference model's score, per unit.
    ///
    /// **Four of the twelve are set by argument rather than fitted**, because eight
    /// authored comparisons cannot identify twelve coefficients - the same gap phase 10's
    /// ranker has and reports, one phase later and with the same honesty. The four are
    /// `HorizonConfidence` (which is a confidence, not a quality, and is weighted at zero
    /// so that an unmeasurable horizon neither helps nor hurts), `Headroom`,
    /// `NegativeSpace` and `CentreOffset`. Condition C2 of the phase 11 exit report.
    ///
    /// Signs are the argument. Balance is the only positive term of any size: a balanced
    /// frame is better and everything else on this list is a way of being worse.
    #[must_use]
    pub const fn weight(self) -> f32 {
        match self {
            Self::Tilt => -0.34,
            Self::HorizonConfidence => 0.00,
            Self::Headroom => -0.05,
            Self::HeadroomDeviation => -0.22,
            Self::ThirdsOffset => -0.14,
            Self::CentreOffset => -0.04,
            Self::Balance => 0.30,
            Self::NegativeSpace => 0.06,
            Self::Clutter => -0.26,
            Self::BrightBlobArea => -0.18,
            Self::HeadMerge => -0.28,
            Self::ColourCompetition => -0.16,
        }
    }
}

/// Build the thirty-five-number feature vector.
///
/// Every geometric slot is squashed into `0..1` here rather than in the model, so that a
/// stored feature vector means the same thing whichever head reads it - and so that the
/// training script in `ml/models/composition/` can be fed exactly what the product feeds
/// the head without reimplementing a normalisation.
#[must_use]
pub fn features(
    horizon: &HorizonResult,
    placement: &Placement,
    background: &Background,
    rule: &SceneRule,
    scene: SceneId,
) -> Vec<f32> {
    let blob_area: f32 = background
        .bright_blobs
        .iter()
        .map(|area| area.w * area.h)
        .sum();
    let deviation = if placement.headroom < rule.headroom_min {
        rule.headroom_min - placement.headroom
    } else if placement.headroom > rule.headroom_max {
        placement.headroom - rule.headroom_max
    } else {
        0.0
    };

    let mut out = Vec::with_capacity(AESTHETIC_FEATURES);
    for feature in Feature::ALL {
        let value = match feature {
            Feature::Tilt => (horizon.tilt_deg.abs() / 12.0).clamp(0.0, 1.0),
            Feature::HorizonConfidence => horizon.confidence.clamp(0.0, 1.0),
            Feature::Headroom => (placement.headroom.max(0.0) / 0.40).clamp(0.0, 1.0),
            Feature::HeadroomDeviation => (deviation / 0.20).clamp(0.0, 1.0),
            Feature::ThirdsOffset => (placement.thirds_offset * 2.0).clamp(0.0, 1.0),
            Feature::CentreOffset => (placement.centre_offset * 2.0).clamp(0.0, 1.0),
            Feature::Balance => placement.balance.clamp(0.0, 1.0),
            Feature::NegativeSpace => placement.negative_space.clamp(0.0, 1.0),
            Feature::Clutter => (background.clutter / 2.0).clamp(0.0, 1.0),
            Feature::BrightBlobArea => (blob_area / 0.10).clamp(0.0, 1.0),
            Feature::HeadMerge => f32::from(u8::from(background.head_merge)),
            Feature::ColourCompetition => background.colour_competition.clamp(0.0, 1.0),
        };
        out.push(value);
    }

    let slot = scene.as_index().min(AESTHETIC_SCENES - 1);
    for index in 0..AESTHETIC_SCENES {
        out.push(f32::from(u8::from(index == slot)));
    }
    out
}

/// What the aesthetic stage produced.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aesthetic {
    /// The reading, `0..1`.
    pub value: f32,
    /// True when the learned head produced it.
    ///
    /// False means the reference model did, and the judgement carries
    /// `CompositionCode::AestheticUnavailable` and loses confidence. A caller must be able
    /// to tell "the head said 0.5" from "there was no head and geometry alone said 0.5".
    pub learned: bool,
}

impl Aesthetic {
    /// The deterministic reference model: a logistic over the twelve weighted features.
    ///
    /// Centred at 0.5 for an all-zero frame, so that "nothing measured" reads as "nothing
    /// to say" rather than as "beautiful" or "ugly". The gain of 2.2 is chosen so that a
    /// frame at the worst end of every term lands near 0.05 rather than at 0.00 - a score
    /// of exactly zero would multiply the whole composite to zero in
    /// [`super::score::compose`], and no framing error should be able to do that on its
    /// own.
    #[must_use]
    pub fn from_features(features: &[f32]) -> Self {
        let mut sum = 0.0f32;
        for (index, feature) in Feature::ALL.into_iter().enumerate() {
            sum += features.get(index).copied().unwrap_or(0.0) * feature.weight();
        }
        let value = 1.0 / (1.0 + (-2.2 * sum).exp());
        Self {
            value: value.clamp(0.02, 0.98),
            learned: false,
        }
    }
}

/// The learned aesthetic head.
#[derive(Debug, Clone)]
pub struct AestheticHead {
    infer: Arc<dyn InferService>,
    model: ModelRef,
}

impl AestheticHead {
    /// Wrap the one way to run a model.
    #[must_use]
    pub fn new(infer: Arc<dyn InferService>) -> Self {
        Self {
            infer,
            model: ModelRef::new(AESTHETIC_MODEL, AESTHETIC_MODEL_VERSION),
        }
    }

    /// The model this runner is pinned to.
    #[must_use]
    pub const fn model(&self) -> ModelRef {
        self.model
    }

    /// True only when the pinned weights are approved for product judgements.
    #[must_use]
    pub const fn is_trained(&self) -> bool {
        AESTHETIC_HEAD_TRAINED
    }

    /// Load the head before a photographer is waiting on it.
    ///
    /// # Errors
    ///
    /// Whatever the loader raised.
    pub fn warmup(&self) -> AuraResult<()> {
        self.infer.warmup(&[self.model])
    }

    /// Score a batch of feature vectors.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5007` when the tensor shape disagrees with the manifest, `AURA-ML-5011`
    /// when the work was cancelled, and whatever the loader raised.
    pub fn run_batch(&self, batch: &[Vec<f32>], prio: Priority) -> AuraResult<Vec<f32>> {
        if batch.is_empty() {
            return Ok(Vec::new());
        }
        let mut inputs = Vec::with_capacity(batch.len());
        for features in batch {
            inputs.push(vec![TensorView::new(
                vec![1, AESTHETIC_FEATURES],
                features,
            )?]);
        }
        let results = self.infer.run_batch(self.model, inputs, prio)?;
        let mut out = Vec::with_capacity(results.len());
        for result in results {
            let Some(tensor) = result.outputs.first() else {
                return Err(aura_core::errors::ml::shape_mismatch(
                    "one aesthetic tensor",
                    "no outputs",
                ));
            };
            out.push(tensor.data.first().copied().unwrap_or(0.5).clamp(0.0, 1.0));
        }
        Ok(out)
    }
}
