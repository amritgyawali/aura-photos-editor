//! How far every other frame moves toward its node's anchors.
//!
//! Section 6.2, in one line:
//!
//! ```text
//! d = damping * (target - current)
//! ```
//!
//! then clamped by the bounds. Four sentences of arithmetic, and every hard decision in this phase
//! is about **what `current` is** and **what happens when the clamp bites**.
//!
//! ## `current` is the un-normalised estimate, always
//!
//! [`solve`] takes a [`crate::tree::Frame`] whose values came from phase 15's `ToneEstimate` and
//! phase 16's `ColourDecision`. It never takes a `NormalisationDelta`, there is no argument it
//! could arrive through, and the function is pure. So a second run computes exactly the same
//! number as the first and writing it again is a no-op.
//!
//! **That is what idempotence is here.** It is not achieved by detecting a second run, and it is
//! not achieved by convergence - a solver that iterated to a fixed point would converge toward the
//! mean of the node, which is the mediocrity section 6.1 exists to avoid, at a rate that depends on
//! floating-point ordering. `NormalisationDelta::agrees_with` is the gate; the purity is the
//! mechanism. ADR-0051 section 2.
//!
//! ## Damping first, bound second
//!
//! The order matters and the wrong one is subtle. Bounding first and damping afterwards would make
//! the bound a *target* rather than a limit: every frame more than one bound away would land at
//! `damping * bound`, exactly, and a gallery would grow a visible band of identically-corrected
//! frames at the edge of a transition. Damping first means a distant frame is clamped and a nearby
//! one is not, which is the intended behaviour - the bound is for the frames the node is wrong
//! about.
//!
//! ## A clamped frame is less confident, not more
//!
//! The instinct is that a big correction is a confident one. It is the opposite. The bound bit
//! because the frame and the node disagree about what room they are in, and the most likely
//! explanations are a change point the detector missed or an anchor that should not have been
//! chosen. So `bounded_by` *lowers* the delta's confidence, and the frame is a candidate for
//! [`crate::outlier`] on the same evidence.

use aura_core::contract::gallery::{
    Bound, GalleryCode, GalleryReason, NodeTarget, NormalisationDelta, SkinCorrection,
};
use aura_core::contract::ids::NodeId;

use crate::policy::{Consistency, ScenePolicy};
use crate::tree::Frame;

/// One frame's answer, before the skin half is attached.
#[derive(Debug, Clone, PartialEq)]
pub struct Solved {
    /// The delta.
    pub delta: NormalisationDelta,
    /// What the frame is still away from its target, after the delta. What
    /// [`crate::outlier::detect`] reads.
    pub residual: Residual,
}

/// What is left after a delta was applied.
///
/// Signed and in each axis's own units, because section 6.4 asks for the deviation quantified -
/// "+310 K warmer than node anchors" - and an unsigned magnitude cannot say "warmer".
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Residual {
    /// Kelvin. Positive is warmer than the node.
    pub cct: f32,
    /// Tint units. Positive is more magenta.
    pub tint: f32,
    /// Stops. Positive is brighter.
    pub exposure: f32,
    /// Recipe units.
    pub contrast: f32,
    /// Recipe units.
    pub saturation: f32,
}

impl Residual {
    /// How far out this is overall, `0..1`, as a fraction of the bounds.
    ///
    /// The worst axis rather than the mean, for the reason `NormalisationDelta::magnitude` is:
    /// averaging a full-bound miss on one axis with four zeroes reports it as a fifth of a miss.
    #[must_use]
    pub fn deviation(&self, policy: &Consistency) -> f32 {
        [
            (self.cct / policy.bound(Bound::Cct)).abs(),
            (self.tint / policy.bound(Bound::Tint)).abs(),
            (self.exposure / policy.bound(Bound::Exposure)).abs(),
            (self.contrast / policy.bound(Bound::Contrast)).abs(),
            (self.saturation / policy.bound(Bound::Saturation)).abs(),
        ]
        .into_iter()
        .fold(0.0_f32, f32::max)
        .clamp(0.0, 1.0)
    }
}

