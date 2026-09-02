//! The phase 29 curation gates. PHASE-29 section 10.1.
//!
//! Run as an ordinary test so a red gate is a red build, beside the phase 05 to 28 harnesses.
//!
//! ## What these gates prove, and what they do not
//!
//! Section 10.1 asks seven things of this phase. Three of them - hero agreement at 0.75, an album
//! photographers reorder by under 15 %, monochrome picks accepted at 70 % - are **studies**. They
//! need sixty real weddings and a room of photographers, and this repository has neither. What runs
//! here in their place is the same measurement taken against `fixtures.rs`'s own opinion: a wedding
//! whose best frames were planted, whose chapter order is known by construction, and whose tonal
//! model is authored.
//!
//! That is a test of the selector against this file, and it is worth exactly what that sentence
//! says. It proves the ranking is stable, the diversity constraints bind, the coverage filter
//! cannot be outvoted, the pairing refusals are refusals rather than penalties, the grounding check
//! catches invention, and the offline path is the whole product. It is **not** evidence that a
//! photographer would pick the same twenty photographs. That is condition C1 of
//! `docs/progress/PHASE-29-EXIT.md`, and conditions C2 and C3 narrow it further: on the shipped
//! build phase 06 finds no faces, so the skin rule and the facing term are inert on a real wedding
//! and are proved here only on the `complete` fixture.
//!
//! ## Why gate 3 renders
//!
//! "Generated mixes rated better than a fixed preset" is a question about **greys**, not about band
//! weights. Two mixes with different numbers can render a photograph identically, and a mix that
//! separates a red from a green on paper can collapse them on the pixel. So gate 3 pushes both
//! mixes through `aura_render::tonemap::monochrome` - the arithmetic that will actually run when a
//! photographer accepts a proposal. Phase 16's rule: a guarantee about a pixel is enforced on the
//! pixel. `aura-render` is a dev-dependency of `aura-curate` and only a dev-dependency;
//! `crates/aura-curate/tests/no_outputs.rs` fails the build if the library ever names it.

use std::collections::{BTreeMap, BTreeSet};

use aura_core::contract::cull::Coverage;
use aura_core::contract::curate::{
    BwMix, CurateCode, ImageId, ShotScale, ALBUM_MAX, ALBUM_MIN, HERO_TARGET, MAX_MOVES,
    MAX_PAIR_SIMILARITY, MAX_PAIR_TONAL_GAP, MAX_REASONS, MAX_SKIN_BAND_SHIFT, MIX_BANDS,
    TEASER_MAX, TEASER_MIN,
};
use aura_core::contract::scene::ChapterId;
use aura_curate::album::{self, Context};
use aura_curate::caption::{self, Vocabulary};
use aura_curate::fixtures::{self, FixtureField, Shape};
use aura_curate::policy::{Policy, DEFAULT_TOML};
use aura_curate::read::{Field, Frame};
use aura_curate::sequence::{self, DraftCaption, Move, SequenceOutput};
use aura_curate::{bw, hero, social, spread, teaser};
use aura_recipe::contract::recipe::Bw;

// ---------------------------------------------------------------------------
// Section 10.1's own numbers
// ---------------------------------------------------------------------------

/// Section 10.1: hero agreement at least this, on top-20 overlap.
///
/// Against this file's planted picks rather than against photographers. Condition C1.
const HERO_AGREEMENT_FLOOR: f32 = 0.75;

/// Section 10.1: at most this share of an album reordered by photographers in the study.
///
/// Reported by gate 2 and **not asserted**, because the quantity is not measurable here and the
/// nearest thing that is - how far the sequencer departs from chronology - is a different number
/// with a different meaning. Departing from chronology is what the pairing exists to do; a build
/// that scored zero on it would be a build with no sequencer in it. What gate 2 asserts instead is
/// the bound that makes the departure safe, and condition C4 carries the study.
const REORDER_CEILING: f32 = 0.15;

/// Section 10.1: monochrome picks accepted at least this often, and the generated mix rated better
/// than a fixed preset.
///
/// Both halves are **studies** and neither runs here. What gate 3 measures in their place is the
/// fact underneath them: the share of offered candidates whose collapsed tones the mix actually
/// pulls apart. The number is section 10.1's, honestly relabelled - it is a floor on the mix being
/// useful, not on a photographer liking it.
///
/// # Why there is no arithmetic proxy for "better than a preset"
///
/// There cannot be an honest one, and the attempt is worth recording. Any statistic that rewards
/// how far a mix moves the tones is won by a fixed preset, because a preset moves every band by a
/// large fixed amount and the solver moves each band by what that band needs - scaled by the square
/// root of its share, and not at all where somebody's skin is. Any statistic that rewards restraint
/// is won by the solver for the same reason. Choosing between them is choosing the answer, so
/// neither is here.
///
/// What *is* here is the one sense in which the comparison is a fact rather than a taste:
/// [`gate_3d_a_fixed_preset_cannot_respect_a_face_and_the_solved_mix_always_does`]. A preset does
/// not know where anybody's skin is. Condition C4 carries the rest.
const USEFUL_SEPARATION_FLOOR: f32 = 0.70;

/// The seeds every property gate runs over.
///
/// Seven rather than one, because a property that holds on one synthetic wedding and not on seven is
/// a property of that wedding.
const SEEDS: [u64; 7] = [1, 2, 3, 5, 8, 13, 21];

/// The shipped policy, or a failed gate.
fn policy() -> Policy {
    Policy::load_str(DEFAULT_TOML).expect("the shipped curation table must parse")
}

// ---------------------------------------------------------------------------
// Gate 1 - heroes
// ---------------------------------------------------------------------------

