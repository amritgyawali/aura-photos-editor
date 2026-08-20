//! The shadow under the eyes, lifted by less than somebody would notice.
//!
//! PHASE-20 section 2.1: "luminance and chroma correction in the periorbital region with strict
//! caps and no texture loss". Section 6.4 says why the caps are strict:
//!
//! > Under-eye correction is capped hard (typical luma lift <= 0.25 EV inside the periorbital
//! > mask) because over-correction is the classic tell of automated retouching.
//!
//! The cap in [`aura_core::contract::retouch::MAX_UNDEREYE_LUMA_EV`] is **hard rather than
//! typical**, and that is a deliberate reading: a cap the solver may exceed on an unusual frame
//! is a cap that is exceeded on exactly the frames somebody looks at closely.
//!
//! ## Both halves are measured against the skin around the eye
//!
//! There is no ideal under-eye luminance in this module and no ideal under-eye colour. What is
//! corrected is the **separation** between the tear trough and the cheek beside it, which is
//! what a dark circle actually is - and measuring it that way is what keeps the correction
//! identical in mechanism across skin tones. Phase 15 wrote this rule about white balance and
//! `docs/skin-fairness.md` says it in the product voice.
//!
//! ## It never corrects the whole way
//!
//! [`CORRECTION_SHARE`] is the fraction of the measured separation the solver aims at. A
//! periorbital region lifted until it matches the cheek is a face with no eye sockets, which is
//! the second most recognisable artefact of automated retouching after plastic skin.

use aura_core::contract::people::FaceRef;
use aura_core::contract::retouch::{MAX_UNDEREYE_CHROMA, MAX_UNDEREYE_LUMA_EV};
use aura_render::retouch::{UNDEREYE_DROP, UNDEREYE_WIDTH};

use crate::blemish::FaceCrop;

/// How much of the measured separation the correction aims at.
///
/// Three fifths. Enough that nobody comments on the shadow, far enough short of one that the
/// socket is still there. Section 6.4 word is "improvement but not retouching", and this is the
/// number that decides which of the two it reads as.
pub const CORRECTION_SHARE: f32 = 0.60;

/// The smallest separation worth correcting, as a fraction of the luminance of the skin.
///
/// Three per cent. Below this the shadow is not a dark circle - it is the ordinary modelling of
/// an eye socket, and lifting it takes the shape out of the face.
pub const MIN_SEPARATION: f32 = 0.03;

/// What the correction decided.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnderEyeDecision {
    /// Luminance lift in stops, bounded by [`MAX_UNDEREYE_LUMA_EV`].
    pub luma_ev: f32,
    /// Chroma separation reduction, `0..1`, bounded by [`MAX_UNDEREYE_CHROMA`].
    pub chroma: f32,
    /// True when the solver wanted more than the cap allowed.
    ///
    /// Stored so the panel can say the correction stopped rather than that it was small, which
    /// is [`aura_core::contract::retouch::RetouchCode::UnderEyeCapped`].
    pub capped: bool,
    /// The measured separation, for the panel and for the eval harness.
    pub separation: f32,
}

