use super::*;
use crate::color_lab::{filtered, tests::operation};
fn chart() -> Vec<f32> {
    (0..24 * 18)
        .flat_map(|i| {
            let x = (i % 24) as f32 / 23.0;
            let y = (i / 24) as f32 / 17.0;
            [
                0.04 + 0.9 * x + 0.04 * (i % 3) as f32,
                0.03 + 0.65 * y + 0.025 * (i % 7) as f32,
                0.05 + 0.45 * (1.0 - x) + 0.02 * (i % 11) as f32,
            ]
        })
        .collect()
}
fn op(tool: Tool) -> Operation {
    let mut op = operation(tool);
    op.amount = 1.0;
    op.strength = 1.0;
    op.width = 0.35;
    op.pivot = 0.8;
    op.target = [0.2, 0.4, 0.7];
    op.radius = 0.15;
    op
}
fn bits(v: &[f32]) -> Vec<u32> {
    v.iter().map(|v| v.to_bits()).collect()
}
#[test]
fn all_twenty_five_new_mechanisms_are_active_distinct_deterministic_finite_and_zero_exact() {
    let source = chart();
    let weights = vec![1.0; 24 * 18];
    let mut signatures = std::collections::BTreeSet::new();
    for tool in TOOLS {
        let operation = op(tool);
        let result = filtered(&source, &weights, 24, 18, &operation);
        assert!(
            result.iter().all(|v| v.is_finite() && *v >= 0.0),
            "{tool:?}"
        );
        assert!(
            source
                .iter()
                .zip(&result)
                .any(|(a, b)| (a - b).abs() > 1e-5),
            "inactive {tool:?}"
        );
        assert!(signatures.insert(bits(&result)), "duplicate {tool:?}");
        assert_eq!(
            bits(&result),
            bits(&filtered(&source, &weights, 24, 18, &operation)),
            "{tool:?}"
        );
        for amount_zero in [true, false] {
            let mut zero = operation.clone();
            if amount_zero {
                zero.amount = 0.0;
            } else {
                zero.strength = 0.0;
            }
            assert_eq!(
                bits(&source),
                bits(&filtered(&source, &weights, 24, 18, &zero))
            );
        }
    }
}
#[test]
fn reference_patches_statistics_and_error_diffusion_ignore_all_excluded_pixels() {
    let source = chart();
    let weights: Vec<_> = (0..24 * 18)
        .map(|i| if i % 24 < 12 { 1.0 } else { 0.0 })
        .collect();
    let mut changed = source.clone();
    for (i, weight) in weights.iter().enumerate() {
        if *weight == 0.0 {
            changed[i * 3..i * 3 + 3].copy_from_slice(&[16.0, 0.0001, 8.0]);
        }
    }
    for tool in TOOLS {
        let operation = op(tool);
        let a = filtered(&source, &weights, 24, 18, &operation);
        let b = filtered(&changed, &weights, 24, 18, &operation);
        for (i, weight) in weights.iter().enumerate() {
            let r = i * 3..i * 3 + 3;
            if *weight > 0.0 {
                assert_eq!(bits(&a[r.clone()]), bits(&b[r]), "{tool:?} at {i}");
            } else {
                assert_eq!(bits(&source[r.clone()]), bits(&a[r.clone()]));
                assert_eq!(bits(&changed[r.clone()]), bits(&b[r]));
            }
        }
    }
}
#[test]
fn uniform_feather_changes_only_the_final_blend() {
    let source = chart();
    let weights = vec![1.0; 24 * 18];
    let feather = vec![0.25; 24 * 18];
    for tool in TOOLS {
        let operation = op(tool);
        let a = filtered(&source, &weights, 24, 18, &operation);
        let b = filtered(&source, &feather, 24, 18, &operation);
        for ((p, q), r) in source.iter().zip(a).zip(b) {
            assert!((r - (p + (q - p) * 0.25)).abs() < 3e-5, "{tool:?}");
        }
    }
}
#[test]
fn black_hdr_single_pixel_and_empty_selection_are_safe() {
    for p in [[0.0; 3], [1e-12; 3], [32.0, 0.1, 6.0], [0.0, 1.0, 0.0]] {
        for tool in TOOLS {
            let operation = op(tool);
            let result = filtered(&p, &[1.0], 1, 1, &operation);
            assert!(
                result.iter().all(|v| v.is_finite() && *v >= 0.0),
                "{tool:?} {p:?}"
            );
            assert_eq!(bits(&p), bits(&filtered(&p, &[0.0], 1, 1, &operation)));
        }
    }
}
#[test]
fn gaussian_transport_simplex_and_linear_solver_have_known_answers() {
    let matrix = math::transport([[1.0, 0.0], [0.0, 4.0]], [[4.0, 0.0], [0.0, 1.0]], 0.0);
    assert!((matrix[0][0] - 2.0).abs() < 1e-6);
    assert!((matrix[1][1] - 0.5).abs() < 1e-6);
    assert!(matrix[0][1].abs() < 1e-6);
    assert_eq!(
        math::simplex([2.0, 2.0], [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]).map(f32::to_bits),
        [0.5_f32, 0.5].map(f32::to_bits)
    );
    let result = measured::solve(
        [[2.0, 0.0, 0.0], [0.0, 3.0, 0.0], [0.0, 0.0, 4.0]],
        [2.0, 6.0, 12.0],
    );
    assert_eq!(
        result.map(f32::to_bits),
        [1.0_f32, 2.0, 3.0].map(f32::to_bits)
    );
}
#[test]
fn energy_projection_preserves_channel_sum_and_hits_feasible_luminance() {
    let p = [0.3, 0.4, 0.2];
    let q = math::energy_projection(p, 0.35);
    assert!((q.iter().sum::<f32>() - p.iter().sum::<f32>()).abs() < 1e-6);
    assert!((luma(q) - 0.35).abs() < 1e-5);
    let black = math::energy_projection([0.0; 3], 0.5);
    assert_eq!(black.map(f32::to_bits), [0.0_f32; 3].map(f32::to_bits));
}
#[test]
fn lab_and_uniform_chromaticity_roundtrips_preserve_known_neutrals() {
    for p in [[0.18; 3], [1.0; 3], [0.2, 0.4, 0.3]] {
        let q = math::from_lab(math::lab(p));
        let r = math::from_uv(p, math::uv(p));
        for k in 0..3 {
            assert!((q[k] - p[k]).abs() < 2e-5);
            assert!((r[k] - p[k]).abs() < 2e-5);
        }
    }
}
#[test]
fn error_diffusion_has_a_known_causal_quantization_result() {
    let source = vec![0.2; 4 * 3];
    let mut operation = op(Tool::LuminanceErrorDiffusion);
    operation.width = 0.25;
    let actual = filtered(&source, &[1.0; 4], 4, 1, &operation);
    for (pixel, value) in actual.chunks_exact(3).zip([0.25, 0.25, 0.0, 0.25]) {
        assert!((pixel[0] - value).abs() < 1e-6);
    }
}
#[test]
fn radial_fit_reduces_a_known_illumination_falloff_and_plane_fit_reduces_cast_slope() {
    let w = 24;
    let h = 18;
    let weights = vec![1.0; w * h];
    let mut source = Vec::new();
    for i in 0..w * h {
        let x = (i % w) as f32 / w as f32 + 0.5 / w as f32;
        let y = (i / w) as f32 / h as f32 + 0.5 / h as f32;
        let light = (0.2_f32.ln() + 0.8 * ((x - 0.5).powi(2) + (y - 0.5).powi(2))).exp();
        source.extend([light; 3]);
    }
    let mut operation = op(Tool::RadialLogIllumination);
    operation.source = [0.5, 0.5];
    operation.width = 1.0;
    let output = filtered(&source, &weights, w, h, &operation);
    let before = (luma(pixel(&source, 0)) / luma(pixel(&source, 9 * w + 12))).ln();
    let after = (luma(pixel(&output, 0)) / luma(pixel(&output, 9 * w + 12))).ln();
    assert!((after / before - 0.5).abs() < 0.01);
    let plane: Vec<_> = (0..w * h)
        .flat_map(|i| {
            from_chroma(
                [0.3; 3],
                [
                    0.15 * (i % w) as f32 / w as f32,
                    0.1 * (i / w) as f32 / h as f32,
                ],
            )
        })
        .collect();
    operation.tool = Tool::SpatialChromaPlane;
    let output = filtered(&plane, &weights, w, h, &operation);
    let old = chroma(pixel(&plane, w - 1))[0] - chroma(pixel(&plane, 0))[0];
    let new = chroma(pixel(&output, w - 1))[0] - chroma(pixel(&output, 0))[0];
    assert!((new / old - 0.5).abs() < 0.01);
}
#[test]
fn empirical_channel_recovery_learns_a_known_unclipped_relation() {
    let mut source = Vec::new();
    for i in 0..20 {
        let g = 0.08 + 0.013 * i as f32;
        let b = 0.03 + 0.009 * (i * 7 % 17) as f32;
        source.extend([0.05 + 2.0 * g + 0.5 * b, g, b]);
    }
    source.extend([0.9, 0.3, 0.2]);
    let mut operation = op(Tool::ChannelRegressionRecover);
    operation.pivot = 0.8;
    operation.width = 0.05;
    let output = filtered(&source, &[1.0; 21], 21, 1, &operation);
    assert!((output[60] - 0.75).abs() < 1e-4, "{}", output[60]);
}
#[test]
fn compositional_tint_and_pigment_identity_match_their_analytical_contracts() {
    let p = [0.2, 0.4, 0.3];
    let weights = [1.0];
    let mut operation = op(Tool::AitchisonTint);
    operation.width = 1.0;
    operation.target = [0.5, 0.2, 0.1];
    let actual = filtered(&p, &weights, 1, 1, &operation);
    let expected = with_luma(
        std::array::from_fn(|k| (p[k] * operation.target[k]).sqrt()),
        luma(p),
    );
    for k in 0..3 {
        assert!((actual[k] - expected[k]).abs() < 1e-6);
    }
    operation.tool = Tool::KubelkaMunkMix;
    operation.target = p;
    let actual = filtered(&p, &weights, 1, 1, &operation);
    for k in 0..3 {
        assert!((actual[k] - p[k]).abs() < 1e-6);
    }
}

