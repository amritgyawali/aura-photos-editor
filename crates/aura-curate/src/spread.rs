//! Two facing pages: what makes a pair work, and what makes one refused.
//!
//! Section 6.3's pairing objective: "similar tonal weight, compatible colour temperature after
//! consistency, complementary gaze/movement direction (subjects looking inward, not off the spread),
//! and no two near-identical frames facing each other". Section 10.1 turns the last of those into a
//! property test.
//!
//! # Two mechanisms, deliberately
//!
//! **No facing near-duplicates is a hard constraint.** Two frames from the same moment, or within
//! `MAX_PAIR_SIMILARITY` of each other, are never placed opposite one another. A photographer
//! looking at a spread of the same photograph twice does not think the pairing objective weighted
//! something poorly - they think nobody looked at it, and there is no weight at which that is
//! acceptable. So it is not a weight.
//!
//! **Tonal clash is a term, with a ceiling.** A spread whose two frames differ in tonal weight is
//! worse than one whose frames match, and considerably better than a spread left half-empty to avoid
//! it. It is scored below the ceiling and refused above it, and the number is on the wire so a
//! photographer can see which pairs the optimiser was unhappy with.
//!
//! ADR-0059 section 9.
//!
//! # Why an unmeasured direction scores zero rather than a half
//!
//! Because a spread whose subjects' facing could not be measured is not a spread whose subjects face
//! inward, and it is not a spread whose subjects face outward either. Scoring it at a half would
//! make an unmeasurable spread indistinguishable from a measured mediocre one; scoring it at zero
//! with [`SpreadPair::facing_known`] false beside it makes the two different values, and the panel
//! renders the second in grey. Phase 27's rule.
//!
//! On this build phase 06's detector finds no faces, so `facing_known` is false almost everywhere
//! and the direction term is renormalised out of nearly every spread. The pairing score that
//! survives is a real measurement of tone, warmth and variety; it is not a measurement of the thing
//! album designers actually spend their time on. Condition C3 of the exit report.

use aura_core::contract::curate::{CurateCode, CurateReason, SpreadPair};

use crate::policy::Policy;
use crate::read::{Facing, Frame};

/// Measure a candidate pair.
///
/// `similarity` is the phase 05 cosine similarity between the two frames, or `None` when either
/// vector is missing - which is a **skipped** variety term rather than a similarity of zero, and
/// which cannot make the pair permitted: a pair whose similarity could not be measured is treated as
/// permitted only when the two frames are not in the same moment, because a moment is a certainty
/// where a vector is a measurement.
#[must_use]
pub fn measure(
    left: &Frame,
    right: &Frame,
    similarity: Option<f32>,
    policy: &Policy,
) -> SpreadPair {
    let tonal_gap = match (left.tonal_weight(), right.tonal_weight()) {
        (Some(a), Some(b)) => (a - b).abs(),
        _ => 0.0,
    };
    let tonal_known = left.tonal_weight().is_some() && right.tonal_weight().is_some();

    let warmth_gap_k = match (left.warmth_k, right.warmth_k) {
        (Some(a), Some(b)) => (a - b).abs(),
        _ => 0.0,
    };
    let warmth_known = left.warmth_k.is_some() && right.warmth_k.is_some();

    let (facing_score, facing_known) = facing(left.facing(), right.facing());
    let measured_similarity = similarity.unwrap_or(0.0).clamp(-1.0, 1.0);

    let terms: [(f32, f32, bool); 4] = [
        (
            policy.pairing.tonal,
            (1.0 - tonal_gap / policy.max_tonal_gap()).clamp(0.0, 1.0),
            tonal_known,
        ),
        (
            policy.pairing.warmth,
            (1.0 - warmth_gap_k / aura_core::contract::curate::MAX_PAIR_WARMTH_GAP_K)
                .clamp(0.0, 1.0),
            warmth_known,
        ),
        (policy.pairing.direction, facing_score, facing_known),
        (
            policy.pairing.variety,
            // Interestingly different: a pair at zero similarity is two unrelated photographs and a
            // pair at the ceiling is the same photograph twice. The best variety sits in between, so
            // this peaks at half and falls away either side.
            (1.0 - (measured_similarity - 0.45).abs() / 0.45).clamp(0.0, 1.0),
            similarity.is_some(),
        ),
    ];
    let score = crate::explain::blend(&terms).unwrap_or(0.0);

    SpreadPair {
        tonal_gap,
        warmth_gap_k,
        facing_score,
        facing_known,
        similarity: measured_similarity,
        score,
    }
}

