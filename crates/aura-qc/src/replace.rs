//! Swapping a delivered frame for its runner-up. PHASE-27 section 6.4.
//!
//! ## The feature photographers will demo, and the one with the worst failure mode
//!
//! Section 6.4's example is the pitch: "Image #382's face is below the sharpness threshold; #381 in
//! the same moment has eyes open, higher sharpness and a better expression."
//!
//! It is also the only remedy in this phase whose mistake a photographer **cannot see**. A bad
//! parameter fix produces a slightly worse photograph that the loop reverts inside the same pass; a
//! bad swap delivers a *different photograph*, and somebody scrolling a gallery has no way to notice
//! that the frame they would have chosen is not in it.
//!
//! Three things follow, and all three are structural rather than careful.
//!
//! ## Coverage is a filter, and the filter runs first
//!
//! [`consider`] re-validates coverage **before** any metric is compared. A candidate that would
//! leave a must-have uncovered is not a worse candidate - it is not a candidate, and it never
//! reaches the comparison.
//!
//! The temptation this avoids would look reasonable in review: score the swap, notice it breaks
//! coverage, dock it. A docked swap wins as soon as its metrics are good enough, and "good enough"
//! is a tuning parameter. Phase 12 wrote this rule for coverage guarantees, phase 23 for crop
//! safety and phase 24 made it a property of the type system; this is its fourth application, and
//! the ordering is enforced by [`Verdict`] having no variant that carries both a refusal and a
//! score.
//!
//! ## The margin is a share of the threshold rather than a difference
//!
//! A runner-up that measures 2 % better is the same photograph twice. Swapping on that margin makes
//! the contents of a gallery a function of measurement noise, and on a build whose upstream heads
//! are placeholders, *entirely* a function of it.
//!
//! ## Only phase 12's own runner-up
//!
//! [`crate::remedy::validate`] refuses a `ReplaceFrame` naming any other frame. A swap to a frame
//! phase 12 did not rank would be this phase re-answering "what is being delivered", which is
//! `CullService`'s question - and two answers to it is a delivery that does not match the album.

use aura_core::contract::qc::{
    ImageId, QcCategory, QcCode, QcTicket, Replacement, REPLACE_CONFIDENCE_FLOOR,
};
use aura_core::contract::scene::Timestamp;

use crate::checks::Frame;
use crate::policy::LoopPolicy;

/// What a swap would do to the gallery's guarantees.
///
/// Supplied by the caller rather than computed here, because coverage is phase 12's question and
/// this crate has no `CullService`. `api::collect` builds it by asking whether the runner-up carries
/// the same coverage role as the frame it would replace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoverageEffect {
    /// True when the frame being replaced is holding a must-have or an identity minimum.
    pub replaced_is_protected: bool,
    /// True when the runner-up would satisfy the same guarantee.
    ///
    /// The whole of the filter. A protected frame may still be swapped - a blurred photograph of the
    /// rings can be replaced by a sharper one of the rings - and only for a candidate that carries
    /// the guarantee too.
    pub replacement_covers_same: bool,
    /// True when the runner-up is already in the delivered gallery.
    ///
    /// A swap to a frame that is already delivered would leave the gallery one frame shorter and
    /// the moment represented once instead of twice, which is a size change nobody asked for.
    pub replacement_already_selected: bool,
}

impl CoverageEffect {
    /// The unprotected case: nothing is holding the frame in the gallery.
    #[must_use]
    pub const fn unprotected() -> Self {
        Self {
            replaced_is_protected: false,
            replacement_covers_same: false,
            replacement_already_selected: false,
        }
    }

    /// True when this swap leaves every guarantee intact.
    #[must_use]
    pub const fn holds(&self) -> bool {
        if self.replacement_already_selected {
            return false;
        }
        !self.replaced_is_protected || self.replacement_covers_same
    }
}

/// What the candidate measured, on the same check the ticket was opened on.
///
/// One number rather than a whole `Frame`, because a swap is decided on the *ticket's own metric*.
/// Comparing two frames on everything would make the decision a fused score, and a fused score is
/// how a sharper frame with a worse expression wins - which is phase 12's job and phase 12 already
/// did it when it chose the runner-up over the rest of the moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CandidateMetric {
    /// The runner-up's deviation on the ticket's category, in the ticket's unit.
    pub deviation: f32,
    /// True when the runner-up carries a finding of its own in a *different* category.
    ///
    /// A swap that fixes softness and introduces a colour drift is not an improvement, and the loop
    /// would catch it on the collateral re-check - but catching it here saves a round.
    pub has_other_findings: bool,
}

