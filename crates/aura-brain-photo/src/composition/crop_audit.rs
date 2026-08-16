//! What the frame edge cut, and how much that matters here.
//!
//! PHASE-11 section 2.1: "crop violation detection using body keypoints: cut at joints
//! (wrist, elbow, knee, ankle), top-of-head crop, half-limb crop, third-person edge
//! intrusion." Section 6.1 adds the two rules that make it a judgement rather than a
//! geometry test:
//!
//! > "cutting at a joint scores worse than cutting mid-limb, and top-of-head crops are
//! > only violations outside `couple_portrait` close-ups where they can be deliberate."
//!
//! ## The rule that stops this being a rule engine
//!
//! Every cut this module finds is multiplied by [`SceneRule::joint_cut_weight`] before it
//! becomes a severity. On a `dance_floor` that weight is 0.20 and on a `family_portrait`
//! it is 1.20, so the *same* cut ankle is 0.09 in one and 0.54 in the other - one below
//! `JointCut::FLAG_AT` and one well above it. That is not tuning: a dance floor is a place
//! where limbs leave the frame constantly, and a family portrait is a place where a cut
//! wrist is the reason the photograph is taken again.
//!
//! ## A cut is a *body crossing an edge*, not a joint near one
//!
//! A wrist an inch from the bottom of the frame with the whole arm inside it is a tight
//! crop, not a cut. What makes it a cut is that the limb continues past the edge - so
//! [`analyse`] asks two questions in order: does this person's box cross this edge at all,
//! and if it does, where is the nearest joint to the crossing. Asking only the second
//! question is the classic false positive, and it is the one a photographer notices first
//! because it fires on every well-framed half-length portrait.
//!
//! ## Intrusions are about *who*, not about *what*
//!
//! Section 2.1 says "third-person edge intrusion" and "cropped guest heads at edges". Both
//! are the same event: a person who is not the subject is partly in the frame at an edge.
//! [`intrusions`] finds them from face boxes rather than from keypoints, because a guest
//! half in shot usually has a face and rarely has a derivable body - and because a face is
//! what a viewer's eye actually snags on.

use aura_core::contract::composition::{Box2, FrameEdge, Joint, JointCut};

use crate::composition::keypoints::{Point, Pose};
use crate::composition::rules::SceneRule;

/// How near a frame edge a joint has to be, as a fraction of the person's height, to be
/// the joint the edge cut.
///
/// Eight per cent of the body. A cut that lands more than that from any joint is a
/// mid-limb cut, which is the milder finding.
pub const AT_JOINT_BAND: f32 = 0.08;

/// How far outside the frame a person box has to reach before anything is called a cut.
///
/// One per cent of the frame. Below that the box is touching the edge rather than crossing
/// it, and a derived person box is not accurate enough to distinguish those two.
pub const CROSSING_MARGIN: f32 = 0.01;

/// Largest fraction of the frame a face can cover and still be an intruder.
///
/// Twelve per cent. Above that the person is a subject of the photograph, however close to
/// the edge they are - a bride at the left edge of a wide frame is composition, not
/// intrusion.
pub const INTRUDER_MAX_AREA: f32 = 0.12;

/// How much of a face box has to be outside the frame for it to be an intrusion.
///
/// Fifteen per cent. A guest fully inside the frame at the edge is a guest; a guest with a
/// slice missing is the thing section 2.1 calls "cropped guest heads at edges".
pub const INTRUDER_CUT_FRACTION: f32 = 0.15;

/// How much a mid-limb cut costs relative to a cut at the joint itself.
///
/// Forty-six per cent. Section 6.1 requires the ordering and not a particular ratio; this
/// is the smallest value that keeps a mid-limb cut in the strict `family_portrait` row
/// above [`JointCut::FLAG_AT`] while leaving the same geometry unflagged in permissive
/// reception and dance-floor rows.
pub const MID_LIMB_RATIO: f32 = 0.46;

/// What one cut costs before the scene weight is applied.
///
/// Ordered down the body, and the ordering is the argument. A neck cut is the one nobody
/// does on purpose; an ankle cut is in half the full-length portraits ever taken.
#[must_use]
pub const fn base_severity(joint: Joint) -> f32 {
    match joint {
        Joint::Neck => 1.00,
        Joint::Wrist => 0.85,
        Joint::Elbow => 0.70,
        Joint::Knee => 0.60,
        Joint::Hip => 0.55,
        Joint::Shoulder => 0.50,
        Joint::Ankle => 0.35,
    }
}

