#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::disallowed_methods
)]
use aura_recipe::retouch_tools::{self, Edit, Tool};
use aura_render::{blemish_review, retouch_tools as render};
use std::collections::BTreeMap;

fn brush() -> Edit {
    serde_json::from_value(serde_json::json!({
        "id":"review", "tool":"acne_clear", "enabled":true,
        "region":[0.5,0.5,0.3,0.3], "source":null, "amount":0.35,
        "feather":0.2,"radius":0.025,"texture":0.25,"tone":1.0,
        "warmth":0.0,"tint":0.0,"sensitivity":1.0
    }))
    .unwrap()
}

fn photo() -> Vec<f32> {
    let mut pixels = [0.4, 0.26, 0.19].repeat(96 * 96);
    for y in 43..54 {
        for x in 43..54 {
            if (x as f32 - 48.0).hypot(y as f32 - 48.0) < 5.0 {
                let i = (y * 96 + x) * 3;
                pixels[i..i + 3].copy_from_slice(&[0.09, 0.035, 0.025]);
            }
        }
    }
    pixels
}

#[test]
fn extra_passes_improve_partial_repair_without_changing_the_unselected_photo() {
    let original = photo();
    let mut two = original.clone();
    let mut four = original.clone();
    let mut edit = brush();
    render::apply(&mut two, 96, 96, std::slice::from_ref(&edit));
    edit.repair_passes = Some(4);
    render::apply(&mut four, 96, 96, &[edit]);
    let centre = (48 * 96 + 48) * 3;
    assert!(
        four[centre] > two[centre],
        "Four passes must improve the partially healed core: {} <= {}",
        four[centre],
        two[centre]
    );
    for y in 0..96 {
        for x in 0..96 {
            if (((x as f32 + 0.5) / 96.0 - 0.5) / 0.3).hypot(((y as f32 + 0.5) / 96.0 - 0.5) / 0.3)
                >= 1.0
            {
                let i = (y * 96 + x) * 3;
                assert_eq!(&four[i..i + 3], &original[i..i + 3]);
            }
        }
    }
}

#[test]
fn old_recipes_keep_two_pass_rendering_and_invalid_passes_are_refused() {
    let old = brush();
    let serialized = serde_json::to_value(&old).unwrap();
    assert!(serialized.get("repairPasses").is_none());
    let mut explicit = old.clone();
    explicit.repair_passes = Some(2);
    let mut a = photo();
    let mut b = a.clone();
    render::apply(&mut a, 96, 96, &[old]);
    render::apply(&mut b, 96, 96, &[explicit.clone()]);
    assert_eq!(a, b);
    for passes in [0, 5, 255] {
        explicit.repair_passes = Some(passes);
        assert!(retouch_tools::validate(&[explicit.clone()]).is_err());
    }
    explicit.repair_passes = Some(4);
    retouch_tools::validate(&[explicit.clone()]).unwrap();
    explicit.tool = Tool::Dodge;
    assert!(retouch_tools::validate(&[explicit]).is_err());
}

#[test]
fn review_is_read_only_and_a_successful_repair_can_be_rechecked() {
    let original = photo();
    let unchanged = original.clone();
    let mut edit = brush();
    let before = blemish_review::measure(&original, 96, 96, &edit, &BTreeMap::new());
    assert!(before.measured_areas > 0);
    assert_eq!(original, unchanged);
    assert!(before
        .regions
        .iter()
        .all(|r| (r.region[0] - 0.5).hypot(r.region[1] - 0.5) < 0.15));
    let mut healed = original.clone();
    edit.amount = 1.0;
    edit.repair_passes = Some(4);
    render::apply(&mut healed, 96, 96, &[edit.clone()]);
    let after = blemish_review::measure(&healed, 96, 96, &edit, &BTreeMap::new());
    assert!(after.selected_mark_pixels < before.selected_mark_pixels);
}

#[test]
fn review_ignores_a_defect_outside_the_selected_area_and_clean_skin() {
    let edit = brush();
    let mut pixels = [0.4, 0.26, 0.19].repeat(96 * 96);
    for y in 6..16 {
        for x in 6..16 {
            pixels[(y * 96 + x) * 3..(y * 96 + x) * 3 + 3].fill(0.02);
        }
    }
    let result = blemish_review::measure(&pixels, 96, 96, &edit, &BTreeMap::new());
    assert_eq!(result.measured_areas, 0);
    assert_eq!(result.selected_mark_pixels, 0);
}
