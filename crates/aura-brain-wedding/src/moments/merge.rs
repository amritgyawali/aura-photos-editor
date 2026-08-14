//! Two photographers, one instant - and the photographer's own corrections.
//!
//! PHASE-08 section 6.2's last paragraph and section 9's editing API, together in one
//! module because they are the same operation reached two ways: something the evidence
//! says is one moment, and something the person says is.
//!
//! ## The automatic half
//!
//! > "Cross-camera merge: two moments from different cameras merge when their time
//! > ranges overlap by > 60 % and their embedding medoids are within 0.12."
//!
//! Both conditions, and both are deliberately hard to satisfy. Section 12's third
//! failure mode is "cross-camera merge misfires on similar scenes", and the mitigation
//! it names is exactly this: "require strong temporal overlap and medoid proximity;
//! keep sub-groups separable so a bad merge is recoverable".
//!
//! Separability is what makes the risk acceptable. A merged moment keeps its
//! per-camera bursts intact, so a photographer who disagrees splits it back along the
//! line it was joined on - and gets exactly the two moments they had before, rather
//! than an arbitrary cut through a merged list.
//!
//! **Why not do this in the graph?** Because the two are different claims about
//! different things. Frame-level cross-camera edges say "these two photographs are the
//! same photograph", which is true when two shooters stand beside each other and false
//! when they stand at opposite ends of an aisle. Moment-level merging says "these two
//! groups are one event", which is true in both cases. Running only the first would
//! miss the second shooter's angle on the kiss; running only the second would merge two
//! separate family groups that happened to be photographed simultaneously from two
//! sides. Both exist, and this is the conservative one.
//!
//! ## The manual half
//!
//! [`plan_split`] and [`plan_merge`] validate what the photographer asked for and
//! return what to write. They are pure, so the refusals - a photograph that is not in
//! the moment, a split that would empty a side, two moments in different weddings - are
//! testable without a catalog, and `AURA-ML-5029` says which one happened.
//!
//! A photographer's edit outranks every number in this crate. The lock is set on
//! **both** sides of a split for the reason phase 07 locks both sides of a boundary
//! move: locking one would let the next re-grouping put them back together from the
//! other side.

use aura_core::contract::moment::{ImageId, Timestamp};
use aura_core::AuraError;

use crate::errors;
use crate::moments::graph::Frame;

/// The share of the shorter moment's span that must overlap the other's.
///
/// Section 6.2's 60 %. Measured against the *shorter* of the two rather than against
/// their union, because a two-second burst inside a forty-second sequence overlaps the
/// union by 5 % and is still entirely inside it - and that is the two-shooter case this
/// rule exists to catch, not a counter-example to it.
pub const CROSS_CAMERA_OVERLAP: f32 = 0.60;

/// The greatest cosine distance between two moments' medoids that still merges.
///
/// Section 6.2's 0.12. Four times the near-duplicate bound, because two bodies at two
/// angles with two lenses do not produce near-duplicate frames - they produce two
/// photographs of one thing.
pub const CROSS_CAMERA_MEDOID: f32 = 0.12;

/// A proposed cross-camera merge, with the evidence for it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CrossCameraMerge {
    /// Index of the earlier moment in the caller's list.
    pub left: usize,
    /// Index of the later one.
    pub right: usize,
    /// Overlap as a share of the shorter span, `0..1`.
    pub overlap: f32,
    /// Cosine distance between the two medoids.
    pub medoid_distance: f32,
}

/// One moment, as the merge pass needs to see it.
#[derive(Debug, Clone)]
pub struct MomentSpan {
    /// Frame indices, in timeline order.
    pub members: Vec<usize>,
    /// First frame's time.
    pub start_ts: Timestamp,
    /// Last frame's time.
    pub end_ts: Timestamp,
    /// Distinct camera handles, ascending.
    pub cameras: Vec<u32>,
}

impl MomentSpan {
    /// Build a span from a group of frame indices.
    #[must_use]
    pub fn of(frames: &[Frame], members: &[usize]) -> Self {
        let mut cameras: Vec<u32> = members
            .iter()
            .filter_map(|index| frames.get(*index).map(|frame| frame.camera))
            .collect();
        cameras.sort_unstable();
        cameras.dedup();

        let times: Vec<Timestamp> = members
            .iter()
            .filter_map(|index| frames.get(*index).map(|frame| frame.ts))
            .collect();

        Self {
            members: members.to_vec(),
            start_ts: times.iter().copied().min().unwrap_or(0),
            end_ts: times.iter().copied().max().unwrap_or(0),
            cameras,
        }
    }

