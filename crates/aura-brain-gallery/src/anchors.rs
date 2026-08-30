//! Which frames a node should be judged against.
//!
//! Section 6.1, and it is the shortest and most consequential argument in the phase document:
//!
//! > Averaging a scene's frames produces mediocrity; anchoring to the *best-judged* frames
//! > preserves quality.
//!
//! An average over a ceremony is the ceremony's mistakes included at their true weight. If a
//! quarter of the frames are half a stop dark because the photographer was shooting into a window,
//! the average is an eighth of a stop dark, and normalising toward it makes the other three
//! quarters worse. Anchors are the frames the product is most confident about, and a node is
//! normalised toward *those*.
//!
//! ## The ranking, and why every term is a product
//!
//! Four terms, from section 2.1: "WB confidence, subject exposure quality, primary-identity
//! presence and absence of mixed light". They multiply rather than sum, which is the rule this
//! product has applied since phase 12's keep score: **no signal may rescue another**. A frame with
//! a perfect white-balance confidence and no identifiable subject is not a good anchor for a
//! ceremony, and a sum would let the first term carry it.
//!
//! The identity term is the one that needs stating carefully. Section 2.1 asks for "primary-identity
//! presence", which is the couple - and a frame of the couple is the frame whose skin, whose dress
//! and whose light everything else in the chapter will be compared against. But a node of a
//! detail chapter has no people in it at all, and a term that went to zero there would leave every
//! detail node unanchored. So the term is a *floor plus a bonus* rather than a gate: presence
//! raises an anchor's rank and absence does not disqualify it.
//!
//! ## A pin is authoritative and a rejection is as durable as a pin
//!
//! Section 6.1: "users can pin or reject anchors in the UI; pinned anchors are authoritative,
//! which gives professionals direct control over the look of a scene." Pinned frames are placed
//! first, before the ranking is consulted, and rejected frames are removed before it. Neither is
//! re-decided by a re-analysis - the store's `DELETE` excludes them and the re-insert skips them,
//! which is two statements because phase 18 learned that one is not enough.

use std::collections::BTreeSet;

use aura_core::contract::gallery::ImageId;
use aura_core::contract::gallery::{
    GalleryCode, NodeTarget, MAX_ANCHORS, MIN_ANCHORS, SKIN_DE00_SPREAD_CEILING,
};

use crate::policy::ScenePolicy;
use crate::stats::{self, GradeSignature};
use crate::tree::{Frame, RawNode};

/// The white-balance confidence a frame needs before it may anchor anything.
///
/// Phase 15's own `REVIEW_WB_BELOW` is 0.55 - the point at which a frame goes into its review
/// queue - and an anchor has to be better than "not worth a photographer's attention". 0.65 is
/// where a frame is a positive answer rather than the absence of a problem.
pub const MIN_ANCHOR_WB_CONF: f32 = 0.65;

/// The exposure confidence a frame needs.
///
/// Lower than the white-balance floor, because the exposure half of an anchor is used for one
/// number - the node's subject-luminance band - while the white balance decides two of the five
/// axes and the whole of the skin half.
pub const MIN_ANCHOR_EXPOSURE_CONF: f32 = 0.55;

/// What a frame with no recognisable person scores on the identity term.
///
/// See the module header. A floor rather than a zero, so a details node still has anchors.
pub const IDENTITY_FLOOR: f32 = 0.55;

/// What mixed light costs an anchor.
///
/// Not a veto. A reception is mixed light nearly everywhere, and a rule that refused every
/// mixed-light frame would leave the hardest chapter in the wedding unanchored - which is exactly
/// the chapter that needs a target most.
pub const MIXED_LIGHT_PENALTY: f32 = 0.55;

/// One candidate, with the four terms that ranked it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Candidate {
    /// The photograph.
    pub image: ImageId,
    /// The product of the four terms, `0..1`.
    pub quality: f32,
    /// True when a photographer put it here.
    pub pinned: bool,
}

/// What anchor selection produced.
#[derive(Debug, Clone, PartialEq)]
pub struct Anchored {
    /// The chosen frames, best first, between [`MIN_ANCHORS`] and [`MAX_ANCHORS`] - or empty.
    pub anchors: Vec<Candidate>,
    /// What the anchors say the node should look like, when there are enough of them.
    pub target: Option<NodeTarget>,
    /// Why. Includes `GalleryCode::NodeUnanchored` when nothing could be chosen.
    pub reasons: Vec<GalleryCode>,
}