/// Everything the frame edge did to the people in it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CropAudit {
    /// Every cut, worst first.
    pub cuts: Vec<JointCut>,
    /// True when a head is cut and this scene does not allow it.
    pub head_crop: bool,
    /// How far above the frame the crown sits, as a fraction of frame height.
    ///
    /// Zero when nothing is cut. Carried separately from the flag because
    /// `CompositionResult::headroom` reports it as a negative headroom and phase 23 needs
    /// the depth to know how far out to crop.
    pub head_crop_depth: f32,
    /// True when a head is cut and the scene's rule row permits it.
    ///
    /// The exoneration half of the same measurement. Kept apart from `head_crop` so that
    /// the panel can say "this is a deliberate crop" rather than saying nothing at all,
    /// which is the difference between a product that explains itself and one that is
    /// merely quiet.
    pub head_crop_allowed: bool,
    /// People entering the frame at an edge.
    pub intrusions: Vec<Box2>,
    /// How many people had enough keypoints to be audited.
    pub audited: usize,
}

impl CropAudit {
    /// The cuts severe enough to raise a flag.
    #[must_use]
    pub fn flagged(&self) -> Vec<&JointCut> {
        self.cuts.iter().filter(|cut| cut.is_flagged()).collect()
    }

    /// True when at least one flagged cut lands on a joint.
    #[must_use]
    pub fn has_joint_cut(&self) -> bool {
        self.cuts.iter().any(|cut| cut.is_flagged() && cut.at_joint)
    }

    /// True when at least one flagged cut lands between joints.
    #[must_use]
    pub fn has_limb_cut(&self) -> bool {
        self.cuts
            .iter()
            .any(|cut| cut.is_flagged() && !cut.at_joint)
    }
}

/// Audit every located person against the frame edges.
///
/// `faces` is every face in the frame with its area, used only for [`intrusions`]; the
/// cuts come from `poses` alone.
#[must_use]
pub fn analyse(poses: &[Pose], faces: &[Box2], rule: &SceneRule) -> CropAudit {
    let mut audit = CropAudit {
        audited: poses.iter().filter(|pose| pose.located() > 0).count(),
        intrusions: intrusions(faces, poses),
        ..CropAudit::default()
    };

    for pose in poses {
        for edge in FrameEdge::ALL {
            if !crosses(pose.person, edge) {
                continue;
            }
            if let Some(cut) = cut_at(pose, edge, rule) {
                audit.cuts.push(cut);
            }
        }
        if let Some(depth) = head_cut_depth(pose) {
            audit.head_crop_depth = audit.head_crop_depth.max(depth);
        }
    }

    // One cut per joint per frame, keeping the worst. Two people whose wrists are both cut
    // by the bottom edge is one sentence in a panel, not two, and the evidence box shown
    // should be the worse of them.
    audit.cuts.sort_by(|left, right| {
        right
            .severity
            .total_cmp(&left.severity)
            .then(left.joint.cmp(&right.joint))
    });
    let mut seen: Vec<Joint> = Vec::new();
    audit.cuts.retain(|cut| {
        if seen.contains(&cut.joint) {
            return false;
        }
        seen.push(cut.joint);
        true
    });

    if audit.head_crop_depth > 0.0 {
        audit.head_crop = !rule.head_crop_allowed;
        audit.head_crop_allowed = rule.head_crop_allowed;
    }
    audit
}

/// True when this person's box reaches past this edge of the frame.
#[must_use]
pub fn crosses(person: Box2, edge: FrameEdge) -> bool {
    match edge {
        FrameEdge::Left => person.x < -CROSSING_MARGIN,
        FrameEdge::Right => person.x + person.w > 1.0 + CROSSING_MARGIN,
        FrameEdge::Top => person.y < -CROSSING_MARGIN,
        FrameEdge::Bottom => person.y + person.h > 1.0 + CROSSING_MARGIN,
    }
}

/// Find the joint this edge cut through, if any.
///
/// Returns the *nearest* named joint to the edge among those that are present, and marks
/// it `at_joint` when it is within [`AT_JOINT_BAND`] of the edge. A crossing with no
/// present joint anywhere near it is a mid-limb cut attributed to the nearest joint, which
/// is what a photographer would call it: "cut halfway up the shin" is reported against the
/// knee.
#[must_use]
pub fn cut_at(pose: &Pose, edge: FrameEdge, rule: &SceneRule) -> Option<JointCut> {
    let span = pose.person.h.max(pose.person.w).max(f32::EPSILON);
    let mut best: Option<(Joint, Point, f32)> = None;
    for joint in Joint::ALL {
        let Some(point) = pose.joint_at(joint) else {
            continue;
        };
        let distance = edge_distance(point, edge);
        // Only joints on the far side of the frame from the picture's middle can have been
        // cut by this edge: a shoulder two thirds of the way up cannot be what the bottom
        // edge cut through.
        if distance > span * 0.5 {
            continue;
        }
        let better = best.is_none_or(|(_, _, seen)| distance.abs() < seen.abs());
        if better {
            best = Some((joint, point, distance));
        }
    }

    let (joint, point, distance) = best?;
    let at_joint = distance.abs() <= span * AT_JOINT_BAND;
    let base = base_severity(joint) * if at_joint { 1.0 } else { MID_LIMB_RATIO };
    let severity = (base * rule.joint_cut_weight).clamp(0.0, 1.0);

    Some(JointCut {
        joint,
        edge,
        at_joint,
        severity,
        area: evidence_box(point, span),
    })
}

