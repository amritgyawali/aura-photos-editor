//! Texture loss and retouch artefacts. PHASE-27 section 2.1, phases 19, 20 and 21.
//!
//! ## A withdrawn retouch is not a defect, and this is where that could most easily go wrong
//!
//! Phase 20 withdraws a whole plan rather than shipping one that missed its texture floor. A frame
//! carrying `texture_withdrawn = true` therefore has **no retouch on it at all** - which is the
//! product working, and which a naive reading of the band ratio would report as a failure, because
//! the withdrawn plan's stored ratio is the one that missed.
//!
//! So a withdrawn plan is `Clean` here. There is nothing to find: the delivered photograph is the
//! unretouched one.
//!
//! ## The allowance is phase 19's and this check does not get its own
//!
//! Phase 19 wrote the rule that six individually defensible adjustments are how a gallery quietly
//! starts looking processed, and gave the whole product one per-image perceptual allowance. Phase 20
//! inherited it and phase 21 inherited it again. This check reads the *share that was spent* and
//! files a ticket when it went over, which is the only way an over-spend becomes visible to anybody
//! - the allowance is enforced upstream, and a frame past it means an enforcement path did not run.
//!
//! ## Naturalness is three measurements over three regions, and they never merge
//!
//! Phase 21 withdraws hair, teeth and eyes independently, because the three are measured over
//! disjoint regions and a frame whose teeth could not be evened safely should still get its lint
//! removed. This check keeps them separate for the same reason: a single "naturalness" number would
//! throw away which of the three a photographer is looking at.

use aura_core::contract::qc::{QcCategory, QcCode};

use super::{Finding, Frame, Outcome};
use crate::policy::Thresholds;

/// How much of a texture shortfall a strength reduction is predicted to remove.
///
/// Phase 20's own guard re-solves at three quarters strength up to three times before withdrawing,
/// so a frame that reached QC below its floor is one that guard did not run on. A further reduction
/// is therefore effective but not certain, because the mark being smoothed may be the texture.
const TEXTURE_GAIN_SHARE: f32 = 0.70;

/// How much of a naturalness excursion a strength reduction is predicted to remove.
const NATURALNESS_GAIN_SHARE: f32 = 0.80;

/// How much of an allowance over-spend a strength reduction is predicted to remove.
///
/// Nearly all of it: the allowance is a sum of strengths, so reducing a strength reduces it almost
/// exactly. `LocalOp::PRIORITY` decides which operation gives way, which is phase 19's decision and
/// not this phase's.
const ALLOWANCE_GAIN_SHARE: f32 = 0.90;

