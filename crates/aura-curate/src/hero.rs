//! Portfolio selection: a veto, a blend, and three diversity constraints.
//!
//! Section 6.2: "rank by a weighted blend of technical, emotion, composition, uniqueness (embedding
//! distance from other picks) and story importance", then "enforce diversity: at most N heroes per
//! chapter, at most one per moment, and a spread across framing types, so the portfolio set is not
//! eight versions of the kiss".
//!
//! # Why the blend is arithmetic when phase 12's fusion is geometric
//!
//! Phase 12 fuses its four sub-scores as a geometric mean so that no signal can rescue another, and
//! that has been the right shape for four phases. This one does not inherit it, and the difference
//! matters.
//!
//! Culling decides what is **delivered**. A gallery is the whole record of a wedding, and a frame
//! that is out of focus does not belong in it however extraordinary the moment was - so a
//! multiplicative fusion, where a near-zero term drags the product to near zero whatever the others
//! say, is correct.
//!
//! A portfolio is a **ranking among frames that already passed that test**. Every candidate here is
//! a keeper, so every candidate has already cleared phase 12's vetoes and its geometric fusion.
//! Applying a second multiplicative penalty would re-rank a technically sound set by technical
//! quality again, and the result of that is a portfolio of the sharpest photographs rather than the
//! best ones - which is exactly what the diversity constraints exist to prevent.
//!
//! What this phase keeps is the **veto**: `HERO_TECHNICAL_FLOOR`, applied before any score is
//! computed. Portfolio work is looked at closely by people deciding whether to hire somebody, and a
//! soft frame in that set costs more than a missing one. ADR-0059 section 6.
//!
//! # Why uniqueness is recomputed at every step
//!
//! Because it is a property of the *set*, not of the frame. Section 6.2's own words: "uniqueness
//! uses the Phase 05 index, which is why heroes feel like a curated set rather than a top-scoring
//! list." A uniqueness computed once against the whole gallery would rank a frame by how unusual it
//! is, which selects for oddities; recomputed against what has already been chosen, it selects for
//! variety.
//!
//! On this build the embedding is phase 05's placeholder, so the uniqueness term is a distance
//! between two random projections. The mechanism is real and the number is not a claim about
//! photographs; that is condition C1 of the exit report and it closes with phase 05's C10.

use std::collections::{BTreeMap, BTreeSet};

use aura_core::contract::curate::{
    CurateCode, CurateReason, HeroBinding, HeroPick, HeroTerms, ImageId, ShotScale, MAX_REASONS,
};
use aura_core::contract::ids::MomentId;
use aura_core::contract::scene::ChapterId;

use crate::explain::{blend, measured_share, rank_reasons};
use crate::policy::Policy;
use crate::read::{Field, Frame};

/// One candidate, with everything the selector needs about it.
#[derive(Debug, Clone)]
struct Candidate<'a> {
    frame: &'a Frame,
    technical: f32,
    emotion: Option<f32>,
    composition: Option<f32>,
    story: Option<f32>,
    chapter: ChapterId,
    moment: Option<MomentId>,
    scale: ShotScale,
}

