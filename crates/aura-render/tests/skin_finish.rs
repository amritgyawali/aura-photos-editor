//! Frequency healing and the texture graft on skin whose answer is known. ADR-0090.
//!
//! The frames are painted: a skin tone with a slope of light, pore-sized relief, and marks
//! whose size, colour and position this file chose. That proves the arithmetic - what is
//! found, what is left alone, what is conserved - and says nothing about a photograph of a
//! person. `skin_finish_photos.rs` is the by-hand check on real pixels.
// Tests assert by unwrapping; a panic here is a failed test, never a photographer's crash.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::disallowed_methods,
    clippy::float_cmp
)]

use aura_recipe::retouch_tools::{self, Edit, Tool};
use aura_render::retouch_tools::{apply, frequency_heal_marks};
use std::collections::BTreeMap;

const W: usize = 256;

fn luma(p: &[f32]) -> f32 {
    p[0] * 0.2627 + p[1] * 0.6780 + p[2] * 0.0593
}

/// An operation over the whole frame.
fn operation(tool: Tool) -> Edit {
    Edit {
        id: "finish".into(),
        tool,
        enabled: true,
        // The ellipse reaches past every corner, so the whole frame is selected.
        region: [0.5, 0.5, 0.75, 0.75],
        source: None,
        amount: 1.0,
        feather: 0.0,
        // Four pixels on this frame: pores below it, marks above.
        radius: 4.0 / W as f32,
        source_scale: 1.0,
        preserve_microtexture: false,
        texture_heal: false,
        clean_ring_fit: false,
        curved_heal: false,
        heal_samples: Vec::new(),
        texture_sources: Vec::new(),
        target_color: None,
        sensitivity: None,
        keep_dark_marks: false,
        texture: 0.25,
        tone: 1.0,
        warmth: 0.0,
        tint: 0.0,
        selection: None,
        matte: None,
        mask: None,
        skin: None,
    }
}

/// Pore relief: a fixed pattern of about three per cent, different at every pixel.
fn pore(x: usize, y: usize) -> f32 {
    let mut v = (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA77);
    v ^= v >> 13;
    v = v.wrapping_mul(0xC2B2_AE3D);
    v ^= v >> 16;
    ((v % 2001) as f32 / 1000.0 - 1.0) * 0.03
}

/// Skin of one tone under light that falls off across the frame, with pores.
fn skin(tone: [f32; 3], exposure: f32) -> Vec<f32> {
    (0..W * W)
        .flat_map(|i| {
            let (x, y) = (i % W, i / W);
            let light = exposure * (1.0 - x as f32 * 0.0012 + y as f32 * 0.0004);
            let relief = 1.0 + pore(x, y);
            tone.map(|v| v * light * relief)
        })
        .collect()
}

/// Multiply a soft-edged disk by `colour`.
fn mark(rgb: &mut [f32], centre: [usize; 2], radius: f32, colour: [f32; 3]) {
    for y in 0..W {
        for x in 0..W {
            let d = (x as f32 - centre[0] as f32).hypot(y as f32 - centre[1] as f32);
            let t = (1.0 - (d - radius + 1.0).clamp(0.0, 1.0)).clamp(0.0, 1.0);
            for c in 0..3 {
                rgb[(y * W + x) * 3 + c] *= 1.0 + t * (colour[c] - 1.0);
            }
        }
    }
}

/// How much darker the disk is than the ring around it, as a share of the ring.
fn contrast(rgb: &[f32], centre: [usize; 2], radius: f32) -> f32 {
    let (mut inside, mut n_in, mut ring, mut n_ring) = (0.0, 0.0, 0.0, 0.0);
    for y in 0..W {
        for x in 0..W {
            let d = (x as f32 - centre[0] as f32).hypot(y as f32 - centre[1] as f32);
            let l = luma(&rgb[(y * W + x) * 3..(y * W + x) * 3 + 3]);
            if d <= radius * 0.6 {
                inside += l;
                n_in += 1.0;
            } else if d >= radius * 2.2 && d <= radius * 3.0 {
                ring += l;
                n_ring += 1.0;
            }
        }
    }
    1.0 - (inside / n_in) / (ring / n_ring)
}

