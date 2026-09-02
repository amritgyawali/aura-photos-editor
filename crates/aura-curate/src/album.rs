//! Composing the album: allocate, guarantee, fill, pair, then improve.
//!
//! Section 6.3: "start from chapter order, allocate spread counts proportionally to chapter
//! importance and duration, then fill with the highest-value images subject to coverage rules",
//! then rhythm, then spread pairing, then "guarantee coverage: must-have moments and close-family
//! members appear in the album".
//!
//! The order those five things happen in is the whole design, and it is not the order the phase
//! document lists them in.
//!
//! # Coverage runs first, as a filter
//!
//! An album is 60 to 120 images out of a gallery of hundreds, so it is a far tighter selection than
//! the cull that produced the gallery - and the frames a coverage rule protects are
//! disproportionately frames that scored moderately. A coverage *term* in the fill objective would
//! lose to two beautiful portraits every single time, and the album would arrive without the ring
//! exchange in it.
//!
//! So [`allocate`] reserves a slot for every must-have the gallery covers and for every close-family
//! identity the album would otherwise under-carry, and the value ranking fills what is left. Phase
//! 12 wrote this rule, phase 23 applied it to crop safety, phase 24 made it a property of the type
//! system, phase 27 applied it to replacements. This is the fifth application. ADR-0059 section 7.
//!
//! One thing this deliberately does not do: it does not force a must-have into the album when the
//! *gallery* has no frame for it. `Coverage::Missing` propagates from phase 12 and is reported.
//! The product cannot invent coverage, and an album is not the place to start.
//!
//! # Chapters never move
//!
//! [`optimise`] only proposes swaps inside one chapter's span, and every path that could reorder an
//! album checks [`AlbumPlan::chapters_are_ordered`] afterwards. A wedding album whose ceremony
//! follows its reception is not an album with an unusual sequence; it is an album that is wrong.
//!
//! # The search is bounded rather than converged
//!
//! `policy.swap_passes` passes over the sequence, deterministic in order, each swap accepted only
//! when the combined rhythm-and-pairing objective improves. Invariant 4 requires the same gallery to
//! produce the same album on every machine, and an annealer is reproducible only with a pinned seed
//! and a pinned iteration count - at which point it is a bounded deterministic search with a worse
//! failure mode, because a swap accepted because the temperature was high is a swap nobody can
//! explain. ADR-0059 section 12.

use std::collections::{BTreeMap, BTreeSet};

use aura_core::contract::cull::{Coverage, CoverageReport, MustHave};
use aura_core::contract::curate::{
    AlbumPlan, ChapterSpan, CurateCode, CurateReason, ImageId, ShotScale, Spread, SpreadPair,
    IMAGES_PER_SPREAD, MAX_REASONS,
};
use aura_core::contract::ids::{IdentityId, SpreadId};
use aura_core::contract::scene::ChapterId;

use crate::policy::Policy;
use crate::read::{Field, Frame};
use crate::spread;

/// Everything the composer needs that is not a frame.
#[derive(Debug, Clone)]
pub struct Context {
    /// Phase 12's own coverage report over the gallery. The album's report is a subset of it.
    pub gallery_coverage: CoverageReport,
    /// The identities phase 12's rules treat as close family, and how many frames each needs.
    pub close_family: (Vec<IdentityId>, u32),
    /// A photographer's own order, when they have set one. Never overwritten by a pass.
    pub user_order: Option<Vec<ImageId>>,
}

/// Compose one album.
///
/// `target_size` is in **images**, inside `policy.album_min ..= policy.album_max`; the caller has
/// already clamped it.
#[must_use]
pub fn compose(
    frames: &[Frame],
    context: &Context,
    field: &dyn Field,
    policy: &Policy,
    target_size: u32,
) -> AlbumPlan {
    if frames.is_empty() {
        return AlbumPlan::empty(target_size);
    }

    let by_id: BTreeMap<ImageId, &Frame> = frames.iter().map(|f| (f.image_id, f)).collect();

    // A photographer's order replaces the selection and the sequence both. Everything after this -
    // the pairing, the coverage report, the scores - is still computed, because the panel shows
    // them and because phase 30's learning loop reads the difference between what they chose and
    // what AURA would have.
    let chosen: Vec<ImageId> = match &context.user_order {
        Some(order) => order
            .iter()
            .copied()
            .filter(|id| by_id.contains_key(id))
            .collect(),
        None => allocate(frames, context, policy, target_size),
    };
    let user_ordered = context.user_order.is_some();

    let mut spreads = lay_out(&chosen, &by_id, field, policy, !user_ordered);
    if !user_ordered {
        optimise(&mut spreads, &by_id, field, policy);
    }
    renumber(&mut spreads);

    let chapter_map = spans(&spreads, frames, policy, target_size);
    let coverage = album_coverage(&chosen, &by_id, context);
    let (rhythm_score, rhythm_measurable) = rhythm(&spreads, &by_id, policy);
    let pairing_score = pairing(&spreads);

    let mut reasons = Vec::new();
    if user_ordered {
        reasons.push(CurateReason::plain(CurateCode::UserOrdered, 1.0));
    }
    for span in &chapter_map {
        if !span.is_satisfied() {
            reasons.push(CurateReason::detailed(
                CurateCode::ChapterUnderAllocated,
                format!(
                    "{} has {} spreads rather than the {} AURA planned, because there were not \
                     enough frames",
                    span.chapter.as_str(),
                    span.len,
                    span.target
                ),
                -0.2,
            ));
        }
    }
    if rhythm_measurable < 0.33 {
        reasons.push(CurateReason::detailed(
            CurateCode::RhythmUnmeasurable,
            format!(
                "AURA could only tell how close the photographer was on {:.0}% of the album, so \
                 the rhythm score is not worth reading",
                rhythm_measurable * 100.0
            ),
            -0.3,
        ));
    }
    let size = spreads.iter().map(|s| s.len() as u32).sum();

    AlbumPlan {
        spreads,
        chapter_map,
        coverage,
        rhythm_score,
        rhythm_measurable,
        pairing_score,
        size,
        target_size,
        user_ordered,
        reasons,
    }
}

