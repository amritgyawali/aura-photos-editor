//! Lifting a face without making it glow, and lifting twelve of them without making one of
//! them look pasted in.
//!
//! PHASE-19 section 6.1. Four rules, and the fourth is the one that is hard:
//!
//! 1. modulate by a luminosity mask - [`crate::local::luminosity`] owns that;
//! 2. cap the lift at the scene's noise budget, dynamically;
//! 3. feather by face size;
//! 4. **solve all faces jointly toward the same target band with a maximum inter-face
//!    difference, so nobody looks pasted in**.
//!
//! ## Why the joint solve is not "light each face and then check"
//!
//! Because checking afterwards has only one remedy - pulling somebody back - and pulling
//! somebody back to satisfy a spread constraint means the person who was hardest to light
//! decides the result for everybody. A family formal where grandmother is in shadow and the
//! rest of the row is fine would either leave her dark or drag the whole row down.
//!
//! What happens instead is that a **common target** is agreed first, weighted by how
//! confidently each face can be moved toward it, and every face is solved toward that. Faces
//! that cannot reach it - because the noise cap or the highlight cap stops them - pull the
//! common target toward themselves rather than being abandoned at it, and the loop settles.
//! Three iterations, because the update is a weighted mean and a weighted mean converges
//! faster than anyone re-derives the argument for four.
//!
//! ## The noise cap is phase 09's number, not a new one
//!
//! Section 6.1: "a face lifted 1.2 EV in a high-ISO reception would reveal noise, so the cap
//! is dynamic". The measurement behind it is `IntegrityResult::noise` - phase 09 already
//! measured how noisy this frame is against its own body and ISO - and the scene's
//! `shadow_lift_scale` from phase 15's target table already says how much of that this kind of
//! photograph will accept. This module multiplies them. It does not measure noise, because
//! `IntegrityService` is the only way to ask whether a frame worked and that rule has held
//! for ten phases.

use aura_core::contract::local::{FaceLightDelta, MAX_FACE_LIFT_EV, MAX_FACE_PULL_EV};

use crate::local::luminosity;
use crate::local::measure::{apply_ev, ev_between};

/// How many rounds the joint solve runs.
///
/// Three. The update is a weighted mean of the reachable targets, which is a contraction, and
/// the third round moves the common target by less than a thousandth of a luminance unit on
/// every fixture in `crate::local::fixtures`.
pub const JOINT_ROUNDS: usize = 3;

/// The lift a completely clean frame allows, in stops.
///
/// The ceiling the noise cap starts from before phase 09's measurement pulls it down. Equal
/// to [`MAX_FACE_LIFT_EV`] on purpose: a frame with no measurable noise has no reason to be
/// capped below the contract's own ceiling.
pub const CLEAN_FRAME_CAP_EV: f32 = MAX_FACE_LIFT_EV;

/// One face, as the solver sees it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceInput {
    /// The face's mean perceptual luminance now.
    pub luma: f32,
    /// The face's 95th-percentile luminance, for the highlight cap.
    pub p95_luma: f32,
    /// The face's shorter side as a fraction of the frame's shorter side.
    pub side: f32,
    /// How prominent the face is, `0..1`. Phase 06's number.
    pub prominence: f32,
    /// The mask confidence and edge quality this face's mask carries, `0..1`.
    pub mask_scale: f32,
}

/// What bounds a lift on this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Caps {
    /// The dynamic noise cap, in stops. See the module header.
    pub noise_ev: f32,
    /// The scene policy's own ceiling, in stops.
    pub scene_ev: f32,
    /// The luminance above which a face's bright end is considered at risk.
    ///
    /// Not 1.0: skin that reaches 0.94 in an 8-bit proxy has lost its specular roll-off
    /// already, and a lift that takes it there has flattened the face whether or not a
    /// histogram calls it clipped.
    pub highlight_ceiling: f32,
}

impl Caps {
    /// The default ceiling for the bright end of a face.
    pub const DEFAULT_HIGHLIGHT_CEILING: f32 = 0.94;

    /// Build the caps from phase 09's noise reading and phase 15's scene scale.
    ///
    /// `noise` is `IntegrityResult::noise`, `0..1`, where one is as noisy as this phase has a
    /// name for. `shadow_scale` is the scene's `shadow_lift_scale` from the exposure target
    /// table: above one means this scene accepts a noisier lift than the body's baseline.
    #[must_use]
    pub fn from_noise(noise: f32, shadow_scale: f32, scene_ev: f32) -> Self {
        let clean = 1.0 - noise.clamp(0.0, 1.0);
        // Quadratic rather than linear: noise is not visible at all until the shadows are
        // lifted past a point, and then it is visible very quickly. A linear cap spends most
        // of its range on frames where the answer does not matter.
        let noise_ev = (CLEAN_FRAME_CAP_EV * clean * clean * shadow_scale.clamp(0.4, 1.6))
            .clamp(0.0, CLEAN_FRAME_CAP_EV);
        Self {
            noise_ev,
            scene_ev: scene_ev.clamp(0.0, MAX_FACE_LIFT_EV),
            highlight_ceiling: Self::DEFAULT_HIGHLIGHT_CEILING,
        }
    }