/// Choose the portfolio.
///
/// Deterministic: ties break on image id, the candidate list is filtered and sorted before the greedy
/// loop starts, and the loop's own order is a plain descending scan. The same gallery produces the
/// same twenty heroes on every machine. Invariant 4.
#[must_use]
pub fn select(frames: &[Frame], field: &dyn Field, policy: &Policy) -> Vec<HeroPick> {
    let candidates: Vec<Candidate<'_>> = frames
        .iter()
        .filter_map(|frame| {
            // The veto. A frame with no technical reading is **not** vetoed - it was never
            // measured, and reporting "not sharp enough for portfolio work" about a frame nobody
            // looked at would be a sentence the product cannot support.
            let technical = frame.technical?;
            (technical >= policy.hero_technical_floor).then_some(Candidate {
                frame,
                technical,
                emotion: frame.emotion,
                composition: frame.composition,
                story: frame.narrative,
                chapter: frame.chapter_or_other(),
                moment: frame.moment,
                scale: frame.scale(),
            })
        })
        .collect();

    let mut chosen: Vec<HeroPick> = Vec::new();
    let mut used_moments: BTreeSet<MomentId> = BTreeSet::new();
    let mut per_chapter: BTreeMap<ChapterId, u32> = BTreeMap::new();
    let mut per_scale: BTreeMap<ShotScale, u32> = BTreeMap::new();
    let mut taken: BTreeSet<ImageId> = BTreeSet::new();

    let target = policy.hero_target as usize;
    let scale_ceiling = ((policy.hero_target as f32) * policy.hero_scale_share).ceil() as u32;

    while chosen.len() < target {
        let chosen_ids: Vec<ImageId> = chosen.iter().map(|h| h.image_id).collect();

        // Score every remaining candidate against the set as it stands.
        let mut scored: Vec<(f32, HeroTerms, bool, &Candidate<'_>)> = Vec::new();
        for candidate in &candidates {
            if taken.contains(&candidate.frame.image_id) {
                continue;
            }
            let (uniqueness, unique_known) =
                uniqueness_of(candidate.frame.image_id, &chosen_ids, field);
            let terms = HeroTerms {
                technical: candidate.technical,
                emotion: candidate.emotion.unwrap_or(0.0),
                composition: candidate.composition.unwrap_or(0.0),
                uniqueness,
                story: candidate.story.unwrap_or(0.0),
            };
            let weights = policy.hero_weights;
            let Some(score) = blend(&[
                (weights.technical, terms.technical, true),
                (weights.emotion, terms.emotion, candidate.emotion.is_some()),
                (
                    weights.composition,
                    terms.composition,
                    candidate.composition.is_some(),
                ),
                (weights.uniqueness, terms.uniqueness, unique_known),
                (weights.story, terms.story, candidate.story.is_some()),
            ]) else {
                continue;
            };
            scored.push((score, terms, unique_known, candidate));
        }
        if scored.is_empty() {
            break;
        }

        // Descending, with a total tiebreak so the result does not depend on the sort.
        scored.sort_by(|a, b| {
            b.0.total_cmp(&a.0).then_with(|| {
                a.3.frame
                    .image_id
                    .to_db()
                    .cmp(&b.3.frame.image_id.to_db())
            })
        });

        // Walk down until something passes the three constraints, remembering what stopped the
        // higher-scoring frames on the way. That memory is `binding`, and it is the field a
        // photographer actually reads: two frames from the same kiss can differ by 0.004, and what
        // decided between them was a constraint rather than a score.
        let mut binding = HeroBinding::Unconstrained;
        let mut picked: Option<(f32, HeroTerms, bool, &Candidate<'_>)> = None;
        for entry in scored {
            let (_, _, _, candidate) = &entry;
            let chapter_full = per_chapter
                .get(&candidate.chapter)
                .copied()
                .unwrap_or_default()
                >= policy.heroes_per_chapter;
            if chapter_full {
                binding = HeroBinding::ChapterQuota;
                continue;
            }
            if candidate
                .moment
                .is_some_and(|moment| used_moments.contains(&moment))
            {
                binding = HeroBinding::MomentExhausted;
                continue;
            }
            // A scale nobody could measure is not a scale, and does not consume the quota. Phase
            // 27's rule: an unmeasured value is a third state.
            if candidate.scale.is_known()
                && per_scale.get(&candidate.scale).copied().unwrap_or_default() >= scale_ceiling
            {
                binding = HeroBinding::ScaleQuota;
                continue;
            }
            picked = Some(entry);
            break;
        }

        let Some((score, terms, unique_known, candidate)) = picked else {
            break;
        };

        taken.insert(candidate.frame.image_id);
        if let Some(moment) = candidate.moment {
            used_moments.insert(moment);
        }
        *per_chapter.entry(candidate.chapter).or_default() += 1;
        if candidate.scale.is_known() {
            *per_scale.entry(candidate.scale).or_default() += 1;
        }

        let measured = measured_share(&[
            (policy.hero_weights.technical, true),
            (policy.hero_weights.emotion, candidate.emotion.is_some()),
            (
                policy.hero_weights.composition,
                candidate.composition.is_some(),
            ),
            (policy.hero_weights.uniqueness, unique_known),
            (policy.hero_weights.story, candidate.story.is_some()),
        ]);

        let mut reasons = Vec::new();
        if terms.emotion >= 0.7 {
            reasons.push(CurateReason::plain(CurateCode::EmotionalPeak, terms.emotion));
        }
        if terms.composition >= 0.7 {
            reasons.push(CurateReason::plain(
                CurateCode::StrongComposition,
                terms.composition,
            ));
        }
        if terms.technical >= 0.85 {
            reasons.push(CurateReason::plain(
                CurateCode::TechnicalExcellence,
                terms.technical,
            ));
        }
        if unique_known && terms.uniqueness >= 0.6 {
            reasons.push(CurateReason::plain(CurateCode::UniqueFrame, terms.uniqueness));
        }
        if !unique_known {
            reasons.push(CurateReason::plain(
                CurateCode::UniquenessUnavailable,
                -0.05,
            ));
        }
        if terms.story >= 0.6 {
            reasons.push(CurateReason::plain(CurateCode::StoryImportant, terms.story));
        }
        match binding {
            HeroBinding::ChapterQuota => reasons.push(CurateReason::plain(
                CurateCode::ChapterQuotaBinding,
                -0.20,
            )),
            HeroBinding::MomentExhausted => reasons.push(CurateReason::plain(
                CurateCode::MomentAlreadyRepresented,
                -0.20,
            )),
            HeroBinding::ScaleQuota => {
                reasons.push(CurateReason::plain(CurateCode::ScaleQuotaBinding, -0.20));
            }
            HeroBinding::Unconstrained => {}
        }
        crate::explain::ensure_reason(
            &mut reasons,
            CurateCode::TechnicalExcellence,
            terms.technical,
        );
        rank_reasons(&mut reasons, MAX_REASONS);

        chosen.push(HeroPick {
            image_id: candidate.frame.image_id,
            rank: chosen.len() as u32,
            score,
            terms,
            chapter: candidate.chapter,
            moment: candidate.moment,
            scale: candidate.scale,
            binding,
            reasons,
            // Invariant 2. Tempered by how much of the blend was measured *and* by how strong the
            // frame is: a hero at 0.95 with every term present is a suggestion worth trusting, and
            // one at 0.58 with two terms is a suggestion to look at.
            confidence: (0.30 + 0.45 * measured + 0.25 * score).clamp(0.0, 1.0),
            accepted: None,
        });
    }

    chosen
}

/// How unlike the already-chosen heroes a frame is, and whether that could be measured.
///
/// `(1.0, false)` when nothing could be measured, which is a **skipped** term: the caller drops
/// uniqueness from the blend and renormalises rather than treating an unmeasurable frame as
/// maximally unique. The value is 1.0 rather than 0.0 only so that a caller which ignored the flag
/// would fail safe toward variety rather than toward eight versions of the kiss.
fn uniqueness_of(image: ImageId, chosen: &[ImageId], field: &dyn Field) -> (f32, bool) {
    if chosen.is_empty() {
        // The first hero has nothing to be unlike. Fully unique, and honestly so.
        return (1.0, true);
    }
    let readings = field.similarity(image, chosen);
    let mut worst: Option<f32> = None;
    for reading in readings.iter().flatten() {
        worst = Some(worst.map_or(*reading, |w: f32| w.max(*reading)));
    }
    match worst {
        Some(similarity) => ((1.0 - similarity).clamp(0.0, 1.0), true),
        None => (1.0, false),
    }
}

/// How many chapters contributed at least one hero.
#[must_use]
pub fn chapters_covered(heroes: &[HeroPick]) -> u32 {
    heroes
        .iter()
        .map(|h| h.chapter)
        .collect::<BTreeSet<_>>()
        .len() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use aura_core::contract::cull::CoverageReport;
    use aura_core::contract::ids::IdentityId;
    use aura_core::{AuraResult, ProjectId};

    /// A field whose similarity answers come from a table, so a test can say exactly which frames
    /// look alike.
    #[derive(Debug, Default)]
    struct TestField {
        /// `(a, b) -> similarity`, symmetric.
        pairs: Mutex<BTreeMap<(String, String), f32>>,
        /// When true, every similarity is `None`: the uniqueness term is unmeasurable.
        blind: bool,
    }

    impl TestField {
        fn alike(&self, a: ImageId, b: ImageId, similarity: f32) {
            let mut pairs = self.pairs.lock().unwrap();
            pairs.insert((a.to_db(), b.to_db()), similarity);
            pairs.insert((b.to_db(), a.to_db()), similarity);
        }
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
        fn skin_bands(
            &self,
            _project: ProjectId,
        ) -> AuraResult<BTreeMap<IdentityId, u8>> {
            Ok(BTreeMap::new())
        }
        fn similarity(&self, from: ImageId, others: &[ImageId]) -> Vec<Option<f32>> {
            if self.blind {
                return vec![None; others.len()];
            }
            let pairs = self.pairs.lock().unwrap();
            others
                .iter()
                .map(|other| {
                    Some(
                        pairs
                            .get(&(from.to_db(), other.to_db()))
                            .copied()
                            .unwrap_or(0.1),
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

    fn frame(order: u32, chapter: ChapterId, technical: f32, emotion: f32) -> Frame {
        let mut f = Frame::bare(ImageId::new(), order);
        f.chapter = Some(chapter);
        f.technical = Some(technical);
        f.emotion = Some(emotion);
        f.composition = Some(0.7);
        f.narrative = Some(0.5);
        f
    }

    #[test]
    fn a_frame_below_the_technical_floor_is_never_a_candidate() {
        let policy = Policy::default();
        let field = TestField::default();
        let frames = vec![
            frame(0, ChapterId::Ceremony, 0.40, 0.99),
            frame(1, ChapterId::Ceremony, 0.90, 0.60),
        ];
        let heroes = select(&frames, &field, &policy);
        assert_eq!(heroes.len(), 1);
        assert_eq!(heroes[0].image_id, frames[1].image_id);
    }

    #[test]
    fn a_frame_with_no_technical_reading_is_skipped_rather_than_vetoed() {
        // "Not sharp enough for portfolio work" is a sentence about a measurement. A frame nobody
        // measured must not get it.
        let policy = Policy::default();
        let field = TestField::default();
        let mut unmeasured = frame(0, ChapterId::Ceremony, 0.9, 0.9);
        unmeasured.technical = None;
        let heroes = select(&[unmeasured], &field, &policy);
        assert!(heroes.is_empty());
    }

    #[test]
    fn no_chapter_contributes_more_than_its_quota() {
        let policy = Policy::default();
        let field = TestField::default();
        let frames: Vec<Frame> = (0..12)
            .map(|i| frame(i, ChapterId::Ceremony, 0.9, 0.9))
            .collect();
        let heroes = select(&frames, &field, &policy);
        assert_eq!(heroes.len() as u32, policy.heroes_per_chapter);
        assert!(heroes
            .iter()
            .all(|h| h.chapter == ChapterId::Ceremony));
    }

    #[test]
    fn at_most_one_hero_comes_from_a_moment() {
        let policy = Policy::default();
        let field = TestField::default();
        let moment = MomentId::new();
        let mut frames: Vec<Frame> = (0..4)
            .map(|i| frame(i, ChapterId::Ceremony, 0.9, 0.9))
            .collect();
        for f in &mut frames {
            f.moment = Some(moment);
        }
        // A second chapter so the quota is not what stops it.
        frames.push(frame(9, ChapterId::Dance, 0.9, 0.8));
        let heroes = select(&frames, &field, &policy);
        let from_moment = heroes.iter().filter(|h| h.moment == Some(moment)).count();
        assert_eq!(from_moment, 1, "eight versions of the kiss");
    }

    #[test]
    fn the_binding_constraint_is_recorded_on_the_pick_that_got_through() {
        let policy = Policy::default();
        let field = TestField::default();
        let moment = MomentId::new();
        // Four very strong frames from one moment, then a weaker one from elsewhere. The weaker one
        // is chosen second, and the reason is the moment rather than the score.
        let mut frames: Vec<Frame> = (0..4)
            .map(|i| frame(i, ChapterId::Ceremony, 0.95, 0.95))
            .collect();
        for f in &mut frames {
            f.moment = Some(moment);
        }
        frames.push(frame(9, ChapterId::Dance, 0.80, 0.60));
        let heroes = select(&frames, &field, &policy);
        assert!(heroes.len() >= 2);
        assert_eq!(heroes[1].binding, HeroBinding::MomentExhausted);
        assert!(heroes[1]
            .reasons
            .iter()
            .any(|r| r.code == CurateCode::MomentAlreadyRepresented));
    }

    #[test]
    fn uniqueness_is_measured_against_what_has_already_been_chosen() {
        let policy = Policy::default();
        let field = TestField::default();
        let a = frame(0, ChapterId::Ceremony, 0.90, 0.90);
        let b = frame(1, ChapterId::Dance, 0.90, 0.89);
        let c = frame(2, ChapterId::Portraits, 0.90, 0.88);
        // b is nearly identical to a; c is not. Even though b scores higher on emotion, the second
        // hero should be c once a is in the set.
        field.alike(a.image_id, b.image_id, 0.98);
        field.alike(a.image_id, c.image_id, 0.05);
        let heroes = select(&[a.clone(), b.clone(), c.clone()], &field, &policy);
        assert_eq!(heroes[0].image_id, a.image_id);
        assert_eq!(
            heroes[1].image_id, c.image_id,
            "a near-duplicate of the first hero must not be the second"
        );
    }

    #[test]
    fn an_unmeasurable_uniqueness_is_dropped_from_the_blend_and_named() {
        let policy = Policy::default();
        let field = TestField {
            blind: true,
            ..TestField::default()
        };
        let frames = vec![
            frame(0, ChapterId::Ceremony, 0.9, 0.9),
            frame(1, ChapterId::Dance, 0.9, 0.8),
        ];
        let heroes = select(&frames, &field, &policy);
        assert!(heroes.len() >= 2);
        // The first hero has nothing to be unlike, so it is honestly unique. The second cannot be
        // measured against it.
        assert!(heroes[1]
            .reasons
            .iter()
            .any(|r| r.code == CurateCode::UniquenessUnavailable));
        assert!(
            heroes[1].confidence < heroes[0].confidence,
            "a narrower blend is a less confident suggestion"
        );
    }

    #[test]
    fn the_result_is_the_same_on_every_run() {
        let policy = Policy::default();
        let field = TestField::default();
        let frames: Vec<Frame> = (0..30)
            .map(|i| {
                frame(
                    i,
                    ChapterId::ALL[(i as usize) % ChapterId::COUNT],
                    0.85,
                    0.85,
                )
            })
            .collect();
        let first = select(&frames, &field, &policy);
        let second = select(&frames, &field, &policy);
        assert_eq!(first.len(), second.len());
        for (a, b) in first.iter().zip(&second) {
            assert_eq!(a.image_id, b.image_id);
            assert_eq!(a.rank, b.rank);
        }
    }

    #[test]
    fn ranks_are_dense_and_start_at_zero() {
        let policy = Policy::default();
        let field = TestField::default();
        let frames: Vec<Frame> = (0..25)
            .map(|i| {
                frame(
                    i,
                    ChapterId::ALL[(i as usize) % ChapterId::COUNT],
                    0.85,
                    0.85,
                )
            })
            .collect();
        let heroes = select(&frames, &field, &policy);
        for (ix, hero) in heroes.iter().enumerate() {
            assert_eq!(hero.rank as usize, ix);
            assert!(hero.is_well_formed(), "{hero:?}");
        }
    }

    #[test]
    fn every_hero_carries_a_reason() {
        let policy = Policy::default();
        let field = TestField::default();
        // Deliberately mediocre: every threshold in the reason list falls the wrong way.
        let frames: Vec<Frame> = (0..3)
            .map(|i| {
                let mut f = frame(i, ChapterId::ALL[i as usize], 0.60, 0.10);
                f.composition = Some(0.10);
                f.narrative = Some(0.05);
                f
            })
            .collect();
        let heroes = select(&frames, &field, &policy);
        assert!(!heroes.is_empty());
        for hero in &heroes {
            assert!(!hero.reasons.is_empty(), "invariant 2");
        }
    }
}
