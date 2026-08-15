//! The nine features, the Bradley-Terry utility, the calibration and the reasons.
//!
//! ## Why a linear ranker and not a network
//!
//! Section 6.4 asks for a Bradley-Terry ranker fitted on pairwise "which of these two
//! would you deliver?" comparisons. A Bradley-Terry model with a linear utility says
//!
//! ```text
//! P(a beats b) = sigmoid(u(a) - u(b)),   u(x) = w . f(x)
//! ```
//!
//! so the score *is* the utility, up to the per-scene calibration. Three properties come
//! out of that shape and all three are load-bearing:
//!
//! * **The coefficients are readable.** `emotion_weights.toml`'s `[ranker]` table is a
//!   list a product manager can argue with. A two-layer network fitted to the same data
//!   would rank about as well and could not be reviewed.
//! * **A photographer's disagreement is expressible.** Every reason this module writes
//!   names one feature and carries its contribution, so "why is this frame above that
//!   one" has an arithmetic answer rather than a shrug.
//! * **Phase 30 can personalise it.** Section 6.4: "the same mechanism is later reused
//!   for per-user personalisation in Phase 30, so the interface is designed for it now."
//!   Moving nine coefficients towards one photographer's comparisons is a well-posed
//!   problem; moving a network is not.
//!
//! ## The nine features
//!
//! Every one is in `0..1` before its coefficient is applied, which is what makes the
//! coefficients comparable with each other and what `COEFFICIENT_RANGE` is set against.
//!
//! | # | Feature | What it reads |
//! |---|---|---|
//! | 0 | `subject_expression` | the scene-weighted channel mix over the frame's faces, prominence-weighted |
//! | 1 | `genuineness` | genuineness times smile, so it cannot credit a face that is not smiling |
//! | 2 | `interaction` | the strongest interaction, times the scene's interaction weight, times a milestone bonus |
//! | 3 | `peak` | peak proximity, times the scene's peak weight |
//! | 4 | `reaction` | the reaction bonus, times the scene's reaction weight |
//! | 5 | `mutual_gaze` | one when two people are looking at each other |
//! | 6 | `composure` | composure times the scene's composure weight, and **the one the cultural argument turns on** |
//! | 7 | `tears` | the strongest tear reading above `TEARS_CERTAIN`, and zero below it |
//! | 8 | `group` | how many faces are expressing something, normalised |
//!
//! Feature 6 is separate from feature 0 even though `composed` is one of the eight
//! channels feature 0 mixes. That redundancy is deliberate: it gives the ranker a term
//! it can weight *independently of the scene weights*, so a build that discovers its
//! composure handling is wrong can fix it in one coefficient rather than in twenty-three
//! scene rows.
//!
//! ## Nothing here decides anything
//!
//! Section 2.2 puts final selection in phase 12. This module produces a number and a list
//! of sentences. There is no threshold in it above which a frame is kept, and there is no
//! function that returns a boolean about a photograph's fate.

use aura_core::contract::emotion::{
    EmotionCode, EmotionReason, FaceExpression, ImageEmotion, Interaction, TEARS_CERTAIN,
};
use aura_core::{IdentityId, SceneId};
use std::collections::BTreeMap;

use crate::emotion::peak;
use crate::emotion::weights::{Calibration, Ranker, SceneWeights};

/// How much a milestone interaction is worth over an ordinary one.
///
/// `Interaction::is_milestone` names four - kiss, ring exchange, blessing, tears wiped -
/// and this is the multiplier they get. 1.25 rather than 2.0 because the interaction term
/// is one of nine and a milestone that doubled it would let a weakly detected kiss
/// outrank a frame with three people crying in it.
pub const MILESTONE_BONUS: f32 = 1.25;

/// How many expressive faces count as a full group.
///
/// Four. Past four the frame is a crowd and the marginal face changes nothing about
/// whether it is worth delivering.
pub const GROUP_FULL: f32 = 4.0;

