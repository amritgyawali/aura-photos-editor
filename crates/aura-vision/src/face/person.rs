//! Bodies, and which face belongs to which one.
//!
//! Two things in this product need a body box rather than a face box, and they are
//! both cases where the face is not available.
//!
//! **Counting people.** A wide reception frame has forty people in it and the
//! detector finds eleven faces, because the rest are turned away, in shadow, or
//! behind somebody's shoulder. A coverage guarantee built on face counts would
//! report that a wedding has eleven guests. Sections 6.1 and 12 both name this,
//! and [`people_count`] is the answer: bodies plus the faces no body claimed.
//!
//! **Attaching a mask later.** Phases 18 and 20 need to know where a person is,
//! not only where their face is, and a mask anchored to a face box stops at the
//! chin. The association here is what lets a later phase say "the bride's dress"
//! without running a second detector over the whole wedding.
//!
//! ## Why there is no person detector
//!
//! Because there does not need to be one. The detector's head predicts a face box
//! and a body box **from the same anchor** - see the channel layout on
//! `aura_infer::onnx::fixtures::FACE_HEAD_CHANNELS` - so the common case is not an
//! association problem at all: two boxes that came from one anchor are one person
//! by construction, at no cost and with no ambiguity. A second model would have
//! cost a fourth model card, a fourth warmup, a fourth memory reservation, and it
//! would have had to be reconciled with the face detector's answer anyway.
//!
//! Only two cases need geometry. A body whose anchor did not fire on the face -
//! the back-of-head case - has no face to bind, and that is the correct answer
//! rather than a failure. And a body that survived non-maximum suppression while
//! the anchor that produced it did not needs re-binding, which is what
//! [`associate`] does.

use crate::face::detect::{Detection, NormBox};

/// How a body box came to be tied to a face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Association {
    /// The face box sits inside the body's head region. This covers the
    /// anchor-shared case, where the two boxes were predicted together and
    /// containment holds by construction.
    Contained,
    /// The closest unclaimed face within the search radius.
    Nearest,
    /// No face. A back-of-head, a hair-covered guest, or somebody facing away.
    None,
}

impl Association {
    /// Stable text for the `person_boxes.assoc_kind` column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Contained => "contained",
            Self::Nearest => "nearest",
            Self::None => "none",
        }
    }
}

/// The top fraction of a body box that a head can plausibly occupy.
///
/// A third. Standing adult proportions put the head in the top one-seventh to
/// one-eighth of the body, but a wedding body box is almost never a whole standing
/// body: it is a seated guest at a table, a torso behind a chair, a dancer cropped
/// at the waist. A third is generous enough to hold the head in all of those and
/// tight enough to refuse a face that belongs to the person standing behind.
pub const HEAD_REGION: f32 = 0.34;

/// Intersection-over-union at which an anchor's body box is judged to be the
/// same body as one that survived suppression.
pub const ANCHOR_REBIND_IOU: f32 = 0.5;

/// How far a face centre may sit from a body's head-region centre and still be
/// bound to it, in units of the body box's width.
///
/// Three-quarters of the body's width. Wider than it sounds necessary, and
/// deliberately: a body box in a crowd is often clipped to the visible sliver of a
/// torso, so its width understates the person, and a tight radius would leave
/// every face in a group photograph unbound.
pub const NEAREST_RADIUS: f32 = 0.75;

/// One body, and the face it belongs to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PersonBox {
    /// The body, in normalised frame coordinates.
    pub bbox: NormBox,
    /// Body objectness, `0..1`.
    pub det_score: f32,
    /// Index into the face slice this body was bound to.
    pub face: Option<usize>,
    /// How confident the binding is, `0..1`. Zero when there is no face.
    pub assoc_score: f32,
    /// How the binding was made.
    pub assoc_kind: Association,
}

/// The head region of a body box: the top [`HEAD_REGION`] of it.
#[must_use]
pub fn head_region(body: &NormBox) -> NormBox {
    NormBox {
        x: body.x,
        y: body.y,
        w: body.w,
        h: body.h * HEAD_REGION,
    }
}