    /// How long the moment lasted, in milliseconds. At least one, so a single-frame
    /// moment does not divide by zero when its overlap is measured.
    #[must_use]
    pub const fn span_ms(&self) -> i64 {
        let span = self.end_ts - self.start_ts;
        if span > 0 {
            span
        } else {
            1
        }
    }

    /// True when the two moments have no camera in common.
    #[must_use]
    pub fn disjoint_cameras(&self, other: &Self) -> bool {
        !self
            .cameras
            .iter()
            .any(|camera| other.cameras.contains(camera))
    }
}

/// Overlap between two spans, as a share of the shorter one. `0..1`.
#[must_use]
pub fn overlap_fraction(left: &MomentSpan, right: &MomentSpan) -> f32 {
    let start = left.start_ts.max(right.start_ts);
    let end = left.end_ts.min(right.end_ts);
    let shared = (end - start).max(0);
    let shorter = left.span_ms().min(right.span_ms());
    (shared as f32 / shorter as f32).clamp(0.0, 1.0)
}

/// The member of a group whose summed distance to the others is smallest.
///
/// The medoid rather than the centroid, for `aura_index::query::reference_frame`'s
/// reason: a centroid is a vector no photograph is, and a photographer cannot be shown
/// one. Here it is also the cheaper answer - a moment holds a handful of frames, and
/// building a centroid would mean widening every vector to compute something no
/// downstream step needs.
#[must_use]
pub fn medoid(frames: &[Frame], members: &[usize]) -> Option<usize> {
    if members.len() == 1 {
        return members.first().copied();
    }
    let mut best: Option<(usize, f32)> = None;
    for candidate in members {
        let Some(left) = frames.get(*candidate) else {
            continue;
        };
        let mut total = 0.0f32;
        for other in members {
            if other == candidate {
                continue;
            }
            if let Some(right) = frames.get(*other) {
                total +=
                    aura_index::contract::index::cosine_distance(&left.embedding, &right.embedding);
            }
        }
        // Strictly less, so ties go to the earlier frame and two machines agree.
        if best.is_none_or(|(_, score)| total < score) {
            best = Some((*candidate, total));
        }
    }
    best.map(|(index, _)| index)
}

/// Find every cross-camera merge the evidence supports.
///
/// `spans` must be ordered by `start_ts`. The sweep is forward-only and stops as soon
/// as a later moment starts after this one ends, so the cost is linear in the number of
/// moments rather than quadratic - which matters because a 12,000-frame wedding has
/// three thousand of them.
///
/// Each moment is merged at most once per pass. A three-camera wedding needs two
/// passes to fold all three together, and the caller runs it to a fixed point; doing it
/// transitively inside one pass would let a chain of weak overlaps join two moments
/// that overlap each other by nothing at all.
#[must_use]
pub fn cross_camera(frames: &[Frame], spans: &[MomentSpan]) -> Vec<CrossCameraMerge> {
    let mut out: Vec<CrossCameraMerge> = Vec::new();
    let mut taken = vec![false; spans.len()];

    for (index, left) in spans.iter().enumerate() {
        if taken.get(index).copied().unwrap_or(true) {
            continue;
        }
        for (offset, right) in spans.iter().enumerate().skip(index + 1) {
            if taken.get(offset).copied().unwrap_or(true) {
                continue;
            }
            if right.start_ts > left.end_ts {
                break;
            }
            if !left.disjoint_cameras(right) {
                continue;
            }
            let overlap = overlap_fraction(left, right);
            if overlap < CROSS_CAMERA_OVERLAP {
                continue;
            }
            let (Some(a), Some(b)) = (
                medoid(frames, &left.members),
                medoid(frames, &right.members),
            ) else {
                continue;
            };
            let (Some(x), Some(y)) = (frames.get(a), frames.get(b)) else {
                continue;
            };
            let distance = aura_index::contract::index::cosine_distance(&x.embedding, &y.embedding);
            if distance > CROSS_CAMERA_MEDOID {
                continue;
            }

            out.push(CrossCameraMerge {
                left: index,
                right: offset,
                overlap,
                medoid_distance: distance,
            });
            if let Some(slot) = taken.get_mut(index) {
                *slot = true;
            }
            if let Some(slot) = taken.get_mut(offset) {
                *slot = true;
            }
            break;
        }
    }

    out
}

