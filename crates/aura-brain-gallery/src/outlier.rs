//! Which frames would not come, and by how much.
//!
//! Section 6.4 makes outliers a first-class output, and section 2.1 says what they are for: "this
//! is exactly the Phase 27 QC input."
//!
//! ## The residual, never the raw deviation
//!
//! A frame 900 K from its node that the bound could only move 450 K is an outlier with a 450 K
//! residual. A frame 300 K away that was corrected in full is **not an outlier at all**, even
//! though its raw deviation was larger than a frame that started at 100 K and could not be moved.
//!
//! Getting this backwards produces a QC queue full of frames the product already fixed, which is
//! the worst possible failure for a queue: a photographer works through fifty tickets, finds
//! nothing wrong with any of them, and stops opening the panel. Section 6.4's own sentence -
//! "+310 K warmer than node anchors, magenta skin cast 4.2 dE00" - is only true of the residual,
//! and `Outlier::describe` assembles exactly that.
//!
//! ## An outlier is a row, not a counter
//!
//! Phase 24 made the same choice about its refusals and paid forty per cent of its storage for it.
//! Here it is much cheaper and the argument is the same: phase 27's queue is *fed* from these rows,
//! and "which frames drifted" is unanswerable from a count.
//!
//! ## Three ways to be an outlier, and they are three codes
//!
//! `OutlierAfterNormalisation` is a frame the bounds could not reach.
//! `SkinOutlier` is a person whose skin is still outside the gallery promise on this frame.
//! `AnchorsDisagree` is a node whose target was never trustworthy - which is not a claim about the
//! frame at all, and is the one a photographer fixes by pinning an anchor rather than by re-editing
//! a photograph.

use aura_core::contract::gallery::ImageId;
use aura_core::contract::gallery::{GalleryCode, GalleryReason, Outlier, SkinCorrection};
use aura_core::contract::ids::NodeId;
use aura_core::IdentityId;

use crate::normalise::{Residual, Solved};
use crate::policy::Consistency;
use crate::skin_consistency;

/// Decide whether one solved frame is an outlier, and describe it if it is.
///
/// `anchors_disagree` comes from the node rather than from the frame: when a node's target was
/// never usable, every frame in it is reported, because the honest statement is about the node and
/// a photographer needs to see all of it to know that.
#[must_use]
pub fn detect(
    solved: &Solved,
    node: NodeId,
    policy: &Consistency,
    anchors_disagree: bool,
) -> Option<Outlier> {
    // A frame nobody normalised cannot have drifted from a target it was never compared against.
    // Reporting a disabled frame or one the photographer set by hand would put their own decision
    // in a QC queue, which is the one place it must never appear.
    if solved.delta.reasons.iter().any(|reason| {
        matches!(
            reason.code,
            GalleryCode::Disabled
                | GalleryCode::UserEdited
                | GalleryCode::MoodPreserved
                | GalleryCode::ToneEstimateAbsent
        )
    }) {
        return None;
    }

    let deviation = solved.residual.deviation(policy);
    let skin = solved.delta.skin_correction;
    let skin_out = skin.as_ref().is_some_and(skin_consistency::is_skin_outlier);
    let over = deviation >= policy.outlier_residual;

    if !over && !skin_out && !anchors_disagree {
        return None;
    }

    let mut reasons = Vec::new();
    if over {
        reasons.push(GalleryCode::OutlierAfterNormalisation);
    }
    if skin_out {
        reasons.push(GalleryCode::SkinOutlier);
    }
    if anchors_disagree {
        reasons.push(GalleryCode::AnchorsDisagree);
    }

    Some(Outlier {
        image_id: solved.delta.image_id,
        node_id: node,
        residual_cct: solved.residual.cct,
        residual_tint: solved.residual.tint,
        residual_exposure: solved.residual.exposure,
        residual_skin_de00: skin.map_or(0.0, |c| c.de00_after),
        worst_identity: worst_identity(skin.as_ref()),
        deviation: deviation.max(skin_deviation(skin.as_ref())),
        reasons: reasons.into_iter().map(GalleryReason::of).collect(),
        analysis_ver: crate::ANALYSIS_VER,
    })
}