/// Bind faces to bodies.
///
/// `faces` must be the suppressed detections and `persons` the suppressed body
/// boxes, both from [`crate::face::detect::FaceDetector::detect`].
///
/// The order is the point. Anchor-shared bindings are claimed first, because they
/// are the only ones that are certain; containment second, because a face inside a
/// head region is nearly certain; proximity last, and only for what is left. A
/// single-pass greedy match by distance would let a confident-but-distant pair
/// steal a face from the body that actually predicted it.
///
/// Deterministic: `faces` arrives sorted by score then geometry from
/// [`crate::face::detect::nms`], and `persons` is sorted here before the geometric
/// passes, so two machines bind the same pairs.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn associate(faces: &[Detection], persons: &[NormBox]) -> Vec<PersonBox> {
    let mut ordered: Vec<(usize, NormBox)> = persons.iter().copied().enumerate().collect();
    ordered.sort_by_key(|(index, body)| {
        (
            (body.y * 10_000.0).round() as i32,
            (body.x * 10_000.0).round() as i32,
            *index,
        )
    });

    let mut out: Vec<PersonBox> = ordered
        .iter()
        .map(|(_, body)| PersonBox {
            bbox: *body,
            det_score: 0.0,
            face: None,
            assoc_score: 0.0,
            assoc_kind: Association::None,
        })
        .collect();
    let mut claimed = vec![false; faces.len()];

    // Pass 1: the anchor-shared pairs. Each face that carried a body box is
    // re-bound to whichever surviving body box that prediction became.
    for (face_index, face) in faces.iter().enumerate() {
        let Some(predicted) = face.person else {
            continue;
        };
        let mut best: Option<(usize, f32)> = None;
        for (slot_index, slot) in out.iter().enumerate() {
            if slot.face.is_some() {
                continue;
            }
            let overlap = slot.bbox.iou(&predicted);
            if overlap < ANCHOR_REBIND_IOU {
                continue;
            }
            if best.is_none_or(|(_, previous)| overlap > previous) {
                best = Some((slot_index, overlap));
            }
        }
        if let Some((slot_index, overlap)) = best {
            if let Some(slot) = out.get_mut(slot_index) {
                slot.face = Some(face_index);
                slot.det_score = face.person_score;
                slot.assoc_score =
                    (face.face_score.min(face.person_score) * overlap).clamp(0.0, 1.0);
                slot.assoc_kind = Association::Contained;
                if let Some(flag) = claimed.get_mut(face_index) {
                    *flag = true;
                }
            }
        }
    }

    // Pass 2: containment. A face centre inside a body's head region.
    for slot_index in 0..out.len() {
        let Some(slot) = out.get(slot_index) else {
            continue;
        };
        if slot.face.is_some() {
            continue;
        }
        let region = head_region(&slot.bbox);
        let mut best: Option<(usize, f32)> = None;
        for (face_index, face) in faces.iter().enumerate() {
            if claimed.get(face_index).copied().unwrap_or(true) {
                continue;
            }
            let [cx, cy] = face.face.centre();
            let inside =
                cx >= region.x && cx <= region.right() && cy >= region.y && cy <= region.bottom();
            if !inside {
                continue;
            }
            let overlap = region.iou(&face.face);
            if best.is_none_or(|(_, previous)| overlap > previous) {
                best = Some((face_index, overlap));
            }
        }
        if let Some((face_index, overlap)) = best {
            let score = faces
                .get(face_index)
                .map_or(0.0, |face| face.face_score * (0.5 + 0.5 * overlap));
            if let Some(slot) = out.get_mut(slot_index) {
                slot.face = Some(face_index);
                slot.assoc_score = score.clamp(0.0, 1.0);
                slot.assoc_kind = Association::Contained;
                if slot.det_score <= 0.0 {
                    slot.det_score = faces.get(face_index).map_or(0.0, |f| f.person_score);
                }
            }
            if let Some(flag) = claimed.get_mut(face_index) {
                *flag = true;
            }
        }
    }

    // Pass 3: proximity, for what is left.
    for slot_index in 0..out.len() {
        let Some(slot) = out.get(slot_index) else {
            continue;
        };
        if slot.face.is_some() {
            continue;
        }
        let region = head_region(&slot.bbox);
        let target = region.centre();
        let radius = slot.bbox.w * NEAREST_RADIUS;
        if radius <= f32::EPSILON {
            continue;
        }
        let mut best: Option<(usize, f32)> = None;
        for (face_index, face) in faces.iter().enumerate() {
            if claimed.get(face_index).copied().unwrap_or(true) {
                continue;
            }
            let [cx, cy] = face.face.centre();
            let dx = cx - target[0];
            let dy = cy - target[1];
            let separation = (dx * dx + dy * dy).sqrt();
            if separation > radius {
                continue;
            }
            if best.is_none_or(|(_, previous)| separation < previous) {
                best = Some((face_index, separation));
            }
        }
        if let Some((face_index, separation)) = best {
            // Confidence falls off linearly with distance, so a face at the very
            // edge of the radius is bound at nearly zero rather than at the same
            // confidence as one dead centre. A later phase that needs certainty
            // can threshold on this.
            let closeness = (1.0 - separation / radius).clamp(0.0, 1.0);
            let score = faces
                .get(face_index)
                .map_or(0.0, |face| face.face_score * closeness * 0.8);
            if let Some(slot) = out.get_mut(slot_index) {
                slot.face = Some(face_index);
                slot.assoc_score = score.clamp(0.0, 1.0);
                slot.assoc_kind = Association::Nearest;
                if slot.det_score <= 0.0 {
                    slot.det_score = faces.get(face_index).map_or(0.0, |f| f.person_score);
                }
            }
            if let Some(flag) = claimed.get_mut(face_index) {
                *flag = true;
            }
        }
    }

    out
}