/// Solve one frame against its node's target.
///
/// `node_confidence` is [`crate::anchors::target_confidence`]: how much the target is worth
/// believing. It multiplies into the delta's own confidence rather than gating it, because a
/// weakly-anchored node still produces a better gallery than no node at all - and the frame that
/// says otherwise is the one the outlier detector catches.
///
/// Pure. Every input is a value, nothing is read from a store, and the same arguments produce the
/// same answer on every machine on every run. That is invariant 4 and it is also the whole of this
/// phase's idempotence.
// Five axes, each with its own gap, its own damping and its own bound, plus the reason codes that
// describe what happened on each. Splitting it per axis would hide the one thing a reader has to
// see in one place: that the *same* damping and the *same* ordering apply to all five.
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn solve(
    frame: &Frame,
    node: NodeId,
    target: &NodeTarget,
    policy: ScenePolicy,
    bounds: &Consistency,
    node_confidence: f32,
) -> Solved {
    let from_cct = frame.cct_k.unwrap_or(0.0);
    let from_tint = frame.tint.unwrap_or(0.0);
    let from_exposure = frame.exposure_ev.unwrap_or(0.0);

    // A frame that cannot or must not move gets a zero delta and a code that says which. Five
    // zeroes with `GalleryCode::UserEdited` and five zeroes with `GalleryCode::AlreadyConsistent`
    // are the same numbers and opposite facts, which is why the code is not optional.
    if let Some(blocked) = frame.blocked_by() {
        return Solved {
            delta: zero(
                frame,
                node,
                from_cct,
                from_tint,
                from_exposure,
                policy,
                blocked,
            ),
            residual: Residual::default(),
        };
    }

    let subject_luma = frame.subject_luma.unwrap_or(target.subject_luma);
    let mut reasons: Vec<GalleryCode> = Vec::new();
    let mut bounded_by: Option<Bound> = None;

    let already = target.contains(from_cct, from_tint, subject_luma);

    // --- the tone half ---------------------------------------------------------------------
    //
    // Mixed light is not a veto and is not a free pass. A frame with two lights in it has one
    // temperature that is right for the subject and another that is right for the room, and moving
    // the whole frame toward a node's target corrects one and worsens the other. So the damping is
    // halved rather than zeroed: the frame still joins its chapter, less far.
    let mixed = frame.mixed_light;
    if mixed {
        reasons.push(GalleryCode::MixedLightSkipped);
    }
    let damping = if mixed {
        policy.damping * 0.5
    } else {
        policy.damping
    };

    let (d_cct, cct_bit) = axis(target.cct_k - from_cct, damping, bounds.bound(Bound::Cct));
    let (d_tint, tint_bit) = axis(target.tint - from_tint, damping, bounds.bound(Bound::Tint));
    // Exposure moves in stops and the target is a luminance, so the distance is a ratio in stops
    // rather than a subtraction. A subject at 0.20 and a target at 0.40 is one stop apart, and
    // subtracting them would say 0.2 - which read as stops is a fifth of the correction needed at
    // the dark end and five times too much at the bright one.
    let luma_gap = stops_between(subject_luma, target.subject_luma);
    let (d_exposure, exposure_bit) = axis(luma_gap, damping, bounds.bound(Bound::Exposure));

    // --- the grade half --------------------------------------------------------------------
    let harmonise = policy.harmonise_grade && frame.has_grade();
    let (d_contrast, contrast_bit) = if harmonise {
        axis(
            target.contrast - frame.contrast.unwrap_or(target.contrast),
            damping,
            bounds.bound(Bound::Contrast),
        )
    } else {
        (0.0, false)
    };
    let (d_saturation, saturation_bit) = if harmonise {
        axis(
            target.saturation - frame.saturation.unwrap_or(target.saturation),
            damping,
            bounds.bound(Bound::Saturation),
        )
    } else {
        (0.0, false)
    };
    if policy.harmonise_grade && !frame.has_grade() {
        reasons.push(GalleryCode::ColourDecisionAbsent);
    }

    // The first bound that bit, in `Bound::ALL` order, so two frames clamped on two axes report the
    // same one every run. The panel shows one; the residual carries all five.
    for (bound, bit) in [
        (Bound::Cct, cct_bit),
        (Bound::Tint, tint_bit),
        (Bound::Exposure, exposure_bit),
        (Bound::Contrast, contrast_bit),
        (Bound::Saturation, saturation_bit),
    ] {
        if bit && bounded_by.is_none() {
            bounded_by = Some(bound);
        }
    }

    if bounded_by.is_some() {
        reasons.push(GalleryCode::BoundedByPolicy);
    }
    if d_cct.abs() > f32::EPSILON || d_tint.abs() > f32::EPSILON {
        reasons.push(GalleryCode::WarmthNormalised);
    }
    if d_exposure.abs() > f32::EPSILON {
        reasons.push(GalleryCode::ExposureNormalised);
    }
    if d_contrast.abs() > f32::EPSILON || d_saturation.abs() > f32::EPSILON {
        reasons.push(GalleryCode::GradeHarmonised);
    }
    if already
        && reasons.iter().all(|code| {
            !matches!(
                code,
                GalleryCode::WarmthNormalised
                    | GalleryCode::ExposureNormalised
                    | GalleryCode::GradeHarmonised
            )
        })
    {
        reasons.push(GalleryCode::AlreadyConsistent);
    }

    let residual = Residual {
        cct: (from_cct + d_cct) - target.cct_k,
        tint: (from_tint + d_tint) - target.tint,
        exposure: -(luma_gap - d_exposure),
        contrast: frame
            .contrast
            .map_or(0.0, |c| (c + d_contrast) - target.contrast),
        saturation: frame
            .saturation
            .map_or(0.0, |s| (s + d_saturation) - target.saturation),
    };

    let confidence = confidence_of(frame, node_confidence, bounded_by.is_some(), mixed);

    Solved {
        delta: NormalisationDelta {
            image_id: frame.image,
            node_id: node,
            d_exposure,
            d_cct,
            d_tint,
            d_contrast,
            d_saturation,
            skin_correction: None,
            from_exposure_ev: from_exposure,
            from_cct_k: from_cct,
            from_tint,
            damping,
            bounded_by,
            reasons: reasons.iter().copied().map(GalleryReason::of).collect(),
            confidence,
            user_edited: false,
            analysis_ver: crate::ANALYSIS_VER,
            policy_ver: bounds.version,
        },
        residual,
    }
}