#[test]
fn gate_1_finds_at_least_three_quarters_of_the_planted_portfolio() {
    let policy = policy();
    let mut worst = 1.0f32;
    for seed in SEEDS {
        let (wedding, planted) =
            fixtures::planted(Shape::complete(600), seed, HERO_TARGET as usize);
        let field = FixtureField::new(wedding);
        let frames = field.frames(field.wedding().project).expect("frames");
        let heroes = hero::select(&frames, &field, &policy);

        let found: BTreeSet<ImageId> = heroes.iter().map(|h| h.image_id).collect();
        let wanted: BTreeSet<ImageId> = planted.iter().copied().collect();
        let overlap = found.intersection(&wanted).count() as f32 / wanted.len() as f32;
        println!(
            "gate 1: seed {seed} - {}/{} planted heroes found ({overlap:.3})",
            found.intersection(&wanted).count(),
            wanted.len()
        );
        worst = worst.min(overlap);
    }
    assert!(
        worst >= HERO_AGREEMENT_FLOOR,
        "hero agreement {worst:.3} below the {HERO_AGREEMENT_FLOOR} floor"
    );
}

#[test]
fn gate_1b_the_portfolio_is_spread_across_the_wedding() {
    // A top-20 that is twenty frames of the first dance is a top-20 nobody can use, and it is what
    // a pure score ranking produces on a gallery whose best-lit chapter is one chapter.
    let policy = policy();
    for seed in SEEDS {
        let field = FixtureField::new(fixtures::wedding(Shape::complete(600), seed));
        let frames = field.frames(field.wedding().project).expect("frames");
        let heroes = hero::select(&frames, &field, &policy);
        let mut per_chapter: BTreeMap<ChapterId, u32> = BTreeMap::new();
        for pick in &heroes {
            *per_chapter.entry(pick.chapter).or_default() += 1;
        }
        let most = per_chapter.values().copied().max().unwrap_or(0);
        assert!(
            most <= policy.heroes_per_chapter,
            "seed {seed}: {most} heroes from one chapter, above the {} the policy allows",
            policy.heroes_per_chapter
        );
        assert!(
            hero::chapters_covered(&heroes) >= 4,
            "seed {seed}: the portfolio covers only {} chapters",
            hero::chapters_covered(&heroes)
        );
    }
}

#[test]
fn gate_1c_every_hero_says_why_and_says_how_sure() {
    // Invariant 2, measured rather than asserted. A portfolio a photographer cannot interrogate is
    // a portfolio they have to re-check by eye, which is the work this phase claims to save.
    let policy = policy();
    for shape in [Shape::complete(400), Shape::as_shipped(400)] {
        let field = FixtureField::new(fixtures::wedding(shape, 11));
        let frames = field.frames(field.wedding().project).expect("frames");
        for pick in hero::select(&frames, &field, &policy) {
            assert!(
                !pick.reasons.is_empty() && pick.reasons.len() <= MAX_REASONS,
                "hero {} carries {} reasons",
                pick.rank,
                pick.reasons.len()
            );
            assert!(
                pick.confidence > 0.0 && pick.confidence <= 1.0,
                "hero {} has confidence {}",
                pick.rank,
                pick.confidence
            );
            assert!(pick.is_well_formed(), "hero {} is malformed", pick.rank);
        }
    }
}

// ---------------------------------------------------------------------------
// Gate 2 - the album sequence
// ---------------------------------------------------------------------------

#[test]
fn gate_2_the_optimiser_never_produces_a_pair_the_rules_forbid() {
    // Section 10.1's second row - "album sequence accepted with <= 15 % of images reordered by
    // photographers in the study" - needs photographers. The share this prints is a **different
    // quantity**: how far the composer's own sequence sits from chronology, which is what the
    // pairing exists to change rather than a defect. It is printed and not asserted, and condition
    // C4 carries the study.
    //
    // What is assertable is the invariant the sequence has to preserve while it moves things.
    // `lay_out` walks the day in order and looks ahead only when the adjacent pair is refused, and
    // then `optimise` runs swap passes over the result - so the constraint has to hold *after* the
    // optimiser, not merely as the layout's own output. This calls the same predicate the composer
    // calls, on every final spread.
    //
    // Three earlier versions of this gate tried to bound a *distance* instead, and all three were
    // wrong in the same way: they measured the algorithm rather than the product. A pull of four in
    // the list still to be placed is a pull of more than four in the album's own chronology because
    // successive pulls compound, and swap passes compound it further, so any constant is either
    // loose enough to prove nothing or tight enough to fail a correct build on the eighth wedding.
    let policy = policy();
    for seed in SEEDS {
        for shape in [Shape::complete(600), Shape::as_shipped(600)] {
            let field = FixtureField::new(fixtures::wedding(shape, seed));
            let frames = field.frames(field.wedding().project).expect("frames");
            let plan = album::compose(&frames, &context(&field), &field, &policy, 80);
            let by_id: BTreeMap<ImageId, &Frame> = frames.iter().map(|f| (f.image_id, f)).collect();

            let mut pairs = 0usize;
            for spread in &plan.spreads {
                let (Some(left), Some(right)) = (spread.left, spread.right) else {
                    continue;
                };
                let (Some(a), Some(b)) = (by_id.get(&left).copied(), by_id.get(&right).copied())
                else {
                    continue;
                };
                let similarity = field.similarity(left, &[right])[0];
                assert!(
                    spread::permitted(a, b, similarity, &policy),
                    "seed {seed}: spread {} survived the optimiser carrying a pair the rules \
                     refuse",
                    spread.index
                );
                pairs += 1;
            }

            let order: Vec<ImageId> = plan.spreads.iter().flat_map(album_images).collect();
            let displaced = displaced_share(&order, &frames);
            println!(
                "gate 2: seed {seed} - {pairs} pairs all permitted, {displaced:.3} of the album \
                 away from chronology (section 10.1's study ceiling is {REORDER_CEILING:.2} and \
                 is a different quantity)"
            );
        }
    }
}

