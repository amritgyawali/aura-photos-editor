//! Where each face is looking, measured from geometry rather than predicted.
//!
//! ## Why this is not a model output
//!
//! Section 8 step 5 asks for gaze estimation as its own step, and section 2.1 wants four
//! targets: camera, partner, officiant, away. Every one of those is a *relation between
//! two positions in the frame*, and phase 06 already hands over both: `FaceRef::bbox` and
//! the two eye landmarks the ADR-0019 amendment added.
//!
//! So gaze is computed here. A head that emitted a gaze slot would be guessing with
//! authority about something the geometry answers, and it would be one more output to
//! retrain when the vocabulary changes.
//!
//! What is genuinely lost by not using a model is **eyeball direction**. This module
//! measures where the *head* is pointed - yaw from the eye midpoint's offset inside the
//! face box, roll from the eye line's angle - and a person can look sideways without
//! turning their head. The mitigation is that every threshold here is set on the
//! conservative side: a face that is not clearly turned reads as
//! [`GazeTarget::Camera`], which is the answer that claims least, and
//! [`GazeTarget::Unknown`] is what a face with no landmarks gets so that a missing
//! measurement is never mistaken for a person looking away.
//!
//! That is condition C3 of the phase 10 exit report, and it is bounded rather than
//! open-ended: the reaction linker weighs gaze as one term of several, and no flag, score
//! or delivery decision turns on it alone.

use aura_core::contract::emotion::GazeTarget;
use aura_core::contract::people::FaceRef;
use aura_core::{IdentityId, SceneId};

/// How far the eye midpoint must sit from the face box's centre before the head reads as
/// turned, as a fraction of the box's width.
///
/// A face looking straight at the lens has its eye midpoint near the horizontal centre of
/// its own box. Turning the head moves the visible eye pair towards the near edge, because
/// the far side of the face is foreshortened. Twelve percent is about fifteen degrees of
/// yaw at this product's face scale, and it is set where it is because below it the
/// measurement is landmark noise.
pub const TURNED_FRACTION: f32 = 0.12;

/// How close two faces must be, as a multiple of the wider face's width, before they can
/// be looking at *each other* rather than merely in each other's direction.
///
/// Four widths. Two people in a mutual-gaze photograph are within arm's length of one
/// another; two people across a marquee happen to be facing the same way.
pub const MUTUAL_REACH: f32 = 4.0;

/// How level two faces must be before a mutual gaze is claimed, as a fraction of the
/// wider face's height.
///
/// Mutual gaze is two people at roughly one height looking towards each other. A guest
/// standing behind and above a seated couple faces the same direction and is not looking
/// at either of them, and without this the reaction linker would call every crowd a
/// conversation.
pub const MUTUAL_LEVEL: f32 = 1.25;

/// Which way a face is turned, in the frame's coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Turn {
    /// Towards the lens, or not turned enough to say.
    Forward,
    /// Turned towards the left of the frame.
    Left,
    /// Turned towards the right of the frame.
    Right,
}

/// Which way one face is turned.
///
/// The eye midpoint's horizontal offset from the centre of the face box, normalised by
/// the box width. Positive is towards the right of the frame.
///
/// **The sign is the thing that is easy to get backwards.** When a head turns to the
/// frame's right, the visible eyes move *left* within the box, because the right side of
/// the face is foreshortened away. So the offset is negated. `turn_direction_is_signed`
/// in the eval harness asserts it on a synthetic pair.
#[must_use]
pub fn turn_of(face: &FaceRef) -> Turn {
    if !face.has_eyes() || face.bbox.w <= f32::EPSILON {
        return Turn::Forward;
    }
    let midpoint = f32::midpoint(face.eyes[0][0], face.eyes[1][0]);
    let centre = face.bbox.x + face.bbox.w / 2.0;
    let offset = -(midpoint - centre) / face.bbox.w;
    if offset > TURNED_FRACTION {
        Turn::Right
    } else if offset < -TURNED_FRACTION {
        Turn::Left
    } else {
        Turn::Forward
    }
}

