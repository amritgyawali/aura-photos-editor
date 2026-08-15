//! One number, and why it is a product rather than a sum.
//!
//! PHASE-09 section 6.5: "`technical_score = product of scene-weighted sub-scores with
//! soft penalties`, deliberately not a linear sum, so one catastrophic factor cannot be
//! averaged away."
//!
//! That sentence is the whole design and it is worth spelling out. A frame that is
//! perfectly exposed, clean, well framed and **completely out of focus on the bride** has
//! four good sub-scores and one terrible one. A weighted sum gives it 0.75 and a place in
//! the gallery. A weighted *product* gives it 0.2, because a factor near zero drags a
//! product to zero regardless of what it is multiplied by - which is what a photographer
//! looking at the frame would say.
//!
//! ## Five sub-scores, and where their weights come from
//!
//! | Sub-score | Weight | Source |
//! |---|---|---|
//! | focus | `SceneProfile::subject_focus_weight`, renormalised | phase 07's config |
//! | eyes | half the focus weight | this module |
//! | exposure | [`EXPOSURE_WEIGHT`] | this module |
//! | noise | [`NOISE_WEIGHT`] | this module |
//! | motion | [`MOTION_WEIGHT`] | this module |
//!
//! Only the first is scene-conditioned by a config file, and that is deliberate rather
//! than incomplete: `SceneProfile` is a frozen contract with no exposure or noise
//! *weight* on it - it has tolerances, `max_acceptable_noise` and `max_acceptable_blur`,
//! which is a better place for the conditioning to live. **The scene enters the exposure
//! and noise sub-scores through their tolerances rather than through their weights**, and
//! adding four more weight fields to a frozen contract that ten phases read would have
//! been the worse trade. ADR-0019 section 5 records the decision.
//!
//! ## Why 0.8 means the same thing in a ceremony and on a dance floor
//!
//! Section 13's fifth acceptance criterion, and the mechanism is *construction* rather
//! than fitting. Every sub-score is computed against the scene's own tolerance before it
//! reaches this module: `noise_sigma_rel` is already "sigma over what this scene allows",
//! and [`crate::integrity::flags::soft_threshold`] is already a function of
//! `max_acceptable_blur`. A frame that is at 80 % of what its scene demands scores the
//! same whichever scene that is, and `tests/eval/integrity_eval.rs` asserts exactly
//! that across all 23 scenes.
//!
//! [`Isotonic`] is the second half of section 6.5 - "scores are mapped through a
//! per-scene isotonic regression fitted on labelled keeper/reject data" - and it ships as
//! the identity map because there is no labelled keeper/reject data in this repository.
//! The machinery is real, `ml/models/integrity/eval_integrity.py` fits it, and the
//! deferral is condition C5 in the phase 09 exit report.

use aura_core::contract::integrity::{ExposureVerdict, MotionKind};
use aura_core::{SceneId, SceneProfile};

use crate::integrity::exposure::ExposureResult;
use crate::integrity::focus::FocusResult;
use crate::integrity::motion::MotionResult;
use crate::integrity::noise::NoiseResult;

/// Weight of the exposure sub-score.
pub const EXPOSURE_WEIGHT: f32 = 0.22;
/// Weight of the noise sub-score.
pub const NOISE_WEIGHT: f32 = 0.14;
/// Weight of the motion sub-score.
pub const MOTION_WEIGHT: f32 = 0.16;

/// The lowest any sub-score may fall.
///
/// 0.08. The "soft penalties" half of section 6.5's sentence: a product with a true zero
/// in it is zero, and a frame scored zero is indistinguishable from a frame that was
/// never analysed - which collapses the distinction migration 9 spends a paragraph
/// protecting. The floor keeps a catastrophic frame near the bottom of the range without
/// putting it *at* the bottom.
pub const SUB_SCORE_FLOOR: f32 = 0.08;

/// The five sub-scores, before they are combined.
///
/// Kept as a struct rather than an array because the panel shows them individually: a
/// photographer arguing with a 0.31 wants to see which of the five caused it, and a
/// composite that cannot be decomposed is a number nobody can dispute.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SubScores {
    /// Is the right subject sharp.
    pub focus: f32,
    /// Are the important eyes open, or justifiably not.
    pub eyes: f32,
    /// Can the exposure be brought back.
    pub exposure: f32,
    /// Is it noisier than this scene carries.
    pub noise: f32,
    /// Was the motion a decision.
    pub motion: f32,
}

impl SubScores {
    /// A frame with nothing wrong with it.
    pub const PERFECT: Self = Self {
        focus: 1.0,
        eyes: 1.0,
        exposure: 1.0,
        noise: 1.0,
        motion: 1.0,
    };

    /// The five, in the order the panel lists them.
    #[must_use]
    pub const fn as_array(&self) -> [f32; 5] {
        [
            self.focus,
            self.eyes,
            self.exposure,
            self.noise,
            self.motion,
        ]
    }

