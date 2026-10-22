use super::*;
const TOOLS: [Tool; 25] = [
    Tool::AnisotropicDiffusion,
    Tool::TotalVariation,
    Tool::NonLocalMeans,
    Tool::VectorMedoid,
    Tool::TrimmedMean,
    Tool::AdaptiveWiener,
    Tool::HaarDenoise,
    Tool::HaarDetail,
    Tool::LaplacianDetail,
    Tool::Kuwahara,
    Tool::TensorFlow,
    Tool::BlockDeblock,
    Tool::ApertureBokeh,
    Tool::Starburst,
    Tool::RotationBlur,
    Tool::TrajectoryBlur,
    Tool::HarmonicFill,
    Tool::PoissonDonor,
    Tool::ChannelRegister,
    Tool::MorphologyClean,
    Tool::LocalRankContrast,
    Tool::MosquitoSuppress,
    Tool::EdgeDirectedSmooth,
    Tool::DonorBilateral,
    Tool::TextureQuilt,
];
fn operation(tool: Tool) -> Operation {
    Operation {
        id: "one".into(),
        mask_id: "selected".into(),
        tool,
        enabled: true,
        amount: 0.8,
        radius: 0.12,
        threshold: 0.07,
        detail: 0.7,
        angle: 90.0,
        source: [0.2, 0.2],
        shift: [0.05, -0.05],
        path: vec![[-0.04, 0.0], [0.0, 0.04], [0.04, 0.0]],
    }
}
fn chart() -> Vec<f32> {
    (0..32 * 24)
        .flat_map(|i| {
            let x = i % 32;
            let y = i / 32;
            let noise = if (i * 17) % 7 == 0 { 0.018 } else { -0.006 };
            let v = 0.2 + x as f32 * 0.002 + y as f32 * 0.001 + noise;
            if x == 16 {
                [0.75, 0.65, 0.55]
            } else {
                [v * 1.2, v, v * 0.8]
            }
        })
        .collect()
}
#[test]
fn all_twenty_five_are_finite_deterministic_neutral_at_zero_and_preserve_unselected_pixels() {
    let input = chart();
    let weights: Vec<_> = (0..768)
        .map(|i| if i % 32 < 24 { 1.0 } else { 0.0 })
        .collect();
    for tool in TOOLS {
        let mut op = operation(tool);
        op.amount = 0.0;
        let mut zero = input.clone();
        process(&mut zero, 32, 24, &weights, &op);
        assert_eq!(zero, input, "{tool:?}");
        op.amount = 0.8;
        let mut output = input.clone();
        process(&mut output, 32, 24, &weights, &op);
        let mut again = input.clone();
        process(&mut again, 32, 24, &weights, &op);
        assert_eq!(again, output, "{tool:?}");
        assert!(
            output.iter().all(|v| v.is_finite() && *v >= 0.0),
            "{tool:?}"
        );
        for (i, weight) in weights.iter().enumerate() {
            if *weight <= 0.0 {
                for c in 0..3 {
                    assert_eq!(
                        output[i * 3 + c].to_bits(),
                        input[i * 3 + c].to_bits(),
                        "{tool:?}"
                    );
                }
            }
        }
        assert!(
            output.iter().zip(&input).any(|(a, b)| (a - b).abs() > 1e-6),
            "{tool:?} produced no correction on the target fixture"
        );
    }
}
#[test]
fn tiny_hdr_and_empty_selection_stay_finite() {
    for tool in TOOLS {
        for value in [0.0, 0.001, 4.0] {
            let input = vec![value; 3];
            let mut out = input.clone();
            process(&mut out, 1, 1, &[0.0], &operation(tool));
            assert_eq!(out, input);
            process(&mut out, 1, 1, &[1.0], &operation(tool));
            assert!(out.iter().all(|v| v.is_finite() && *v >= 0.0));
        }
    }
}
#[test]
fn fractional_selection_scales_once_and_haar_preserves_block_mean() {
    let input = chart();
    let op = operation(Tool::HaarDenoise);
    let mut full = input.clone();
    let mut quarter = input.clone();
    process(&mut full, 32, 24, &vec![1.0; 768], &op);
    process(&mut quarter, 32, 24, &vec![0.25; 768], &op);
    for ((a, b), original) in full.iter().zip(&quarter).zip(&input) {
        assert!((b - original - (a - original) * 0.25).abs() < 1e-5);
    }
    let grid = Grid {
        pixels: vec![[0.1; 3], [0.2; 3], [0.3; 3], [0.8; 3]],
        weight: vec![1.0; 4],
        width: 2,
        height: 2,
        centre: [0.5; 2],
    };
    let result = evaluate(&grid, &op);
    let before = grid.pixels.iter().map(|p| p[0]).sum::<f32>();
    let after = result.iter().map(|p| p[0]).sum::<f32>();
    assert!((before - after).abs() < 1e-6);
}
#[test]
fn missing_recipe_selection_cannot_fall_back_to_global_processing() {
    let input = chart();
    let mut recipe = aura_recipe::fixtures::reference();
    spatial_lab::write(&mut recipe, &[operation(Tool::HaarDetail)]).unwrap();
    let mut out = input.clone();
    apply(&mut out, 32, 24, &recipe, None);
    assert_eq!(out, input);
}

