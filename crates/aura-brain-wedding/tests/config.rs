//! The shipped config, and every rule the loaders refuse.
//!
//! Section 12's third failure mode is that `scene_profiles.toml` becomes a dumping
//! ground of magic numbers, and ADR-0015 section 3's rule is that a ritual id is a model
//! output slot. Both are enforced by loaders that refuse, and a loader that refuses is
//! only useful if something proves it refuses - which is what this file is for.

use aura_brain_wedding::scene::profile::{SceneProfileRegistry, MIN_RATIONALE};
use aura_brain_wedding::scene::ritual::TRADITIONS;
use aura_brain_wedding::scene::taxonomy::{Taxonomy, FIRST_ID, SLOT_COUNT};
use aura_core::contract::scene::{EditIntent, RitualId, SceneId};

// -- the shipped files -------------------------------------------------------

#[test]
fn the_shipped_taxonomies_load() {
    let taxonomy = Taxonomy::embedded().expect("the five shipped taxonomies load");
    assert_eq!(taxonomy.traditions().len(), 5);
    assert!(
        taxonomy.len() >= 40,
        "only {} rites declared",
        taxonomy.len()
    );
    assert!(taxonomy.version() >= 1);
}

#[test]
fn every_ritual_id_is_unique_and_in_range() {
    // The load-bearing rule. The id is the ritual head's output slot, so a duplicate
    // relabels a trained model and an out-of-range id addresses a slot that does not
    // exist. `Taxonomy::load` refuses both; this asserts the shipped files satisfy them.
    let taxonomy = Taxonomy::embedded().expect("loads");
    let mut seen = std::collections::BTreeSet::new();
    for rite in taxonomy.all() {
        assert!(
            rite.id >= FIRST_ID,
            "{} has id {}, and 0 is reserved for RitualId::NONE",
            rite.slug,
            rite.id
        );
        assert!(
            rite.id < SLOT_COUNT,
            "{} has id {}, at or above the head's {SLOT_COUNT} slots",
            rite.slug,
            rite.id
        );
        assert!(seen.insert(rite.id), "id {} is used twice", rite.id);
    }
}

#[test]
fn every_ritual_has_a_slug_a_title_and_evidence() {
    let taxonomy = Taxonomy::embedded().expect("loads");
    for rite in taxonomy.all() {
        assert!(!rite.slug.is_empty());
        assert!(!rite.title.is_empty(), "{} has no title", rite.slug);
        assert!(
            !rite.evidence.is_empty(),
            "{} has no visible evidence recorded; a rite nobody can describe is a rite \
             nobody can label",
            rite.slug
        );
    }
}

#[test]
fn every_declared_tradition_is_one_the_head_knows() {
    // One list of traditions in the product. A taxonomy file declaring `sikh` when the
    // head has no slot for it would silently produce rites the mask can never allow.
    let taxonomy = Taxonomy::embedded().expect("loads");
    for tradition in taxonomy.traditions() {
        assert!(
            TRADITIONS.contains(&tradition.as_str()),
            "the taxonomy declares tradition `{tradition}` which the ritual head has no slot for"
        );
    }
}

#[test]
fn the_cloud_vocabulary_and_the_taxonomy_agree() {
    // Every `cloud_label` a taxonomy file names must be a member of phase 04's frozen
    // vocabulary, or a cloud answer can never be mapped back to a rite.
    let taxonomy = Taxonomy::embedded().expect("loads");
    for rite in taxonomy.all() {
        if let Some(label) = &rite.cloud_label {
            assert!(
                aura_cloud::tasks::ALLOWED_RITUALS.contains(&label.as_str()),
                "{} maps to cloud label `{label}`, which is not in ALLOWED_RITUALS",
                rite.slug
            );
        }
    }
}

#[test]
fn a_rite_resolves_from_its_slug_its_title_and_its_aliases() {
    let taxonomy = Taxonomy::embedded().expect("loads");
    // The route a cloud answer takes back in. `ALLOWED_RITUALS` spells it
    // `jaimala_varmala`; a photographer types "jaymala"; the file's title has a slash.
    for spelling in ["jaimala_varmala", "jaymala", "Jaimala / Varmala", "varmala"] {
        assert!(
            taxonomy.resolve(spelling).is_some(),
            "`{spelling}` did not resolve to a rite"
        );
    }
    assert!(taxonomy.resolve("a rite nobody has heard of").is_none());
}

