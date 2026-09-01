//! Colour and tone against this frame's scene-node anchors. PHASE-27 section 2.1, phase 25.
//!
//! ## The measurement is against the node, not against the gallery
//!
//! Phase 25 divides a wedding into lighting groups and gives each one a target built from three to
//! five anchors it *chose* rather than averaged. This check asks whether a delivered frame ended up
//! inside its own group, and it asks it in units of **that group's own tolerance**.
//!
//! A threshold in kelvin would be wrong in every node whose light was more or less variable than
//! average, which is all of them: a ceremony lit by one window has a tolerance of 120 K and a
//! reception under moving colour has 600 K, and a fixed 300 K threshold reports the first as clean
//! when it is drifting and the second as broken when it is doing exactly what the room did.
//!
//! ## An unanchored node is a skip, and it is the one this build hits most
//!
//! `GalleryCode::NodeUnanchored` and `GalleryCode::AlreadyConsistent` both produce five zeroes
//! upstream and mean opposite things - phase 25 wrote that rule and this is where it is consumed. A
//! node with no target has nothing to be outside of, and a check that reported it clean would tell a
//! photographer their gallery is coherent when what happened is that nobody could tell.

use aura_core::contract::qc::{Evidence, QcCategory, QcCode};

use super::{Finding, Frame, Outcome};
use crate::policy::Thresholds;

/// How much of a colour drift a re-solve of the normalisation is predicted to remove.
///
/// Phase 25's normalisation is damped and then bounded, so a frame outside its node is usually a
/// frame the damping held back rather than one the solver got wrong - which means a re-solve under
/// a tighter constraint recovers most of the gap but not all of it. Three quarters is the share
/// that survived the fixture corpus; the loop measures what was actually realised against it, so an
/// optimistic constant costs a reverted round rather than a bad delivery.
const NORMALISE_GAIN_SHARE: f32 = 0.75;

/// How much of a grade-character difference a re-solve is predicted to remove.
///
/// Lower than the colour share, because a signature difference is often a *content* difference - a
/// frame of a dark suit inside a node of pale dresses - and no amount of re-solving makes a suit
/// pale. The gate is deliberately pessimistic here so a signature ticket escalates rather than
/// burning both rounds.
const SIGNATURE_GAIN_SHARE: f32 = 0.40;

/// Inspect one frame against its lighting group.
#[must_use]
pub fn inspect(frame: &Frame, thresholds: &Thresholds) -> Outcome {
    let Some(node) = frame.node.as_ref() else {
        return Outcome::Skipped("no scene node for this frame");
    };
    if !node.anchored {
        // Phase 25's rule, consumed: a node with no target is a node nobody could measure against.
        return Outcome::Skipped("the scene node has no anchors and therefore no target");
    }
    let row = thresholds.scene(frame.scene);
    let mut findings = Vec::new();

    // Each axis in units of its own tolerance, then the worst of the three. A sum would let three
    // small drifts add into a ticket nobody can see, and a mean would let one large one hide.
    let cct = axis_sigma(node.frame_cct_k - node.target_cct_k, node.cct_tol);
    let tint = axis_sigma(node.frame_tint - node.target_tint, node.tint_tol);
    let luma = axis_sigma(node.frame_luma - node.target_luma, node.luma_tol);
    let worst = cct.max(tint).max(luma);

    if worst > row.consistency_sigma {
        let gain = (worst - row.consistency_sigma) * NORMALISE_GAIN_SHARE;
        findings.push(
            Finding::new(
                QcCategory::Consistency,
                QcCode::ConsistencyDrift,
                worst,
                row.consistency_sigma,
                gain,
                // Confidence rises with how far out it is and with how many anchors the target was
                // built from. A target from three anchors is a real target and a weaker one than a
                // target from five, and a panel that showed both at the same confidence would be
                // presenting two different amounts of evidence identically.
                confidence(worst, row.consistency_sigma, node.anchors.len()),
            )
            .with_evidence(Evidence::Anchors(node.anchors.clone())),
        );
    }

    // `None` skips this half rather than passing it: defaulting to the node's own target would
    // read as a perfect match on every frame nobody could measure.
    let signature = node
        .frame_signature
        .map_or(0.0, |frame| distance(&frame, &node.target_signature));
    if node.frame_signature.is_some() && signature > row.signature_distance {
        let gain = (signature - row.signature_distance) * SIGNATURE_GAIN_SHARE;
        findings.push(
            Finding::new(
                QcCategory::Consistency,
                QcCode::SignatureDrift,
                signature,
                row.signature_distance,
                gain,
                confidence(signature, row.signature_distance, node.anchors.len()),
            )
            .with_evidence(Evidence::Anchors(node.anchors.clone())),
        );
    }

    Outcome::from_findings(findings)
}

/// One axis's departure, in units of the node's own tolerance for it.
///
/// A zero or negative tolerance is treated as "no opinion" rather than as "infinitely strict",
/// which is what a naive division would produce: `x / 0.0` is infinity, and an infinite deviation
/// against a finite threshold is a maximum-severity ticket on every frame of a node whose tolerance
/// column was never written.
fn axis_sigma(delta: f32, tolerance: f32) -> f32 {
    if !delta.is_finite() || !tolerance.is_finite() || tolerance <= 0.0 {
        return 0.0;
    }
    (delta.abs() / tolerance).max(0.0)
}

/// Euclidean distance between two eight-number grade signatures.
fn distance(left: &[f32; 8], right: &[f32; 8]) -> f32 {
    let sum: f32 = left
        .iter()
        .zip(right.iter())
        .map(|(a, b)| {
            let d = a - b;
            if d.is_finite() {
                d * d
            } else {
                0.0
            }
        })
        .sum();
    sum.max(0.0).sqrt()
}

