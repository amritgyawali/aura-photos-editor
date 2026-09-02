//! The grid set, the story set and the single hero, with aspect variants and legibility.
//!
//! Section 6.4: "the grid set balances one hero, two portraits, two details, two candids, two
//! family/group and one exit-style frame, chosen for thumbnail legibility (strong subject, clear
//! silhouette at small size)".
//!
//! # The quota is filled, never faked
//!
//! A wedding with no exit photographs gets a nine-image grid and a sentence saying so, not a tenth
//! frame promoted out of another slot to make the number right. `SocialSets::unfilled_slots` is what
//! the panel reads and `CurateCode::SlotUnfilled` is the reason on the set. Phase 12's rule - the
//! product cannot invent coverage - in the smallest place it applies.
//!
//! # Aspects come from phase 23 and nowhere else
//!
//! A social pick is delivered at a crop `GeometryService` said was safe, or at the frame's own
//! aspect. There is no crop calculation in this module. `AspectVariant::Original` is the honest
//! fallback and `CurateCode::AspectVariantAbsent` says when it was used, because a frame posted at
//! 3:2 into a 4:5 slot is a frame with its edges cut by somebody else's software.
//!
//! **The story set is delivered at 4:5 rather than 9:16**, and that is a limitation rather than a
//! choice: phase 23 froze five aspect ratios and 9:16 is not among them. Inventing a sixth here
//! would be offering a crop nobody had checked for safety, which is the thing phase 23's whole
//! safety filter exists to prevent. `docs/curation.md` says so, and the first studio that needs
//! vertical video crops reopens ADR-0041 rather than this file.

use std::collections::BTreeMap;

use aura_core::contract::curate::{
    AspectVariant, Caption, CurateCode, CurateReason, HeroPick, ImageId, SocialPick, SocialSets,
    SocialSlot, MAX_REASONS,
};
use aura_core::contract::scene::{ChapterId, SceneId};

use crate::caption::{image_caption, Vocabulary};
use crate::explain::{blend, ensure_reason, rank_reasons};
use crate::policy::Policy;
use crate::read::Frame;

/// Which slot a frame can fill, from phase 07's scene label.
///
/// Read off the label rather than decided again. Phase 07's `StoryService` is the only way to ask
/// what a photograph is of, and a second classifier here would be a second answer.
///
/// A frame with no scene is a [`SocialSlot::Candid`], which is the least specific claim about a
/// photograph rather than the first variant of the enum.
#[must_use]
pub const fn slot_of(scene: Option<SceneId>) -> SocialSlot {
    match scene {
        Some(SceneId::Details) => SocialSlot::Detail,
        Some(
            SceneId::CouplePortrait
            | SceneId::GoldenHour
            | SceneId::FirstLook
            | SceneId::GettingReadyBride
            | SceneId::GettingReadyGroom,
        ) => SocialSlot::Portrait,
        Some(SceneId::FamilyPortrait | SceneId::GroupPortrait) => SocialSlot::Group,
        Some(SceneId::Exit) => SocialSlot::Exit,
        _ => SocialSlot::Candid,
    }
}

/// How well a frame reads at thumbnail size, `0..1`, and whether anything could be measured.
///
/// Four terms from `curation.toml`, renormalised over the ones that were measured. A frame with no
/// readings at all returns `None`, which is a **skip**: it is still eligible for a slot, and the
/// ranking simply has nothing to prefer it by.
#[must_use]
pub fn legibility(frame: &Frame, policy: &Policy) -> Option<f32> {
    let face = frame.largest_face();
    let weights = policy.legibility;
    blend(&[
        (
            weights.subject_size,
            // A face over about a tenth of the frame is unmistakable at 150 px; below a fortieth it
            // is a smudge. Linear between the two.
            face.map_or(0.0, |area| (area / 0.10).clamp(0.0, 1.0)),
            face.is_some(),
        ),
        (
            weights.subject_sharp,
            frame.subject_sharpness.unwrap_or(0.0),
            frame.subject_sharpness.is_some(),
        ),
        (
            weights.uncluttered,
            // Context at full size, noise at thumbnail size.
            1.0 - frame.clutter.unwrap_or(0.0).clamp(0.0, 1.0),
            frame.clutter.is_some(),
        ),
        (
            weights.negative_space,
            frame.negative_space.unwrap_or(0.0),
            frame.negative_space.is_some(),
        ),
    ])
}