/// How well two frames' subjects face inward, and whether that could be measured at all.
///
/// The left page wants a subject looking right and the right page a subject looking left; a reader's
/// eye then travels into the gutter rather than off the edge of the book. A frontal subject is
/// neutral - it does not carry the eye anywhere, and it does not carry it away.
#[must_use]
pub fn facing(left: Facing, right: Facing) -> (f32, bool) {
    if !left.is_known() || !right.is_known() {
        return (0.0, false);
    }
    let inward = |side: Facing, want: Facing| -> f32 {
        if side == want {
            1.0
        } else if side == Facing::Frontal {
            0.5
        } else {
            0.0
        }
    };
    let score = 0.5 * inward(left, Facing::Right) + 0.5 * inward(right, Facing::Left);
    (score, true)
}

/// True when these two frames may face each other at all.
///
/// Two hard constraints, and the moment one is not redundant with the similarity one: two frames of
/// the same shot from opposite sides of the room can sit far apart in the index and are still the
/// same shot, and phase 08 already decided which frames those are. A build that checked only
/// similarity would be re-deriving phase 08's grouping badly.
#[must_use]
pub fn permitted(left: &Frame, right: &Frame, similarity: Option<f32>, policy: &Policy) -> bool {
    if let (Some(a), Some(b)) = (left.moment, right.moment) {
        if a == b {
            return false;
        }
    }
    if similarity.is_some_and(|s| s > policy.max_similarity()) {
        return false;
    }
    match (left.tonal_weight(), right.tonal_weight()) {
        (Some(a), Some(b)) => (a - b).abs() <= policy.max_tonal_gap(),
        // Unmeasured tone is not a clash. It is an unmeasured tone, and the score says so by
        // renormalising the term out.
        _ => true,
    }
}