    /// The smallest of the caps that apply to a lift.
    #[must_use]
    pub fn lift_ceiling(&self) -> f32 {
        self.noise_ev.min(self.scene_ev).min(MAX_FACE_LIFT_EV)
    }
}

/// What one face may move by, before the group is considered.
///
/// Two rules, and the second is the one that took a failing test to find.
///
/// The highlight cap is **per face** rather than per frame: a face turned away from a window
/// and a face turned into it have different bright ends, and one number for both would either
/// stop the dark one being lifted or let the bright one clip.
///
/// And **a face is only ever moved between where it is and where the band is.** The common
/// target the joint solve agrees on can sit anywhere between the group's members; without
/// this clamp, one blown face in a family formal drags the target above the band and
/// everybody else gets *brightened past* the scene's own target to meet them. Lifting a face
/// beyond the band is not lighting it toward the band, and the constraint makes the joint
/// solve something that can only ever reduce a move rather than create one.
fn reachable(face: &FaceInput, caps: &Caps, target: f32, band: f32) -> f32 {
    let aim = target.clamp(face.luma.min(band), face.luma.max(band));
    let wanted = ev_between(face.luma, aim);
    if wanted <= 0.0 {
        return wanted.max(-MAX_FACE_PULL_EV);
    }
    let highlight_room = ev_between(face.p95_luma, caps.highlight_ceiling).max(0.0);
    wanted.min(caps.lift_ceiling()).min(highlight_room)
}

/// The result of solving one frame's faces.
///
/// Four booleans rather than a bitset or an enum, because each of them is a separate
/// question the panel asks separately - "was this a group", "was somebody held back", "did
/// the noise stop it", "did the highlights stop it" - and folding them would make every
/// caller re-derive the four from one.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Solved {
    /// One delta per input face, in the same order.
    pub deltas: Vec<FaceLightDelta>,
    /// The common target the group settled on.
    pub common_target: f32,
    /// True when more than one face was solved and they were solved together.
    pub joint: bool,
    /// True when at least one face was held back to keep the group consistent.
    pub spread_capped: bool,
    /// True when at least one lift stopped at the noise cap.
    pub noise_capped: bool,
    /// True when at least one lift stopped at the highlight cap.
    pub highlight_capped: bool,
}

/// Solve every face in one frame toward a band.
///
/// `band` is the scene's own luminance target from phase 15's table - the same number the
/// global exposure was set against, so a face that phase 15 already put in the band arrives
/// here needing nothing and leaves with a no-op delta.
///
/// `strength` is the scene policy's face-lighting strength after the governor has had its
/// say, `0..1`. It scales the *move*, not the target: a scene at half strength lifts a face
/// half of the way to where it should be, which is a defensible edit, rather than lifting it
/// all the way to a target halfway there, which is a different and worse claim.
#[must_use]
pub fn solve(faces: &[FaceInput], band: f32, caps: &Caps, strength: f32) -> Solved {
    let mut out = Solved {
        deltas: Vec::with_capacity(faces.len()),
        common_target: band,
        joint: faces.len() > 1,
        spread_capped: false,
        noise_capped: false,
        highlight_capped: false,
    };
    if faces.is_empty() {
        return out;
    }

    // Round one: what can each face actually reach, aiming at the band?
    let mut target = band;
    for _ in 0..JOINT_ROUNDS {
        let mut weighted = 0.0f32;
        let mut weight = 0.0f32;
        for face in faces {
            let ev = reachable(face, caps, target, band);
            let landed = apply_ev(face.luma, ev);
            // Weight by prominence *and* by mask quality: a face nobody can mask reliably
            // should not be the one that decides where the group lands.
            let w = (face.prominence.clamp(0.05, 1.0) * face.mask_scale.max(0.05)).max(1e-3);
            weighted += landed * w;
            weight += w;
        }
        if weight <= f32::EPSILON {
            break;
        }
        target = weighted / weight;
    }
    out.common_target = target;

    for face in faces {
        let full_ev = reachable(face, caps, target, band);
        // Measured against the *band* rather than against the converged common target. The
        // common target has already absorbed the caps - that is what makes it reachable - so
        // comparing against it would report that nothing was ever capped, which is exactly
        // what the first version of this did and what the noise-cap test caught.
        let wanted = ev_between(face.luma, band);
        if full_ev > 0.0 && (wanted - full_ev) > 1e-3 {
            // Something stopped it. Which one is what the panel shows.
            let highlight_room = ev_between(face.p95_luma, caps.highlight_ceiling).max(0.0);
            if highlight_room <= caps.lift_ceiling() {
                out.highlight_capped = true;
            } else {
                out.noise_capped = true;
            }
        }
        let ev = full_ev * strength.clamp(0.0, 1.0) * face.mask_scale.clamp(0.0, 1.0);
        let mut delta = FaceLightDelta {
            exposure_ev: 0.0,
            shadows: 0,
            highlights: 0,
            feather: luminosity::feather_for(face.side),
            luma_before: face.luma,
            // The *band*, not the group's converged target. `FaceLightDelta::luma_target` is
            // what the scene wanted, so `was_capped` means "did not reach the band" and the
            // panel can say "wanted 0.50, reached 0.20". The group's own target is in
            // `Solved::common_target`, where the one caller that needs it looks.
            luma_target: band,
            luma_after: apply_ev(face.luma, ev),
            noise_cap_ev: caps.lift_ceiling(),
            mask_scale: face.mask_scale,
        };
        delta = luminosity::apply_split(delta, ev);
        out.deltas.push(delta);
    }

    out.spread_capped = enforce_spread(&mut out.deltas);
    out
}

