use super::*;

const TOOLS: [Tool; 25] = [
    Tool::AdaptiveMedianImpulse,
    Tool::SigmaRangeMean,
    Tool::SpatialWeightedMedian,
    Tool::LuminanceWinsor,
    Tool::ChromaBilateral,
    Tool::ChromaCovariance,
    Tool::RgbCovariance,
    Tool::EntropyDetail,
    Tool::RmsContrast,
    Tool::LogRetinex,
    Tool::PeriodicNotch,
    Tool::CrossMedian,
    Tool::HessianRidge,
    Tool::CoherenceDetail,
    Tool::DirectionalMedian,
    Tool::QuadraticResidual,
    Tool::LanczosShift,
    Tool::EllipticalAperture,
    Tool::HexagonalAperture,
    Tool::CatEyeAperture,
    Tool::HighlightStreak,
    Tool::BilateralToneCompress,
    Tool::DarkChannelHaze,
    Tool::ConnectedImpulse,
    Tool::ChromaMedian,
];
fn operation(tool: Tool) -> Operation {
    Operation {
        id: "advanced".into(),
        mask_id: "selection".into(),
        tool,
        enabled: true,
        amount: 0.75,
        radius: 0.13,
        threshold: 0.09,
        detail: 0.7,
        angle: 31.0,
        source: [0.3, 0.4],
        shift: [0.035, -0.04],
        path: vec![[-0.01, 0.0], [0.01, 0.0]],
    }
}
fn chart() -> Grid {
    Grid {
        pixels: (0..768)
            .map(|i| {
                let x = (i % 32) as f32;
                let y = (i / 32) as f32;
                let impulse = if i % 17 == 0 { 0.28 } else { 0.0 };
                [
                    0.2 + x * 0.007 + (y * 1.2).sin() * 0.03 + impulse,
                    0.25 + y * 0.004 + (x * 0.6).cos() * 0.015 + impulse,
                    0.16 + x * 0.003 + (y * 0.8).cos() * 0.03 + impulse,
                ]
            })
            .collect(),
        weight: vec![1.0; 768],
        width: 32,
        height: 24,
        centre: [0.5; 2],
    }
}
#[test]
fn added_methods_roundtrip_and_preserve_zero_and_empty_selection() {
    let grid = chart();
    let input: Vec<_> = grid.pixels.iter().flatten().copied().collect();
    for tool in TOOLS {
        let mut op = operation(tool);
        let mut recipe = aura_recipe::fixtures::reference();
        spatial_lab::write(&mut recipe, std::slice::from_ref(&op)).unwrap();
        assert_eq!(spatial_lab::read(&recipe).unwrap(), vec![op.clone()]);
        let mut output = input.clone();
        op.amount = 0.0;
        process(&mut output, 32, 24, &grid.weight, &op);
        assert_eq!(output, input, "{tool:?} zero");
        op.amount = 0.75;
        process(&mut output, 32, 24, &[0.0; 768], &op);
        assert_eq!(output, input, "{tool:?} empty");
        let a = evaluate(&grid, &op);
        let b = evaluate(&grid, &op);
        assert_eq!(a, b, "{tool:?} deterministic");
        assert!(
            a.iter().flatten().all(|v| v.is_finite() && *v >= 0.0),
            "{tool:?} finite"
        );
        assert!(
            a.iter()
                .zip(&grid.pixels)
                .any(|(a, b)| distance(*a, *b) > 1e-12),
            "{tool:?} inactive"
        );
    }
}
#[test]
fn all_added_methods_ignore_excluded_neighbours_and_keep_excluded_pixels_exact() {
    let mut a = chart();
    a.weight = (0..768)
        .map(|i| if i % 32 < 18 && i / 32 > 3 { 0.6 } else { 0.0 })
        .collect();
    let mut b = Grid {
        pixels: a.pixels.clone(),
        weight: a.weight.clone(),
        width: a.width,
        height: a.height,
        centre: a.centre,
    };
    for (p, w) in b.pixels.iter_mut().zip(&b.weight) {
        if *w == 0.0 {
            *p = [14.0, 0.01, 3.0];
        }
    }
    for tool in TOOLS {
        let out_a = evaluate(&a, &operation(tool));
        let out_b = evaluate(&b, &operation(tool));
        for i in 0..768 {
            if a.weight[i] > 0.0 {
                assert_eq!(
                    out_a[i].map(f32::to_bits),
                    out_b[i].map(f32::to_bits),
                    "{tool:?} leaked {i}"
                );
            } else {
                assert_eq!(
                    out_a[i].map(f32::to_bits),
                    a.pixels[i].map(f32::to_bits),
                    "{tool:?} outside {i}"
                );
            }
        }
        let mut full: Vec<_> = a.pixels.iter().flatten().copied().collect();
        process(&mut full, 32, 24, &a.weight, &operation(tool));
        for i in 0..768 {
            if a.weight[i] == 0.0 {
                assert_eq!(&full[i * 3..i * 3 + 3], &a.pixels[i], "{tool:?}");
            }
        }
    }
}
#[test]
fn added_methods_handle_single_pixel_hdr_and_uniform_fractional_coverage() {
    let grid = chart();
    let input: Vec<_> = grid.pixels.iter().flatten().copied().collect();
    for tool in TOOLS {
        for value in [0.0, 1e-7, 0.3, 8.0, 1000.0] {
            let mut p = vec![value; 3];
            process(&mut p, 1, 1, &[1.0], &operation(tool));
            assert!(p.iter().all(|v| v.is_finite() && *v >= 0.0), "{tool:?}");
        }
        let mut full = input.clone();
        let mut quarter = input.clone();
        process(&mut full, 32, 24, &[1.0; 768], &operation(tool));
        process(&mut quarter, 32, 24, &[0.25; 768], &operation(tool));
        for ((a, b), p) in full.iter().zip(&quarter).zip(&input) {
            assert!((b - p - (a - p) * 0.25).abs() < 2e-5, "{tool:?} coverage");
        }
    }
}
#[test]
fn robust_repairs_remove_isolated_spots_without_erasing_large_components() {
    let mut grid = Grid {
        pixels: vec![[0.3; 3]; 81],
        weight: vec![1.0; 81],
        width: 9,
        height: 9,
        centre: [0.5; 2],
    };
    grid.pixels[40] = [0.95; 3];
    for tool in [
        Tool::AdaptiveMedianImpulse,
        Tool::SpatialWeightedMedian,
        Tool::LuminanceWinsor,
        Tool::CrossMedian,
        Tool::DirectionalMedian,
        Tool::ConnectedImpulse,
    ] {
        let output = evaluate(&grid, &operation(tool));
        assert!((output[40][0] - 0.3).abs() < 1e-5, "{tool:?}");
    }
    for y in 2..7 {
        for x in 2..7 {
            grid.pixels[y * 9 + x] = [0.95; 3];
        }
    }
    assert_eq!(
        evaluate(&grid, &operation(Tool::ConnectedImpulse))[40].map(f32::to_bits),
        grid.pixels[40].map(f32::to_bits)
    );
}
#[test]
fn chroma_estimators_preserve_luminance_and_covariance_obeys_noise_floor() {
    let grid = chart();
    for tool in [
        Tool::ChromaBilateral,
        Tool::ChromaCovariance,
        Tool::ChromaMedian,
    ] {
        let output = evaluate(&grid, &operation(tool));
        for (p, q) in grid.pixels.iter().zip(output) {
            assert!((luminance(*p) - luminance(q)).abs() < 2e-6, "{tool:?}");
        }
    }
    let mut flat = Grid {
        pixels: vec![[0.3; 3]; 49],
        weight: vec![1.0; 49],
        width: 7,
        height: 7,
        centre: [0.5; 2],
    };
    flat.pixels[24] = [0.31, 0.29, 0.3];
    let out = evaluate(&flat, &operation(Tool::RgbCovariance));
    assert!(distance(out[24], [0.3; 3]) < distance(flat.pixels[24], [0.3; 3]) * 0.02);
}
#[test]
fn quadratic_detail_preserves_smooth_curved_illumination() {
    let grid = Grid {
        pixels: (0..81)
            .map(|i| {
                let x = (i % 9) as f32;
                let y = (i / 9) as f32;
                [0.1 + x * 0.01 + y * 0.003 + x * x * 0.001 + x * y * 0.002 + y * y * 0.0005; 3]
            })
            .collect(),
        weight: vec![1.0; 81],
        width: 9,
        height: 9,
        centre: [0.5; 2],
    };
    let output = evaluate(&grid, &operation(Tool::QuadraticResidual));
    assert!(distance(output[40], grid.pixels[40]) < 1e-10);
}
#[test]
fn fitted_notch_removes_the_authored_periodic_frequency() {
    let mut grid = chart();
    let mut op = operation(Tool::PeriodicNotch);
    op.angle = 0.0;
    op.threshold = 0.2;
    let frequency = op.radius;
    for (i, p) in grid.pixels.iter_mut().enumerate() {
        *p = [0.4 + 0.05 * (2.0 * std::f32::consts::PI * frequency * (i % 32) as f32).cos(); 3];
    }
    let output = evaluate(&grid, &op);
    let before = (3..29)
        .map(|x| (grid.pixels[12 * 32 + x][0] - 0.4).powi(2))
        .sum::<f32>();
    let after = (3..29)
        .map(|x| (output[12 * 32 + x][0] - 0.4).powi(2))
        .sum::<f32>();
    assert!(after < before * 0.1, "before {before}, after {after}");
}

