//! Which photographs should look like each other.
//!
//! Section 8 step 1: "build the scene-node tree from Phase 07 segments plus sub-clustering inside
//! long segments." Section 3 draws it as `Ceremony > Entrance/Ritual/Couple/Reactions`.
//!
//! ## A node is not a segment
//!
//! A segment is a **chapter of the story**: what a photographer renames, what the timeline shows,
//! what phase 07 owns. A node is a **lighting group**: the frames of one chapter that were shot
//! under one light. One segment becomes several nodes when a flash goes on, when the sun sets, or
//! when it is simply long enough that its first hour and its last do not describe the same room.
//!
//! Conflating the two is the mistake this module exists to avoid. A two-hour reception is one
//! chapter and is not one look, and a target computed over the whole of it is a target that
//! describes nowhere.
//!
//! ## Sub-clustering runs before change-point detection, and does something different
//!
//! Sub-clustering splits a segment on **time**: a run longer than [`MAX_NODE_SPAN_MS`] or wider
//! than [`MAX_NODE_FRAMES`] is divided into equal parts, because a photographer moved and the
//! product has no evidence of exactly when. Change-point detection then splits on **light**, which
//! is evidence. The two are separate because their failure modes are opposite: sub-clustering that
//! is too eager produces small nodes that cannot be anchored, and change-point detection that is
//! too eager produces nodes that describe a flicker.
//!
//! Sub-clustered nodes carry `GalleryCode::NodeSubClustered` and split nodes carry
//! `GalleryCode::NodeSplitByChangePoint`, so a panel can tell a photographer which of the two
//! happened - and they are different sentences, because only the second is a claim about the room.

use std::collections::BTreeMap;

use aura_core::contract::gallery::ImageId;
use aura_core::contract::gallery::{GalleryCode, MIN_NODE_FRAMES};
use aura_core::contract::ids::NodeId;
use aura_core::{IdentityId, SceneId, SegmentId};

/// Everything the consistency pass knows about one photograph.
///
/// Assembled by the caller from phase 07's scene, phase 15's tone estimate and phase 16's colour
/// decision, through the frozen `StoryService`, `ToneService` and `ColourService` and through
/// nothing else. This crate never asks a second time.
///
/// **Every field that comes from another phase is optional or carries its own confidence**, and
/// the absences are what the reason codes are made of. A frame with no tone estimate is not a
/// frame at 5,500 K.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// The photograph.
    pub image: ImageId,
    /// The chapter it belongs to.
    pub segment: SegmentId,
    /// What it is of.
    pub scene: SceneId,
    /// When it was taken, in milliseconds on the project's own aligned timeline.
    ///
    /// Phase 08's `sub_sec_ms` is already folded in by the caller, because EXIF's
    /// `DateTimeOriginal` has whole-second resolution and fourteen frames of a 10 fps burst would
    /// otherwise sort arbitrarily - which would make the change-point detector's signal depend on
    /// map order. Invariant 4.
    pub timeline_ms: i64,
    /// The solved temperature, in kelvin, or `None` when phase 15 has not looked.
    pub cct_k: Option<f32>,
    /// The solved tint.
    pub tint: Option<f32>,
    /// The solved exposure offset, in stops.
    pub exposure_ev: Option<f32>,
    /// The subject luminance after that exposure, `0..1`.
    pub subject_luma: Option<f32>,
    /// Phase 15's white-balance confidence, `0..1`.
    pub wb_conf: f32,
    /// Phase 15's exposure confidence, `0..1`.
    pub exposure_conf: f32,
    /// True when phase 15 found more than one light.
    pub mixed_light: bool,
    /// True when the light governing the subject is one phase 15 calls intentional.
    ///
    /// Read straight off `IlluminantKind::is_intentional` rather than decided a second time here.
    /// Phase 15 already established that a purple dance floor stays purple; this phase does not get
    /// a second opinion on it.
    pub intentional_light: bool,
    /// How much of the frame the intentional light accounts for, `0..1`.
    ///
    /// What [`aura_core::contract::gallery::SkinCorrection::cap_for_mood`] is given. Zero on an
    /// ordinary frame.
    pub mood: f32,
    /// Phase 16's contrast, in the recipe's units, or `None` when it has not graded this frame.
    pub contrast: Option<f32>,
    /// Phase 16's saturation.
    pub saturation: Option<f32>,
    /// Phase 16's colour character, as the eight numbers a node's target compares.
    pub signature: Option<[f32; 8]>,
    /// The identities in the frame, and how prominent each is, `0..1`.
    ///
    /// A `BTreeMap` rather than a hash map for the reason every collection in this product is
    /// ordered: the iteration order reaches an anchor ranking and a determinism test.
    pub identities: BTreeMap<IdentityId, f32>,
    /// True when a photographer has set this frame's gallery delta by hand.
    pub user_edited: bool,
    /// True when the consistency pass is switched on for this frame.
    pub enabled: bool,
}

