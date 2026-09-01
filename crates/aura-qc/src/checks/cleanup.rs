//! Generative cleanup artefacts and undisclosed removals. PHASE-27 section 2.1, phase 24.
//!
//! ## Two findings, and the second one is not about how the photograph looks
//!
//! `CleanupArtefact` is the ordinary one: phase 24's self-check measured three artefacts against the
//! rest of the frame and one of them is visible. A photographer can look at the region and agree or
//! disagree.
//!
//! `CleanupUndisclosed` is different in kind. It says a removal reached the delivered gallery
//! **without a disclosure row**, which is not a visual defect at all - the photograph may look
//! perfect. It is a failure of the record, and phase 24 made three triggers and a recipe field to
//! stop it happening, so a finding here means one of those paths was routed around.
//!
//! That is why its confidence is 1.0 and its gain is zero. It is not a measurement with error bars:
//! either the row is there or it is not. And no strength reduction fixes a missing record, so the
//! only remedy is a person.
//!
//! ## A frame with no removals is clean rather than skipped
//!
//! This is the one check in the phase where an empty list is a real answer. Phase 24 proposes no
//! removals on most frames and, in this build, on *every* frame - `DISTRACTION_HEAD_TRAINED` is
//! false, so the safety engine refuses everything it finds. A frame with nothing removed has nothing
//! that could have been removed badly, which is a genuine pass and not an absence of evidence.
//!
//! The distinction from the other nine checks is worth stating precisely: elsewhere, an empty
//! reading means an upstream phase produced nothing *for this frame* and we cannot tell whether
//! there was anything to find. Here, an empty removal list is itself phase 24's answer.

use aura_core::contract::qc::{QcCategory, QcCode};

use super::{Finding, Frame, Outcome};
use crate::policy::Thresholds;

/// The threshold an undisclosed removal is measured against.
///
/// A boolean upstream, so the deviation is 1.0. Half, so the severity ratio is 2.0 and a missing
/// disclosure sits above the ordinary run of colour findings in the queue - which is where a
/// failure of the record belongs.
const DISCLOSURE_THRESHOLD: f32 = 0.5;

/// Inspect one frame's removals.
#[must_use]
pub fn inspect(frame: &Frame, thresholds: &Thresholds) -> Outcome {
    let Some(cleanup) = frame.cleanup.as_ref() else {
        return Outcome::Skipped("no cleanup record for this frame");
    };
    if cleanup.removals.is_empty() {
        // A real answer rather than an absence of one. See this module's header.
        return Outcome::Clean;
    }
    let row = thresholds.scene(frame.scene);
    let mut findings = Vec::new();

    for (artefact, disclosed) in &cleanup.removals {
        if !disclosed {
            findings.push(
                Finding::new(
                    QcCategory::Cleanup,
                    QcCode::CleanupUndisclosed,
                    1.0,
                    DISCLOSURE_THRESHOLD,
                    // No strength reduction fixes a missing record.
                    0.0,
                    // Not a measurement with error bars: either the row is there or it is not.
                    1.0,
                )
                .because(QcCode::EscalatedToHuman, 1.0),
            );
            continue;
        }
        if artefact.is_finite() && *artefact > row.cleanup_artefact {
            findings.push(Finding::new(
                QcCategory::Cleanup,
                QcCode::CleanupArtefact,
                *artefact,
                row.cleanup_artefact,
                // The remedy is a revert - putting the object back - which removes the whole
                // deviation rather than a share of it. Phase 24's self-check already tried the
                // smaller version and reverted what it could.
                0.0,
                margin_confidence(*artefact, row.cleanup_artefact).max(0.75),
            ));
        }
    }

    Outcome::from_findings(findings)
}

fn margin_confidence(deviation: f32, threshold: f32) -> f32 {
    if threshold <= 0.0 || !deviation.is_finite() {
        return 0.0;
    }
    let margin = ((deviation - threshold) / threshold).clamp(0.0, 1.0);
    (0.55 + 0.40 * margin).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::CleanupReading;
    use aura_core::contract::qc::ImageId;
    use aura_core::contract::scene::SceneId;

    fn frame_with(removals: Vec<(f32, bool)>) -> Frame {
        Frame {
            cleanup: Some(CleanupReading { removals }),
            ..Frame::empty(ImageId::new(), SceneId::ReceptionEntrance)
        }
    }

    #[test]
    fn a_frame_with_nothing_removed_is_clean_rather_than_skipped() {
        // The one check in the phase where an empty list is phase 24's own answer rather than an
        // absence of evidence - and in this build it is every frame.
        let outcome = inspect(&frame_with(Vec::new()), &Thresholds::reference());
        assert_eq!(outcome, Outcome::Clean);
        assert!(outcome.ran());
    }

    #[test]
    fn a_clean_removal_is_clean() {
        assert_eq!(
            inspect(&frame_with(vec![(0.02, true)]), &Thresholds::reference()),
            Outcome::Clean
        );
    }

    #[test]
    fn a_visible_removal_is_a_finding_with_no_predicted_gain() {
        let findings = inspect(&frame_with(vec![(0.9, true)]), &Thresholds::reference()).findings();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, QcCode::CleanupArtefact);
        // Phase 24's own self-check already reverted what a smaller version could fix, so the only
        // remedy left removes the whole deviation rather than a share of it.
        assert_eq!(findings[0].expected_gain, 0.0);
    }

    #[test]
    fn an_undisclosed_removal_is_certain_rather_than_measured() {
        let findings =
            inspect(&frame_with(vec![(0.0, false)]), &Thresholds::reference()).findings();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, QcCode::CleanupUndisclosed);
        // Either the row is there or it is not.
        assert_eq!(findings[0].confidence, 1.0);
        assert_eq!(findings[0].expected_gain, 0.0);
    }

    #[test]
    fn a_perfect_looking_removal_with_no_record_is_still_a_finding() {
        // The artefact score is zero: the photograph looks fine. The record is what failed.
        let findings =
            inspect(&frame_with(vec![(0.0, false)]), &Thresholds::reference()).findings();
        assert_eq!(findings[0].code, QcCode::CleanupUndisclosed);
    }

    #[test]
    fn a_missing_disclosure_outranks_an_ordinary_colour_finding() {
        let findings =
            inspect(&frame_with(vec![(0.0, false)]), &Thresholds::reference()).findings();
        assert!(findings[0].severity() >= 2.0);
    }

    #[test]
    fn one_finding_per_removal() {
        let findings = inspect(
            &frame_with(vec![(0.9, true), (0.95, true), (0.01, true)]),
            &Thresholds::reference(),
        )
        .findings();
        assert_eq!(findings.len(), 2);
    }

    #[test]
    fn an_absent_record_skips() {
        let frame = Frame::empty(ImageId::new(), SceneId::ReceptionEntrance);
        assert!(matches!(
            inspect(&frame, &Thresholds::reference()),
            Outcome::Skipped(_)
        ));
    }
}