#[test]
fn gate_2b_the_chapters_are_in_wedding_order_and_no_spread_straddles_two() {
    // The one constraint in this phase that is never traded against anything: an album that opens
    // with the exit is not a stylistic choice.
    let policy = policy();
    for seed in SEEDS {
        for shape in [Shape::complete(600), Shape::as_shipped(600)] {
            let field = FixtureField::new(fixtures::wedding(shape, seed));
            let frames = field.frames(field.wedding().project).expect("frames");
            let plan = album::compose(&frames, &context(&field), &field, &policy, 80);

            let mut last = None::<usize>;
            for span in &plan.chapter_map {
                let rank = ChapterId::ALL
                    .iter()
                    .position(|c| *c == span.chapter)
                    .expect("every chapter is in the vocabulary");
                if let Some(previous) = last {
                    assert!(
                        rank > previous,
                        "seed {seed}: {:?} follows a later chapter",
                        span.chapter
                    );
                }
                last = Some(rank);
            }
            for spread in &plan.spreads {
                let chapters: BTreeSet<ChapterId> = spread
                    .images()
                    .iter()
                    .filter_map(|id| frames.iter().find(|f| f.image_id == *id))
                    .map(Frame::chapter_or_other)
                    .collect();
                assert!(
                    chapters.len() <= 1,
                    "seed {seed}: spread {} straddles {chapters:?}",
                    spread.index
                );
            }
        }
    }
}

#[test]
fn gate_2c_a_photographers_order_is_reproduced_exactly() {
    // The other half of gate 2, and the more important one: an optimiser that improves on somebody's
    // own sequence has reordered 100 % of their album.
    let policy = policy();
    let field = FixtureField::new(fixtures::wedding(Shape::complete(600), 4));
    let frames = field.frames(field.wedding().project).expect("frames");
    let first = album::compose(&frames, &context(&field), &field, &policy, 80);
    let mut wanted: Vec<ImageId> = first.spreads.iter().flat_map(album_images).collect();
    wanted.reverse();
    // Reversed inside each chapter rather than across the album, because a cross-chapter order is
    // refused by `check_order` and would be testing the wrong refusal.
    let wanted = by_chapter_reversed(&first.spreads);

    let mut context = context(&field);
    context.user_order = Some(wanted.clone());
    let second = album::compose(&frames, &context, &field, &policy, 80);
    let got: Vec<ImageId> = second.spreads.iter().flat_map(album_images).collect();

    assert!(second.user_ordered, "the plan does not record the order");
    assert_eq!(got, wanted, "a pass reordered a photographer's own album");
}

// ---------------------------------------------------------------------------
// Gate 3 - monochrome
// ---------------------------------------------------------------------------

#[test]
fn gate_3_a_generated_mix_pulls_the_collapsed_tones_apart() {
    // Measured through `aura_render::tonemap::monochrome`, on the bands that would otherwise print
    // as one grey. "Useful" is one [`bw::COLLAPSE`] of separation: what was a single tone is two.
    let policy = policy();
    let mut useful = 0usize;
    let mut total = 0usize;

    for seed in SEEDS {
        for shape in [Shape::complete(600), Shape::as_shipped(600)] {
            let field = FixtureField::new(fixtures::wedding(shape, seed));
            let frames = field.frames(field.wedding().project).expect("frames");
            let loci = field.skin_bands(field.wedding().project).expect("loci");

            for pick in bw::candidates(&frames, &loci, &policy) {
                let Some(frame) = frames.iter().find(|f| f.image_id == pick.image_id) else {
                    continue;
                };
                let Some(descriptor) = frame.descriptor.as_ref() else {
                    continue;
                };
                let reading = bw::bands(descriptor);
                total += 1;
                if separation(&reading, &pick.mix) >= bw::COLLAPSE {
                    useful += 1;
                }
            }
        }
    }

    let share = useful as f32 / total.max(1) as f32;
    println!("gate 3: the mix separates the collapsed tones on {useful}/{total} = {share:.3}");
    assert!(
        share >= USEFUL_SEPARATION_FLOOR,
        "only {share:.3} of offered mixes separate anything, below {USEFUL_SEPARATION_FLOOR}"
    );
}

#[test]
fn gate_3d_a_fixed_preset_cannot_respect_a_face_and_the_solved_mix_always_does() {
    // The one place "better than a fixed preset" is a fact rather than a preference. A preset is a
    // set of numbers chosen before the photograph existed, so it moves whatever band somebody's
    // skin happens to fall in - and on a dark-skinned subject under warm light that band is the
    // one a red filter lifts hardest. The solved mix pins it.
    let policy = policy();
    let mut checked = 0usize;
    for seed in SEEDS {
        let field = FixtureField::new(fixtures::wedding(Shape::complete(600), seed));
        let frames = field.frames(field.wedding().project).expect("frames");
        let loci = field.skin_bands(field.wedding().project).expect("loci");

        for pick in bw::candidates(&frames, &loci, &policy) {
            let Some(frame) = frames.iter().find(|f| f.image_id == pick.image_id) else {
                continue;
            };
            let skin: Vec<usize> = bw::skin_bands_of(frame, &loci).into_iter().collect();
            if skin.is_empty() {
                continue;
            }
            checked += 1;
            assert!(
                pick.mix.within_skin_bound(&skin),
                "the solved mix moved a skin band: {:?} over {skin:?}",
                pick.mix.bands
            );
            assert!(
                !RED_FILTER_PRESET.within_skin_bound(&skin),
                "the control preset is not actually a control: it respects {skin:?}"
            );
        }
    }
    assert!(checked > 0, "no candidate had a measured skin band");
    println!("gate 3d: {checked} candidates where a preset would have moved somebody's skin");
}