/// Mean absolute difference between a pixel's luminance and its 3x3 mean, relative to the
/// mean: pore-scale relief.
fn relief(rgb: &[f32], x0: usize, y0: usize, size: usize) -> f32 {
    let mut total = 0.0;
    for y in y0..y0 + size {
        for x in x0..x0 + size {
            let mut mean = 0.0;
            for dy in 0..3 {
                for dx in 0..3 {
                    let i = ((y + dy - 1) * W + x + dx - 1) * 3;
                    mean += luma(&rgb[i..i + 3]) / 9.0;
                }
            }
            let i = (y * W + x) * 3;
            total += (luma(&rgb[i..i + 3]) - mean).abs() / mean;
        }
    }
    total / (size * size) as f32
}

const RED_MARK: [f32; 3] = [0.80, 0.52, 0.52];
const DARK_MARK: [f32; 3] = [0.62, 0.62, 0.62];
const SPOTS: [[usize; 2]; 4] = [[60, 60], [150, 70], [90, 170], [200, 190]];

/// Skin with four red marks and a long dark crease.
fn marked(tone: [f32; 3], exposure: f32) -> Vec<f32> {
    let mut rgb = skin(tone, exposure);
    for centre in SPOTS {
        mark(&mut rgb, centre, 5.0, RED_MARK);
    }
    for x in 30..200 {
        for y in 120..122 {
            for c in 0..3 {
                rgb[(y * W + x) * 3 + c] *= 0.72;
            }
        }
    }
    rgb
}

#[test]
fn frequency_healing_rebuilds_compact_marks_and_leaves_lines_and_clean_skin_alone() {
    let before = marked([0.42, 0.30, 0.22], 1.0);
    let edit = operation(Tool::FrequencyHeal);
    retouch_tools::validate(std::slice::from_ref(&edit)).unwrap();
    let marks = frequency_heal_marks(&before, W, W, &edit, &BTreeMap::new());
    let mut after = before.clone();
    apply(&mut after, W, W, std::slice::from_ref(&edit));
    for centre in SPOTS {
        assert!(
            marks[centre[1] * W + centre[0]] > 0.9,
            "mark at {centre:?} not found"
        );
        let (was, is) = (
            contrast(&before, centre, 5.0),
            contrast(&after, centre, 5.0),
        );
        assert!(was > 0.3, "the fixture mark at {centre:?} measures {was}");
        assert!(is.abs() < 0.025, "mark at {centre:?}: {was} -> {is}");
    }
    // A crease is long and thin: never a mark, at any threshold, and never partly healed.
    for x in (40..190).step_by(10) {
        assert_eq!(marks[121 * W + x], 0.0, "the crease was marked at x={x}");
        let i = (121 * W + x) * 3;
        assert_eq!(&after[i..i + 3], &before[i..i + 3]);
    }
    // Skin with nothing wrong with it is not a little bit healed: it is untouched.
    let untouched = marks.iter().filter(|m| **m == 0.0).count();
    assert!(
        untouched > W * W * 9 / 10,
        "{untouched} of {} unmarked",
        W * W
    );
    for (i, m) in marks.iter().enumerate() {
        if *m == 0.0 {
            assert_eq!(
                &after[i * 3..i * 3 + 3],
                &before[i * 3..i * 3 + 3],
                "pixel {i}"
            );
        }
    }
    // Pores under a repair are still pores: at least half the relief the same skin has
    // with no mark on it, and no more than it had.
    let unmarked = relief(&skin([0.42, 0.30, 0.22], 1.0), 52, 52, 16);
    let is = relief(&after, 52, 52, 16);
    assert!(
        is > unmarked * 0.5 && is < unmarked * 1.2,
        "relief {unmarked} -> {is}"
    );
    let mut again = before.clone();
    apply(&mut again, W, W, std::slice::from_ref(&edit));
    assert_eq!(after, again, "frequency healing must be deterministic");
}

