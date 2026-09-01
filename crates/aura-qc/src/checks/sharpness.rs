//! Subject sharpness after restoration. PHASE-27 section 2.1, phases 09 and 22.
//!
//! ## Four findings, and only one of them is about softness
//!
//! A soft subject is the obvious one. The other three are damage the *repair* did:
//! sharpening that left a halo, denoising that took texture, and face recovery that moved somebody
//! away from their own face. All four are read out of numbers phase 22 already measured through the
//! real renderer, which is what makes them findings rather than guesses.
//!
//! ## Softness is measured relative to the background, and that is not a refinement
//!
//! A frame can be uniformly soft because the whole photograph is soft, or soft on the subject
//! because the focus missed. The second is a defect and the first is often a lens wide open in a
//! dark room, which is what a photographer chose.
//!
//! `relative_sharpness` is phase 09's subject-against-background reading and it is the one that
//! decides here; `subject_sharpness` alone would file a ticket on every frame of a candlelit
//! reception. Phase 09 introduced `subject_aware` as a coverage number for exactly this reason and
//! this check is the consumer it was introduced for.
//!
//! ## A self-check that did not run is a skip
//!
//! `selfcheck_measured_on` is zero when phase 22's artefact report never ran, and the report then
//! carries a texture retention of zero and a ringing of zero. Those are the *best possible* and the
//! *worst possible* readings respectively, so trusting them unread would produce a texture-loss
//! ticket on every frame and no ringing ticket ever. Both halves skip together.

use aura_core::contract::qc::{QcCategory, QcCode};

use super::{Finding, Frame, Outcome};
use crate::policy::{Thresholds, SHARPNESS_REFERENCE_FLOOR};

/// How much of a softness shortfall a restoration re-solve is predicted to remove.
///
/// Low, and it is the most pessimistic gain in the phase. Sharpening cannot recover detail the
/// sensor did not record: phase 22 refuses deconvolution unless four preconditions hold, and a
/// frame that is soft because focus missed is soft permanently. A quarter is what the fixture corpus
/// realises on frames where sharpening was *available and held back*, and the loop's revert is what
/// catches the rest.
const SHARPEN_GAIN_SHARE: f32 = 0.25;

/// How much of a ringing excursion a strength reduction is predicted to remove.
///
/// High and nearly linear: ringing is caused by the sharpening amount, so reducing the amount
/// reduces it.
const RINGING_GAIN_SHARE: f32 = 0.85;

/// How much of a texture loss a denoise tier reduction is predicted to remove.
const TEXTURE_GAIN_SHARE: f32 = 0.75;

/// Inspect one frame's detail.
#[must_use]
pub fn inspect(frame: &Frame, thresholds: &Thresholds) -> Outcome {
    let Some(sharp) = frame.sharpness.as_ref() else {
        return Outcome::Skipped("no sharpness readings for this frame");
    };
    let row = thresholds.scene(frame.scene);
    let mut findings = Vec::new();

    // Relative rather than absolute. A candlelit reception is soft everywhere and that is a choice;
    // a subject softer than its own background is a focus miss.
    //
    // The floor is code-owned and the *slack* is what a scene tunes, so that a larger number in the
    // table means a more permissive check - the same direction as the other eighteen rows. See
    // `policy::SHARPNESS_REFERENCE_FLOOR`.
    let shortfall = (SHARPNESS_REFERENCE_FLOOR - sharp.relative_sharpness).max(0.0);
    if shortfall > row.sharpness_slack && sharp.relative_sharpness.is_finite() {
        findings.push(
            Finding::new(
                QcCategory::Sharpness,
                QcCode::SharpnessBelowFloor,
                shortfall,
                row.sharpness_slack,
                (shortfall - row.sharpness_slack) * SHARPEN_GAIN_SHARE,
                shortfall_confidence(
                    shortfall,
                    SHARPNESS_REFERENCE_FLOOR,
                    sharp.subject_sharpness,
                ),
            )
            .because(QcCode::EscalatedToHuman, 0.3),
        );
    }

    if sharp.selfcheck_measured_on == 0 {
        // Both restoration halves skip together: zero retention is the worst possible reading and
        // zero ringing is the best, so trusting an unrun report would file a texture ticket on
        // every frame and a ringing ticket on none.
        return if findings.is_empty() {
            Outcome::Skipped("phase 22's artefact self-check did not run on this frame")
        } else {
            Outcome::Found(findings)
        };
    }

    if sharp.ringing.is_finite() && sharp.ringing > row.ringing {
        let gain = (sharp.ringing - row.ringing) * RINGING_GAIN_SHARE;
        findings.push(Finding::new(
            QcCategory::Sharpness,
            QcCode::RingingDetected,
            sharp.ringing,
            row.ringing,
            gain,
            margin_confidence(sharp.ringing, row.ringing).max(0.70),
        ));
    }

    let lost = (1.0 - sharp.texture_retention).clamp(0.0, 1.0);
    if lost > row.texture_loss {
        let gain = (lost - row.texture_loss) * TEXTURE_GAIN_SHARE;
        findings.push(Finding::new(
            QcCategory::Sharpness,
            QcCode::TextureLost,
            lost,
            row.texture_loss,
            gain,
            margin_confidence(lost, row.texture_loss),
        ));
    }

    if sharp.identity_drift.is_finite() && sharp.identity_drift > row.identity_drift {
        findings.push(
            Finding::new(
                QcCategory::Sharpness,
                QcCode::IdentityDrift,
                sharp.identity_drift,
                row.identity_drift,
                // Zero predicted gain, deliberately. Phase 22 holds every recovered face to this
                // ceiling through the real renderer, so a frame past it means that guard did not
                // run rather than that it failed - and the remedy for a guard that did not run is
                // not a smaller amount of the same operation. It is a person.
                0.0,
                0.90,
            )
            .because(QcCode::EscalatedToHuman, 1.0),
        );
    }

    Outcome::from_findings(findings)
}