/// How many people are in a frame, and how that number was arrived at.
///
/// Invariant 2 again: a coverage report that says "eleven guests" has to be able to
/// say where eleven came from, because the answer is often "eleven faces and
/// nineteen backs of heads" and the photographer needs to know which.
#[derive(Debug, Clone, PartialEq)]
pub struct PeopleCount {
    /// Faces the detector found.
    pub faces: usize,
    /// Body boxes the detector found.
    pub bodies: usize,
    /// Bodies with no face bound to them.
    pub headless: usize,
    /// Faces with no body bound to them. Common and not a fault: a face in a
    /// crowd often has no visible torso at all.
    pub bodiless_faces: usize,
    /// The count this frame contributes.
    pub total: usize,
    /// Why.
    pub reasons: Vec<String>,
}

/// Count the people in a frame from its faces and bodies.
///
/// The rule is a union, not a maximum: every body is a person, and every face that
/// no body claimed is also a person. A maximum would undercount the frame where
/// twelve faces are visible over a row of six resolvable torsos, and a sum would
/// double-count every person the detector saw properly.
#[must_use]
pub fn people_count(faces: &[Detection], bound: &[PersonBox]) -> PeopleCount {
    let bodies = bound.len();
    let headless = bound.iter().filter(|body| body.face.is_none()).count();
    let claimed: usize = bound.iter().filter(|body| body.face.is_some()).count();
    let bodiless_faces = faces.len().saturating_sub(claimed);
    let total = bodies + bodiless_faces;

    let mut reasons = Vec::new();
    reasons.push(format!(
        "{bodies} bodies, {headless} of them with no visible face"
    ));
    if bodiless_faces > 0 {
        reasons.push(format!(
            "{bodiless_faces} faces had no resolvable body and were counted on their own"
        ));
    }
    if total == 0 {
        reasons.push("no faces and no bodies in this frame".to_string());
    }

    PeopleCount {
        faces: faces.len(),
        bodies,
        headless,
        bodiless_faces,
        total,
        reasons,
    }
}
