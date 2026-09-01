//! A person's skin against their own gallery target. PHASE-27 section 2.1, phases 15, 16 and 25.
//!
//! ## Two independent questions in one category
//!
//! **Did this person's skin end up looking like it does elsewhere in the gallery?** Phase 25 builds
//! a per-identity target from that person's own well-lit frames, and a delivered frame that sits
//! far from it is a frame where somebody's face does not match their face.
//!
//! **Did the grade move skin further than phase 16 permits?** That is a different failure with a
//! different remedy: the first is fixed by re-solving the normalisation, the second by reducing the
//! grade. They are separate codes for the same reason phase 18 keeps confidence and edge quality
//! separate - they fail independently and are fixed by different things.
//!
//! ## An unmeasured guard is a skip, and the reason is that zero looks like a perfect result
//!
//! `SkinGuardReport::measured` is false when phase 16 could not find skin pixels to grade through
//! the renderer, and the report then carries zeroes. Zero hue shift and zero chroma change is
//! exactly what a perfect grade produces, so a build that read the report without checking
//! `measured` would report every frame with no detected face as the best-behaved frame in the
//! wedding. In this build, where phase 06's detector finds no faces, that is *every* frame.
//!
//! There is no ideal-skin constant anywhere in this module, in the thresholds table, or in
//! migration 27. Phase 15 wrote that rule and the phase gate scans the schema for one on every run:
//! every number here is a distance from **this person's own** measured target.

use aura_core::contract::qc::{Evidence, QcCategory, QcCode};

use super::{Finding, Frame, Outcome};
use crate::policy::Thresholds;

/// How much of a skin drift a re-solve of the normalisation is predicted to remove.
///
/// Phase 25's skin correction is capped, and the cap falls with the mood of the room - so a frame
/// far from its person's target in a dark reception is a frame the cap held back rather than one
/// the solver got wrong. Two thirds is the share the fixture corpus realises.
const SKIN_GAIN_SHARE: f32 = 0.66;

/// How much of a guard excursion a strength reduction is predicted to remove.
///
/// Higher than the drift share because this remedy is direct: the excursion is caused by the grade,
/// and reducing the grade reduces it nearly linearly. Phase 16 re-solves rather than attenuating
/// for exactly the reason this is not 1.0 - a curve moves chroma without touching a band, so the
/// relationship is not quite proportional.
const GUARD_GAIN_SHARE: f32 = 0.80;

/// Inspect one frame's skin.
#[must_use]
pub fn inspect(frame: &Frame, thresholds: &Thresholds) -> Outcome {
    let Some(skin) = frame.skin.as_ref() else {
        return Outcome::Skipped("no skin readings for this frame");
    };
    let row = thresholds.scene(frame.scene);
    let mut findings = Vec::new();

    // One finding per person, not one per frame. A family formal where one person's skin drifted
    // and three did not is a ticket about one person, and a per-frame maximum would lose which.
    for (identity, de00) in &skin.per_identity_de00 {
        if !de00.is_finite() || *de00 <= row.skin_de00 {
            continue;
        }
        let gain = (de00 - row.skin_de00) * SKIN_GAIN_SHARE;
        let anchors = frame
            .node
            .as_ref()
            .map(|node| node.anchors.clone())
            .unwrap_or_default();
        let mut finding = Finding::new(
            QcCategory::Skin,
            QcCode::SkinDrift,
            *de00,
            row.skin_de00,
            gain,
            margin_confidence(*de00, row.skin_de00),
        )
        .about(*identity);
        if !anchors.is_empty() {
            finding = finding.with_evidence(Evidence::Anchors(anchors));
        }
        findings.push(finding);
    }

    // The guard half. An unmeasured guard carries zeroes, and zeroes are what a perfect grade
    // produces - so this half skips rather than passing when nobody could measure it.
    if skin.guard_measured {
        let hue = skin.guard_hue_shift_deg.abs();
        let chroma = skin.guard_chroma_change.abs();
        // Each in units of its own threshold, then the worse of the two, so one number in degrees
        // and one dimensionless can share a severity ordering.
        let hue_ratio = ratio(hue, row.skin_hue_deg);
        let chroma_ratio = ratio(chroma, row.skin_chroma);
        if hue_ratio > 1.0 || chroma_ratio > 1.0 {
            let (deviation, threshold) = if hue_ratio >= chroma_ratio {
                (hue, row.skin_hue_deg)
            } else {
                // Expressed in degrees-equivalent so the ticket's unit stays dE00-adjacent and the
                // queue's severity ratio still means the same thing.
                (
                    chroma * row.skin_hue_deg / row.skin_chroma.max(1e-6),
                    row.skin_hue_deg,
                )
            };
            let gain = (deviation - threshold).max(0.0) * GUARD_GAIN_SHARE;
            findings.push(Finding::new(
                QcCategory::Skin,
                QcCode::SkinGuardExceeded,
                deviation,
                threshold,
                gain,
                // High confidence: this is a measurement phase 16 made through the real renderer
                // rather than an inference from stored parameters.
                margin_confidence(deviation, threshold).max(0.75),
            ));
        }
    } else if skin.per_identity_de00.is_empty() {
        // Neither half could run. A frame with no per-identity readings *and* no measured guard has
        // not been checked for skin at all, and reporting it clean would be the exact failure this
        // module's header describes.
        return Outcome::Skipped(
            "phase 16's skin guard was not measured and no identity has a target",
        );
    }

    Outcome::from_findings(findings)
}