/// Choose which frames are in the album, coverage first.
///
/// Returns them in **timeline order**, which is chapter order because phase 07's chapters are
/// contiguous in time. The sequence inside a chapter is what [`optimise`] later improves.
#[must_use]
pub fn allocate(
    frames: &[Frame],
    context: &Context,
    policy: &Policy,
    target_size: u32,
) -> Vec<ImageId> {
    let target = target_size as usize;
    let mut chosen: BTreeSet<ImageId> = BTreeSet::new();

    // --- the guarantees, before any ranking -------------------------------------------------
    //
    // A rule the gallery misses stays missed: the product cannot invent coverage.
    let covered_in_gallery: BTreeSet<MustHave> = context
        .gallery_coverage
        .must_haves
        .iter()
        .filter(|(_, state)| state.is_satisfied())
        .map(|(rule, _)| *rule)
        .collect();

    for rule in &covered_in_gallery {
        let best = frames
            .iter()
            .filter(|f| f.satisfies.contains(rule))
            .max_by(|a, b| value(a).total_cmp(&value(b)));
        if let Some(frame) = best {
            chosen.insert(frame.image_id);
        }
    }

    let (family, minimum) = &context.close_family;
    for identity in family {
        let have = chosen
            .iter()
            .filter(|id| {
                frames
                    .iter()
                    .find(|f| f.image_id == **id)
                    .is_some_and(|f| f.identities.contains(identity))
            })
            .count() as u32;
        if have >= *minimum {
            continue;
        }
        let mut candidates: Vec<&Frame> = frames
            .iter()
            .filter(|f| f.identities.contains(identity) && !chosen.contains(&f.image_id))
            .collect();
        candidates.sort_by(|a, b| value(b).total_cmp(&value(a)));
        for frame in candidates.into_iter().take((*minimum - have) as usize) {
            chosen.insert(frame.image_id);
        }
    }

    // --- what each chapter gets, by importance times duration ---------------------------------
    let mut per_chapter: BTreeMap<ChapterId, usize> = BTreeMap::new();
    for frame in frames {
        *per_chapter.entry(frame.chapter_or_other()).or_default() += 1;
    }
    let quotas = apportion(&per_chapter, policy, target);

    // --- fill by value, inside each chapter's quota --------------------------------------------
    for (chapter, quota) in &quotas {
        let mut candidates: Vec<&Frame> = frames
            .iter()
            .filter(|f| f.chapter_or_other() == *chapter && !chosen.contains(&f.image_id))
            .collect();
        candidates.sort_by(|a, b| {
            value(b)
                .total_cmp(&value(a))
                .then_with(|| a.image_id.to_db().cmp(&b.image_id.to_db()))
        });
        let already = frames
            .iter()
            .filter(|f| f.chapter_or_other() == *chapter && chosen.contains(&f.image_id))
            .count();
        let room = quota.saturating_sub(already);
        for frame in candidates.into_iter().take(room) {
            chosen.insert(frame.image_id);
        }
    }

    // --- top up or trim to the target ----------------------------------------------------------
    //
    // Trimming never removes a guarantee. `chosen` already holds every protected frame, and this
    // walks the *unprotected* ones from the bottom - so an album that came out over target loses a
    // portrait rather than the ring exchange.
    let protected: BTreeSet<ImageId> = frames
        .iter()
        .filter(|f| {
            !f.satisfies.is_empty()
                && f.satisfies.iter().any(|r| covered_in_gallery.contains(r))
                && chosen.contains(&f.image_id)
        })
        .map(|f| f.image_id)
        .collect();

    if chosen.len() > target {
        let mut removable: Vec<&Frame> = frames
            .iter()
            .filter(|f| chosen.contains(&f.image_id) && !protected.contains(&f.image_id))
            .collect();
        removable.sort_by(|a, b| {
            value(a)
                .total_cmp(&value(b))
                .then_with(|| a.image_id.to_db().cmp(&b.image_id.to_db()))
        });
        for frame in removable {
            if chosen.len() <= target {
                break;
            }
            chosen.remove(&frame.image_id);
        }
    } else if chosen.len() < target {
        let mut spare: Vec<&Frame> = frames
            .iter()
            .filter(|f| !chosen.contains(&f.image_id))
            .collect();
        spare.sort_by(|a, b| {
            value(b)
                .total_cmp(&value(a))
                .then_with(|| a.image_id.to_db().cmp(&b.image_id.to_db()))
        });
        for frame in spare {
            if chosen.len() >= target {
                break;
            }
            chosen.insert(frame.image_id);
        }
    }

    let mut order: Vec<&Frame> = frames
        .iter()
        .filter(|f| chosen.contains(&f.image_id))
        .collect();
    order.sort_by_key(|f| (f.order, f.image_id.to_db()));
    order.into_iter().map(|f| f.image_id).collect()
}