/// Attach a skin correction to a solved delta.
///
/// Separate from [`solve`] so the tone half stays pure with respect to pixels: the skin readings
/// arrive from [`crate::skin_consistency`], which needs a mask and a proxy, and a solver that took
/// them as an argument could not be run in a unit test without both.
pub fn with_skin(solved: &mut Solved, correction: Option<SkinCorrection>, code: GalleryCode) {
    solved.delta.skin_correction = correction;
    if !solved
        .delta
        .reasons
        .iter()
        .any(|reason| reason.code == code)
    {
        solved.delta.reasons.push(GalleryReason::of(code));
    }
    solved.delta.reasons.sort_by(|a, b| {
        b.weight
            .partial_cmp(&a.weight)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.code.cmp(&b.code))
    });
}

/// One axis: damp, then clamp. Returns the movement and whether the clamp bit.
fn axis(gap: f32, damping: f32, bound: f32) -> (f32, bool) {
    if !gap.is_finite() || !bound.is_finite() || bound <= 0.0 {
        return (0.0, false);
    }
    let wanted = damping * gap;
    if wanted.abs() <= bound {
        (wanted, false)
    } else {
        (bound.copysign(wanted), true)
    }
}

/// How many stops apart two subject luminances are.
///
/// A ratio in stops rather than a subtraction; see the note in [`solve`]. Both are floored at a
/// small positive value, because `log2(0)` is negative infinity and a subject luminance of zero is
/// a frame that is black rather than a frame that needs infinite exposure.
#[must_use]
pub fn stops_between(from: f32, to: f32) -> f32 {
    const FLOOR: f32 = 0.004;
    let from = from.max(FLOOR);
    let to = to.max(FLOOR);
    let stops = (to / from).log2();
    if stops.is_finite() {
        stops
    } else {
        0.0
    }
}

