//! Exposure and clipping after the edit. PHASE-27 section 2.1, phases 09, 14, 15 and 16.
//!
//! ## The clipping question is asked as a difference, and that is the whole check
//!
//! A wedding photograph with blown highlights is often a photograph of a window, and a check that
//! reported absolute clipping would file a ticket on every frame shot toward one. What is a defect
//! is clipping the **edit introduced**: the original held detail and the delivered frame does not.
//!
//! Both fractions are on the reading for that reason - `clip_hi_before` and `clip_hi_after` - and
//! the deviation is the difference. This is the same lesson phase 22 recorded for its ringing
//! measurement and phase 19 for its halo test, and it is the third place in the product where
//! comparing an absolute against a threshold would have scored the *subject* rather than the
//! damage.
//!
//! ## Shadows are checked against headroom rather than against a level
//!
//! Phase 09 measures how much shadow room a frame had. A reception at ISO 12800 has very little,
//! and a delivered frame that used all of it is not making a mistake - it is working with what the
//! sensor gave it. So the finding is a *deficit* against the scene's own tolerance rather than a
//! black-point reading, and a scene row naming a larger `shadow_deficit` is asking for more
//! headroom to be preserved.

use aura_core::contract::qc::{QcCategory, QcCode};

use super::{Finding, Frame, Outcome};
use crate::policy::Thresholds;

/// Roughly how many stops separate the darkest and brightest subject luminance this product works
/// in.
///
/// Used only to turn a `0..1` luminance difference into the EV the ticket is stated in, so a
/// photographer reading "0.7 EV" is reading something they can act on rather than a normalised
/// fraction nobody has intuition for. Five stops is the working range phase 15's exposure targets
/// are expressed over.
const LUMA_RANGE_EV: f32 = 5.0;

/// How much of an exposure error a re-solve is predicted to remove.
///
/// High, because phase 15's exposure solve is nearly direct: it moves the subject onto the scene's
/// face-luminance band and the only thing that stops it is the clipping bound. A frame outside its
/// band is usually a frame that bound held back, and re-solving under a constraint that permits a
/// little clipping recovers most of it.
const EXPOSURE_GAIN_SHARE: f32 = 0.85;

/// How much of an introduced clipping a re-solve is predicted to remove.
const CLIPPING_GAIN_SHARE: f32 = 0.70;