/// Pull the outliers back toward the group.
///
/// **The last thing that runs, and the only thing that may undo a lift.** The joint solve
/// usually makes this a no-op; it does not always, because a face that could not be lifted at
/// all pulls the common target down and a face that was already bright then sits above it.
///
/// Two properties, and the group-fairness guarantee in
/// [`aura_core::contract::local::LocalLightPlan::group_is_fair`] is written against both:
///
/// * **it never brightens.** A face is only ever moved down toward the group, so a fairness
///   rule cannot become a second lighting rule with different arithmetic;
/// * **it never darkens a face below where the photograph put it.** It may give back a lift
///   AURA applied and nothing more, which is what stops one person nobody could light from
///   deciding the brightness of everybody else.
///
/// The consequence is that the spread is not always closed to
/// [`aura_core::contract::local::MAX_INTER_FACE_SPREAD`], and that is deliberate: a frame
/// where it cannot be closed is a frame where closing it would mean darkening people, and the
/// plan says [`aura_core::contract::local::LocalCode::GroupSpreadCapped`] instead.
fn enforce_spread(deltas: &mut [FaceLightDelta]) -> bool {
    use aura_core::contract::local::MAX_INTER_FACE_SPREAD;
    if deltas.len() < 2 {
        return false;
    }
    let mut lo = f32::MAX;
    for delta in deltas.iter() {
        lo = lo.min(delta.luma_after);
    }
    let ceiling = lo + MAX_INTER_FACE_SPREAD;
    let mut capped = false;
    for delta in deltas.iter_mut() {
        if delta.luma_after <= ceiling {
            continue;
        }
        // Two floors, and both of them matter. The first is the ceiling itself. The second is
        // **where the face started**: fairness may give back a lift AURA applied, and it may
        // never darken a face the photographer's own exposure put there. Without the second
        // floor, one person nobody could light decides the brightness of everybody else, and
        // a family formal comes back uniformly two stops down.
        let floor = delta.luma_before.min(delta.luma_after);
        let wanted = ceiling.max(floor);
        if wanted >= delta.luma_after - 1e-4 {
            continue;
        }
        capped = true;
        let ev = ev_between(delta.luma_before, wanted).clamp(-MAX_FACE_PULL_EV, MAX_FACE_LIFT_EV);
        delta.luma_after = apply_ev(delta.luma_before, ev);
        *delta = luminosity::apply_split(*delta, ev);
    }
    capped
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::local::MAX_INTER_FACE_SPREAD;

    fn face(luma: f32) -> FaceInput {
        FaceInput {
            luma,
            p95_luma: luma + 0.15,
            side: 0.20,
            prominence: 0.8,
            mask_scale: 1.0,
        }
    }

    fn clean() -> Caps {
        Caps::from_noise(0.0, 1.0, MAX_FACE_LIFT_EV)
    }

    #[test]
    fn a_face_already_in_the_band_is_left_alone() {
        let solved = solve(&[face(0.50)], 0.50, &clean(), 1.0);
        assert!(solved.deltas[0].is_noop(), "{:?}", solved.deltas[0]);
    }

    #[test]
    fn a_dark_face_is_lifted_toward_the_band() {
        let solved = solve(&[face(0.22)], 0.50, &clean(), 1.0);
        let delta = solved.deltas[0];
        assert!(delta.exposure_ev > 0.0 || delta.shadows > 0);
        assert!(delta.luma_after > delta.luma_before);
        assert!(delta.luma_after <= 0.51);
    }

    #[test]
    fn a_noisy_frame_caps_the_lift_and_says_so() {
        let noisy = Caps::from_noise(0.75, 1.0, MAX_FACE_LIFT_EV);
        assert!(noisy.lift_ceiling() < clean().lift_ceiling());
        let solved = solve(&[face(0.12)], 0.50, &noisy, 1.0);
        assert!(solved.noise_capped, "the cap fired but nothing recorded it");
        assert!(solved.deltas[0].was_capped());
    }

    #[test]
    fn a_group_that_can_be_evened_ends_inside_the_documented_spread() {
        // The ordinary case: four faces within reach of one band.
        let faces = [face(0.36), face(0.44), face(0.50), face(0.52)];
        let solved = solve(&faces, 0.50, &clean(), 1.0);
        let mut lo = f32::MAX;
        let mut hi = f32::MIN;
        for delta in &solved.deltas {
            lo = lo.min(delta.luma_after);
            hi = hi.max(delta.luma_after);
        }
        assert!(
            hi - lo <= MAX_INTER_FACE_SPREAD + 1e-4,
            "a reachable group ended {:.3} apart",
            hi - lo
        );
    }

    #[test]
    fn a_group_nobody_could_even_is_still_made_more_even() {
        // Section 10.1's group-fairness criterion on the frame that shows what it can and
        // cannot promise: a family formal where one person is two stops down under a doorway
        // and the noise cap will not lift them the whole way.
        let faces = [face(0.14), face(0.46), face(0.50), face(0.52)];
        let solved = solve(&faces, 0.50, &clean(), 1.0);
        let mut lo = f32::MAX;
        let mut hi = f32::MIN;
        for delta in &solved.deltas {
            lo = lo.min(delta.luma_after);
            hi = hi.max(delta.luma_after);
        }
        let before = 0.52f32 - 0.14;
        assert!(
            hi - lo < before,
            "the lighting did not make the group more even: {:.3} became {:.3}",
            before,
            hi - lo
        );
        assert!(solved.joint);
        assert!(
            solved.noise_capped || solved.deltas.iter().any(FaceLightDelta::was_capped),
            "the frame nobody could even did not say what stopped it"
        );
    }

    #[test]
    fn the_fairness_rule_never_brightens_a_face_it_did_not_lift() {
        let faces = [face(0.50), face(0.52), face(0.90)];
        let solved = solve(&faces, 0.50, &clean(), 1.0);
        // The bright face may be pulled down toward the group; nobody may be pushed up to
        // meet it.
        for delta in &solved.deltas {
            if delta.luma_before >= 0.50 {
                assert!(
                    delta.luma_after <= delta.luma_before + 1e-3,
                    "a face at {} was brightened to {}",
                    delta.luma_before,
                    delta.luma_after
                );
            }
        }
    }

    #[test]
    fn strength_scales_the_move_rather_than_the_target() {
        let full = solve(&[face(0.20)], 0.50, &clean(), 1.0);
        let half = solve(&[face(0.20)], 0.50, &clean(), 0.5);
        assert!((half.common_target - full.common_target).abs() < 1e-4);
        assert!(half.deltas[0].luma_after < full.deltas[0].luma_after);
        assert!(half.deltas[0].luma_after > 0.20);
    }

    #[test]
    fn an_unmaskable_face_is_barely_moved_and_does_not_decide_the_group() {
        let mut ghost = face(0.10);
        ghost.mask_scale = 0.05;
        ghost.prominence = 1.0;
        let solved = solve(&[ghost, face(0.50), face(0.50)], 0.50, &clean(), 1.0);
        assert!(
            solved.common_target > 0.40,
            "one unmaskable face dragged the whole group down to {}",
            solved.common_target
        );
    }

    #[test]
    fn a_bright_face_is_pulled_down_but_never_past_the_limit() {
        let solved = solve(&[face(0.88)], 0.50, &clean(), 1.0);
        let delta = solved.deltas[0];
        assert!(delta.exposure_ev < 0.0);
        assert!(delta.exposure_ev >= -MAX_FACE_PULL_EV - 1e-6);
        assert_eq!(delta.shadows, 0, "a pull-down must not deepen the face");
    }

    #[test]
    fn no_faces_is_an_empty_answer_rather_than_an_error() {
        let solved = solve(&[], 0.50, &clean(), 1.0);
        assert!(solved.deltas.is_empty());
        assert!(!solved.joint);
    }
}
