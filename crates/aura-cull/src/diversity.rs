//! Forty near-identical dance frames, and why the gallery does not contain them.
//!
//! PHASE-12 section 2.1: "avoid 40 near-identical dance frames; enforce spread across time,
//! framing (wide/medium/tight) and identity mix."
//!
//! ## Why this is a time cap and not a similarity cap
//!
//! Because the similarity cap already exists. Phase 08 grouped the bursts and produced the
//! duplicate sets, and [`moment_pass`](crate::moment_pass) already refuses to deliver two
//! frames from one of them. What is left over is the failure phase 08 cannot see: forty
//! frames of one song, from forty *different* moments, of the same six people, each of
//! which is a perfectly good photograph and none of which the couple wants.
//!
//! That failure is a *rate* rather than a similarity, so the cap is a rate: at most
//! [`DiversityPolicy::per_window`](crate::rules::DiversityPolicy::per_window) delivered
//! frames per chapter per sliding window. The window is two minutes, which is roughly a
//! song, a speech or one formal group - deliberately not thirty seconds, because thirty
//! seconds is a burst and phase 08 owns bursts.
//!
//! ## Three caps, and the identity one is the loosest on purpose
//!
//! Total, per framing bucket and per dominant identity. The third is set generously
//! because the couple are *supposed* to be in most of the photographs: it exists to stop
//! one guest appearing in eight consecutive delivered frames, not to ration the bride.
//! Getting that backwards would produce a gallery that systematically under-delivers the
//! two people who paid for it.
//!
//! ## Nothing here can drop a protected frame
//!
//! The coverage guard runs before this pass and again after sizing. This pass takes the
//! protected set as an argument and skips every frame in it, which is the second of the
//! three places section 6.4's guarantee is enforced. A photographer's own keep is
//! protected the same way.

use std::collections::{BTreeMap, BTreeSet};

use aura_core::contract::cull::ImageId;
use aura_core::{ChapterId, IdentityId, KeepScore};

use crate::input::{Candidate, CullInput, Framing};
use crate::rules::DiversityPolicy;

/// What the pass removed, and what it removed them in favour of.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DiversityOutcome {
    /// Frames dropped for repetition, each pointing at the strongest frame that stayed in
    /// its window.
    ///
    /// The pointer is what turns "the gallery already carries several frames very like this
    /// one" into something a photographer can check in one click.
    pub dropped: BTreeMap<ImageId, ImageId>,
}

/// Enforce the spread caps over an already chosen gallery.
///
/// Walks each chapter in timeline order, keeping a two-minute window behind the current
/// frame. Deterministic: the walk order is `(timeline_ts, image_id)` and the frame a
/// dropped one points at is the highest-scoring frame in the window with the id as the
/// tie-break.
#[must_use]
pub fn prune(
    input: &CullInput,
    scores: &BTreeMap<ImageId, KeepScore>,
    protected: &BTreeSet<ImageId>,
    policy: DiversityPolicy,
    kept: &mut BTreeSet<ImageId>,
) -> DiversityOutcome {
    let mut outcome = DiversityOutcome::default();

    let mut by_chapter: BTreeMap<ChapterId, Vec<&Candidate>> = BTreeMap::new();
    for candidate in &input.candidates {
        if kept.contains(&candidate.image_id) {
            by_chapter
                .entry(candidate.chapter)
                .or_default()
                .push(candidate);
        }
    }

    for (_, frames) in by_chapter {
        // `input.candidates` is already sorted by (timeline_ts, image_id) and this filter
        // preserves that order, so the window below is a genuine sliding window rather
        // than a set membership test.
        let mut window: Vec<&Candidate> = Vec::new();
        for candidate in frames {
            window.retain(|earlier| {
                candidate.timeline_ts.saturating_sub(earlier.timeline_ts) <= policy.window_ms
            });

            if protected.contains(&candidate.image_id) {
                window.push(candidate);
                continue;
            }

            let total = u32::try_from(window.len()).unwrap_or(u32::MAX);
            let same_framing = u32::try_from(
                window
                    .iter()
                    .filter(|earlier| earlier.framing == candidate.framing)
                    .count(),
            )
            .unwrap_or(u32::MAX);
            let dominant = dominant_identity(candidate);
            let same_identity = u32::try_from(
                window
                    .iter()
                    .filter(|earlier| dominant.is_some() && dominant_identity(earlier) == dominant)
                    .count(),
            )
            .unwrap_or(u32::MAX);

            let crowded = total >= policy.per_window
                || same_framing >= policy.per_window_framing
                || same_identity >= policy.per_window_identity;
            if !crowded {
                window.push(candidate);
                continue;
            }

            let Some(strongest) = window
                .iter()
                .max_by(|left, right| {
                    score_of(scores, left.image_id)
                        .total_cmp(&score_of(scores, right.image_id))
                        .then_with(|| right.image_id.cmp(&left.image_id))
                })
                .map(|earlier| earlier.image_id)
            else {
                window.push(candidate);
                continue;
            };
            kept.remove(&candidate.image_id);
            outcome.dropped.insert(candidate.image_id, strongest);
        }
    }

    outcome
}

