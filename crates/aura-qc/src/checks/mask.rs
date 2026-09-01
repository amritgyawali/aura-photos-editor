//! Edits applied through boundaries that did not support them. PHASE-27 section 2.1, phases 18
//! and 19.
//!
//! ## Why this check reads a product rather than a quality
//!
//! Phase 18's rule is that a region says how much may be done with it and a later phase multiplies:
//! `Mask::allowance` is the geometric mean of the class confidence and the boundary quality, and
//! `quality::allowance` is the one place a gating decision is made.
//!
//! A ragged mask is not itself a defect. A ragged mask **that something ran at full strength
//! through** is - that is the bright rim along a jaw, the halo at a hairline, the visible step where
//! a background reduction met a wall. So the measured deviation is the *overreach*: how far the
//! applied strength exceeded what the region supports. A frame with a poor mask and no operation
//! inside it is clean, and that is correct, because nothing was done with it.
//!
//! ## A gated operation is not an artefact, and a gated operation that ran is a contradiction
//!
//! Phase 19 gates an operation down when its region does not support it, and lists what it gated in
//! `LocalLightPlan::gated_by_mask_quality`. A frame where an operation appears in that list *and*
//! ran at strength is a frame where two subsystems disagree about what happened, which is a stronger
//! finding than either alone - so it raises the confidence rather than adding a second ticket.
//!
//! ## `MaskUncovered` is a finding and an unavailability at once
//!
//! It is the only code in this phase that is both. A local operation that ran with no region behind
//! it is a real defect - something edited an area nobody delimited - *and* it is a statement that
//! this check could not measure a boundary. `QcCode::is_finding` and `QcCode::is_unavailable` are
//! both true for it, and the contract's own test carves out that one exception explicitly rather
//! than letting it look like an oversight.

use aura_core::contract::qc::{QcCategory, QcCode};

use super::{Finding, Frame, Outcome};
use crate::policy::Thresholds;

/// How much of an overreach a strength reduction is predicted to remove.
///
/// Nearly all of it, and this is the most direct remedy in the phase: the deviation *is* applied
/// strength minus allowance, so reducing the strength reduces it one for one until it reaches zero.
/// The share is below one only because `MIN_STRENGTH_FACTOR` bounds how far a single reduction may
/// go.
const OVERREACH_GAIN_SHARE: f32 = 0.90;

/// The applied strength above which an operation on an uncovered region is worth a ticket.
///
/// A local operation that ran at 2 % strength through no region is a rounding error rather than a
/// defect a photographer can see. Ten per cent is the point at which a lift becomes visible against
/// its own edge in the fixture corpus.
const UNCOVERED_STRENGTH_FLOOR: f32 = 0.10;