#[test]
fn small_red_marks_are_repaired_without_removing_surrounding_pores() {
    let clean = skin([0.42, 0.30, 0.22], 1.0);
    let mut before = clean.clone();
    mark(&mut before, [60, 60], 2.5, RED_MARK);
    let mut edit = operation(Tool::FrequencyHeal);
    edit.sensitivity = Some(0.85);
    let mut after = before.clone();
    apply(&mut after, W, W, &[edit]);
    let at = (60 * W + 60) * 3;
    assert!(
        (luma(&after[at..at + 3]) - luma(&clean[at..at + 3])).abs()
            < (luma(&before[at..at + 3]) - luma(&clean[at..at + 3])).abs() * 0.5
    );
    for y in 100..140 {
        for x in 100..140 {
            let i = (y * W + x) * 3;
            assert_eq!(&after[i..i + 3], &before[i..i + 3]);
        }
    }
}

#[test]
fn frequency_healing_preserves_curved_chromatic_shadow_boundaries() {
    let mut before = skin([0.42, 0.30, 0.22], 1.0);
    // A curved nose-like shadow has both a luminance transition and a colour
    // transition. Redness alone must not turn its bright edge into a blemish.
    for y in 0..W {
        for x in 0..W {
            let edge = 128.0 + 12.0 * (y as f32 / 24.0).sin();
            let shade = ((x as f32 - edge) / 5.0).tanh() * 0.5 + 0.5;
            for (c, dark) in [0.20, 0.28, 0.32].into_iter().enumerate() {
                before[(y * W + x) * 3 + c] *= 1.0 + shade * (dark - 1.0);
            }
        }
    }
    mark(&mut before, [60, 60], 5.0, RED_MARK);
    let mut after = before.clone();
    apply(&mut after, W, W, &[operation(Tool::FrequencyHeal)]);
    for y in 16..W - 16 {
        for x in 100..166 {
            let i = (y * W + x) * 3;
            assert_eq!(&before[i..i + 3], &after[i..i + 3], "shadow at {x},{y}");
        }
    }
    assert!(contrast(&after, [60, 60], 5.0).abs() < 0.04);
}

#[test]
fn frequency_healing_measures_ratios_so_exposure_and_skin_tone_do_not_change_what_it_finds() {
    let edit = operation(Tool::FrequencyHeal);
    for tone in [[0.62, 0.48, 0.40], [0.42, 0.30, 0.22], [0.12, 0.075, 0.05]] {
        let reference = {
            let mut rgb = marked(tone, 1.0);
            apply(&mut rgb, W, W, std::slice::from_ref(&edit));
            rgb
        };
        for exposure in [0.08, 3.0] {
            let before = marked(tone, exposure);
            let mut after = before.clone();
            apply(&mut after, W, W, std::slice::from_ref(&edit));
            for centre in SPOTS {
                let is = contrast(&after, centre, 5.0);
                assert!(is.abs() < 0.025, "tone {tone:?} at {exposure}x: {is}");
            }
            // The same repair, scaled: nothing here knows how bright the frame is.
            for (scaled, expected) in after.iter().zip(&reference) {
                assert!(
                    (scaled / exposure - expected).abs() <= expected.abs() * 0.02 + 1e-5,
                    "tone {tone:?} at {exposure}x: {} vs {expected}",
                    scaled / exposure
                );
            }
        }
    }
}

#[test]
fn a_dark_mark_is_kept_when_asked_and_a_red_one_is_healed_either_way() {
    let mut before = skin([0.42, 0.30, 0.22], 1.0);
    mark(&mut before, [80, 128], 5.0, DARK_MARK);
    mark(&mut before, [176, 128], 5.0, RED_MARK);
    let mut keeping = operation(Tool::FrequencyHeal);
    keeping.keep_dark_marks = true;
    let mut kept = before.clone();
    apply(&mut kept, W, W, std::slice::from_ref(&keeping));
    assert!(
        contrast(&kept, [80, 128], 5.0) > 0.3,
        "the mole was removed"
    );
    assert!(contrast(&kept, [176, 128], 5.0).abs() < 0.05);
    let mut all = before.clone();
    apply(&mut all, W, W, &[operation(Tool::FrequencyHeal)]);
    assert!(contrast(&all, [80, 128], 5.0).abs() < 0.05);
    assert!(contrast(&all, [176, 128], 5.0).abs() < 0.05);
}

