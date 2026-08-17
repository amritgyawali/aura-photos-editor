//! Four numbers become one, and two kinds of frame never get there.
//!
//! PHASE-12 section 6.1, and the sentence the whole module exists to satisfy: "so that a
//! catastrophic technical failure cannot be rescued by emotion and vice versa".
//!
//! ## Why a geometric mean and not a weighted sum
//!
//! Every competitor sums. A weighted sum has one property that disqualifies it here: it is
//! *compensatory*. A frame at technical 0.05 and emotion 0.95 sums to something
//! respectable, and it is a photograph of a beautiful moment that is completely out of
//! focus. Nobody delivers it.
//!
//! A weighted geometric mean is not compensatory. One factor near zero drags the product
//! near zero however good the other three are:
//!
//! ```text
//! keep_score = exp( ( w_t·ln t + w_e·ln e + w_c·ln c + w_p·ln p ) / (w_t+w_e+w_c+w_p) )
//! ```
//!
//! Section 6.1 spells it "in log space" and that is exactly how it is computed - not
//! because logs are faster, but because `ln` of a clamped floor is finite and a product of
//! four powers is not obviously so.
//!
//! ## The floor under the logarithm, and why it is not zero
//!
//! `ln 0` is negative infinity, so a zero sub-score would make every fused score zero and
//! every frame indistinguishable. [`SCORE_EPS`] is the smallest value a sub-score is
//! treated as having. It is 0.02 rather than 0.001 because the difference between "very
//! bad" and "catastrophically bad" is not information a gallery needs: both lose, and
//! ordering them precisely would let a frame at 0.001 be ranked meaningfully below one at
//! 0.004 when both readings are noise.
//!
//! ## Two things bypass the arithmetic entirely
//!
//! **Vetoes** (section 6.1) reject before fusion, and their reasons name a physical fact
//! rather than a score: the subject is out of focus, the exposure is lost, the eyes are
//! closed with no reason for them to be. That is deliberate. "This scored 0.19" is not an
//! explanation a photographer can argue with; "the subject is not in focus" is, and if
//! they disagree they can look and say so, and the override is unbeatable.
//!
//! **Promotions** (section 6.1) are not here. A coverage promotion happens in
//! [`coverage`](crate::coverage), after the score exists, because a promotion is a
//! statement about the *gallery* and not about the frame - and a frame that was promoted
//! must still carry the score it actually earned so the panel can be honest about it.
//!
//! ## The calibration ships as the identity map
//!
//! Section 6.1 asks for "per-scene isotonic calibration so `keep_score` thresholds mean
//! the same thing everywhere". Fitting one needs labelled keeper/reject pairs from real
//! weddings and there are none in this repository. [`Calibration::identity`] is what ships,
//! `KeepScore::UNCALIBRATED` is its version, and a fitted table gets a non-zero one so the
//! two can never be confused. ADR-0025 section 3, and condition C2 in the exit report.

use aura_core::{CullCode, KeepScore, SceneId};

use crate::input::Candidate;
use crate::weights::SceneWeights;

/// The smallest value a sub-score is treated as having.
///
/// See this module's header. Also the value a vetoed frame would fuse to if a veto ever
/// reached the arithmetic, which is why [`fuse`] refuses to be called on one rather than
/// returning a small number that looks like a score.
pub const SCORE_EPS: f32 = 0.02;

/// A per-scene monotone map from a fused score onto a calibrated one.
///
/// Isotonic regression, in the shape it will have when there is data to fit it: a sorted
/// list of `(input, output)` knots per scene with linear interpolation between them, and
/// the identity map when a scene has none.
///
/// It is here, unfitted, rather than absent, because the *shape* of the pipeline is the
/// part that has to be right now. A phase that added calibration later would have to
/// re-derive where it goes, what invalidates it and which version column it lives in - and
/// the answer to the third question is `KeepScore::calibration_ver`, which is already
/// stored, already compared and already the subject of `AURA-ML-5048`.
#[derive(Debug, Clone, PartialEq)]
pub struct Calibration {
    version: u16,
    knots: Vec<(SceneId, Vec<(f32, f32)>)>,
}

impl Calibration {
    /// The unfitted map. Every scene passes through unchanged.
    #[must_use]
    pub fn identity() -> Self {
        Self {
            version: KeepScore::UNCALIBRATED,
            knots: Vec::new(),
        }
    }

    /// A fitted map. `knots` must be sorted and monotone per scene; unsorted knots are
    /// sorted here rather than refused, because a monotone map built from sorted knots is
    /// still a monotone map and refusing would lose a fit to a data-preparation slip.
    #[must_use]
    pub fn fitted(version: u16, mut knots: Vec<(SceneId, Vec<(f32, f32)>)>) -> Self {
        for (_, points) in &mut knots {
            points.sort_by(|left, right| left.0.total_cmp(&right.0));
        }
        knots.sort_by_key(|(scene, _)| *scene);
        Self { version, knots }
    }

    /// Which calibration this is. `0` is the identity map.
    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }

    /// Map one fused score for one scene.
    #[must_use]
    pub fn apply(&self, scene: SceneId, raw: f32) -> f32 {
        let Some((_, points)) = self.knots.iter().find(|(id, _)| *id == scene) else {
            return raw;
        };
        interpolate(points, raw)
    }
}

