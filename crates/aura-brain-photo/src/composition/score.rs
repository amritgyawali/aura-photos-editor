//! One number, and why it is a product rather than a sum.
//!
//! PHASE-11 section 8's seventh step: "fuse and calibrate `composition_score`". The shape
//! is phase 09's `integrity::score`, deliberately: **a weighted geometric mean, not a
//! weighted sum**, so that one catastrophic framing error cannot be averaged away by five
//! good measures.
//!
//! A frame with the horizon eleven degrees off, a cut neck and a pole out of the bride's
//! head scores 0.72 under a sum and 0.31 under a product. The second number is the one a
//! picture editor would recognise.
//!
//! ## Six sub-scores and a tie-breaker
//!
//! | Sub-score | Falls when |
//! |---|---|
//! | [`SubScores::level`] | the horizon is off by more than the scene tolerates |
//! | [`SubScores::framing`] | the frame cut somebody |
//! | [`SubScores::headroom`] | there is too little or too much space above a head |
//! | [`SubScores::placement`] | the subject is near neither the thirds nor the centre |
//! | [`SubScores::balance`] | the visual weight is all on one side |
//! | [`SubScores::background`] | it is busy, bright, merged or competing back there |
//!
//! The aesthetic reading is **not** a seventh sub-score. It is applied afterwards as a
//! bounded multiplier, and the bound is [`AESTHETIC_CAP`]. That is section 6.3's
//! requirement expressed structurally rather than as a weight somebody could raise: taste
//! can move a frame by a quarter and cannot move it from 0.3 to 0.8, whatever a rule file
//! says.
//!
//! ## Weights come from the scene, not from this file
//!
//! [`Weights::for_rule`] reads `thirds_weight`, `balance_weight` and `aesthetic_weight`
//! straight out of the rule row, and derives the other three from the scene's tolerances.
//! Invariant 7 again: there is no constant in this module that a scene could not have
//! changed, except the ones that bound what a scene is allowed to ask for.

use aura_core::SceneId;

use crate::composition::aesthetic::Aesthetic;
use crate::composition::background::Background;
use crate::composition::crop_audit::CropAudit;
use crate::composition::horizon::HorizonResult;
use crate::composition::placement::{HeadroomVerdict, Placement};
use crate::composition::rules::SceneRule;

/// The most the learned aesthetic may move the composite, either way.
///
/// A quarter. Section 6.3: "taste should break ties, not override substance." A frame that
/// geometry scored at 0.40 can reach 0.50 with perfect taste and falls to 0.30 with none,
/// and no value of `aesthetic_weight` in any rule file changes that - the cap binds
/// underneath the weight.
///
/// The number is chosen against a specific case: two frames of the same moment, both
/// technically clean, one framed better. Their geometric scores typically differ by 0.03
/// to 0.08, so a quarter is comfortably enough for taste to decide between them - which is
/// what "break ties" means - and nowhere near enough to lift a frame with a cut neck above
/// one without.
pub const AESTHETIC_CAP: f32 = 0.25;

/// The floor a sub-score is clamped to before the product.
///
/// Five per cent. A true zero in a geometric mean takes the whole composite to zero, and
/// no single framing measure should be able to do that: a photograph with a badly cut limb
/// and nothing else wrong is a photograph, and phase 12 has to be able to tell it from one
/// where everything failed at once.
pub const SUB_SCORE_FLOOR: f32 = 0.05;

/// How fast the level sub-score falls past the scene's tolerance.
///
/// One over four degrees. At the tolerance the score is 1.0; four degrees past it the
/// score is about 0.37, which is where a viewer stops seeing "slightly off" and starts
/// seeing "tilted".
pub const TILT_FALLOFF_DEG: f32 = 4.0;

