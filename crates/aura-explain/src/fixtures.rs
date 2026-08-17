//! Synthetic decisions and synthetic outcomes, with the answer known by construction.
//!
//! Everything the phase 13 gates measure is in this file, and the reason to read it first is
//! that it is also the honest limit of what those gates prove.
//!
//! * The **decisions** are real in shape and authored in content: real reason codes from the
//!   frozen vocabularies, real evidence rectangles, real canonical outputs. What they are
//!   not is a wedding.
//! * The **outcomes** are drawn from a stated model - an overconfident predictor, a
//!   well-calibrated one, an underconfident one - by a deterministic generator. That makes
//!   the expected calibration error of each of them *known*, which is what lets the gate
//!   assert that the estimator is right. It says nothing about how often AURA is right.
//!
//! Condition C2 of the phase 13 exit report is exactly this sentence.
//!
//! ## Why the generator is a linear congruential one
//!
//! Because it has to produce identical numbers on every machine and every architecture, and
//! `rand` does not promise that across versions. Sixteen lines of arithmetic with a fixed
//! multiplier do, and a fixture set that differed between two developers' machines would
//! make every determinism gate in this phase meaningless.

use aura_core::contract::ids::DecisionId;
use aura_core::contract::ledger::{
    DecisionKind, DecisionSource, DecisionSubject, Evidence, LedgerDecision, LedgerReason,
};
use aura_core::{Box2, CullCode, PhotoId, ProjectId};

use crate::calibration::Outcome;
use crate::decision::DecisionBuilder;
use crate::replay::{Recomputed, ReplaySource};

/// A deterministic 64-bit linear congruential generator.
///
/// Numerical Recipes' constants. Not cryptographic and not trying to be: the requirement is
/// reproducibility across machines, which is the one property a system generator does not
/// offer.
#[derive(Debug, Clone, Copy)]
pub struct Lcg(u64);

impl Lcg {
    /// A generator from a seed.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// The next value, `0..1`.
    pub fn next_unit(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        // The top 24 bits, which are the ones an LCG gets right.
        let bits = (self.0 >> 40) as u32;
        f32::from_bits(0x3f80_0000 | (bits & 0x007f_ffff)) - 1.0
    }
}

/// How a synthetic predictor is wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutcomeShape {
    /// Says 0.9 and is right 0.9 of the time. Expected calibration error near zero.
    Calibrated,
    /// Says 0.9 and is right 0.75 of the time. The failure mode section 12 names: an
    /// overconfident model makes Zero-Touch unsafe.
    Overconfident,
    /// Says 0.6 and is right 0.8 of the time. Harmless to a client and expensive to a
    /// photographer, because it fills a review queue with decisions that were fine.
    Underconfident,
}

impl OutcomeShape {
    /// The probability that a prediction of `confidence` is actually right.
    #[must_use]
    pub fn truth(self, confidence: f32) -> f32 {
        match self {
            Self::Calibrated => confidence,
            // A fixed 0.15 shortfall in the middle of the range, tapering at both ends so
            // that the mapping stays inside 0..1 and stays monotone - a non-monotone truth
            // curve would make the isotonic fit's own guarantee untestable.
            Self::Overconfident => {
                (confidence - 0.15 * confidence * (1.0 - confidence) * 4.0).clamp(0.0, 1.0)
            }
            Self::Underconfident => {
                (confidence + 0.20 * confidence * (1.0 - confidence) * 4.0).clamp(0.0, 1.0)
            }
        }
    }
}

/// A run of labelled outcomes from one synthetic predictor.
///
/// The confidences are spread over `0.5..1.0` rather than `0..1`, because that is the range
/// a decision actually arrives in: nothing in this product records a decision it believes at
/// 0.02, and a fixture that pretended otherwise would measure the estimator on numbers it
/// never sees.
#[must_use]
pub fn outcomes(shape: OutcomeShape, count: usize, seed: u64) -> Vec<Outcome> {
    let mut rng = Lcg::new(seed);
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let confidence = 0.5 + rng.next_unit() * 0.5;
        let truth = shape.truth(confidence);
        let correct = rng.next_unit() < truth;
        out.push(Outcome::new(confidence, correct));
    }
    out
}

/// One synthetic cull decision, keeping a frame.
///
/// Three reasons, two of them with evidence, at a stated confidence. Everything a ledger row
/// needs and nothing a wedding would have.
#[must_use]
pub fn kept(
    project: ProjectId,
    image: PhotoId,
    runner_up: PhotoId,
    confidence: f32,
) -> DecisionBuilder {
    DecisionBuilder::new(project, DecisionKind::Cull, DecisionSubject::Image(image))
        .output_bool("kept", true)
        .output_num("keep_score", confidence)
        .confidence(confidence)
        .source(DecisionSource::Local)
        .input("image", image.to_db())
        .input_num("technical", 0.82)
        .input_num("emotion", 0.64)
        .model("cull_subscores", 1)
        .config("cull_analysis", 1)
        .reason(LedgerReason::about(
            CullCode::MomentWinner.as_str(),
            CullCode::MomentWinner.user_text(),
            0.6,
            vec![runner_up],
        ))
        .reason(LedgerReason::at(
            "subject_soft",
            "the subject is softer than this camera should manage in this kind of photograph",
            -0.2,
            Box2 {
                x: 0.4,
                y: 0.3,
                w: 0.2,
                h: 0.2,
            },
        ))
        .reason(LedgerReason::plain(
            CullCode::PeakFrame.as_str(),
            CullCode::PeakFrame.user_text(),
            0.3,
        ))
}