impl Anchored {
    /// True when this node can normalise anything.
    #[must_use]
    pub fn is_anchored(&self) -> bool {
        self.target.as_ref().is_some_and(NodeTarget::is_usable)
    }
}

/// How good an anchor one frame would be, `0..1`.
///
/// Zero when the frame fails a floor, which is a refusal rather than a low score: an anchor is a
/// frame every other frame in the chapter is moved toward, and a mediocre one does not produce a
/// mediocre gallery - it produces a confidently wrong one. Phase 15 made the same argument about
/// its skin locus.
#[must_use]
pub fn quality_of(frame: &Frame) -> f32 {
    if !frame.has_tone() {
        return 0.0;
    }
    if frame.wb_conf < MIN_ANCHOR_WB_CONF || frame.exposure_conf < MIN_ANCHOR_EXPOSURE_CONF {
        return 0.0;
    }
    // An intentional light is what a node's *frames* may be, and never what its target is made of.
    // Anchoring a ceremony to the one candle-lit frame in it would neutralise the ceremony toward
    // candlelight, which is section 12's first failure mode arriving through the back door.
    if frame.intentional_light {
        return 0.0;
    }

    let wb = frame.wb_conf.clamp(0.0, 1.0);
    let exposure = frame.exposure_conf.clamp(0.0, 1.0);
    let identity = frame
        .identities
        .values()
        .copied()
        .fold(0.0_f32, f32::max)
        .mul_add(1.0 - IDENTITY_FLOOR, IDENTITY_FLOOR)
        .clamp(0.0, 1.0);
    let light = if frame.mixed_light {
        MIXED_LIGHT_PENALTY
    } else {
        1.0
    };

    (wb * exposure * identity * light).clamp(0.0, 1.0)
}