#[test]
fn gate_3b_no_band_anybodys_skin_sits_in_is_moved_beyond_its_own_ceiling() {
    // The one bound in this phase that is about a person. It is vacuous on the shipped build - no
    // faces means no loci - so it is measured on the `complete` shape, and condition C2 says so.
    let policy = policy();
    for seed in SEEDS {
        let field = FixtureField::new(fixtures::wedding(Shape::complete(600), seed));
        let frames = field.frames(field.wedding().project).expect("frames");
        let loci = field.skin_bands(field.wedding().project).expect("loci");
        let mut checked = 0usize;

        for pick in bw::candidates(&frames, &loci, &policy) {
            let Some(frame) = frames.iter().find(|f| f.image_id == pick.image_id) else {
                continue;
            };
            let skin = bw::skin_bands_of(frame, &loci);
            if skin.is_empty() {
                continue;
            }
            checked += 1;
            let bands: Vec<usize> = skin.iter().copied().collect();
            assert!(
                pick.mix.within_skin_bound(&bands),
                "seed {seed}: a mix moved a skin band by more than {MAX_SKIN_BAND_SHIFT}: {:?} over {bands:?}",
                pick.mix.bands
            );
            assert!(pick.mix.within_bounds(), "a mix left the general ceiling");
        }
        assert!(
            checked > 0,
            "seed {seed}: no candidate had a measured skin band, so this gate proved nothing"
        );
    }
}

#[test]
fn gate_3c_a_gallery_with_no_measured_skin_says_so_rather_than_claiming_the_bound_held() {
    // Phase 24's rule, in the place it is most tempting to break: `within_skin_bound` over an empty
    // set is vacuously true, and a build that rendered that as "skin protected" would be claiming
    // a guarantee it never checked. It is a caveat code instead.
    let policy = policy();
    let field = FixtureField::new(fixtures::wedding(Shape::as_shipped(400), 6));
    let frames = field.frames(field.wedding().project).expect("frames");
    let loci = field.skin_bands(field.wedding().project).expect("loci");
    assert!(loci.is_empty(), "the as-shipped fixture has no loci");

    let picks = bw::candidates(&frames, &loci, &policy);
    assert!(
        !picks.is_empty(),
        "the shipped build still finds candidates"
    );
    for pick in &picks {
        assert!(
            pick.reasons
                .iter()
                .any(|r| r.code == CurateCode::SkinLocusUnavailable),
            "a mix solved without a locus did not say so"
        );
    }
}

// ---------------------------------------------------------------------------
// Gate 4 - album coverage
// ---------------------------------------------------------------------------

#[test]
fn gate_4_every_guarantee_the_gallery_covers_is_in_the_album() {
    // Coverage is a filter rather than a term - ADR-0059 section 6, phase 12's rule in its fifth
    // application - so an 80-image album out of a 600-image gallery still carries the ring
    // exchange, whatever it scored.
    let policy = policy();
    for seed in SEEDS {
        for shape in [Shape::complete(600), Shape::as_shipped(600)] {
            let field = FixtureField::new(fixtures::wedding(shape, seed));
            let frames = field.frames(field.wedding().project).expect("frames");
            let context = context(&field);
            let plan = album::compose(&frames, &context, &field, &policy, 80);

            for (rule, state) in &context.gallery_coverage.must_haves {
                if *state == Coverage::Missing {
                    continue;
                }
                let in_album = plan
                    .coverage
                    .must_haves
                    .iter()
                    .find(|(r, _)| r == rule)
                    .is_some_and(|(_, s)| *s != Coverage::Missing);
                assert!(
                    in_album,
                    "seed {seed}: the gallery covers {rule:?} and the album does not"
                );
            }
        }
    }
}

#[test]
fn gate_4b_every_close_family_member_appears() {
    let policy = policy();
    for seed in SEEDS {
        let field = FixtureField::new(fixtures::wedding(Shape::complete(600), seed));
        let frames = field.frames(field.wedding().project).expect("frames");
        let context = context(&field);
        let plan = album::compose(&frames, &context, &field, &policy, 80);

        let chosen: BTreeSet<ImageId> = plan.spreads.iter().flat_map(album_images).collect();
        let (family, minimum) = &context.close_family;
        for identity in family {
            let count = frames
                .iter()
                .filter(|f| chosen.contains(&f.image_id) && f.identities.contains(identity))
                .count() as u32;
            assert!(
                count >= *minimum,
                "seed {seed}: {identity:?} appears {count} times, below the {minimum} guaranteed"
            );
        }
    }
}

#[test]
fn gate_4c_the_album_lands_inside_the_size_the_contract_allows() {
    let policy = policy();
    for target in [ALBUM_MIN, 80, ALBUM_MAX] {
        let field = FixtureField::new(fixtures::wedding(Shape::complete(600), 9));
        let frames = field.frames(field.wedding().project).expect("frames");
        let plan = album::compose(&frames, &context(&field), &field, &policy, target);
        assert_eq!(
            plan.size, target,
            "asked for {target}, produced {}",
            plan.size
        );
        assert!((ALBUM_MIN..=ALBUM_MAX).contains(&plan.size));
    }
}

// ---------------------------------------------------------------------------
// Gate 5 - the pairing properties
// ---------------------------------------------------------------------------