impl Frame {
    /// True when there is enough here to normalise anything.
    #[must_use]
    pub fn has_tone(&self) -> bool {
        self.cct_k.is_some()
            && self.tint.is_some()
            && self.exposure_ev.is_some()
            && self.subject_luma.is_some()
    }

    /// True when there is enough here to harmonise a grade.
    #[must_use]
    pub fn has_grade(&self) -> bool {
        self.contrast.is_some() && self.saturation.is_some() && self.signature.is_some()
    }

    /// Why this frame cannot be normalised, when it cannot.
    ///
    /// The one place the four refusals are decided, so the tree, the solver, the outline and the
    /// panel cannot disagree about which of them applies - and the *order* is the priority: a
    /// frame a photographer edited is not also reported as a frame with no tone estimate.
    #[must_use]
    pub fn blocked_by(&self) -> Option<GalleryCode> {
        if !self.enabled {
            return Some(GalleryCode::Disabled);
        }
        if self.user_edited {
            return Some(GalleryCode::UserEdited);
        }
        if self.intentional_light {
            return Some(GalleryCode::MoodPreserved);
        }
        if !self.has_tone() {
            return Some(GalleryCode::ToneEstimateAbsent);
        }
        None
    }
}

/// The longest stretch of time one node may span, in milliseconds.
///
/// Forty minutes. A ceremony is about that long and is one room; a two-hour reception is not one
/// room, and a target computed over the whole of it describes nowhere. Chosen from the shape of a
/// wedding day rather than from a statistic, which is what section 9 asks a product manager to
/// approve.
pub const MAX_NODE_SPAN_MS: i64 = 40 * 60 * 1000;

/// The most frames one node may hold before it is sub-clustered.
///
/// Four hundred. Past it the robust statistics stop being the constraint and the *representativeness*
/// of three anchors does: three frames cannot describe six hundred, however good they are.
pub const MAX_NODE_FRAMES: usize = 400;

/// One node before it has anchors or a target.
///
/// [`aura_core::contract::gallery::SceneNode`] is what this becomes once
/// [`crate::anchors::select`] has run over it. The two are separate types because a node with an
/// empty anchor list and a node that has not been through anchor selection are different things,
/// and a single type would make them the same value.
#[derive(Debug, Clone, PartialEq)]
pub struct RawNode {
    /// This node.
    pub id: NodeId,
    /// What it was split out of.
    pub parent: Option<NodeId>,
    /// The chapter.
    pub segment: SegmentId,
    /// Its place among the nodes of its segment, zero first.
    pub ordinal: usize,
    /// How many nodes its segment has, so the label can say "2 of 3".
    pub siblings: usize,
    /// The dominant scene of its frames.
    pub scene: SceneId,
    /// Its frames, in capture order.
    pub frames: Vec<Frame>,
    /// Why it is shaped the way it is.
    pub reasons: Vec<GalleryCode>,
}

impl RawNode {
    /// What a photographer reads.
    ///
    /// `"Ceremony"` for a segment that is one node, `"Ceremony (2 of 3)"` for one that is not. The
    /// chapter's own label is the caller's to supply, because phase 07 owns what a chapter is
    /// called and a photographer may have renamed it.
    #[must_use]
    pub fn label(&self, chapter: &str) -> String {
        if self.siblings <= 1 {
            chapter.to_string()
        } else {
            format!("{chapter} ({} of {})", self.ordinal + 1, self.siblings)
        }
    }