/// The six sub-scores, each `0..1`, each higher-is-better.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SubScores {
    /// How level the frame is.
    pub level: f32,
    /// How much the frame kept of the people in it.
    pub framing: f32,
    /// How well the headroom fits this scene's band.
    pub headroom: f32,
    /// How well the subject is placed.
    pub placement: f32,
    /// How evenly the weight is spread.
    pub balance: f32,
    /// How quiet the background is.
    pub background: f32,
}

impl SubScores {
    /// A frame with nothing measurable in it: every sub-score neutral.
    ///
    /// One rather than a half, and it matters. A sub-score is a *penalty carrier* here;
    /// "not measured" must not be a penalty, or a photograph of a room would be punished
    /// for having no head in it.
    #[must_use]
    pub const fn neutral() -> Self {
        Self {
            level: 1.0,
            framing: 1.0,
            headroom: 1.0,
            placement: 1.0,
            balance: 1.0,
            background: 1.0,
        }
    }

    /// The weakest sub-score, for the panel's headline sentence.
    #[must_use]
    pub fn weakest(&self) -> f32 {
        [
            self.level,
            self.framing,
            self.headroom,
            self.placement,
            self.balance,
            self.background,
        ]
        .into_iter()
        .fold(f32::MAX, f32::min)
    }
}

/// Turn every measurement into a sub-score.
#[must_use]
pub fn sub_scores(
    horizon: &HorizonResult,
    audit: &CropAudit,
    placement: &Placement,
    background: &Background,
    rule: &SceneRule,
) -> SubScores {
    SubScores {
        level: level_score(horizon, rule),
        framing: framing_score(audit),
        headroom: headroom_score(placement, rule),
        placement: placement_score(placement, rule),
        balance: balance_score(placement, rule),
        background: background_score(background),
    }
}

/// How level, `0..1`.
///
/// **An intentional tilt scores 1.0**, not "1.0 minus a little". Section 6.1 treats a
/// dutch angle as style rather than error, and a style that still costs a tenth of a point
/// is a style the product disapproves of - which a photographer would notice in the
/// ordering long before they noticed it in a panel.
#[must_use]
pub fn level_score(horizon: &HorizonResult, rule: &SceneRule) -> f32 {
    if horizon.intentional || !horizon.is_measured() {
        return 1.0;
    }
    let excess = (horizon.tilt_deg.abs() - rule.tilt_tolerance_deg).max(0.0);
    if excess <= 0.0 {
        return 1.0;
    }
    // Scaled by confidence: a tilt nobody is sure about costs proportionally less, which
    // is the same direction section 6.1's "never straighten below 0.6" points in.
    let penalty = 1.0 - (-excess / TILT_FALLOFF_DEG).exp();
    (1.0 - penalty * horizon.confidence.clamp(0.0, 1.0)).clamp(0.0, 1.0)
}

/// How much of the people the frame kept, `0..1`.
///
/// The worst cut decides, not the sum of them. A frame that cuts a wrist and an ankle is
/// about as bad as one that cuts a wrist, because a viewer looks at the worst thing in a
/// photograph and stops.
#[must_use]
pub fn framing_score(audit: &CropAudit) -> f32 {
    let worst = audit
        .cuts
        .iter()
        .map(|cut| cut.severity)
        .fold(0.0f32, f32::max);
    let head = if audit.head_crop { 0.25 } else { 0.0 };
    (1.0 - worst.max(head)).clamp(0.0, 1.0)
}

/// How well the headroom fits, `0..1`.
///
/// Scaled by the scene's own `headroom_penalty`, which is the strictness knob the file's
/// header points a reader at when the product feels harsh.
#[must_use]
pub fn headroom_score(placement: &Placement, rule: &SceneRule) -> f32 {
    match placement.headroom_verdict {
        HeadroomVerdict::NotMeasured | HeadroomVerdict::InBand => 1.0,
        HeadroomVerdict::Tight | HeadroomVerdict::Excessive => {
            let deviation = if placement.headroom < rule.headroom_min {
                rule.headroom_min - placement.headroom
            } else {
                placement.headroom - rule.headroom_max
            };
            // Full penalty at a fifth of the frame outside the band, which is a large miss.
            let severity = (deviation / 0.20).clamp(0.0, 1.0);
            (1.0 - rule.headroom_penalty * severity).clamp(0.0, 1.0)
        }
    }
}

