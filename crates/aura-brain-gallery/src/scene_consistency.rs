//! How a node's contrast and colour character are harmonised.
//!
//! Section 2.1: "harmonise contrast, saturation, black point and grade character within each scene
//! node so sequences read as one look."
//!
//! ## Why this is a separate module from the solver rather than two more axes in it
//!
//! Because the *decision* is different even though the arithmetic is the same. Warmth and exposure
//! are corrections: a frame that is 200 K warm than its node is wrong, and moving it is a repair.
//! Contrast is not - a frame with more contrast than its node may be a different subject at a
//! different distance in the same light, and moving it is a **style** decision about whether this
//! kind of scene is supposed to be uniform.
//!
//! That is why `harmonise_grade` is a per-scene switch in `consistency.toml` rather than a damping
//! factor. It is **off** in four scenes - golden hour, first dance, dance floor and exit - where the
//! variation is the point, and turning it on in any of them is section 12's first failure mode
//! written as a config change.
//!
//! ## The grade signature is compared and never applied
//!
//! [`character_gap`] measures how far a frame's colour character is from its node's. Nothing turns
//! a signature back into parameters, in this module or anywhere in the contract: phase 16 owns the
//! grade, and a signature that could be inverted would make this crate a second grader. What the
//! gap does is **scale the contrast and saturation movement**, so a frame that already looks like
//! its node is moved less than one that does not - which is the difference between harmonising a
//! sequence and flattening it.

use aura_core::contract::gallery::NodeTarget;

use crate::stats::GradeSignature;
use crate::tree::Frame;

/// How far a frame's colour character is from its node's, `0..1`.
///
/// Zero when the frame already looks like its node. One when it could not look less like it.
/// Returns `None` when the frame has no grade decision, which is `GalleryCode::ColourDecisionAbsent`
/// rather than a gap of zero: a frame nobody graded is not a frame that already matches.
#[must_use]
pub fn character_gap(frame: &Frame, target: &NodeTarget) -> Option<f32> {
    let values = frame.signature?;
    Some(GradeSignature { values }.distance(&GradeSignature {
        values: target.grade_signature,
    }))
}

/// How much of a contrast or saturation movement to keep, given the character gap.
///
/// A frame whose character is already close to its node keeps nearly all of its own contrast, and
/// one whose character is a long way off is moved further. That is the opposite of the naive
/// reading - "it is already different, leave it alone" - and it is deliberate: the *point* of the
/// grade half is that a sequence reads as one look, and the frames that break the sequence are
/// exactly the ones whose character is furthest away.
///
/// The scale never reaches zero, because a frame with an identical character can still have a
/// contrast a long way from the node's - two frames of the same room at the same grade character
/// and eight units of contrast apart look like an error - and it never exceeds one, because the
/// bounds are the bounds.
#[must_use]
pub fn harmony_scale(gap: f32) -> f32 {
    const FLOOR: f32 = 0.35;
    if !gap.is_finite() {
        return FLOOR;
    }
    // A gap of 0.2 in signature distance is already a visibly different look; past it the scale is
    // saturated at one rather than continuing to grow, because a bound is not a target.
    FLOOR + (1.0 - FLOOR) * (gap / 0.2).clamp(0.0, 1.0)
}

/// The black point a node's frames share, `0..1`.
///
/// Slot 7 of the grade signature, pulled out by name because it is the one component section 2.1
/// mentions on its own - "harmonise contrast, saturation, black point and grade character" - and a
/// reader looking for it should not have to count array indices.
#[must_use]
pub fn black_point(signature: &[f32; 8]) -> f32 {
    signature.get(7).copied().unwrap_or(0.0)
}

/// The mid-tone slope a node's frames share, in the `-1..1` form the signature stores at 0.5.
#[must_use]
pub fn mid_slope(signature: &[f32; 8]) -> f32 {
    signature.get(6).copied().unwrap_or(0.5).mul_add(2.0, -1.0)
}

/// True when two nodes are close enough in character to read as one look.
///
/// Not used by the solver - nothing harmonises across nodes, and ADR-0051 section 11 records why -
/// but it is what the panel's timeline strips colour a boundary with, so a photographer can see
/// whether a change point split two genuinely different looks or two nearly identical ones.
#[must_use]
pub fn reads_as_one_look(a: &NodeTarget, b: &NodeTarget) -> bool {
    const SAME_LOOK: f32 = 0.08;
    NodeTarget::signature_distance(&a.grade_signature, &b.grade_signature) < SAME_LOOK
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use aura_core::{SceneId, SegmentId};

    fn target(signature: [f32; 8]) -> NodeTarget {
        NodeTarget {
            cct_k: 5000.0,
            cct_tol: 150.0,
            tint: 0.0,
            tint_tol: 4.0,
            subject_luma: 0.45,
            luma_tol: 0.05,
            contrast: 10.0,
            saturation: 4.0,
            grade_signature: signature,
            anchor_count: 4,
            cohesion: 0.9,
        }
    }

    #[test]
    fn a_frame_with_no_grade_has_no_gap_rather_than_a_gap_of_zero() {
        let mut frame = fixtures::frame_at(SegmentId::new(), 0, SceneId::Ceremony);
        frame.signature = None;
        assert!(character_gap(&frame, &target([0.5; 8])).is_none());
    }

    #[test]
    fn a_frame_that_already_looks_like_its_node_has_a_gap_near_zero() {
        let signature = GradeSignature::new(30.0, 8.0, 40.0, 6.0, 0.1, 0.05, 0.1, 0.02).values;
        let mut frame = fixtures::frame_at(SegmentId::new(), 0, SceneId::Ceremony);
        frame.signature = Some(signature);
        let gap = character_gap(&frame, &target(signature)).unwrap();
        assert!(gap < 1e-5, "{gap}");
    }

    #[test]
    fn the_harmony_scale_moves_the_frames_that_break_the_sequence_furthest() {
        let close = harmony_scale(0.0);
        let far = harmony_scale(0.3);
        assert!(far > close);
        assert!(
            close > 0.0,
            "an identical character still allows a contrast match"
        );
        assert!(far <= 1.0);
    }

    #[test]
    fn the_scale_saturates_rather_than_growing_without_limit() {
        assert_eq!(harmony_scale(0.2), harmony_scale(5.0));
    }

    #[test]
    fn the_black_point_and_slope_come_out_of_the_slots_they_went_into() {
        let signature = GradeSignature::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -0.4, 0.07).values;
        assert!((black_point(&signature) - 0.07).abs() < 1e-5);
        assert!((mid_slope(&signature) + 0.4).abs() < 1e-5);
    }

    #[test]
    fn two_nodes_of_the_same_room_read_as_one_look_and_two_rooms_do_not() {
        let warm = GradeSignature::new(30.0, 8.0, 40.0, 6.0, 0.10, 0.05, 0.1, 0.02).values;
        let same = GradeSignature::new(31.0, 8.0, 41.0, 6.0, 0.10, 0.05, 0.1, 0.02).values;
        let cool = GradeSignature::new(210.0, 8.0, 200.0, 6.0, 0.35, 0.30, -0.5, 0.25).values;
        assert!(reads_as_one_look(&target(warm), &target(same)));
        assert!(!reads_as_one_look(&target(warm), &target(cool)));
    }
}