/// Why a swap was refused, or what it would gain.
///
/// A single enum rather than an `Option<f32>` plus a reason, so that a refusal cannot carry a score
/// somebody later compares. See this module's header: the ordering is the guarantee, and a type that
/// could hold both would let a caller re-order it.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    /// Swap, and by this margin as a share of the threshold.
    Swap {
        /// How much better the runner-up is, as a share of the ticket's own threshold.
        margin: f32,
        /// The runner-up.
        with: ImageId,
    },
    /// Do not swap, for this reason.
    Refuse(QcCode),
}

impl Verdict {
    /// The runner-up, when this verdict is a swap.
    #[must_use]
    pub const fn accepted(&self) -> Option<ImageId> {
        match self {
            Self::Swap { with, .. } => Some(*with),
            Self::Refuse(_) => None,
        }
    }
}

/// Decide whether one frame should be replaced by its runner-up.
///
/// The order of the four gates is the guarantee, and it is deliberate:
///
/// 1. **Is there a runner-up at all?** `RunnerUpAbsent` - a moment photographed once.
/// 2. **Would coverage hold?** `ReplacementBreaksCoverage` - a filter, before any metric.
/// 3. **Is the confidence high enough?** The policy floor, never below the contract's.
/// 4. **Is the runner-up clearly better?** `RunnerUpNotBetter` - the margin.
///
/// Reordering 2 after 4 would turn coverage into a score. Reordering 3 after 4 would let a marginal
/// improvement justify a low-confidence swap.
#[must_use]
pub fn consider(
    ticket: &QcTicket,
    frame: &Frame,
    candidate: CandidateMetric,
    coverage: CoverageEffect,
    policy: LoopPolicy,
) -> Verdict {
    let Some(runner_up) = frame.runner_up else {
        return Verdict::Refuse(QcCode::RunnerUpAbsent);
    };

    // Gate 2, before any metric is looked at.
    if !coverage.holds() {
        return Verdict::Refuse(QcCode::ReplacementBreaksCoverage);
    }

    // Gate 3. The policy may raise this floor and the loader refuses one below the contract's.
    let floor = policy.replace_confidence.max(REPLACE_CONFIDENCE_FLOOR);
    if ticket.confidence < floor {
        return Verdict::Refuse(QcCode::RunnerUpNotBetter);
    }

    // A candidate carrying its own problems elsewhere is not clearly better, whatever it measures
    // on this one check.
    if candidate.has_other_findings {
        return Verdict::Refuse(QcCode::RunnerUpNotBetter);
    }

    // Gate 4. A share of the threshold rather than a difference, so that one rule works across the
    // five units the ten checks are measured in.
    let margin = margin(ticket, candidate.deviation);
    if margin < policy.replace_margin {
        return Verdict::Refuse(QcCode::RunnerUpNotBetter);
    }

    Verdict::Swap {
        margin,
        with: runner_up,
    }
}

/// How much better the candidate is, as a share of the ticket's own threshold.
///
/// Duplicates run the other way - a smaller hamming distance is worse - and the deviation this
/// phase stores for them is already inverted by `checks::duplicate`, so no special case is needed
/// here. The assertion that keeps it true lives in that module's own tests.
#[must_use]
pub fn margin(ticket: &QcTicket, candidate_deviation: f32) -> f32 {
    if ticket.threshold <= 0.0 || !ticket.deviation.is_finite() || !candidate_deviation.is_finite()
    {
        return 0.0;
    }
    ((ticket.deviation - candidate_deviation) / ticket.threshold).max(0.0)
}

/// Record a swap that was made.
///
/// `coverage_held` is always true, because a swap that would have broken coverage never reached
/// here - it was refused at gate 2. The column exists so that "no replacement broke coverage" is a
/// `SELECT MIN(coverage_held)` rather than a claim about this function, which is phase 16's rule in
/// its seventh application: a guarantee is measured, not asserted.
#[must_use]
pub fn record(
    ticket: &QcTicket,
    with: ImageId,
    candidate: CandidateMetric,
    margin: f32,
    now: Timestamp,
) -> Replacement {
    Replacement {
        ticket: ticket.id,
        replaced: ticket.image_id,
        replacement: with,
        category: ticket.category,
        metric_before: ticket.deviation,
        metric_after: candidate.deviation,
        confidence: ticket.confidence,
        coverage_held: true,
        note: note_for(ticket, candidate, margin),
        at: now,
    }
}

/// The sentence section 6.4 asks for, built from the numbers rather than stored.
///
/// Phase 09's rule again: a stored sentence is copy a release can change. This one is short because
/// it sits beside a before-and-after in the report, and the two photographs are the argument.
fn note_for(ticket: &QcTicket, candidate: CandidateMetric, margin: f32) -> String {
    format!(
        "{} on this frame measured {:.2} {} against a {:.2} {} threshold; the alternative from the \
         same moment measures {:.2} {}, which is {:.0}% of the threshold better",
        ticket.category,
        ticket.deviation,
        ticket.category.unit(),
        ticket.threshold,
        ticket.category.unit(),
        candidate.deviation,
        ticket.category.unit(),
        margin * 100.0,
    )
}