    /// The frames' ids, in capture order.
    #[must_use]
    pub fn image_ids(&self) -> Vec<ImageId> {
        self.frames.iter().map(|frame| frame.image).collect()
    }

    /// When its first frame was taken.
    #[must_use]
    pub fn first_ts(&self) -> i64 {
        self.frames.first().map_or(0, |frame| frame.timeline_ms)
    }
}

/// Build the tree.
///
/// Frames are grouped by segment, ordered by capture time, and long segments are divided. The
/// change-point split is *not* here: [`crate::changepoint::split`] runs over the result, because
/// section 8 orders them that way and because the two answer different questions.
///
/// Frames with no segment are dropped and reported by the caller as
/// `GalleryCode::SegmentAbsent`. They are not put in a node of their own: a node whose only
/// property is "these frames belong nowhere" has no light in common and would be normalised
/// toward a target assembled from unrelated rooms.
#[must_use]
pub fn build(frames: &[Frame]) -> Vec<RawNode> {
    let mut by_segment: BTreeMap<SegmentId, Vec<Frame>> = BTreeMap::new();
    for frame in frames {
        by_segment
            .entry(frame.segment)
            .or_default()
            .push(frame.clone());
    }

    let mut nodes = Vec::new();
    for (segment, mut group) in by_segment {
        // Capture order, ties broken on the id so two runs of the same project agree. Invariant 4:
        // a wedding shot on two bodies has frames at the same millisecond, and a sort that left
        // their order to the input would make the change-point signal input-order dependent.
        group.sort_by(|a, b| {
            a.timeline_ms
                .cmp(&b.timeline_ms)
                .then_with(|| a.image.to_db().cmp(&b.image.to_db()))
        });

        let parts = sub_cluster(&group);
        let siblings = parts.len();
        for (ordinal, frames) in parts.into_iter().enumerate() {
            let mut reasons = Vec::new();
            if siblings > 1 {
                reasons.push(GalleryCode::NodeSubClustered);
            }
            let scene = dominant_scene(&frames);
            nodes.push(RawNode {
                id: NodeId::new(),
                parent: None,
                segment,
                ordinal,
                siblings,
                scene,
                frames,
                reasons,
            });
        }
    }

    nodes.sort_by_key(RawNode::first_ts);
    nodes
}

/// Divide one segment's frames into equal parts when it is too long or too wide.
///
/// Equal parts rather than a clustering, deliberately. There is no evidence here about *where* a
/// photographer moved - that is what the change-point detector looks for, on the light - so
/// inventing a boundary from the frame count would be a claim this module cannot support. Equal
/// parts is the honest division: it says "this is too long to describe as one thing" and nothing
/// more.
fn sub_cluster(frames: &[Frame]) -> Vec<Vec<Frame>> {
    if frames.is_empty() {
        return Vec::new();
    }
    let span = frames
        .last()
        .zip(frames.first())
        .map_or(0, |(last, first)| last.timeline_ms - first.timeline_ms);
    let by_span = if span > MAX_NODE_SPAN_MS {
        ((span + MAX_NODE_SPAN_MS - 1) / MAX_NODE_SPAN_MS) as usize
    } else {
        1
    };
    let by_count = frames.len().div_ceil(MAX_NODE_FRAMES).max(1);
    let mut parts = by_span.max(by_count);

    // Never divide a segment into parts that cannot be anchored. A node of two frames is worse than
    // a node of eighty: the eighty can be anchored and the two cannot, and an unanchored node
    // normalises nothing at all.
    while parts > 1 && frames.len() / parts < MIN_NODE_FRAMES {
        parts -= 1;
    }
    if parts <= 1 {
        return vec![frames.to_vec()];
    }

    let size = frames.len().div_ceil(parts);
    frames.chunks(size).map(<[Frame]>::to_vec).collect()
}

