//! The wedding-night teaser: six slots, filled by emotional impact, topped up by variety.
//!
//! Section 6.4: "the teaser set is optimised for immediate emotional impact and fast delivery:
//! hero, couple, ceremony peak, one family, one detail, one dance".
//!
//! # Why the six slots are filled before the top-up
//!
//! Because the set is sent at one in the morning to a couple who have not seen a single photograph
//! yet, and what they want is *their day* rather than the six best frames from one hour of it. Six
//! named slots is a filter, not a scoring term, for the same reason the album's coverage is a
//! filter: an impact ranking would fill the whole set from the ceremony every time.
//!
//! Once the six are placed, the rest is topped up to `policy.teaser_size` by impact with a
//! **chapter spread**: no chapter contributes more than a third of the set, so a teaser never turns
//! out to be eighteen photographs of dancing.
//!
//! # Why the hero comes from the portfolio
//!
//! Two answers to "which is the best photograph of this wedding" would be two answers, and phase
//! 29's own hero selector already made one. The teaser reads it rather than re-deriving it.

use std::collections::BTreeMap;

use aura_core::contract::curate::{
    CurateCode, CurateReason, HeroPick, SocialSlot, TeaserPick, MAX_REASONS, TEASER_MAX, TEASER_MIN,
};
use aura_core::contract::scene::{ChapterId, SceneId};

use crate::explain::{blend, ensure_reason, rank_reasons};
use crate::policy::Policy;
use crate::read::Frame;

/// The six slots section 6.4 names, in the order they are filled.
///
/// `Hero` first because it is the frame the message opens with. The rest in the order of the day,
/// so a teaser assembled from exactly six frames already reads as a story.
const SLOTS: [(SocialSlot, SceneKind); 6] = [
    (SocialSlot::Hero, SceneKind::Any),
    (SocialSlot::Portrait, SceneKind::Couple),
    (SocialSlot::Candid, SceneKind::CeremonyPeak),
    (SocialSlot::Group, SceneKind::Family),
    (SocialSlot::Detail, SceneKind::Detail),
    (SocialSlot::Candid, SceneKind::Dance),
];

/// What a teaser slot is looking for, in phase 07's own vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SceneKind {
    /// Anything. Only the hero uses this.
    Any,
    /// The couple, together.
    Couple,
    /// The peak of the ceremony: the vows, the rings or the kiss.
    CeremonyPeak,
    /// Family, posed or otherwise.
    Family,
    /// An object.
    Detail,
    /// Dancing.
    Dance,
}

impl SceneKind {
    /// True when a frame's scene fits this slot.
    ///
    /// Read off phase 07's label rather than decided again; `StoryService` is the one way to ask
    /// what a photograph is of.
    fn matches(self, scene: Option<SceneId>, chapter: ChapterId) -> bool {
        match self {
            Self::Any => true,
            Self::Couple => matches!(
                scene,
                Some(SceneId::CouplePortrait | SceneId::GoldenHour | SceneId::FirstLook)
            ),
            Self::CeremonyPeak => matches!(
                scene,
                Some(SceneId::Vows | SceneId::Rings | SceneId::Kiss | SceneId::Ritual)
            ),
            Self::Family => matches!(
                scene,
                Some(SceneId::FamilyPortrait | SceneId::GroupPortrait)
            ),
            Self::Detail => scene == Some(SceneId::Details),
            Self::Dance => {
                matches!(scene, Some(SceneId::FirstDance | SceneId::DanceFloor))
                    || chapter == ChapterId::Dance
            }
        }
    }
}

/// How much immediate impact a frame has, `0..1`, and whether anything could be measured.
///
/// Emotion first, then technical quality, then composition. Not phase 12's keep score: a teaser is
/// chosen for what it does to somebody who has seen nothing yet, and a keep score is a judgement
/// about what belongs in a complete record of the day.
#[must_use]
pub fn impact(frame: &Frame) -> Option<f32> {
    blend(&[
        (0.55, frame.emotion.unwrap_or(0.0), frame.emotion.is_some()),
        (
            0.25,
            frame.technical.unwrap_or(0.0),
            frame.technical.is_some(),
        ),
        (
            0.20,
            frame.composition.unwrap_or(0.0),
            frame.composition.is_some(),
        ),
    ])
}