/// Whether a category's finding may be resolved by a swap at all.
///
/// Five of the ten. The other five are excluded for reasons that differ:
///
/// * **Coverage** - a swap is what *creates* a coverage question, so resolving one with a swap is
///   circular.
/// * **Duplicate** - the two frames are near-identical, so the runner-up is the problem rather
///   than the fix.
/// * **Cleanup** - a removal artefact is on a frame somebody chose to remove something from;
///   swapping delivers a frame with the object still in it, which is a different decision from the
///   one the ticket is about.
/// * **Mask** and **Retouch** - both are damage a strength reduction undoes on the frame the
///   photographer's own selection chose. Swapping to avoid a fixable edit throws away phase 12's
///   ranking to save a parameter change.
#[must_use]
pub const fn swappable(category: QcCategory) -> bool {
    matches!(
        category,
        QcCategory::Sharpness
            | QcCategory::Exposure
            | QcCategory::Consistency
            | QcCategory::Skin
            | QcCategory::Crop
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::Finding;
    use aura_core::contract::ids::ProjectId;
    use aura_core::contract::qc::{Remedy, SolveTarget};
    use aura_core::contract::scene::SceneId;

    fn frame_with_runner_up() -> (Frame, ImageId) {
        let runner_up = ImageId::new();
        let mut frame = Frame::empty(ImageId::new(), SceneId::CouplePortrait);
        frame.runner_up = Some(runner_up);
        (frame, runner_up)
    }

    fn ticket(confidence: f32, deviation: f32) -> QcTicket {
        let frame = Frame::empty(ImageId::new(), SceneId::CouplePortrait);
        let finding = Finding::new(
            QcCategory::Sharpness,
            QcCode::SharpnessBelowFloor,
            deviation,
            0.20,
            0.1,
            confidence,
        );
        crate::ticket::from_finding(
            ProjectId::new(),
            &frame,
            finding,
            Remedy::ResolveParam {
                target: SolveTarget::Restoration,
                constraint: "x".into(),
            },
            0,
        )
    }

    fn better() -> CandidateMetric {
        CandidateMetric {
            deviation: 0.02,
            has_other_findings: false,
        }
    }

    #[test]
    fn a_clearly_better_runner_up_is_swapped() {
        let (frame, runner_up) = frame_with_runner_up();
        let verdict = consider(
            &ticket(0.95, 0.60),
            &frame,
            better(),
            CoverageEffect::unprotected(),
            LoopPolicy::reference(),
        );
        assert_eq!(verdict.accepted(), Some(runner_up));
    }

    #[test]
    fn coverage_is_checked_before_any_metric_is_compared() {
        // The candidate is enormously better and the confidence is maximal. It is still refused,
        // and the refusal code says coverage rather than merit - which is the whole of ADR-0055
        // section 6. A build that scored the swap and docked it would swap this frame as soon as
        // somebody nudged a weight.
        let (frame, _) = frame_with_runner_up();
        let breaks = CoverageEffect {
            replaced_is_protected: true,
            replacement_covers_same: false,
            replacement_already_selected: false,
        };
        let verdict = consider(
            &ticket(1.0, 9.0),
            &frame,
            CandidateMetric {
                deviation: 0.0,
                has_other_findings: false,
            },
            breaks,
            LoopPolicy::reference(),
        );
        assert_eq!(verdict, Verdict::Refuse(QcCode::ReplacementBreaksCoverage));
        assert_eq!(verdict.accepted(), None);
    }

    #[test]
    fn a_protected_frame_may_be_swapped_for_one_that_carries_the_same_guarantee() {
        // A blurred photograph of the rings can be replaced by a sharper photograph of the rings.
        let (frame, runner_up) = frame_with_runner_up();
        let holds = CoverageEffect {
            replaced_is_protected: true,
            replacement_covers_same: true,
            replacement_already_selected: false,
        };
        let verdict = consider(
            &ticket(0.95, 0.60),
            &frame,
            better(),
            holds,
            LoopPolicy::reference(),
        );
        assert_eq!(verdict.accepted(), Some(runner_up));
    }

    #[test]
    fn a_runner_up_already_in_the_gallery_is_refused() {
        let (frame, _) = frame_with_runner_up();
        let already = CoverageEffect {
            replaced_is_protected: false,
            replacement_covers_same: false,
            replacement_already_selected: true,
        };
        assert_eq!(
            consider(
                &ticket(0.99, 0.60),
                &frame,
                better(),
                already,
                LoopPolicy::reference()
            ),
            Verdict::Refuse(QcCode::ReplacementBreaksCoverage)
        );
    }

    #[test]
    fn a_marginally_better_runner_up_is_the_same_photograph_twice() {
        let (frame, _) = frame_with_runner_up();
        // Two per cent of the threshold better.
        let noise = CandidateMetric {
            deviation: 0.60 - 0.004,
            has_other_findings: false,
        };
        assert_eq!(
            consider(
                &ticket(0.99, 0.60),
                &frame,
                noise,
                CoverageEffect::unprotected(),
                LoopPolicy::reference()
            ),
            Verdict::Refuse(QcCode::RunnerUpNotBetter)
        );
    }

    #[test]
    fn a_swap_needs_more_confidence_than_a_parameter_fix() {
        let (frame, _) = frame_with_runner_up();
        // 0.80 clears `FIX_CONFIDENCE_FLOOR` and not `REPLACE_CONFIDENCE_FLOOR`.
        assert_eq!(
            consider(
                &ticket(0.80, 0.60),
                &frame,
                better(),
                CoverageEffect::unprotected(),
                LoopPolicy::reference()
            ),
            Verdict::Refuse(QcCode::RunnerUpNotBetter)
        );
    }

    #[test]
    fn the_policy_may_raise_the_confidence_floor_and_never_lower_it() {
        let (frame, _) = frame_with_runner_up();
        let mut stricter = LoopPolicy::reference();
        stricter.replace_confidence = 0.99;
        assert!(matches!(
            consider(
                &ticket(0.90, 0.60),
                &frame,
                better(),
                CoverageEffect::unprotected(),
                stricter
            ),
            Verdict::Refuse(_)
        ));

        // And a policy that tried to lower it below the contract's floor is ignored here as well as
        // refused by the loader - two layers, because the loader lives in a file and this lives in
        // the path a swap actually takes.
        let mut looser = LoopPolicy::reference();
        looser.replace_confidence = 0.10;
        assert!(matches!(
            consider(
                &ticket(0.50, 0.60),
                &frame,
                better(),
                CoverageEffect::unprotected(),
                looser
            ),
            Verdict::Refuse(_)
        ));
    }

    #[test]
    fn a_candidate_with_problems_of_its_own_is_not_clearly_better() {
        let (frame, _) = frame_with_runner_up();
        let mixed = CandidateMetric {
            deviation: 0.0,
            has_other_findings: true,
        };
        assert_eq!(
            consider(
                &ticket(0.99, 0.60),
                &frame,
                mixed,
                CoverageEffect::unprotected(),
                LoopPolicy::reference()
            ),
            Verdict::Refuse(QcCode::RunnerUpNotBetter)
        );
    }

    #[test]
    fn a_moment_photographed_once_says_so_rather_than_saying_not_better() {
        let frame = Frame::empty(ImageId::new(), SceneId::CouplePortrait);
        // "There is no alternative" and "the alternative is not better" are two different sentences
        // a photographer reads and two different runbooks.
        assert_eq!(
            consider(
                &ticket(0.99, 9.0),
                &frame,
                better(),
                CoverageEffect::unprotected(),
                LoopPolicy::reference()
            ),
            Verdict::Refuse(QcCode::RunnerUpAbsent)
        );
    }

    #[test]
    fn a_recorded_swap_always_carries_the_coverage_proof() {
        let ticket = ticket(0.95, 0.60);
        let with = ImageId::new();
        let row = record(&ticket, with, better(), 0.9, 1_700_000_000);
        assert!(row.coverage_held);
        assert_eq!(row.replaced, ticket.image_id);
        assert_eq!(row.replacement, with);
        // Both metrics, never the difference: a photographer looking at a swap wants to know what
        // each frame measured.
        assert_eq!(row.metric_before, 0.60);
        assert_eq!(row.metric_after, 0.02);
        assert!(row.note.contains("0.60"));
        assert!(row.note.contains("0.02"));
    }

    #[test]
    fn five_categories_may_be_resolved_by_a_swap_and_five_may_not() {
        let allowed = QcCategory::ALL.iter().filter(|c| swappable(**c)).count();
        assert_eq!(allowed, 5);
        // The two gallery-scoped ones, in particular: a swap is what *creates* a coverage question.
        assert!(!swappable(QcCategory::Coverage));
        assert!(!swappable(QcCategory::Duplicate));
    }

    #[test]
    fn a_refusal_can_never_carry_a_margin() {
        // The type is the guarantee. `Verdict::Refuse` has no field a score could go in, so a caller
        // cannot re-order the four gates by comparing a refused swap's merit.
        let refused = Verdict::Refuse(QcCode::ReplacementBreaksCoverage);
        assert_eq!(refused.accepted(), None);
    }
}
