//! Candidate detection: unexplained salience, ranked and capped.
//!
//! ## What this is not
//!
//! Section 6.1 asks for a learned detector on a labelled wedding-distraction vocabulary, and
//! section 9's DATA row asks for that vocabulary on ten thousand frames. There are no wedding
//! photographs in this repository, so there are no labels, so there is no detector to train.
//! [`crate::DISTRACTION_HEAD_TRAINED`] is false and nothing here consults a model.
//!
//! ## What it is
//!
//! The other half of section 6.1: *unexplained salience*. A region that draws the eye, sits in the
//! background plane, is far from every subject, and is not explained by anything the story knows
//! about. That is measurable without labels, and it is the half a measurement can honestly do.
//!
//! It names nothing. Every candidate it produces is `DistractionClass::Unclassified`, which cannot
//! be shown to be story-irrelevant, so [`crate::safety::check`] blocks all of them at the
//! confidence check. **This build therefore proposes no removals**, which is the correct behaviour
//! for a build that cannot tell a bin from a gift, and is condition C2 of the exit report.
//!
//! ADR-0049 section 6.

use aura_core::contract::cleanup::{Box2, DistractionClass, MAX_PROPOSALS_PER_IMAGE};

use crate::safety::Candidate;

/// What the detector is given about one photograph.
///
/// Everything here was measured by an earlier phase. This module opens no pixels of its own: phase
/// 11 measured the salience field, phase 06 found the subjects, and phase 23's estimator found the
/// straight lines. Invariant 3 and phase 05's rule that descriptors are computed once.
#[derive(Debug, Clone, Default)]
pub struct Frame {
    /// Regions that draw attention, with how much, from phase 11.
    pub salient: Vec<(Box2, f32)>,
    /// Where the people are, from phase 06. A candidate near one of these is not background.
    pub subjects: Vec<Box2>,
    /// Long straight lines and repeating-pattern boundaries, from phase 23's edge tracker.
    pub structure: Vec<Box2>,
    /// True when the frame's own depth or focus separation puts a region behind the subject.
    ///
    /// Absent in this build - there is no depth estimate - so it is `false` everywhere and the
    /// distance-from-subject term does the whole job.
    pub background_known: bool,
}

/// How near the frame edge a region has to be to count as edge clutter, in normalised units.
const EDGE_BAND: f32 = 0.15;

/// How far from every subject a region has to sit before it is treated as background.
///
/// Measured centre to nearest box edge. Generous, because the cost of treating a near-subject
/// region as background is a candidate that the denylist then has to catch, and the cost of the
/// opposite is a distraction nobody offers to remove.
const SUBJECT_CLEARANCE: f32 = 0.08;

