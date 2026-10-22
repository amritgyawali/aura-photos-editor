use super::*;

const TOOLS: [Tool; 25] = [
    Tool::GuidedLinear,
    Tool::DomainTransform,
    Tool::RichardsonLucy,
    Tool::ShockSharpen,
    Tool::CurvatureFlow,
    Tool::OpeningReconstruct,
    Tool::ClosingReconstruct,
    Tool::ToggleMapping,
    Tool::ContraharmonicMean,
    Tool::GeometricMean,
    Tool::HuberLocation,
    Tool::TukeyLocation,
    Tool::LocalPlaneResidual,
    Tool::ChromaGuidedRegression,
    Tool::AnscombeDenoise,
    Tool::SideWindowMean,
    Tool::MedianOfMeans,
    Tool::BilateralSharpen,
    Tool::AiryDiffraction,
    Tool::AnnularBokeh,
    Tool::TiltShiftIntegration,
    Tool::ZoomIntegration,
    Tool::RadialChromatic,
    Tool::PerspectivePlane,
    Tool::ThinPlateWarp,
];
fn operation(tool: Tool) -> Operation {
    Operation {
        id: "next".into(),
        mask_id: "selected".into(),
        tool,
        enabled: true,
        amount: 0.8,
        radius: 0.14,
        threshold: 0.08,
        detail: 0.8,
        angle: 35.0,
        source: [0.4, 0.35],
        shift: [0.08, -0.07],
        path: vec![[-0.01, 0.0], [0.01, 0.0]],
    }
}
fn chart() -> Vec<f32> {
    (0..32 * 24)
        .flat_map(|i| {
            let (x, y) = ((i % 32) as f32, (i / 32) as f32);
            let noise = if i % 11 == 0 {
                0.25
            } else if i % 7 == 0 {
                -0.09
            } else {
                0.0
            };
            let v = 0.2 + x * 0.004 + y * 0.002 + noise;
            [
                v * 1.1 + (x * 0.3).sin() * 0.02,
                v,
                v * 0.8 + (y * 0.5).cos() * 0.02,
            ]
        })
        .collect()
}
#[test]
fn all_additions_are_finite_deterministic_neutral_and_selection_bound() {
    let input = chart();
    let weights: Vec<_> = (0..768)
        .map(|i| if i % 32 < 28 { 1.0 } else { 0.0 })
        .collect();
    for tool in TOOLS {
        let mut op = operation(tool);
        let mut output = input.clone();
        op.amount = 0.0;
        process(&mut output, 32, 24, &weights, &op);
        assert_eq!(output, input, "{tool:?}");
        op.amount = 0.8;
        process(&mut output, 32, 24, &weights, &op);
        let mut again = input.clone();
        process(&mut again, 32, 24, &weights, &op);
        assert_eq!(again, output, "{tool:?}");
        assert!(
            output.iter().all(|v| v.is_finite() && *v >= 0.0),
            "{tool:?}"
        );
        assert!(
            output.iter().zip(&input).any(|(a, b)| (a - b).abs() > 1e-6),
            "{tool:?} no correction"
        );
        for (i, w) in weights.iter().enumerate() {
            if *w <= 0.0 {
                assert_eq!(
                    output[i * 3..i * 3 + 3],
                    input[i * 3..i * 3 + 3],
                    "{tool:?}"
                );
            }
        }
        let mut empty = input.clone();
        process(&mut empty, 32, 24, &[0.0; 768], &op);
        assert_eq!(empty, input);
    }
}
#[test]
fn all_additions_handle_single_pixel_hdr_and_recipe_roundtrip() {
    for tool in TOOLS {
        for value in [0.0, 0.0001, 0.4, 8.0] {
            let mut pixels = vec![value; 3];
            process(&mut pixels, 1, 1, &[1.0], &operation(tool));
            assert!(
                pixels.iter().all(|v| v.is_finite() && *v >= 0.0),
                "{tool:?}"
            );
        }
        let mut recipe = aura_recipe::fixtures::reference();
        let op = operation(tool);
        spatial_lab::write(&mut recipe, std::slice::from_ref(&op)).unwrap();
        assert_eq!(spatial_lab::read(&recipe).unwrap(), vec![op]);
    }
}
#[test]
fn uniform_fractional_coverage_blends_each_correction_once() {
    let input = chart();
    for tool in TOOLS {
        let mut full = input.clone();
        let mut quarter = input.clone();
        process(&mut full, 32, 24, &[1.0; 768], &operation(tool));
        process(&mut quarter, 32, 24, &[0.25; 768], &operation(tool));
        for ((a, b), original) in full.iter().zip(&quarter).zip(&input) {
            assert!(
                (b - original - (a - original) * 0.25).abs() < 1e-5,
                "{tool:?}"
            );
        }
    }
}
#[test]
fn estimators_reject_impulses_and_reconstruction_obeys_order_bounds() {
    let mut pixels = vec![[0.3; 3]; 49];
    pixels[24] = [1.0; 3];
    let grid = Grid {
        pixels,
        weight: vec![1.0; 49],
        width: 7,
        height: 7,
        centre: [0.5; 2],
    };
    for tool in [
        Tool::HuberLocation,
        Tool::TukeyLocation,
        Tool::MedianOfMeans,
    ] {
        let output = evaluate(&grid, &operation(tool));
        assert!((output[24][0] - 0.3).abs() < 0.03, "{tool:?}");
    }
    let opened = evaluate(&grid, &operation(Tool::OpeningReconstruct));
    let closed = evaluate(&grid, &operation(Tool::ClosingReconstruct));
    assert!(opened[24][0] < 0.31);
    for i in 0..49 {
        for c in 0..3 {
            assert!(opened[i][c] <= grid.pixels[i][c] + 1e-6);
            assert!(closed[i][c] >= grid.pixels[i][c] - 1e-6);
        }
    }
}
#[test]
fn plane_residual_preserves_linear_ramps_and_chroma_regression_preserves_luminance() {
    let grid = Grid {
        pixels: (0..81)
            .map(|i| [0.2 + (i % 9) as f32 * 0.01 + (i / 9) as f32 * 0.02; 3])
            .collect(),
        weight: vec![1.0; 81],
        width: 9,
        height: 9,
        centre: [0.5; 2],
    };
    let output = evaluate(&grid, &operation(Tool::LocalPlaneResidual));
    for (actual, original) in output[40].iter().zip(grid.pixels[40]) {
        assert!((actual - original).abs() < 1e-5);
    }
    let grid = Grid {
        pixels: chart()
            .chunks_exact(3)
            .map(|p| [p[0], p[1], p[2]])
            .collect(),
        weight: vec![1.0; 768],
        width: 32,
        height: 24,
        centre: [0.5; 2],
    };
    let output = evaluate(&grid, &operation(Tool::ChromaGuidedRegression));
    for (a, b) in output.iter().zip(&grid.pixels) {
        assert!((luminance(*a) - luminance(*b)).abs() < 1e-5);
    }
}
#[test]
fn statistical_and_optical_sampling_cannot_leak_excluded_neighbours() {
    let input = chart();
    let weights: Vec<_> = (0..768)
        .map(|i| if i % 32 < 16 { 1.0 } else { 0.0 })
        .collect();
    let mut changed = input.clone();
    for i in 0..768 {
        if weights[i] <= 0.0 {
            changed[i * 3] = 99.0;
            changed[i * 3 + 1] = 8.0;
            changed[i * 3 + 2] = 51.0;
        }
    }
    for tool in TOOLS {
        let mut a = input.clone();
        let mut b = changed.clone();
        process(&mut a, 32, 24, &weights, &operation(tool));
        process(&mut b, 32, 24, &weights, &operation(tool));
        for i in 0..768 {
            if weights[i] > 0.0 {
                for c in 0..3 {
                    assert!((a[i * 3 + c] - b[i * 3 + c]).abs() < 1e-5, "{tool:?}");
                }
            }
        }
    }
}