/// One synthetic cull decision, rejecting a frame.
#[must_use]
pub fn dropped(
    project: ProjectId,
    image: PhotoId,
    kept_instead: PhotoId,
    score: f32,
) -> DecisionBuilder {
    DecisionBuilder::new(project, DecisionKind::Cull, DecisionSubject::Image(image))
        .output_bool("kept", false)
        .output_num("keep_score", score)
        .confidence(score)
        .source(DecisionSource::Local)
        .input("image", image.to_db())
        .model("cull_subscores", 1)
        .config("cull_analysis", 1)
        .reason(LedgerReason::about(
            CullCode::LostMomentRank.as_str(),
            CullCode::LostMomentRank.user_text(),
            -0.5,
            vec![kept_instead],
        ))
        .reason(LedgerReason::plain(
            CullCode::NearDuplicate.as_str(),
            CullCode::NearDuplicate.user_text(),
            -0.3,
        ))
}

/// One synthetic decision the photographer made.
#[must_use]
pub fn user_kept(project: ProjectId, image: PhotoId) -> DecisionBuilder {
    DecisionBuilder::new(project, DecisionKind::Cull, DecisionSubject::Image(image))
        .output_bool("kept", true)
        .confidence(1.0)
        .source(DecisionSource::User)
        .input("image", image.to_db())
        .reason(LedgerReason::plain(
            CullCode::UserKept.as_str(),
            CullCode::UserKept.user_text(),
            1.0,
        ))
}

/// A ledger's worth of synthetic decisions, alternating keeps and rejections.
///
/// `built_at` is the timestamp of the first decision; each subsequent one is a second later,
/// so `history` and the compaction pass have an order to work with that does not depend on a
/// clock.
#[must_use]
pub fn wedding(project: ProjectId, frames: usize, built_at: i64) -> Vec<LedgerDecision> {
    let mut rng = Lcg::new(0x005e_ed13);
    let images: Vec<PhotoId> = (0..frames.max(2)).map(|_| PhotoId::new()).collect();
    let mut out = Vec::with_capacity(images.len());
    for (index, image) in images.iter().enumerate() {
        let other = images
            .get(index.wrapping_add(1) % images.len())
            .copied()
            .unwrap_or(*image);
        let confidence = 0.55 + rng.next_unit() * 0.44;
        let builder = if index % 3 == 0 {
            kept(project, *image, other, confidence)
        } else {
            dropped(project, *image, other, confidence * 0.6)
        };
        out.push(builder.build(
            DecisionId::new(),
            built_at.saturating_add(i64::try_from(index).unwrap_or(0).saturating_mul(1_000)),
        ));
    }
    out
}

/// A replay source that reproduces exactly what it was given.
///
/// The determinism gate's control: a source that cannot disagree proves the comparison sees
/// agreement. [`DriftingSource`] is the other half.
#[derive(Debug, Clone, Copy, Default)]
pub struct FaithfulSource;

impl ReplaySource for FaithfulSource {
    fn recompute(&self, decision: &LedgerDecision) -> aura_core::AuraResult<Recomputed> {
        Ok(Recomputed {
            inputs_hash: decision.inputs_hash,
            outputs_json: decision.outputs_json.clone(),
        })
    }
}

/// A replay source that decides differently.
///
/// `moved_inputs` says which of the two failures it simulates: an upgrade (the question
/// changed) or a determinism defect (the question did not).
#[derive(Debug, Clone, Copy)]
pub struct DriftingSource {
    /// True to change the inputs hash as well as the outputs.
    pub moved_inputs: bool,
}

impl ReplaySource for DriftingSource {
    fn recompute(&self, decision: &LedgerDecision) -> aura_core::AuraResult<Recomputed> {
        Ok(Recomputed {
            inputs_hash: if self.moved_inputs {
                decision.inputs_hash ^ 0xffff_ffff
            } else {
                decision.inputs_hash
            },
            outputs_json: decision.outputs_json.replace("true", "false"),
        })
    }
}

/// A reason with an evidence crop, for a test that needs one.
#[must_use]
pub fn evidence_crop() -> Evidence {
    Evidence::Crop(Box2 {
        x: 0.25,
        y: 0.25,
        w: 0.5,
        h: 0.5,
    })
}