#[test]
fn frequency_healing_changes_nothing_outside_its_selection() {
    let before = marked([0.42, 0.30, 0.22], 1.0);
    let mut edit = operation(Tool::FrequencyHeal);
    // The left half only: two of the four marks are outside it.
    edit.region = [0.25, 0.5, 0.2, 0.45];
    let coverage = aura_render::retouch_tools::selection_mask(&before, W, W, &edit);
    let mut after = before.clone();
    apply(&mut after, W, W, std::slice::from_ref(&edit));
    for (i, a) in coverage.iter().enumerate() {
        if *a == 0.0 {
            assert_eq!(&after[i * 3..i * 3 + 3], &before[i * 3..i * 3 + 3]);
        }
    }
    assert!(contrast(&after, [60, 60], 5.0).abs() < 0.06);
    assert!(contrast(&after, [200, 190], 5.0) > 0.3);
}

#[test]
fn a_mark_in_the_soft_edge_of_a_selection_is_rebuilt_as_fully_as_one_inside_it() {
    let before = marked([0.42, 0.30, 0.22], 1.0);
    let mut edit = operation(Tool::FrequencyHeal);
    // Nothing selected at the left edge, everything from just left of the middle on.
    edit.selection = Some(retouch_tools::Selection {
        gradient: Some(retouch_tools::Gradient {
            start: [0.0, 0.5],
            end: [0.45, 0.5],
        }),
        ..retouch_tools::Selection::default()
    });
    retouch_tools::validate(std::slice::from_ref(&edit)).unwrap();
    let coverage = aura_render::retouch_tools::selection_mask(&before, W, W, &edit);
    let soft = coverage[60 * W + 60];
    assert!(soft > 0.3 && soft < 0.7, "the fixture's edge is {soft}");
    let mut after = before.clone();
    apply(&mut after, W, W, std::slice::from_ref(&edit));
    // Half a mark is still a mark: the one in the soft edge goes as completely as the others.
    for centre in SPOTS {
        let is = contrast(&after, centre, 5.0);
        assert!(is.abs() < 0.03, "mark at {centre:?}: {is}");
    }
}

/// A patch of skin flattened by an earlier operation, then the graft.
fn flattened_then_grafted(graft: bool) -> (Vec<f32>, Vec<f32>) {
    let before = skin([0.42, 0.30, 0.22], 1.0);
    let mut flatten = operation(Tool::Backdrop);
    flatten.id = "flatten".into();
    flatten.region = [0.5, 0.5, 0.14, 0.14];
    flatten.feather = 0.3;
    let mut stack = vec![flatten];
    if graft {
        let mut edit = operation(Tool::TextureGraft);
        edit.radius = 1.0 / W as f32;
        edit.texture = 1.0;
        stack.push(edit);
    }
    retouch_tools::validate(&stack).unwrap();
    let mut after = before.clone();
    apply(&mut after, W, W, &stack);
    (before, after)
}

