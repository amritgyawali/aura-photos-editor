//! Near-duplicate frames that both reached the gallery. PHASE-27 section 2.1, phase 08.
//!
//! ## What this check is, and what it is carefully not
//!
//! Phase 08 owns duplicate policy. It decides what a burst is, what a moment is and which frames are
//! the same shot, and phase 12 then picks one keeper per moment. **This check does not re-answer any
//! of that.**
//!
//! What it asks is narrower and is a question about the *delivered* set: did two frames a client
//! will scroll past in sequence turn out to be the same photograph? That can happen without either
//! upstream phase being wrong - two moments that phase 08 correctly separated on time can still
//! contain frames that look identical, and phase 12 keeps one from each because coverage asked for
//! both.
//!
//! Phase 05's rule is what makes the separation legal: a distance is evidence and the deciding phase
//! owns the threshold. `MAX_DUPLICATE_HAMMING` is this phase's own and is deliberately tighter than
//! phase 08's near-duplicate band, so this check fires only on pairs phase 08 would also call
//! identical.
//!
//! ## A pair from two different moments is the stronger finding
//!
//! Two frames of one moment being similar is what a moment *is*, so the leak is that both were
//! delivered. A pair from two different moments at the same hamming distance is a stronger
//! statement, because phase 08 did not even think they were the same shot and they look identical
//! anyway - which usually means one of them is in the gallery for a coverage reason that a person
//! should look at.
//!
//! ## The nearest neighbour is measured inside the gallery
//!
//! A near-identical frame phase 12 *rejected* is phase 12 working, not a leak. The reading this
//! check is handed is the nearest neighbour among delivered frames only, and that restriction is in
//! `api::collect` rather than here - which is why this module cannot get it wrong and why the
//! module doc says so.

use aura_core::contract::qc::{Evidence, QcCategory, QcCode};

use super::{Finding, Frame, Outcome};
use crate::policy::Thresholds;

/// Inspect one frame against its nearest delivered neighbour.
#[must_use]
pub fn inspect(frame: &Frame, thresholds: &Thresholds) -> Outcome {
    let Some(duplicate) = frame.duplicate.as_ref() else {
        return Outcome::Skipped("no nearest-neighbour reading for this frame");
    };
    let row = thresholds.scene(frame.scene);
    if duplicate.hamming > row.duplicate_hamming {
        return Outcome::Clean;
    }

    // Deviation runs the other way from every other check in the phase: a *smaller* hamming
    // distance is a worse problem. It is stated as the shortfall below the threshold so the
    // severity ratio still means "how far past acceptable", and so one queue can order it beside a
    // dE00 finding without the comparison inverting.
    let deviation = (row.duplicate_hamming.saturating_sub(duplicate.hamming)) as f32 + 1.0;
    let threshold = 1.0;

    let confidence = if duplicate.same_moment {
        // Phase 08 already grouped them, so this is a delivery question rather than a similarity
        // question, and one of the two is very likely a keeper phase 12 chose deliberately.
        0.65
    } else {
        // Two frames phase 08 thought were different shots and that look identical anyway.
        0.85
    };

    Outcome::Found(vec![Finding::new(
        QcCategory::Duplicate,
        QcCode::DuplicateLeak,
        deviation,
        threshold,
        // Zero. There is no smaller amount of a duplicate: the remedy is to drop one of the two,
        // which is a coverage-checked removal rather than a parameter change, and the loop judges
        // it on whether the pair stopped being a pair.
        0.0,
        confidence,
    )
    .with_evidence(Evidence::Frames(vec![duplicate.neighbour]))])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::DuplicateReading;
    use aura_core::contract::qc::ImageId;
    use aura_core::contract::scene::SceneId;

    fn frame_with(hamming: u32, same_moment: bool) -> Frame {
        Frame {
            duplicate: Some(DuplicateReading {
                neighbour: ImageId::new(),
                hamming,
                same_moment,
            }),
            ..Frame::empty(ImageId::new(), SceneId::Candid)
        }
    }

    #[test]
    fn two_different_photographs_are_clean() {
        assert_eq!(
            inspect(&frame_with(30, false), &Thresholds::reference()),
            Outcome::Clean
        );
    }

    #[test]
    fn two_identical_photographs_are_a_finding_pointing_at_the_neighbour() {
        let findings = inspect(&frame_with(0, true), &Thresholds::reference()).findings();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, QcCode::DuplicateLeak);
        assert!(matches!(findings[0].evidence, Evidence::Frames(ref f) if f.len() == 1));
    }

    #[test]
    fn a_closer_pair_is_a_worse_finding_even_though_the_number_is_smaller() {
        // The deviation runs the other way from every other check, and this is the assertion that
        // the inversion was handled rather than inherited.
        let near = inspect(&frame_with(0, false), &Thresholds::reference()).findings();
        let further = inspect(&frame_with(5, false), &Thresholds::reference()).findings();
        assert!(near[0].severity() > further[0].severity());
    }

    #[test]
    fn a_pair_from_two_moments_is_more_confident_than_a_pair_from_one() {
        let one = inspect(&frame_with(1, true), &Thresholds::reference()).findings();
        let two = inspect(&frame_with(1, false), &Thresholds::reference()).findings();
        assert!(two[0].confidence > one[0].confidence);
        assert_eq!(one[0].deviation, two[0].deviation);
    }

    #[test]
    fn a_duplicate_predicts_no_gain_because_there_is_no_smaller_amount_of_one() {
        let findings = inspect(&frame_with(0, false), &Thresholds::reference()).findings();
        assert_eq!(findings[0].expected_gain, 0.0);
    }

    #[test]
    fn the_threshold_is_this_phases_own_and_is_tighter_than_phase_eights() {
        // Phase 05's rule: a distance is evidence and the deciding phase owns the threshold. This
        // check fires only on pairs phase 08 would also call identical.
        assert!(crate::policy::MAX_DUPLICATE_HAMMING <= 8);
    }

    #[test]
    fn an_absent_reading_skips() {
        let frame = Frame::empty(ImageId::new(), SceneId::Candid);
        assert!(matches!(
            inspect(&frame, &Thresholds::reference()),
            Outcome::Skipped(_)
        ));
    }
}
