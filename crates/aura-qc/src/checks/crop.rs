//! Crop safety on the delivered frame. PHASE-27 section 2.1, phase 23.
//!
//! ## This check exists because phase 23's filter can only refuse candidates it saw
//!
//! Phase 23 filters candidate rectangles for safety *before* the composition objective ever scores
//! one, which is the right ordering and the one phase 12 and phase 24 also use. What that ordering
//! cannot cover is the case where the safety report itself could not be computed - a frame whose
//! faces phase 06 never found has `faces_checked = 0`, and a crop filter with nothing to protect
//! passes every rectangle.
//!
//! So `faces_intact = true` with `faces_checked = 0` is **not** a frame whose faces survived. It is
//! a frame where nobody looked, and it is the single most common shape in this build, because phase
//! 06's detector finds no faces at all. Reading the boolean without the count is the mistake this
//! check is written to avoid.
//!
//! ## The gain is zero on every finding here, and that is deliberate
//!
//! There is no smaller amount of an unsafe crop. A rectangle that cuts a hand is not improved by
//! being nudged - phase 23 wrote that rule as "any later phase that finds itself adjusting a
//! rejected rectangle has misunderstood the ordering", and it applies to this phase most of all.
//!
//! The remedy is `SolveTarget::Crop`, which re-runs phase 23's own search under a constraint. That
//! either produces a safe rectangle or it does not, so the loop measures a re-solve against a
//! prediction of zero and the round is judged on whether the deviation actually reached zero.

use aura_core::contract::qc::{QcCategory, QcCode};

use super::{Finding, Frame, Outcome};
use crate::policy::Thresholds;

/// The threshold a safety violation is measured against.
///
/// A violation is a boolean upstream, so the deviation is 1.0 and the threshold is anything below
/// it. Half, so the severity ratio comes out at 2.0 - which puts an unsafe crop above a colour drift
/// that is 1.5 tolerances out, which is the ordering a photographer wants.
const VIOLATION_THRESHOLD: f32 = 0.5;

/// Inspect one frame's delivered crop.
#[must_use]
pub fn inspect(frame: &Frame, thresholds: &Thresholds) -> Outcome {
    let Some(crop) = frame.crop.as_ref() else {
        return Outcome::Skipped("no geometry plan for this frame");
    };
    let row = thresholds.scene(frame.scene);
    let mut findings = Vec::new();

    if crop.faces_checked == 0 {
        // A filter with nothing to protect passed every rectangle. That is not a safe crop; it is
        // an unchecked one, and in this build it is the common case rather than the exotic one.
        if !crop.resolution_ok || shortfall(crop, row.crop_shortfall).is_some() {
            // The resolution half does not depend on faces, so it still reports.
        } else {
            return Outcome::Skipped("the crop safety report checked no faces");
        }
    } else if !crop.faces_intact {
        findings.push(
            Finding::new(
                QcCategory::Crop,
                QcCode::CropUnsafe,
                1.0,
                VIOLATION_THRESHOLD,
                // There is no smaller amount of an unsafe crop. Phase 23's rule.
                0.0,
                0.95,
            )
            .because(QcCode::EscalatedToHuman, 0.5),
        );
    }

    if let Some(miss) = shortfall(crop, row.crop_shortfall) {
        findings.push(Finding::new(
            QcCategory::Crop,
            QcCode::CropResolutionLow,
            miss,
            row.crop_shortfall,
            0.0,
            margin_confidence(miss, row.crop_shortfall).max(0.80),
        ));
    } else if !crop.resolution_ok {
        // The report says the resolution failed and the long-edge fraction does not show it, which
        // means the floor came from a purpose this check does not know about. Report it at the
        // report's own word rather than second-guessing phase 23's own rule table.
        findings.push(Finding::new(
            QcCategory::Crop,
            QcCode::CropResolutionLow,
            1.0,
            VIOLATION_THRESHOLD,
            0.0,
            0.75,
        ));
    }

    if !crop.content_kept {
        findings.push(Finding::new(
            QcCategory::Crop,
            QcCode::CropContentLost,
            1.0,
            VIOLATION_THRESHOLD,
            0.0,
            0.80,
        ));
    }

    Outcome::from_findings(findings)
}