/// The aspect a set wants, resolved against what phase 23 said was safe.
///
/// Returns the variant and whether the preferred one existed. `Original` when it did not - the frame
/// is posted as it was shot rather than cropped to a rectangle nobody checked.
#[must_use]
pub fn aspect_for(frame: &Frame, wanted: &[AspectVariant]) -> (AspectVariant, bool) {
    for candidate in wanted {
        if frame.aspects.contains(candidate) {
            return (*candidate, true);
        }
    }
    (AspectVariant::Original, false)
}

/// Build all three sets and their captions.
///
/// `heroes` is the portfolio, whose first entry becomes the social hero: two answers to "which is
/// the best photograph of this wedding" would be two answers, and phase 29's own hero selector
/// already made one.
#[must_use]
pub fn build(
    frames: &[Frame],
    heroes: &[HeroPick],
    vocabulary: &Vocabulary,
    policy: &Policy,
) -> SocialSets {
    let by_id: BTreeMap<ImageId, &Frame> = frames.iter().map(|f| (f.image_id, f)).collect();

    // Rank every frame once. Legibility decides the order inside a slot; the hero list decides the
    // hero. Ties break on image id so the sets are the same on every machine.
    let mut ranked: Vec<(&Frame, f32, bool)> = frames
        .iter()
        .map(|frame| {
            let measured = legibility(frame, policy);
            (frame, measured.unwrap_or(0.0), measured.is_some())
        })
        .collect();
    ranked.sort_by(|a, b| {
        b.1.total_cmp(&a.1)
            .then_with(|| a.0.image_id.to_db().cmp(&b.0.image_id.to_db()))
    });

    let hero_frame = heroes
        .first()
        .and_then(|hero| by_id.get(&hero.image_id).copied());

    // --- the grid, slot by slot ---------------------------------------------------------------
    let mut grid: Vec<SocialPick> = Vec::new();
    let mut used: Vec<ImageId> = Vec::new();
    if let Some(frame) = hero_frame {
        grid.push(pick(
            frame,
            SocialSlot::Hero,
            &[AspectVariant::Square, AspectVariant::FourFive],
            legibility(frame, policy),
        ));
        used.push(frame.image_id);
    }
    for (slot, want) in SocialSlot::GRID_QUOTA {
        if slot == SocialSlot::Hero {
            continue;
        }
        let mut taken = 0u32;
        for (frame, _, measured) in &ranked {
            if taken >= want {
                break;
            }
            if used.contains(&frame.image_id) || slot_of(frame.scene) != slot {
                continue;
            }
            grid.push(pick(
                frame,
                slot,
                &[AspectVariant::Square, AspectVariant::FourFive],
                measured.then(|| legibility(frame, policy)).flatten(),
            ));
            used.push(frame.image_id);
            taken += 1;
        }
    }

    // --- the story set ------------------------------------------------------------------------
    //
    // Chronological rather than ranked: a story is watched through once, and a set ordered by
    // legibility is a set that opens with a close-up of a ring and never establishes where anybody
    // is. Chosen by legibility, ordered by time.
    let mut story_pool: Vec<&Frame> = ranked
        .iter()
        .map(|(frame, _, _)| *frame)
        .take(policy.story_size as usize * 3)
        .collect();
    story_pool.sort_by_key(|f| (f.order, f.image_id.to_db()));
    // Spread the picks evenly across the pool rather than taking the first N, so a story that
    // opens in the morning still reaches the dance floor. Integer arithmetic, so the same gallery
    // produces the same set on every machine.
    let wanted = policy.story_size.max(1) as usize;
    let mut story: Vec<SocialPick> = Vec::new();
    for slot_index in 0..wanted {
        if story_pool.is_empty() {
            break;
        }
        let position = slot_index * story_pool.len() / wanted;
        let Some(frame) = story_pool.get(position) else {
            break;
        };
        if story.iter().any(|p| p.image_id == frame.image_id) {
            continue;
        }
        story.push(pick(
            frame,
            slot_of(frame.scene),
            &[AspectVariant::FourFive],
            legibility(frame, policy),
        ));
    }

    let hero = hero_frame.map(|frame| {
        pick(
            frame,
            SocialSlot::Hero,
            &[AspectVariant::FourFive, AspectVariant::Square],
            legibility(frame, policy),
        )
    });

    // --- captions -----------------------------------------------------------------------------
    let mut captions: Vec<Caption> = Vec::new();
    for entry in grid.iter().chain(story.iter()).chain(hero.iter()) {
        let Some(frame) = by_id.get(&entry.image_id) else {
            continue;
        };
        let chapter = frame.chapter_or_other();
        let text = image_caption(chapter, frame.scene);
        // Assembled from the vocabulary, so this holds by construction - and is asserted anyway,
        // because "by construction" is a claim about code somebody may later change.
        if !vocabulary.grounds(&text) {
            continue;
        }
        if captions.iter().any(|c| c.image_id == Some(entry.image_id)) {
            continue;
        }
        captions.push(Caption {
            image_id: Some(entry.image_id),
            chapter,
            text,
            source: aura_core::contract::curate::CaptionSource::Template,
            grounded: true,
        });
    }

    SocialSets {
        grid,
        story,
        hero,
        captions,
    }
}

