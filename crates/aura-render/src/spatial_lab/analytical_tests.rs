use super::*;
pub(super) const TOOLS: [Tool; 25] = [
    Tool::LocalDctShrink,
    Tool::Cdf53Shrink,
    Tool::GeometricMedian,
    Tool::MeanShiftColour,
    Tool::HodgesLehmann,
    Tool::GeodesicRange,
    Tool::PatchLowRank,
    Tool::MahalanobisRange,
    Tool::LeeSpeckle,
    Tool::FrostSpeckle,
    Tool::GammaMapSpeckle,
    Tool::BiharmonicStencil,
    Tool::FractionalLaplacian,
    Tool::ParabolicBackground,
    Tool::DirectionalScratch,
    Tool::PhaseOnlyDetail,
    Tool::SpectralResidualDetail,
    Tool::HampelImpulse,
    Tool::CircularHueSmooth,
    Tool::ChromaMagnitudeMedian,
    Tool::WhitePatchAdapt,
    Tool::GrayEdgeAdapt,
    Tool::LocalSkewnessBalance,
    Tool::LocalKurtosisDetail,
    Tool::MinmodEdgeLimiter,
];
pub(super) fn operation(tool: Tool) -> Operation {
    Operation {
        id: "analytical".into(),
        mask_id: "selection".into(),
        tool,
        enabled: true,
        amount: 0.8,
        radius: 0.025,
        threshold: if tool == Tool::GammaMapSpeckle {
            0.8
        } else {
            0.08
        },
        detail: 0.8,
        angle: 31.0,
        source: [0.25, 0.5],
        shift: [0.005, 0.0],
        path: vec![[-0.025, 0.0], [0.025, 0.0]],
    }
}
pub(super) fn chart(w: usize, h: usize) -> Grid {
    Grid {
        pixels: (0..w * h)
            .map(|i| {
                let x = (i % w) as f32;
                let y = (i / w) as f32;
                let impulse = if i % 17 == 0 { 0.28 } else { 0.0 };
                [
                    0.25 + (x * 0.2).sin() * 0.05 + impulse,
                    0.3 + (y * 0.4).cos() * 0.03 + impulse,
                    0.2 + (x * 0.5 + y * 0.1).cos() * 0.02 + impulse,
                ]
            })
            .collect(),
        weight: vec![1.0; w * h],
        width: w,
        height: h,
        centre: [0.5; 2],
    }
}
#[test]
fn all_new_methods_are_active_deterministic_and_recipe_roundtrip() {
    let grid = chart(24, 17);
    for tool in TOOLS {
        let op = operation(tool);
        let a = evaluate(&grid, &op);
        let b = evaluate(&grid, &op);
        assert_eq!(a, b, "{tool:?} deterministic");
        assert!(
            a.iter()
                .zip(&grid.pixels)
                .any(|(a, b)| distance(*a, *b) > 1e-12),
            "{tool:?} inactive"
        );
        let mut recipe = aura_recipe::fixtures::reference();
        spatial_lab::write(&mut recipe, std::slice::from_ref(&op)).unwrap();
        assert_eq!(spatial_lab::read(&recipe).unwrap(), vec![op]);
    }
}
#[test]
fn native_methods_exclude_unselected_statistics_and_blend_coverage_once() {
    let mut a = chart(24, 17);
    a.weight = (0..408)
        .map(|i| if i % 24 < 15 && i / 24 > 2 { 1.0 } else { 0.0 })
        .collect();
    let mut b = chart(24, 17);
    b.weight = a.weight.clone();
    for (p, w) in b.pixels.iter_mut().zip(&b.weight) {
        if *w <= 0.0 {
            *p = [9.0, 0.001, 4.0];
        }
    }
    let input: Vec<_> = a.pixels.iter().flatten().copied().collect();
    for tool in TOOLS {
        let op = operation(tool);
        let out_a = evaluate(&a, &op);
        let out_b = evaluate(&b, &op);
        for i in 0..408 {
            if a.weight[i] > 0.0 {
                assert_eq!(
                    out_a[i].map(f32::to_bits),
                    out_b[i].map(f32::to_bits),
                    "{tool:?} leak {i}"
                );
            } else {
                assert_eq!(
                    out_a[i].map(f32::to_bits),
                    a.pixels[i].map(f32::to_bits),
                    "{tool:?} outside"
                );
            }
        }
        let mut full = input.clone();
        let mut quarter = input.clone();
        process(&mut full, 24, 17, &a.weight, &op);
        let fraction: Vec<_> = a.weight.iter().map(|w| w * 0.25).collect();
        process(&mut quarter, 24, 17, &fraction, &op);
        for ((p, q), original) in full.iter().zip(&quarter).zip(&input) {
            assert!(
                (q - original - (p - original) * 0.25).abs() < 2e-5,
                "{tool:?} coverage"
            );
        }
        let mut neutral = input.clone();
        let mut zero = op.clone();
        zero.amount = 0.0;
        process(&mut neutral, 24, 17, &a.weight, &zero);
        assert_eq!(neutral, input, "{tool:?} zero");
        process(&mut neutral, 24, 17, &[0.0; 408], &op);
        assert_eq!(neutral, input, "{tool:?} empty");
    }
}
#[test]
fn native_methods_remain_finite_on_thin_selections_black_and_hdr() {
    for tool in TOOLS {
        for value in [0.0, 1e-7, 0.4, 8.0, 1_000.0] {
            let mut p = vec![value; 3];
            process(&mut p, 1, 1, &[1.0], &operation(tool));
            assert!(p.iter().all(|v| v.is_finite() && *v >= 0.0), "{tool:?}");
        }
        let grid = chart(9, 9);
        let mut pixels: Vec<_> = grid.pixels.iter().flatten().copied().collect();
        let weights: Vec<_> = (0..81)
            .map(|i| if i % 9 == 4 { 0.7 } else { 0.0 })
            .collect();
        process(&mut pixels, 9, 9, &weights, &operation(tool));
        assert!(
            pixels.iter().all(|v| v.is_finite() && *v >= 0.0),
            "{tool:?} thin"
        );
    }
}
#[test]
fn dct_thresholding_has_known_dc_solution_and_lifting_is_reversible_at_zero() {
    let grid = chart(9, 9);
    let mut op = operation(Tool::LocalDctShrink);
    op.threshold = 1.0;
    let output = evaluate(&grid, &op);
    let expected = (-2..=2)
        .flat_map(|y| (-2..=2).map(move |x| (x, y)))
        .map(|(x, y)| luminance(grid.pixels[grid.neighbor(40, x, y)]))
        .sum::<f32>()
        / 25.0;
    assert!((luminance(output[40]) - expected).abs() < 2e-6);
    for tool in [Tool::LocalDctShrink, Tool::Cdf53Shrink] {
        op.tool = tool;
        op.threshold = 0.0;
        for (a, b) in evaluate(&grid, &op).iter().zip(&grid.pixels) {
            assert!(
                (luminance(*a) - luminance(*b)).abs() < 2e-6,
                "{tool:?} invert"
            );
        }
    }
}
#[test]
fn geometric_and_hampel_estimators_reject_an_isolated_outlier() {
    let mut grid = Grid {
        pixels: vec![[0.3; 3]; 81],
        weight: vec![1.0; 81],
        width: 9,
        height: 9,
        centre: [0.5; 2],
    };
    grid.pixels[40] = [0.95; 3];
    for tool in [
        Tool::GeometricMedian,
        Tool::HodgesLehmann,
        Tool::HampelImpulse,
    ] {
        assert!(
            (evaluate(&grid, &operation(tool))[40][0] - 0.3).abs() < 0.001,
            "{tool:?}"
        );
    }
    grid.pixels[40] = [0.31; 3];
    assert_eq!(
        evaluate(&grid, &operation(Tool::HampelImpulse))[40].map(f32::to_bits),
        grid.pixels[40].map(f32::to_bits)
    );
}
#[test]
fn biharmonic_and_minmod_stencils_preserve_linear_ramps() {
    let grid = Grid {
        pixels: (0..81)
            .map(|i| [0.2 + (i % 9) as f32 * 0.01 + (i / 9) as f32 * 0.02; 3])
            .collect(),
        weight: vec![1.0; 81],
        width: 9,
        height: 9,
        centre: [0.5; 2],
    };
    for tool in [Tool::BiharmonicStencil, Tool::MinmodEdgeLimiter] {
        assert!(
            distance(evaluate(&grid, &operation(tool))[40], grid.pixels[40]) < 1e-12,
            "{tool:?}"
        );
    }
}
#[test]
fn hue_filters_and_illuminant_adaptation_preserve_luminance() {
    let grid = chart(24, 17);
    for tool in [
        Tool::CircularHueSmooth,
        Tool::ChromaMagnitudeMedian,
        Tool::WhitePatchAdapt,
        Tool::GrayEdgeAdapt,
    ] {
        for (a, b) in evaluate(&grid, &operation(tool)).iter().zip(&grid.pixels) {
            assert!((luminance(*a) - luminance(*b)).abs() < 2e-6, "{tool:?}");
        }
    }
    let grid = Grid {
        pixels: vec![[0.6, 0.3, 0.15]; 25],
        weight: vec![1.0; 25],
        width: 5,
        height: 5,
        centre: [0.5; 2],
    };
    let white = evaluate(&grid, &operation(Tool::WhitePatchAdapt));
    assert!(
        (white[12][0] - white[12][1]).abs() < 1e-6 && (white[12][1] - white[12][2]).abs() < 1e-6
    );
}
