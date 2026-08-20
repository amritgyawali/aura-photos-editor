//! What is a person rather than a defect, and how the product knows.
//!
//! PHASE-20 section 6.1 - the ethical core of the phase. Section 1 states the rule:
//!
//! > Freckles, moles, scars and birthmarks are identity, not defects.
//!
//! Two mechanisms produce a protect set, and they are different in kind.
//!
//! ## Single frame: the classifier, which here is a measurement
//!
//! [`classify`] reads one candidate and says how permanent it looks and what kind of feature it
//! would be. It is deliberately weak evidence: a dark, round, unsaturated mark is probably a
//! mole, and "probably" is not enough to *unprotect* anything but is enough to protect it,
//! because [`aura_core::contract::retouch::PERMANENT_FLOOR`] is lower than
//! [`aura_core::contract::retouch::TEMPORARY_FLOOR`] and that asymmetry is the whole posture of
//! the phase.
//!
//! ## Across the gallery: the evidence that is actually decisive
//!
//! Section 6.1: "a spot that appears on the same facial coordinate in many frames across hours
//! is permanent; one that appears in a few frames is temporary or transient lighting". That is
//! unique to a gallery-aware product and it is the strongest signal available.
//!
//! The coordinate system is what makes it work. A frame coordinate is useless because the
//! person moves, so every observation is projected into the **face frame**: the origin is the
//! midpoint between the eyes, the x axis is the eye-to-eye line and the unit is the
//! inter-ocular distance. [`to_face_frame`] is that projection, and it is the same
//! normalisation phase 06 alignment and phase 10 expression crops use - two definitions of
//! "the same place on a face" would mean the retoucher and the expression head disagree about
//! which pixels are a cheek.
//!
//! Both of section 6.1 thresholds must hold: [`PERMANENCE_MIN_FRAMES`] and
//! [`PERMANENCE_MIN_SPAN_MIN`]. The count alone would call a burst permanent; the span alone
//! would call one long-lived lighting artefact permanent.
//!
//! ## On this build it finds nothing, and that is honest
//!
//! Phase 06 detector is a placeholder, so there are no identities and no landmarks, so there
//! is no correspondence to accumulate. What survives is [`classify`] and the conservative
//! default. `docs/progress/PHASE-20-EXIT.md` carries it as a condition.

use std::collections::BTreeMap;

use aura_core::contract::composition::Box2;
use aura_core::contract::people::FaceRef;
use aura_core::contract::retouch::{
    ImageId, ProtectedFeature, ProtectedKind, ProtectedSource, PERMANENCE_MIN_FRAMES,
    PERMANENCE_MIN_SPAN_MIN, PERMANENT_FLOOR,
};
use aura_core::IdentityId;

use crate::blemish::Candidate;

/// How close two observations must be, in face-frame units, to be the same feature.
///
/// Six hundredths of the inter-ocular distance - about two millimetres on an adult face. Wide
/// enough to absorb the landmark jitter between two frames of the same person, narrow enough
/// that two different freckles a centimetre apart stay two features.
pub const SAME_FEATURE_RADIUS: f32 = 0.06;

/// One sighting of a mark on one person face.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Observation {
    /// Whose face.
    pub identity: IdentityId,
    /// Which photograph.
    pub image: ImageId,
    /// Where on the face, in face-frame coordinates.
    pub area: Box2,
    /// Minutes since the start of the project, for the span test.
    pub minute: f32,
    /// How permanent it looked in that one frame, `0..1`.
    pub permanent: f32,
    /// What kind of feature it looked like.
    pub kind: ProtectedKind,
}

/// Project a rectangle in frame coordinates into the frame of one face.
///
/// `None` when the detector produced no eye landmarks. **That is a refusal rather than a
/// fallback**: phase 09 rule is that `[[0,0],[0,0]]` means unknown and must never be read as
/// the top-left corner, and a protect row written in a coordinate system nobody can reproduce
/// would protect a random part of every other photograph of that person.
#[must_use]
pub fn to_face_frame(area: Box2, face: &FaceRef) -> Option<Box2> {
    if !face.has_eyes() {
        return None;
    }
    let left = face.eyes[0];
    let right = face.eyes[1];
    let dx = right[0] - left[0];
    let dy = right[1] - left[1];
    let unit = dx.hypot(dy);
    if unit <= f32::EPSILON {
        return None;
    }
    let ox = (left[0] + right[0]) * 0.5;
    let oy = (left[1] + right[1]) * 0.5;
    // The axis is the eye-to-eye line, so a tilted head projects to the same coordinates as a
    // level one. Without the rotation, a protect row from a frame where somebody tipped their
    // head would sit a centimetre from the mark it was written for.
    let cos = dx / unit;
    let sin = dy / unit;
    let project = |x: f32, y: f32| -> (f32, f32) {
        let rx = x - ox;
        let ry = y - oy;
        ((rx * cos + ry * sin) / unit, (ry * cos - rx * sin) / unit)
    };
    let (x0, y0) = project(area.x, area.y);
    let (x1, y1) = project(area.x + area.w, area.y + area.h);
    Some(Box2 {
        x: x0.min(x1),
        y: y0.min(y1),
        w: (x1 - x0).abs().max(1e-4),
        h: (y1 - y0).abs().max(1e-4),
    })
}

