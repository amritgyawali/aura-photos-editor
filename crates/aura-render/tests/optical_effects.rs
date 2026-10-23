//! Pixel-level acceptance for manually selected optical and colour effects.
#![allow(clippy::unwrap_used, clippy::disallowed_methods)]
use aura_recipe::retouch_tools::{self, BrushMask, BrushStroke, Edit, Tool};
use aura_render::retouch_tools::{apply, selection_mask};

const TOOLS: [Tool; 4] = [
    Tool::MotionBlur,
    Tool::ZoomBlur,
    Tool::ColorDespill,
    Tool::HighlightCompress,
];

fn edit(tool: Tool) -> Edit {
    serde_json::from_value(serde_json::json!({
        "id":"optics", "tool":tool, "enabled":true, "region":[0.5,0.5,1,1],
        "source":[0.5,0.5], "amount":1.0, "feather":0.0, "radius":0.05,
        "texture":1.0, "tone":0.2, "warmth":0.0, "tint":0.0,
        "targetColor":[0.0,1.0,0.0]
    }))
    .unwrap()
}

fn frame(w: usize, h: usize, f: impl Fn(usize, usize) -> [f32; 3]) -> Vec<f32> {
    (0..w * h).flat_map(|i| f(i % w, i / w)).collect()
}

fn render(input: &[f32], w: usize, h: usize, op: &Edit) -> Vec<f32> {
    let mut output = input.to_vec();
    apply(&mut output, w, h, std::slice::from_ref(op));
    output
}

fn luma(p: &[f32]) -> f32 {
    p[0] * 0.2627 + p[1] * 0.678 + p[2] * 0.0593
}

#[test]
fn motion_direction_changes_the_axis_of_the_trail() {
    let (w, h) = (161, 161);
    let input = frame(w, h, |x, y| {
        if x == 80 && y == 80 {
            [1.0; 3]
        } else {
            [0.0; 3]
        }
    });
    let mut op = edit(Tool::MotionBlur);
    let horizontal = render(&input, w, h, &op);
    op.warmth = 0.5;
    let vertical = render(&input, w, h, &op);
    assert!(horizontal[(80 * w + 83) * 3] > 0.05);
    assert!(vertical[(83 * w + 80) * 3] > 0.05);
    assert!(horizontal[(83 * w + 80) * 3] < 1e-6);
    assert!(vertical[(80 * w + 83) * 3] < 1e-6);
    for y in 0..h {
        for x in 0..w {
            assert!((horizontal[(y * w + x) * 3] - vertical[(x * w + y) * 3]).abs() < 1e-5);
        }
    }
    let energy = horizontal.chunks_exact(3).map(|p| p[0]).sum::<f32>();
    assert!((energy - 1.0).abs() < 0.025, "line kernel energy: {energy}");
}

#[test]
fn zoom_centre_is_fixed_and_moving_it_changes_trail_direction() {
    let (w, h) = (97, 97);
    let input = frame(w, h, |x, y| {
        if x == 48 && y == 48 {
            [1.0; 3]
        } else {
            [0.0; 3]
        }
    });
    let mut op = edit(Tool::ZoomBlur);
    let centered = render(&input, w, h, &op);
    assert_eq!(
        &centered[(48 * w + 48) * 3..(48 * w + 48) * 3 + 3],
        &[1.0; 3]
    );
    op.source = Some([0.0, 0.5]);
    let off_center = render(&input, w, h, &op);
    assert!(off_center[(48 * w + 53) * 3] > 0.01);
    assert!(off_center[(53 * w + 48) * 3] < 1e-6);
    assert!(off_center[(48 * w + 48) * 3] < 0.5);
}

#[test]
fn long_zoom_trails_prefilter_high_frequency_texture() {
    let (w, h) = (1024, 64);
    let input = frame(w, h, |x, _| [if x % 2 == 0 { 1.0 } else { 0.0 }; 3]);
    let mut op = edit(Tool::ZoomBlur);
    op.source = Some([0.0, 0.5]);
    let output = render(&input, w, h, &op);
    for x in 600..850 {
        assert!(
            (output[(32 * w + x) * 3] - 0.5).abs() < 0.025,
            "sparse trail at {x}"
        );
    }
}

#[test]
fn optical_blurs_do_not_sample_excluded_subject_colour_even_at_pyramid_levels() {
    let (w, h) = (512, 64);
    let input = frame(w, h, |x, _| {
        if x < 256 {
            [3.0, 0.0, 0.0]
        } else {
            [0.0, 0.0, 0.5]
        }
    });
    for tool in [Tool::MotionBlur, Tool::ZoomBlur] {
        let mut op = edit(tool);
        op.region = [0.75, 0.5, 0.25, 1.0];
        op.source = Some([0.0, 0.5]);
        let output = render(&input, w, h, &op);
        let coverage = selection_mask(&input, w, h, &op);
        for ((before, after), alpha) in input
            .chunks_exact(3)
            .zip(output.chunks_exact(3))
            .zip(coverage)
        {
            if alpha == 0.0 {
                assert_eq!(before, after);
            } else {
                assert_eq!(after[0], 0.0);
                assert_eq!(after[1], 0.0);
                assert!((after[2] - 0.5).abs() < 1e-5);
            }
        }
    }
}