/// Choose a node's anchors and compute its target.
///
/// `pinned` and `rejected` are the photographer's own decisions, applied before the ranking rather
/// than blended into it. `target_count` comes from `consistency.toml` and is clamped to the
/// contract's own range.
#[must_use]
pub fn select(
    node: &RawNode,
    policy: ScenePolicy,
    pinned: &BTreeSet<ImageId>,
    rejected: &BTreeSet<ImageId>,
    target_count: usize,
) -> Anchored {
    let want = target_count.clamp(MIN_ANCHORS, MAX_ANCHORS);
    let mut reasons = Vec::new();

    let mut ranked: Vec<Candidate> = node
        .frames
        .iter()
        .filter(|frame| !rejected.contains(&frame.image))
        .map(|frame| Candidate {
            image: frame.image,
            quality: quality_of(frame),
            pinned: pinned.contains(&frame.image),
        })
        // A pinned frame is kept whatever its measured quality, because a photographer looking at
        // a photograph knows something the four terms do not. Everything else must clear the
        // floors.
        .filter(|candidate| candidate.pinned || candidate.quality > 0.0)
        .collect();

    if !rejected.is_empty() && node.frames.iter().any(|f| rejected.contains(&f.image)) {
        reasons.push(GalleryCode::AnchorRejected);
    }

    // Pinned first, then by quality, then by id. The last term is what makes two runs of the same
    // node produce the same anchors when two frames score identically - which they do, often,
    // because the four terms are quantised confidences.
    ranked.sort_by(|a, b| {
        b.pinned
            .cmp(&a.pinned)
            .then_with(|| {
                b.quality
                    .partial_cmp(&a.quality)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.image.to_db().cmp(&b.image.to_db()))
    });
    ranked.truncate(want);

    if ranked.iter().any(|c| c.pinned) {
        reasons.push(GalleryCode::AnchorPinned);
    }

    if ranked.len() < MIN_ANCHORS {
        reasons.push(GalleryCode::NodeUnanchored);
        return Anchored {
            anchors: Vec::new(),
            target: None,
            reasons,
        };
    }

    let chosen: Vec<&Frame> = ranked
        .iter()
        .filter_map(|candidate| {
            node.frames
                .iter()
                .find(|frame| frame.image == candidate.image)
        })
        .collect();

    let Some(target) = target_of(&chosen, policy) else {
        reasons.push(GalleryCode::NodeUnanchored);
        return Anchored {
            anchors: Vec::new(),
            target: None,
            reasons,
        };
    };

    if !target.is_usable() {
        reasons.push(GalleryCode::AnchorsDisagree);
        return Anchored {
            anchors: ranked,
            target: Some(target),
            reasons,
        };
    }

    reasons.push(GalleryCode::NodeAnchored);
    if chosen.len() >= 4 {
        // Four or more anchors is where the trimmed mean actually trims, so the target is robust
        // in a way a three-anchor one is not, and the panel should be able to say so.
        reasons.push(GalleryCode::RobustTarget);
    }

    Anchored {
        anchors: ranked,
        target: Some(target),
        reasons,
    }
}

/// The robust target a set of anchors implies.
///
/// Trimmed means for the scalars, a component-wise median for anything chromatic, and the scene's
/// own tolerances beside each value. Returns `None` when the anchors carry no tone estimate at
/// all, which cannot happen through [`select`] - the floors reject those - and is the honest
/// answer for a caller that assembled a set by hand.
#[must_use]
pub fn target_of(anchors: &[&Frame], policy: ScenePolicy) -> Option<NodeTarget> {
    if anchors.is_empty() {
        return None;
    }
    let trim = usize::from(anchors.len() >= 4);

    let ccts: Vec<f32> = anchors.iter().filter_map(|f| f.cct_k).collect();
    let tints: Vec<f32> = anchors.iter().filter_map(|f| f.tint).collect();
    let lumas: Vec<f32> = anchors.iter().filter_map(|f| f.subject_luma).collect();

    let cct_k = stats::trimmed_mean(&ccts, trim)?;
    let tint = stats::trimmed_mean(&tints, trim)?;
    let subject_luma = stats::trimmed_mean(&lumas, trim)?;

    let contrasts: Vec<f32> = anchors.iter().filter_map(|f| f.contrast).collect();
    let saturations: Vec<f32> = anchors.iter().filter_map(|f| f.saturation).collect();
    let signatures: Vec<GradeSignature> = anchors
        .iter()
        .filter_map(|f| f.signature)
        .map(|values| GradeSignature { values })
        .collect();

    // A node whose anchors have no grade decision keeps a zero contrast and a zero saturation, and
    // `has_grade` on each frame is what stops the solver moving anything toward them.
    // `GalleryCode::ColourDecisionAbsent` is what the frame records.
    let contrast = stats::trimmed_mean(&contrasts, trim).unwrap_or(0.0);
    let saturation = stats::trimmed_mean(&saturations, trim).unwrap_or(0.0);
    let grade_signature = GradeSignature::central(&signatures)
        .unwrap_or_default()
        .values;

    // The agreement is measured on the axis the node's damping is *about*, scaled by what this
    // scene tolerates. Invariant 7: the same 300 K of anchor disagreement is cohesive on a dance
    // floor and is a selection failure in a family portrait session.
    let cohesion = stats::cohesion(&ccts, policy.cct_tol.max(1.0));

    Some(NodeTarget {
        cct_k,
        cct_tol: policy.cct_tol,
        tint,
        tint_tol: policy.tint_tol,
        subject_luma,
        luma_tol: policy.luma_tol,
        contrast,
        saturation,
        grade_signature,
        anchor_count: anchors.len().min(u16::MAX as usize) as u16,
        cohesion,
    })
}

/// How much a node's target is worth believing, `0..1`.
///
/// Not the same as its cohesion. Cohesion says the anchors agree; this says the agreement is
/// backed by enough of them and by frames the product was confident about in the first place. It
/// is what a delta's own confidence is built on, and it is why a frame in a five-anchor node with
/// a tight target ends up more confident than the same frame in a three-anchor node with a loose
/// one.
#[must_use]
pub fn target_confidence(anchored: &Anchored) -> f32 {
    let Some(target) = anchored.target else {
        return 0.0;
    };
    let count = (f32::from(target.anchor_count) / MAX_ANCHORS as f32).clamp(0.0, 1.0);
    let mean_quality = if anchored.anchors.is_empty() {
        0.0
    } else {
        anchored
            .anchors
            .iter()
            .map(|candidate| candidate.quality)
            .sum::<f32>()
            / anchored.anchors.len() as f32
    };
    // A geometric mean of three, so no term rescues another. Phase 12's rule, and phase 18's for
    // its allowance.
    (count * target.cohesion * mean_quality)
        .max(0.0)
        .cbrt()
        .clamp(0.0, 1.0)
}

/// The dE00 spread ceiling a node's skin corrections are held to.
///
/// Restated from the contract so the anchor module, the skin module and the eval harness share one
/// symbol rather than three copies of 2.0.
pub const SKIN_SPREAD_CEILING: f32 = SKIN_DE00_SPREAD_CEILING;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use crate::policy::Consistency;
    use aura_core::contract::ids::NodeId;
    use aura_core::{IdentityId, SceneId, SegmentId};

    fn node(frames: Vec<Frame>) -> RawNode {
        RawNode {
            id: NodeId::new(),
            parent: None,
            segment: frames.first().map_or_else(SegmentId::new, |f| f.segment),
            ordinal: 0,
            siblings: 1,
            scene: SceneId::Ceremony,
            frames,
            reasons: Vec::new(),
        }
    }

    fn policy() -> ScenePolicy {
        Consistency::default().scene(SceneId::Ceremony)
    }

    #[test]
    fn a_confident_frame_outranks_a_doubtful_one() {
        let segment = SegmentId::new();
        let mut good = fixtures::frame_at(segment, 0, SceneId::Ceremony);
        good.wb_conf = 0.95;
        good.exposure_conf = 0.9;
        let mut poor = fixtures::frame_at(segment, 1_000, SceneId::Ceremony);
        poor.wb_conf = 0.70;
        poor.exposure_conf = 0.60;
        assert!(quality_of(&good) > quality_of(&poor));
    }

    #[test]
    fn no_term_rescues_another() {
        let segment = SegmentId::new();
        let mut frame = fixtures::frame_at(segment, 0, SceneId::Ceremony);
        frame.wb_conf = 1.0;
        frame.exposure_conf = 0.54;
        assert_eq!(
            quality_of(&frame),
            0.0,
            "a perfect white balance does not carry a frame under the exposure floor"
        );
    }

    #[test]
    fn a_frame_with_no_people_in_it_is_still_a_usable_anchor() {
        let segment = SegmentId::new();
        let mut frame = fixtures::frame_at(segment, 0, SceneId::Details);
        frame.identities.clear();
        assert!(
            quality_of(&frame) > 0.0,
            "a details node would otherwise be unanchorable"
        );
    }

    #[test]
    fn an_intentionally_lit_frame_never_anchors_anything() {
        let segment = SegmentId::new();
        let mut frame = fixtures::frame_at(segment, 0, SceneId::Vows);
        frame.wb_conf = 0.99;
        frame.exposure_conf = 0.99;
        frame.intentional_light = true;
        assert_eq!(quality_of(&frame), 0.0);
    }

    #[test]
    fn a_node_with_two_usable_frames_is_unanchored_rather_than_anchored_on_two() {
        let segment = SegmentId::new();
        let mut frames: Vec<Frame> = (0..6)
            .map(|i| fixtures::frame_at(segment, i * 1_000, SceneId::Ceremony))
            .collect();
        for frame in frames.iter_mut().skip(2) {
            frame.wb_conf = 0.2;
        }
        let anchored = select(
            &node(frames),
            policy(),
            &BTreeSet::new(),
            &BTreeSet::new(),
            4,
        );
        assert!(anchored.anchors.is_empty());
        assert!(anchored.target.is_none());
        assert!(anchored.reasons.contains(&GalleryCode::NodeUnanchored));
        assert!(!anchored.is_anchored());
    }

    #[test]
    fn a_pinned_frame_is_first_whatever_it_scores() {
        let segment = SegmentId::new();
        let mut frames: Vec<Frame> = (0..6)
            .map(|i| fixtures::frame_at(segment, i * 1_000, SceneId::Ceremony))
            .collect();
        frames[5].wb_conf = 0.10;
        frames[5].exposure_conf = 0.10;
        let pin: BTreeSet<ImageId> = [frames[5].image].into_iter().collect();
        let anchored = select(&node(frames.clone()), policy(), &pin, &BTreeSet::new(), 4);
        assert_eq!(anchored.anchors[0].image, frames[5].image);
        assert!(anchored.anchors[0].pinned);
        assert!(anchored.reasons.contains(&GalleryCode::AnchorPinned));
    }

    #[test]
    fn a_rejected_frame_never_comes_back() {
        let segment = SegmentId::new();
        let frames: Vec<Frame> = (0..6)
            .map(|i| fixtures::frame_at(segment, i * 1_000, SceneId::Ceremony))
            .collect();
        let reject: BTreeSet<ImageId> = [frames[0].image].into_iter().collect();
        let anchored = select(
            &node(frames.clone()),
            policy(),
            &BTreeSet::new(),
            &reject,
            4,
        );
        assert!(anchored.anchors.iter().all(|c| c.image != frames[0].image));
        assert!(anchored.reasons.contains(&GalleryCode::AnchorRejected));
    }

    #[test]
    fn one_wrong_anchor_does_not_move_the_target() {
        let segment = SegmentId::new();
        let mut frames: Vec<Frame> = (0..5)
            .map(|i| {
                let mut frame = fixtures::frame_at(segment, i * 1_000, SceneId::Ceremony);
                frame.cct_k = Some(5000.0 + i as f32 * 5.0);
                frame
            })
            .collect();
        frames[4].cct_k = Some(9000.0);
        let refs: Vec<&Frame> = frames.iter().collect();
        let target = target_of(&refs, policy()).unwrap();
        assert!(
            (target.cct_k - 5010.0).abs() < 60.0,
            "one bad anchor moved the target to {}",
            target.cct_k
        );
    }

    #[test]
    fn anchors_that_disagree_more_than_the_scene_tolerates_do_not_produce_a_usable_target() {
        let segment = SegmentId::new();
        let frames: Vec<Frame> = (0..4)
            .map(|i| {
                let mut frame = fixtures::frame_at(segment, i * 1_000, SceneId::Ceremony);
                frame.cct_k = Some(3000.0 + i as f32 * 1_600.0);
                frame
            })
            .collect();
        let anchored = select(
            &node(frames),
            policy(),
            &BTreeSet::new(),
            &BTreeSet::new(),
            4,
        );
        assert!(anchored.reasons.contains(&GalleryCode::AnchorsDisagree));
        assert!(!anchored.is_anchored());
    }

    #[test]
    fn selection_is_deterministic_when_two_frames_score_the_same() {
        let segment = SegmentId::new();
        let mut frames: Vec<Frame> = (0..8)
            .map(|i| fixtures::frame_at(segment, i * 1_000, SceneId::Ceremony))
            .collect();
        for frame in &mut frames {
            frame.wb_conf = 0.8;
            frame.exposure_conf = 0.8;
            frame.identities = [(IdentityId::new(), 0.5)].into_iter().collect();
        }
        let one = select(
            &node(frames.clone()),
            policy(),
            &BTreeSet::new(),
            &BTreeSet::new(),
            4,
        );
        frames.reverse();
        let two = select(
            &node(frames),
            policy(),
            &BTreeSet::new(),
            &BTreeSet::new(),
            4,
        );
        let a: Vec<String> = one.anchors.iter().map(|c| c.image.to_db()).collect();
        let b: Vec<String> = two.anchors.iter().map(|c| c.image.to_db()).collect();
        assert_eq!(a, b);
    }

    #[test]
    fn a_five_anchor_tight_node_is_more_confident_than_a_three_anchor_loose_one() {
        let segment = SegmentId::new();
        let tight: Vec<Frame> = (0..5)
            .map(|i| {
                let mut f = fixtures::frame_at(segment, i * 1_000, SceneId::Ceremony);
                f.cct_k = Some(5000.0 + i as f32);
                f
            })
            .collect();
        let loose: Vec<Frame> = (0..3)
            .map(|i| {
                let mut f = fixtures::frame_at(segment, i * 1_000, SceneId::Ceremony);
                f.cct_k = Some(5000.0 + i as f32 * 90.0);
                f.wb_conf = 0.68;
                f
            })
            .collect();
        let a = select(
            &node(tight),
            policy(),
            &BTreeSet::new(),
            &BTreeSet::new(),
            5,
        );
        let b = select(
            &node(loose),
            policy(),
            &BTreeSet::new(),
            &BTreeSet::new(),
            5,
        );
        assert!(target_confidence(&a) > target_confidence(&b));
    }
}
