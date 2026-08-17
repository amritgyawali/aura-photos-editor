//! How much of the gallery each part of the day gets.
//!
//! PHASE-12 section 6.2, second half:
//!
//! ```text
//! target_c = round(total_target · importance_c · sqrt(volume_c) / sum(...))
//! ```
//!
//! clamped by the min/max bands in `coverage_rules.toml`, "details/venue chapters get
//! small fixed allocations", and "solve the allocation with a greedy pass followed by a
//! bounded local-search improvement".
//!
//! ## Why the square root
//!
//! Because a wedding is not shot at a constant rate and a gallery should not be delivered
//! at one either. Four thousand dance-floor frames and two hundred ceremony frames is a
//! ratio of twenty; under a linear volume term the dance floor would take twenty times the
//! gallery, which is a reception with a wedding attached. Under a square root it takes four
//! and a half times, which the `importance` column then argues with in either direction.
//!
//! ## What the local search is actually for
//!
//! A greedy pass that takes the highest-scoring winners until a chapter is full is already
//! optimal *if the objective is the sum of scores*. It is not. A gallery of forty
//! photographs from eight moments is worse than a gallery of forty from thirty moments
//! even when the first has a higher total score, because the second is a wedding and the
//! first is eight moments photographed forty times.
//!
//! So the objective this pass maximises is
//!
//! ```text
//!     Σ keep_score  +  DISTINCT_MOMENT_BONUS · (number of moments represented)
//! ```
//!
//! and the local search is the bounded pass that trades a chapter's weakest *second*
//! keeper for the strongest *first* keeper of a moment that has none. That is a swap the
//! greedy pass cannot make, it strictly improves the objective when it fires, and it is
//! capped at [`MAX_SWAPS`] so the pass stays inside section 11's 1.5 seconds on four
//! thousand frames.
//!
//! ## What this pass may not do
//!
//! It may not drop a frame a coverage guarantee holds - it runs *before* the guard, so it
//! does not know about them yet, which is exactly why the guard runs afterwards and again
//! after sizing. And it may not empty a chapter that has candidates: the band's `min` is
//! applied after the formula, and a chapter with fewer candidates than its minimum simply
//! delivers all of them.

use std::collections::{BTreeMap, BTreeSet};

use aura_core::contract::cull::ImageId;
use aura_core::{ChapterId, KeepScore};

use crate::moment_pass::MomentOutcome;
use crate::rules::RuleTable;

/// How much one additional represented moment is worth, in score units.
///
/// A tenth of a point. Deliberately small: it is a tie-break between frames of comparable
/// quality, not a licence to deliver a bad photograph because it is from a new moment. A
/// swap only fires when the incoming frame is within this much of the outgoing one, so a
/// moment represented by a genuinely weak frame stays unrepresented.
pub const DISTINCT_MOMENT_BONUS: f32 = 0.10;

/// The most swaps the local search will attempt.
///
/// Section 11 budgets 1.5 seconds for every selection pass over four thousand analysed
/// frames, and an unbounded improvement loop is the one part of this pipeline that could
/// spend all of it. Five hundred and twelve is far more than any real wedding needs -
/// measured, the largest fixture fires eleven - and the bound is here so that a pathological
/// input degrades the *gallery* slightly rather than the *product* completely.
pub const MAX_SWAPS: usize = 512;

/// What each chapter is aiming for.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChapterPlan {
    /// Frames each chapter may deliver.
    pub targets: BTreeMap<ChapterId, u32>,
    /// Frames each chapter was shot with.
    pub volumes: BTreeMap<ChapterId, u32>,
}

impl ChapterPlan {
    /// This chapter's target, or zero for a chapter with no frames.
    #[must_use]
    pub fn target(&self, chapter: ChapterId) -> u32 {
        self.targets.get(&chapter).copied().unwrap_or(0)
    }
}

/// Divide a total gallery size between the chapters.
///
/// Section 6.2's formula. `volumes` is the number of *eligible* frames per chapter, not
/// the number shot: a chapter whose analysis never ran must not be allocated a share of
/// the gallery it cannot fill, because the frames it would fill it with would be the ones
/// nobody checked.
#[must_use]
pub fn plan(
    total_target: u32,
    volumes: &BTreeMap<ChapterId, u32>,
    rules: &RuleTable,
) -> ChapterPlan {
    let mut weights: BTreeMap<ChapterId, f32> = BTreeMap::new();
    let mut total_weight = 0.0f32;
    for (chapter, volume) in volumes {
        if *volume == 0 {
            continue;
        }
        let band = rules.chapter(*chapter);
        let weight = band.importance * (f64::from(*volume).sqrt() as f32);
        weights.insert(*chapter, weight);
        total_weight += weight;
    }

    let mut targets = BTreeMap::new();
    if total_weight <= 0.0 {
        return ChapterPlan {
            targets,
            volumes: volumes.clone(),
        };
    }
    for (chapter, weight) in &weights {
        let band = rules.chapter(*chapter);
        let share = f32::from(u16::try_from(total_target.min(65_535)).unwrap_or(u16::MAX))
            * (weight / total_weight);
        let volume = volumes.get(chapter).copied().unwrap_or(0);
        // Round half up, then clamp by the band, then by what actually exists. The last
        // clamp is why a chapter with eleven frames and a minimum of fifteen delivers
        // eleven rather than being reported as short: the band is a share of the gallery,
        // not a promise about photographs nobody took.
        let rounded = (share + 0.5) as u32;
        targets.insert(*chapter, rounded.clamp(band.min, band.max).min(volume));
    }
    ChapterPlan {
        targets,
        volumes: volumes.clone(),
    }
}