    /// Which sub-score is the reason for the composite.
    ///
    /// The lowest one. What the panel puts at the top and what the review queue sorts
    /// its explanation by.
    #[must_use]
    pub fn weakest(&self) -> (&'static str, f32) {
        let names = ["focus", "eyes", "exposure", "noise", "motion"];
        let mut worst = ("focus", self.focus);
        for (name, value) in names.into_iter().zip(self.as_array()) {
            if value < worst.1 {
                worst = (name, value);
            }
        }
        worst
    }
}

/// The five weights for one scene, already normalised to sum to one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Weights {
    /// Focus.
    pub focus: f32,
    /// Eyes.
    pub eyes: f32,
    /// Exposure.
    pub exposure: f32,
    /// Noise.
    pub noise: f32,
    /// Motion.
    pub motion: f32,
}

impl Weights {
    /// Derive the weights for one scene.
    ///
    /// The eye weight is half the focus weight, everywhere. Two reasons, and the second
    /// is the real one:
    ///
    /// * a closed eye is a *binary* defect that already carries a large flat penalty in
    ///   its own sub-score, so it does not need a large weight as well;
    /// * `SceneProfile::subject_focus_weight` is phase 07's statement of "how much this
    ///   scene is about the person", and eyes matter in exactly the scenes faces do. Two
    ///   independent config numbers that would always move together are one config
    ///   number and a ratio.
    #[must_use]
    pub fn for_scene(profile: &SceneProfile) -> Self {
        let focus = profile.subject_focus_weight.clamp(0.05, 0.95);
        let eyes = focus * 0.5;
        let raw = [focus, eyes, EXPOSURE_WEIGHT, NOISE_WEIGHT, MOTION_WEIGHT];
        let total: f32 = raw.iter().sum();
        if total <= f32::EPSILON {
            return Self {
                focus: 0.2,
                eyes: 0.2,
                exposure: 0.2,
                noise: 0.2,
                motion: 0.2,
            };
        }
        Self {
            focus: focus / total,
            eyes: eyes / total,
            exposure: EXPOSURE_WEIGHT / total,
            noise: NOISE_WEIGHT / total,
            motion: MOTION_WEIGHT / total,
        }
    }
}

/// Build the five sub-scores from the analyses.
///
/// `closed_eye_penalty` is the fraction of gating faces whose closure was **not**
/// justified - computed by the caller, because whether a closure is justified is a
/// policy question and this module only does arithmetic.
#[must_use]
pub fn sub_scores(
    focus: &FocusResult,
    motion: &MotionResult,
    exposure: &ExposureResult,
    noise: &NoiseResult,
    profile: &SceneProfile,
    closed_eye_penalty: f32,
) -> SubScores {
    // Focus: the normalised subject sharpness against the scene's soft threshold, mapped
    // so that exactly meeting the threshold scores 0.6 rather than 1.0. Meeting the
    // minimum is not the same as being good, and phase 12 ranking within a moment needs
    // the range above it to be usable.
    let threshold = crate::integrity::flags::soft_threshold(profile);
    let focus_score = if threshold <= f32::EPSILON {
        1.0
    } else if focus.subject >= threshold {
        let above =
            ((focus.subject - threshold) / (1.0 - threshold).max(f32::EPSILON)).clamp(0.0, 1.0);
        0.6 + 0.4 * above
    } else {
        0.6 * (focus.subject / threshold).clamp(0.0, 1.0)
    };

    // Motion: an intentional smear costs nothing, ever. Section 6.2, as arithmetic.
    let motion_score = match motion.kind {
        MotionKind::None | MotionKind::Intentional => 1.0,
        MotionKind::CameraShake => 1.0 - 0.85 * motion.severity,
        // A moving subject is a smaller fault than a moving camera at the same severity:
        // it is usually the photograph's own subject doing something, and half the time
        // the photographer chose the shutter speed that let it happen.
        MotionKind::SubjectMotion => 1.0 - 0.55 * motion.severity,
    };

    let exposure_score = match exposure.verdict {
        ExposureVerdict::Good => 1.0,
        ExposureVerdict::Recoverable => 0.85,
        ExposureVerdict::Marginal => 0.55,
        ExposureVerdict::Lost => 0.18,
    };

    // Noise: 1.0 at or below the scene's tolerance, falling away above it. `sigma_rel`
    // is already scene-relative, which is why no profile field appears here.
    let noise_score = if noise.measured {
        let over = (noise.sigma_rel - 1.0).max(0.0);
        1.0 / (1.0 + over * 1.4)
    } else {
        // An unmeasured frame is not a noisy frame. Scoring it as one would penalise
        // every close-up with no flat area in it.
        1.0
    };

    let eye_score = 1.0 - 0.75 * closed_eye_penalty.clamp(0.0, 1.0);

    SubScores {
        focus: floor(focus_score),
        eyes: floor(eye_score),
        exposure: floor(exposure_score),
        noise: floor(noise_score),
        motion: floor(motion_score),
    }
}