/// A delta with nothing in it, carrying the one code that says why.
fn zero(
    frame: &Frame,
    node: NodeId,
    from_cct: f32,
    from_tint: f32,
    from_exposure: f32,
    policy: ScenePolicy,
    code: GalleryCode,
) -> NormalisationDelta {
    NormalisationDelta {
        image_id: frame.image,
        node_id: node,
        d_exposure: 0.0,
        d_cct: 0.0,
        d_tint: 0.0,
        d_contrast: 0.0,
        d_saturation: 0.0,
        skin_correction: None,
        from_exposure_ev: from_exposure,
        from_cct_k: from_cct,
        from_tint,
        damping: policy.damping,
        bounded_by: None,
        reasons: vec![GalleryReason::of(code)],
        // A refusal the product is sure about. `MoodPreserved` and `UserEdited` are decisions
        // rather than gaps, and rendering them at a low confidence would put them in a review queue
        // they do not belong in.
        confidence: match code {
            GalleryCode::ToneEstimateAbsent => 0.0,
            _ => 0.9,
        },
        user_edited: matches!(code, GalleryCode::UserEdited),
        analysis_ver: crate::ANALYSIS_VER,
        policy_ver: 0,
    }
}

/// How much to believe a delta, `0..1`.
///
/// Three things multiplied, so none rescues another: how good the node's target is, how confident
/// phase 15 was about this frame, and whether the movement survived the bounds intact. Invariant 2,
/// and the third term is the one worth remembering - see the module header for why a clamped frame
/// is *less* confident rather than more.
#[must_use]
pub fn confidence_of(frame: &Frame, node_confidence: f32, bounded: bool, mixed: bool) -> f32 {
    let frame_term = (frame.wb_conf.clamp(0.0, 1.0) * frame.exposure_conf.clamp(0.0, 1.0)).sqrt();
    let bound_term = if bounded { 0.55 } else { 1.0 };
    let mixed_term = if mixed { 0.75 } else { 1.0 };
    (node_confidence.clamp(0.0, 1.0) * frame_term * bound_term * mixed_term).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anchors;
    use crate::fixtures;
    use aura_core::contract::gallery::{DEFAULT_DAMPING, MAX_D_CCT_K};
    use aura_core::{SceneId, SegmentId};

    fn setup(frame_cct: f32) -> (Frame, NodeTarget, ScenePolicy, Consistency) {
        let bounds = Consistency::default();
        let policy = bounds.scene(SceneId::Ceremony);
        let segment = SegmentId::new();
        let anchors_set: Vec<Frame> = (0..4)
            .map(|i| {
                let mut f = fixtures::frame_at(segment, i * 1_000, SceneId::Ceremony);
                f.cct_k = Some(5000.0);
                f.tint = Some(0.0);
                f.subject_luma = Some(0.45);
                f.contrast = Some(10.0);
                f.saturation = Some(4.0);
                f
            })
            .collect();
        let refs: Vec<&Frame> = anchors_set.iter().collect();
        let target = anchors::target_of(&refs, policy).unwrap();
        let mut frame = fixtures::frame_at(segment, 9_000, SceneId::Ceremony);
        frame.cct_k = Some(frame_cct);
        frame.tint = Some(0.0);
        frame.subject_luma = Some(0.45);
        frame.contrast = Some(10.0);
        frame.saturation = Some(4.0);
        (frame, target, policy, bounds)
    }

    #[test]
    fn a_frame_moves_the_damped_share_of_the_way_and_no_further() {
        let (frame, target, policy, bounds) = setup(5200.0);
        let solved = solve(&frame, NodeId::new(), &target, policy, &bounds, 1.0);
        let expected = policy.damping * (5000.0 - 5200.0);
        assert!(
            (solved.delta.d_cct - expected).abs() < 1.0,
            "moved {} rather than {expected}",
            solved.delta.d_cct
        );
        assert!(solved.delta.bounded_by.is_none());
        assert!(solved.delta.within_bounds());
    }

    #[test]
    fn a_distant_frame_is_clamped_and_names_the_axis() {
        let (frame, target, policy, bounds) = setup(6800.0);
        let solved = solve(&frame, NodeId::new(), &target, policy, &bounds, 1.0);
        assert_eq!(solved.delta.bounded_by, Some(Bound::Cct));
        assert!((solved.delta.d_cct + MAX_D_CCT_K).abs() < 1e-3);
        assert!(solved
            .delta
            .reasons
            .iter()
            .any(|r| r.code == GalleryCode::BoundedByPolicy));
    }

    #[test]
    fn damping_runs_before_the_bound_so_the_bound_is_not_a_target() {
        // Two frames both past the bound by different amounts land on the same clamp; two frames
        // inside it do not. What is being asserted is the *ordering*: with the bound applied first
        // and damping second, a frame at 5,600 K would land at `damping * bound` rather than at
        // `damping * gap`.
        let (near, target, policy, bounds) = setup(5400.0);
        let solved = solve(&near, NodeId::new(), &target, policy, &bounds, 1.0);
        let damped_gap = policy.damping * 400.0;
        assert!(
            (solved.delta.d_cct.abs() - damped_gap).abs() < 1.0,
            "a nearby frame landed at {} rather than {damped_gap}",
            solved.delta.d_cct.abs()
        );
        assert!(solved.delta.d_cct.abs() < bounds.bound(Bound::Cct));
    }

    #[test]
    fn solving_twice_produces_an_identical_delta() {
        let (frame, target, policy, bounds) = setup(5300.0);
        let node = NodeId::new();
        let one = solve(&frame, node, &target, policy, &bounds, 0.8);
        let two = solve(&frame, node, &target, policy, &bounds, 0.8);
        assert_eq!(one.delta, two.delta);
        assert!(one.delta.agrees_with(&two.delta));
    }

    #[test]
    fn a_solved_frame_re_solved_from_its_own_stored_input_does_not_move_again() {
        // The property that matters: `solve` reads `Frame`, which carries phase 15's answer, and a
        // stored delta never becomes an input. So re-running the pass over the same project is a
        // no-op whatever was written last time.
        let (frame, target, policy, bounds) = setup(5300.0);
        let node = NodeId::new();
        let first = solve(&frame, node, &target, policy, &bounds, 0.8);
        // Simulate a second pass: the frame is unchanged, because phase 15's row is unchanged.
        let second = solve(&frame, node, &target, policy, &bounds, 0.8);
        assert!(first.delta.agrees_with(&second.delta));
        assert_eq!(first.residual, second.residual);
    }

    #[test]
    fn a_frame_already_inside_tolerance_moves_almost_nothing_and_says_so() {
        let (frame, target, policy, bounds) = setup(5030.0);
        let solved = solve(&frame, NodeId::new(), &target, policy, &bounds, 1.0);
        assert!(solved.delta.d_cct.abs() < 30.0);
        assert!(solved
            .delta
            .reasons
            .iter()
            .any(|r| r.code == GalleryCode::WarmthNormalised
                || r.code == GalleryCode::AlreadyConsistent));
    }

    #[test]
    fn a_user_edited_frame_gets_five_zeroes_and_the_code_that_says_which_zero_it_is() {
        let (mut frame, target, policy, bounds) = setup(6800.0);
        frame.user_edited = true;
        let solved = solve(&frame, NodeId::new(), &target, policy, &bounds, 1.0);
        assert!(solved.delta.is_zero());
        assert!(solved.delta.user_edited);
        assert_eq!(solved.delta.reasons[0].code, GalleryCode::UserEdited);
    }

    #[test]
    fn an_intentionally_lit_frame_is_left_alone_however_far_it_is_from_the_node() {
        let (mut frame, target, policy, bounds) = setup(2100.0);
        frame.intentional_light = true;
        let solved = solve(&frame, NodeId::new(), &target, policy, &bounds, 1.0);
        assert!(solved.delta.is_zero());
        assert_eq!(solved.delta.reasons[0].code, GalleryCode::MoodPreserved);
    }

    #[test]
    fn a_mixed_light_frame_still_joins_its_chapter_but_less_far() {
        let (mut frame, target, policy, bounds) = setup(5400.0);
        let plain = solve(&frame, NodeId::new(), &target, policy, &bounds, 1.0);
        frame.mixed_light = true;
        let mixed = solve(&frame, NodeId::new(), &target, policy, &bounds, 1.0);
        assert!(mixed.delta.d_cct.abs() < plain.delta.d_cct.abs());
        assert!(mixed.delta.d_cct.abs() > 0.0, "half a correction, not none");
        assert!(mixed.delta.confidence < plain.delta.confidence);
    }

    #[test]
    fn a_clamped_frame_is_less_confident_rather_than_more() {
        let (near, target, policy, bounds) = setup(5200.0);
        let (far, ..) = setup(7500.0);
        let a = solve(&near, NodeId::new(), &target, policy, &bounds, 1.0);
        let b = solve(&far, NodeId::new(), &target, policy, &bounds, 1.0);
        assert!(
            b.delta.confidence < a.delta.confidence,
            "the bound bit because the frame and the node disagree about the room"
        );
    }

    #[test]
    fn exposure_moves_in_stops_rather_than_in_luminance_units() {
        let bounds = Consistency::default();
        let policy = bounds.scene(SceneId::Ceremony);
        let segment = SegmentId::new();
        let anchor_set: Vec<Frame> = (0..4)
            .map(|i| {
                let mut f = fixtures::frame_at(segment, i * 1_000, SceneId::Ceremony);
                f.subject_luma = Some(0.40);
                f
            })
            .collect();
        let refs: Vec<&Frame> = anchor_set.iter().collect();
        let target = anchors::target_of(&refs, policy).unwrap();
        let mut dark = fixtures::frame_at(segment, 9_000, SceneId::Ceremony);
        dark.subject_luma = Some(0.20);
        let solved = solve(&dark, NodeId::new(), &target, policy, &bounds, 1.0);
        // One full stop apart, damped, then clamped at 0.35 EV.
        assert!(solved.delta.d_exposure > 0.0);
        assert_eq!(solved.delta.bounded_by, Some(Bound::Exposure));
    }

    #[test]
    fn the_dance_floor_grade_is_not_harmonised() {
        let bounds = Consistency::default();
        let policy = bounds.scene(SceneId::DanceFloor);
        let segment = SegmentId::new();
        let anchor_set: Vec<Frame> = (0..4)
            .map(|i| {
                let mut f = fixtures::frame_at(segment, i * 1_000, SceneId::DanceFloor);
                f.contrast = Some(30.0);
                f.saturation = Some(20.0);
                f
            })
            .collect();
        let refs: Vec<&Frame> = anchor_set.iter().collect();
        let target = anchors::target_of(&refs, policy).unwrap();
        let mut frame = fixtures::frame_at(segment, 9_000, SceneId::DanceFloor);
        frame.contrast = Some(-10.0);
        frame.saturation = Some(-5.0);
        let solved = solve(&frame, NodeId::new(), &target, policy, &bounds, 1.0);
        assert_eq!(solved.delta.d_contrast, 0.0);
        assert_eq!(solved.delta.d_saturation, 0.0);
    }

    #[test]
    fn default_damping_is_what_the_contract_says_when_the_scene_has_no_opinion() {
        let table = Consistency::load("version = 9\n[[scene]]\nscene = \"cake\"\n").unwrap();
        assert!((table.scene(SceneId::Cake).damping - DEFAULT_DAMPING).abs() < 1e-6);
    }
}