/// Whose skin is furthest out, when a correction was planned at all.
fn worst_identity(correction: Option<&SkinCorrection>) -> Option<IdentityId> {
    correction
        .filter(|c| skin_consistency::is_skin_outlier(c))
        .map(|c| c.identity)
}

/// The skin half expressed on the same `0..1` scale as the tone half.
///
/// Scaled by the gallery promise rather than by a bound, because the skin half has no bound in the
/// tone sense: the promise is that the *spread* is at or below 2.0 dE00, so a frame at 4.0 is twice
/// as far out as one at 2.0 and both are out.
fn skin_deviation(correction: Option<&SkinCorrection>) -> f32 {
    correction.map_or(0.0, |c| {
        (c.de00_after / aura_core::contract::gallery::SKIN_DE00_SPREAD_CEILING).clamp(0.0, 1.0)
    })
}

/// The worst-first ordering phase 27's queue reads.
///
/// Ties break on the photograph's id so two runs of the same project produce the same queue in the
/// same order - a queue that reshuffled between runs is a queue a photographer cannot work through
/// half of and come back to. Invariant 4.
pub fn rank(outliers: &mut [Outlier]) {
    outliers.sort_by(|a, b| {
        b.deviation
            .partial_cmp(&a.deviation)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.image_id.to_db().cmp(&b.image_id.to_db()))
    });
}

/// The mean deviation across a set, for section 11's `gallery.outliers` telemetry.
#[must_use]
pub fn mean_deviation(outliers: &[Outlier]) -> f32 {
    if outliers.is_empty() {
        return 0.0;
    }
    outliers.iter().map(|o| o.deviation).sum::<f32>() / outliers.len() as f32
}

/// A residual that is entirely within tolerance, for a caller assembling a report.
#[must_use]
pub fn zero_residual() -> Residual {
    Residual::default()
}