#[test]
fn gate_5_no_spread_pairs_two_near_duplicates_or_two_frames_from_one_moment() {
    // A property test over seven weddings and both shapes, because this is the refusal a
    // photographer notices first: two frames of the same kiss, facing each other, across the gutter.
    let policy = policy();
    for seed in SEEDS {
        for shape in [Shape::complete(600), Shape::as_shipped(600)] {
            let field = FixtureField::new(fixtures::wedding(shape, seed));
            let frames = field.frames(field.wedding().project).expect("frames");
            let plan = album::compose(&frames, &context(&field), &field, &policy, 80);

            for spread in &plan.spreads {
                let (Some(left), Some(right)) = (spread.left, spread.right) else {
                    continue;
                };
                assert!(
                    spread.pair.similarity <= MAX_PAIR_SIMILARITY,
                    "seed {seed}: spread {} pairs two frames at {:.3} similarity",
                    spread.index,
                    spread.pair.similarity
                );
                let (Some(a), Some(b)) = (
                    frames.iter().find(|f| f.image_id == left),
                    frames.iter().find(|f| f.image_id == right),
                ) else {
                    continue;
                };
                if let (Some(x), Some(y)) = (a.moment, b.moment) {
                    assert!(
                        x != y,
                        "seed {seed}: spread {} pairs two frames from one moment",
                        spread.index
                    );
                }
            }
        }
    }
}

#[test]
fn gate_5b_no_spread_pairs_two_frames_beyond_the_tonal_ceiling() {
    let policy = policy();
    for seed in SEEDS {
        for shape in [Shape::complete(600), Shape::as_shipped(600)] {
            let field = FixtureField::new(fixtures::wedding(shape, seed));
            let frames = field.frames(field.wedding().project).expect("frames");
            let plan = album::compose(&frames, &context(&field), &field, &policy, 80);
            for spread in &plan.spreads {
                if spread.single {
                    continue;
                }
                assert!(
                    spread.pair.tonal_gap <= MAX_PAIR_TONAL_GAP,
                    "seed {seed}: spread {} pairs across a {:.3} tonal gap",
                    spread.index,
                    spread.pair.tonal_gap
                );
            }
        }
    }
}

#[test]
fn gate_5c_a_facing_score_is_never_claimed_where_facing_could_not_be_measured() {
    // The failure this catches is a zero rendered as a judgement. On the shipped build every spread
    // is unmeasurable, and a panel that showed those as "subjects face outward" would be inventing
    // the one term album designers spend the most time on. Condition C3.
    let policy = policy();
    let field = FixtureField::new(fixtures::wedding(Shape::as_shipped(600), 3));
    let frames = field.frames(field.wedding().project).expect("frames");
    let plan = album::compose(&frames, &context(&field), &field, &policy, 80);

    let mut unknown = 0usize;
    for spread in &plan.spreads {
        if spread.pair.facing_known {
            continue;
        }
        unknown += 1;
        assert!(
            spread.pair.facing_score == 0.0,
            "spread {} claims a facing score with nothing measured",
            spread.index
        );
    }
    assert!(unknown > 0, "the shipped fixture measured facing after all");
}

// ---------------------------------------------------------------------------
// Gate 6 - captions
// ---------------------------------------------------------------------------

#[test]
fn gate_6_every_caption_is_grounded_in_this_weddings_own_labels() {
    // The automated grounding check section 10.1 asks for, and it is the same check for a template
    // and for a model: a caption is accepted when every word in it is one this wedding supplied.
    for seed in SEEDS {
        let wedding = fixtures::wedding(Shape::complete(400), seed);
        let vocabulary = Vocabulary::build(&wedding.rituals);
        let field = FixtureField::new(wedding);
        let frames = field.frames(field.wedding().project).expect("frames");
        let policy = policy();
        let plan = album::compose(&frames, &context(&field), &field, &policy, 80);
        let highlights = social::chapter_highlights(&frames);
        let captions = caption::for_album(
            &plan.chapter_map,
            &highlights,
            &vocabulary,
            &BTreeMap::new(),
        );

        assert!(!captions.is_empty(), "seed {seed}: no captions at all");
        for written in &captions {
            assert!(
                written.grounded,
                "seed {seed}: '{}' is not grounded",
                written.text
            );
            assert!(
                vocabulary.ungrounded(&written.text).is_empty(),
                "seed {seed}: '{}' invents {:?}",
                written.text,
                vocabulary.ungrounded(&written.text)
            );
        }
    }
}

#[test]
fn gate_6b_an_invented_name_place_or_claim_is_refused() {
    // Six kinds of invention, one per failure a caption generator actually commits. A name and a
    // place are the obvious two; the rest are the ones a fluent model produces without noticing.
    let vocabulary = Vocabulary::build(&["saptapadi".to_string()]);
    for draft in [
        "Priya and Arjun exchange rings", // names
        "The ceremony at Rosewood Manor", // a place
        "Their happiest day",             // a claim nobody measured
        "The bride's mother wept",        // a relationship nobody established
        "A Catholic blessing",            // a tradition nobody named
        "Photographed on a Leica",        // a fact from outside the wedding
    ] {
        let written = caption::accept(Some(draft), &vocabulary, ChapterId::Ceremony, None);
        assert!(
            !written.text.contains("Rosewood") && written.text != draft,
            "'{draft}' was accepted verbatim"
        );
        assert!(written.grounded, "the replacement is itself ungrounded");
    }
}

// ---------------------------------------------------------------------------
// Gate 7 - offline
// ---------------------------------------------------------------------------

