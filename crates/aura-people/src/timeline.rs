//! When each person was photographed, and with whom.
//!
//! Section 2.1 asks for "a person timeline: for each identity, when they appear, in
//! which scenes, with which other identities". That sounds like a reporting feature and
//! it is not: it is the input to three later phases and one immediate one.
//!
//! * **Coverage (phase 12)** needs the gaps. "Every close family member appears at
//!   least three times" is a statement about counts; "the groom's grandmother was not
//!   photographed between the ceremony and the speeches" is a statement about a
//!   timeline, and it is the one a photographer can still act on during the reception.
//! * **Role inference (this phase)** needs the first appearance. Somebody present
//!   during getting-ready is family, and "present during getting-ready" is a position
//!   on a timeline before phase 07 gives it a name.
//! * **Curation (phase 29)** needs the span. A guest who appears across eleven hours
//!   is different from one who appears across eleven minutes, even at the same frame
//!   count.
//!
//! Everything here is derived from rows that already exist, and the timeline is
//! recomputed rather than stored. A stored timeline is a cache with no invalidation
//! rule, and every merge in the People panel would invalidate it.

use std::collections::{BTreeMap, BTreeSet};

use aura_core::{IdentityId, PhotoId};

use crate::graph::CooccurrenceGraph;
use crate::store::FaceRow;

/// One appearance of one identity.
#[derive(Debug, Clone, PartialEq)]
pub struct Appearance {
    /// The photograph.
    pub photo: PhotoId,
    /// Milliseconds since the epoch, when the photograph has a timeline time.
    pub at_ms: Option<i64>,
    /// The scene, when phase 07 has named one.
    pub scene: Option<String>,
    /// The best face this identity has in the frame, by fused quality.
    pub best_quality: f32,
    /// The largest area this identity's face covers in the frame.
    pub area_frac: f32,
}

/// A gap in somebody's coverage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gap {
    /// Start of the gap, in milliseconds since the epoch.
    pub from_ms: i64,
    /// End of the gap.
    pub to_ms: i64,
}

impl Gap {
    /// How long the gap lasted, in whole minutes.
    #[must_use]
    pub const fn minutes(&self) -> i64 {
        (self.to_ms - self.from_ms) / 60_000
    }
}

/// The smallest gap worth reporting, in milliseconds.
///
/// Forty-five minutes. Shorter than that is a course of dinner, a costume change or a
/// trip to the bar, and reporting it would bury the gap that matters - the one where
/// somebody was missed for two hours - under thirty that do not.
pub const MIN_REPORTABLE_GAP_MS: i64 = 45 * 60 * 1_000;

/// One identity's whole timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct IdentityTimeline {
    /// Whose.
    pub identity: IdentityId,
    /// Appearances, earliest first. Photographs with no timeline time sort last, in id
    /// order, because a frame with no time is still an appearance and dropping it would
    /// undercount somebody.
    pub appearances: Vec<Appearance>,
    /// First appearance with a known time.
    pub first_ms: Option<i64>,
    /// Last appearance with a known time.
    pub last_ms: Option<i64>,
    /// Gaps longer than [`MIN_REPORTABLE_GAP_MS`].
    pub gaps: Vec<Gap>,
    /// Frames per scene label.
    pub scenes: BTreeMap<String, u32>,
    /// Everybody this identity was photographed with, most often first.
    pub companions: Vec<(IdentityId, u32)>,
    /// Distinct photographs, which is not the same as [`IdentityTimeline::appearances`]
    /// length only when a frame holds two faces of one person - a mirror, a screen, a
    /// portrait on a wall.
    pub frames: u32,
}

impl IdentityTimeline {
    /// How long between the first and last appearance, in whole minutes.
    #[must_use]
    pub fn span_minutes(&self) -> i64 {
        match (self.first_ms, self.last_ms) {
            (Some(first), Some(last)) => (last - first) / 60_000,
            _ => 0,
        }
    }

    /// The longest gap, in whole minutes.
    #[must_use]
    pub fn longest_gap_minutes(&self) -> i64 {
        self.gaps.iter().map(Gap::minutes).max().unwrap_or(0)
    }