/// One moment winner, tagged with its rank inside its moment.
///
/// Rank zero is a moment's first keeper and is what the distinct-moment objective in this
/// module's header is about: the local search trades a rank-one keeper for another moment's
/// rank-zero one.
#[derive(Clone, Copy)]
struct Entry {
    image: ImageId,
    moment: usize,
    rank: usize,
    score: f32,
}

/// What the allocation kept, and why it dropped the rest.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Allocation {
    /// The frames that survive into the coverage guard, in no particular order.
    pub kept: BTreeSet<ImageId>,
    /// Frames whose fused score is under their scene's floor.
    pub below_floor: BTreeSet<ImageId>,
    /// Frames that lost to a full chapter.
    pub chapter_full: BTreeSet<ImageId>,
    /// How many improving swaps the local search made.
    pub swaps: usize,
    /// How many frames each chapter ended up with.
    pub delivered: BTreeMap<ChapterId, u32>,
}

/// Run the greedy allocation and the bounded improvement pass.
///
/// Deterministic throughout: the candidate order is `(rank within moment ascending, score
/// descending, image id ascending)`, and the swap search visits chapters in id order and
/// candidates in that same order.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn allocate(
    outcomes: &[MomentOutcome],
    chapters: &BTreeMap<ImageId, ChapterId>,
    scores: &BTreeMap<ImageId, KeepScore>,
    floors: &BTreeMap<ImageId, f32>,
    plan: &ChapterPlan,
) -> Allocation {
    let mut allocation = Allocation::default();

    let mut pool: BTreeMap<ChapterId, Vec<Entry>> = BTreeMap::new();
    for (index, outcome) in outcomes.iter().enumerate() {
        for (rank, image) in outcome.winners.iter().enumerate() {
            let score = scores.get(image).map_or(0.0, |keep| keep.scene_weighted);
            let floor = floors.get(image).copied().unwrap_or(0.0);
            if score < floor {
                allocation.below_floor.insert(*image);
                continue;
            }
            let chapter = chapters.get(image).copied().unwrap_or(ChapterId::Other);
            pool.entry(chapter).or_default().push(Entry {
                image: *image,
                moment: index,
                rank,
                score,
            });
        }
    }

    for (chapter, entries) in &mut pool {
        entries.sort_by(|left, right| {
            left.rank
                .cmp(&right.rank)
                .then_with(|| right.score.total_cmp(&left.score))
                .then_with(|| left.image.cmp(&right.image))
        });

        let target = plan.target(*chapter) as usize;
        let mut kept: Vec<Entry> = Vec::new();
        let mut spare: Vec<Entry> = Vec::new();
        for entry in entries.iter().copied() {
            if kept.len() < target {
                kept.push(entry);
            } else {
                spare.push(entry);
            }
        }

        // The bounded improvement pass. One swap: give up the weakest keeper whose moment
        // is already represented, take the strongest spare whose moment is not. Repeat
        // while it improves the objective and the budget allows.
        let mut swaps = 0usize;
        while swaps < MAX_SWAPS {
            let represented: BTreeSet<usize> = kept.iter().map(|entry| entry.moment).collect();
            let Some(incoming) = spare
                .iter()
                .copied()
                .filter(|entry| !represented.contains(&entry.moment))
                .max_by(|left, right| {
                    left.score
                        .total_cmp(&right.score)
                        .then_with(|| right.image.cmp(&left.image))
                })
            else {
                break;
            };
            // Only a frame whose moment survives without it may be given up: dropping a
            // moment's sole keeper to represent another moment trades one gap for another.
            let mut counts: BTreeMap<usize, usize> = BTreeMap::new();
            for entry in &kept {
                *counts.entry(entry.moment).or_insert(0) += 1;
            }
            let Some(outgoing) = kept
                .iter()
                .copied()
                .filter(|entry| counts.get(&entry.moment).copied().unwrap_or(0) > 1)
                .min_by(|left, right| {
                    left.score
                        .total_cmp(&right.score)
                        .then_with(|| left.image.cmp(&right.image))
                })
            else {
                break;
            };
            if incoming.score + DISTINCT_MOMENT_BONUS <= outgoing.score {
                break;
            }
            kept.retain(|entry| entry.image != outgoing.image);
            kept.push(incoming);
            spare.retain(|entry| entry.image != incoming.image);
            spare.push(outgoing);
            swaps += 1;
        }
        allocation.swaps += swaps;

        for entry in &kept {
            allocation.kept.insert(entry.image);
        }
        for entry in &spare {
            allocation.chapter_full.insert(entry.image);
        }
        allocation
            .delivered
            .insert(*chapter, u32::try_from(kept.len()).unwrap_or(u32::MAX));
    }

    allocation
}
