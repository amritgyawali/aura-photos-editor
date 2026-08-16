//! Which frames of one moment are worth delivering.
//!
//! PHASE-12 section 6.2, first half. This is the pass that answers the question phase 08
//! spent a whole phase making askable: of the fourteen frames the photographer shot of one
//! instant, which are the photographs?
//!
//! ## `k` is a function of variety, not of quality
//!
//! Section 6.2: "k = 1 for low-diversity moments, up to 3-5 for high-diversity,
//! high-significance moments (dance sequences, bouquet toss, ritual peaks)."
//!
//! The important word is *diversity*. A moment of fourteen frames that are all the same
//! photograph earns one keeper however good they are - keeping three would be delivering
//! the same photograph three times. A moment of four frames where the action evolved earns
//! three, even if each of them scores slightly lower, because they are three different
//! photographs.
//!
//! Significance is the second term and it is the tie-break between two equally varied
//! moments: a varied dance sequence and a varied set of frames of the car park are not
//! owed the same number of keepers.
//!
//! ## Every moment contributes at least one frame
//!
//! `k` is never zero. A moment with an eligible frame always puts its best one forward,
//! and if the chapter quota cannot afford it the *chapter pass* drops it with a reason -
//! not this pass, silently, by allocating zero. The difference matters because "this
//! moment lost to a quota" is an explanation and "this moment was never considered" is a
//! bug.
//!
//! ## The peak is either selected or explicitly rejected
//!
//! Section 6.2: "guarantee the moment's peak frame (Phase 10) is either selected or
//! explicitly rejected with a reason - never silently dropped." [`MomentOutcome::peak`]
//! and [`MomentOutcome::peak_selected`] carry both halves, and the engine attaches
//! `CullCode::PeakRejected` when the second is false. The phase gate counts them.
//!
//! A *flat* peak - phase 10's `PeakKind::Flat`, where no frame could be separated from its
//! neighbours - is deliberately not protected. Protecting an arbitrary frame of fourteen
//! near-identical ones as "the peak" would be this phase acting on a claim phase 10
//! explicitly refused to make.
//!
//! ## Near-identical frames cannot both win
//!
//! Phase 08's duplicate sets are `Identical` and `NearIdentical` only - it deliberately
//! stopped producing `Variant` sets - so everything in one really is the same photograph
//! twice. The first winner from a group takes it and the rest are suppressed with
//! `CullCode::NearDuplicate` and a pointer to the frame that won.

use std::collections::{BTreeMap, BTreeSet};

use aura_core::contract::cull::ImageId;
use aura_core::{CullMode, KeepScore, MomentId};

use crate::input::{Candidate, CullInput};
use crate::modes::{self, Tuning};
use crate::weights::WeightTable;

/// What the pass decided about one moment.
///
/// A moment with no eligible frames still produces an outcome, with empty winners - so
/// that the engine can explain every frame in it rather than skipping the group entirely.
#[derive(Debug, Clone, PartialEq)]
pub struct MomentOutcome {
    /// The moment, or `None` for a frame that belongs to no moment.
    pub moment_id: Option<MomentId>,
    /// The chosen frames, strongest first.
    pub winners: Vec<ImageId>,
    /// Every eligible frame ranked, strongest first. The order every later pass reuses.
    pub ranked: Vec<(ImageId, f32)>,
    /// The best frame that was not chosen.
    ///
    /// One per moment rather than one per winner: with `k = 3` and five frames, all three
    /// winners point at the fourth, which is the honest answer to "what would replace
    /// this" for each of them. Phase 27 consumes it.
    pub runner_up: Option<ImageId>,
    /// How many keepers this moment was allowed.
    pub k: u32,
    /// The moment's peak frame, when phase 10 named one that was not flat.
    pub peak: Option<ImageId>,
    /// True when that peak is among the winners.
    pub peak_selected: bool,
    /// Frames that lost only because a near-identical sibling won, and which one did.
    pub suppressed: BTreeMap<ImageId, ImageId>,
}

impl MomentOutcome {
    /// True when this moment held exactly one eligible frame.
    ///
    /// What earns `CullCode::OnlyCandidate` rather than `CullCode::MomentWinner`, because
    /// "the strongest frame of this moment" is a misleading thing to say about a group of
    /// one.
    #[must_use]
    pub fn is_sole(&self) -> bool {
        self.ranked.len() == 1
    }
}

/// How much of `k` comes from variety rather than from significance.
///
/// Section 6.2 names diversity first and significance second, and this is that ordering as
/// a number. Variety dominates because it answers "are these different photographs", which
/// is the question; significance only answers "does this moment deserve the benefit of the
/// doubt".
pub const DIVERSITY_SHARE: f32 = 0.65;