#[test]
fn gate_7_the_whole_of_curation_runs_with_no_provider() {
    // "Curation works fully without cloud, using the deterministic optimiser." Every selector here
    // is reached without a network, and the album an absent provider produces is the album.
    let policy = policy();
    let field = FixtureField::new(fixtures::wedding(Shape::complete(600), 12));
    let frames = field.frames(field.wedding().project).expect("frames");
    let loci = field.skin_bands(field.wedding().project).expect("loci");

    let heroes = hero::select(&frames, &field, &policy);
    let bw = bw::candidates(&frames, &loci, &policy);
    let album = album::compose(&frames, &context(&field), &field, &policy, 80);
    let vocabulary = Vocabulary::build(&field.wedding().rituals);
    let sets = social::build(&frames, &heroes, &vocabulary, &policy);
    let teaser = teaser::select(&frames, &heroes, &policy);

    assert_eq!(heroes.len(), HERO_TARGET as usize);
    assert!(!bw.is_empty(), "no monochrome candidates offline");
    assert_eq!(album.size, 80);
    assert!(!sets.grid.is_empty(), "no social grid offline");
    assert!(
        (TEASER_MIN as usize..=TEASER_MAX as usize).contains(&teaser.len()),
        "the teaser is {} frames",
        teaser.len()
    );
}

#[test]
fn gate_7b_an_unreachable_provider_and_a_refused_answer_produce_the_same_album() {
    // ADR-0059 section 11: the cloud can only be agreed with. This is what makes an outage, a
    // budget refusal, a malformed response and a cautious model one outcome rather than four.
    let policy = policy();
    let field = FixtureField::new(fixtures::wedding(Shape::complete(600), 15));
    let frames = field.frames(field.wedding().project).expect("frames");
    let offline = album::compose(&frames, &context(&field), &field, &policy, 80);

    let mut with_cloud = album::compose(&frames, &context(&field), &field, &policy, 80);
    let answer = SequenceOutput {
        // Every move crosses a chapter, so every one is refused.
        moves: (0..MAX_MOVES.min(8) as i64)
            .map(|ix| Move {
                from_index: ix,
                to_index: 79 - ix,
                reason: "a stronger opening".to_string(),
            })
            .collect(),
        captions: vec![DraftCaption {
            chapter: "ceremony".to_string(),
            caption: "The Willowbrook chapel".to_string(),
        }],
        confidence: 0.9,
    };
    let applied = sequence::apply(&mut with_cloud, &answer, &frames, &field, &policy);

    assert_eq!(applied.applied, 0, "a chapter-crossing move was applied");
    assert_eq!(
        images_of(&offline),
        images_of(&with_cloud),
        "a refused answer changed the album"
    );
}

#[test]
fn gate_7c_the_same_gallery_produces_the_same_curation_twice() {
    // Invariant 4. Two runs over one gallery, compared image by image rather than by score, because
    // a stable score with an unstable tie-break is exactly what a photographer sees as churn.
    let policy = policy();
    for seed in [4u64, 17] {
        let field = FixtureField::new(fixtures::wedding(Shape::complete(600), seed));
        let frames = field.frames(field.wedding().project).expect("frames");
        let loci = field.skin_bands(field.wedding().project).expect("loci");

        let first = (
            hero::select(&frames, &field, &policy),
            bw::candidates(&frames, &loci, &policy),
            album::compose(&frames, &context(&field), &field, &policy, 80),
        );
        let second = (
            hero::select(&frames, &field, &policy),
            bw::candidates(&frames, &loci, &policy),
            album::compose(&frames, &context(&field), &field, &policy, 80),
        );

        let heroes = |v: &[aura_core::contract::curate::HeroPick]| -> Vec<ImageId> {
            v.iter().map(|h| h.image_id).collect()
        };
        assert_eq!(heroes(&first.0), heroes(&second.0), "heroes moved");
        let mixes = |v: &[aura_core::contract::curate::BwPick]| -> Vec<(ImageId, [i16; 8])> {
            v.iter().map(|p| (p.image_id, p.mix.bands)).collect()
        };
        assert_eq!(mixes(&first.1), mixes(&second.1), "mixes moved");
        assert_eq!(images_of(&first.2), images_of(&second.2), "the album moved");
    }
}

// ---------------------------------------------------------------------------
// Property and fuzz - section 10.2's second row
// ---------------------------------------------------------------------------

#[test]
fn a_gallery_of_one_and_a_gallery_of_six_thousand_both_curate() {
    let policy = policy();
    for frames_in in [1u32, 40, 6_000] {
        let field = FixtureField::new(fixtures::wedding(Shape::complete(frames_in), 2));
        let frames = field.frames(field.wedding().project).expect("frames");
        let plan = album::compose(&frames, &context(&field), &field, &policy, 80);
        // An album cannot be larger than the gallery it is drawn from, and asking for eighty out of
        // one is a small album rather than a failure.
        assert!(
            plan.size <= frames.len() as u32,
            "{frames_in} frames produced a {} image album",
            plan.size
        );
        for spread in &plan.spreads {
            assert!(spread.len() <= 2);
        }
    }
}