/// Apply the merges to a list of groups, returning the new list.
///
/// The surviving group holds both sides' frames in timeline order, so the bursts
/// recomputed over it come out per camera - which is section 2.1's "one moment with
/// per-camera sub-groups", achieved by construction rather than by bookkeeping.
#[must_use]
pub fn apply(
    groups: &[Vec<usize>],
    merges: &[CrossCameraMerge],
    frames: &[Frame],
) -> Vec<Vec<usize>> {
    let mut absorbed: Vec<bool> = vec![false; groups.len()];
    let mut out: Vec<Vec<usize>> = Vec::new();

    for (index, group) in groups.iter().enumerate() {
        if absorbed.get(index).copied().unwrap_or(false) {
            continue;
        }
        let mut folded = group.clone();
        for merge in merges.iter().filter(|merge| merge.left == index) {
            if let Some(other) = groups.get(merge.right) {
                folded.extend(other.iter().copied());
                if let Some(slot) = absorbed.get_mut(merge.right) {
                    *slot = true;
                }
            }
        }
        folded.sort_by_key(|position| {
            frames
                .get(*position)
                .map_or((i64::MAX, *position), |frame| (frame.ts, *position))
        });
        folded.dedup();
        out.push(folded);
    }

    out
}

// ---------------------------------------------------------------------------
// The photographer's own edits
// ---------------------------------------------------------------------------

/// What a split would produce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitPlan {
    /// Frames staying with the original moment.
    pub keep: Vec<ImageId>,
    /// Frames moving to the new one, starting at the photograph the split was on.
    pub moved: Vec<ImageId>,
}

/// Work out what splitting a moment at a photograph would do.
///
/// The photograph named becomes the **first frame of the new later half**, which is the
/// same convention `StoryService::split_segment` uses for a chapter. A photographer who
/// clicks a frame and says "split here" means "this frame starts something new".
///
/// # Errors
///
/// `AURA-ML-5029` when the photograph is not in the moment, when it is the first frame
/// (which would leave the original empty), or when the moment holds fewer than two
/// frames.
pub fn plan_split(members: &[ImageId], at: ImageId) -> Result<SplitPlan, AuraError> {
    if members.len() < 2 {
        return Err(errors::moment_edit_refused(
            "split",
            "the moment holds a single frame, so there is nothing to split",
        ));
    }
    let Some(position) = members.iter().position(|image| *image == at) else {
        return Err(errors::moment_edit_refused(
            "split",
            "that photograph is not in this moment",
        ));
    };
    if position == 0 {
        return Err(errors::moment_edit_refused(
            "split",
            "splitting before the first frame would leave the original moment empty; split at the \
             frame that starts the new one",
        ));
    }
    let (keep, moved) = members.split_at(position);
    Ok(SplitPlan {
        keep: keep.to_vec(),
        moved: moved.to_vec(),
    })
}

/// What a merge would produce: every frame of both, in timeline order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergePlan {
    /// The surviving moment's frames.
    pub members: Vec<ImageId>,
}

/// Work out what merging two moments would do.
///
/// `left` and `right` carry `(image, ts)` pairs because the merged order is by time and
/// the caller has already read both from the catalog.
///
/// # Errors
///
/// `AURA-ML-5029` when either side is empty, or when a photograph appears in both -
/// which would mean the catalog had one frame in two moments, and is checked here
/// rather than trusted because `idx_moment_images_photo` is the only thing preventing
/// it and a bug upstream would otherwise silently duplicate a frame.
pub fn plan_merge(
    left: &[(ImageId, Timestamp)],
    right: &[(ImageId, Timestamp)],
) -> Result<MergePlan, AuraError> {
    if left.is_empty() || right.is_empty() {
        return Err(errors::moment_edit_refused(
            "merge",
            "one of the two moments holds no frames",
        ));
    }
    if let Some((duplicate, _)) = right
        .iter()
        .find(|(image, _)| left.iter().any(|(other, _)| other == image))
    {
        return Err(errors::moment_edit_refused(
            "merge",
            &format!("photograph {duplicate} is already in both moments"),
        ));
    }

    let mut all: Vec<(ImageId, Timestamp)> = left.to_vec();
    all.extend_from_slice(right);
    all.sort_by_key(|(image, ts)| (*ts, *image));
    Ok(MergePlan {
        members: all.into_iter().map(|(image, _)| image).collect(),
    })
}