/// Choose the teaser.
#[must_use]
pub fn select(frames: &[Frame], heroes: &[HeroPick], policy: &Policy) -> Vec<TeaserPick> {
    let size = policy.teaser_size.clamp(TEASER_MIN, TEASER_MAX) as usize;
    let mut ranked: Vec<(&Frame, f32, bool)> = frames
        .iter()
        .map(|frame| {
            let measured = impact(frame);
            (frame, measured.unwrap_or(0.0), measured.is_some())
        })
        .collect();
    ranked.sort_by(|a, b| {
        b.1.total_cmp(&a.1)
            .then_with(|| a.0.image_id.to_db().cmp(&b.0.image_id.to_db()))
    });

    let mut chosen: Vec<TeaserPick> = Vec::new();
    let mut taken: Vec<aura_core::contract::curate::ImageId> = Vec::new();

    // --- the six named slots, before any ranking decides the rest ------------------------------
    for (slot, kind) in SLOTS {
        let filled = if slot == SocialSlot::Hero {
            heroes
                .first()
                .and_then(|hero| frames.iter().find(|f| f.image_id == hero.image_id))
        } else {
            ranked.iter().map(|(frame, _, _)| *frame).find(|frame| {
                !taken.contains(&frame.image_id)
                    && kind.matches(frame.scene, frame.chapter_or_other())
            })
        };
        let Some(frame) = filled else {
            continue;
        };
        if taken.contains(&frame.image_id) {
            continue;
        }
        taken.push(frame.image_id);
        chosen.push(entry(frame, slot, chosen.len() as u32, true));
    }

    // --- top up by impact, with no chapter over a third of the set -----------------------------
    let chapter_ceiling = (size as f32 / 3.0).ceil() as usize;
    let mut per_chapter: BTreeMap<ChapterId, usize> = BTreeMap::new();
    for pick in &chosen {
        if let Some(frame) = frames.iter().find(|f| f.image_id == pick.image_id) {
            *per_chapter.entry(frame.chapter_or_other()).or_default() += 1;
        }
    }
    for (frame, _, _) in &ranked {
        if chosen.len() >= size {
            break;
        }
        if taken.contains(&frame.image_id) {
            continue;
        }
        let chapter = frame.chapter_or_other();
        let count = per_chapter.get(&chapter).copied().unwrap_or(0);
        if count >= chapter_ceiling {
            continue;
        }
        *per_chapter.entry(chapter).or_default() += 1;
        taken.push(frame.image_id);
        chosen.push(entry(
            frame,
            crate::social::slot_of(frame.scene),
            chosen.len() as u32,
            false,
        ));
    }

    chosen
}