/// Why these two are together, or why this one is alone.
#[must_use]
pub fn reasons(pair: &SpreadPair, single: bool, policy: &Policy) -> Vec<CurateReason> {
    let mut out = Vec::new();
    if single {
        out.push(CurateReason::plain(CurateCode::SingleSpread, 0.30));
        return out;
    }
    if pair.score >= 0.65 {
        out.push(CurateReason::detailed(
            CurateCode::SpreadPaired,
            format!(
                "these two work together across the fold ({:.2})",
                pair.score
            ),
            pair.score,
        ));
    }
    if pair.tonal_gap > policy.max_tonal_gap() * 0.6 {
        out.push(CurateReason::detailed(
            CurateCode::SpreadTonalGap,
            format!(
                "one of these is {:.0}% darker than the other",
                pair.tonal_gap * 100.0
            ),
            -pair.tonal_gap,
        ));
    }
    if !pair.facing_known {
        out.push(CurateReason::plain(CurateCode::SpreadFacingUnknown, -0.10));
    }
    crate::explain::ensure_reason(&mut out, CurateCode::SpreadPaired, pair.score.max(0.05));
    crate::explain::rank_reasons(&mut out, aura_core::contract::curate::MAX_REASONS);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::curate::{ImageId, MAX_PAIR_SIMILARITY, MAX_PAIR_TONAL_GAP};
    use aura_core::contract::ids::MomentId;
    use aura_index::contract::index::LumaStats;

    use crate::read::{Descriptor, FaceRead};

    fn frame(luma: f32, warmth: f32) -> Frame {
        let mut f = Frame::bare(ImageId::new(), 0);
        f.descriptor = Some(Descriptor {
            hsv_hist: vec![0u8; 512],
            luma: LumaStats {
                mean: luma,
                p1: 0.0,
                p50: luma,
                p99: 1.0,
                clip_lo: 0.0,
                clip_hi: 0.0,
            },
            edge_energy: 0.2,
        });
        f.warmth_k = Some(warmth);
        f
    }

    fn looking(f: &mut Frame, eye_offset: f32) {
        f.faces = vec![FaceRead {
            identity: None,
            area_frac: 0.08,
            centre_x: 0.5,
            width: 0.2,
            eye_mid_x: Some(0.5 + eye_offset),
        }];
    }

    #[test]
    fn two_frames_from_one_moment_never_face_each_other() {
        let policy = Policy::default();
        let moment = MomentId::new();
        let mut a = frame(0.5, 5000.0);
        let mut b = frame(0.5, 5000.0);
        a.moment = Some(moment);
        b.moment = Some(moment);
        assert!(!permitted(&a, &b, Some(0.10), &policy));
        // Phase 08 already decided this; a build that checked only the vector would re-derive its
        // grouping badly, and two frames of one shot from opposite sides of the room sit far apart
        // in the index.
        assert!(permitted(&a, &frame(0.5, 5000.0), Some(0.10), &policy));
    }

    #[test]
    fn two_near_identical_frames_never_face_each_other() {
        let policy = Policy::default();
        let a = frame(0.5, 5000.0);
        let b = frame(0.5, 5000.0);
        assert!(!permitted(
            &a,
            &b,
            Some(MAX_PAIR_SIMILARITY + 0.01),
            &policy
        ));
        assert!(permitted(&a, &b, Some(MAX_PAIR_SIMILARITY - 0.01), &policy));
    }

    #[test]
    fn a_tonal_clash_beyond_the_ceiling_is_refused_and_below_it_is_only_scored() {
        let policy = Policy::default();
        let dark = frame(0.15, 5000.0);
        let bright = frame(0.15 + MAX_PAIR_TONAL_GAP + 0.02, 5000.0);
        assert!(!permitted(&dark, &bright, Some(0.3), &policy));

        let mild = frame(0.15 + MAX_PAIR_TONAL_GAP - 0.05, 5000.0);
        assert!(permitted(&dark, &mild, Some(0.3), &policy));
        let pair = measure(&dark, &mild, Some(0.3), &policy);
        assert!(pair.is_permitted());
        assert!(
            pair.score < 1.0,
            "a mild clash is a worse pair, not a refused one"
        );
    }

    #[test]
    fn a_warmth_gap_is_a_term_and_never_a_refusal() {
        let policy = Policy::default();
        let a = frame(0.5, 3000.0);
        let b = frame(0.5, 6500.0);
        assert!(permitted(&a, &b, Some(0.3), &policy));
        let pair = measure(&a, &b, Some(0.3), &policy);
        assert!(pair.is_permitted());
        assert!(pair.warmth_gap_k > 3000.0);
    }

    #[test]
    fn subjects_facing_inward_score_higher_than_subjects_facing_out() {
        let policy = Policy::default();
        let mut left_in = frame(0.5, 5000.0);
        let mut right_in = frame(0.5, 5000.0);
        looking(&mut left_in, 0.05); // eyes right of centre: facing the gutter
        looking(&mut right_in, -0.05);
        let inward = measure(&left_in, &right_in, Some(0.4), &policy);
        assert!(inward.facing_known);
        assert!((inward.facing_score - 1.0).abs() < 1e-6);

        let mut left_out = frame(0.5, 5000.0);
        let mut right_out = frame(0.5, 5000.0);
        looking(&mut left_out, -0.05);
        looking(&mut right_out, 0.05);
        let outward = measure(&left_out, &right_out, Some(0.4), &policy);
        assert_eq!(outward.facing_score, 0.0);
        assert!(outward.facing_known, "measured, and measured as bad");
        assert!(inward.score > outward.score);
    }

    #[test]
    fn an_unmeasured_facing_is_a_different_value_from_a_bad_one() {
        let policy = Policy::default();
        let a = frame(0.5, 5000.0);
        let b = frame(0.5, 5000.0);
        let unknown = measure(&a, &b, Some(0.4), &policy);
        assert!(!unknown.facing_known);
        assert_eq!(unknown.facing_score, 0.0);

        let mut left_out = frame(0.5, 5000.0);
        let mut right_out = frame(0.5, 5000.0);
        looking(&mut left_out, -0.05);
        looking(&mut right_out, 0.05);
        let bad = measure(&left_out, &right_out, Some(0.4), &policy);
        assert_eq!(bad.facing_score, unknown.facing_score);
        assert_ne!(bad.facing_known, unknown.facing_known);
        // The unmeasured one renormalises the term out, so it does *not* score worse for something
        // nobody checked.
        assert!(
            unknown.score > bad.score,
            "unknown {} should not be punished like measured-bad {}",
            unknown.score,
            bad.score
        );
    }

    #[test]
    fn variety_peaks_between_unrelated_and_identical() {
        let policy = Policy::default();
        let a = frame(0.5, 5000.0);
        let b = frame(0.5, 5000.0);
        let unrelated = measure(&a, &b, Some(0.0), &policy);
        let related = measure(&a, &b, Some(0.45), &policy);
        let twins = measure(&a, &b, Some(0.90), &policy);
        assert!(related.score > unrelated.score);
        assert!(related.score > twins.score);
    }

    #[test]
    fn a_single_spread_says_so_and_carries_a_reason() {
        let policy = Policy::default();
        let reasons = reasons(&SpreadPair::none(), true, &policy);
        assert_eq!(reasons.len(), 1);
        assert_eq!(reasons[0].code, CurateCode::SingleSpread);
    }

    #[test]
    fn a_pair_that_could_not_be_measured_at_all_is_still_permitted_and_scores_nothing() {
        let policy = Policy::default();
        let bare_a = Frame::bare(ImageId::new(), 0);
        let bare_b = Frame::bare(ImageId::new(), 1);
        assert!(permitted(&bare_a, &bare_b, None, &policy));
        let pair = measure(&bare_a, &bare_b, None, &policy);
        assert_eq!(pair.score, 0.0, "nothing measured is nothing claimed");
        assert!(!pair.facing_known);
    }
}