/// One pick, with its reasons.
fn pick(
    frame: &Frame,
    slot: SocialSlot,
    wanted: &[AspectVariant],
    measured: Option<f32>,
) -> SocialPick {
    let (aspect, exact) = aspect_for(frame, wanted);
    let score = measured.unwrap_or(0.0);
    let mut reasons = Vec::new();
    if measured.is_some() && score >= 0.6 {
        reasons.push(CurateReason::detailed(
            CurateCode::ThumbnailLegible,
            format!("this reads clearly at thumbnail size ({score:.2})"),
            score,
        ));
    }
    if exact {
        reasons.push(CurateReason::plain(
            CurateCode::AspectVariantAvailable,
            0.20,
        ));
    } else {
        reasons.push(CurateReason::plain(CurateCode::AspectVariantAbsent, -0.15));
    }
    ensure_reason(&mut reasons, CurateCode::ThumbnailLegible, score.max(0.05));
    rank_reasons(&mut reasons, MAX_REASONS);
    SocialPick {
        image_id: frame.image_id,
        aspect,
        slot,
        legibility: score,
        reasons,
        accepted: None,
    }
}

/// The reasons a set carries about what it could not fill.
#[must_use]
pub fn set_reasons(sets: &SocialSets) -> Vec<CurateReason> {
    let mut out = Vec::new();
    for (slot, short) in sets.unfilled_slots() {
        out.push(CurateReason::detailed(
            CurateCode::SlotUnfilled,
            format!(
                "there was nothing in this wedding for {} of the {} slots",
                short,
                slot.as_str()
            ),
            -0.2,
        ));
    }
    out
}