/// Signed distance from a point to one frame edge, positive inside the frame.
#[must_use]
pub fn edge_distance(point: Point, edge: FrameEdge) -> f32 {
    match edge {
        FrameEdge::Left => point.x,
        FrameEdge::Right => 1.0 - point.x,
        FrameEdge::Top => point.y,
        FrameEdge::Bottom => 1.0 - point.y,
    }
}

/// How far above the top of the frame a crown sits, or `None` when it is inside.
///
/// Reported from [`Pose::crown`] when the nose is located and from the face box otherwise.
/// The face-box fallback is deliberately conservative - it uses the box top rather than an
/// estimated crown - so a missing nose under-reports a head crop rather than inventing
/// one.
#[must_use]
pub fn head_cut_depth(pose: &Pose) -> Option<f32> {
    let crown = pose.crown().unwrap_or(pose.face.y);
    if crown < 0.0 {
        Some(-crown)
    } else {
        None
    }
}

/// The rectangle to show for one cut.
///
/// A square a fifth of the body across, centred on the joint and clamped to the frame. Big
/// enough that the panel shows the limb going out of frame rather than a close-up of one
/// pixel of skin, which is the mistake `CropRect::expanded` exists to prevent elsewhere.
#[must_use]
pub fn evidence_box(point: Point, span: f32) -> Box2 {
    let side = (span * 0.20).clamp(0.04, 0.40);
    Box2 {
        x: point.x - side / 2.0,
        y: point.y - side / 2.0,
        w: side,
        h: side,
    }
    .clamped()
}

/// People entering the frame at an edge.
///
/// A face is an intrusion when it is small, near an edge, and partly outside the frame.
/// All three conditions, in that order:
///
/// * **small**, because a large face at an edge is a subject;
/// * **near an edge**, because that is what "entering the frame" means;
/// * **partly outside**, because a whole guest standing at the edge of a reception frame
///   is a guest at a reception and flagging that would flag most of a wedding.
///
/// `poses` is consulted only to skip the faces that belong to people the crop audit has
/// already reported on, so that one guest is not both a cut body and an intrusion.
#[must_use]
pub fn intrusions(faces: &[Box2], poses: &[Pose]) -> Vec<Box2> {
    let mut out = Vec::new();
    for face in faces {
        let area = face.w * face.h;
        if area > INTRUDER_MAX_AREA {
            continue;
        }
        let outside = outside_fraction(*face);
        if outside < INTRUDER_CUT_FRACTION {
            continue;
        }
        let already = poses.iter().any(|pose| overlaps(pose.face, *face));
        if already && poses.len() == 1 {
            // The frame's only person is this one: they are the subject, not an intruder.
            continue;
        }
        out.push(face.clamped());
    }
    out.sort_by(|left, right| (right.w * right.h).total_cmp(&(left.w * left.h)));
    out
}

/// What fraction of a box falls outside the frame, `0..1`.
#[must_use]
pub fn outside_fraction(area: Box2) -> f32 {
    let total = area.w * area.h;
    if total <= f32::EPSILON {
        return 0.0;
    }
    let inside = area.clamped();
    let kept = inside.w * inside.h;
    (1.0 - kept / total).clamp(0.0, 1.0)
}

/// True when two boxes share more than a quarter of the smaller one.
#[must_use]
pub fn overlaps(left: Box2, right: Box2) -> bool {
    let x0 = left.x.max(right.x);
    let y0 = left.y.max(right.y);
    let x1 = (left.x + left.w).min(right.x + right.w);
    let y1 = (left.y + left.h).min(right.y + right.h);
    if x1 <= x0 || y1 <= y0 {
        return false;
    }
    let shared = (x1 - x0) * (y1 - y0);
    let smaller = (left.w * left.h).min(right.w * right.h).max(f32::EPSILON);
    shared / smaller > 0.25
}