#[test]
fn the_shipped_profiles_load_and_are_total() {
    let registry = SceneProfileRegistry::embedded().expect("the shipped profiles load");
    for scene in SceneId::ALL {
        if scene == SceneId::Unknown {
            continue;
        }
        assert!(
            registry.rationale(scene).is_some(),
            "{} has no profile, so every wedding would grade it neutrally",
            scene.as_str()
        );
    }
    assert_eq!(registry.len(), SceneId::CLASSES);
}

#[test]
fn every_rationale_is_long_enough_to_be_a_reason() {
    let registry = SceneProfileRegistry::embedded().expect("loads");
    for scene in registry.scenes() {
        let rationale = registry.rationale(scene).unwrap_or_default();
        assert!(
            rationale.len() >= MIN_RATIONALE,
            "{}'s rationale is {} characters; a value nobody can explain does not load",
            scene.as_str(),
            rationale.len()
        );
        // A rationale that restates the number is not an explanation. The weakest
        // checkable form of that: it has to be a sentence, not a fragment.
        assert!(
            rationale.split_whitespace().count() >= 8,
            "{}'s rationale is {} words; say why a photographer would agree, not what the \
             number is",
            scene.as_str(),
            rationale.split_whitespace().count()
        );
    }
}

#[test]
fn the_worked_examples_from_section_six_three_hold() {
    // Section 6.3 names three specific behaviours. They are assertions, not prose.
    let registry = SceneProfileRegistry::embedded().expect("loads");

    // "`dance_floor` allows noise 3x the ceremony budget, expects motion blur, weights
    // emotion 0.5 and composition 0.15"
    let dance = registry.get(SceneId::DanceFloor);
    let ceremony = registry.get(SceneId::Ceremony);
    assert!(dance.max_acceptable_noise > ceremony.max_acceptable_noise);
    assert!(dance.max_acceptable_blur > ceremony.max_acceptable_blur * 1.5);
    assert!((dance.emotion_weight - 0.50).abs() < 0.01);
    assert!((dance.composition_weight - 0.15).abs() < 0.01);

    // "`family_portrait` ... weights composition 0.35 and forbids clipped faces"
    let family = registry.get(SceneId::FamilyPortrait);
    assert!((family.composition_weight - 0.35).abs() < 0.01);
    assert!(
        family.max_acceptable_blur < dance.max_acceptable_blur / 3.0,
        "a posed group is judged the way a client grades it, which is not how a dance \
         floor is judged"
    );

    // "`details` ignores faces entirely"
    let details = registry.get(SceneId::Details);
    assert!(details.subject_focus_weight.abs() < f32::EPSILON);
    assert!(details.emotion_weight.abs() < f32::EPSILON);
}

#[test]
fn the_coverage_guarantee_list_is_short_and_deliberate() {
    // `must_cover` is the most argued-over list in the file. It should be a minority of
    // scenes: a guarantee on everything is a guarantee on nothing.
    let registry = SceneProfileRegistry::embedded().expect("loads");
    let covered = registry.must_cover();
    assert!(
        !covered.is_empty(),
        "no scene has a coverage guarantee, so phase 12 has nothing to guarantee"
    );
    assert!(
        covered.len() < SceneId::CLASSES,
        "every scene has a coverage guarantee, which means none of them does"
    );
    // The four a client would notice the absence of first.
    for scene in [
        SceneId::Ceremony,
        SceneId::FamilyPortrait,
        SceneId::CouplePortrait,
        SceneId::Kiss,
    ] {
        assert!(
            registry.get(scene).must_cover,
            "{} has no coverage guarantee",
            scene.as_str()
        );
    }
    // And one it would not.
    assert!(!registry.get(SceneId::DanceFloor).must_cover);
}

// -- the refusals ------------------------------------------------------------

#[test]
fn a_profile_with_no_rationale_is_refused() {
    let text = r#"
version = 1
[scenes.ceremony]
keeper_rate = [0.1, 0.3]
max_acceptable_noise = 0.6
max_acceptable_blur = 0.3
subject_focus_weight = 0.4
emotion_weight = 0.3
composition_weight = 0.3
rationale = "short"
"#;
    let refusal = SceneProfileRegistry::parse("test.toml", text).expect_err("must refuse");
    assert_eq!(refusal.code.0, "AURA-ML-5024");
    assert!(
        refusal.detail.contains("rationale"),
        "the refusal did not name the rule: {}",
        refusal.detail
    );
}

