//! The salient person, and the matte that makes their edge look right.
//!
//! Section 3's diagram gives this its own branch beside the segmentation, and the reason it is
//! not just "the union of the person classes" is the matte: subject is one of the four classes
//! stored as alpha, it is the one every later phase reaches for first, and it is the one whose
//! boundary a photographer inspects at 100 % zoom.
//!
//! # What "salient" means when there are eleven people in frame
//!
//! It means *everybody who was photographed on purpose*, not "the largest person". A wedding
//! photograph of a couple has two subjects and a mask that picked one of them would make phase
//! 19 light half a couple. What narrows it is phase 06's own work: the person boxes it bound to
//! faces are people the detector found, and the ones with no face bound are bodies in the
//! crowd. When there are no boxes at all, the fallback is a centre-and-focus prior that says so
//! with [`MaskReason::NoFaces`] and carries a confidence low enough that
//! [`crate::contract::mask::AGGRESSIVE_FLOOR`] refuses skin smoothing through it.

use crate::contract::mask::{EdgeQuality, MaskKind, MaskReason};
use crate::face::person::PersonBox;
use crate::face::FaceObservation;
use crate::mask::algebra::{self, Plane};
use crate::mask::segment::Features;
use crate::mask::{matting, trimap, MaskFrame, MaskPlane};

/// The confidence a subject mask built from person boxes gets.
const CONF_BOXED: f32 = 0.88;

/// The confidence the no-boxes fallback gets.
///
/// Below `AGGRESSIVE_FLOOR` squared over a perfect edge, so a fallback subject can carry a
/// local exposure lift and cannot carry skin smoothing. That is not a coincidence - it is the
/// number chosen so the structural consequence holds without a special case anywhere.
const CONF_FALLBACK: f32 = 0.30;

/// How far the centre prior reaches, as a fraction of the frame's short edge.
const PRIOR_RADIUS: f32 = 0.38;

/// Build the subject mask.
///
/// `planes` is what [`crate::mask::segment::run`] produced, so the person classes are already
/// measured and this composes rather than re-measures. Two modules measuring where somebody's
/// hair ends is two answers to the same question.
#[must_use]
pub fn run(
    frame: &MaskFrame,
    planes: &[MaskPlane],
    persons: &[PersonBox],
    faces: &[FaceObservation],
) -> MaskPlane {
    let features = Features::measure(frame);
    let person_classes = [
        MaskKind::Skin,
        MaskKind::Face,
        MaskKind::Hair,
        MaskKind::FacialHair,
        MaskKind::Clothing,
        MaskKind::Dress,
    ];

    let mut coarse = Plane::zeros(frame.width, frame.height);
    let mut any = false;
    for kind in person_classes {
        if let Some(plane) = planes
            .iter()
            .find(|p| p.kind == kind && p.identity.is_none())
        {
            if plane.plane.is_empty() {
                continue;
            }
            any = true;
            coarse = algebra::union(&coarse, &plane.plane);
        }
    }

    if !any {
        return fallback(&features, persons, faces);
    }

    // The person boxes bound the union. A skin component on a guest at the edge of frame is
    // skin; it is not the subject, and phase 19 lighting it would be phase 19 lighting a
    // stranger.
    if !persons.is_empty() || !faces.is_empty() {
        let mut allowed = Plane::zeros(frame.width, frame.height);
        for body in persons {
            paint_box(
                &mut allowed,
                body.bbox.x,
                body.bbox.y,
                body.bbox.w,
                body.bbox.h,
            );
        }
        for face in faces {
            // A face with no bound body still implies a body: the same proportions
            // `segment::clothing` uses, written once there and mirrored here.
            paint_box(
                &mut allowed,
                face.bbox.x - face.bbox.w * 0.5,
                face.bbox.y - face.bbox.h * 0.3,
                face.bbox.w * 2.0,
                face.bbox.h * 4.3,
            );
        }
        coarse = algebra::intersect(&coarse, &allowed);
    }

    let band = trimap::band_radius(&coarse);
    let map = trimap::build(&coarse, band);
    let matted = matting::refine(&features, &coarse, &map);
    let quality = matting::edge_quality(&features, &matted, &map);
    let matted = matted.alpha;

    let mut reasons = vec![MaskReason::Derived, MaskReason::Matted];
    if touches_frame_edge(&matted) {
        reasons.push(MaskReason::ClippedByFrame);
    }
    if map.unknown_count() == 0 {
        reasons.push(MaskReason::TooSmallToMatte);
    }
    if quality < 0.4 {
        reasons.push(MaskReason::LowContrastBoundary);
    }

    MaskPlane {
        kind: MaskKind::Subject,
        identity: None,
        plane: matted,
        confidence: CONF_BOXED,
        edge_quality: quality,
        edge: if quality >= 0.6 {
            EdgeQuality::Matted
        } else {
            EdgeQuality::Soft
        },
        reasons,
    }
}

