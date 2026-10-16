// Tests assert by unwrapping; a panic here is a failed test, never a photographer's crash.
#![allow(clippy::unwrap_used, clippy::disallowed_methods)]
//! Clearing stretch marks on painted skin: marks go, shading, steps and grain stay. ADR-0118.

use aura_recipe::retouch_tools::{Edit, Matte, Tool};
use std::collections::BTreeMap;

// Large enough that the window structure is measured over is wider than a painted mark.
const W: usize = 480;
const H: usize = 320;

/// Skin lit from the left, times whatever `mark` says at each cell.
fn frame(mark: impl Fn(usize, usize) -> f32) -> Vec<f32> {
    (0..W * H)
        .flat_map(|i| {
            let (x, y) = (i % W, i / W);
            let shade = 0.5 - 0.15 * x as f32 / W as f32;
            [0.9, 0.7, 0.6].map(|c| c * shade * mark(x, y))
        })
        .collect()
}

fn operation() -> Edit {
    Edit {
        id: "marks".into(),
        tool: Tool::StretchMarks,
        enabled: true,
        region: [0.5, 0.5, 1.0, 1.0],
        source: None,
        amount: 1.0,
        feather: 0.0,
        radius: 0.05,
        source_scale: 1.0,
        preserve_microtexture: false,
        texture_heal: false,
        clean_ring_fit: false,
        curved_heal: false,
        heal_samples: Vec::new(),
        texture_sources: Vec::new(),
        repair_passes: None,
        target_color: None,
        sensitivity: None,
        keep_dark_marks: false,
        mole_edge: None,
        texture: 1.0,
        tone: 1.0,
        warmth: 0.0,
        tint: 0.0,
        selection: None,
        matte: None,
        mask: None,
        skin: None,
    }
}

fn green(pixels: &[f32], x: usize, y: usize) -> f32 {
    pixels[(y * W + x) * 3 + 1]
}

#[test]
fn thin_marks_are_cleared_and_the_shading_under_them_is_kept() {
    // Pale and dark lines three cells wide, a tenth away from the skin they cross.
    let mark = |x: usize, y: usize| {
        if (80..240).contains(&y) && x % 24 < 3 && (60..400).contains(&x) {
            if (x / 24).is_multiple_of(2) {
                1.1
            } else {
                0.9
            }
        } else {
            1.0
        }
    };
    let clean = frame(|_, _| 1.0);
    let mut pixels = frame(mark);
    // How far any sample is from the same skin without marks, as a share of that skin.
    let off = |pixels: &[f32]| -> f32 {
        pixels
            .iter()
            .zip(&clean)
            .map(|(a, b)| (a / b - 1.0).abs())
            .fold(0.0, f32::max)
    };
    assert!(off(&pixels) > 0.099);
    aura_render::retouch_tools::apply(&mut pixels, W, H, &[operation()]);
    // What is left is the lines' own share of the tone, spread over the skin between them.
    let left = off(&pixels);
    assert!(
        left < 0.01,
        "marks left at {:.2}% of the skin",
        left * 100.0
    );
    // The light still falls off from left to right as it did.
    for x in [40, 240, 440] {
        let ratio = green(&pixels, x, 160) / green(&clean, x, 160);
        assert!((ratio - 1.0).abs() < 0.03, "shading at {x}: {ratio}");
    }
}

#[test]
fn a_step_is_structure_and_nothing_is_carried_across_it() {
    // A finger: a band twice as bright, with hard edges, lying across the skin.
    let mut pixels = frame(|_, y| if (140..200).contains(&y) { 2.0 } else { 1.0 });
    let before = pixels.clone();
    aura_render::retouch_tools::apply(&mut pixels, W, H, &[operation()]);
    for x in [60, 240, 420] {
        for y in [139, 140, 199, 200] {
            let (was, is) = (green(&before, x, y), green(&pixels, x, y));
            assert!(
                (is - was).abs() < 1e-6,
                "edge moved at {x},{y}: {was} -> {is}"
            );
        }
        // Either side of the edge keeps its own tone.
        let ratio = green(&pixels, x, 170) / green(&pixels, x, 80);
        assert!(
            (ratio - 2.0).abs() < 0.04,
            "tone crossed the edge at {x}: {ratio}"
        );
    }
}

#[test]
fn clean_grain_survives_and_nothing_outside_the_selection_moves() {
    let grain = |x: usize, y: usize| 1.0 + 0.01 * (((x * 7 + y * 13) % 5) as f32 - 2.0);
    let mut pixels = frame(grain);
    let before = pixels.clone();
    let mut op = operation();
    op.matte = Some("left".into());
    let mattes = BTreeMap::from([(
        "left".to_owned(),
        Matte::encode([0.0, 0.0, 0.5, 1.0], 4, 4, &[255; 16]),
    )]);
    aura_render::retouch_tools::apply_with_mattes(&mut pixels, W, H, &[op], &mattes);
    let energy = |v: &[f32]| -> f32 {
        (40..180)
            .map(|x| (green(v, x, 160) - green(v, x + 1, 160)).abs())
            .sum()
    };
    let kept = energy(&pixels) / energy(&before);
    assert!(kept > 0.6, "grain kept: {kept}");
    for x in 260..W {
        for y in [20, 160, 300] {
            let i = (y * W + x) * 3;
            assert!(
                (pixels[i] - before[i]).abs() < f32::EPSILON,
                "moved at {x},{y}"
            );
        }
    }
}