/// The weight a face with no assigned identity carries in the prominence mean.
///
/// Not zero. A wedding whose face pass has run but whose clustering has not is a wedding
/// where every face is unassigned, and zeroing them would make every frame score its
/// bias - which looks like a broken product rather than like a missing pass.
pub const UNASSIGNED_PROMINENCE: f32 = 0.35;

/// Everything one frame's score is computed from.
#[derive(Debug, Clone)]
pub struct Inputs<'a> {
    /// Every face's reading.
    pub faces: &'a [FaceExpression],
    /// Per-identity prominence from phase 06, when a face pass has run.
    pub prominence: &'a BTreeMap<IdentityId, f32>,
    /// The interactions detected, strongest first.
    pub interactions: &'a [(Interaction, f32)],
    /// True when two faces are looking at each other.
    pub mutual_gaze: bool,
    /// Where the frame sits on its moment's curve, `0..1`.
    pub peak_proximity: f32,
    /// The reaction bonus, or zero when this frame is not a reaction.
    pub reaction_bonus: f32,
    /// What phase 07 says the frame is of.
    pub scene: SceneId,
    /// True when a scene label was actually present, for the confidence.
    pub scene_known: bool,
    /// True when a face pass has run over this frame.
    pub faces_scanned: bool,
}

/// The nine features, in `Ranker::FEATURE_NAMES` order.
#[must_use]
pub fn features(inputs: &Inputs<'_>, weights: &SceneWeights) -> [f32; Ranker::FEATURES] {
    let expression = subject_expression(inputs, weights);
    let genuine = prominence_weighted(inputs, |face| {
        // Genuineness times smile: a face that is not smiling cannot be credited for the
        // unposedness of a smile it does not have. The multiplication is what makes
        // `[scene.family_portrait]`'s near-zero genuineness weight safe - it damps a
        // term rather than penalising every smile in the frame.
        face.genuineness * face.smile
    }) * weights.genuineness();

    let interaction = inputs.interactions.first().map_or(0.0, |(kind, strength)| {
        let milestone = if kind.is_milestone() {
            MILESTONE_BONUS
        } else {
            1.0
        };
        strength * weights.interaction * milestone
    });

    let peak_term = inputs.peak_proximity.clamp(0.0, 1.0) * weights.peak;
    let reaction = inputs.reaction_bonus.clamp(0.0, 1.0) * weights.reaction;
    let mutual = if inputs.mutual_gaze {
        weights.mutual_gaze
    } else {
        0.0
    };
    let composure = prominence_weighted(inputs, |face| face.composed) * weights.composed();
    let tears = inputs
        .faces
        .iter()
        .filter(|face| face.reads_as_crying())
        .map(|face| face.tears)
        .fold(0.0f32, f32::max)
        * weights.tears();

    let expressive = inputs
        .faces
        .iter()
        .filter(|face| face.intensity() >= 0.35)
        .count() as f32;
    let group = (expressive / GROUP_FULL).clamp(0.0, 1.0);

    [
        expression.clamp(0.0, 1.0),
        genuine.clamp(0.0, 1.0),
        interaction.clamp(0.0, 1.0),
        peak_term.clamp(0.0, 1.0),
        reaction.clamp(0.0, 1.0),
        mutual.clamp(0.0, 1.0),
        composure.clamp(0.0, 1.0),
        tears.clamp(0.0, 1.0),
        group,
    ]
}

