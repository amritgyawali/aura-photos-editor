use super::*;
const TOOLS: [Tool; 25] = [
    Tool::SpatialNeutralBridge,
    Tool::TonalNeutralBridge,
    Tool::ChromaReject,
    Tool::PaletteAxis,
    Tool::DecorrelationStretch,
    Tool::PrincipalChromaSuppress,
    Tool::SoftGamut,
    Tool::LumaChromaCompress,
    Tool::ChromaCusp,
    Tool::NeutralExclusion,
    Tool::HistogramEqualize,
    Tool::ClippedEqualize,
    Tool::LogEqualize,
    Tool::RankGaussian,
    Tool::BimodalSeparate,
    Tool::PercentileRemap,
    Tool::PhotographicMap,
    Tool::AsinhMap,
    Tool::LogToeShoulder,
    Tool::LocalLogContrast,
    Tool::RetinexRatio,
    Tool::ChromaEdgeProtect,
    Tool::ChromaWinsorize,
    Tool::OpponentContrast,
    Tool::SampledReplace,
];
pub(super) fn operation(tool: Tool) -> Operation {
    Operation {
        id: "one".into(),
        tool,
        mask_id: "left".into(),
        enabled: true,
        amount: 0.8,
        strength: 1.0,
        radius: 0.1,
        pivot: 0.18,
        width: 0.1,
        source: [0.1, 0.2],
        second: [0.3, 0.8],
        target: [0.1, 0.4, 0.6],
    }
}
pub(super) fn chart() -> Vec<f32> {
    (0..32 * 24)
        .flat_map(|i| {
            let x = (i % 32) as f32 / 31.0;
            let y = (i / 32) as f32 / 23.0;
            let signal = 0.05 + 0.5 * x * x + 0.12 * y;
            let color = if i % 7 == 0 { 0.22 } else { 0.0 };
            [
                signal * (1.0 + x) + color,
                signal * (0.8 + y * 0.3),
                signal * (0.35 + y) + color * 0.1,
            ]
        })
        .collect()
}
pub(super) fn bits(values: &[f32]) -> Vec<u32> {
    values.iter().map(|v| v.to_bits()).collect()
}
#[test]
fn all_twenty_five_methods_are_real_deterministic_and_zero_exact() {
    let source = chart();
    let weights = vec![1.0; 32 * 24];
    for tool in TOOLS {
        let mut op = operation(tool);
        let result = filtered(&source, &weights, 32, 24, &op);
        assert!(
            result.iter().all(|v| v.is_finite() && *v >= 0.0),
            "{tool:?}"
        );
        assert!(
            result
                .iter()
                .zip(&source)
                .any(|(a, b)| (a - b).abs() > 1e-5),
            "{tool:?} is inert"
        );
        assert_eq!(
            bits(&result),
            bits(&filtered(&source, &weights, 32, 24, &op)),
            "{tool:?}"
        );
        op.amount = 0.0;
        assert_eq!(
            bits(&source),
            bits(&filtered(&source, &weights, 32, 24, &op))
        );
        op.amount = 1.0;
        op.strength = 0.0;
        assert_eq!(
            bits(&source),
            bits(&filtered(&source, &weights, 32, 24, &op))
        );
    }
}
#[test]
fn selected_kernels_and_statistics_ignore_unselected_colour() {
    let a = chart();
    let mut b = a.clone();
    let weights: Vec<_> = (0..32 * 24)
        .map(|i| if i % 32 < 16 { 1.0 } else { 0.0 })
        .collect();
    for (i, weight) in weights.iter().enumerate() {
        if *weight <= 0.0 {
            b[i * 3..i * 3 + 3].copy_from_slice(&[15.0, 0.0, 8.0]);
        }
    }
    for tool in TOOLS {
        let op = operation(tool);
        let x = filtered(&a, &weights, 32, 24, &op);
        let y = filtered(&b, &weights, 32, 24, &op);
        for (i, weight) in weights.iter().enumerate() {
            if *weight <= 0.0 {
                assert_eq!(
                    pixel(&a, i).map(f32::to_bits),
                    pixel(&x, i).map(f32::to_bits)
                );
                assert_eq!(
                    pixel(&b, i).map(f32::to_bits),
                    pixel(&y, i).map(f32::to_bits)
                );
            } else {
                for (u, v) in pixel(&x, i).into_iter().zip(pixel(&y, i)) {
                    assert!((u - v).abs() < 1e-5, "{tool:?} read excluded colour");
                }
            }
        }
    }
}
#[test]
fn source_references_change_adaptation_and_sample_replacement() {
    let src = chart();
    let weights = vec![1.0; 32 * 24];
    for tool in [
        Tool::SpatialNeutralBridge,
        Tool::TonalNeutralBridge,
        Tool::ChromaReject,
        Tool::PaletteAxis,
        Tool::SampledReplace,
    ] {
        let mut op = operation(tool);
        op.width = 1.0;
        let first = filtered(&src, &weights, 32, 24, &op);
        op.source = [0.9, 0.1];
        op.second = [0.7, 0.2];
        let next = filtered(&src, &weights, 32, 24, &op);
        assert!(
            first.iter().zip(&next).any(|(a, b)| (a - b).abs() > 1e-5),
            "{tool:?}"
        );
    }
}
#[test]
fn spatial_bridge_uses_pixel_centres_for_normalized_reference_locations() {
    let mut op = operation(Tool::SpatialNeutralBridge);
    op.source = [0.125, 0.5];
    op.second = [0.875, 0.5];
    let mut kernel = Kernel::new(&[0.2; 12], &[1.0; 4], 4, 1, &op);
    kernel.source = [0.4, 0.2, 0.1];
    kernel.second = [0.1, 0.2, 0.4];
    for (i, reference) in [(0, kernel.source), (3, kernel.second)] {
        let neutral = kernel.bridge(i, reference);
        for value in neutral {
            assert!((value - luma(reference)).abs() < 1e-6);
        }
    }
}
#[test]
fn measured_tone_maps_are_monotone_and_colour_maps_preserve_luminance() {
    let src = chart();
    let weights = vec![1.0; 32 * 24];
    for tool in [
        Tool::HistogramEqualize,
        Tool::ClippedEqualize,
        Tool::LogEqualize,
        Tool::RankGaussian,
        Tool::BimodalSeparate,
        Tool::PercentileRemap,
    ] {
        let op = operation(tool);
        let kernel = Kernel::new(&src, &weights, 32, 24, &op);
        let mut previous = 0.0;
        for n in 1..200 {
            let y = kernel.stats.lo + (kernel.stats.hi - kernel.stats.lo) * n as f32 / 200.0;
            let target = luma(kernel.histogram([y; 3]));
            assert!(target + 1e-5 >= previous, "{tool:?} reverses tones");
            previous = target;
        }
    }
    for tool in [
        Tool::ChromaReject,
        Tool::PaletteAxis,
        Tool::DecorrelationStretch,
        Tool::PrincipalChromaSuppress,
        Tool::SoftGamut,
        Tool::NeutralExclusion,
        Tool::ChromaEdgeProtect,
        Tool::ChromaWinsorize,
        Tool::OpponentContrast,
    ] {
        let result = filtered(&src, &weights, 32, 24, &operation(tool));
        for (p, q) in src.chunks_exact(3).zip(result.chunks_exact(3)) {
            assert!(
                (luma([p[0], p[1], p[2]]) - luma([q[0], q[1], q[2]])).abs() < 1e-5,
                "{tool:?}"
            );
        }
    }
}
#[test]
fn dark_hdr_single_pixel_and_fractional_masks_are_safe() {
    for source in [[0.0; 3], [1e-12; 3], [8.0, 3.0, 0.4]] {
        for tool in TOOLS {
            let op = operation(tool);
            assert!(
                filtered(&source, &[1.0], 1, 1, &op)
                    .iter()
                    .all(|v| v.is_finite() && *v >= 0.0),
                "{tool:?}"
            );
            assert_eq!(bits(&source), bits(&filtered(&source, &[0.0], 1, 1, &op)));
        }
    }
    let source = chart();
    let op = operation(Tool::AsinhMap);
    let full = filtered(&source, &vec![1.0; 32 * 24], 32, 24, &op);
    let quarter = filtered(&source, &vec![0.25; 32 * 24], 32, 24, &op);
    for ((p, q), a) in full.iter().zip(quarter).zip(source) {
        assert!((q - (a + (p - a) * 0.25)).abs() < 1e-5);
    }
}
#[test]
fn missing_disabled_and_zero_masks_neutralize_real_saved_operations() {
    let mut recipe = aura_recipe::fixtures::reference();
    recipe.extra.clear();
    recipe.extra.insert(
        aura_recipe::local_masks::KEY.into(),
        serde_json::json!([{
            "id":"left","name":"Selected","enabled":true,"amount":1.0,
            "components":[{"mode":"add","source":{"type":"radial","centre":[0.5,0.5],
                "radii":[1.0,1.0],"angle":0,"feather":0}}],"params":{}
        }]),
    );
    let source = chart();
    let op = operation(Tool::AsinhMap);
    color_lab::write(&mut recipe, &[op]).unwrap();
    let mut actual = source.clone();
    apply(&mut actual, 32, 24, &recipe, None);
    assert_ne!(bits(&source), bits(&actual));
    for (field, value) in [
        ("enabled", serde_json::json!(false)),
        ("amount", serde_json::json!(0.0)),
    ] {
        let mut candidate = recipe.clone();
        candidate
            .extra
            .get_mut(aura_recipe::local_masks::KEY)
            .unwrap()[0][field] = value;
        actual = source.clone();
        apply(&mut actual, 32, 24, &candidate, None);
        assert_eq!(bits(&actual), bits(&source));
    }
    recipe.extra.remove(aura_recipe::local_masks::KEY);
    actual = source.clone();
    apply(&mut actual, 32, 24, &recipe, None);
    assert_eq!(bits(&actual), bits(&source));
}