/// Rank every moment and choose its winners.
///
/// Deterministic: moments are visited in id order, frames within a moment in `(score
/// descending, image id ascending)` order, and every tie ends on the id. Two machines
/// given the same [`CullInput`] produce the same vector.
#[must_use]
pub fn run(
    input: &CullInput,
    scores: &BTreeMap<ImageId, KeepScore>,
    table: &WeightTable,
    mode: CullMode,
) -> Vec<MomentOutcome> {
    let mut by_moment: BTreeMap<MomentId, Vec<&Candidate>> = BTreeMap::new();
    let mut loose: Vec<&Candidate> = Vec::new();
    for candidate in &input.candidates {
        match candidate.moment_id {
            Some(moment) => by_moment.entry(moment).or_default().push(candidate),
            None => loose.push(candidate),
        }
    }

    let mut out = Vec::with_capacity(by_moment.len() + loose.len());
    for (moment_id, frames) in by_moment {
        let info = input.moment(moment_id);
        let tuning = tuning_for(table, mode, &frames);
        let diversity = info.map_or(0.0, |m| m.diversity).clamp(0.0, 1.0);
        let significance = info.map_or(0.0, |m| m.significance).clamp(0.0, 1.0);
        out.push(choose(
            Some(moment_id),
            &frames,
            scores,
            &tuning,
            diversity,
            significance,
        ));
    }

    // A frame in no moment is a moment of one. Modelling it that way rather than as a
    // special case means every later pass sees one kind of thing, and the only visible
    // difference is the `NoMoment` reason and the confidence penalty the fusion applied.
    for candidate in loose {
        let frames = [candidate];
        let tuning = tuning_for(table, mode, &frames);
        out.push(choose(None, &frames, scores, &tuning, 0.0, 0.0));
    }
    out
}

/// The tuning for a group, taken from its first frame's scene.
///
/// A moment's frames share a scene at almost every wedding - that is what phase 08 grouped
/// them on - and the exception is a moment straddling a boundary, where taking the first
/// frame's scene is as defensible as any other rule and is at least deterministic.
fn tuning_for(table: &WeightTable, mode: CullMode, frames: &[&Candidate]) -> Tuning {
    let scene = frames
        .first()
        .map_or(aura_core::SceneId::Unknown, |c| c.scene);
    modes::tune(table, mode, scene)
}

/// Choose the winners of one group.
fn choose(
    moment_id: Option<MomentId>,
    frames: &[&Candidate],
    scores: &BTreeMap<ImageId, KeepScore>,
    tuning: &Tuning,
    diversity: f32,
    significance: f32,
) -> MomentOutcome {
    let mut ranked: Vec<(ImageId, f32)> = frames
        .iter()
        .filter(|candidate| candidate.is_eligible())
        .filter_map(|candidate| {
            scores
                .get(&candidate.image_id)
                .map(|score| (candidate.image_id, score.scene_weighted))
        })
        .collect();
    // Descending by score, ascending by id. The second key is the determinism guarantee:
    // two frames from two cameras can score identically, and a sort that left them in
    // input order would make the gallery depend on how the catalog returned rows.
    ranked.sort_by(|left, right| {
        right
            .1
            .total_cmp(&left.1)
            .then_with(|| left.0.cmp(&right.0))
    });

    let k = keeper_count(tuning.max_keepers, diversity, significance, ranked.len());
    let groups: BTreeMap<ImageId, usize> = frames
        .iter()
        .filter_map(|candidate| {
            candidate
                .duplicate_group
                .map(|group| (candidate.image_id, group))
        })
        .collect();

    let mut winners = Vec::new();
    let mut taken_groups = BTreeSet::new();
    let mut suppressed = BTreeMap::new();
    for (image, _) in &ranked {
        if winners.len() >= k as usize {
            break;
        }
        match groups.get(image) {
            Some(group) if !taken_groups.insert(*group) => {
                // A near-identical sibling already won. Record which one, so the rejection
                // can point at it rather than saying "a duplicate" and leaving the
                // photographer to find it.
                if let Some(winner) = winners
                    .iter()
                    .find(|candidate| groups.get(*candidate) == Some(group))
                {
                    suppressed.insert(*image, *winner);
                }
            }
            _ => winners.push(*image),
        }
    }

    let runner_up = ranked
        .iter()
        .map(|(image, _)| *image)
        .find(|image| !winners.contains(image));

    let peak = frames
        .iter()
        .find(|candidate| candidate.is_peak && !candidate.peak_flat && candidate.is_eligible())
        .map(|candidate| candidate.image_id);
    let peak_selected = peak.is_some_and(|image| winners.contains(&image));

    MomentOutcome {
        moment_id,
        winners,
        ranked,
        runner_up,
        k,
        peak,
        peak_selected,
        suppressed,
    }
}

/// How many keepers a moment earns.
///
/// Section 6.2's rule as arithmetic. One plus a share of the scene's headroom, where the
/// share is a blend of how varied the moment is and how much it matters - never more than
/// the moment actually holds, and never fewer than one when it holds anything.
#[must_use]
pub fn keeper_count(max_keepers: u32, diversity: f32, significance: f32, available: usize) -> u32 {
    if available == 0 {
        return 0;
    }
    let headroom = f32::from(u16::try_from(max_keepers.saturating_sub(1)).unwrap_or(0));
    let blend = (DIVERSITY_SHARE * diversity.clamp(0.0, 1.0)
        + (1.0 - DIVERSITY_SHARE) * significance.clamp(0.0, 1.0))
    .clamp(0.0, 1.0);
    // `+ 0.5` then truncate is a round-half-up, written out rather than using `round()`
    // so that the tie at exactly one half is documented: a moment on the boundary gets
    // the extra frame, which is the conservative direction and the one this phase leans
    // in everywhere else.
    let extra = (headroom * blend + 0.5) as u32;
    let allowed = 1u32.saturating_add(extra).min(max_keepers.max(1));
    allowed.min(u32::try_from(available).unwrap_or(u32::MAX))
}