#[test]
fn weights_summing_above_one_are_refused() {
    let text = r#"
version = 1
[scenes.ceremony]
keeper_rate = [0.1, 0.3]
max_acceptable_noise = 0.6
max_acceptable_blur = 0.3
subject_focus_weight = 0.6
emotion_weight = 0.5
composition_weight = 0.4
rationale = "a long enough sentence to pass the rationale check"
"#;
    let refusal = SceneProfileRegistry::parse("test.toml", text).expect_err("must refuse");
    assert!(refusal.detail.contains("summing"), "{}", refusal.detail);
}

#[test]
fn an_inverted_keeper_band_is_refused() {
    let text = r#"
version = 1
[scenes.ceremony]
keeper_rate = [0.5, 0.2]
max_acceptable_noise = 0.6
max_acceptable_blur = 0.3
subject_focus_weight = 0.4
emotion_weight = 0.3
composition_weight = 0.3
rationale = "a long enough sentence to pass the rationale check"
"#;
    let refusal = SceneProfileRegistry::parse("test.toml", text).expect_err("must refuse");
    assert!(refusal.detail.contains("keeper_rate"), "{}", refusal.detail);
}

#[test]
fn an_unknown_scene_key_is_refused() {
    let text = r#"
version = 1
[scenes.after_party]
keeper_rate = [0.1, 0.3]
max_acceptable_noise = 0.6
max_acceptable_blur = 0.3
subject_focus_weight = 0.4
emotion_weight = 0.3
composition_weight = 0.3
rationale = "a long enough sentence to pass the rationale check"
"#;
    let refusal = SceneProfileRegistry::parse("test.toml", text).expect_err("must refuse");
    assert!(refusal.detail.contains("22 scenes"), "{}", refusal.detail);
}

#[test]
fn a_profile_for_the_abstention_is_refused() {
    // `unknown` is not a scene, it is the absence of one, and giving it tolerances would
    // mean an unclassified frame got graded as though it had been classified.
    let text = r#"
version = 1
[scenes.unknown]
keeper_rate = [0.1, 0.3]
max_acceptable_noise = 0.6
max_acceptable_blur = 0.3
subject_focus_weight = 0.4
emotion_weight = 0.3
composition_weight = 0.3
rationale = "a long enough sentence to pass the rationale check"
"#;
    let refusal = SceneProfileRegistry::parse("test.toml", text).expect_err("must refuse");
    assert!(refusal.detail.contains("abstention"), "{}", refusal.detail);
}

#[test]
fn an_unknown_editing_intent_is_refused() {
    let text = r#"
version = 1
[scenes.ceremony]
keeper_rate = [0.1, 0.3]
max_acceptable_noise = 0.6
max_acceptable_blur = 0.3
subject_focus_weight = 0.4
emotion_weight = 0.3
composition_weight = 0.3
editing_intent = "cinematic"
rationale = "a long enough sentence to pass the rationale check"
"#;
    let refusal = SceneProfileRegistry::parse("test.toml", text).expect_err("must refuse");
    assert!(
        refusal.detail.contains("editing_intent"),
        "{}",
        refusal.detail
    );
}

#[test]
fn a_duplicate_ritual_id_is_refused_across_files() {
    let one = r#"
version = 1
tradition = "hindu"
[[ritual]]
id = 10
slug = "haldi"
title = "Haldi"
"#;
    let two = r#"
version = 1
tradition = "nepali"
[[ritual]]
id = 10
slug = "supari_chhopne"
title = "Supari Chhopne"
"#;
    let refusal = Taxonomy::load(&[("one.toml", one), ("two.toml", two)]).expect_err("must refuse");
    assert_eq!(refusal.code.0, "AURA-ML-5024");
    assert!(
        refusal.detail.contains("reuses id 10"),
        "{}",
        refusal.detail
    );
    assert!(
        refusal.detail.contains("output slot"),
        "the refusal did not say why an id matters: {}",
        refusal.detail
    );
}

#[test]
fn a_ritual_id_of_zero_is_refused() {
    let text = r#"
version = 1
tradition = "hindu"
[[ritual]]
id = 0
slug = "haldi"
title = "Haldi"
"#;
    let refusal = Taxonomy::load(&[("one.toml", text)]).expect_err("must refuse");
    assert!(refusal.detail.contains("reserved"), "{}", refusal.detail);
}