/// Linear interpolation between sorted knots, clamped at both ends.
fn interpolate(points: &[(f32, f32)], at: f32) -> f32 {
    let Some(first) = points.first() else {
        return at;
    };
    if at <= first.0 {
        return first.1;
    }
    let Some(last) = points.last() else {
        return at;
    };
    if at >= last.0 {
        return last.1;
    }
    for window in points.windows(2) {
        let (Some(left), Some(right)) = (window.first(), window.get(1)) else {
            continue;
        };
        if at >= left.0 && at <= right.0 {
            let span = right.0 - left.0;
            if span <= f32::EPSILON {
                return right.1;
            }
            let t = (at - left.0) / span;
            return left.1 + t * (right.1 - left.1);
        }
    }
    at
}

/// Fuse one candidate's four sub-scores into a keep score.
///
/// Returns a [`KeepScore`] with `scene_weighted` at zero when the candidate is vetoed or
/// unanalysed - and the caller is expected to check [`KeepScore::is_vetoed`] rather than
/// compare the number, because zero is a sentinel here and not a measurement.
#[must_use]
pub fn fuse(candidate: &Candidate, weights: SceneWeights, calibration: &Calibration) -> KeepScore {
    let base = KeepScore {
        image_id: candidate.image_id,
        technical: candidate.technical,
        emotion: candidate.emotion,
        composition: candidate.composition,
        prominence: candidate.prominence,
        scene_weighted: 0.0,
        scene: candidate.scene,
        calibration_ver: calibration.version(),
    };
    if !candidate.is_eligible() {
        return base;
    }

    let total = weights.total();
    if total <= 0.0 {
        // Unreachable through the loader, which refuses a row with four zero weights.
        // Handled rather than asserted because this function is on the hot path for every
        // photograph in a wedding and a panic here would take a whole cull with it.
        return base;
    }
    let terms = [
        (weights.technical, candidate.technical),
        (weights.emotion, candidate.emotion),
        (weights.composition, candidate.composition),
        (weights.prominence, candidate.prominence),
    ];
    let mut log_sum = 0.0f32;
    for (weight, value) in terms {
        log_sum += weight * value.clamp(SCORE_EPS, 1.0).ln();
    }
    let raw = (log_sum / total).exp().clamp(0.0, 1.0);

    KeepScore {
        scene_weighted: calibration.apply(candidate.scene, raw).clamp(0.0, 1.0),
        ..base
    }
}

/// How sure a decision about this frame can be.
///
/// The evidence confidence, reduced for every input that was missing. Invariant 2 requires
/// a confidence on every decision, and a decision drawn from two of four signals has to be
/// able to say so.
///
/// The penalties are subtractive and small rather than multiplicative and large, because a
/// frame with no emotion reading is still a frame whose technical verdict is trustworthy -
/// and a product that reported 0.2 confidence on every frame of a wedding whose phase 10
/// pass had not run would train photographers to ignore the number.
#[must_use]
pub fn confidence(candidate: &Candidate, unweighted: bool) -> f32 {
    let mut confidence = candidate.confidence.clamp(0.0, 1.0);
    if !candidate.has_emotion {
        confidence -= MISSING_EMOTION_PENALTY;
    }
    if !candidate.has_composition {
        confidence -= MISSING_COMPOSITION_PENALTY;
    }
    if candidate.scene == SceneId::Unknown {
        confidence -= MISSING_SCENE_PENALTY;
    }
    if candidate.moment_id.is_none() {
        confidence -= NO_MOMENT_PENALTY;
    }
    if unweighted {
        confidence -= crate::weights::UNWEIGHTED_CONFIDENCE_PENALTY;
    }
    confidence.clamp(0.0, 1.0)
}

/// What a missing phase 10 reading costs the confidence of a decision.
///
/// The largest of the four, because emotion is the signal that most often decides between
/// two technically equal frames and its absence is what turns this engine back into a
/// sharpness filter.
pub const MISSING_EMOTION_PENALTY: f32 = 0.12;

/// What a missing phase 11 judgement costs.
pub const MISSING_COMPOSITION_PENALTY: f32 = 0.08;

/// What an unclassified scene costs.
///
/// Larger than a missing composition judgement, because invariant 7 makes every threshold
/// a function of the scene: an unknown scene means the floor, the keeper cap *and* the
/// four weights were all guesses.
pub const MISSING_SCENE_PENALTY: f32 = 0.10;

/// What being outside any moment costs.
///
/// A frame with no moment is ranked against nothing, so "the strongest frame of its
/// moment" is a statement about a group of one. Small, because a genuinely isolated
/// photograph is a normal thing at a wedding and this is not a fault.
pub const NO_MOMENT_PENALTY: f32 = 0.05;

/// Which veto, if any, applies to a frame - as a reason a photographer can check.
///
/// The decision itself is made in [`gather`](crate::gather) from the phase 09 verdict,
/// because a veto is a *measurement* and re-deriving one from a composite here would be
/// this crate inventing a second answer to a question phase 09 already answered. This
/// function only turns the stored code into the sentence.
#[must_use]
pub fn veto_text(code: CullCode) -> &'static str {
    code.user_text()
}