/// Inspect one frame's regions.
#[must_use]
pub fn inspect(frame: &Frame, thresholds: &Thresholds) -> Outcome {
    let Some(mask) = frame.mask.as_ref() else {
        return Outcome::Skipped("no mask readings for this frame");
    };
    if mask.regions.is_empty() {
        // No regions at all is different from regions that are poor. Phase 18's
        // `SkipReason::MaskGeneratorAbsent` upstream, and a skip here: a frame nobody could
        // segment has not been checked for edge artefacts.
        return Outcome::Skipped("no regions were generated for this frame");
    }
    let row = thresholds.scene(frame.scene);
    let mut findings = Vec::new();

    for region in &mask.regions {
        let overreach = region.overreach();
        let gated = mask.gated.contains(&region.kind);

        if region.allowance() <= 0.0 && region.applied_strength > UNCOVERED_STRENGTH_FLOOR {
            // Something ran through a region with no usable boundary at all.
            findings.push(Finding::new(
                QcCategory::Mask,
                QcCode::MaskUncovered,
                region.applied_strength,
                UNCOVERED_STRENGTH_FLOOR,
                // Zero gain: the remedy is to revert the operation, not to do less of it, and a
                // reverted operation removes the whole deviation rather than a share of it. Stating
                // a gain here would make the loop measure a revert against a prediction.
                0.0,
                0.85,
            ));
            continue;
        }

        if overreach > row.mask_overreach {
            let mut finding = Finding::new(
                QcCategory::Mask,
                QcCode::MaskEdgeArtefact,
                overreach,
                row.mask_overreach,
                (overreach - row.mask_overreach) * OVERREACH_GAIN_SHARE,
                {
                    let measured = margin_confidence(overreach, row.mask_overreach);
                    if gated {
                        // Two subsystems disagreeing about what happened is stronger evidence than
                        // either alone, so the contradiction moves the confidence half way to
                        // certainty rather than replacing it with a constant. A constant would be
                        // *lower* than the measurement on a badly overreached region, which is the
                        // opposite of what corroboration should do.
                        measured.midpoint(1.0)
                    } else {
                        measured
                    }
                },
            );
            if gated {
                finding = finding.because(QcCode::MaskQualityLow, 0.8);
            }
            findings.push(finding);
        } else if region.confidence < 0.35 && region.applied_strength > UNCOVERED_STRENGTH_FLOOR {
            // The class itself is doubtful even though the boundary held. A separate code, because
            // a photographer can re-brush a boundary and cannot re-brush a class - phase 18's
            // reason for keeping the two numbers apart, consumed here.
            findings.push(Finding::new(
                QcCategory::Mask,
                QcCode::MaskQualityLow,
                1.0 - region.confidence,
                0.65,
                0.0,
                0.60,
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
    use crate::checks::{MaskReading, MaskRegion};
    use aura_core::contract::qc::ImageId;
    use aura_core::contract::scene::SceneId;
    use aura_vision::contract::mask::MaskKind;

    fn region(confidence: f32, edge: f32, strength: f32) -> MaskRegion {
        MaskRegion {
            kind: MaskKind::Face,
            confidence,
            edge_quality: edge,
            applied_strength: strength,
        }
    }

    fn frame_with(regions: Vec<MaskRegion>, gated: Vec<MaskKind>) -> Frame {
        Frame {
            mask: Some(MaskReading { regions, gated }),
            ..Frame::empty(ImageId::new(), SceneId::CouplePortrait)
        }
    }

    #[test]
    fn a_good_region_edited_within_its_allowance_is_clean() {
        let frame = frame_with(vec![region(0.95, 0.95, 0.6)], Vec::new());
        assert_eq!(inspect(&frame, &Thresholds::reference()), Outcome::Clean);
    }

    #[test]
    fn a_ragged_region_nobody_edited_through_is_clean() {
        // The mask is poor and nothing was done with it. Phase 18's rule read correctly: a wrong
        // mask is silent until something edits through it.
        let frame = frame_with(vec![region(0.30, 0.10, 0.0)], Vec::new());
        assert_eq!(inspect(&frame, &Thresholds::reference()), Outcome::Clean);
    }

    #[test]
    fn a_ragged_region_edited_at_full_strength_is_the_finding() {
        let frame = frame_with(vec![region(0.90, 0.10, 1.0)], Vec::new());
        let findings = inspect(&frame, &Thresholds::reference()).findings();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, QcCode::MaskEdgeArtefact);
        // The deviation is the overreach, not the edge quality: 1.0 applied against an allowance of
        // sqrt(0.9 * 0.1) = 0.3.
        assert!(findings[0].deviation > 0.6 && findings[0].deviation < 0.75);
    }

    #[test]
    fn a_gated_operation_that_ran_anyway_raises_confidence_rather_than_adding_a_ticket() {
        let quiet = frame_with(vec![region(0.90, 0.10, 1.0)], Vec::new());
        let contradicted = frame_with(vec![region(0.90, 0.10, 1.0)], vec![MaskKind::Face]);
        let a = inspect(&quiet, &Thresholds::reference()).findings();
        let b = inspect(&contradicted, &Thresholds::reference()).findings();
        assert_eq!(a.len(), b.len(), "one ticket either way");
        assert!(b[0].confidence > a[0].confidence);
        assert!(b[0]
            .extra_reasons
            .iter()
            .any(|(code, _)| *code == QcCode::MaskQualityLow));
    }

    #[test]
    fn an_operation_through_a_region_with_no_boundary_predicts_no_gain() {
        let frame = frame_with(vec![region(0.0, 0.0, 0.8)], Vec::new());
        let findings = inspect(&frame, &Thresholds::reference()).findings();
        assert_eq!(findings[0].code, QcCode::MaskUncovered);
        // The remedy is a revert, which removes the whole deviation rather than a share of it.
        // Predicting a share would make the loop judge a revert against a partial expectation.
        assert_eq!(findings[0].expected_gain, 0.0);
    }

    #[test]
    fn a_doubtful_class_with_a_clean_boundary_is_its_own_code() {
        let frame = frame_with(vec![region(0.20, 0.99, 0.3)], Vec::new());
        let findings = inspect(&frame, &Thresholds::reference()).findings();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, QcCode::MaskQualityLow);
    }

    #[test]
    fn a_frame_with_no_regions_skips_rather_than_passes() {
        let frame = frame_with(Vec::new(), Vec::new());
        let outcome = inspect(&frame, &Thresholds::reference());
        assert!(matches!(outcome, Outcome::Skipped(_)));
        assert_ne!(outcome, Outcome::Clean);
    }

    #[test]
    fn the_uncovered_code_is_both_a_finding_and_an_unavailability() {
        // The only code in the phase that is both, and the contract's own test carves it out
        // explicitly. Asserted from this side too, so a change to either half is a red build in
        // the module that depends on the distinction.
        assert!(QcCode::MaskUncovered.is_finding());
        assert!(QcCode::MaskUncovered.is_unavailable());
    }

    #[test]
    fn an_absent_reading_skips() {
        let frame = Frame::empty(ImageId::new(), SceneId::CouplePortrait);
        assert!(matches!(
            inspect(&frame, &Thresholds::reference()),
            Outcome::Skipped(_)
        ));
    }
}
