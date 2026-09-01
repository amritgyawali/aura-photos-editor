//! Assembling reasons, and the one place they are ordered.
//!
//! Invariant 2: every AI decision carries `confidence` and `reasons[]`, and a decision without an
//! explanation is a bug. This phase makes five kinds of decision and every one of them goes through
//! [`rank_reasons`] before it is stored, so the ordering a photographer reads is the same ordering in
//! every panel, in the export and in the archived report.
//!
//! The ordering itself lives on the contract - `CurateReason::rank` - because it is a property of
//! the shape rather than of this crate. What lives here is the truncation, and the rule that a pick
//! that ended up with nothing to say gets a reason anyway rather than an empty list.

use aura_core::contract::curate::{CurateCode, CurateReason};

/// Sort strongest-first, truncate to `limit`, and **keep one caveat**.
///
/// Vetoes lead, then magnitude, then the code's own order - which makes the result total and
/// deterministic. `partial_cmp` on a weight would be `None` on a NaN and leave the order dependent
/// on the sort's implementation, which invariant 4 forbids.
///
/// # Why a slot is reserved
///
/// The first version of this function was a sort and a truncate, and it had a defect its own unit
/// tests could not see. A caveat - "AURA could not tell how similar this is to the rest", "nobody in
/// this frame has a measured skin locus" - carries a small weight, because it *is* a small
/// contribution to the score. So on a pick with four strong arguments in its favour the caveat is
/// the fifth item and is truncated away, and the only picks that would ever show a caveat are the
/// picks with nothing else to say.
///
/// That is exactly backwards. The frames a photographer most needs the caveat on are the confident
/// ones, because those are the ones they will accept without looking. Phase 24 wrote the rule - an
/// absent input is ignorance, not permission - and phase 27 wrote its second half - clean and
/// skipped are different values. This is the third: **a skip that a stronger reason can hide is a
/// skip nobody sees.**
///
/// So the strongest caveat gets the last slot whenever there is one and the list would otherwise be
/// full of arguments. At most one, because two caveats on one pick is a pick nobody should be
/// looking at anyway.
pub fn rank_reasons(reasons: &mut Vec<CurateReason>, limit: usize) {
    reasons.sort_by(CurateReason::rank);
    reasons.dedup_by(|a, b| a.code == b.code);
    if reasons.len() <= limit {
        return;
    }
    let caveat = reasons
        .iter()
        .position(|r| r.code.is_caveat())
        .filter(|ix| *ix >= limit)
        .map(|ix| reasons[ix].clone());
    reasons.truncate(limit);
    if let Some(caveat) = caveat {
        // The last slot, so the arguments a photographer reads first are still the strongest.
        reasons.pop();
        reasons.push(caveat);
    }
}

/// A reason list that is never empty.
///
/// A pick with no reasons violates invariant 2, and migration 29's two triggers refuse one. This is
/// what a caller uses when every threshold happened to fall the wrong way: the fallback names the
/// term that was strongest even though it was not strong enough to speak up on its own, which is a
/// true sentence rather than a placeholder.
pub fn ensure_reason(reasons: &mut Vec<CurateReason>, fallback: CurateCode, weight: f32) {
    if reasons.is_empty() {
        reasons.push(CurateReason::plain(fallback, weight));
    }
}

/// The share of a weighted blend that was actually measured.
///
/// The confidence every selector in this phase is built on. A frame with two of five readings gets a
/// narrower score, and saying so is the difference between a suggestion a photographer can weigh and
/// one they have to take on trust.
#[must_use]
pub fn measured_share(terms: &[(f32, bool)]) -> f32 {
    let total: f32 = terms.iter().map(|(w, _)| *w).sum();
    if total <= f32::EPSILON {
        return 0.0;
    }
    let measured: f32 = terms
        .iter()
        .filter(|(_, seen)| *seen)
        .map(|(w, _)| *w)
        .sum();
    (measured / total).clamp(0.0, 1.0)
}