/// Inspect one frame's exposure.
#[must_use]
pub fn inspect(frame: &Frame, thresholds: &Thresholds) -> Outcome {
    let Some(exposure) = frame.exposure.as_ref() else {
        return Outcome::Skipped("no exposure readings for this frame");
    };
    let row = thresholds.scene(frame.scene);
    let mut findings = Vec::new();

    // `None` skips this half rather than passing it: an unmeasured landing luminance defaulted to
    // the target reads as a frame that hit its band exactly, on every frame nobody could measure.
    let error_ev = exposure.subject_luma.map_or(0.0, |luma| {
        (luma - exposure.target_luma).abs() * LUMA_RANGE_EV
    });
    if exposure.subject_luma.is_some() && error_ev.is_finite() && error_ev > row.exposure_ev {
        let gain = (error_ev - row.exposure_ev) * EXPOSURE_GAIN_SHARE;
        findings.push(Finding::new(
            QcCategory::Exposure,
            QcCode::ExposureRegression,
            error_ev,
            row.exposure_ev,
            gain,
            margin_confidence(error_ev, row.exposure_ev),
        ));
    }

    // The difference, never the absolute. A frame of a window is not a defect; a frame the edit
    // blew out is.
    let added_hi = (exposure.clip_hi_after - exposure.clip_hi_before).max(0.0);
    let added_lo = (exposure.clip_lo_after - exposure.clip_lo_before).max(0.0);
    let added = added_hi.max(added_lo);
    if added.is_finite() && added > row.clipping_added {
        let gain = (added - row.clipping_added) * CLIPPING_GAIN_SHARE;
        findings.push(Finding::new(
            QcCategory::Exposure,
            QcCode::ClippingIntroduced,
            added,
            row.clipping_added,
            gain,
            // Confident: this is a difference between two measured histograms rather than an
            // inference. What it cannot know is whether the photographer wanted it, which is what
            // the autonomy band and the review queue are for.
            margin_confidence(added, row.clipping_added).max(0.70),
        ));
    }

    // Headroom is a `0..1` reading where lower is less room, so the deficit is `1 - headroom` and
    // the threshold is how much deficit the scene tolerates. Absent skips, for the reason above.
    let deficit = exposure
        .shadow_headroom
        .map_or(0.0, |room| (1.0 - room).clamp(0.0, 1.0));
    if exposure.shadow_headroom.is_some() && deficit > row.shadow_deficit {
        let gain = (deficit - row.shadow_deficit) * CLIPPING_GAIN_SHARE;
        findings.push(Finding::new(
            QcCategory::Exposure,
            QcCode::ShadowsCrushed,
            deficit,
            row.shadow_deficit,
            gain,
            margin_confidence(deficit, row.shadow_deficit),
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
    use crate::checks::ExposureReading;
    use aura_core::contract::qc::ImageId;
    use aura_core::contract::scene::SceneId;

    fn reading() -> ExposureReading {
        ExposureReading {
            subject_luma: Some(0.45),
            target_luma: 0.45,
            clip_hi_after: 0.02,
            clip_lo_after: 0.00,
            clip_hi_before: 0.02,
            clip_lo_before: 0.00,
            shadow_headroom: Some(0.90),
        }
    }

    fn frame_with(exposure: ExposureReading) -> Frame {
        Frame {
            exposure: Some(exposure),
            ..Frame::empty(ImageId::new(), SceneId::Ceremony)
        }
    }

    #[test]
    fn a_well_exposed_frame_is_clean() {
        assert_eq!(
            inspect(&frame_with(reading()), &Thresholds::reference()),
            Outcome::Clean
        );
    }

    #[test]
    fn a_photograph_of_a_window_is_not_a_defect() {
        let mut exposure = reading();
        // Forty per cent of the frame blown, and the original was blown by exactly as much. The
        // edit did nothing wrong; the room had a window in it.
        exposure.clip_hi_before = 0.40;
        exposure.clip_hi_after = 0.40;
        let outcome = inspect(&frame_with(exposure), &Thresholds::reference());
        assert_eq!(outcome, Outcome::Clean);
    }

    #[test]
    fn clipping_the_edit_introduced_is_a_defect() {
        let mut exposure = reading();
        exposure.clip_hi_before = 0.40;
        exposure.clip_hi_after = 0.55;
        let findings = inspect(&frame_with(exposure), &Thresholds::reference()).findings();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, QcCode::ClippingIntroduced);
        // The number is the fifteen points the edit added, never the fifty-five it ended at.
        assert!((findings[0].deviation - 0.15).abs() < 1e-5);
    }

    #[test]
    fn an_edit_that_recovered_clipping_is_never_a_finding() {
        let mut exposure = reading();
        exposure.clip_hi_before = 0.40;
        exposure.clip_hi_after = 0.05;
        assert_eq!(
            inspect(&frame_with(exposure), &Thresholds::reference()),
            Outcome::Clean
        );
    }

    #[test]
    fn an_exposure_error_is_reported_in_ev_rather_than_in_a_normalised_fraction() {
        let mut exposure = reading();
        exposure.subject_luma = Some(0.45 + 0.30);
        let findings = inspect(&frame_with(exposure), &Thresholds::reference()).findings();
        assert_eq!(findings[0].code, QcCode::ExposureRegression);
        assert!(
            (findings[0].deviation - 1.5).abs() < 1e-4,
            "0.30 of the working range is 1.5 EV"
        );
    }

    #[test]
    fn a_dark_reception_that_used_its_headroom_is_judged_on_the_deficit() {
        let mut exposure = reading();
        exposure.shadow_headroom = Some(0.50);
        let findings = inspect(&frame_with(exposure), &Thresholds::reference()).findings();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, QcCode::ShadowsCrushed);
        assert!((findings[0].deviation - 0.50).abs() < 1e-5);
    }

    #[test]
    fn an_absent_reading_skips() {
        let frame = Frame::empty(ImageId::new(), SceneId::Ceremony);
        assert!(matches!(
            inspect(&frame, &Thresholds::reference()),
            Outcome::Skipped(_)
        ));
    }
}