    /// The frame where this identity is at their best, by quality then by area.
    ///
    /// The People panel's cover photograph. Quality first and area second, in that
    /// order: a large soft face makes a worse card than a smaller sharp one.
    #[must_use]
    pub fn best_frame(&self) -> Option<PhotoId> {
        self.appearances
            .iter()
            .max_by(|left, right| {
                left.best_quality
                    .partial_cmp(&right.best_quality)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| {
                        left.area_frac
                            .partial_cmp(&right.area_frac)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .then_with(|| right.photo.cmp(&left.photo))
            })
            .map(|appearance| appearance.photo)
    }
}

/// Build every identity's timeline.
///
/// `timestamps` maps a photograph to its clock-aligned capture time in milliseconds -
/// `photo.timeline_time`, not `capture_time`, because a second body with a wrong clock
/// would otherwise put a guest at the ceremony an hour before it started. `scenes` is
/// empty until phase 07 runs.
#[must_use]
pub fn build_timelines(
    faces: &[FaceRow],
    timestamps: &BTreeMap<PhotoId, i64>,
    scenes: &BTreeMap<PhotoId, String>,
    graph: &CooccurrenceGraph,
) -> Vec<IdentityTimeline> {
    // Group faces by identity, keeping the best face per frame.
    let mut per_identity: BTreeMap<IdentityId, BTreeMap<PhotoId, Appearance>> = BTreeMap::new();
    for face in faces {
        let Some(identity) = face.identity_id else {
            continue;
        };
        let slot = per_identity
            .entry(identity)
            .or_default()
            .entry(face.image_id)
            .or_insert_with(|| Appearance {
                photo: face.image_id,
                at_ms: timestamps.get(&face.image_id).copied(),
                scene: scenes.get(&face.image_id).cloned(),
                best_quality: 0.0,
                area_frac: 0.0,
            });
        slot.best_quality = slot.best_quality.max(face.quality);
        slot.area_frac = slot.area_frac.max(face.area_frac);
    }

    let mut out = Vec::with_capacity(per_identity.len());
    for (identity, appearances) in per_identity {
        let mut ordered: Vec<Appearance> = appearances.into_values().collect();
        // Timed appearances first, in time order; untimed ones last, in id order. A
        // sort that put `None` first would report a gap from the start of the day.
        ordered.sort_by(|left, right| match (left.at_ms, right.at_ms) {
            (Some(a), Some(b)) => a.cmp(&b).then_with(|| left.photo.cmp(&right.photo)),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => left.photo.cmp(&right.photo),
        });

        let timed: Vec<i64> = ordered.iter().filter_map(|entry| entry.at_ms).collect();
        let first_ms = timed.first().copied();
        let last_ms = timed.last().copied();

        let mut gaps = Vec::new();
        for pair in timed.windows(2) {
            let (Some(from), Some(to)) = (pair.first(), pair.get(1)) else {
                continue;
            };
            if to - from >= MIN_REPORTABLE_GAP_MS {
                gaps.push(Gap {
                    from_ms: *from,
                    to_ms: *to,
                });
            }
        }

        let mut scene_counts: BTreeMap<String, u32> = BTreeMap::new();
        for appearance in &ordered {
            if let Some(scene) = &appearance.scene {
                *scene_counts.entry(scene.clone()).or_insert(0) += 1;
            }
        }

        let frames = u32::try_from(
            ordered
                .iter()
                .map(|entry| entry.photo)
                .collect::<BTreeSet<_>>()
                .len(),
        )
        .unwrap_or(u32::MAX);

        out.push(IdentityTimeline {
            identity,
            appearances: ordered,
            first_ms,
            last_ms,
            gaps,
            scenes: scene_counts,
            companions: graph.companions(identity),
            frames,
        });
    }
    out
}

/// Parse the catalog's RFC 3339 timeline times into milliseconds.
///
/// Reuses `aura_index::store::parse_rfc3339_ms`, which phase 05 wrote for the index's
/// time-window filter. One parser in the product, so a window query and a person
/// timeline can never disagree about when a photograph was taken.
#[must_use]
pub fn parse_timestamp(text: &str) -> Option<i64> {
    aura_index::store::parse_rfc3339_ms(text)
}
