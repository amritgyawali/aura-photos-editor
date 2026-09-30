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
        texture: 1.0,
        tone: 0.5,
        warmth: 0.0,
        tint: 0.0,
        selection: None,
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