/// How many images each chapter gets, by importance times duration.
///
/// Largest-remainder apportionment, which is deterministic and gives every chapter with any frames
/// at least one. Ties in the remainder break on the chapter's own order, so two chapters that want
/// the same fraction of a page get it in wedding order rather than in map order.
#[must_use]
pub fn apportion(
    per_chapter: &BTreeMap<ChapterId, usize>,
    policy: &Policy,
    target: usize,
) -> Vec<(ChapterId, usize)> {
    let mut weights: Vec<(ChapterId, f32)> = ChapterId::ALL
        .iter()
        .filter_map(|chapter| {
            let count = per_chapter.get(chapter).copied().unwrap_or(0);
            (count > 0).then(|| (*chapter, policy.importance(*chapter) * count as f32))
        })
        .collect();
    let total: f32 = weights.iter().map(|(_, w)| *w).sum();
    if total <= f32::EPSILON || weights.is_empty() {
        return Vec::new();
    }

    let mut out: Vec<(ChapterId, usize)> = Vec::with_capacity(weights.len());
    let mut remainders: Vec<(ChapterId, f32)> = Vec::with_capacity(weights.len());
    let mut assigned = 0usize;
    for (chapter, weight) in &weights {
        let exact = (*weight / total) * target as f32;
        let whole = exact.floor() as usize;
        // A chapter with frames gets at least one image, so an album never simply omits a part of
        // the day that was photographed.
        let floor_value = whole.max(1);
        assigned += floor_value;
        out.push((*chapter, floor_value));
        remainders.push((*chapter, exact - exact.floor()));
    }

    // Hand out what is left, largest remainder first.
    remainders.sort_by(|a, b| {
        b.1.total_cmp(&a.1).then_with(|| {
            let ax = ChapterId::ALL.iter().position(|c| *c == a.0).unwrap_or(0);
            let bx = ChapterId::ALL.iter().position(|c| *c == b.0).unwrap_or(0);
            ax.cmp(&bx)
        })
    });
    let mut ix = 0usize;
    while assigned < target && !remainders.is_empty() {
        let Some((chapter, _)) = remainders.get(ix % remainders.len()) else {
            break;
        };
        if let Some(slot) = out.iter_mut().find(|(c, _)| c == chapter) {
            // Never allocate a chapter more images than it has frames.
            let available = per_chapter.get(chapter).copied().unwrap_or(0);
            if slot.1 < available {
                slot.1 += 1;
                assigned += 1;
            }
        }
        ix += 1;
        // Every chapter is at its ceiling; there is nothing more to hand out.
        if ix > remainders.len() * target.max(1) {
            break;
        }
    }
    // Cap each chapter at what it actually has.
    for (chapter, quota) in &mut out {
        let available = per_chapter.get(chapter).copied().unwrap_or(0);
        *quota = (*quota).min(available);
    }
    weights.clear();
    out
}

/// How far ahead the layout looks for a frame that can face the one on the left.
///
/// Four. The whole point is that a refused pair should cost a small reordering rather than a page,
/// and a window this size covers the common cause - a run of frames from one moment - without
/// moving a frame far enough that a reader notices the sequence is not chronological.
pub const PAIR_LOOKAHEAD: usize = 4;

/// Turn an ordered list of images into spreads, chapter by chapter.
///
/// A chapter never shares a spread with another chapter: the fold is where a reader turns the page,
/// and a spread whose left page is the ceremony and whose right is the reception reads as an
/// editing mistake. So a chapter with an odd number of images ends on a **single**, which is a
/// design a photographer recognises.
///
/// # Why it looks ahead
///
/// The first version of this function paired `order[ix]` with `order[ix + 1]` and made a single
/// whenever that pair was refused. Every unit test passed - the constraints were enforced, the
/// chapters held, no near-duplicate faced another - and the phase gate reported **75 spreads for 80
/// images**, which is an album of seventy single pages.
///
/// The cause is that the constraints refuse exactly the pairs a timeline order produces most often:
/// two frames of one moment are adjacent in time, so they are adjacent in the album, so the
/// near-duplicate rule fires on nearly every pair. Refusing was right; giving up was not.
///
/// So a refused pair now costs a small reordering rather than a page: the next frame *inside the
/// same chapter* that can face this one is brought forward, up to [`PAIR_LOOKAHEAD`] positions.
/// Reordering inside a chapter is what the optimiser does anyway, and the optimiser could never have
/// fixed this - it trades right pages between existing pairs, and there were no pairs to trade.
///
/// # Why `may_reorder` exists
///
/// Because the look-ahead broke a guarantee the moment it was added, and the phase gate caught it on
/// the next run. A photographer who drags a frame has chosen an **adjacency**, not just an order;
/// bringing a different frame forward to make a pair work is AURA moving their album. So on a
/// user-ordered album the look-ahead is switched off entirely: the pages are exactly what they
/// dragged, and a pair that cannot be permitted becomes a single. The operating manual's fifth code
/// rule - a parameter a person set is never overwritten - applied to a sequence.
#[must_use]
pub fn lay_out(
    order: &[ImageId],
    by_id: &BTreeMap<ImageId, &Frame>,
    field: &dyn Field,
    policy: &Policy,
    may_reorder: bool,
) -> Vec<Spread> {
    // A working copy, because a refused pair is fixed by moving a frame forward rather than by
    // leaving a page blank. The caller's order is unchanged.
    let mut remaining: Vec<ImageId> = order.to_vec();
    let mut spreads = Vec::new();
    let mut ix = 0usize;

    while ix < remaining.len() {
        let Some(left_id) = remaining.get(ix).copied() else {
            break;
        };
        let Some(left) = by_id.get(&left_id) else {
            ix += 1;
            continue;
        };
        let chapter = left.chapter_or_other();

        // Look for the nearest frame inside this chapter that may face the left page. The first
        // candidate is the next frame, which is the common case and costs no reordering at all.
        let mut chosen: Option<(usize, ImageId, SpreadPair)> = None;
        let window = if may_reorder { PAIR_LOOKAHEAD } else { 1 };
        for offset in 1..=window {
            let Some(candidate_id) = remaining.get(ix + offset).copied() else {
                break;
            };
            let Some(candidate) = by_id.get(&candidate_id) else {
                continue;
            };
            // Never past the chapter's own end: the fold is where a reader turns the page.
            if candidate.chapter_or_other() != chapter {
                break;
            }
            let similarity = one_similarity(field, left, candidate_id);
            if spread::permitted(left, candidate, similarity, policy) {
                chosen = Some((
                    ix + offset,
                    candidate_id,
                    spread::measure(left, candidate, similarity, policy),
                ));
                break;
            }
        }

        let (right_id, pair, single) = match chosen {
            Some((position, id, pair)) => {
                // Bring it forward. `remove` then `insert` rather than a swap, because a swap would
                // send the displaced frame backwards past pages already laid out.
                remaining.remove(position);
                remaining.insert(ix + 1, id);
                (Some(id), pair, false)
            }
            None => (None, SpreadPair::none(), true),
        };

        let reasons = spread::reasons(&pair, single, policy);
        spreads.push(Spread {
            id: SpreadId::new(),
            index: spreads.len() as u32,
            left: Some(left_id),
            right: right_id,
            single,
            chapter,
            pair,
            reasons,
        });
        ix += if right_id.is_some() { 2 } else { 1 };
    }
    spreads
}