/// A weighted blend over the terms that were measured, renormalised.
///
/// `None` when nothing was measured, which is a **skip** rather than a score of zero: a caller that
/// treated an unmeasurable frame as a bad one would rank a wedding nobody analysed below one that
/// was analysed and found wanting.
#[must_use]
pub fn blend(terms: &[(f32, f32, bool)]) -> Option<f32> {
    let mut weight = 0.0f32;
    let mut sum = 0.0f32;
    for (w, value, measured) in terms {
        if *measured {
            weight += *w;
            sum += *w * value;
        }
    }
    (weight > f32::EPSILON).then(|| (sum / weight).clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vetoes_lead_and_the_list_is_truncated() {
        let mut reasons = vec![
            CurateReason::plain(CurateCode::StrongComposition, 0.3),
            CurateReason::plain(CurateCode::EmotionalPeak, 0.9),
            CurateReason::plain(CurateCode::TechnicalVeto, -1.0),
            CurateReason::plain(CurateCode::UniqueFrame, 0.5),
            CurateReason::plain(CurateCode::StoryImportant, 0.2),
        ];
        rank_reasons(&mut reasons, 3);
        assert_eq!(reasons.len(), 3);
        assert_eq!(reasons[0].code, CurateCode::TechnicalVeto);
        assert_eq!(reasons[1].code, CurateCode::EmotionalPeak);
    }

    #[test]
    fn a_caveat_survives_truncation_and_lands_last() {
        // The defect this function was written with: four strong arguments crowd out the sentence
        // saying what could not be checked, so the only picks that show a caveat are the ones with
        // nothing else to say.
        let mut reasons = vec![
            CurateReason::plain(CurateCode::EmotionalPeak, 0.95),
            CurateReason::plain(CurateCode::StrongComposition, 0.90),
            CurateReason::plain(CurateCode::TechnicalExcellence, 0.88),
            CurateReason::plain(CurateCode::StoryImportant, 0.80),
            CurateReason::plain(CurateCode::UniquenessUnavailable, -0.05),
        ];
        rank_reasons(&mut reasons, 4);
        assert_eq!(reasons.len(), 4);
        assert_eq!(reasons[3].code, CurateCode::UniquenessUnavailable);
        assert_eq!(reasons[0].code, CurateCode::EmotionalPeak);
    }

    #[test]
    fn only_one_slot_is_reserved_however_many_caveats_there_are() {
        let mut reasons = vec![
            CurateReason::plain(CurateCode::EmotionalPeak, 0.95),
            CurateReason::plain(CurateCode::StrongComposition, 0.90),
            CurateReason::plain(CurateCode::UniquenessUnavailable, -0.06),
            CurateReason::plain(CurateCode::SkinLocusUnavailable, -0.05),
        ];
        rank_reasons(&mut reasons, 3);
        let caveats = reasons.iter().filter(|r| r.code.is_caveat()).count();
        assert_eq!(caveats, 1);
        assert_eq!(reasons.len(), 3);
    }

    #[test]
    fn a_caveat_already_inside_the_limit_is_not_moved_to_the_end() {
        let mut reasons = vec![
            CurateReason::plain(CurateCode::UniquenessUnavailable, -0.9),
            CurateReason::plain(CurateCode::EmotionalPeak, 0.5),
            CurateReason::plain(CurateCode::StrongComposition, 0.4),
            CurateReason::plain(CurateCode::StoryImportant, 0.3),
            CurateReason::plain(CurateCode::TechnicalExcellence, 0.2),
        ];
        rank_reasons(&mut reasons, 3);
        assert_eq!(reasons[0].code, CurateCode::UniquenessUnavailable);
        assert_eq!(reasons.len(), 3);
    }

    #[test]
    fn a_duplicate_code_is_collapsed() {
        let mut reasons = vec![
            CurateReason::plain(CurateCode::EmotionalPeak, 0.9),
            CurateReason::detailed(CurateCode::EmotionalPeak, "again", 0.8),
        ];
        rank_reasons(&mut reasons, 4);
        assert_eq!(reasons.len(), 1);
    }

    #[test]
    fn an_empty_list_gets_one_reason_rather_than_violating_invariant_two() {
        let mut reasons = Vec::new();
        ensure_reason(&mut reasons, CurateCode::StrongTonalSeparation, 0.5);
        assert_eq!(reasons.len(), 1);
        // And it does not add a second when there is already something to say.
        ensure_reason(&mut reasons, CurateCode::HighEmotion, 0.5);
        assert_eq!(reasons.len(), 1);
    }

    #[test]
    fn a_blend_over_nothing_measured_is_none_rather_than_zero() {
        assert_eq!(blend(&[(0.5, 0.9, false), (0.5, 0.8, false)]), None);
        let partial = blend(&[(0.5, 0.9, true), (0.5, 0.1, false)]).expect("one term measured");
        assert!((partial - 0.9).abs() < 1e-6, "renormalised over what was seen");
    }

    #[test]
    fn the_measured_share_is_the_weight_that_was_seen() {
        assert_eq!(measured_share(&[(0.6, true), (0.4, false)]), 0.6);
        assert_eq!(measured_share(&[]), 0.0);
    }
}
