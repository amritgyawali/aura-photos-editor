use super::*;
use crate::color_lab::{filtered, tests::operation};
fn chart() -> Vec<f32> {
    (0..(18 * 14))
        .flat_map(|i| {
            let x = (i % 18) as f32 / 17.0;
            let y = (i / 18) as f32 / 13.0;
            [
                0.03 + x * 1.3 + ((i * 13 % 17) as f32) * 0.02,
                0.04 + y * 0.9 + ((i * 7 % 19) as f32) * 0.025,
                0.02 + (1.0 - x) * 0.65 + ((i * 11 % 23) as f32) * 0.015,
            ]
        })
        .collect()
}
fn bits(values: &[f32]) -> Vec<u32> {
    values.iter().map(|v| v.to_bits()).collect()
}
#[test]
fn all_twenty_five_have_distinct_active_finite_deterministic_results_and_exact_neutral_controls() {
    let source = chart();
    let weights = vec![1.0; 18 * 14];
    let mut fingerprints = std::collections::BTreeSet::new();
    for tool in TOOLS {
        let mut op = operation(tool);
        op.width = 0.7;
        op.pivot = 0.25;
        let actual = filtered(&source, &weights, 18, 14, &op);
        assert!(
            actual.iter().all(|v| v.is_finite() && *v >= 0.0),
            "{tool:?}"
        );
        assert!(
            source
                .iter()
                .zip(&actual)
                .any(|(a, b)| (a - b).abs() > 1e-5),
            "inactive {tool:?}"
        );
        assert_eq!(
            bits(&actual),
            bits(&filtered(&source, &weights, 18, 14, &op)),
            "{tool:?}"
        );
        assert!(
            fingerprints.insert(bits(&actual)),
            "duplicate output {tool:?}"
        );
        for field in ["amount", "strength"] {
            let mut neutral = op.clone();
            if field == "amount" {
                neutral.amount = 0.0;
            } else {
                neutral.strength = 0.0;
            }
            assert_eq!(
                bits(&source),
                bits(&filtered(&source, &weights, 18, 14, &neutral)),
                "{tool:?} {field}"
            );
        }
    }
}
#[test]
fn excluded_pixels_cannot_change_selected_statistics_or_local_results() {
    let source = chart();
    let weights: Vec<_> = (0..18 * 14)
        .map(|i| if i % 18 < 9 { 1.0 } else { 0.0 })
        .collect();
    let mut changed = source.clone();
    for (i, weight) in weights.iter().enumerate() {
        if *weight == 0.0 {
            changed[i * 3..i * 3 + 3].copy_from_slice(&[37.0, 0.000_001, 19.0]);
        }
    }
    for tool in TOOLS {
        let mut op = operation(tool);
        op.width = 0.7;
        let a = filtered(&source, &weights, 18, 14, &op);
        let b = filtered(&changed, &weights, 18, 14, &op);
        for (i, weight) in weights.iter().enumerate() {
            let range = i * 3..i * 3 + 3;
            if *weight > 0.0 {
                assert_eq!(
                    bits(&a[range.clone()]),
                    bits(&b[range]),
                    "selected contamination {tool:?}, pixel {i}"
                );
            } else {
                assert_eq!(bits(&a[range.clone()]), bits(&source[range.clone()]));
                assert_eq!(bits(&b[range.clone()]), bits(&changed[range]));
            }
        }
    }
}
#[test]
fn feather_coverage_blends_linearly_for_each_method() {
    let source = chart();
    let full = vec![1.0; 18 * 14];
    let feather = vec![0.25; 18 * 14];
    for tool in TOOLS {
        let op = operation(tool);
        let a = filtered(&source, &full, 18, 14, &op);
        let b = filtered(&source, &feather, 18, 14, &op);
        for ((p, q), r) in source.iter().zip(a).zip(b) {
            assert!((r - (p + (q - p) * 0.25)).abs() < 2e-5, "{tool:?}");
        }
    }
}
#[test]
fn black_hdr_tiny_frames_and_empty_selection_are_safe() {
    for p in [[0.0; 3], [1e-12; 3], [16.0, 4.0, 0.1], [0.0, 1.0, 0.0]] {
        for tool in TOOLS {
            let op = operation(tool);
            let result = filtered(&p, &[1.0], 1, 1, &op);
            assert!(
                result.iter().all(|v| v.is_finite() && *v >= 0.0),
                "{tool:?} {p:?}"
            );
            assert_eq!(bits(&p), bits(&filtered(&p, &[0.0], 1, 1, &op)));
        }
    }
}
#[test]
fn chromatic_methods_hold_luminance_and_keep_neutral_pixels_stable_where_defined() {
    let source = chart();
    let weights = vec![1.0; 18 * 14];
    for tool in TOOLS.into_iter().skip(10) {
        let actual = filtered(&source, &weights, 18, 14, &operation(tool));
        for (p, q) in source.chunks_exact(3).zip(actual.chunks_exact(3)) {
            assert!(
                (luma([p[0], p[1], p[2]]) - luma([q[0], q[1], q[2]])).abs() < 2e-5,
                "{tool:?}"
            );
        }
    }
}
#[test]
fn wiener_removes_constant_field_noise_and_median_rejects_an_impulse() {
    let source = vec![0.2; 9 * 9 * 3];
    let weights = vec![1.0; 9 * 9];
    let mut op = operation(Tool::LocalWienerTone);
    op.amount = 1.0;
    op.width = 0.2;
    let result = filtered(&source, &weights, 9, 9, &op);
    assert!(result.iter().all(|v| (*v - 0.2).abs() < 1e-6));
    let mut impulse = source.clone();
    let center = 4 * 9 + 4;
    impulse[center * 3..center * 3 + 3].copy_from_slice(&[0.6, 0.05, 0.1]);
    op.tool = Tool::MedianChroma;
    op.radius = 0.2;
    let result = filtered(&impulse, &weights, 9, 9, &op);
    let recovered = chroma(pixel(&result, center));
    assert!(length(recovered) < 1e-5, "{recovered:?}");
    assert!((luma(pixel(&result, center)) - luma(pixel(&impulse, center))).abs() < 1e-6);
}
#[test]
fn bradford_maps_measured_white_to_target_white_at_constant_luminance() {
    let source = [0.4, 0.2, 0.1];
    let target = [0.2, 0.2, 0.2];
    let actual = adapt(source, source, target);
    assert!((actual[0] - actual[1]).abs() < 2e-5);
    assert!((actual[2] - actual[1]).abs() < 2e-5);
    assert!((luma(source) - luma(actual)).abs() < 1e-6);
}
#[test]
fn tone_curves_match_their_analytical_contracts() {
    let source = vec![
        0.0, 0.0, 0.0, 0.25, 0.25, 0.25, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0,
    ];
    let weights = vec![1.0; 4];
    for tool in [
        Tool::ExponentialShoulder,
        Tool::HermiteTone,
        Tool::SchlickBias,
    ] {
        let mut op = operation(tool);
        op.amount = 1.0;
        let actual = filtered(&source, &weights, 4, 1, &op);
        assert_eq!(&actual[0..3], &[0.0; 3]);
        assert!((actual[6] - 1.0).abs() < 1e-6);
        assert!(actual[9] > actual[6]);
    }
    let mut op = operation(Tool::BlackFloor);
    op.amount = 1.0;
    op.pivot = 0.25;
    let actual = filtered(&source, &weights, 4, 1, &op);
    assert_eq!(&actual[3..6], &[0.0; 3]);
}