/// How well placed, `0..1`.
///
/// Only `off_placement` costs anything. Being between a power point and the centre is
/// where most subjects in most photographs are, and a score that fell off continuously
/// with distance would penalise every one of them by a little - which over four thousand
/// frames is a systematic downward drift nobody can point at.
#[must_use]
pub fn placement_score(placement: &Placement, rule: &SceneRule) -> f32 {
    if !placement.off_placement {
        return 1.0;
    }
    (1.0 - rule.thirds_weight * 0.5).clamp(0.0, 1.0)
}

/// How balanced, `0..1`.
#[must_use]
pub fn balance_score(placement: &Placement, rule: &SceneRule) -> f32 {
    (1.0 - rule.balance_weight * (1.0 - placement.balance)).clamp(0.0, 1.0)
}

/// How quiet the background is, `0..1`.
///
/// Four contributions, and the head merge is worth more than the other three because it is
/// the only one of them that is unambiguously a mistake. A busy background can be a
/// reception; a pole out of somebody's head cannot be anything.
#[must_use]
pub fn background_score(background: &Background) -> f32 {
    if !background.subject_aware {
        return 1.0;
    }
    let clutter = (background.clutter - 1.0).clamp(0.0, 1.0) * 0.30;
    let blobs = background
        .bright_blobs
        .iter()
        .map(|area| area.w * area.h)
        .sum::<f32>()
        .min(0.10)
        / 0.10
        * 0.20;
    let merge = if background.head_merge { 0.35 } else { 0.0 };
    let colour = background.colour_competition.clamp(0.0, 1.0) * 0.20;
    (1.0 - (clutter + blobs + merge + colour)).clamp(0.0, 1.0)
}

/// How much each sub-score matters in this scene.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Weights {
    /// Level.
    pub level: f32,
    /// Framing.
    pub framing: f32,
    /// Headroom.
    pub headroom: f32,
    /// Placement.
    pub placement: f32,
    /// Balance.
    pub balance: f32,
    /// Background.
    pub background: f32,
    /// How much the aesthetic multiplier is allowed to act, `0..1` of [`AESTHETIC_CAP`].
    pub aesthetic: f32,
}

impl Weights {
    /// Read the weights out of a scene's rule row.
    ///
    /// Three come straight from the file. The other three are derived from the scene's own
    /// tolerances, so that a scene which has declared it does not care about tilt does not
    /// also need a separate weight saying so:
    ///
    /// * **level** falls as `tilt_tolerance_deg` rises - a dance floor tolerating six
    ///   degrees has declared that being level is not the point there;
    /// * **framing** is the scene's `joint_cut_weight`, which is already the statement
    ///   "cuts matter this much here";
    /// * **headroom** falls to zero when `headroom_penalty` is zero, which is how the
    ///   `details`, `rings` and `venue` rows switch the whole measure off.
    #[must_use]
    pub fn for_rule(rule: &SceneRule) -> Self {
        Self {
            level: (1.20 - rule.tilt_tolerance_deg * 0.12).clamp(0.20, 1.20),
            framing: rule.joint_cut_weight.clamp(0.0, 1.5),
            headroom: (rule.headroom_penalty / 0.14).clamp(0.0, 1.0),
            placement: rule.thirds_weight,
            balance: rule.balance_weight,
            background: 0.60,
            aesthetic: (rule.aesthetic_weight / 0.50).clamp(0.0, 1.0),
        }
    }

    /// The weight of each sub-score, in [`SubScores`] field order.
    #[must_use]
    pub const fn as_array(&self) -> [f32; 6] {
        [
            self.level,
            self.framing,
            self.headroom,
            self.placement,
            self.balance,
            self.background,
        ]
    }
}