/// Feature 0: the scene-weighted channel mix over the frame's faces.
///
/// The seven positive channels weighted by the scene's table, minus the discomfort
/// channel weighted by its own. Per face, then combined by prominence.
fn subject_expression(inputs: &Inputs<'_>, weights: &SceneWeights) -> f32 {
    prominence_weighted(inputs, |face| {
        let channels = face.channels();
        let mut positive = 0.0f32;
        let mut total_weight = 0.0f32;
        // Slots 0 to 6 are the positive channels; slot 7 is discomfort and is subtracted.
        for (slot, value) in channels
            .iter()
            .enumerate()
            .take(FaceExpression::CHANNELS - 1)
        {
            // Genuineness has its own feature and would otherwise be counted twice - once
            // here as a channel and once as feature 1 - so it is excluded from the mix.
            if slot == 1 {
                continue;
            }
            let weight = weights.channel(slot);
            positive += value * weight;
            total_weight += weight;
        }
        let mixed = if total_weight <= f32::EPSILON {
            0.0
        } else {
            positive / total_weight
        };
        let penalty = channels
            .get(FaceExpression::CHANNELS - 1)
            .copied()
            .unwrap_or(0.0)
            * weights.discomfort()
            * 0.5;
        (mixed - penalty).clamp(0.0, 1.0)
    })
}

/// Combine a per-face quantity into one frame-level number, weighted by prominence.
///
/// **A prominence-weighted mean, not a maximum.** A maximum would let one guest in row
/// four decide a family portrait, which is the same mistake phase 09 avoided by
/// weighting sharpness across regions rather than taking the sharpest. A mean over
/// prominence is what "how much is this frame about somebody expressing something"
/// actually asks.
fn prominence_weighted(inputs: &Inputs<'_>, of: impl Fn(&FaceExpression) -> f32) -> f32 {
    let mut total = 0.0f32;
    let mut weight_sum = 0.0f32;
    for face in inputs.faces {
        let prominence = face
            .identity
            .and_then(|identity| inputs.prominence.get(&identity).copied())
            .unwrap_or(UNASSIGNED_PROMINENCE)
            .max(0.01);
        // The head's own confidence is part of the weight. A face read with no
        // confidence contributes almost nothing rather than contributing its zeroes as
        // if they were a finding.
        let weight = prominence * face.confidence.max(0.05);
        total += of(face) * weight;
        weight_sum += weight;
    }
    if weight_sum <= f32::EPSILON {
        return 0.0;
    }
    (total / weight_sum).clamp(0.0, 1.0)
}

/// The calibrated emotion score.
#[must_use]
pub fn score(
    features: &[f32; Ranker::FEATURES],
    ranker: &Ranker,
    calibration: &Calibration,
) -> f32 {
    calibration.apply(ranker.score(features))
}

/// How sure the whole reading is.
///
/// Four things lower it, and each one is a piece of evidence that was not there:
///
/// * **no scene label** costs 0.10, exactly as phase 09's verdict does, because the
///   weights fell back to `[default]` and invariant 7 is degraded rather than satisfied;
/// * **no face pass** costs 0.25, the largest of the four, because seven of the nine
///   features come from faces;
/// * **low head confidence** scales the whole thing, because a frame read at 0.2 has been
///   read badly;
/// * **an unresolved peak** costs 0.05, because feature 3 is then a number about a
///   moment with no apex.
///
/// Subtractive rather than multiplicative for the first, second and fourth, so a frame
/// missing all three lands at a floor rather than at zero: a frame with no scene, no
/// faces and no moment still has an interaction reading, and reporting zero confidence in
/// it would be a different kind of overclaim.
#[must_use]
pub fn confidence(inputs: &Inputs<'_>, peak_resolved: bool) -> f32 {
    let head = if inputs.faces.is_empty() {
        // No faces: the interaction head is the only reader, and it has its own
        // confidence which the caller has already folded into `reaction_bonus` and the
        // interaction strength. 0.55 is the ceiling a frame read on interactions alone
        // can reach.
        0.55
    } else {
        inputs
            .faces
            .iter()
            .map(|face| face.confidence)
            .fold(0.0f32, f32::max)
            .max(0.05)
    };

    let mut confidence = head;
    if !inputs.scene_known {
        confidence -= 0.10;
    }
    if !inputs.faces_scanned {
        confidence -= 0.25;
    }
    if !peak_resolved {
        confidence -= 0.05;
    }
    confidence.clamp(0.05, 1.0)
}