#[test]
fn a_ritual_id_past_the_head_is_refused() {
    let text = format!(
        r#"
version = 1
tradition = "hindu"
[[ritual]]
id = {}
slug = "haldi"
title = "Haldi"
"#,
        SLOT_COUNT
    );
    let refusal = Taxonomy::load(&[("one.toml", &text)]).expect_err("must refuse");
    assert!(refusal.detail.contains("retrain"), "{}", refusal.detail);
}

#[test]
fn a_non_snake_case_slug_is_refused() {
    let text = r#"
version = 1
tradition = "hindu"
[[ritual]]
id = 10
slug = "Haldi Ceremony"
title = "Haldi"
"#;
    let refusal = Taxonomy::load(&[("one.toml", text)]).expect_err("must refuse");
    assert!(refusal.detail.contains("snake_case"), "{}", refusal.detail);
}

#[test]
fn a_duplicate_slug_is_refused() {
    let text = r#"
version = 1
tradition = "hindu"
[[ritual]]
id = 10
slug = "haldi"
title = "Haldi"
[[ritual]]
id = 11
slug = "haldi"
title = "Haldi again"
"#;
    let refusal = Taxonomy::load(&[("one.toml", text)]).expect_err("must refuse");
    assert!(
        refusal.detail.contains("already declared"),
        "{}",
        refusal.detail
    );
}

// -- the mask ----------------------------------------------------------------

#[test]
fn the_mask_always_allows_the_abstention() {
    let taxonomy = Taxonomy::embedded().expect("loads");
    for tradition in [None, Some("hindu"), Some("civil"), Some("a made-up one")] {
        let mask = taxonomy.mask_for(tradition);
        assert_eq!(
            mask.first().copied(),
            Some(true),
            "the abstention was masked out for {tradition:?}"
        );
        assert_eq!(mask.len(), SLOT_COUNT as usize);
    }
}

#[test]
fn a_tradition_mask_excludes_another_traditions_rites() {
    let taxonomy = Taxonomy::embedded().expect("loads");
    let hindu = taxonomy.mask_for(Some("hindu"));

    let communion = taxonomy
        .by_slug("communion")
        .expect("christian.toml declares it");
    assert_eq!(
        hindu.get(communion.id as usize).copied(),
        Some(false),
        "a communion slot is live on a wedding whose evidence has been Hindu all day"
    );

    let saptapadi = taxonomy
        .by_slug("saptapadi_pheras")
        .expect("hindu.toml declares it");
    assert_eq!(hindu.get(saptapadi.id as usize).copied(), Some(true));
}

#[test]
fn no_tradition_allows_every_rite() {
    let taxonomy = Taxonomy::embedded().expect("loads");
    let open = taxonomy.mask_for(None);
    for rite in taxonomy.all() {
        assert_eq!(
            open.get(rite.id as usize).copied(),
            Some(true),
            "{} is masked out with no tradition established",
            rite.slug
        );
    }
}

// -- the contract's own invariants -------------------------------------------

#[test]
fn a_neutral_profile_is_gradeable() {
    // `SceneProfile::neutral` is what a missing row substitutes, and a substitution that
    // could not be graded would take a wedding out of the product.
    let neutral = aura_core::contract::scene::SceneProfile::neutral(SceneId::Candid);
    assert!(neutral.keeper_rate.0 < neutral.keeper_rate.1);
    assert_eq!(neutral.editing_intent, EditIntent::Neutral);
    assert!(!neutral.must_cover);
    // Exactly 1.0. The substitution for a missing row must not be systematically
    // weaker than a real profile; see `SceneProfile::neutral`.
    let sum = neutral.subject_focus_weight + neutral.emotion_weight + neutral.composition_weight;
    assert!((sum - 1.0).abs() < 1e-3, "neutral weights sum to {sum}");
}

#[test]
fn a_missing_profile_substitutes_rather_than_refuses() {
    // The asymmetry that is the design: refuse a config file that would silently change
    // every number, never refuse a wedding.
    let empty =
        SceneProfileRegistry::parse("empty.toml", "version = 1").expect("an empty file loads");
    let substituted = empty.get(SceneId::DanceFloor);
    assert_eq!(substituted.scene, SceneId::DanceFloor);
    assert_eq!(substituted.editing_intent, EditIntent::Neutral);
    assert!(empty.rationale(SceneId::DanceFloor).is_none());
}

#[test]
fn ritual_none_is_slot_zero() {
    assert!(RitualId::NONE.is_none());
    assert_eq!(RitualId::NONE.get(), 0);
    assert!(!RitualId::new(17).is_none());
}