/// The identity a frame is most about, when phase 06 named one.
///
/// The first entry of `Candidate::identities`, which phase 06 orders by prominence. `None`
/// for a frame with no faces, which is why the identity cap never fires on details, venue
/// or establishing shots - the three kinds of frame a mix rule has no business touching.
fn dominant_identity(candidate: &Candidate) -> Option<IdentityId> {
    candidate.identities.first().copied()
}

fn score_of(scores: &BTreeMap<ImageId, KeepScore>, image: ImageId) -> f32 {
    scores.get(&image).map_or(0.0, |keep| keep.scene_weighted)
}

/// How many of each framing bucket a set of frames holds.
///
/// Used by the telemetry summary and by the phase gate's spread assertion, not by the
/// pruning itself.
#[must_use]
pub fn framing_histogram(input: &CullInput, kept: &BTreeSet<ImageId>) -> [u32; Framing::ALL.len()] {
    let mut counts = [0u32; Framing::ALL.len()];
    for candidate in &input.candidates {
        if !kept.contains(&candidate.image_id) {
            continue;
        }
        if let Some(slot) = Framing::ALL
            .iter()
            .position(|framing| *framing == candidate.framing)
            .and_then(|index| counts.get_mut(index))
        {
            *slot = slot.saturating_add(1);
        }
    }
    counts
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use aura_core::contract::cull::ImageId;
    use aura_core::{ChapterId, IdentityId, KeepScore, SceneId};
    use uuid::Uuid;

    use super::{framing_histogram, prune};
    use crate::input::{Candidate, CullInput, Framing};
    use crate::rules::DiversityPolicy;

    fn image(n: u128) -> aura_core::contract::cull::ImageId {
        aura_core::PhotoId::from_uuid(Uuid::from_u128(n))
    }

    fn identity(n: u128) -> IdentityId {
        IdentityId::from_uuid(Uuid::from_u128(n))
    }

    fn policy() -> DiversityPolicy {
        DiversityPolicy {
            window_ms: 120_000,
            per_window: 3,
            per_window_framing: 2,
            per_window_identity: 4,
        }
    }

    /// `count` frames, `gap_ms` apart, all in one chapter with one framing and one face.
    fn run(count: u32, gap_ms: i64, framing: Framing) -> (CullInput, BTreeMap<ImageId, KeepScore>) {
        let mut candidates = Vec::new();
        let mut scores = BTreeMap::new();
        for index in 0..count {
            let id = image(u128::from(index) + 1);
            let mut candidate = Candidate::unanalysed(id, gap_ms * i64::from(index));
            candidate.analysed = true;
            candidate.chapter = ChapterId::Dance;
            candidate.scene = SceneId::DanceFloor;
            candidate.framing = framing;
            candidate.identities = vec![identity(1)];
            candidates.push(candidate);
            scores.insert(
                id,
                KeepScore {
                    image_id: id,
                    technical: 0.5,
                    emotion: 0.5,
                    composition: 0.5,
                    prominence: 0.5,
                    // Descending, so the strongest frame is always the first in the window.
                    scene_weighted: 0.9 - 0.01 * f32::from(u16::try_from(index).unwrap_or(0)),
                    scene: SceneId::DanceFloor,
                    calibration_ver: KeepScore::UNCALIBRATED,
                },
            );
        }
        (
            CullInput {
                candidates,
                moments: Vec::new(),
                identities: Vec::new(),
                hours: 8.0,
                couple_unconfirmed: false,
                model_ver: 1,
            },
            scores,
        )
    }

    #[test]
    fn forty_near_identical_dance_frames_do_not_all_reach_the_gallery() {
        // Section 2.1, as the sentence it was written as. Ten frames two seconds apart is one
        // song, and the window cap is what stops all ten being delivered.
        let (input, scores) = run(10, 2_000, Framing::Wide);
        let mut kept: BTreeSet<_> = input.candidates.iter().map(|c| c.image_id).collect();
        let outcome = prune(&input, &scores, &BTreeSet::new(), policy(), &mut kept);
        assert!(kept.len() < 10, "nothing was pruned");
        assert!(!outcome.dropped.is_empty());
    }

    #[test]
    fn a_dropped_frame_points_at_the_strongest_frame_that_stayed() {
        // The pointer is what turns "the gallery already carries several frames very like this
        // one" into something a photographer can check in one click. Without it a drop is an
        // absence, and an absence is not reviewable.
        let (input, scores) = run(10, 2_000, Framing::Wide);
        let mut kept: BTreeSet<_> = input.candidates.iter().map(|c| c.image_id).collect();
        let outcome = prune(&input, &scores, &BTreeSet::new(), policy(), &mut kept);
        for (dropped, instead) in &outcome.dropped {
            assert_ne!(dropped, instead);
            assert!(
                kept.contains(instead),
                "a dropped frame points at another dropped frame"
            );
        }
    }

    #[test]
    fn frames_spread_across_the_day_are_never_pruned_for_repetition() {
        // The cap is over a two-minute window. Ten frames an hour apart repeat nothing.
        let (input, scores) = run(10, 3_600_000, Framing::Wide);
        let mut kept: BTreeSet<_> = input.candidates.iter().map(|c| c.image_id).collect();
        let outcome = prune(&input, &scores, &BTreeSet::new(), policy(), &mut kept);
        assert_eq!(kept.len(), 10);
        assert!(outcome.dropped.is_empty());
    }

    #[test]
    fn a_protected_frame_is_never_pruned_however_crowded_its_window() {
        // A guarantee outranks a preference, in that order, always. A diversity cap is a
        // preference and a coverage guarantee is not.
        let (input, scores) = run(10, 2_000, Framing::Wide);
        let protected: BTreeSet<_> = input
            .candidates
            .iter()
            .map(|candidate| candidate.image_id)
            .collect();
        let mut kept = protected.clone();
        let outcome = prune(&input, &scores, &protected, policy(), &mut kept);
        assert_eq!(kept.len(), 10);
        assert!(outcome.dropped.is_empty());
    }

    #[test]
    fn the_framing_cap_bites_before_the_window_cap_when_every_frame_is_framed_the_same() {
        // Two of three is a tighter bound than three of three, so a run of identically framed
        // frames is thinned by the framing rule rather than by the count. That is the point:
        // "spread across framing" is a different requirement from "not too many".
        let (input, scores) = run(4, 2_000, Framing::Tight);
        let mut kept: BTreeSet<_> = input.candidates.iter().map(|c| c.image_id).collect();
        let _ = prune(&input, &scores, &BTreeSet::new(), policy(), &mut kept);
        assert!(kept.len() <= 2, "kept {}", kept.len());
    }

    #[test]
    fn pruning_is_deterministic() {
        // Section 10.1's determinism gate depends on this, and the walk order is
        // `(timeline_ts, image_id)` for exactly that reason.
        let (input, scores) = run(12, 2_000, Framing::Wide);
        let all: BTreeSet<_> = input.candidates.iter().map(|c| c.image_id).collect();
        let mut first = all.clone();
        let mut second = all;
        let a = prune(&input, &scores, &BTreeSet::new(), policy(), &mut first);
        let b = prune(&input, &scores, &BTreeSet::new(), policy(), &mut second);
        assert_eq!(first, second);
        assert_eq!(a, b);
    }

    #[test]
    fn the_histogram_counts_only_what_is_kept() {
        let (input, _) = run(6, 2_000, Framing::Medium);
        let kept: BTreeSet<_> = input
            .candidates
            .iter()
            .take(2)
            .map(|candidate| candidate.image_id)
            .collect();
        let counts = framing_histogram(&input, &kept);
        assert_eq!(
            counts[Framing::ALL
                .iter()
                .position(|f| *f == Framing::Medium)
                .unwrap()],
            2
        );
        assert_eq!(counts.iter().sum::<u32>(), 2);
    }

    #[test]
    fn a_frame_with_no_faces_is_never_pruned_by_the_identity_rule() {
        // Details, venue and establishing shots are the three kinds of frame a mix rule has no
        // business touching, and they are exactly the ones with no dominant identity.
        let (mut input, scores) = run(10, 2_000, Framing::Wide);
        for candidate in &mut input.candidates {
            candidate.identities.clear();
        }
        let tight_identity = DiversityPolicy {
            per_window: 100,
            per_window_framing: 100,
            per_window_identity: 1,
            ..policy()
        };
        let mut kept: BTreeSet<_> = input.candidates.iter().map(|c| c.image_id).collect();
        let outcome = prune(&input, &scores, &BTreeSet::new(), tight_identity, &mut kept);
        assert!(
            outcome.dropped.is_empty(),
            "a detail shot lost to an identity cap"
        );
    }
}
