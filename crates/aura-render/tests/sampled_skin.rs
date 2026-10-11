// Tests assert by unwrapping; a panic here is a failed test, never a photographer's crash.
#![allow(clippy::unwrap_used, clippy::disallowed_methods)]

use aura_recipe::retouch_tools::{self, Edit, SkinSettings, Tool};
use aura_render::retouch_tools::apply;

const W: usize = 160;
const H: usize = 96;

fn operation(tool: Tool) -> Edit {
    Edit {
        id: "sampled".into(),
        tool,
        enabled: true,
        region: [0.5, 0.5, 1.0, 1.0],
        source: Some([0.1, 0.5]),
        amount: 1.0,
        feather: 0.0,
        radius: 0.025,
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
        texture: 1.0,
        tone: 1.0,
        warmth: 0.0,
        tint: 0.0,
        selection: None,
        matte: None,
        mask: None,
        skin: Some(SkinSettings {
            tolerance: 0.2,
            edge_protection: 0.8,
            connected: false,
        }),
    }
}
fn luma(p: &[f32]) -> f32 {
    p[0] * 0.2627 + p[1] * 0.678 + p[2] * 0.0593
}
fn fixture(brightness: f32) -> Vec<f32> {
    let mut pixels = Vec::new();
    for y in 0..H {
        for x in 0..W {
            let color = if x >= 130 {
                [0.15, 0.4, 1.3]
            } else {
                [1.3, 0.9, 0.7]
            };
            let patch = if (45..110).contains(&x) {
                0.12 * (x as f32 * 0.25).sin()
            } else {
                0.0
            };
            let detail = if (x + y) % 2 == 0 { 0.015 } else { -0.015 };
            pixels.extend(color.map(|c| c * brightness * (1.0 + patch + detail)));
        }
    }
    pixels
}

#[test]
fn skin_parameters_require_a_sample_and_round_trip_without_changing_old_edits() {
    let mut op = operation(Tool::SkinSmooth);
    assert!(retouch_tools::validate(&[op.clone()]).is_ok());
    let encoded = serde_json::to_string(&op).unwrap();
    assert_eq!(serde_json::from_str::<Edit>(&encoded).unwrap(), op);
    op.source = None;
    assert!(retouch_tools::validate(&[op.clone()]).is_err());
    op.source = Some([1.0, 1.0]);
    op.skin.as_mut().unwrap().tolerance = f32::NAN;
    assert!(retouch_tools::validate(&[op.clone()]).is_err());
    op.skin = Some(SkinSettings {
        tolerance: 0.08,
        edge_protection: 1.01,
        connected: false,
    });
    assert!(retouch_tools::validate(&[op]).is_err());
}

#[test]
fn all_sampled_tools_protect_unmatched_colors_and_are_exposure_equivariant() {
    for tool in [
        Tool::SkinSmooth,
        Tool::SkinUniformity,
        Tool::PortraitDodgeBurn,
    ] {
        let reference = fixture(0.5);
        let mut reference_out = reference.clone();
        apply(&mut reference_out, W, H, &[operation(tool)]);
        for brightness in [0.025, 0.1, 0.8] {
            let original = fixture(brightness);
            let mut out = original.clone();
            apply(&mut out, W, H, &[operation(tool)]);
            assert!(out.iter().all(|p| p.is_finite() && *p >= 0.0));
            for y in 0..H {
                for x in 130..W {
                    let i = (y * W + x) * 3;
                    assert_eq!(
                        &out[i..i + 3],
                        &original[i..i + 3],
                        "unmatched background: {tool:?}"
                    );
                }
            }
            for (a, b) in out.iter().zip(&reference_out) {
                assert!(
                    (a / brightness - b / 0.5).abs() < 0.0002,
                    "exposure bias {tool:?}"
                );
            }
        }
    }
}

#[test]
fn smoothing_reduces_blotches_while_preserving_fine_detail() {
    let original = fixture(0.3);
    let mut out = original.clone();
    let mut op = operation(Tool::SkinSmooth);
    op.skin.as_mut().unwrap().edge_protection = 0.4;
    apply(&mut out, W, H, &[op]);
    let mut before_mid = 0.0;
    let mut after_mid = 0.0;
    let mut before_high = 0.0;
    let mut after_high = 0.0;
    for x in 55..100 {
        let i = (48 * W + x) * 3;
        let j = (49 * W + x) * 3;
        before_mid += (0.5 * (luma(&original[i..i + 3]) + luma(&original[j..j + 3]))
            - luma(&[0.39, 0.27, 0.21]))
        .powi(2);
        after_mid += (0.5 * (luma(&out[i..i + 3]) + luma(&out[j..j + 3]))
            - luma(&[0.39, 0.27, 0.21]))
        .powi(2);
        before_high += (luma(&original[i..i + 3]) - luma(&original[j..j + 3])).abs();
        after_high += (luma(&out[i..i + 3]) - luma(&out[j..j + 3])).abs();
    }
    assert!(
        after_mid < before_mid * 0.7,
        "mid energy: {before_mid} -> {after_mid}"
    );
    assert!(
        (after_high / before_high - 1.0).abs() < 0.06,
        "detail retention {}",
        after_high / before_high
    );
}