/// Which photographs are outliers, as ids, in queue order.
#[must_use]
pub fn image_ids(outliers: &[Outlier]) -> Vec<ImageId> {
    outliers.iter().map(|o| o.image_id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalise;
    use crate::tree::Frame;
    use crate::{anchors, fixtures};
    use aura_core::{SceneId, SegmentId};

    fn solved_at(frame_cct: f32) -> (Solved, NodeId, Consistency) {
        let bounds = Consistency::default();
        let policy = bounds.scene(SceneId::Ceremony);
        let segment = SegmentId::new();
        let anchor_set: Vec<Frame> = (0..4)
            .map(|i| fixtures::frame_at(segment, i * 1_000, SceneId::Ceremony))
            .collect();
        let refs: Vec<&Frame> = anchor_set.iter().collect();
        let target = anchors::target_of(&refs, policy).unwrap();
        let mut frame = fixtures::frame_at(segment, 9_000, SceneId::Ceremony);
        frame.cct_k = Some(frame_cct);
        let node = NodeId::new();
        (
            normalise::solve(&frame, node, &target, policy, &bounds, 1.0),
            node,
            bounds,
        )
    }

    #[test]
    fn a_frame_that_was_corrected_in_full_is_not_an_outlier() {
        let (solved, node, bounds) = solved_at(5200.0);
        assert!(detect(&solved, node, &bounds, false).is_none());
    }

    #[test]
    fn a_frame_the_bound_could_not_reach_is_one_and_says_by_how_much() {
        let (solved, node, bounds) = solved_at(7500.0);
        let outlier = detect(&solved, node, &bounds, false).expect("2,500 K away is an outlier");
        assert!(outlier.residual_cct > 400.0, "{}", outlier.residual_cct);
        let text = outlier.describe();
        assert!(text.contains("warmer"), "{text}");
        assert!(outlier
            .reasons
            .iter()
            .any(|r| r.code == GalleryCode::OutlierAfterNormalisation));
    }

    #[test]
    fn a_larger_raw_deviation_that_was_fixed_beats_a_smaller_one_that_was_not() {
        // 400 K away, fully correctable at the 450 K bound: not an outlier.
        let (fixed, node_a, bounds) = solved_at(5400.0);
        // 2,000 K away, clamped: an outlier, despite the *correction* being the same size.
        let (stuck, node_b, _) = solved_at(7000.0);
        assert!(detect(&fixed, node_a, &bounds, false).is_none());
        assert!(detect(&stuck, node_b, &bounds, false).is_some());
    }

    #[test]
    fn a_frame_the_photographer_set_by_hand_never_reaches_the_queue() {
        let bounds = Consistency::default();
        let policy = bounds.scene(SceneId::Ceremony);
        let segment = SegmentId::new();
        let anchor_set: Vec<Frame> = (0..4)
            .map(|i| fixtures::frame_at(segment, i * 1_000, SceneId::Ceremony))
            .collect();
        let refs: Vec<&Frame> = anchor_set.iter().collect();
        let target = anchors::target_of(&refs, policy).unwrap();
        let node = NodeId::new();
        for blocking in [
            |f: &mut Frame| f.user_edited = true,
            |f: &mut Frame| f.enabled = false,
            |f: &mut Frame| f.intentional_light = true,
        ] {
            let mut frame = fixtures::frame_at(segment, 9_000, SceneId::Ceremony);
            frame.cct_k = Some(9000.0);
            blocking(&mut frame);
            let solved = normalise::solve(&frame, node, &target, policy, &bounds, 1.0);
            assert!(
                detect(&solved, node, &bounds, false).is_none(),
                "a photographer's own decision is not a QC ticket"
            );
        }
    }

    #[test]
    fn a_node_whose_anchors_disagree_reports_every_frame_in_it() {
        let (solved, node, bounds) = solved_at(5010.0);
        let outlier = detect(&solved, node, &bounds, true).expect("the node is the problem");
        assert!(outlier
            .reasons
            .iter()
            .any(|r| r.code == GalleryCode::AnchorsDisagree));
    }

    #[test]
    fn a_skin_cast_makes_a_frame_an_outlier_even_when_the_tone_is_fine() {
        let (mut solved, node, bounds) = solved_at(5010.0);
        solved.delta.skin_correction = Some(SkinCorrection {
            identity: IdentityId::new(),
            d_uv: [0.001, 0.001],
            d_luma: 0.0,
            de00_before: 6.0,
            de00_after: 4.2,
            cap: 0.018,
            capped: true,
        });
        let outlier = detect(&solved, node, &bounds, false).expect("4.2 dE00 is out");
        assert!(
            outlier.describe().contains("4.2 dE00"),
            "{}",
            outlier.describe()
        );
        assert!(outlier.worst_identity.is_some());
    }

    #[test]
    fn the_queue_is_worst_first_and_the_same_every_run() {
        let mut a = detect(
            &solved_at(9000.0).0,
            NodeId::new(),
            &Consistency::default(),
            false,
        )
        .unwrap();
        let mut b = a.clone();
        a.deviation = 0.4;
        b.deviation = 0.9;
        b.image_id = ImageId::new();
        let mut set = vec![a.clone(), b.clone()];
        rank(&mut set);
        assert_eq!(set[0].image_id, b.image_id);
        let mut reversed = vec![b, a];
        rank(&mut reversed);
        assert_eq!(image_ids(&set), image_ids(&reversed));
    }

    #[test]
    fn the_mean_deviation_of_nothing_is_zero_rather_than_a_division() {
        assert_eq!(mean_deviation(&[]), 0.0);
        assert_eq!(zero_residual(), Residual::default());
    }
}
