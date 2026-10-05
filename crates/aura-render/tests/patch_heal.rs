// Tests assert by unwrapping; a panic here is a failed test, never a photographer's crash.
#![allow(clippy::unwrap_used, clippy::disallowed_methods)]

use aura_recipe::retouch_tools::{self, BrushMask, BrushStroke, Edit, Tool};
use aura_render::retouch_tools::apply;

const W: usize = 128;

fn operation() -> Edit {
    Edit {
        id: "patch".into(),
        tool: Tool::PatchHeal,
        enabled: true,
        region: [64.5 / 128.0, 64.5 / 128.0, 7.0 / 128.0, 7.0 / 128.0],
        source: None,
        amount: 1.0,
        feather: 0.2,
        radius: 0.01,
        source_scale: 1.0,
        preserve_microtexture: false,
        texture_heal: false,
        texture: 1.0,
        tone: 0.5,
        warmth: 0.0,
        tint: 0.0,
        selection: None,
        matte: None,
        mask: None,
        skin: None,
    }
}
fn surface() -> Vec<f32> {
    (0..W * W)
        .flat_map(|i| {
            let x = (i % W) as f32;
            let y = (i / W) as f32;
            [
                0.25 + x * 0.001 + y * 0.0004,
                0.2 + x * 0.0008,
                0.15 + y * 0.0007,
            ]
        })
        .collect()
}
fn blemish(rgb: &mut [f32]) {
    for y in 62..=66 {
        for x in 62..=66 {
            for c in 0..3 {
                rgb[(y * W + x) * 3 + c] *= 0.4;
            }
        }
    }
}

#[test]
fn texture_heal_preserves_lighting_and_real_pores_across_skin_tones() {
    for exposure in [0.35, 1.0, 1.8] {
        let clean: Vec<_> = surface().into_iter().map(|v| v * exposure).collect();
        let mut damaged = clean.clone();
        // A real donor pattern with a different low-frequency colour and light.
        for y in 50..80 {
            for x in 18..46 {
                for c in 0..3 {
                    damaged[(y * W + x) * 3 + c] +=
                        (0.03 + (x as f32 - 32.0) * 0.002 + if x % 2 == 0 { 0.01 } else { -0.01 })
                            * exposure;
                }
            }
        }
        blemish(&mut damaged);
        let before = damaged.clone();
        let mut edit = operation();
        edit.texture_heal = true;
        edit.source = Some([32.5 / W as f32, 64.5 / W as f32]);
        let serialized = serde_json::to_string(&edit).unwrap();
        let saved: Edit = serde_json::from_str(&serialized).unwrap();
        apply(&mut damaged, W, W, std::slice::from_ref(&saved));
        let mut replay = before.clone();
        apply(&mut replay, W, W, &[saved]);
        assert_eq!(damaged, replay, "saved heals must reproduce exactly");
        let mean_error = (61..68)
            .flat_map(|y| (61..68).map(move |x| (y * W + x) * 3))
            .map(|i| damaged[i] - clean[i])
            .sum::<f32>()
            .abs()
            / 49.0;
        assert!(mean_error < 0.004 * exposure, "colour drift: {mean_error}");
        let detail = (61..67)
            .map(|x| (damaged[(64 * W + x + 1) * 3] - damaged[(64 * W + x) * 3]).abs())
            .sum::<f32>()
            / 6.0;
        assert!(detail > 0.01 * exposure, "donor pores lost: {detail}");
        assert!((damaged[(64 * W + 64) * 3] - clean[(64 * W + 64) * 3]).abs() < 0.02 * exposure);
        for y in 0..W {
            for x in 0..W {
                if (x as f32 - 64.0).hypot(y as f32 - 64.0) >= 7.0 {
                    let i = (y * W + x) * 3;
                    assert_eq!(&damaged[i..i + 3], &before[i..i + 3]);
                }
            }
        }
    }
}

#[test]
fn texture_filter_never_borrows_a_defect_outside_its_clean_source() {
    let mut clean = surface();
    blemish(&mut clean);
    let mut dirty = clean.clone();
    // Source spans x=25..39. A black stripe just outside must not enter
    // the low band and create a bright halo inside the repaired area.
    for y in 48..80 {
        for x in 40..44 {
            dirty[(y * W + x) * 3..(y * W + x) * 3 + 3].fill(0.0);
        }
    }
    let mut edit = operation();
    edit.source = Some([32.5 / W as f32, 64.5 / W as f32]);
    edit.texture_heal = true;
    apply(&mut clean, W, W, std::slice::from_ref(&edit));
    apply(&mut dirty, W, W, &[edit]);
    for y in 58..71 {
        for x in 58..71 {
            let i = (y * W + x) * 3;
            assert_eq!(&clean[i..i + 3], &dirty[i..i + 3]);
        }
    }
}