/// Decide how much to lift and de-tint one face periorbital region.
///
/// `None` when there is nothing to correct, when the landmarks are missing, or when there is no
/// skin under the region. All three are withdrawals the plan reports rather than silences.
#[must_use]
pub fn solve(crop: &FaceCrop, face: &FaceRef, strength: f32) -> Option<UnderEyeDecision> {
    if !face.has_eyes() || strength <= 0.0 {
        return None;
    }
    let (width, height) = (crop.width, crop.height);
    if width == 0 || height == 0 {
        return None;
    }

    // Landmarks are in frame coordinates and the crop is a window on the frame, so the eyes
    // move into crop coordinates once, here.
    let to_crop = |point: [f32; 2]| -> (f32, f32) {
        (
            (point[0] - crop.bounds.x) / crop.bounds.w.max(1e-6) * width as f32,
            (point[1] - crop.bounds.y) / crop.bounds.h.max(1e-6) * height as f32,
        )
    };
    let left = to_crop(face.eyes[0]);
    let right = to_crop(face.eyes[1]);
    let separation_px = (right.0 - left.0).hypot(right.1 - left.1);
    if separation_px <= 1.0 {
        return None;
    }

    let half_w = separation_px * UNDEREYE_WIDTH * 0.5;
    let drop = separation_px * UNDEREYE_DROP;

    let mut region_luma = 0.0f64;
    let mut region_weight = 0.0f64;
    let mut region_chroma = [0.0f64; 3];
    let mut ring_luma = 0.0f64;
    let mut ring_weight = 0.0f64;
    let mut ring_chroma = [0.0f64; 3];

    for eye in [left, right] {
        let cx = eye.0;
        let cy = eye.1 + drop * 0.5;
        let x0 = (cx - half_w * 2.0).floor().max(0.0) as usize;
        let x1 = (cx + half_w * 2.0).ceil().min(width as f32) as usize;
        let y0 = eye.1.floor().max(0.0) as usize;
        let y1 = (eye.1 + drop * 2.0).ceil().min(height as f32) as usize;

        for y in y0..y1 {
            for x in x0..x1 {
                let index = y * width + x;
                let coverage = f64::from(crop.skin_at(index));
                if coverage <= 0.0 {
                    continue;
                }
                let luma = f64::from(crop.luma_at(index).max(1e-6));
                let rgb = crop.rgb_at(index);
                let ex = (x as f32 - cx) / half_w.max(1.0);
                let ey = (y as f32 - cy) / (drop * 0.5).max(1.0);
                let inside = (ex * ex + ey * ey).sqrt() <= 1.0;

                let (luma_acc, weight_acc, chroma_acc) = if inside {
                    (&mut region_luma, &mut region_weight, &mut region_chroma)
                } else {
                    (&mut ring_luma, &mut ring_weight, &mut ring_chroma)
                };
                *luma_acc += luma * coverage;
                *weight_acc += coverage;
                for (slot, value) in chroma_acc.iter_mut().zip(rgb.iter()) {
                    *slot += f64::from(*value) / luma * coverage;
                }
            }
        }
    }

    if region_weight <= 0.0 || ring_weight <= 0.0 {
        return None;
    }
    let region = (region_luma / region_weight) as f32;
    let ring = (ring_luma / ring_weight) as f32;
    if ring <= 1e-6 {
        return None;
    }

    let separation = ((ring - region) / ring).max(0.0);
    if separation < MIN_SEPARATION {
        return None;
    }

    // What a full correction would cost, in stops, and what this phase is willing to spend.
    let wanted_ev = (ring / region.max(1e-6)).log2() * CORRECTION_SHARE * strength;
    let luma_ev = wanted_ev.min(MAX_UNDEREYE_LUMA_EV);

    // The chroma half: how far the colour under the eye sits from the colour of the skin around
    // it. Reduced by a share of itself, never moved to a target.
    let mut chroma_distance = 0.0f32;
    for (inside, outside) in region_chroma.iter().zip(ring_chroma.iter()) {
        let inside = (inside / region_weight) as f32;
        let outside = (outside / ring_weight) as f32;
        chroma_distance += (inside - outside).abs();
    }
    let wanted_chroma = chroma_distance * CORRECTION_SHARE * strength;
    let chroma = wanted_chroma.min(MAX_UNDEREYE_CHROMA);

    Some(UnderEyeDecision {
        luma_ev,
        chroma,
        capped: wanted_ev > MAX_UNDEREYE_LUMA_EV + 1e-4
            || wanted_chroma > MAX_UNDEREYE_CHROMA + 1e-4,
        separation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;

    #[test]
    fn a_dark_circle_is_lifted_and_the_lift_is_bounded() {
        let (crop, face) = fixtures::face_with_dark_circles();
        let decision = solve(&crop, &face, 1.0).expect("a correction");
        assert!(decision.luma_ev > 0.0);
        assert!(decision.luma_ev <= MAX_UNDEREYE_LUMA_EV + 1e-6);
        assert!(decision.chroma <= MAX_UNDEREYE_CHROMA + 1e-6);
        assert!(decision.separation > MIN_SEPARATION);
    }

    #[test]
    fn an_even_face_needs_no_correction() {
        let (crop, face) = fixtures::even_face_with_eyes();
        assert!(solve(&crop, &face, 1.0).is_none());
    }

    #[test]
    fn a_face_with_no_landmarks_is_skipped() {
        let (crop, mut face) = fixtures::face_with_dark_circles();
        face.eyes = [[0.0, 0.0], [0.0, 0.0]];
        assert!(solve(&crop, &face, 1.0).is_none());
    }

    #[test]
    fn a_deep_shadow_reports_that_it_was_capped() {
        let (crop, face) = fixtures::face_with_deep_circles();
        let decision = solve(&crop, &face, 1.0).expect("a correction");
        assert!(decision.capped, "a two-stop shadow did not report a cap");
        assert!((decision.luma_ev - MAX_UNDEREYE_LUMA_EV).abs() < 1e-5);
    }

    #[test]
    fn strength_scales_the_correction_without_ever_passing_the_cap() {
        let (crop, face) = fixtures::face_with_dark_circles();
        let gentle = solve(&crop, &face, 0.3).expect("a correction");
        let full = solve(&crop, &face, 1.0).expect("a correction");
        assert!(gentle.luma_ev < full.luma_ev);
        assert!(full.luma_ev <= MAX_UNDEREYE_LUMA_EV + 1e-6);
    }
}