/// The scene most of a node's frames are of.
///
/// Ties break on `SceneId::ALL` order rather than on whichever the map yielded first, so two runs
/// of the same node produce the same scene and therefore the same policy row. Invariant 4, and it
/// matters here more than it looks: the scene decides the damping, so a tie broken differently is
/// a different gallery.
#[must_use]
pub fn dominant_scene(frames: &[Frame]) -> SceneId {
    let mut counts: BTreeMap<usize, u32> = BTreeMap::new();
    for frame in frames {
        *counts.entry(frame.scene.as_index()).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
        .and_then(|(index, _)| SceneId::ALL.get(index).copied())
        .unwrap_or(SceneId::Unknown)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;

    fn frame(segment: SegmentId, ms: i64, scene: SceneId) -> Frame {
        fixtures::frame_at(segment, ms, scene)
    }

    #[test]
    fn one_short_segment_is_one_node_and_is_not_labelled_as_a_part() {
        let segment = SegmentId::new();
        let frames: Vec<Frame> = (0..40)
            .map(|i| frame(segment, i * 10_000, SceneId::Ceremony))
            .collect();
        let nodes = build(&frames);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].label("Ceremony"), "Ceremony");
        assert!(!nodes[0].reasons.contains(&GalleryCode::NodeSubClustered));
    }

    #[test]
    fn a_two_hour_segment_is_sub_clustered_and_says_so() {
        let segment = SegmentId::new();
        let frames: Vec<Frame> = (0..120)
            .map(|i| frame(segment, i * 60_000, SceneId::DanceFloor))
            .collect();
        let nodes = build(&frames);
        assert!(nodes.len() >= 3, "two hours is more than one look");
        assert!(nodes[0].reasons.contains(&GalleryCode::NodeSubClustered));
        assert_eq!(nodes[0].label("Reception"), "Reception (1 of 3)");
    }

    #[test]
    fn a_segment_is_never_divided_into_parts_too_small_to_anchor() {
        let segment = SegmentId::new();
        // Six frames spread over three hours: by span this wants five parts, and five parts of one
        // frame each is five nodes none of which can be anchored.
        let frames: Vec<Frame> = (0..6)
            .map(|i| frame(segment, i * 30 * 60_000, SceneId::Venue))
            .collect();
        let nodes = build(&frames);
        assert_eq!(nodes.len(), 1, "six frames cannot become five nodes");
    }

    #[test]
    fn frames_are_ordered_by_capture_time_whatever_order_they_arrive_in() {
        let segment = SegmentId::new();
        let mut frames: Vec<Frame> = (0..10)
            .map(|i| frame(segment, i * 1_000, SceneId::Vows))
            .collect();
        frames.reverse();
        let nodes = build(&frames);
        let times: Vec<i64> = nodes[0].frames.iter().map(|f| f.timeline_ms).collect();
        let mut sorted = times.clone();
        sorted.sort_unstable();
        assert_eq!(times, sorted);
    }

    #[test]
    fn two_frames_at_the_same_millisecond_sort_the_same_way_every_run() {
        let segment = SegmentId::new();
        let a = frame(segment, 1_000, SceneId::Ceremony);
        let b = frame(segment, 1_000, SceneId::Ceremony);
        let one = build(&[a.clone(), b.clone()]);
        let two = build(&[b, a]);
        assert_eq!(one[0].image_ids(), two[0].image_ids());
    }

    #[test]
    fn the_dominant_scene_is_the_commonest_and_ties_break_deterministically() {
        let segment = SegmentId::new();
        let mut frames = vec![
            frame(segment, 0, SceneId::Ceremony),
            frame(segment, 1, SceneId::Ceremony),
            frame(segment, 2, SceneId::Vows),
        ];
        assert_eq!(dominant_scene(&frames), SceneId::Ceremony);
        frames.push(frame(segment, 3, SceneId::Vows));
        let first = dominant_scene(&frames);
        frames.reverse();
        assert_eq!(
            dominant_scene(&frames),
            first,
            "a tie is not input-order dependent"
        );
    }

    #[test]
    fn a_frame_says_why_it_cannot_be_normalised_in_priority_order() {
        let segment = SegmentId::new();
        let mut f = frame(segment, 0, SceneId::Ceremony);
        assert_eq!(f.blocked_by(), None);
        f.intentional_light = true;
        assert_eq!(f.blocked_by(), Some(GalleryCode::MoodPreserved));
        f.user_edited = true;
        assert_eq!(
            f.blocked_by(),
            Some(GalleryCode::UserEdited),
            "an edited frame is not also reported as a mood"
        );
        f.enabled = false;
        assert_eq!(f.blocked_by(), Some(GalleryCode::Disabled));
    }
}