#[test]
fn illumination_and_base_compression_have_predictable_flat_field_response() {
    let grid = Grid {
        pixels: vec![[0.4; 3]; 81],
        weight: vec![1.0; 81],
        width: 9,
        height: 9,
        centre: [0.5; 2],
    };
    let mut op = operation(Tool::LogRetinex);
    op.detail = 1.0;
    op.threshold = 0.2;
    for p in evaluate(&grid, &op) {
        assert!((p[0] - 0.2).abs() < 1e-5);
    }
    op.tool = Tool::BilateralToneCompress;
    op.detail = 0.8;
    for p in evaluate(&grid, &op) {
        assert!((p[0] - 0.4 / 1.32).abs() < 1e-5);
    }
    for tool in [
        Tool::EntropyDetail,
        Tool::CoherenceDetail,
        Tool::HessianRidge,
    ] {
        for p in evaluate(&grid, &operation(tool)) {
            assert!(distance(p, [0.4; 3]) < 1e-12, "{tool:?} flat field");
        }
    }
}

#[test]
fn sigma_gate_keeps_contrast_boundaries_and_accepts_small_variations() {
    let mut grid = Grid {
        pixels: vec![[0.2; 3]; 81],
        weight: vec![1.0; 81],
        width: 9,
        height: 9,
        centre: [0.5; 2],
    };
    for y in 0..9 {
        for x in 5..9 {
            grid.pixels[y * 9 + x] = [0.8; 3];
        }
    }
    let mut op = operation(Tool::SigmaRangeMean);
    op.threshold = 0.05;
    let output = evaluate(&grid, &op);
    assert!(distance(output[40], [0.2; 3]) < 1e-12);
    grid.pixels[40] = [0.22; 3];
    let output = evaluate(&grid, &op);
    assert!(output[40][0] > 0.2 && output[40][0] < 0.205);
}