/// One teaser pick, with its reasons.
fn entry(frame: &Frame, slot: SocialSlot, rank: u32, guaranteed: bool) -> TeaserPick {
    let score = impact(frame).unwrap_or(0.0);
    let mut reasons = Vec::new();
    if guaranteed {
        reasons.push(CurateReason::detailed(
            CurateCode::CoverageProtected,
            format!("every teaser gets one {} frame", slot.as_str()),
            0.6,
        ));
    }
    if frame.emotion.unwrap_or(0.0) >= 0.7 {
        reasons.push(CurateReason::plain(
            CurateCode::EmotionalPeak,
            frame.emotion.unwrap_or(0.0),
        ));
    }
    if frame.technical.unwrap_or(0.0) >= 0.85 {
        reasons.push(CurateReason::plain(
            CurateCode::TechnicalExcellence,
            frame.technical.unwrap_or(0.0),
        ));
    }
    if impact(frame).is_none() {
        reasons.push(CurateReason::plain(
            CurateCode::UniquenessUnavailable,
            -0.05,
        ));
    }
    ensure_reason(&mut reasons, CurateCode::HighEmotion, score.max(0.05));
    rank_reasons(&mut reasons, MAX_REASONS);
    TeaserPick {
        image_id: frame.image_id,
        slot,
        rank,
        reasons,
        accepted: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::curate::{HeroBinding, HeroTerms, ImageId, ShotScale};

    fn frame(order: u32, scene: SceneId, chapter: ChapterId, emotion: f32) -> Frame {
        let mut f = Frame::bare(ImageId::new(), order);
        f.scene = Some(scene);
        f.chapter = Some(chapter);
        f.emotion = Some(emotion);
        f.technical = Some(0.8);
        f.composition = Some(0.7);
        f
    }

    fn hero_of(frame: &Frame) -> HeroPick {
        HeroPick {
            image_id: frame.image_id,
            rank: 0,
            score: 0.9,
            terms: HeroTerms {
                technical: 0.9,
                emotion: 0.9,
                composition: 0.8,
                uniqueness: 0.8,
                story: 0.6,
            },
            chapter: ChapterId::Ceremony,
            moment: None,
            scale: ShotScale::Tight,
            binding: HeroBinding::Unconstrained,
            reasons: Vec::new(),
            confidence: 0.8,
            accepted: None,
        }
    }

    fn gallery() -> Vec<Frame> {
        let mut out = Vec::new();
        let mut order = 0u32;
        let plan: [(SceneId, ChapterId, f32); 10] = [
            (SceneId::CouplePortrait, ChapterId::Portraits, 0.7),
            (SceneId::Kiss, ChapterId::Ceremony, 0.9),
            (SceneId::FamilyPortrait, ChapterId::Portraits, 0.6),
            (SceneId::Details, ChapterId::Details, 0.4),
            (SceneId::FirstDance, ChapterId::Dance, 0.8),
            (SceneId::Candid, ChapterId::Reception, 0.5),
            (SceneId::DanceFloor, ChapterId::Dance, 0.85),
            (SceneId::Vows, ChapterId::Ceremony, 0.88),
            (SceneId::Speeches, ChapterId::Reception, 0.45),
            (SceneId::Exit, ChapterId::Exit, 0.5),
        ];
        for _ in 0..4 {
            for (scene, chapter, emotion) in plan {
                out.push(frame(order, scene, chapter, emotion));
                order += 1;
            }
        }
        out
    }

    #[test]
    fn all_six_named_slots_are_filled_before_the_ranking_decides_anything() {
        let policy = Policy::default();
        let frames = gallery();
        let heroes = vec![hero_of(&frames[1])];
        let teaser = select(&frames, &heroes, &policy);
        let by_id: BTreeMap<_, _> = frames.iter().map(|f| (f.image_id, f)).collect();

        let has = |kind: SceneKind| -> bool {
            teaser.iter().take(6).any(|pick| {
                by_id
                    .get(&pick.image_id)
                    .is_some_and(|f| kind.matches(f.scene, f.chapter_or_other()))
            })
        };
        assert!(has(SceneKind::Couple));
        assert!(has(SceneKind::CeremonyPeak));
        assert!(has(SceneKind::Family));
        assert!(has(SceneKind::Detail));
        assert!(has(SceneKind::Dance));
    }

    #[test]
    fn the_detail_frame_is_in_even_though_it_scores_lowest() {
        // The whole point of a filter rather than a term: an impact ranking would never choose it.
        let policy = Policy::default();
        let frames = gallery();
        let heroes = vec![hero_of(&frames[1])];
        let teaser = select(&frames, &heroes, &policy);
        let by_id: BTreeMap<_, _> = frames.iter().map(|f| (f.image_id, f)).collect();
        assert!(teaser.iter().any(|pick| by_id
            .get(&pick.image_id)
            .is_some_and(|f| f.scene == Some(SceneId::Details))));
    }

    #[test]
    fn no_chapter_takes_more_than_a_third_of_the_set() {
        let policy = Policy::default();
        // A wedding whose dance floor is all anybody photographed.
        let mut frames = Vec::new();
        for i in 0..60 {
            frames.push(frame(i, SceneId::DanceFloor, ChapterId::Dance, 0.9));
        }
        for i in 60..70 {
            frames.push(frame(i, SceneId::Vows, ChapterId::Ceremony, 0.5));
        }
        let teaser = select(&frames, &[], &policy);
        let by_id: BTreeMap<_, _> = frames.iter().map(|f| (f.image_id, f)).collect();
        let dance = teaser
            .iter()
            .filter(|p| {
                by_id
                    .get(&p.image_id)
                    .is_some_and(|f| f.chapter_or_other() == ChapterId::Dance)
            })
            .count();
        // Two of the six named slots can legitimately be dance frames, and the top-up is capped.
        assert!(
            dance <= (policy.teaser_size as f32 / 3.0).ceil() as usize + 2,
            "{dance} of {} are dance frames",
            teaser.len()
        );
    }

    #[test]
    fn the_hero_comes_from_the_portfolio_and_leads_the_set() {
        let policy = Policy::default();
        let frames = gallery();
        // Deliberately not the highest-impact frame.
        let heroes = vec![hero_of(&frames[3])];
        let teaser = select(&frames, &heroes, &policy);
        assert_eq!(teaser[0].image_id, frames[3].image_id);
        assert_eq!(teaser[0].slot, SocialSlot::Hero);
    }

    #[test]
    fn the_set_is_the_configured_size_when_the_gallery_can_fill_it() {
        let policy = Policy::default();
        let frames = gallery();
        let heroes = vec![hero_of(&frames[1])];
        let teaser = select(&frames, &heroes, &policy);
        assert_eq!(teaser.len(), policy.teaser_size as usize);
        assert!(teaser.len() >= TEASER_MIN as usize);
        assert!(teaser.len() <= TEASER_MAX as usize);
    }

    #[test]
    fn a_small_gallery_produces_a_short_set_rather_than_repeating_a_frame() {
        let policy = Policy::default();
        let frames: Vec<Frame> = gallery().into_iter().take(4).collect();
        let teaser = select(&frames, &[], &policy);
        assert!(teaser.len() <= 4);
        let mut ids: Vec<_> = teaser.iter().map(|p| p.image_id.to_db()).collect();
        let before = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), before);
    }

    #[test]
    fn ranks_are_dense_and_every_pick_carries_a_reason() {
        let policy = Policy::default();
        let frames = gallery();
        let heroes = vec![hero_of(&frames[1])];
        let teaser = select(&frames, &heroes, &policy);
        for (ix, pick) in teaser.iter().enumerate() {
            assert_eq!(pick.rank as usize, ix);
            assert!(!pick.reasons.is_empty(), "invariant 2");
            assert!(pick.reasons.len() <= MAX_REASONS);
        }
    }

    #[test]
    fn the_same_gallery_produces_the_same_teaser_twice() {
        let policy = Policy::default();
        let frames = gallery();
        let heroes = vec![hero_of(&frames[1])];
        let a = select(&frames, &heroes, &policy);
        let b = select(&frames, &heroes, &policy);
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(&b) {
            assert_eq!(x.image_id, y.image_id);
            assert_eq!(x.slot, y.slot);
        }
    }

    #[test]
    fn impact_over_nothing_measured_is_none_rather_than_zero() {
        assert_eq!(impact(&Frame::bare(ImageId::new(), 0)), None);
    }
}