/// Confidence in a softness finding.
///
/// Two terms multiplied. How far under the floor the relative reading is, and whether the *absolute*
/// reading agrees - a subject that is soft relative to its background but sharp in absolute terms is
/// a shallow depth of field working correctly, and a frame like that is a finding worth showing and
/// not one worth acting on.
fn shortfall_confidence(shortfall: f32, floor: f32, absolute: f32) -> f32 {
    if floor <= 0.0 {
        return 0.0;
    }
    let by_margin = (shortfall / floor).clamp(0.0, 1.0);
    let by_absolute = if absolute.is_finite() {
        // Falls away as the absolute sharpness rises: at or above the floor in absolute terms, this
        // is almost certainly bokeh rather than a miss.
        (1.0 - (absolute / floor).clamp(0.0, 1.0)).max(0.15)
    } else {
        0.5
    };
    (0.40 + 0.55 * by_margin * by_absolute).clamp(0.0, 1.0)
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
    use crate::checks::SharpnessReading;
    use aura_core::contract::qc::ImageId;
    use aura_core::contract::scene::SceneId;

    fn reading() -> SharpnessReading {
        SharpnessReading {
            subject_sharpness: 0.70,
            relative_sharpness: 0.70,
            texture_retention: 0.95,
            ringing: 0.01,
            identity_drift: 0.0,
            selfcheck_measured_on: 4,
        }
    }

    fn frame_with(sharpness: SharpnessReading) -> Frame {
        Frame {
            sharpness: Some(sharpness),
            ..Frame::empty(ImageId::new(), SceneId::CouplePortrait)
        }
    }

    #[test]
    fn a_sharp_frame_is_clean() {
        assert_eq!(
            inspect(&frame_with(reading()), &Thresholds::reference()),
            Outcome::Clean
        );
    }

    #[test]
    fn a_uniformly_soft_frame_scores_lower_confidence_than_a_focus_miss() {
        // Both are soft relative to the background by the same amount. The first is also soft in
        // absolute terms, which is a dark room and a wide lens; the second is sharp in absolute
        // terms, which is shallow depth of field. Only the relative reading fires, and the
        // absolute one decides how much to believe it.
        let mut miss = reading();
        miss.relative_sharpness = 0.10;
        miss.subject_sharpness = 0.10;
        let mut bokeh = reading();
        bokeh.relative_sharpness = 0.10;
        bokeh.subject_sharpness = 0.90;
        let a = inspect(&frame_with(miss), &Thresholds::reference()).findings();
        let b = inspect(&frame_with(bokeh), &Thresholds::reference()).findings();
        assert_eq!(a[0].code, QcCode::SharpnessBelowFloor);
        assert_eq!(a[0].deviation, b[0].deviation);
        assert!(a[0].confidence > b[0].confidence);
    }

    #[test]
    fn softness_has_the_most_pessimistic_gain_in_the_phase() {
        let mut soft = reading();
        soft.relative_sharpness = 0.05;
        let findings = inspect(&frame_with(soft), &Thresholds::reference()).findings();
        // Sharpening cannot recover detail the sensor never recorded, so the ticket predicts little
        // and escalates rather than burning both rounds on it.
        assert!(findings[0].expected_gain < findings[0].deviation * 0.3);
    }

    #[test]
    fn a_selfcheck_that_did_not_run_skips_both_restoration_halves() {
        let mut sharp = reading();
        sharp.selfcheck_measured_on = 0;
        // The zeroes an unrun report carries: worst possible retention, best possible ringing.
        sharp.texture_retention = 0.0;
        sharp.ringing = 0.0;
        let outcome = inspect(&frame_with(sharp), &Thresholds::reference());
        assert!(matches!(outcome, Outcome::Skipped(_)));
    }

    #[test]
    fn a_softness_finding_survives_a_selfcheck_that_did_not_run() {
        let mut sharp = reading();
        sharp.selfcheck_measured_on = 0;
        sharp.relative_sharpness = 0.05;
        let outcome = inspect(&frame_with(sharp), &Thresholds::reference());
        // Half the check ran and half did not; the half that ran still reports.
        let findings = outcome.findings();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, QcCode::SharpnessBelowFloor);
    }

    #[test]
    fn ringing_and_texture_loss_are_separate_findings() {
        let mut sharp = reading();
        sharp.ringing = 0.9;
        sharp.texture_retention = 0.1;
        let findings = inspect(&frame_with(sharp), &Thresholds::reference()).findings();
        let codes: Vec<_> = findings.iter().map(|f| f.code).collect();
        assert!(codes.contains(&QcCode::RingingDetected));
        assert!(codes.contains(&QcCode::TextureLost));
    }

    #[test]
    fn an_identity_drift_predicts_no_gain_and_goes_to_a_person() {
        let mut sharp = reading();
        sharp.identity_drift = 0.5;
        let findings = inspect(&frame_with(sharp), &Thresholds::reference()).findings();
        let drift = findings
            .iter()
            .find(|f| f.code == QcCode::IdentityDrift)
            .expect("an identity drift is a finding");
        // Phase 22 holds every recovered face to this ceiling, so past it means the guard did not
        // run. The remedy for a guard that did not run is not a smaller dose of the same operation.
        assert_eq!(drift.expected_gain, 0.0);
        assert!(drift
            .extra_reasons
            .iter()
            .any(|(code, _)| *code == QcCode::EscalatedToHuman));
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