#[test]
fn error_diffusion_does_not_inject_nonfinite_values_for_extreme_finite_hdr() {
    let source = [f32::MAX * 0.75, f32::MAX * 0.5, f32::MAX * 0.25];
    let mut operation = op(Tool::LuminanceErrorDiffusion);
    operation.width = 0.001;
    let output = filtered(&source, &[1.0], 1, 1, &operation);
    assert!(output
        .iter()
        .all(|value| value.is_finite() && *value >= 0.0));
}

#[test]
fn absent_selected_reference_patches_are_exact_identity() {
    let source = chart();
    let weights: Vec<_> = (0..24 * 18)
        .map(|i| if i % 24 < 12 { 1.0 } else { 0.0 })
        .collect();
    for tool in [
        Tool::GaussianPaletteTransport,
        Tool::PaletteSimplex,
        Tool::KmeansPaletteRecolor,
        Tool::FisherMonochrome,
        Tool::DichromaticSpecular,
    ] {
        let mut operation = op(tool);
        operation.source = [0.9, 0.5];
        let output = filtered(&source, &weights, 24, 18, &operation);
        assert_eq!(bits(&source), bits(&output), "{tool:?}");
    }
}

#[test]
fn undefined_target_chromaticity_and_hue_are_exact_identity() {
    let source = chart();
    let weights = vec![1.0; 24 * 18];
    for tool in [
        Tool::UvCastTranslate,
        Tool::HueDensityEqualize,
        Tool::MirrorHueFold,
    ] {
        let mut operation = op(tool);
        operation.target = [0.0; 3];
        assert_eq!(
            bits(&source),
            bits(&filtered(&source, &weights, 24, 18, &operation)),
            "{tool:?}"
        );
        if tool != Tool::UvCastTranslate {
            operation.target = [0.3; 3];
            assert_eq!(
                bits(&source),
                bits(&filtered(&source, &weights, 24, 18, &operation)),
                "{tool:?}"
            );
        }
    }
}