/// The subject when there are no people classes at all.
///
/// A centre-weighted prior intersected with the sharper half of the frame. Not a guess about
/// *who* - there is nobody to guess about - but a usable region for the flat-lays, the details
/// and the venue shots that make up a fifth of a wedding, and it says what it is.
fn fallback(features: &Features, persons: &[PersonBox], faces: &[FaceObservation]) -> MaskPlane {
    let _ = (persons, faces);
    let mut plane = Plane::zeros(features.w, features.h);
    let cx = features.w as f32 / 2.0;
    let cy = features.h as f32 / 2.0;
    let radius = features.w.min(features.h) as f32 * PRIOR_RADIUS;
    let sharp_floor = features.median_texture;

    for y in 0..i64::from(features.h) {
        for x in 0..i64::from(features.w) {
            let d = (x as f32 - cx).hypot(y as f32 - cy) / radius.max(1.0);
            if d > 1.0 {
                continue;
            }
            // Sharper than the frame's own median, which is phase 09's argument reused: a
            // subject is the part of a photograph the photographer focused on, and "sharp"
            // only means something relative to the rest of this frame.
            if features.texture_at(x, y) >= sharp_floor {
                plane.set(x, y, (1.0 - d).clamp(0.0, 1.0));
            }
        }
    }
    let smooth = algebra::feather(&plane, 0.25);
    MaskPlane {
        kind: MaskKind::Subject,
        identity: None,
        plane: smooth,
        confidence: CONF_FALLBACK,
        edge_quality: 0.35,
        edge: EdgeQuality::Soft,
        reasons: vec![MaskReason::NoFaces, MaskReason::HeadUntrained],
    }
}

fn paint_box(plane: &mut Plane, x: f32, y: f32, w: f32, h: f32) {
    let x0 = ((x * plane.w as f32).floor() as i64).clamp(0, i64::from(plane.w));
    let y0 = ((y * plane.h as f32).floor() as i64).clamp(0, i64::from(plane.h));
    let x1 = (((x + w) * plane.w as f32).ceil() as i64).clamp(x0, i64::from(plane.w));
    let y1 = (((y + h) * plane.h as f32).ceil() as i64).clamp(y0, i64::from(plane.h));
    for py in y0..y1 {
        for px in x0..x1 {
            plane.set(px, py, 1.0);
        }
    }
}

/// True when the region reaches any edge of the frame.
fn touches_frame_edge(plane: &Plane) -> bool {
    let last_x = i64::from(plane.w) - 1;
    let last_y = i64::from(plane.h) - 1;
    (0..i64::from(plane.w)).any(|x| plane.at(x, 0) > 0.5 || plane.at(x, last_y) > 0.5)
        || (0..i64::from(plane.h)).any(|y| plane.at(0, y) > 0.5 || plane.at(last_x, y) > 0.5)
}

#[cfg(test)]
mod tests {
    // The panic family is how a test asserts, and a mask test compares alphas that are exactly
    // zero or exactly one by construction - a painted fixture has no rounding to be tolerant of.
    #![allow(
        clippy::float_cmp,
        clippy::indexing_slicing,
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::assertions_on_constants,
        clippy::uninlined_format_args
    )]
    use super::*;

    fn grey(w: u32, h: u32) -> MaskFrame {
        let mut rgb = Vec::new();
        for y in 0..h {
            for x in 0..w {
                // A little structure so the median texture is not zero.
                let v = 0.4 + 0.1 * (((x / 4 + y / 4) % 2) as f32);
                rgb.extend_from_slice(&[v, v, v]);
            }
        }
        MaskFrame::new(rgb, w, h)
    }

    #[test]
    fn a_frame_with_no_people_falls_back_and_says_so() {
        let frame = grey(64, 48);
        let plane = run(&frame, &[], &[], &[]);
        assert_eq!(plane.kind, MaskKind::Subject);
        assert!(plane.reasons.contains(&MaskReason::NoFaces));
        assert_eq!(plane.confidence, CONF_FALLBACK);
    }

    #[test]
    fn the_fallback_cannot_carry_an_aggressive_operation() {
        // The structural consequence the constant was chosen for.
        let frame = grey(64, 48);
        let plane = run(&frame, &[], &[], &[]);
        assert!(plane.allowance() < crate::contract::mask::AGGRESSIVE_FLOOR);
    }
}