#[test]
fn full_resolution_grid_transfer_cannot_import_excluded_corrections() {
    let (width, height) = (640, 96);
    let input: Vec<_> = (0..width * height)
        .flat_map(|i| {
            let v = 0.2 + (i % 37) as f32 * 0.006;
            [v * 1.2, v, v * 0.8]
        })
        .collect();
    // The boundary deliberately falls between working-grid nodes; the first
    // selected full-resolution pixels interpolate both sides of that boundary.
    let weights: Vec<_> = (0..width * height)
        .map(|i| if i % width >= 317 { 1.0 } else { 0.0 })
        .collect();
    let mut changed = input.clone();
    for (i, weight) in weights.iter().enumerate() {
        if *weight <= 0.0 {
            changed[i * 3..i * 3 + 3].copy_from_slice(&[9.0, 40.0, 2.0]);
        }
    }
    for tool in TOOLS {
        let mut a = input.clone();
        let mut b = changed.clone();
        process(&mut a, width, height, &weights, &operation(tool));
        process(&mut b, width, height, &weights, &operation(tool));
        for (i, weight) in weights.iter().enumerate() {
            for c in 0..3 {
                if *weight > 0.0 {
                    assert!((a[i * 3 + c] - b[i * 3 + c]).abs() < 1e-5, "{tool:?}, {i}");
                } else {
                    assert_eq!(a[i * 3 + c].to_bits(), input[i * 3 + c].to_bits());
                    assert_eq!(b[i * 3 + c].to_bits(), changed[i * 3 + c].to_bits());
                }
            }
        }
    }
}

#[test]
fn masked_correction_transfer_renormalizes_only_selected_bilinear_taps() {
    let correction = [[2.0; 3], [50.0; 3], [4.0; 3], [70.0; 3]];
    let result = transfer::sample(&correction, &[1.0, 0.0, 0.5, 0.0], 2, 2, 0.5, 0.5);
    for value in result {
        assert!((value - 8.0 / 3.0).abs() < 1e-6);
    }
    assert_eq!(
        transfer::sample(&correction, &[0.0; 4], 2, 2, 0.5, 0.5).map(f32::to_bits),
        [0.0_f32; 3].map(f32::to_bits)
    );
}