/// The inverse: where a face-frame rectangle sits on one photograph.
///
/// Used by the veto, because the protect set is stored per person and the candidates are found
/// per frame, and one of the two has to move into the other coordinate system.
#[must_use]
pub fn to_frame(area: Box2, face: &FaceRef) -> Option<Box2> {
    if !face.has_eyes() {
        return None;
    }
    let left = face.eyes[0];
    let right = face.eyes[1];
    let dx = right[0] - left[0];
    let dy = right[1] - left[1];
    let unit = dx.hypot(dy);
    if unit <= f32::EPSILON {
        return None;
    }
    let ox = (left[0] + right[0]) * 0.5;
    let oy = (left[1] + right[1]) * 0.5;
    let cos = dx / unit;
    let sin = dy / unit;
    let place = |x: f32, y: f32| -> (f32, f32) {
        let sx = x * unit;
        let sy = y * unit;
        (ox + sx * cos - sy * sin, oy + sx * sin + sy * cos)
    };
    let (x0, y0) = place(area.x, area.y);
    let (x1, y1) = place(area.x + area.w, area.y + area.h);
    Some(
        Box2 {
            x: x0.min(x1),
            y: y0.min(y1),
            w: (x1 - x0).abs().max(1e-4),
            h: (y1 - y0).abs().max(1e-4),
        }
        .clamped(),
    )
}

/// What one candidate looks like from a single frame.
///
/// Returns how permanent it looks and what kind of feature it would be. Weak evidence by
/// design - see the module header.
#[must_use]
pub fn classify(candidate: &Candidate) -> (f32, ProtectedKind) {
    let permanent = (1.0 - candidate.temporary).clamp(0.0, 1.0);
    // A large, flat, unsaturated area is a birthmark or a tattoo; a small dark round one is a
    // mole; an elongated one is a scar; a small cluster is freckles. The kind matters because
    // one of them - a tattoo - can never be unprotected, so the classifier is deliberately
    // unwilling to *call* something a tattoo from one frame: only size does it, and only well
    // past the size at which this phase would have touched it anyway.
    let kind = if candidate.too_large {
        ProtectedKind::Birthmark
    } else if candidate.redness < -0.5 * crate::blemish::REDNESS_MARGIN {
        ProtectedKind::Mole
    } else {
        ProtectedKind::Freckle
    };
    (permanent, kind)
}

/// Turn a project worth of observations into a protect set.
///
/// Deterministic: observations are grouped in the order they arrive, which the caller supplies
/// sorted by photograph, and each group keeps the earliest sighting as its evidence crop.
#[must_use]
pub fn accumulate(observations: &[Observation]) -> Vec<ProtectedFeature> {
    let mut clusters: BTreeMap<String, Vec<Observation>> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();

    for observation in observations {
        if observation.permanent < PERMANENT_FLOOR {
            continue;
        }
        let mut placed = false;
        for key in &order {
            let Some(cluster) = clusters.get_mut(key) else {
                continue;
            };
            let Some(first) = cluster.first() else {
                continue;
            };
            if first.identity != observation.identity {
                continue;
            }
            if centre_distance(first.area, observation.area) <= SAME_FEATURE_RADIUS {
                cluster.push(*observation);
                placed = true;
                break;
            }
        }
        if !placed {
            let key = format!(
                "{}:{:.4}:{:.4}",
                observation.identity.to_db(),
                observation.area.x,
                observation.area.y
            );
            order.push(key.clone());
            clusters.insert(key, vec![*observation]);
        }
    }

    let mut out = Vec::new();
    for key in &order {
        let Some(cluster) = clusters.get(key) else {
            continue;
        };
        let Some(first) = cluster.first() else {
            continue;
        };
        let frames = cluster.len() as u32;
        let min = cluster.iter().map(|o| o.minute).fold(f32::MAX, f32::min);
        let max = cluster.iter().map(|o| o.minute).fold(f32::MIN, f32::max);
        let span = (max - min).max(0.0);
        // Both, not either. See the module header.
        if frames < PERMANENCE_MIN_FRAMES || span < PERMANENCE_MIN_SPAN_MIN {
            continue;
        }
        let confidence = cluster.iter().map(|o| o.permanent).sum::<f32>() / frames as f32;
        out.push(ProtectedFeature {
            identity: first.identity,
            kind: first.kind,
            area: first.area,
            confidence: confidence.clamp(0.0, 1.0),
            source: ProtectedSource::CrossFrame,
            frames,
            span_minutes: span,
            first_seen: first.image,
        });
    }
    out
}