#[test]
fn bilateral_boundary_weights_are_mirror_symmetric() {
    let source = chart();
    let weights = vec![1.0; 18 * 14];
    let mut mirrored = vec![0.0; source.len()];
    for y in 0..14 {
        for x in 0..18 {
            mirrored[(y * 18 + x) * 3..(y * 18 + x) * 3 + 3]
                .copy_from_slice(&source[(y * 18 + 17 - x) * 3..(y * 18 + 17 - x) * 3 + 3]);
        }
    }
    let mut op = operation(Tool::BilateralChroma);
    op.radius = 0.25;
    op.width = 0.4;
    let a = filtered(&source, &weights, 18, 14, &op);
    let b = filtered(&mirrored, &weights, 18, 14, &op);
    for y in 0..14 {
        for x in 0..18 {
            for c in 0..3 {
                assert!((a[(y * 18 + x) * 3 + c] - b[(y * 18 + 17 - x) * 3 + c]).abs() < 1e-6);
            }
        }
    }
}

#[test]
fn local_stencil_methods_retain_no_full_frame_targets_or_unused_scalar_planes() {
    let source = chart();
    let weights = vec![1.0; 18 * 14];
    for tool in [
        Tool::DarkChannelDehaze,
        Tool::LocalMidrange,
        Tool::MedianChroma,
        Tool::BilateralChroma,
    ] {
        let context = Creative::new(&source, &weights, 18, 14, &operation(tool));
        assert!(context.stencil.is_some());
        assert!(context.local.is_none());
        assert!(context.rank.iter().all(|values| values.len() <= 65_536));
    }
    let context = Creative::new(&source, &weights, 18, 14, &operation(Tool::LocalWienerTone));
    let local = context.local.as_ref().unwrap();
    assert_eq!(local.y.len(), 18 * 14);
    assert_eq!(local.variance.len(), 18 * 14);
    assert!(local.c.iter().all(Vec::is_empty));
    assert!(local.yc.iter().all(Vec::is_empty));
}