#[test]
fn guided_bilinear_sampling_excludes_every_unselected_tap() {
    let grid = Grid {
        pixels: vec![[0.2; 3], [20.0, 0.0, 10.0], [0.4; 3], [10.0, 0.0, 20.0]],
        weight: vec![1.0, 0.0, 0.5, 0.0],
        width: 2,
        height: 2,
        centre: [0.5; 2],
    };
    let sampled = grid.guided_sample(0, 0.25, 0.5).unwrap();
    for value in sampled {
        assert!((value - (0.2 + 0.4 * 0.5) / 1.5).abs() < 1e-6);
    }
    assert!(grid.guided_sample(0, 1.0, 0.5).is_none());
    assert!(grid.guided_sample(1, 0.0, 0.0).is_none());
}

#[test]
fn rank_contrast_preserves_constant_patches_and_handles_ties_symmetrically() {
    let mut input = vec![0.3; 7 * 5 * 3];
    let original = input.clone();
    process(
        &mut input,
        7,
        5,
        &[1.0; 35],
        &operation(Tool::LocalRankContrast),
    );
    assert_eq!(input, original);
    let grid = Grid {
        pixels: vec![[0.1; 3], [0.3; 3], [0.3; 3], [0.3; 3], [0.5; 3]],
        weight: vec![1.0; 5],
        width: 5,
        height: 1,
        centre: [0.5; 2],
    };
    assert_eq!(
        robust(&grid, 2, &operation(Tool::LocalRankContrast)).map(f32::to_bits),
        [0.3_f32; 3].map(f32::to_bits)
    );
}

#[test]
fn haar_odd_dimensions_preserve_mean_and_leave_incomplete_blocks_unchanged() {
    let grid = Grid {
        pixels: (0..15).map(|i| [0.2 + (i % 4) as f32 * 0.1; 3]).collect(),
        weight: vec![1.0; 15],
        width: 5,
        height: 3,
        centre: [0.5; 2],
    };
    for tool in [Tool::HaarDenoise, Tool::HaarDetail] {
        let output = evaluate(&grid, &operation(tool));
        assert!(
            (output.iter().map(|p| p[0]).sum::<f32>()
                - grid.pixels.iter().map(|p| p[0]).sum::<f32>())
            .abs()
                < 1e-5
        );
        for i in [4, 9, 10, 11, 12, 13, 14] {
            assert_eq!(
                output[i].map(f32::to_bits),
                grid.pixels[i].map(f32::to_bits),
                "{tool:?} at {i}"
            );
        }
    }
}

#[test]
fn optical_and_registration_samples_do_not_import_excluded_colour() {
    let input = chart();
    let mut changed = input.clone();
    let weights: Vec<_> = (0..768)
        .map(|i| if i % 32 < 16 { 1.0 } else { 0.0 })
        .collect();
    for (i, weight) in weights.iter().enumerate() {
        if *weight == 0.0 {
            changed[i * 3..i * 3 + 3].copy_from_slice(&[20.0, 0.0, 10.0]);
        }
    }
    for tool in [
        Tool::ApertureBokeh,
        Tool::Starburst,
        Tool::RotationBlur,
        Tool::TrajectoryBlur,
        Tool::ChannelRegister,
    ] {
        let mut a = input.clone();
        let mut b = changed.clone();
        let op = operation(tool);
        process(&mut a, 32, 24, &weights, &op);
        process(&mut b, 32, 24, &weights, &op);
        for (i, weight) in weights.iter().enumerate() {
            if *weight > 0.0 {
                assert_eq!(
                    &a[i * 3..i * 3 + 3],
                    &b[i * 3..i * 3 + 3],
                    "{tool:?} at {i}"
                );
            }
        }
    }
}

#[test]
fn rotation_keeps_the_explicit_pixel_centre_stationary() {
    let grid = Grid {
        pixels: (0..25).map(|i| [i as f32 * 0.01; 3]).collect(),
        weight: vec![1.0; 25],
        width: 5,
        height: 5,
        centre: [0.5; 2],
    };
    let mut op = operation(Tool::RotationBlur);
    op.source = [0.5, 0.5];
    let result = optical(&grid, 12, &op);
    for (a, b) in result.into_iter().zip(grid.pixels[12]) {
        assert!((a - b).abs() < 1e-7);
    }
}