/// The distance between the centres of two face-frame rectangles.
fn centre_distance(a: Box2, b: Box2) -> f32 {
    let ax = a.x + a.w * 0.5;
    let ay = a.y + a.h * 0.5;
    let bx = b.x + b.w * 0.5;
    let by = b.y + b.h * 0.5;
    (ax - bx).hypot(ay - by)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::integrity::CropRect;
    use aura_core::{FaceId, PhotoId};

    fn face(eyes: [[f32; 2]; 2]) -> FaceRef {
        FaceRef {
            face_id: FaceId::from_db("fce_00000000-0000-4000-8000-000000000020")
                .expect("a face id"),
            identity_id: None,
            bbox: CropRect {
                x: 0.3,
                y: 0.3,
                w: 0.4,
                h: 0.4,
            },
            eyes,
            area_frac: 0.16,
            centrality: 0.9,
            sharpness: 0.8,
            quality: 0.8,
            votes: true,
        }
    }

    fn identity() -> IdentityId {
        IdentityId::from_db("idt_00000000-0000-4000-8000-000000000020").expect("an identity")
    }

    fn photo(n: u32) -> PhotoId {
        PhotoId::from_db(&format!("pht_00000000-0000-4000-8000-{n:012}")).expect("a photo id")
    }

    fn observation(minute: f32, x: f32, image: u32) -> Observation {
        Observation {
            identity: identity(),
            image: photo(image),
            area: Box2 {
                x,
                y: 0.2,
                w: 0.02,
                h: 0.02,
            },
            minute,
            permanent: 0.8,
            kind: ProtectedKind::Mole,
        }
    }

    #[test]
    fn a_mark_survives_a_tilted_head() {
        // The same mark, seen level and seen with the head tipped. If the projection did not
        // rotate, the second observation would land somewhere else on the face and the protect
        // set would never accumulate.
        let level = face([[0.40, 0.40], [0.60, 0.40]]);
        let tilted = face([[0.40, 0.36], [0.60, 0.44]]);
        let mark_level = Box2 {
            x: 0.44,
            y: 0.50,
            w: 0.02,
            h: 0.02,
        };
        let projected = to_face_frame(mark_level, &level).expect("a projection");
        // Place the same face-frame coordinates back onto the tilted face, then project again.
        let on_tilted = to_frame(projected, &tilted).expect("a placement");
        let round_trip = to_face_frame(on_tilted, &tilted).expect("a projection");
        assert!(
            centre_distance(projected, round_trip) < 0.01,
            "{projected:?} vs {round_trip:?}"
        );
    }

    #[test]
    fn a_face_with_no_landmarks_is_refused_rather_than_guessed() {
        let blind = face([[0.0, 0.0], [0.0, 0.0]]);
        assert!(to_face_frame(CropRect::FULL, &blind).is_none());
        assert!(to_frame(CropRect::FULL, &blind).is_none());
    }

    #[test]
    fn four_frames_across_an_hour_is_permanent_and_a_burst_is_not() {
        let across_the_day = vec![
            observation(0.0, 0.10, 1),
            observation(30.0, 0.104, 2),
            observation(90.0, 0.098, 3),
            observation(200.0, 0.101, 4),
        ];
        let found = accumulate(&across_the_day);
        assert_eq!(found.len(), 1);
        assert!(found[0].is_well_evidenced());
        assert_eq!(found[0].source, ProtectedSource::CrossFrame);

        let burst = vec![
            observation(0.0, 0.10, 1),
            observation(0.2, 0.104, 2),
            observation(0.4, 0.098, 3),
            observation(0.6, 0.101, 4),
            observation(0.8, 0.099, 5),
        ];
        assert!(
            accumulate(&burst).is_empty(),
            "a burst was called permanent"
        );
    }

    #[test]
    fn two_marks_a_centimetre_apart_stay_two_features() {
        let mut observations = Vec::new();
        for (index, minute) in [0.0, 40.0, 100.0, 220.0].into_iter().enumerate() {
            observations.push(observation(minute, 0.10, index as u32 + 1));
            observations.push(observation(minute, 0.30, index as u32 + 10));
        }
        assert_eq!(accumulate(&observations).len(), 2);
    }

    #[test]
    fn a_mark_that_looked_temporary_never_reaches_the_protect_set() {
        let mut observations = Vec::new();
        for (index, minute) in [0.0, 40.0, 100.0, 220.0].into_iter().enumerate() {
            let mut o = observation(minute, 0.10, index as u32 + 1);
            o.permanent = 0.2;
            observations.push(o);
        }
        assert!(accumulate(&observations).is_empty());
    }
}