/// How far the delivered crop falls below its own floor, when it does.
///
/// `None` when there is no floor to fall below - a frame whose purpose sets no minimum - which is
/// different from a frame that met one.
fn shortfall(crop: &super::CropReading, tolerance: f32) -> Option<f32> {
    if !crop.long_edge_fraction.is_finite() || !crop.long_edge_floor.is_finite() {
        return None;
    }
    if crop.long_edge_floor <= 0.0 {
        return None;
    }
    let miss = crop.long_edge_floor - crop.long_edge_fraction;
    if miss > tolerance {
        Some(miss)
    } else {
        None
    }
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
    use crate::checks::CropReading;
    use aura_core::contract::qc::ImageId;
    use aura_core::contract::scene::SceneId;

    fn reading() -> CropReading {
        CropReading {
            faces_intact: true,
            resolution_ok: true,
            content_kept: true,
            long_edge_fraction: 0.90,
            long_edge_floor: 0.50,
            faces_checked: 3,
        }
    }

    fn frame_with(crop: CropReading) -> Frame {
        Frame {
            crop: Some(crop),
            ..Frame::empty(ImageId::new(), SceneId::CouplePortrait)
        }
    }

    #[test]
    fn a_safe_crop_is_clean() {
        assert_eq!(
            inspect(&frame_with(reading()), &Thresholds::reference()),
            Outcome::Clean
        );
    }

    #[test]
    fn faces_intact_with_nothing_checked_is_a_skip_and_not_a_pass() {
        let mut crop = reading();
        crop.faces_checked = 0;
        // The boolean says the faces survived. Nobody looked at any.
        assert!(crop.faces_intact);
        let outcome = inspect(&frame_with(crop), &Thresholds::reference());
        assert!(matches!(outcome, Outcome::Skipped(_)));
        assert_ne!(outcome, Outcome::Clean);
    }

    #[test]
    fn the_resolution_half_still_reports_when_no_face_was_checked() {
        let mut crop = reading();
        crop.faces_checked = 0;
        crop.long_edge_fraction = 0.10;
        let findings = inspect(&frame_with(crop), &Thresholds::reference()).findings();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, QcCode::CropResolutionLow);
    }

    #[test]
    fn a_crop_that_cuts_a_face_predicts_no_gain() {
        let mut crop = reading();
        crop.faces_intact = false;
        let findings = inspect(&frame_with(crop), &Thresholds::reference()).findings();
        assert_eq!(findings[0].code, QcCode::CropUnsafe);
        // There is no smaller amount of an unsafe crop. Phase 23's rule, and the reason a re-solve
        // is judged on whether the deviation actually reached zero rather than on a share.
        assert_eq!(findings[0].expected_gain, 0.0);
    }

    #[test]
    fn an_unsafe_crop_outranks_a_colour_drift_in_the_queue() {
        let mut crop = reading();
        crop.faces_intact = false;
        let unsafe_crop = inspect(&frame_with(crop), &Thresholds::reference()).findings();
        assert!(unsafe_crop[0].severity() >= 2.0);
    }

    #[test]
    fn a_shortfall_is_measured_against_the_purposes_own_floor() {
        let mut crop = reading();
        crop.long_edge_floor = 0.80;
        crop.long_edge_fraction = 0.30;
        let findings = inspect(&frame_with(crop), &Thresholds::reference()).findings();
        assert_eq!(findings[0].code, QcCode::CropResolutionLow);
        assert!((findings[0].deviation - 0.50).abs() < 1e-5);
    }

    #[test]
    fn a_frame_with_no_floor_is_not_measured_against_one() {
        let mut crop = reading();
        crop.long_edge_floor = 0.0;
        crop.long_edge_fraction = 0.01;
        assert_eq!(
            inspect(&frame_with(crop), &Thresholds::reference()),
            Outcome::Clean
        );
    }

    #[test]
    fn lost_content_is_its_own_code() {
        let mut crop = reading();
        crop.content_kept = false;
        let findings = inspect(&frame_with(crop), &Thresholds::reference()).findings();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, QcCode::CropContentLost);
    }

    #[test]
    fn an_absent_plan_skips() {
        let frame = Frame::empty(ImageId::new(), SceneId::CouplePortrait);
        assert!(matches!(
            inspect(&frame, &Thresholds::reference()),
            Outcome::Skipped(_)
        ));
    }
}