#[test]
fn small_clean_source_repairs_large_target_without_copying_nearby_defect() {
    let clean = surface();
    let mut damaged = clean.clone();
    blemish(&mut damaged);
    // This defect lies inside a full-size donor, outside its smaller clean centre.
    for y in 62..=66 {
        for x in 36..=38 {
            for c in 0..3 {
                damaged[(y * W + x) * 3 + c] *= 0.1;
            }
        }
    }
    let mut edit = operation();
    edit.source = Some([32.5 / W as f32, 64.5 / W as f32]);
    edit.source_scale = 0.25;
    retouch_tools::validate(&[edit.clone()]).unwrap();
    let before = damaged.clone();
    apply(&mut damaged, W, W, &[edit]);
    for y in 61..=67 {
        for x in 61..=67 {
            for c in 0..3 {
                let i = (y * W + x) * 3 + c;
                assert!((damaged[i] - clean[i]).abs() < 0.005);
            }
        }
    }
    for y in 0..W {
        for x in 0..W {
            if (x as f32 - 64.0).hypot(y as f32 - 64.0) >= 7.0 {
                let i = (y * W + x) * 3;
                assert_eq!(&damaged[i..i + 3], &before[i..i + 3]);
            }
        }
    }
}

#[test]
fn legacy_donor_defaults_to_original_scale_and_invalid_scales_are_rejected() {
    let original = operation();
    let mut json = serde_json::to_value(&original).unwrap();
    json.as_object_mut().unwrap().remove("sourceScale");
    let parsed: Edit = serde_json::from_value(json).unwrap();
    assert_eq!(parsed, original);
    for scale in [0.0, 0.19, 1.1, f32::NAN] {
        let mut invalid = original.clone();
        invalid.source_scale = scale;
        assert!(retouch_tools::validate(&[invalid]).is_err());
    }
    let mut no_source = original;
    no_source.source_scale = 0.5;
    assert!(retouch_tools::validate(&[no_source]).is_err());
}

#[test]
fn small_donors_preserve_fine_texture_instead_of_enlarging_pores() {
    let mut rgb: Vec<f32> = (0..W * W)
        .flat_map(|i| {
            let pore = if i % 2 == 0 { 0.02 } else { -0.02 };
            [0.4 + pore, 0.3 + pore, 0.2 + pore]
        })
        .collect();
    blemish(&mut rgb);
    let mut edit = operation();
    edit.source = Some([32.5 / W as f32, 64.5 / W as f32]);
    edit.source_scale = 0.25;
    apply(&mut rgb, W, W, &[edit]);
    let detail = (61..67)
        .map(|x| (rgb[(64 * W + x + 1) * 3] - rgb[(64 * W + x) * 3]).abs())
        .sum::<f32>()
        / 6.0;
    assert!(detail > 0.012, "fine donor texture was blurred: {detail}");
}

#[test]
fn automatic_patch_removes_an_isolated_spot_and_matches_sloping_light() {
    let clean = surface();
    let mut damaged = clean.clone();
    blemish(&mut damaged);
    let original = damaged.clone();
    apply(&mut damaged, W, W, &[operation()]);
    for y in 62..=66 {
        for x in 62..=66 {
            for c in 0..3 {
                let i = (y * W + x) * 3 + c;
                assert!(
                    (damaged[i] - clean[i]).abs() < 0.001,
                    "repair at {x},{y}: {} vs {}",
                    damaged[i],
                    clean[i]
                );
            }
        }
    }
    for y in 0..W {
        for x in 0..W {
            if (x as f32 - 64.0).hypot(y as f32 - 64.0) >= 7.0 {
                let i = (y * W + x) * 3;
                assert_eq!(&damaged[i..i + 3], &original[i..i + 3]);
            }
        }
    }
    let mut repeat = original;
    apply(&mut repeat, W, W, &[operation()]);
    assert_eq!(damaged, repeat);
}