/// Improve the sequence by bounded local search inside each chapter.
///
/// Adjacent spreads only, and only inside one chapter's span. Each swap is accepted when the
/// combined rhythm-and-pairing objective over the two spreads improves; ties are rejected, so a
/// sequence that has stopped improving stops changing and the pass is idempotent.
pub fn optimise(
    spreads: &mut [Spread],
    by_id: &BTreeMap<ImageId, &Frame>,
    field: &dyn Field,
    policy: &Policy,
) {
    for _ in 0..policy.swap_passes {
        let mut changed = false;
        for ix in 0..spreads.len().saturating_sub(1) {
            let (Some(a), Some(b)) = (spreads.get(ix), spreads.get(ix + 1)) else {
                continue;
            };
            if a.chapter != b.chapter {
                continue;
            }
            // Try trading the right page of `a` for the right page of `b`. That is the smallest
            // move that changes two pairings at once, which is what a pairing objective can
            // actually be improved by - swapping whole spreads changes the rhythm and nothing else.
            let (Some(a_right), Some(b_right)) = (a.right, b.right) else {
                continue;
            };
            let (Some(a_left), Some(b_left)) = (a.left, b.left) else {
                continue;
            };
            let before = a.pair.score + b.pair.score;

            let (Some(a_left_frame), Some(b_left_frame)) = (by_id.get(&a_left), by_id.get(&b_left))
            else {
                continue;
            };
            let (Some(a_right_frame), Some(b_right_frame)) =
                (by_id.get(&a_right), by_id.get(&b_right))
            else {
                continue;
            };

            let sim_a = one_similarity(field, a_left_frame, b_right);
            let sim_b = one_similarity(field, b_left_frame, a_right);
            if !spread::permitted(a_left_frame, b_right_frame, sim_a, policy)
                || !spread::permitted(b_left_frame, a_right_frame, sim_b, policy)
            {
                continue;
            }
            let new_a = spread::measure(a_left_frame, b_right_frame, sim_a, policy);
            let new_b = spread::measure(b_left_frame, a_right_frame, sim_b, policy);
            let after = new_a.score + new_b.score;

            if after > before + f32::EPSILON {
                if let Some(slot) = spreads.get_mut(ix) {
                    slot.right = Some(b_right);
                    slot.pair = new_a;
                    slot.reasons = spread::reasons(&new_a, false, policy);
                    slot.reasons.push(CurateReason::plain(
                        CurateCode::RhythmImproved,
                        after - before,
                    ));
                    crate::explain::rank_reasons(&mut slot.reasons, MAX_REASONS);
                }
                if let Some(slot) = spreads.get_mut(ix + 1) {
                    slot.right = Some(a_right);
                    slot.pair = new_b;
                    slot.reasons = spread::reasons(&new_b, false, policy);
                }
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

/// Renumber spreads after any reordering.
pub fn renumber(spreads: &mut [Spread]) {
    for (ix, spread) in spreads.iter_mut().enumerate() {
        spread.index = ix as u32;
    }
}

/// Which spreads belong to which chapter, in wedding order.
#[must_use]
pub fn spans(
    spreads: &[Spread],
    frames: &[Frame],
    policy: &Policy,
    target_size: u32,
) -> Vec<ChapterSpan> {
    let mut per_chapter: BTreeMap<ChapterId, usize> = BTreeMap::new();
    for frame in frames {
        *per_chapter.entry(frame.chapter_or_other()).or_default() += 1;
    }
    let quotas: BTreeMap<ChapterId, usize> = apportion(&per_chapter, policy, target_size as usize)
        .into_iter()
        .collect();

    let mut out: Vec<ChapterSpan> = Vec::new();
    let mut first = 0u32;
    let mut current: Option<ChapterId> = None;
    let mut len = 0u32;
    for spread in spreads {
        match current {
            Some(chapter) if chapter == spread.chapter => len += 1,
            Some(chapter) => {
                out.push(ChapterSpan {
                    chapter,
                    first,
                    len,
                    target: target_spreads(&quotas, chapter),
                });
                first += len;
                len = 1;
                current = Some(spread.chapter);
            }
            None => {
                current = Some(spread.chapter);
                len = 1;
            }
        }
    }
    if let Some(chapter) = current {
        out.push(ChapterSpan {
            chapter,
            first,
            len,
            target: target_spreads(&quotas, chapter),
        });
    }
    out
}

/// How many spreads a chapter's image quota implies.
fn target_spreads(quotas: &BTreeMap<ChapterId, usize>, chapter: ChapterId) -> u32 {
    let images = quotas.get(&chapter).copied().unwrap_or(0) as u32;
    images.div_ceil(IMAGES_PER_SPREAD)
}

/// The album's coverage, as a subset of the gallery's.
///
/// A rule the gallery covers and the album carries is `Covered`; one the gallery covers weakly and
/// the album carries stays `CoveredWeak`, because the album cannot make a forced-in frame better
/// than it is; one the album does not carry is `Missing`; and one the gallery itself misses is
/// `Missing` whatever the album does.
///
/// Reusing phase 12's own vocabulary and phase 12's own answer about which frames satisfy which
/// rule. There is no second coverage engine in this product.
#[must_use]
pub fn album_coverage(
    chosen: &[ImageId],
    by_id: &BTreeMap<ImageId, &Frame>,
    context: &Context,
) -> CoverageReport {
    let in_album: BTreeSet<ImageId> = chosen.iter().copied().collect();
    let satisfied: BTreeSet<MustHave> = chosen
        .iter()
        .filter_map(|id| by_id.get(id))
        .flat_map(|f| f.satisfies.iter().copied())
        .collect();

    let must_haves: Vec<(MustHave, Coverage)> = context
        .gallery_coverage
        .must_haves
        .iter()
        .map(|(rule, gallery_state)| {
            let state = match gallery_state {
                Coverage::Missing => Coverage::Missing,
                other if satisfied.contains(rule) => *other,
                _ => Coverage::Missing,
            };
            (*rule, state)
        })
        .collect();

    let identity_coverage: Vec<(IdentityId, u32)> = context
        .gallery_coverage
        .identity_coverage
        .iter()
        .map(|(identity, _)| {
            let count = in_album
                .iter()
                .filter_map(|id| by_id.get(id))
                .filter(|f| f.identities.contains(identity))
                .count() as u32;
            (*identity, count)
        })
        .collect();

    let mut chapter_counts: BTreeMap<ChapterId, u32> = BTreeMap::new();
    for frame in in_album.iter().filter_map(|id| by_id.get(id)) {
        *chapter_counts.entry(frame.chapter_or_other()).or_default() += 1;
    }

    let mut warnings = Vec::new();
    let (family, minimum) = &context.close_family;
    for identity in family {
        let count = identity_coverage
            .iter()
            .find(|(id, _)| id == identity)
            .map_or(0, |(_, c)| *c);
        if count < *minimum {
            warnings.push(format!(
                "somebody close to the couple appears {count} times in the album and should appear \
                 at least {minimum}; there were not enough frames of them in the gallery"
            ));
        }
    }
    for (rule, state) in &must_haves {
        if *state == Coverage::Missing {
            warnings.push(format!(
                "the album has no photograph of {} - {}",
                MustHave::title(*rule),
                if context
                    .gallery_coverage
                    .must_haves
                    .iter()
                    .any(|(r, s)| r == rule && *s == Coverage::Missing)
                {
                    "and neither does the gallery"
                } else {
                    "though the gallery does"
                }
            ));
        }
    }

    CoverageReport {
        must_haves,
        identity_coverage,
        chapter_counts: chapter_counts.into_iter().map(|(c, n)| (c, n, n)).collect(),
        warnings,
    }
}

/// How well the sequence matches its chapters' target patterns, and over how much of it.
///
/// **Two numbers, always.** Frames whose shot scale could not be measured are excluded from the
/// denominator rather than counted as misses, so a chapter of unmeasurable frames scores *nothing*
/// rather than zero - and `measurable` is what tells a caller whether the first number means
/// anything. On this build it is near zero.
#[must_use]
pub fn rhythm(
    spreads: &[Spread],
    by_id: &BTreeMap<ImageId, &Frame>,
    policy: &Policy,
) -> (f32, f32) {
    let mut position: BTreeMap<ChapterId, usize> = BTreeMap::new();
    let mut matched = 0u32;
    let mut measured = 0u32;
    let mut total = 0u32;

    for spread in spreads {
        for image in spread.images() {
            total += 1;
            let Some(frame) = by_id.get(&image) else {
                continue;
            };
            let slot = position.entry(spread.chapter).or_default();
            let pattern = policy.pattern(spread.chapter);
            let want = pattern.get(*slot % pattern.len().max(1)).copied();
            *slot += 1;

            let scale = frame.scale();
            if scale == ShotScale::Unknown {
                continue;
            }
            measured += 1;
            if want == Some(scale) {
                matched += 1;
            }
        }
    }

    let score = if measured == 0 {
        0.0
    } else {
        matched as f32 / measured as f32
    };
    let measurable = if total == 0 {
        0.0
    } else {
        measured as f32 / total as f32
    };
    (score, measurable)
}

/// The mean pairing score over the spreads that carry two images.
///
/// Singles are excluded from the denominator, not scored at zero. A single is a design decision
/// rather than a failed pairing, and an album that is half singles because its chapters have odd
/// lengths should not report a pairing score of 0.5.
#[must_use]
pub fn pairing(spreads: &[Spread]) -> f32 {
    let pairs: Vec<&Spread> = spreads.iter().filter(|s| !s.single).collect();
    if pairs.is_empty() {
        return 0.0;
    }
    pairs.iter().map(|s| s.pair.score).sum::<f32>() / pairs.len() as f32
}

/// How much an album wants this frame, `0..1`.
///
/// Phase 12's keep score, lifted by emotion and composition where they were measured. Not a fusion
/// with its own weights: an album is a selection from a gallery that has already been ranked, and a
/// second complete ranking here would be a second answer to "which of these is better".
#[must_use]
pub fn value(frame: &Frame) -> f32 {
    let terms: [(f32, f32, bool); 3] = [
        (0.5, frame.keep_score, true),
        (0.3, frame.emotion.unwrap_or(0.0), frame.emotion.is_some()),
        (
            0.2,
            frame.composition.unwrap_or(0.0),
            frame.composition.is_some(),
        ),
    ];
    crate::explain::blend(&terms).unwrap_or(frame.keep_score)
}

/// One similarity reading, or `None` when it could not be measured.
fn one_similarity(field: &dyn Field, from: &Frame, to: ImageId) -> Option<f32> {
    field
        .similarity(from.image_id, &[to])
        .first()
        .copied()
        .flatten()
}

/// Check a photographer's order before it is stored.
///
/// Three refusals, and none of them is anything a photographer did wrong: an order that reorders
/// chapters, one naming an image that is not in the gallery, and one repeating an image. ADR-0060
/// section 4.
///
/// # Errors
///
/// `AURA-ML-5143`.
pub fn check_order(order: &[ImageId], frames: &[Frame]) -> Result<(), aura_core::AuraError> {
    let by_id: BTreeMap<ImageId, &Frame> = frames.iter().map(|f| (f.image_id, f)).collect();
    let mut seen: BTreeSet<ImageId> = BTreeSet::new();
    let mut last_chapter: Option<usize> = None;
    let mut visited: BTreeSet<usize> = BTreeSet::new();

    for image in order {
        let Some(frame) = by_id.get(image) else {
            return Err(crate::errors::decision_refused(format!(
                "{image} is not in this wedding's gallery"
            )));
        };
        if !seen.insert(*image) {
            return Err(crate::errors::decision_refused(format!(
                "{image} appears twice in the order"
            )));
        }
        let chapter = frame.chapter_or_other();
        let Some(position) = ChapterId::ALL.iter().position(|c| *c == chapter) else {
            continue;
        };
        match last_chapter {
            Some(previous) if position == previous => {}
            Some(_) | None => {
                if !visited.insert(position) {
                    return Err(crate::errors::chapters_reordered());
                }
                if last_chapter.is_some_and(|previous| position < previous) {
                    return Err(crate::errors::chapters_reordered());
                }
                last_chapter = Some(position);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use aura_core::contract::ids::MomentId;
    use aura_core::{AuraResult, ProjectId};
    use aura_index::contract::index::LumaStats;

    use crate::read::Descriptor;

    #[derive(Debug, Default)]
    struct TestField {
        pairs: Mutex<BTreeMap<(String, String), f32>>,
    }

    impl Field for TestField {
        fn frames(&self, _project: ProjectId) -> AuraResult<Vec<Frame>> {
            Ok(Vec::new())
        }
        fn photo_count(&self, _project: ProjectId) -> AuraResult<u32> {
            Ok(0)
        }
        fn gallery_coverage(&self, _project: ProjectId) -> AuraResult<CoverageReport> {
            Ok(CoverageReport::default())
        }
        fn skin_bands(&self, _project: ProjectId) -> AuraResult<BTreeMap<IdentityId, u8>> {
            Ok(BTreeMap::new())
        }
        fn similarity(&self, from: ImageId, others: &[ImageId]) -> Vec<Option<f32>> {
            let pairs = self.pairs.lock().unwrap();
            others
                .iter()
                .map(|other| {
                    Some(
                        pairs
                            .get(&(from.to_db(), other.to_db()))
                            .copied()
                            .unwrap_or(0.4),
                    )
                })
                .collect()
        }
        fn rituals(&self, _project: ProjectId) -> AuraResult<Vec<String>> {
            Ok(Vec::new())
        }
        fn close_family(&self, _project: ProjectId) -> AuraResult<(Vec<IdentityId>, u32)> {
            Ok((Vec::new(), 0))
        }
    }

    fn frame(order: u32, chapter: ChapterId, keep: f32) -> Frame {
        let mut f = Frame::bare(ImageId::new(), order);
        f.chapter = Some(chapter);
        f.keep_score = keep;
        f.emotion = Some(keep);
        f.composition = Some(0.6);
        f.descriptor = Some(Descriptor {
            hsv_hist: vec![0u8; 512],
            luma: LumaStats {
                mean: 0.5,
                p1: 0.0,
                p50: 0.5,
                p99: 1.0,
                clip_lo: 0.0,
                clip_hi: 0.0,
            },
            edge_energy: 0.2,
        });
        f.warmth_k = Some(5000.0);
        f
    }

    fn gallery(per_chapter: &[(ChapterId, usize)]) -> Vec<Frame> {
        let mut out = Vec::new();
        let mut order = 0u32;
        for (chapter, count) in per_chapter {
            for i in 0..*count {
                out.push(frame(order, *chapter, 0.5 + (i as f32) * 0.001));
                order += 1;
            }
        }
        out
    }

    fn context(rules: &[(MustHave, Coverage)]) -> Context {
        Context {
            gallery_coverage: CoverageReport {
                must_haves: rules.to_vec(),
                identity_coverage: Vec::new(),
                chapter_counts: Vec::new(),
                warnings: Vec::new(),
            },
            close_family: (Vec::new(), 0),
            user_order: None,
        }
    }

    #[test]
    fn every_chapter_with_frames_gets_at_least_one_image() {
        let policy = Policy::default();
        let mut per_chapter = BTreeMap::new();
        per_chapter.insert(ChapterId::Ceremony, 400);
        per_chapter.insert(ChapterId::Exit, 3);
        let quotas: BTreeMap<ChapterId, usize> =
            apportion(&per_chapter, &policy, 80).into_iter().collect();
        assert!(quotas.get(&ChapterId::Exit).copied().unwrap_or(0) >= 1);
        assert!(quotas.get(&ChapterId::Ceremony).copied().unwrap_or(0) > 10);
    }

    #[test]
    fn a_chapter_is_never_allocated_more_images_than_it_has() {
        let policy = Policy::default();
        let mut per_chapter = BTreeMap::new();
        per_chapter.insert(ChapterId::Ceremony, 2);
        per_chapter.insert(ChapterId::Reception, 500);
        let quotas: BTreeMap<ChapterId, usize> =
            apportion(&per_chapter, &policy, 80).into_iter().collect();
        assert!(quotas.get(&ChapterId::Ceremony).copied().unwrap_or(0) <= 2);
    }

    #[test]
    fn a_must_have_the_gallery_covers_is_in_the_album_even_when_its_frame_scored_badly() {
        // The whole of ADR-0059 section 7. The ring exchange frame is the *worst* in the gallery.
        let policy = Policy::default();
        let field = TestField::default();
        let mut frames = gallery(&[(ChapterId::Ceremony, 200), (ChapterId::Reception, 200)]);
        let rings = frames.get_mut(0).expect("a frame");
        rings.keep_score = 0.01;
        rings.emotion = Some(0.01);
        rings.composition = Some(0.01);
        rings.satisfies = vec![MustHave::Rings];
        let rings_id = rings.image_id;

        let ctx = context(&[(MustHave::Rings, Coverage::Covered)]);
        let chosen = allocate(&frames, &ctx, &policy, 80);
        assert!(
            chosen.contains(&rings_id),
            "a coverage guarantee is a filter, never a term"
        );
        drop(field);
    }

    #[test]
    fn a_must_have_the_gallery_misses_is_reported_rather_than_invented() {
        let policy = Policy::default();
        let frames = gallery(&[(ChapterId::Ceremony, 100)]);
        let ctx = context(&[(MustHave::Cake, Coverage::Missing)]);
        let chosen = allocate(&frames, &ctx, &policy, 60);
        let by_id: BTreeMap<ImageId, &Frame> = frames.iter().map(|f| (f.image_id, f)).collect();
        let report = album_coverage(&chosen, &by_id, &ctx);
        assert_eq!(report.missing(), vec![MustHave::Cake]);
        assert!(report
            .warnings
            .iter()
            .any(|w| w.contains("neither does the gallery")));
    }

    #[test]
    fn a_must_have_the_gallery_covers_and_the_album_drops_is_reported_differently() {
        let frames = gallery(&[(ChapterId::Ceremony, 10)]);
        let ctx = context(&[(MustHave::Kiss, Coverage::Covered)]);
        let by_id: BTreeMap<ImageId, &Frame> = frames.iter().map(|f| (f.image_id, f)).collect();
        // Nothing in the album satisfies the rule.
        let report = album_coverage(&[], &by_id, &ctx);
        assert_eq!(report.missing(), vec![MustHave::Kiss]);
        assert!(report
            .warnings
            .iter()
            .any(|w| w.contains("though the gallery does")));
    }

    #[test]
    fn the_albums_chapters_are_in_wedding_order() {
        let policy = Policy::default();
        let field = TestField::default();
        let frames = gallery(&[
            (ChapterId::GettingReady, 40),
            (ChapterId::Ceremony, 80),
            (ChapterId::Reception, 60),
            (ChapterId::Dance, 40),
        ]);
        let ctx = context(&[]);
        let plan = compose(&frames, &ctx, &field, &policy, 80);
        assert!(plan.chapters_are_ordered(), "{:?}", plan.chapter_map);
        assert!(plan.size > 0);
    }

    #[test]
    fn a_chapter_never_shares_a_spread_with_another_chapter() {
        let policy = Policy::default();
        let field = TestField::default();
        let frames = gallery(&[
            (ChapterId::Ceremony, 30),
            (ChapterId::Reception, 30),
            (ChapterId::Dance, 30),
        ]);
        let ctx = context(&[]);
        let plan = compose(&frames, &ctx, &field, &policy, 60);
        let by_id: BTreeMap<ImageId, &Frame> = frames.iter().map(|f| (f.image_id, f)).collect();
        for spread in &plan.spreads {
            for image in spread.images() {
                let Some(frame) = by_id.get(&image) else {
                    continue;
                };
                assert_eq!(frame.chapter_or_other(), spread.chapter);
            }
        }
    }

    #[test]
    fn no_spread_faces_two_frames_from_the_same_moment() {
        // Section 10.1's own property test.
        let policy = Policy::default();
        let field = TestField::default();
        let mut frames = gallery(&[(ChapterId::Ceremony, 60)]);
        let moment = MomentId::new();
        for frame in frames.iter_mut().take(20) {
            frame.moment = Some(moment);
        }
        let ctx = context(&[]);
        let plan = compose(&frames, &ctx, &field, &policy, 60);
        let by_id: BTreeMap<ImageId, &Frame> = frames.iter().map(|f| (f.image_id, f)).collect();
        for spread in &plan.spreads {
            let (Some(left), Some(right)) = (spread.left, spread.right) else {
                continue;
            };
            let (Some(a), Some(b)) = (by_id.get(&left), by_id.get(&right)) else {
                continue;
            };
            assert!(
                a.moment.is_none() || a.moment != b.moment,
                "two frames of one shot are facing each other"
            );
        }
    }

    #[test]
    fn a_rhythm_measured_over_nothing_reports_nothing_rather_than_zero() {
        let policy = Policy::default();
        let field = TestField::default();
        // No faces and no scale-bearing scenes: every frame is `ShotScale::Unknown`.
        let frames = gallery(&[(ChapterId::Ceremony, 40)]);
        let ctx = context(&[]);
        let plan = compose(&frames, &ctx, &field, &policy, 60);
        assert_eq!(plan.rhythm_measurable, 0.0);
        assert_eq!(plan.rhythm_score, 0.0);
        assert!(plan
            .reasons
            .iter()
            .any(|r| r.code == CurateCode::RhythmUnmeasurable));
    }

    #[test]
    fn a_rhythm_that_matches_its_pattern_scores_well() {
        let policy = Policy::default();
        let field = TestField::default();
        // Ceremony's pattern is wide, medium, tight, medium. Details is tight by definition and
        // Venue is wide, so a chapter built out of those two has a measurable scale.
        let mut frames = gallery(&[(ChapterId::Details, 20)]);
        for frame in &mut frames {
            frame.scene = Some(aura_core::contract::scene::SceneId::Details);
        }
        let ctx = context(&[]);
        let plan = compose(&frames, &ctx, &field, &policy, 60);
        assert!(plan.rhythm_measurable > 0.9, "{}", plan.rhythm_measurable);
        // The details pattern is tight, tight, medium; every frame is tight, so two of every three
        // positions match.
        assert!(plan.rhythm_score > 0.5, "{}", plan.rhythm_score);
    }

    #[test]
    fn the_same_gallery_produces_the_same_album_twice() {
        let policy = Policy::default();
        let field = TestField::default();
        let frames = gallery(&[
            (ChapterId::GettingReady, 30),
            (ChapterId::Ceremony, 60),
            (ChapterId::Reception, 50),
        ]);
        let ctx = context(&[]);
        let a = compose(&frames, &ctx, &field, &policy, 80);
        let b = compose(&frames, &ctx, &field, &policy, 80);
        assert_eq!(a.spreads.len(), b.spreads.len());
        for (x, y) in a.spreads.iter().zip(&b.spreads) {
            assert_eq!(x.left, y.left);
            assert_eq!(x.right, y.right);
            assert_eq!(x.chapter, y.chapter);
        }
        assert_eq!(a.size, b.size);
    }

    #[test]
    fn a_photographers_order_is_used_as_it_is() {
        let policy = Policy::default();
        let field = TestField::default();
        let frames = gallery(&[(ChapterId::Ceremony, 20)]);
        let order: Vec<ImageId> = frames.iter().rev().map(|f| f.image_id).collect();
        let mut ctx = context(&[]);
        ctx.user_order = Some(order.clone());
        let plan = compose(&frames, &ctx, &field, &policy, 60);
        assert!(plan.user_ordered);
        assert_eq!(plan.images(), order);
        assert!(plan
            .reasons
            .iter()
            .any(|r| r.code == CurateCode::UserOrdered));
    }

    #[test]
    fn an_order_that_reorders_chapters_is_refused_and_one_inside_a_chapter_is_not() {
        let frames = gallery(&[(ChapterId::Ceremony, 4), (ChapterId::Reception, 4)]);
        let in_order: Vec<ImageId> = frames.iter().map(|f| f.image_id).collect();
        assert!(check_order(&in_order, &frames).is_ok());

        // Reversing inside the ceremony is fine.
        let mut inside = in_order.clone();
        inside.swap(0, 3);
        assert!(check_order(&inside, &frames).is_ok());

        // Putting the reception first is not.
        let mut swapped: Vec<ImageId> = in_order[4..].to_vec();
        swapped.extend_from_slice(&in_order[..4]);
        let err = check_order(&swapped, &frames).unwrap_err();
        assert_eq!(err.code.0, "AURA-ML-5143");
    }

    #[test]
    fn an_order_naming_an_unknown_image_or_repeating_one_is_refused() {
        let frames = gallery(&[(ChapterId::Ceremony, 4)]);
        let mut order: Vec<ImageId> = frames.iter().map(|f| f.image_id).collect();
        order.push(ImageId::new());
        assert!(check_order(&order, &frames).is_err());

        let repeated = vec![frames[0].image_id, frames[0].image_id];
        assert!(check_order(&repeated, &frames).is_err());
    }

    #[test]
    fn an_empty_gallery_produces_an_empty_album_rather_than_a_failure() {
        let policy = Policy::default();
        let field = TestField::default();
        let ctx = Context {
            gallery_coverage: CoverageReport::default(),
            close_family: (Vec::new(), 0),
            user_order: None,
        };
        let plan = compose(&[], &ctx, &field, &policy, 80);
        assert!(plan.spreads.is_empty());
        assert_eq!(plan.size, 0);
        assert!(plan.chapters_are_ordered());
    }

    #[test]
    fn a_single_image_gallery_produces_one_single_spread() {
        let policy = Policy::default();
        let field = TestField::default();
        let frames = gallery(&[(ChapterId::Ceremony, 1)]);
        let ctx = context(&[]);
        let plan = compose(&frames, &ctx, &field, &policy, 60);
        assert_eq!(plan.spreads.len(), 1);
        assert!(plan.spreads[0].single);
        assert!(plan.spreads[0].is_well_formed());
        assert_eq!(plan.size, 1);
    }

    #[test]
    fn every_spread_is_well_formed() {
        let policy = Policy::default();
        let field = TestField::default();
        let frames = gallery(&[
            (ChapterId::GettingReady, 7),
            (ChapterId::Ceremony, 33),
            (ChapterId::Exit, 1),
        ]);
        let ctx = context(&[]);
        let plan = compose(&frames, &ctx, &field, &policy, 60);
        for spread in &plan.spreads {
            assert!(spread.is_well_formed(), "{spread:?}");
            assert!(!spread.reasons.is_empty(), "invariant 2");
        }
    }
}