#[test]
fn constant_colours_remain_constant_through_soft_masks_and_frame_edges() {
    let (w, h) = (65, 33);
    let input = frame(w, h, |_, _| [0.14, 0.25, 0.31]);
    for tool in [Tool::MotionBlur, Tool::ZoomBlur] {
        let mut op = edit(tool);
        op.feather = 1.0;
        op.region = [0.2, 0.5, 0.5, 0.6];
        op.warmth = 0.27;
        let output = render(&input, w, h, &op);
        assert!(input.iter().zip(output).all(|(a, b)| (*a - b).abs() < 1e-6));
    }
}

#[test]
fn despill_removes_target_chroma_preserves_luminance_and_leaves_opposite_hues() {
    let green = aura_render::output::display_srgb_to_working([0.0, 1.0, 0.0]);
    let neutral = luma(&green);
    let opposite = green.map(|v| neutral * 2.0 - v);
    let input = frame(8, 8, |x, _| if x < 4 { green } else { opposite });
    let op = edit(Tool::ColorDespill);
    let output = render(&input, 8, 8, &op);
    for (i, (before, after)) in input
        .chunks_exact(3)
        .zip(output.chunks_exact(3))
        .enumerate()
    {
        assert!((luma(before) - luma(after)).abs() < 1e-6);
        if i % 8 < 4 {
            assert!(after.iter().all(|v| (*v - neutral).abs() < 1e-5));
        } else {
            assert_eq!(before, after);
        }
    }
    let grays = frame(8, 8, |x, _| [x as f32 / 8.0; 3]);
    assert_eq!(render(&grays, 8, 8, &op), grays);
}

#[test]
fn despill_produces_no_negative_channels_from_nonnegative_inputs() {
    let input = frame(64, 64, |x, y| {
        [
            x as f32 / 63.0,
            y as f32 / 63.0,
            ((x * 19 + y * 7) % 64) as f32 / 63.0,
        ]
    });
    for color in [[0.0, 1.0, 0.0], [1.0, 0.0, 1.0], [0.0, 0.1, 1.0]] {
        let mut op = edit(Tool::ColorDespill);
        op.target_color = Some(color);
        op.tone = 0.0;
        let output = render(&input, 64, 64, &op);
        for (before, after) in input.chunks_exact(3).zip(output.chunks_exact(3)) {
            assert!(after.iter().all(|v| v.is_finite() && *v >= -1e-6));
            assert!((luma(before) - luma(after)).abs() < 1e-6);
        }
    }
}

#[test]
fn highlight_shoulder_is_monotonic_preserves_ratios_and_leaves_lower_tones_exact() {
    let input = frame(256, 1, |x, _| {
        let light = x as f32 / 16.0;
        [light, light * 0.7, light * 0.4]
    });
    let op = edit(Tool::HighlightCompress);
    let output = render(&input, 256, 1, &op);
    let threshold = 0.18 * (op.tone * 4.0).exp2();
    let mut previous = -1.0;
    for (before, after) in input.chunks_exact(3).zip(output.chunks_exact(3)) {
        let light = luma(after);
        assert!(light > previous);
        previous = light;
        if luma(before) <= threshold {
            assert_eq!(before, after);
        } else {
            assert!(light < luma(before));
            assert!((after[1] / after[0] - 0.7).abs() < 1e-6);
            assert!((after[2] / after[0] - 0.4).abs() < 1e-6);
        }
    }
    let mut off = op;
    off.texture = 0.0;
    assert_eq!(render(&input, 256, 1, &off), input);
}

#[test]
fn all_new_tools_round_trip_and_invalid_spill_colour_is_rejected() {
    for tool in TOOLS {
        let op = edit(tool);
        retouch_tools::validate(std::slice::from_ref(&op)).unwrap();
        assert_eq!(
            serde_json::from_str::<Edit>(&serde_json::to_string(&op).unwrap()).unwrap(),
            op
        );
    }
    let mut op = edit(Tool::ColorDespill);
    for color in [None, Some([0.0; 3]), Some([0.5; 3]), Some([1.0; 3])] {
        op.target_color = color;
        assert!(retouch_tools::validate(std::slice::from_ref(&op)).is_err());
    }
}

#[test]
fn all_new_tools_respect_erasure_zero_strength_and_disabled_state() {
    let input = aura_render::fixtures::detail_frame(96, 96).rgb;
    for tool in TOOLS {
        let mut op = edit(tool);
        op.amount = 0.0;
        assert_eq!(render(&input, 96, 96, &op), input);
        op.amount = 1.0;
        op.enabled = false;
        assert_eq!(render(&input, 96, 96, &op), input);
        op.enabled = true;
        op.mask = Some(BrushMask {
            strokes: vec![
                BrushStroke {
                    erase: false,
                    radius: 0.35,
                    opacity: 0.4,
                    points: vec![[0.5, 0.5, 1.0]],
                },
                BrushStroke {
                    erase: true,
                    radius: 0.1,
                    opacity: 1.0,
                    points: vec![[0.5, 0.5, 1.0]],
                },
            ],
        });
        let mask = selection_mask(&input, 96, 96, &op);
        let output = render(&input, 96, 96, &op);
        for ((before, after), alpha) in input.chunks_exact(3).zip(output.chunks_exact(3)).zip(mask)
        {
            if alpha == 0.0 {
                assert_eq!(before, after, "{tool:?}");
            }
        }
        assert_eq!(output, render(&input, 96, 96, &op));
    }
}