#[test]
fn a_gallery_with_no_readings_at_all_curates_to_caveats_rather_than_to_confidence() {
    // Every reading a `Frame` carries is an `Option`, and this is the run where all of them are
    // absent: no descriptors, no faces, no loci, and a field that cannot measure similarity.
    let policy = policy();
    let shape = Shape {
        frames: 300,
        faces: false,
        loci: false,
        descriptors: false,
    };
    let field = FixtureField::blind(fixtures::wedding(shape, 5));
    let frames = field.frames(field.wedding().project).expect("frames");
    let heroes = hero::select(&frames, &field, &policy);
    let plan = album::compose(&frames, &context(&field), &field, &policy, 80);

    assert!(!heroes.is_empty(), "nothing was picked at all");
    for pick in heroes.iter().skip(1) {
        // Skipping rank 0 on purpose: the first hero has nothing to be unlike, so its uniqueness is
        // 1.0 and is honestly *measured* rather than missing. Every pick after it is compared
        // against a chosen set, and on a blind field none of those comparisons can be made.
        assert!(
            pick.reasons
                .iter()
                .any(|r| r.code == CurateCode::UniquenessUnavailable),
            "hero {} was picked with no similarity and did not say so",
            pick.rank
        );
    }
    // The rhythm's denominator is on the wire and has to be the truth. Not zero: a scene is a
    // second, weaker route to a shot scale - a details frame is tight, a venue is wide - so a
    // gallery with no faces still measures a rhythm over part of itself. What must be true is that
    // the number *reported* is the share actually measured, because that is what tells a
    // photographer whether a rhythm of 1.000 is a claim about their album or about a tenth of it.
    let chosen: BTreeSet<ImageId> = plan.spreads.iter().flat_map(album_images).collect();
    let in_album: Vec<&Frame> = frames
        .iter()
        .filter(|f| chosen.contains(&f.image_id))
        .collect();
    let known = in_album
        .iter()
        .filter(|f| f.scale() != ShotScale::Unknown)
        .count();
    let truth = known as f32 / in_album.len().max(1) as f32;
    assert!(
        (plan.rhythm_measurable - truth).abs() < 0.02,
        "the album reports rhythm over {:.3} of itself and measured {truth:.3}",
        plan.rhythm_measurable
    );
    assert_eq!(
        plan.reasons
            .iter()
            .any(|r| r.code == CurateCode::RhythmUnmeasurable),
        plan.rhythm_measurable < 0.33,
        "the unmeasurable-rhythm caveat and the number it is about disagree"
    );
}