/// True when `looker` is turned towards `target`'s position in the frame.
///
/// Two conditions, and both are needed: the head is turned the right way, and the target
/// is actually on that side. A face turned left with nobody to its left is looking at
/// something out of frame.
#[must_use]
pub fn faces_towards(looker: &FaceRef, target: &FaceRef) -> bool {
    let looker_x = looker.bbox.x + looker.bbox.w / 2.0;
    let target_x = target.bbox.x + target.bbox.w / 2.0;
    match turn_of(looker) {
        Turn::Forward => false,
        Turn::Left => target_x < looker_x,
        Turn::Right => target_x > looker_x,
    }
}

/// True when two faces are near enough and level enough to be looking at each other.
#[must_use]
pub fn within_reach(a: &FaceRef, b: &FaceRef) -> bool {
    let width = a.bbox.w.max(b.bbox.w);
    let height = a.bbox.h.max(b.bbox.h);
    if width <= f32::EPSILON || height <= f32::EPSILON {
        return false;
    }
    let ax = a.bbox.x + a.bbox.w / 2.0;
    let ay = a.bbox.y + a.bbox.h / 2.0;
    let bx = b.bbox.x + b.bbox.w / 2.0;
    let by = b.bbox.y + b.bbox.h / 2.0;
    (ax - bx).abs() <= width * MUTUAL_REACH && (ay - by).abs() <= height * MUTUAL_LEVEL
}

/// Where one face is looking, given everybody else in the frame.
///
/// The four targets are decided in the order they are argued in, and the order matters:
///
/// 1. **No landmarks** is [`GazeTarget::Unknown`], never [`GazeTarget::Away`]. A missing
///    measurement must not become evidence.
/// 2. **Not turned** is [`GazeTarget::Camera`]. The conservative reading: a face pointed
///    at the lens is the default state of a photographed face.
/// 3. **Turned towards the other half of the couple** is [`GazeTarget::Partner`], and it
///    requires `couple` to hold both identities. Without a confirmed couple the case
///    cannot arise, which is correct - "partner" is a claim about who two people are.
/// 4. **Turned, in a scene with somebody speaking, towards a face that is not the
///    partner** is [`GazeTarget::Officiant`]. The scene list is deliberately short; see
///    [`has_speaker`].
/// 5. **Turned, otherwise** is [`GazeTarget::Away`].
#[must_use]
pub fn target_for(
    face: &FaceRef,
    others: &[FaceRef],
    couple: &[IdentityId],
    scene: SceneId,
) -> GazeTarget {
    if !face.has_eyes() {
        return GazeTarget::Unknown;
    }
    if turn_of(face) == Turn::Forward {
        return GazeTarget::Camera;
    }

    let is_couple = |candidate: &FaceRef| {
        candidate
            .identity_id
            .is_some_and(|identity| couple.contains(&identity))
    };
    let looking_at_partner = couple.len() >= 2
        && is_couple(face)
        && others.iter().any(|other| {
            other.face_id != face.face_id
                && is_couple(other)
                && faces_towards(face, other)
                && within_reach(face, other)
        });
    if looking_at_partner {
        return GazeTarget::Partner;
    }

    if has_speaker(scene)
        && others
            .iter()
            .any(|other| other.face_id != face.face_id && faces_towards(face, other))
    {
        return GazeTarget::Officiant;
    }

    GazeTarget::Away
}

/// True when this scene has somebody at the front of it that a room would look at.
///
/// Four scenes, and the list is short on purpose. `officiant` is a *role* the product
/// does not identify - phase 06's `Role::Vip` is the nearest thing and it is not the same
/// claim - so this target means "turned towards whoever is holding the room", which is
/// only a meaningful reading where there is one. A dance floor has no officiant and a
/// face turned towards another face there is turned towards a friend.
#[must_use]
pub const fn has_speaker(scene: SceneId) -> bool {
    matches!(
        scene,
        SceneId::Ceremony | SceneId::Ritual | SceneId::Vows | SceneId::Speeches
    )
}

/// True when two faces in this frame are looking at each other.
///
/// `ImageEmotion::mutual_gaze`. Symmetric by construction - both have to be turned
/// towards the other - which is what stops one person watching somebody else's back of
/// the head from counting.
#[must_use]
pub fn mutual(faces: &[FaceRef]) -> bool {
    for (index, left) in faces.iter().enumerate() {
        for right in faces.iter().skip(index + 1) {
            if !within_reach(left, right) {
                continue;
            }
            if faces_towards(left, right) && faces_towards(right, left) {
                return true;
            }
        }
    }
    false
}