/// The weighted geometric mean of the five, then the scene's calibration.
///
/// `Π sᵢ^wᵢ` with `Σwᵢ = 1`, computed in log space because five powf calls with
/// fractional exponents cost more than five logarithms and one exponential, and because
/// the log-space form makes the "one bad factor dominates" property obvious: a sub-score
/// of 0.08 contributes `w · ln(0.08) = -2.5w` to a sum whose other terms are near zero.
#[must_use]
pub fn compose(scores: SubScores, weights: Weights, calibration: &Isotonic) -> f32 {
    let terms = [
        (scores.focus, weights.focus),
        (scores.eyes, weights.eyes),
        (scores.exposure, weights.exposure),
        (scores.noise, weights.noise),
        (scores.motion, weights.motion),
    ];
    let mut log_sum = 0.0f32;
    let mut weight_sum = 0.0f32;
    for (score, weight) in terms {
        log_sum += weight * floor(score).ln();
        weight_sum += weight;
    }
    if weight_sum <= f32::EPSILON {
        return 0.0;
    }
    let raw = (log_sum / weight_sum).exp().clamp(0.0, 1.0);
    calibration.apply(raw)
}

fn floor(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(SUB_SCORE_FLOOR, 1.0)
    } else {
        SUB_SCORE_FLOOR
    }
}

/// A monotone piecewise-linear map from raw score to calibrated score.
///
/// Section 6.5's per-scene isotonic regression. Stored as knots on a fixed grid so that
/// applying it is a lookup and an interpolation rather than a search, and so that a
/// fitted map can be checked for monotonicity by reading it.
///
/// **The shipped maps are the identity**, and that is a statement about the data rather
/// than about the design: fitting an isotonic regression needs labelled keeper/reject
/// pairs, there are none in this repository, and a map fitted on synthetic fixtures
/// would encode the fixtures. `ml/models/integrity/eval_integrity.py --fit-calibration`
/// produces a real one from a labelled set and prints it in this format.
#[derive(Debug, Clone, PartialEq)]
pub struct Isotonic {
    knots: [f32; Self::KNOTS],
}

impl Isotonic {
    /// How many knots one map has. Eleven, so the grid is every tenth.
    pub const KNOTS: usize = 11;

    /// The map that changes nothing.
    pub const IDENTITY: Self = Self {
        knots: [0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0],
    };

    /// Build a map from knots, refusing a non-monotone one.
    ///
    /// A calibration that is not monotone is not a calibration: it would say that a
    /// frame which measured better scored worse, and every ranking built on it would be
    /// unstable in a way nobody could debug. Refused at construction rather than checked
    /// at use.
    #[must_use]
    pub fn from_knots(knots: [f32; Self::KNOTS]) -> Option<Self> {
        let mut previous = -1.0f32;
        for knot in knots {
            if !knot.is_finite() || knot < previous || !(0.0..=1.0).contains(&knot) {
                return None;
            }
            previous = knot;
        }
        Some(Self { knots })
    }

    /// Apply the map.
    #[must_use]
    pub fn apply(&self, raw: f32) -> f32 {
        let value = raw.clamp(0.0, 1.0);
        let span = (Self::KNOTS - 1) as f32;
        let position = value * span;
        let low = (position.floor() as usize).min(Self::KNOTS - 1);
        let high = (low + 1).min(Self::KNOTS - 1);
        let fraction = position - low as f32;
        let a = self.knots.get(low).copied().unwrap_or(value);
        let b = self.knots.get(high).copied().unwrap_or(a);
        (a + (b - a) * fraction).clamp(0.0, 1.0)
    }

    /// True when this map changes nothing.
    ///
    /// Reported by the outline so that a caller can tell a calibrated build from an
    /// uncalibrated one without reading the exit report.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        *self == Self::IDENTITY
    }
}

impl Default for Isotonic {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// The per-scene calibration maps.
#[derive(Debug, Clone, Default)]
pub struct SceneCalibration {
    maps: Vec<(SceneId, Isotonic)>,
}

impl SceneCalibration {
    /// The shipped set: the identity for every scene.
    #[must_use]
    pub fn shipped() -> Self {
        Self { maps: Vec::new() }
    }

    /// Install a fitted map for one scene.
    #[must_use]
    pub fn with(mut self, scene: SceneId, map: Isotonic) -> Self {
        self.maps.retain(|(existing, _)| *existing != scene);
        self.maps.push((scene, map));
        self
    }

    /// The map for one scene, or the identity.
    #[must_use]
    pub fn get(&self, scene: SceneId) -> Isotonic {
        self.maps
            .iter()
            .find(|(existing, _)| *existing == scene)
            .map_or(Isotonic::IDENTITY, |(_, map)| map.clone())
    }

    /// How many scenes have a fitted map. Zero in this build.
    #[must_use]
    pub fn fitted(&self) -> usize {
        self.maps.len()
    }
}
