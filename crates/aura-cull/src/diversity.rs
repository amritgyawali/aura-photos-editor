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