/// The per-scene calibration curve.
///
/// Ships as the identity map, and the exit report says so. Fitting one needs labelled
/// well-composed/badly-composed pairs per scene and there are none in this repository -
/// the same gap phase 09's `SceneCalibration` has, one phase later.
///
/// The shape is kept because it is where the fitted curve goes, and because a caller that
/// has to be changed to accept a calibration later is a caller that will not be.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneCurve {
    /// Multiplied before the offset.
    pub gain: f32,
    /// Added after the gain.
    pub offset: f32,
}

impl SceneCurve {
    /// The identity.
    pub const IDENTITY: Self = Self {
        gain: 1.0,
        offset: 0.0,
    };

    /// Apply the curve.
    #[must_use]
    pub fn apply(&self, raw: f32) -> f32 {
        raw.mul_add(self.gain, self.offset).clamp(0.0, 1.0)
    }
}

/// One curve per scene.
#[derive(Debug, Clone)]
pub struct SceneCalibration {
    curves: Vec<(SceneId, SceneCurve)>,
}

impl SceneCalibration {
    /// What ships: the identity for every scene.
    #[must_use]
    pub fn shipped() -> Self {
        Self {
            curves: SceneId::ALL
                .into_iter()
                .map(|scene| (scene, SceneCurve::IDENTITY))
                .collect(),
        }
    }

    /// Install a fitted curve for one scene.
    #[must_use]
    pub fn with(mut self, scene: SceneId, curve: SceneCurve) -> Self {
        if let Some(slot) = self
            .curves
            .iter_mut()
            .find(|(candidate, _)| *candidate == scene)
        {
            slot.1 = curve;
        } else {
            self.curves.push((scene, curve));
        }
        self
    }

    /// The curve for one scene, or the identity.
    #[must_use]
    pub fn get(&self, scene: SceneId) -> SceneCurve {
        self.curves
            .iter()
            .find(|(candidate, _)| *candidate == scene)
            .map_or(SceneCurve::IDENTITY, |(_, curve)| *curve)
    }
}

/// Fuse the sub-scores, apply the aesthetic multiplier, then calibrate.
///
/// The weighted geometric mean is computed in log space, which is the numerically stable
/// way to write a product of six terms and is also the form that makes the floor obvious:
/// every term is clamped to [`SUB_SCORE_FLOOR`] before its logarithm is taken, so no single
/// measure can send the composite to zero.
///
/// A frame whose weights all come out zero - possible in principle if a rule row switched
/// every measure off - returns the aesthetic multiplier applied to a neutral 1.0 rather
/// than dividing by zero.
#[must_use]
pub fn compose(sub: SubScores, weights: Weights, aesthetic: Aesthetic, curve: &SceneCurve) -> f32 {
    let terms = [
        sub.level,
        sub.framing,
        sub.headroom,
        sub.placement,
        sub.balance,
        sub.background,
    ];
    let mut log_sum = 0.0f32;
    let mut weight_sum = 0.0f32;
    for (value, weight) in terms.into_iter().zip(weights.as_array()) {
        if weight <= 0.0 {
            continue;
        }
        log_sum += weight * value.clamp(SUB_SCORE_FLOOR, 1.0).ln();
        weight_sum += weight;
    }
    let geometric = if weight_sum <= f32::EPSILON {
        1.0
    } else {
        (log_sum / weight_sum).exp()
    };

    // Taste, bounded. `aesthetic.value - 0.5` is the deviation from "no opinion", doubled
    // to reach the whole cap at the extremes, scaled by how much this scene lets taste act.
    let swing = AESTHETIC_CAP * weights.aesthetic.clamp(0.0, 1.0);
    let multiplier = 1.0 + swing * (aesthetic.value.clamp(0.0, 1.0) - 0.5) * 2.0;

    curve.apply((geometric * multiplier).clamp(0.0, 1.0))
}
