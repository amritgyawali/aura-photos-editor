//! Objective signal tests for selected material/light effects; no model-quality claim.
#![allow(clippy::unwrap_used, clippy::disallowed_methods)]
use aura_recipe::retouch_tools::{self, BrushMask, BrushStroke, Edit, Tool};
use aura_render::retouch_tools::{apply, selection_mask};

const SIZE: usize = 96;
const TOOLS: [Tool; 5] = [
    Tool::FabricMoire,
    Tool::FabricStain,
    Tool::ShadowBalance,
    Tool::BackgroundBlur,
    Tool::HighlightGlow,
];

fn edit(tool: Tool) -> Edit {
    serde_json::from_value(serde_json::json!({
        "id":"material", "tool":tool, "enabled":true, "region":[0.5,0.5,1,1],
        "source":[0.12,0.12], "amount":1.0, "feather":0.0, "radius":0.04,
        "texture":1.0, "tone":1.0, "warmth":0.0, "tint":0.0
    }))
    .unwrap()
}

fn frame(f: impl Fn(usize, usize) -> [f32; 3]) -> Vec<f32> {
    (0..SIZE * SIZE)
        .flat_map(|i| f(i % SIZE, i / SIZE))
        .collect()
}

fn luminance(p: &[f32]) -> f32 {
    p[0] * 0.2627 + p[1] * 0.678 + p[2] * 0.0593
}

fn rendered(rgb: &[f32], op: &Edit) -> Vec<f32> {
    let mut result = rgb.to_vec();
    apply(&mut result, SIZE, SIZE, std::slice::from_ref(op));
    result
}

#[test]
fn moire_reduces_false_chroma_without_erasing_luminance_weave() {
    let input = frame(|x, y| {
        let weave = if (x + y) % 2 == 0 { 0.05 } else { -0.05 };
        let ripple = (x as f32 * 1.1).sin() * 0.12;
        [
            0.4 + weave + ripple,
            0.4 + weave - ripple * (0.2627 / 0.678),
            0.4 + weave,
        ]
    });
    let output = rendered(&input, &edit(Tool::FabricMoire));
    let energy = |rgb: &[f32]| {
        rgb.chunks_exact(3)
            .map(|p| (p[0] - p[1]).powi(2))
            .sum::<f32>()
    };
    assert!(
        energy(&output) < energy(&input) * 0.08,
        "false chroma must fall substantially"
    );
    for (before, after) in input.chunks_exact(3).zip(output.chunks_exact(3)) {
        assert!((luminance(before) - luminance(after)).abs() < 1e-6);
    }
}

#[test]
fn background_blur_never_borrows_an_excluded_subject_color() {
    let input = frame(|x, _| {
        if x < SIZE / 2 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 0.0, 0.4 + (x % 2) as f32 * 0.2]
        }
    });
    let mut op = edit(Tool::BackgroundBlur);
    op.region = [0.78, 0.5, 0.28, 1.0];
    let output = rendered(&input, &op);
    let mask = selection_mask(&input, SIZE, SIZE, &op);
    for ((before, after), weight) in input.chunks_exact(3).zip(output.chunks_exact(3)).zip(mask) {
        if weight == 0.0 {
            assert_eq!(before, after);
        } else {
            assert_eq!(after[0], 0.0);
            assert_eq!(after[1], 0.0);
        }
    }
    let index = (SIZE / 2 * SIZE + 72) * 3 + 2;
    assert!((output[index] - 0.5).abs() < 0.01);
}

#[test]
fn stain_correction_recovers_clean_material_without_cloning_its_texture() {
    let input = frame(|x, y| {
        let weave = if (x + y) % 2 == 0 { 1.05 } else { 0.95 };
        let stain = if x > 35 && x < 72 {
            [0.55, 0.7, 0.65]
        } else {
            [1.0; 3]
        };
        [
            0.5 * weave * stain[0],
            0.4 * weave * stain[1],
            0.3 * weave * stain[2],
        ]
    });
    let mut op = edit(Tool::FabricStain);
    op.region = [0.56, 0.5, 0.18, 0.45];
    op.feather = 0.15;
    let output = rendered(&input, &op);
    let at = |rgb: &[f32], x: usize| rgb[(48 * SIZE + x) * 3];
    assert!(
        (at(&output, 52) - 0.5).abs() < 0.07,
        "stain brightness recovers"
    );
    assert!(
        (at(&output, 52) / at(&output, 53) - 1.05 / 0.95).abs() < 0.015,
        "weave stays local"
    );
    assert_eq!(&input[..SIZE * 3], &output[..SIZE * 3]);
}

