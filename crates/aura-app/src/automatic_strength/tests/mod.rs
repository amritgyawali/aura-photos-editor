//! Regression fixtures for the saved automatic finishing controller.
#![allow(clippy::expect_used, clippy::float_cmp)]
use super::*;

fn edit(id: &str, tool: &str, amount: f32) -> Edit {
    serde_json::from_value(serde_json::json!({
        "id":id,"tool":tool,"enabled":true,"region":[0.5,0.5,1.0,1.0],
        "source":[0.15,0.15],"amount":amount,"feather":0.0,"radius":0.03,
        "texture":1.0,"tone":0.5,"warmth":0.0,"tint":0.0
    }))
    .expect("fixture operation")
}
fn baseline() -> Recipe {
    let mut recipe = aura_recipe::fixtures::neutral(aura_recipe::fixtures::FIXTURE_HASH, "test");
    recipe.global.exposure = 0.25;
    let mut disabled = edit("auto-portrait-v1-0-disabled", "skin_smooth", 0.7);
    disabled.enabled = false;
    retouch_tools::write(
        &mut recipe,
        &[
            edit("auto-portrait-v1-0-spot-1", "acne_clear", 0.8),
            edit("auto-portrait-v1-0-texture", "skin_smooth", 0.4),
            edit("photographer-repair", "dodge", 0.3),
            disabled,
        ],
    )
    .expect("fixture recipe");
    recipe
}
fn amounts(recipe: &Recipe) -> Vec<f32> {
    retouch_tools::read(recipe)
        .expect("operations")
        .iter()
        .map(|e| e.amount)
        .collect()
}

#[test]
fn category_strengths_preserve_manual_disabled_and_unrelated_edits() {
    let base = baseline();
    let next = adjusted(
        &base,
        &Strengths {
            overall: 1.0,
            categories: BTreeMap::from([(Category::Blemishes, 0.25)]),
        },
    )
    .expect("finishing");
    assert_eq!(amounts(&next), [0.2, 0.4, 0.3, 0.7]);
    assert_eq!(next.global, base.global);
    let saved = retouch_tools::read(&next).expect("operations");
    assert!(saved.first().expect("blemish").id.starts_with(MANAGED));
    assert_eq!(
        saved.get(2).expect("manual"),
        retouch_tools::read(&base)
            .expect("original")
            .get(2)
            .expect("original manual")
    );
    assert!(!saved.get(3).expect("disabled").enabled);
}

#[test]
fn zero_full_and_increased_strengths_are_absolute_after_serialization() {
    let base = baseline();
    let zero = adjusted(
        &base,
        &Strengths {
            overall: 0.0,
            ..Strengths::default()
        },
    )
    .expect("zero");
    assert_eq!(amounts(&zero), [0.0, 0.0, 0.3, 0.7]);
    let reloaded: Recipe =
        serde_json::from_str(&aura_recipe::canonical(&zero).expect("save")).expect("reload");
    let full = adjusted(&reloaded, &Strengths::default()).expect("restore");
    assert_eq!(amounts(&full), amounts(&base));
    let stronger = adjusted(
        &full,
        &Strengths {
            overall: 2.0,
            ..Strengths::default()
        },
    )
    .expect("stronger");
    assert_eq!(amounts(&stronger), [1.0, 0.8, 0.3, 0.7]);
    assert_eq!(
        amounts(&adjusted(&stronger, &Strengths::default()).expect("restore again")),
        amounts(&base)
    );
}

#[test]
fn automatic_mattes_are_frozen_before_a_later_detection_can_replace_them() {
    let mut base = baseline();
    let mut edits = retouch_tools::read(&base).expect("operations");
    edits.first_mut().expect("blemish").matte = Some("auto-portrait-v1-0-face".into());
    let matte = retouch_tools::Matte::encode([0.0, 0.0, 1.0, 1.0], 2, 2, &[255, 0, 255, 0]);
    retouch_tools::write_with_mattes(
        &mut base,
        &edits,
        &BTreeMap::from([("auto-portrait-v1-0-face".into(), matte.clone())]),
    )
    .expect("selection");
    let next = adjusted(
        &base,
        &Strengths {
            overall: 0.5,
            ..Strengths::default()
        },
    )
    .expect("finishing");
    let changed = retouch_tools::read(&next).expect("operations");
    let id = changed
        .first()
        .expect("blemish")
        .matte
        .as_ref()
        .expect("frozen matte");
    assert!(id.starts_with("manual-"));
    assert_eq!(
        retouch_tools::read_mattes(&next)
            .expect("selections")
            .get(id),
        Some(&matte)
    );
    assert_eq!(
        amounts(&adjusted(&next, &Strengths::default()).expect("restored")),
        amounts(&base)
    );
}

#[test]
fn individually_overridden_steps_do_not_rejoin_group_adjustment() {
    let half = adjusted(
        &baseline(),
        &Strengths {
            overall: 0.5,
            ..Strengths::default()
        },
    )
    .expect("half");
    let mut edits = retouch_tools::read(&half).expect("operations");
    let smoothing = edits.get_mut(1).expect("smoothing");
    smoothing.id = format!("manual-{}", smoothing.id);
    smoothing.amount = 0.1;
    let mut manual = half;
    retouch_tools::write(&mut manual, &edits).expect("manual override");
    let restored = adjusted(&manual, &Strengths::default()).expect("restore other groups");
    assert_eq!(amounts(&restored), [0.8, 0.1, 0.3, 0.7]);
}

#[test]
fn invalid_strengths_and_corrupt_baselines_are_refused() {
    for overall in [-0.1, 2.1, f32::NAN, f32::INFINITY] {
        assert!(adjusted(
            &baseline(),
            &Strengths {
                overall,
                ..Strengths::default()
            }
        )
        .is_err());
    }
    let mut corrupt = baseline();
    corrupt
        .extra
        .insert(KEY.into(), serde_json::json!({"strengths":"invalid"}));
    assert!(report(&corrupt).is_err());
}

#[test]
fn new_automatic_steps_are_identified_without_changing_the_saved_recipe() {
    let mut half = adjusted(
        &baseline(),
        &Strengths {
            overall: 0.5,
            ..Strengths::default()
        },
    )
    .expect("half");
    let mut edits = retouch_tools::read(&half).expect("operations");
    edits.push(edit("auto-portrait-v1-1-texture", "skin_smooth", 0.6));
    retouch_tools::write(&mut half, &edits).expect("new detection");
    let before = aura_recipe::canonical(&half).expect("before");
    assert_eq!(report(&half).expect("controls").new_operations, 1);
    assert_eq!(aura_recipe::canonical(&half).expect("after"), before);
    assert_eq!(
        amounts(&adjusted(&half, &Strengths::default()).expect("restore")),
        [0.8, 0.4, 0.3, 0.7, 0.6]
    );
}

#[test]
fn anatomical_and_scene_identity_takes_priority_over_shared_rendering_tools() {
    for (id, tool, expected) in [
        ("auto-scene-v1-sky", "burn", Category::Background),
        (
            "auto-portrait-v1-0-background-lift",
            "dodge",
            Category::Background,
        ),
        (
            "auto-portrait-v1-0-lines-neck",
            "frequency",
            Category::Wrinkles,
        ),
        (
            "auto-portrait-v1-0-undereye-bag",
            "frequency",
            Category::Eyes,
        ),
        ("auto-portrait-v1-0-hair-shine", "dodge", Category::Hair),
        ("auto-portrait-v1-0-fabric", "frequency", Category::Clothing),
    ] {
        let operation = edit(id, tool, 0.5);
        assert_eq!(category(&operation), Some(expected), "{id}");
    }
}