#[test]
fn optical_integrators_keep_flat_fields_and_streaks_obey_the_highlight_gate() {
    let mut grid = Grid {
        pixels: vec![[0.3; 3]; 225],
        weight: vec![1.0; 225],
        width: 15,
        height: 15,
        centre: [0.5; 2],
    };
    for tool in [
        Tool::EllipticalAperture,
        Tool::HexagonalAperture,
        Tool::CatEyeAperture,
    ] {
        for p in evaluate(&grid, &operation(tool)) {
            assert!(distance(p, [0.3; 3]) < 1e-12, "{tool:?}");
        }
    }
    let mut op = operation(Tool::LanczosShift);
    op.shift = [0.0; 2];
    grid.pixels[112] = [0.7, 0.4, 0.1];
    for (p, q) in evaluate(&grid, &op).iter().zip(&grid.pixels) {
        assert!(distance(*p, *q) < 1e-12);
    }
    op.tool = Tool::HighlightStreak;
    op.threshold = 0.95;
    assert_eq!(evaluate(&grid, &op), grid.pixels);
    op.threshold = 0.1;
    op.angle = 0.0;
    let output = evaluate(&grid, &op);
    assert!(output[111][0] > grid.pixels[111][0]);
    assert_eq!(
        output[113].map(f32::to_bits),
        grid.pixels[113].map(f32::to_bits)
    );
}

#[test]
fn selected_dark_channel_inversion_restores_dark_contrast_but_preserves_airlight() {
    let mut grid = Grid {
        pixels: vec![[1.0; 3]; 81],
        weight: vec![1.0; 81],
        width: 9,
        height: 9,
        centre: [0.5; 2],
    };
    for y in 2..7 {
        for x in 2..7 {
            grid.pixels[y * 9 + x] = [0.7; 3];
        }
    }
    grid.pixels[40] = [0.4; 3];
    let output = evaluate(&grid, &operation(Tool::DarkChannelHaze));
    assert!(output[40][0] < 0.1 && output[41][0] < 0.6);
    assert_eq!(output[0].map(f32::to_bits), [1.0_f32.to_bits(); 3]);
}