/// The most common scene in each chapter, for the album's chapter captions.
#[must_use]
pub fn chapter_highlights(frames: &[Frame]) -> BTreeMap<ChapterId, SceneId> {
    let mut counts: BTreeMap<(ChapterId, SceneId), u32> = BTreeMap::new();
    for frame in frames {
        let Some(scene) = frame.scene else { continue };
        if scene == SceneId::Unknown {
            continue;
        }
        *counts.entry((frame.chapter_or_other(), scene)).or_default() += 1;
    }
    let mut best: BTreeMap<ChapterId, (SceneId, u32)> = BTreeMap::new();
    for ((chapter, scene), count) in counts {
        let entry = best.entry(chapter).or_insert((scene, 0));
        if count > entry.1 {
            *entry = (scene, count);
        }
    }
    best.into_iter()
        .map(|(chapter, (scene, _))| (chapter, scene))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::curate::{HeroBinding, HeroTerms, ShotScale, GRID_SIZE};

    use crate::read::FaceRead;

    fn frame(order: u32, scene: SceneId) -> Frame {
        let mut f = Frame::bare(ImageId::new(), order);
        f.scene = Some(scene);
        f.chapter = Some(ChapterId::Ceremony);
        f.subject_sharpness = Some(0.8);
        f.clutter = Some(0.2);
        f.negative_space = Some(0.4);
        f.faces = vec![FaceRead {
            identity: None,
            area_frac: 0.08,
            centre_x: 0.5,
            width: 0.2,
            eye_mid_x: Some(0.5),
        }];
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

    fn full_gallery() -> Vec<Frame> {
        let mut out = Vec::new();
        for (order, scene) in [
            SceneId::CouplePortrait,
            SceneId::CouplePortrait,
            SceneId::CouplePortrait,
            SceneId::Details,
            SceneId::Details,
            SceneId::Details,
            SceneId::Candid,
            SceneId::Candid,
            SceneId::Candid,
            SceneId::FamilyPortrait,
            SceneId::GroupPortrait,
            SceneId::GroupPortrait,
            SceneId::Exit,
            SceneId::Exit,
        ]
        .into_iter()
        .enumerate()
        {
            out.push(frame(order as u32, scene));
        }
        out
    }

    #[test]
    fn a_full_gallery_fills_the_whole_quota() {
        let policy = Policy::default();
        let vocab = Vocabulary::build(&[]);
        let frames = full_gallery();
        let heroes = vec![hero_of(&frames[0])];
        let sets = build(&frames, &heroes, &vocab, &policy);
        assert_eq!(sets.grid.len() as u32, GRID_SIZE);
        assert!(
            sets.unfilled_slots().is_empty(),
            "{:?}",
            sets.unfilled_slots()
        );
        assert!(sets.hero.is_some());
    }

    #[test]
    fn a_wedding_with_no_exit_frames_gets_a_short_grid_and_a_sentence() {
        // Never a tenth frame promoted out of another slot to make the number right.
        let policy = Policy::default();
        let vocab = Vocabulary::build(&[]);
        let frames: Vec<Frame> = full_gallery()
            .into_iter()
            .filter(|f| f.scene != Some(SceneId::Exit))
            .collect();
        let heroes = vec![hero_of(&frames[0])];
        let sets = build(&frames, &heroes, &vocab, &policy);
        assert!(sets.grid.len() < GRID_SIZE as usize);
        let unfilled = sets.unfilled_slots();
        assert!(unfilled.iter().any(|(slot, _)| *slot == SocialSlot::Exit));
        let reasons = set_reasons(&sets);
        assert!(reasons.iter().any(|r| r.code == CurateCode::SlotUnfilled));
    }

    #[test]
    fn a_frame_is_only_ever_delivered_at_an_aspect_phase_23_called_safe() {
        let mut plain = frame(0, SceneId::Details);
        plain.aspects = vec![AspectVariant::Original];
        let (aspect, exact) = aspect_for(&plain, &[AspectVariant::Square, AspectVariant::FourFive]);
        assert_eq!(aspect, AspectVariant::Original);
        assert!(!exact);

        let mut cropped = frame(1, SceneId::Details);
        cropped.aspects = vec![AspectVariant::Original, AspectVariant::Square];
        let (aspect, exact) =
            aspect_for(&cropped, &[AspectVariant::Square, AspectVariant::FourFive]);
        assert_eq!(aspect, AspectVariant::Square);
        assert!(exact);
    }

    #[test]
    fn a_pick_with_no_safe_crop_says_so() {
        let policy = Policy::default();
        let vocab = Vocabulary::build(&[]);
        let frames = full_gallery();
        let heroes = vec![hero_of(&frames[0])];
        let sets = build(&frames, &heroes, &vocab, &policy);
        assert!(sets.grid.iter().all(|p| p
            .reasons
            .iter()
            .any(|r| r.code == CurateCode::AspectVariantAbsent)));
    }

    #[test]
    fn legibility_prefers_a_large_sharp_uncluttered_subject() {
        let policy = Policy::default();
        let mut big = frame(0, SceneId::CouplePortrait);
        big.faces[0].area_frac = 0.12;
        let mut small = frame(1, SceneId::CouplePortrait);
        small.faces[0].area_frac = 0.005;
        small.clutter = Some(0.9);
        assert!(legibility(&big, &policy) > legibility(&small, &policy));
    }

    #[test]
    fn a_frame_with_nothing_measured_has_no_legibility_rather_than_a_legibility_of_zero() {
        let policy = Policy::default();
        let bare = Frame::bare(ImageId::new(), 0);
        assert_eq!(legibility(&bare, &policy), None);
    }

    #[test]
    fn the_story_set_is_in_time_order_rather_than_score_order() {
        // A story is watched through once; a set ordered by legibility opens with a ring and never
        // establishes where anybody is.
        let policy = Policy::default();
        let vocab = Vocabulary::build(&[]);
        let mut frames = full_gallery();
        // Make the *last* frame the most legible, so a score-ordered set would open with it.
        if let Some(last) = frames.last_mut() {
            last.faces[0].area_frac = 0.3;
            last.clutter = Some(0.0);
            last.negative_space = Some(1.0);
        }
        let heroes = vec![hero_of(&frames[0])];
        let sets = build(&frames, &heroes, &vocab, &policy);
        let by_id: BTreeMap<ImageId, &Frame> = frames.iter().map(|f| (f.image_id, f)).collect();
        let orders: Vec<u32> = sets
            .story
            .iter()
            .filter_map(|p| by_id.get(&p.image_id).map(|f| f.order))
            .collect();
        let mut sorted = orders.clone();
        sorted.sort_unstable();
        assert_eq!(orders, sorted, "the story set must be chronological");
    }

    #[test]
    fn every_caption_is_grounded_in_this_weddings_own_labels() {
        let policy = Policy::default();
        let vocab = Vocabulary::build(&[]);
        let frames = full_gallery();
        let heroes = vec![hero_of(&frames[0])];
        let sets = build(&frames, &heroes, &vocab, &policy);
        assert!(!sets.captions.is_empty());
        for caption in &sets.captions {
            assert!(caption.grounded);
            assert!(vocab.grounds(&caption.text), "{}", caption.text);
            assert!(caption.within_bounds(), "{}", caption.text);
        }
    }

    #[test]
    fn no_frame_appears_twice_in_the_grid() {
        let policy = Policy::default();
        let vocab = Vocabulary::build(&[]);
        let frames = full_gallery();
        let heroes = vec![hero_of(&frames[0])];
        let sets = build(&frames, &heroes, &vocab, &policy);
        let mut ids: Vec<ImageId> = sets.grid.iter().map(|p| p.image_id).collect();
        let before = ids.len();
        ids.sort_by_key(aura_core::contract::curate::ImageId::to_db);
        ids.dedup();
        assert_eq!(ids.len(), before);
    }

    #[test]
    fn an_empty_gallery_produces_empty_sets_rather_than_a_failure() {
        let policy = Policy::default();
        let vocab = Vocabulary::build(&[]);
        let sets = build(&[], &[], &vocab, &policy);
        assert!(sets.grid.is_empty());
        assert!(sets.hero.is_none());
        assert_eq!(sets.unfilled_slots().len(), SocialSlot::ALL.len());
    }

    #[test]
    fn a_frame_with_no_scene_is_a_candid_rather_than_a_hero() {
        assert_eq!(slot_of(None), SocialSlot::Candid);
        assert_eq!(slot_of(Some(SceneId::Unknown)), SocialSlot::Candid);
        assert_eq!(slot_of(Some(SceneId::Details)), SocialSlot::Detail);
        assert_eq!(slot_of(Some(SceneId::FamilyPortrait)), SocialSlot::Group);
    }
}