#[test]
fn the_uniqueness_term_grows_with_the_gallery_rather_than_with_its_square() {
    // Section 11's budget is 20 s for a thousand images, and the uniqueness term is the one thing in
    // this phase that could quietly break it: the greedy re-scores every remaining candidate on
    // every round, and each scoring asks how unlike the already-chosen heroes the frame is.
    //
    // That is `candidates x target^2 / 2` readings - linear in the gallery with a constant near two
    // hundred, not quadratic in it. The assertion is on the **shape** as well as the number, by
    // running the same selection over a doubled wedding: a size assertion alone would pass on a
    // build that had made the term quadratic and happened to be measured on a small fixture.
    // Phase 26 wrote this rule and this is its second application.
    let policy = policy();
    let mut readings = Vec::new();
    for frames_in in [300u32, 600] {
        let field = FixtureField::new(fixtures::wedding(Shape::complete(frames_in), 7));
        let frames = field.frames(field.wedding().project).expect("frames");
        let _ = hero::select(&frames, &field, &policy);
        readings.push((frames_in, field.calls()));
    }

    let (small, big) = (readings[0], readings[1]);
    println!(
        "uniqueness: {} readings for {} frames, {} for {}",
        small.1, small.0, big.1, big.0
    );
    let growth = big.1 as f32 / small.1.max(1) as f32;
    assert!(
        growth <= 2.6,
        "doubling the gallery multiplied the similarity readings by {growth:.2}, which is not linear"
    );
    let quadratic = u64::from(big.0) * u64::from(big.0) / 2;
    assert!(
        big.1 < quadratic,
        "{} readings for {} frames is worse than asking every pair",
        big.1,
        big.0
    );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// A fixed preset to measure a solved mix against: the classic red filter.
///
/// Red and orange lifted, blue and aqua dropped - what a photographer reaches for when they want a
/// dramatic sky and bright skin, and what half the monochrome presets in circulation are. It is a
/// good control because it is often right.
const RED_FILTER_PRESET: BwMix = BwMix {
    bands: [60, 40, 20, -10, -40, -60, -20, 20],
};

/// The context a fixture field supplies.
fn context(field: &FixtureField) -> Context {
    let project = field.wedding().project;
    Context {
        gallery_coverage: field.gallery_coverage(project).expect("coverage"),
        close_family: field.close_family(project).expect("family"),
        user_order: None,
    }
}

/// Every image on a spread, left first.
fn album_images(spread: &aura_core::contract::curate::Spread) -> Vec<ImageId> {
    spread.images()
}

/// The album's images, in album order.
fn images_of(plan: &aura_core::contract::curate::AlbumPlan) -> Vec<ImageId> {
    plan.spreads.iter().flat_map(album_images).collect()
}

/// The share of an album that would have to be **moved** to put it back into timeline order.
///
/// Exactly `n` minus the length of the longest increasing subsequence of the album's timeline
/// positions: the fewest images somebody has to drag to restore chronology, which is the quantity
/// section 10.1 phrases as "% of images reordered".
///
/// The first version of this counted adjacent inversions instead, and that is a different number:
/// moving one image `k` places creates about two inversions, so it reported roughly twice the work
/// a photographer would actually do. Phase 22's rule - a threshold on a measurement is a statement
/// about the instrument as well as about the world - met for the fifth time in this repository, and
/// the first time where the instrument was inflating rather than flattening.
fn displaced_share(order: &[ImageId], frames: &[Frame]) -> f32 {
    let position: BTreeMap<ImageId, u32> = frames.iter().map(|f| (f.image_id, f.order)).collect();
    let ranks: Vec<u32> = order
        .iter()
        .filter_map(|id| position.get(id).copied())
        .collect();
    if ranks.is_empty() {
        return 0.0;
    }

    // Patience sorting: `tails[k]` is the smallest possible tail of an increasing subsequence of
    // length `k + 1`. O(n log n), and the album is eighty images.
    let mut tails: Vec<u32> = Vec::new();
    for rank in &ranks {
        match tails.binary_search(rank) {
            Ok(ix) | Err(ix) => {
                if ix == tails.len() {
                    tails.push(*rank);
                } else if let Some(slot) = tails.get_mut(ix) {
                    *slot = *rank;
                }
            }
        }
    }
    (ranks.len() - tails.len()) as f32 / ranks.len() as f32
}

/// A photographer's order: the album's own images, reversed inside each chapter.
///
/// Inside rather than across, because `check_order` refuses an order that reorders chapters and this
/// gate is about whether a pass honours a legal one.
fn by_chapter_reversed(spreads: &[aura_core::contract::curate::Spread]) -> Vec<ImageId> {
    let mut by_chapter: Vec<(ChapterId, Vec<ImageId>)> = Vec::new();
    for spread in spreads {
        let chapter = spread.chapter;
        match by_chapter.last_mut() {
            Some((c, images)) if *c == chapter => images.extend(spread.images()),
            _ => by_chapter.push((chapter, spread.images())),
        }
    }
    by_chapter
        .into_iter()
        .flat_map(|(_, mut images)| {
            images.reverse();
            images
        })
        .collect()
}

/// How far apart a mix pulls the bands that had **collapsed onto each other**, `0..1`.
///
/// Restricted to the bands within [`bw::COLLAPSE`] of the frame's anchor tone: the regions that
/// would print as one grey if nothing were done. Their rendered greys are taken through
/// `aura_render::tonemap::monochrome` and the answer is the range they end up spanning.
///
/// # Why this set, and not every band
///
/// Two earlier instruments were wrong and both are worth recording, because each is the obvious
/// one.
///
/// The **mean spread of every band** rewards maximum separation rather than sufficient separation.
/// A frame whose regions are already well apart wants a near-neutral mix, which is what the solver
/// correctly produces - and a preset that drives every band to its limit then beats it on the
/// average while making a worse photograph. Phase 22's lesson in a fourth place: score what the
/// operation was for, not how large it was.
///
/// The **minimum gap over every band** is the opposite failure. Seven bands crammed into the range
/// a mix can reach always have some near-collision in them, so the statistic becomes a lottery over
/// one arbitrary pair and says nothing about either mix.
///
/// What a monochrome conversion is *for* is the collapsed set - `bw::solve`'s own words are that
/// two regions sitting on the anchor "stay exactly as indistinguishable as they were" unless
/// something spreads them. So that is the set this measures, and a fixed preset competes on it
/// fairly: it moves those bands too, by hue, and sometimes by luck it moves them apart.
///
/// The greys are clamped to `0..1` because that is what the output transform does to them.
/// `monochrome` multiplies luminance by a gain and does not bound it, so a preset that lifts a
/// bright band by sixty saturates it to white - indistinguishable from every other saturated band,
/// which is the flat patch [`MAX_BAND_SHIFT`] exists to prevent. An unclamped measurement scores
/// that as a triumph.
fn separation(reading: &bw::BandReading, mix: &BwMix) -> f32 {
    let bw = Bw {
        mix: MIX_BANDS
            .iter()
            .zip(mix.bands.iter())
            .map(|(name, value)| ((*name).to_string(), *value))
            .collect(),
        grade: None,
    };

    // The anchor: the share-weighted mean band luminance, which is what `bw::solve` separates
    // against on a frame with no measured skin.
    let mut sum = 0.0f32;
    let mut weight = 0.0f32;
    for band in 0..8 {
        let (Some(share), Some(luma)) = (reading.share.get(band), reading.luma.get(band)) else {
            continue;
        };
        sum += luma * share;
        weight += share;
    }
    if weight <= f32::EPSILON {
        return 0.0;
    }
    let anchor = sum / weight;

    let mut greys: Vec<f32> = Vec::new();
    for band in 0..8 {
        let (Some(share), Some(luma), Some(sat)) = (
            reading.share.get(band),
            reading.luma.get(band),
            reading.saturation.get(band),
        ) else {
            continue;
        };
        if *share <= 0.005 || (luma - anchor).abs() >= bw::COLLAPSE {
            continue;
        }
        let hue = band as f32 / 8.0;
        let rgb = from_hsv(hue, *sat, *luma);
        let out = aura_render::tonemap::monochrome(rgb, &bw);
        greys.push(out[0].clamp(0.0, 1.0));
    }
    if greys.len() < 2 {
        return 0.0;
    }

    let lo = greys.iter().copied().fold(f32::MAX, f32::min);
    let hi = greys.iter().copied().fold(f32::MIN, f32::max);
    hi - lo
}

/// HSV to RGB, so a band reading can be turned back into a colour the renderer accepts.
fn from_hsv(h: f32, s: f32, v: f32) -> [f32; 3] {
    let h6 = (h.rem_euclid(1.0)) * 6.0;
    let sector = h6.floor();
    let f = h6 - sector;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    match sector as i32 % 6 {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

/// A frame with neither a face nor a scene has no shot scale, and says so.
///
/// Not "a frame with no face": the scene is a second, weaker route - a details scene is tight and a
/// venue is wide, which `scale_of_scene` encodes - and that route is why the shipped build can
/// measure a rhythm over part of an album at all. What must never happen is a *default*: a frame
/// nobody could place reported as Medium would put the rhythm score's denominator at one hundred
/// per cent of every album. Phase 24's rule.
#[test]
fn a_frame_with_no_face_and_no_scene_has_no_shot_scale() {
    let field = FixtureField::new(fixtures::wedding(Shape::as_shipped(100), 1));
    let frames = field.frames(field.wedding().project).expect("frames");
    let mut bare = 0usize;
    for frame in &frames {
        let mut stripped = frame.clone();
        stripped.scene = None;
        stripped.faces.clear();
        assert_eq!(
            stripped.scale(),
            ShotScale::Unknown,
            "a frame with nothing to go on reported a shot scale"
        );
        bare += 1;
    }
    assert!(bare > 0, "the fixture produced no frames");
}