/// Inspect one frame's retouching.
#[must_use]
pub fn inspect(frame: &Frame, thresholds: &Thresholds) -> Outcome {
    let Some(retouch) = frame.retouch.as_ref() else {
        return Outcome::Skipped("no retouch readings for this frame");
    };
    let row = thresholds.scene(frame.scene);
    let mut findings = Vec::new();

    if retouch.texture_withdrawn {
        // Phase 20 refused to ship a plan that missed its floor, so the delivered photograph is
        // unretouched. There is nothing here to find, and the stored ratio belongs to a plan that
        // never reached a pixel.
        return Outcome::Clean;
    }

    if retouch.texture_measured {
        let miss = (retouch.texture_floor - retouch.texture_band_ratio).max(0.0);
        if miss > row.texture_floor_miss {
            findings.push(Finding::new(
                QcCategory::Retouch,
                QcCode::TextureFloorMissed,
                miss,
                row.texture_floor_miss,
                miss * TEXTURE_GAIN_SHARE,
                // Measured through the real renderer by phase 20's guard, so this is a fact about
                // the delivered pixels rather than an inference from a strength.
                margin_confidence(miss, row.texture_floor_miss).max(0.75),
            ));
        }
    }

    // Three regions, three findings, never one number. Phase 21's rule.
    let excursions = [
        ("catchlight", (retouch.catchlight_ratio - 1.0).abs()),
        ("hairline", (retouch.hair_energy_ratio - 1.0).abs()),
        ("teeth", retouch.teeth_excursion.abs()),
    ];
    for (_region, excursion) in excursions {
        if excursion.is_finite() && excursion > row.naturalness_excursion {
            findings.push(Finding::new(
                QcCategory::Retouch,
                QcCode::NaturalnessMissed,
                excursion,
                row.naturalness_excursion,
                (excursion - row.naturalness_excursion) * NATURALNESS_GAIN_SHARE,
                margin_confidence(excursion, row.naturalness_excursion),
            ));
        }
    }

    // The allowance is a budget phase 19 already enforces, so anything over one is an enforcement
    // path that did not run rather than a preference somebody expressed.
    let overspend = (retouch.allowance_used - 1.0).max(0.0);
    if overspend > row.allowance_overspend {
        findings.push(Finding::new(
            QcCategory::Retouch,
            QcCode::AllowanceExceeded,
            overspend,
            row.allowance_overspend,
            overspend * ALLOWANCE_GAIN_SHARE,
            margin_confidence(overspend, row.allowance_overspend).max(0.70),
        ));
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
    use crate::checks::RetouchReading;
    use aura_core::contract::qc::ImageId;
    use aura_core::contract::scene::SceneId;

    fn reading() -> RetouchReading {
        RetouchReading {
            texture_band_ratio: 0.90,
            texture_floor: 0.70,
            texture_withdrawn: false,
            texture_measured: true,
            catchlight_ratio: 1.0,
            hair_energy_ratio: 1.0,
            teeth_excursion: 0.0,
            allowance_used: 0.6,
        }
    }

    fn frame_with(retouch: RetouchReading) -> Frame {
        Frame {
            retouch: Some(retouch),
            ..Frame::empty(ImageId::new(), SceneId::CouplePortrait)
        }
    }

    #[test]
    fn a_well_retouched_frame_is_clean() {
        assert_eq!(
            inspect(&frame_with(reading()), &Thresholds::reference()),
            Outcome::Clean
        );
    }

    #[test]
    fn a_withdrawn_retouch_is_clean_rather_than_a_finding() {
        let mut retouch = reading();
        // The stored ratio belongs to the plan that missed - and that plan never reached a pixel.
        retouch.texture_withdrawn = true;
        retouch.texture_band_ratio = 0.01;
        retouch.texture_floor = 0.90;
        let outcome = inspect(&frame_with(retouch), &Thresholds::reference());
        assert_eq!(
            outcome,
            Outcome::Clean,
            "phase 20 shipped the unretouched frame; there is nothing here to find"
        );
    }

    #[test]
    fn a_texture_floor_miss_is_measured_against_the_floor_the_plan_was_held_to() {
        let mut retouch = reading();
        retouch.texture_floor = 0.90;
        retouch.texture_band_ratio = 0.50;
        let findings = inspect(&frame_with(retouch), &Thresholds::reference()).findings();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, QcCode::TextureFloorMissed);
        assert!((findings[0].deviation - 0.40).abs() < 1e-5);
    }

    #[test]
    fn an_unmeasured_texture_report_produces_no_texture_finding() {
        let mut retouch = reading();
        retouch.texture_measured = false;
        retouch.texture_band_ratio = 0.0;
        retouch.texture_floor = 0.9;
        // Not a skip for the whole check - the naturalness and allowance halves still ran - but the
        // texture half makes no claim, because an unmeasured report carries a zero that reads as a
        // total loss.
        let outcome = inspect(&frame_with(retouch), &Thresholds::reference());
        assert_eq!(outcome, Outcome::Clean);
    }

    #[test]
    fn three_regions_produce_three_findings_and_never_one_average() {
        let mut retouch = reading();
        retouch.catchlight_ratio = 2.0;
        retouch.hair_energy_ratio = 0.2;
        retouch.teeth_excursion = 0.9;
        let findings = inspect(&frame_with(retouch), &Thresholds::reference()).findings();
        let natural = findings
            .iter()
            .filter(|f| f.code == QcCode::NaturalnessMissed)
            .count();
        assert_eq!(
            natural, 3,
            "hair, teeth and eyes are withdrawn independently"
        );
    }

    #[test]
    fn an_allowance_overspend_is_measured_past_one_rather_than_from_zero() {
        let mut retouch = reading();
        retouch.allowance_used = 1.5;
        let findings = inspect(&frame_with(retouch), &Thresholds::reference()).findings();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, QcCode::AllowanceExceeded);
        assert!((findings[0].deviation - 0.5).abs() < 1e-5);
    }

    #[test]
    fn a_frame_that_used_its_whole_allowance_is_not_a_finding() {
        let mut retouch = reading();
        retouch.allowance_used = 1.0;
        assert_eq!(
            inspect(&frame_with(retouch), &Thresholds::reference()),
            Outcome::Clean
        );
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