/// Find the regions worth asking the safety engine about.
///
/// Ranked by salience times removability and capped at [`MAX_PROPOSALS_PER_IMAGE`], which is
/// section 6.1's "cleanup stays a light touch". A frame with fifteen distractions is a frame whose
/// background is the problem, and removing three of them makes it look edited rather than better.
#[must_use]
pub fn candidates(frame: &Frame) -> Vec<Candidate> {
    let mut found: Vec<Candidate> = frame
        .salient
        .iter()
        .filter_map(|(region, salience)| {
            let clearance = subject_clearance(region, &frame.subjects);
            if clearance < SUBJECT_CLEARANCE {
                return None;
            }
            Some(Candidate {
                region: *region,
                // Nothing here can name what it found. See the module docs.
                class: DistractionClass::Unclassified,
                salience: salience.clamp(0.0, 1.0),
                removability: removability(region, clearance),
                crosses_structure: crosses_structure(region, &frame.structure),
                touches_identity: false,
            })
        })
        .collect();

    found.sort_by(|a, b| {
        let sa = a.salience * a.removability;
        let sb = b.salience * b.removability;
        sb.partial_cmp(&sa)
            .unwrap_or(std::cmp::Ordering::Equal)
            // A deterministic tie-break, because invariant 4 says the same inputs produce the same
            // answer and two regions with identical scores are common on a synthetic frame.
            .then_with(|| {
                a.region
                    .x
                    .partial_cmp(&b.region.x)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                a.region
                    .y
                    .partial_cmp(&b.region.y)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    found.truncate(MAX_PROPOSALS_PER_IMAGE);
    found
}

/// How far a region sits from the nearest subject, centre to box, in normalised units.
fn subject_clearance(region: &Box2, subjects: &[Box2]) -> f32 {
    if subjects.is_empty() {
        // No subjects found is not "everything is background". It is "nothing is known", and the
        // clearance that produces is the largest one - which is safe here because the *denylist*
        // is what protects people, and it blocks outright when phase 18's masks are absent.
        return 1.0;
    }
    let cx = region.x + region.w * 0.5;
    let cy = region.y + region.h * 0.5;
    subjects
        .iter()
        .map(|s| {
            let dx = (s.x - cx).max(0.0).max(cx - (s.x + s.w)).max(0.0);
            let dy = (s.y - cy).max(0.0).max(cy - (s.y + s.h)).max(0.0);
            dx.hypot(dy)
        })
        .fold(f32::MAX, f32::min)
}

/// How confident the measurement is that this region can be removed at all.
///
/// Three terms, multiplied rather than averaged, so a zero in any one of them is a zero overall -
/// the geometric shape phases 09, 11 and 12 use, for the reason they use it: no signal may rescue
/// another.
fn removability(region: &Box2, clearance: f32) -> f32 {
    let area = (region.w * region.h).clamp(0.0, 1.0);
    // Small is good. A region at the cap scores 0.5; a tiny one approaches 1.
    let size_term = (1.0 - area * 12.5).clamp(0.0, 1.0);
    // Near an edge is good.
    let edge = region
        .x
        .min(region.y)
        .min(1.0 - (region.x + region.w))
        .min(1.0 - (region.y + region.h));
    let edge_term = (1.0 - (edge / EDGE_BAND)).clamp(0.0, 1.0);
    // Far from a subject is good.
    let clear_term = (clearance / (SUBJECT_CLEARANCE * 4.0)).clamp(0.0, 1.0);
    (size_term * edge_term * clear_term).clamp(0.0, 1.0)
}

/// True when the region touches any long straight line or repeating-pattern boundary.
fn crosses_structure(region: &Box2, structure: &[Box2]) -> bool {
    structure.iter().any(|line| {
        let x0 = region.x.max(line.x);
        let y0 = region.y.max(line.y);
        let x1 = (region.x + region.w).min(line.x + line.w);
        let y1 = (region.y + region.h).min(line.y + line.h);
        x1 > x0 && y1 > y0
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Box2 {
        Box2 { x, y, w, h }
    }

    #[test]
    fn nothing_this_detector_finds_is_ever_named() {
        let frame = Frame {
            salient: vec![(rect(0.02, 0.85, 0.06, 0.06), 0.9)],
            ..Frame::default()
        };
        for candidate in candidates(&frame) {
            assert_eq!(
                candidate.class,
                DistractionClass::Unclassified,
                "this build has no trained detector and must name nothing"
            );
        }
    }

    #[test]
    fn a_region_on_a_subject_is_not_a_candidate() {
        let frame = Frame {
            salient: vec![(rect(0.45, 0.45, 0.05, 0.05), 0.9)],
            subjects: vec![rect(0.40, 0.40, 0.20, 0.30)],
            ..Frame::default()
        };
        assert!(candidates(&frame).is_empty());
    }

    #[test]
    fn a_corner_region_far_from_everybody_is_a_candidate() {
        let frame = Frame {
            salient: vec![(rect(0.01, 0.90, 0.05, 0.05), 0.8)],
            subjects: vec![rect(0.40, 0.30, 0.20, 0.40)],
            ..Frame::default()
        };
        let found = candidates(&frame);
        assert_eq!(found.len(), 1);
        assert!(found[0].removability > 0.0);
    }

    #[test]
    fn a_region_over_a_straight_line_is_marked_and_the_safety_engine_blocks_it_later() {
        let frame = Frame {
            salient: vec![(rect(0.02, 0.50, 0.05, 0.05), 0.8)],
            subjects: vec![rect(0.40, 0.30, 0.20, 0.40)],
            // Spans y 0.49..0.52, so it genuinely crosses the candidate's 0.50..0.55 rather
            // than abutting it at a single coordinate.
            structure: vec![rect(0.0, 0.49, 1.0, 0.03)],
            ..Frame::default()
        };
        let found = candidates(&frame);
        assert_eq!(found.len(), 1);
        assert!(found[0].crosses_structure);
    }

    #[test]
    fn no_more_than_three_candidates_survive() {
        let salient = (0..9)
            .map(|i| {
                let y = 0.02 + (i as f32) * 0.1;
                (rect(0.01, y, 0.04, 0.04), 0.5 + (i as f32) * 0.05)
            })
            .collect();
        let frame = Frame {
            salient,
            subjects: vec![rect(0.45, 0.45, 0.10, 0.10)],
            ..Frame::default()
        };
        assert!(candidates(&frame).len() <= MAX_PROPOSALS_PER_IMAGE);
    }

    #[test]
    fn the_ordering_is_deterministic_for_equal_scores() {
        let salient = vec![
            (rect(0.30, 0.02, 0.04, 0.04), 0.6),
            (rect(0.10, 0.02, 0.04, 0.04), 0.6),
            (rect(0.20, 0.02, 0.04, 0.04), 0.6),
        ];
        let frame = Frame {
            salient,
            subjects: vec![rect(0.45, 0.45, 0.10, 0.10)],
            ..Frame::default()
        };
        let first = candidates(&frame);
        let second = candidates(&frame);
        assert_eq!(first, second);
        // Invariant 4: the tie-break is positional, so the leftmost wins rather than whichever
        // the sort happened to visit first.
        assert!(first[0].region.x <= first[1].region.x);
    }

    #[test]
    fn a_large_region_scores_lower_than_a_small_one_in_the_same_place() {
        let small = removability(&rect(0.02, 0.90, 0.03, 0.03), 0.5);
        let large = removability(&rect(0.02, 0.85, 0.15, 0.10), 0.5);
        assert!(small > large, "small {small} large {large}");
    }
}