#[test]
fn diffusion_clamps_selected_signed_values_and_preserves_excluded_bits() {
    let source = [-0.1, 0.3, 0.2, -0.2, 0.4, 0.3];
    let operation = op(Tool::LuminanceErrorDiffusion);
    let result = filtered(&source, &[1.0, 0.0], 2, 1, &operation);
    assert!(result[..3]
        .iter()
        .all(|value| value.is_finite() && *value >= 0.0));
    assert_eq!(bits(&source[3..]), bits(&result[3..]));
}
#[test]
fn dotscreen_black_and_white_endpoints_are_independent_of_cell_phase() {
    let mut operation = op(Tool::AmplitudeDotscreen);
    operation.radius = 0.25;
    for (source, target) in [
        (vec![0.0; 9 * 7 * 3], 0.0_f32),
        (vec![1.0; 9 * 7 * 3], operation.pivot),
    ] {
        let result = filtered(&source, &[1.0; 9 * 7], 9, 7, &operation);
        assert!(result.iter().all(|value| (*value - target).abs() < 1e-6));
    }
}
#[test]
fn unavailable_reference_noop_also_preserves_signed_working_values() {
    let source = [-0.1, 0.3, 0.2];
    let mut operation = op(Tool::UvCastTranslate);
    operation.target = [0.0; 3];
    assert_eq!(
        bits(&source),
        bits(&filtered(&source, &[1.0], 1, 1, &operation))
    );
}