#[test]
fn the_graft_gives_flattened_skin_its_pores_back_without_moving_tone_or_colour() {
    let (before, flat) = flattened_then_grafted(false);
    let (_, grafted) = flattened_then_grafted(true);
    let (was, lost, is) = (
        relief(&before, 112, 112, 32),
        relief(&flat, 112, 112, 32),
        relief(&grafted, 112, 112, 32),
    );
    assert!(
        lost < was * 0.25,
        "the fixture did not flatten: {was} -> {lost}"
    );
    assert!(
        is > was * 0.6 && is < was * 1.3,
        "relief {was} -> {lost} -> {is}"
    );
    // Tone: the patch's mean luminance is where the flattening left it.
    let mean = |rgb: &[f32]| {
        let mut sum = 0.0;
        for y in 112..144 {
            for x in 112..144 {
                sum += luma(&rgb[(y * W + x) * 3..(y * W + x) * 3 + 3]);
            }
        }
        sum / 1024.0
    };
    assert!((mean(&grafted) / mean(&flat) - 1.0).abs() < 0.01);
    // Colour: a graft multiplies luminance, so every pixel keeps its channel ratios.
    for (a, b) in grafted.chunks_exact(3).zip(flat.chunks_exact(3)) {
        assert!((a[0] / a[1] - b[0] / b[1]).abs() < 1e-4);
        assert!((a[2] / a[1] - b[2] / b[1]).abs() < 1e-4);
    }
    // Skin that lost nothing gains nothing.
    let (far_was, far_is) = (relief(&before, 16, 16, 32), relief(&grafted, 16, 16, 32));
    assert!(
        (far_is / far_was - 1.0).abs() < 0.08,
        "{far_was} -> {far_is}"
    );
    assert_eq!(
        grafted,
        flattened_then_grafted(true).1,
        "the graft must be deterministic"
    );
}

#[test]
fn the_graft_limits_glints_and_invents_no_texture_for_skin_that_has_none() {
    // Glints: single pixels far brighter than any pore.
    let mut before = skin([0.42, 0.30, 0.22], 1.0);
    let glints = [[70_usize, 70_usize], [130, 90], [180, 160], [90, 200]];
    for [x, y] in glints {
        for c in 0..3 {
            before[(y * W + x) * 3 + c] *= 1.45;
        }
    }
    let mut edit = operation(Tool::TextureGraft);
    edit.radius = 1.0 / W as f32;
    edit.texture = 1.0;
    let mut after = before.clone();
    apply(&mut after, W, W, std::slice::from_ref(&edit));
    for [x, y] in glints {
        let i = (y * W + x) * 3;
        let around = (y * W + x + 3) * 3;
        let was = luma(&before[i..i + 3]) / luma(&before[around..around + 3]);
        let is = luma(&after[i..i + 3]) / luma(&after[around..around + 3]);
        assert!(
            is - 1.0 < (was - 1.0) * 0.6,
            "glint at {x},{y}: {was} -> {is}"
        );
    }
    // Perfectly smooth skin has no pores to borrow and none to restore: it stays smooth.
    let smooth: Vec<f32> = (0..W * W)
        .flat_map(|i| [0.42, 0.30, 0.22].map(|v| v * (1.0 - (i % W) as f32 * 0.001)))
        .collect();
    let mut still = smooth.clone();
    apply(&mut still, W, W, std::slice::from_ref(&edit));
    assert_eq!(still, smooth);
    // And strength zero is no operation at all.
    edit.amount = 0.0;
    let mut off = before.clone();
    apply(&mut off, W, W, std::slice::from_ref(&edit));
    assert_eq!(off, before);
}

#[test]
fn the_new_fields_are_optional_bounded_and_absent_from_recipes_that_do_not_use_them() {
    let edit = operation(Tool::FrequencyHeal);
    let json = serde_json::to_value(&edit).unwrap();
    assert_eq!(json["tool"], "frequency_heal");
    assert!(json.get("sensitivity").is_none() && json.get("keepDarkMarks").is_none());
    assert_eq!(
        serde_json::to_value(operation(Tool::TextureGraft)).unwrap()["tool"],
        "texture_graft"
    );
    // A recipe written before the fields existed reads back unchanged.
    let old: Edit = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(old, edit);
    let mut set = edit.clone();
    set.sensitivity = Some(0.8);
    set.keep_dark_marks = true;
    let json = serde_json::to_value(&set).unwrap();
    assert!((json["sensitivity"].as_f64().unwrap() - 0.8).abs() < 1e-6);
    assert_eq!(json["keepDarkMarks"], true);
    assert_eq!(serde_json::from_value::<Edit>(json).unwrap(), set);
    for bad in [-0.1, 1.5, f32::NAN] {
        let mut wrong = edit.clone();
        wrong.sensitivity = Some(bad);
        assert!(
            retouch_tools::validate(&[wrong]).is_err(),
            "{bad} was accepted"
        );
    }
}