/// The reasons behind one frame's score, strongest credit first.
///
/// At most `ImageEmotion::MAX_REASONS`. The order is the explanation: a photographer
/// reading the card sees the biggest contributor first, and the caveats last.
#[must_use]
pub fn reasons(
    inputs: &Inputs<'_>,
    features: &[f32; Ranker::FEATURES],
    ranker: &Ranker,
    peak_resolved: bool,
) -> Vec<EmotionReason> {
    let contributions = ranker.coefficients();
    let mut out: Vec<EmotionReason> = Vec::new();

    // The strongest face, with the code its strongest channel deserves.
    if let Some(face) = inputs
        .faces
        .iter()
        .max_by(|left, right| left.intensity().total_cmp(&right.intensity()))
    {
        if face.intensity() >= 0.20 {
            let code = peak::lead_code(face);
            let credit = contributions.first().copied().unwrap_or(0.0)
                * features.first().copied().unwrap_or(0.0);
            out.push(EmotionReason::at(
                code,
                code.user_text(),
                credit / 10.0,
                face.crop,
            ));
        }
    }

    if let Some((kind, strength)) = inputs.interactions.first() {
        out.push(EmotionReason::frame(
            EmotionCode::InteractionDetected,
            format!("{} here, at {:.0}%", kind.title(), strength * 100.0),
            contributions.get(2).copied().unwrap_or(0.0) * features.get(2).copied().unwrap_or(0.0)
                / 10.0,
        ));
    }

    if inputs.mutual_gaze {
        out.push(EmotionReason::frame(
            EmotionCode::MutualGaze,
            EmotionCode::MutualGaze.user_text(),
            contributions.get(5).copied().unwrap_or(0.0) * features.get(5).copied().unwrap_or(0.0)
                / 10.0,
        ));
    }

    if peak_resolved {
        let proximity = inputs.peak_proximity;
        let code = if proximity >= peak::NEAR_PEAK {
            EmotionCode::NearPeak
        } else if proximity <= peak::OFF_PEAK {
            EmotionCode::OffPeak
        } else {
            EmotionCode::Unremarkable
        };
        if code != EmotionCode::Unremarkable {
            out.push(EmotionReason::frame(
                code,
                code.user_text(),
                contributions.get(3).copied().unwrap_or(0.0) * (proximity - 0.5) / 10.0,
            ));
        }
    }

    if inputs.reaction_bonus > 0.0 {
        out.push(EmotionReason::frame(
            EmotionCode::ReactionTo,
            EmotionCode::ReactionTo.user_text(),
            contributions.get(4).copied().unwrap_or(0.0) * inputs.reaction_bonus / 10.0,
        ));
    }

    // Tears last among the credits, and only above the certainty gate. Section 12's
    // fourth failure mode: no tear-based reason text below 0.85.
    if features.get(7).copied().unwrap_or(0.0) >= TEARS_CERTAIN * 0.5 {
        if let Some(face) = inputs.faces.iter().find(|face| face.reads_as_crying()) {
            out.push(EmotionReason::at(
                EmotionCode::Tears,
                EmotionCode::Tears.user_text(),
                contributions.get(7).copied().unwrap_or(0.0) * face.tears / 10.0,
                face.crop,
            ));
        }
    }

    out.sort_by(|left, right| right.weight.total_cmp(&left.weight));

    // The caveats go last, whatever their weight, because they are notes about the
    // reading rather than about the photograph.
    if !inputs.faces_scanned || inputs.faces.is_empty() {
        out.push(EmotionReason::frame(
            EmotionCode::NoFaces,
            EmotionCode::NoFaces.user_text(),
            0.0,
        ));
    }
    if !inputs.scene_known {
        out.push(EmotionReason::frame(
            EmotionCode::NoScene,
            EmotionCode::NoScene.user_text(),
            0.0,
        ));
    }

    if out.is_empty() {
        out.push(EmotionReason::frame(
            EmotionCode::Unremarkable,
            EmotionCode::Unremarkable.user_text(),
            0.0,
        ));
    }
    out.truncate(ImageEmotion::MAX_REASONS);
    out
}