/// A value over its threshold, guarding against a zero or absent threshold.
fn ratio(value: f32, threshold: f32) -> f32 {
    if !value.is_finite() || !threshold.is_finite() || threshold <= 0.0 {
        return 0.0;
    }
    value / threshold
}

/// Confidence from how far past the threshold a reading sits.
///
/// 0.55 at the line, 0.95 at twice it. A frame one per cent over is a frame worth showing and not
/// one worth acting on unattended, and `FIX_CONFIDENCE_FLOOR` is 0.60 for that reason.
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
    use crate::checks::SkinReading;
    use aura_core::contract::ids::IdentityId;
    use aura_core::contract::qc::ImageId;
    use aura_core::contract::scene::SceneId;

    fn reading() -> SkinReading {
        SkinReading {
            per_identity_de00: vec![(IdentityId::new(), 0.8)],
            guard_hue_shift_deg: 0.4,
            guard_chroma_change: 0.01,
            guard_measured: true,
        }
    }

    fn frame_with(skin: SkinReading) -> Frame {
        Frame {
            skin: Some(skin),
            ..Frame::empty(ImageId::new(), SceneId::FamilyPortrait)
        }
    }

    #[test]
    fn a_frame_where_everybody_matches_their_own_target_is_clean() {
        assert_eq!(
            inspect(&frame_with(reading()), &Thresholds::reference()),
            Outcome::Clean
        );
    }

    #[test]
    fn one_finding_per_person_rather_than_one_per_frame() {
        let drifted = IdentityId::new();
        let fine = IdentityId::new();
        let mut skin = reading();
        skin.per_identity_de00 = vec![(drifted, 6.0), (fine, 0.5)];
        let findings = inspect(&frame_with(skin), &Thresholds::reference()).findings();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].identity, Some(drifted));
        assert_eq!(findings[0].code, QcCode::SkinDrift);
    }

    #[test]
    fn an_unmeasured_guard_never_reads_as_a_perfect_grade() {
        let mut skin = reading();
        skin.guard_measured = false;
        // The zeroes an unmeasured report carries. A build that trusted them would call this the
        // best-behaved frame in the wedding.
        skin.guard_hue_shift_deg = 0.0;
        skin.guard_chroma_change = 0.0;
        skin.per_identity_de00.clear();
        let outcome = inspect(&frame_with(skin), &Thresholds::reference());
        assert!(matches!(outcome, Outcome::Skipped(_)));
        assert_ne!(outcome, Outcome::Clean);
    }

    #[test]
    fn an_unmeasured_guard_still_reports_the_drift_half_when_it_can() {
        let mut skin = reading();
        skin.guard_measured = false;
        skin.per_identity_de00 = vec![(IdentityId::new(), 7.0)];
        let findings = inspect(&frame_with(skin), &Thresholds::reference()).findings();
        // Half the check ran and half did not; the half that ran still reports.
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, QcCode::SkinDrift);
    }

    #[test]
    fn a_guard_excursion_is_a_separate_code_from_a_drift() {
        let mut skin = reading();
        skin.guard_hue_shift_deg = 20.0;
        let findings = inspect(&frame_with(skin), &Thresholds::reference()).findings();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, QcCode::SkinGuardExceeded);
        // The two fail independently and are fixed by different things, so they never merge.
        assert!(findings[0].identity.is_none());
    }

    #[test]
    fn a_chroma_excursion_fires_even_when_the_hue_is_perfect() {
        let mut skin = reading();
        skin.guard_hue_shift_deg = 0.0;
        skin.guard_chroma_change = 0.9;
        let findings = inspect(&frame_with(skin), &Thresholds::reference()).findings();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, QcCode::SkinGuardExceeded);
    }

    #[test]
    fn the_target_is_per_person_rather_than_a_constant() {
        // Phase 15's rule, asserted by behaviour rather than by grep. Two people in one frame, each
        // measured against their **own** gallery target: one is inside it and one is not, from a
        // single reading. A module holding a fixed skin target could not produce that - it would
        // either flag both or neither.
        //
        // The grep half of this rule lives in `tests/no_pixel_ops.rs`, which scans this file from
        // outside it. The first version was an inline grep and it failed on its own test name,
        // which is the reason all five earlier greps in this repository scan other files.
        let inside = IdentityId::new();
        let outside = IdentityId::new();
        let mut skin = reading();
        skin.per_identity_de00 = vec![(inside, 0.4), (outside, 9.0)];
        let findings = inspect(&frame_with(skin), &Thresholds::reference()).findings();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].identity, Some(outside));
    }

    #[test]
    fn an_absent_reading_skips() {
        let frame = Frame::empty(ImageId::new(), SceneId::FamilyPortrait);
        assert!(matches!(
            inspect(&frame, &Thresholds::reference()),
            Outcome::Skipped(_)
        ));
    }
}