#[test]
fn shadow_balance_is_bounded_preserves_color_and_keeps_black_black() {
    let input = frame(|x, _| {
        let light = if x == 0 {
            0.0
        } else if x < SIZE / 3 {
            0.05
        } else {
            0.4
        };
        [light, light * 0.8, light * 0.6]
    });
    let mut op = edit(Tool::ShadowBalance);
    op.tone = 0.5; // One stop.
    let output = rendered(&input, &op);
    assert_eq!(&output[..3], &[0.0; 3]);
    let center = (48 * SIZE + 16) * 3;
    assert!(output[center] > input[center] * 1.5);
    for (old, new) in input
        .chunks_exact(3)
        .zip(output.chunks_exact(3))
        .filter(|(p, _)| p[0] > 0.0)
    {
        assert!(new[0] <= old[0] * 2.00001);
        assert!((new[1] / new[0] - 0.8).abs() < 1e-6);
        assert!((new[2] / new[0] - 0.6).abs() < 1e-6);
    }
    let bright = (48 * SIZE + 80) * 3;
    assert!((output[bright] - input[bright]).abs() < 1e-5);
}

#[test]
fn glow_requires_highlights_and_spreads_light_without_clipping_hdr() {
    let flat = frame(|_, _| [0.1; 3]);
    let mut op = edit(Tool::HighlightGlow);
    op.tone = 0.25;
    assert_eq!(rendered(&flat, &op), flat);
    let input = frame(|x, y| {
        if (44..52).contains(&x) && (44..52).contains(&y) {
            [4.0, 2.0, 1.0]
        } else {
            [0.1; 3]
        }
    });
    let output = rendered(&input, &op);
    let neighbor = (48 * SIZE + 55) * 3;
    assert!(output[neighbor] > input[neighbor] + 0.01);
    assert!(output[(48 * SIZE + 48) * 3] > 4.0);
    assert_eq!(&output[..3], &input[..3]);
    assert!(output.iter().all(|v| v.is_finite()));
}

#[test]
fn shadow_target_still_works_with_a_translucent_brush() {
    let input = frame(|x, _| if x < SIZE / 2 { [0.06; 3] } else { [0.4; 3] });
    let mut op = edit(Tool::ShadowBalance);
    op.mask = Some(BrushMask {
        strokes: vec![BrushStroke {
            erase: false,
            radius: 0.5,
            opacity: 0.3,
            points: vec![[0.5, 0.5, 1.0]],
        }],
    });
    let output = rendered(&input, &op);
    let index = (48 * SIZE + 32) * 3;
    assert!(
        output[index] > input[index],
        "a soft brush must not disable target measurement"
    );
}

#[test]
fn new_tools_round_trip_validate_and_require_a_clean_stain_reference() {
    for tool in TOOLS {
        let op = edit(tool);
        retouch_tools::validate(std::slice::from_ref(&op)).unwrap();
        assert_eq!(
            serde_json::from_str::<Edit>(&serde_json::to_string(&op).unwrap()).unwrap(),
            op
        );
    }
    let mut stain = edit(Tool::FabricStain);
    stain.source = None;
    assert!(retouch_tools::validate(&[stain]).is_err());
}

#[test]
fn every_new_tool_honors_zero_disabled_erased_and_outside_pixels() {
    let input = aura_render::fixtures::detail_frame(SIZE as u32, SIZE as u32).rgb;
    for tool in TOOLS {
        let mut op = edit(tool);
        op.amount = 0.0;
        assert_eq!(rendered(&input, &op), input);
        op.amount = 1.0;
        op.enabled = false;
        assert_eq!(rendered(&input, &op), input);
        op.enabled = true;
        op.mask = Some(BrushMask {
            strokes: vec![
                BrushStroke {
                    erase: false,
                    radius: 0.25,
                    opacity: 1.0,
                    points: vec![[0.5, 0.5, 1.0]],
                },
                BrushStroke {
                    erase: true,
                    radius: 0.07,
                    opacity: 1.0,
                    points: vec![[0.5, 0.5, 1.0]],
                },
            ],
        });
        let mask = selection_mask(&input, SIZE, SIZE, &op);
        let output = rendered(&input, &op);
        for ((before, after), weight) in input.chunks_exact(3).zip(output.chunks_exact(3)).zip(mask)
        {
            if weight == 0.0 {
                assert_eq!(before, after, "{tool:?}");
            }
        }
        assert_eq!(
            output,
            rendered(&input, &op),
            "repeat is deterministic: {tool:?}"
        );
    }
}

#[test]
fn strength_moves_smoothly_toward_the_full_correction() {
    let input = aura_render::fixtures::detail_frame(SIZE as u32, SIZE as u32).rgb;
    for tool in TOOLS {
        let mut op = edit(tool);
        op.tone = 0.2;
        let full = rendered(&input, &op);
        op.amount = 0.5;
        let half = rendered(&input, &op);
        for ((before, mid), after) in input.iter().zip(half).zip(full) {
            assert!(
                mid >= before.min(after) - 1e-6 && mid <= before.max(after) + 1e-6,
                "{tool:?}"
            );
        }
    }
}