/// How sure a drift finding is.
///
/// Two independent terms multiplied rather than averaged, so neither can rescue the other - the
/// same geometric argument phase 12 uses for its keep score and phase 18 for its allowance. A frame
/// barely over the line in a node with three anchors is a finding worth showing and not one worth
/// acting on unattended.
fn confidence(deviation: f32, threshold: f32, anchors: usize) -> f32 {
    let margin = if threshold > 0.0 {
        ((deviation - threshold) / threshold).clamp(0.0, 1.0)
    } else {
        0.0
    };
    // 0.55 at the threshold itself, rising to 0.95 at twice it.
    let by_margin = 0.55 + 0.40 * margin;
    // Three anchors is phase 25's minimum and five is its maximum.
    let by_evidence = match anchors {
        0 => 0.0,
        1 | 2 => 0.70,
        3 => 0.85,
        _ => 1.0,
    };
    (by_margin * by_evidence).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::NodeReading;
    use aura_core::contract::qc::ImageId;
    use aura_core::contract::scene::SceneId;

    fn node() -> NodeReading {
        NodeReading {
            target_cct_k: 5200.0,
            cct_tol: 200.0,
            target_tint: 0.0,
            tint_tol: 4.0,
            target_luma: 0.45,
            luma_tol: 0.20,
            target_signature: [0.5; 8],
            frame_signature: Some([0.5; 8]),
            frame_cct_k: 5200.0,
            frame_tint: 0.0,
            frame_luma: 0.45,
            anchors: vec![ImageId::new(), ImageId::new(), ImageId::new()],
            anchored: true,
        }
    }

    fn frame_with(node: NodeReading) -> Frame {
        Frame {
            node: Some(node),
            ..Frame::empty(ImageId::new(), SceneId::Ceremony)
        }
    }

    #[test]
    fn a_frame_on_its_nodes_target_is_clean() {
        let outcome = inspect(&frame_with(node()), &Thresholds::reference());
        assert_eq!(outcome, Outcome::Clean);
    }

    #[test]
    fn a_frame_outside_its_nodes_tolerance_is_a_finding_with_the_anchors_attached() {
        let mut reading = node();
        // Three tolerances out on colour temperature, against a two-tolerance threshold.
        reading.frame_cct_k = 5200.0 + 3.0 * reading.cct_tol;
        let outcome = inspect(&frame_with(reading), &Thresholds::reference());
        let findings = outcome.findings();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, QcCode::ConsistencyDrift);
        assert!((findings[0].deviation - 3.0).abs() < 1e-4);
        assert!(matches!(findings[0].evidence, Evidence::Anchors(ref a) if a.len() == 3));
    }

    #[test]
    fn an_unanchored_node_skips_rather_than_passes() {
        let mut reading = node();
        reading.anchored = false;
        // Wildly out - and still a skip, because there is nothing to be out *of*.
        reading.frame_cct_k = 9000.0;
        let outcome = inspect(&frame_with(reading), &Thresholds::reference());
        assert!(matches!(outcome, Outcome::Skipped(_)));
        assert_ne!(outcome, Outcome::Clean);
    }

    #[test]
    fn a_node_with_no_tolerance_does_not_produce_an_infinite_deviation() {
        let mut reading = node();
        reading.cct_tol = 0.0;
        reading.frame_cct_k = 9000.0;
        let outcome = inspect(&frame_with(reading), &Thresholds::reference());
        // A missing tolerance is "no opinion", not "infinitely strict". The naive division would
        // put a maximum-severity ticket on every frame of the node.
        assert_eq!(outcome, Outcome::Clean);
    }

    #[test]
    fn the_worst_axis_decides_rather_than_the_sum() {
        let mut reading = node();
        // Three axes each 1.5 tolerances out. A sum would be 4.5 and would fire; the worst is 1.5
        // and does not, which is correct: three small drifts in the same direction is a frame that
        // is slightly out, not a frame that is badly out.
        reading.frame_cct_k = 5200.0 + 1.5 * reading.cct_tol;
        reading.frame_tint = 1.5 * reading.tint_tol;
        reading.frame_luma = 0.45 + 1.5 * reading.luma_tol;
        let outcome = inspect(&frame_with(reading), &Thresholds::reference());
        assert_eq!(outcome, Outcome::Clean);
    }

    #[test]
    fn more_anchors_means_more_confidence_in_the_same_finding() {
        let mut three = node();
        three.frame_cct_k = 5200.0 + 3.0 * three.cct_tol;
        let mut five = three.clone();
        five.anchors.push(ImageId::new());
        five.anchors.push(ImageId::new());
        let a = inspect(&frame_with(three), &Thresholds::reference()).findings();
        let b = inspect(&frame_with(five), &Thresholds::reference()).findings();
        assert!(b[0].confidence > a[0].confidence);
        assert_eq!(a[0].deviation, b[0].deviation);
    }

    #[test]
    fn a_signature_difference_is_a_separate_finding_with_a_pessimistic_gain() {
        let mut reading = node();
        reading.frame_signature = Some([0.9; 8]);
        let findings = inspect(&frame_with(reading), &Thresholds::reference()).findings();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, QcCode::SignatureDrift);
        // A suit in a node of pale dresses cannot be re-solved into a dress. The gain is under half
        // the gap, so the ticket escalates rather than burning both rounds.
        assert!(findings[0].expected_gain < (findings[0].deviation - findings[0].threshold) * 0.5);
    }

    #[test]
    fn an_absent_node_skips() {
        let frame = Frame::empty(ImageId::new(), SceneId::Ceremony);
        assert!(matches!(
            inspect(&frame, &Thresholds::reference()),
            Outcome::Skipped(_)
        ));
    }
}