#[test]
fn even_tone_reduces_a_color_patch_without_changing_luminance() {
    let mut original = fixture(0.3);
    for y in 25..70 {
        for x in 50..100 {
            let i = (y * W + x) * 3;
            original[i] += 0.045;
            original[i + 1] -= 0.045 * 0.2627 / 0.678;
        }
    }
    let mut out = original.clone();
    apply(&mut out, W, H, &[operation(Tool::SkinUniformity)]);
    let center = (48 * W + 75) * 3;
    assert!(out[center] < original[center] - 0.015);
    for (a, b) in original.chunks_exact(3).zip(out.chunks_exact(3)) {
        assert!((luma(a) - luma(b)).abs() < 1e-6);
    }
}

#[test]
fn dodge_burn_preserves_chroma_and_bounds_exposure() {
    let original = fixture(0.2);
    let mut out = original.clone();
    apply(&mut out, W, H, &[operation(Tool::PortraitDodgeBurn)]);
    assert!(out.iter().zip(&original).any(|(a, b)| (a - b).abs() > 1e-4));
    for (a, b) in original.chunks_exact(3).zip(out.chunks_exact(3)) {
        let gain = b[1] / a[1];
        assert!((gain.log2()).abs() <= 0.50001);
        for c in 0..3 {
            assert!((b[c] / a[c] - gain).abs() < 1e-5);
        }
    }
}

#[test]
fn neutral_black_tiny_and_image_edge_samples_remain_well_defined() {
    for tool in [
        Tool::SkinSmooth,
        Tool::SkinUniformity,
        Tool::PortraitDodgeBurn,
    ] {
        let mut op = operation(tool);
        let original = fixture(0.3);
        let mut out = original.clone();
        op.tone = 0.0;
        apply(&mut out, W, H, &[op.clone()]);
        assert_eq!(out, original);
        op.tone = 1.0;
        op.source = Some([1.0, 1.0]);
        for value in [0.0, 0.1, 2.0] {
            let mut tiny = vec![value; 3];
            apply(&mut tiny, 1, 1, &[op.clone()]);
            assert!(tiny
                .iter()
                .all(|v| v.is_finite() && (*v - value).abs() < 1e-6));
        }
    }
}

#[test]
fn edge_protection_reduces_bleeding_across_a_brightness_boundary() {
    let mut original = Vec::new();
    for _ in 0..H {
        for x in 0..W {
            let level = if x < 80 { 0.1 } else { 0.6 };
            original.extend([1.3 * level, 0.9 * level, 0.7 * level]);
        }
    }
    let mut weak = original.clone();
    let mut protected = original.clone();
    let mut op = operation(Tool::SkinSmooth);
    op.skin.as_mut().unwrap().edge_protection = 0.0;
    apply(&mut weak, W, H, &[op.clone()]);
    op.skin.as_mut().unwrap().edge_protection = 1.0;
    apply(&mut protected, W, H, &[op]);
    let drift = |image: &[f32]| {
        (70..90)
            .map(|x| {
                let i = (48 * W + x) * 3;
                (luma(&image[i..i + 3]) - luma(&original[i..i + 3])).abs()
            })
            .sum::<f32>()
    };
    assert!(
        drift(&protected) < drift(&weak) * 0.5,
        "edge protection: {} vs {}",
        drift(&protected),
        drift(&weak)
    );
}

#[test]
fn connected_skin_never_selects_a_skin_coloured_background_it_does_not_touch() {
    // Skin on the left, a dark gap (hair, clothing or an outline), then a backdrop of the
    // very same colour on the right: colour alone cannot tell them apart.
    let skin = [0.55_f32, 0.36, 0.27];
    let mut rgb = Vec::with_capacity(W * H * 3);
    for _ in 0..H {
        for x in 0..W {
            rgb.extend(if (70..78).contains(&x) {
                [0.02, 0.02, 0.02]
            } else {
                skin
            });
        }
    }
    let mut op = operation(Tool::SkinSmooth);
    let colour_only = aura_render::retouch_tools::selection_mask(&rgb, W, H, &op);
    op.skin = Some(SkinSettings {
        tolerance: 0.2,
        edge_protection: 0.8,
        connected: true,
    });
    let connected = aura_render::retouch_tools::selection_mask(&rgb, W, H, &op);
    let at = |mask: &[f32], x: usize| mask[(H / 2) * W + x];
    assert!(
        at(&colour_only, 120) > 0.9,
        "colour alone selects the backdrop"
    );
    assert!(at(&connected, 20) > 0.9, "the person's skin is selected");
    assert!(
        at(&connected, 120) < 0.01,
        "the backdrop is not: {}",
        at(&connected, 120)
    );
    assert!(at(&connected, 74) < 0.01, "the dark gap is not");
    // And rendering agrees with the preview.
    let mut out = rgb.clone();
    let mut smooth = op.clone();
    smooth.texture = 0.0;
    apply(&mut out, W, H, &[smooth]);
    let i = ((H / 2) * W + 120) * 3;
    assert_eq!(&out[i..i + 3], &rgb[i..i + 3]);
}