#[test]
fn manual_patch_transfers_fine_texture_without_copying_the_donors_brightness() {
    let clean = surface();
    let mut pixels = clean.clone();
    for y in 48..81 {
        for x in 80..113 {
            let detail = if (x + y) % 2 == 0 { 0.012 } else { -0.012 };
            for c in 0..3 {
                pixels[(y * W + x) * 3 + c] += 0.2 + detail;
            }
        }
    }
    blemish(&mut pixels);
    let mut op = operation();
    op.source = Some([96.5 / 128.0, 64.5 / 128.0]);
    apply(&mut pixels, W, W, &[op]);
    let mut tone_error = 0.0;
    for y in 62..=66 {
        for x in 62..=66 {
            let detail = if (x + y) % 2 == 0 { 0.012 } else { -0.012 };
            let i = (y * W + x) * 3;
            tone_error += pixels[i] - clean[i] - detail;
            // Separate high-frequency contrast from the smooth boundary-dependent offset.
            if x < 66 {
                let contrast = (pixels[i] - pixels[i + 3]) - (clean[i] - clean[i + 3]);
                assert!(
                    (contrast - 2.0 * detail).abs() < 0.002,
                    "fine texture lost: {contrast}"
                );
            }
        }
    }
    // Removes over 97% of the donor's 0.2 brightness offset. Boundary texture
    // can introduce a small DC shift; this is an approximate harmonic blend.
    assert!(
        (tone_error / 25.0).abs() < 0.006,
        "donor brightness leaked into target"
    );
}

#[test]
fn overlapping_source_is_independent_of_scan_direction() {
    let mut original = surface();
    for y in 50..79 {
        for x in 50..79 {
            for c in 0..3 {
                original[(y * W + x) * 3 + c] += ((x * 7 + y * 11) as f32).sin() * 0.03;
            }
        }
    }
    let reflect = |rgb: &[f32]| -> Vec<f32> {
        (0..W * W)
            .flat_map(|i| {
                let j = (i / W * W + W - 1 - i % W) * 3;
                [rgb[j], rgb[j + 1], rgb[j + 2]]
            })
            .collect()
    };
    let mut mirrored = reflect(&original);
    let mut op = operation();
    op.source = Some([68.5 / 128.0, 64.5 / 128.0]);
    apply(&mut original, W, W, &[op.clone()]);
    op.region[0] = 1.0 - op.region[0];
    op.source.as_mut().unwrap()[0] = 1.0 - op.source.unwrap()[0];
    apply(&mut mirrored, W, W, &[op]);
    for (a, b) in reflect(&original).iter().zip(&mirrored) {
        assert!(
            (a - b).abs() < 0.00001,
            "repair depends on traversal direction"
        );
    }
}

#[test]
fn neutral_and_edge_repairs_are_safe_and_leave_unavailable_donors_unchanged() {
    let original = surface();
    for mode in 0..3 {
        let mut op = operation();
        match mode {
            0 => op.amount = 0.0,
            1 => op.enabled = false,
            _ => op.source = Some([op.region[0], op.region[1]]),
        }
        let mut pixels = original.clone();
        apply(&mut pixels, W, W, &[op]);
        assert_eq!(pixels, original);
    }
    let mut op = operation();
    op.source = Some([0.0, 0.0]);
    let mut pixels = original.clone();
    apply(&mut pixels, W, W, &[op.clone()]);
    assert!(pixels.iter().all(|v| v.is_finite() && *v >= 0.0));
    let i = (62 * W + 62) * 3;
    assert_eq!(&pixels[i..i + 3], &original[i..i + 3]);
    let mut tiny = vec![0.2, 0.3, 0.4];
    apply(&mut tiny, 1, 1, &[op]);
    assert_eq!(tiny, vec![0.2, 0.3, 0.4]);
}

#[test]
fn painted_and_large_repairs_require_a_source_and_round_trip() {
    let mut op = operation();
    assert!(retouch_tools::validate(&[op.clone()]).is_ok());
    op.region[2] = 0.1001;
    assert!(retouch_tools::validate(&[op.clone()]).is_err());
    op.region[2] = 0.05;
    op.mask = Some(BrushMask {
        strokes: vec![BrushStroke {
            erase: false,
            radius: 0.02,
            opacity: 1.0,
            points: vec![[0.5, 0.5, 1.0]],
        }],
    });
    assert!(retouch_tools::validate(&[op.clone()]).is_err());
    op.source = Some([0.4, 0.4]);
    assert!(retouch_tools::validate(&[op.clone()]).is_ok());
    assert_eq!(
        serde_json::from_str::<Edit>(&serde_json::to_string(&op).unwrap()).unwrap(),
        op
    );
}
