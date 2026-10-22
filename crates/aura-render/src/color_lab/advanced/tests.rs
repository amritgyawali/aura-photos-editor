use super::*;
use crate::color_lab::{
    filtered,
    tests::{bits, chart, operation},
};

#[test]
fn all_new_methods_are_real_deterministic_and_neutral() {
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
            bits(&filtered(&source, &weights, 32, 24, &op)),
            "{tool:?}"
        );
        op.amount = 1.0;
        op.strength = 0.0;
        assert_eq!(
            bits(&source),
            bits(&filtered(&source, &weights, 32, 24, &op)),
            "{tool:?}"
        );
    }
}
#[test]
fn selection_exclusion_and_fractional_blending_are_exact() {
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
                    pixel(&x, i).map(f32::to_bits),
                    "{tool:?}"
                );
                assert_eq!(
                    pixel(&b, i).map(f32::to_bits),
                    pixel(&y, i).map(f32::to_bits),
                    "{tool:?}"
                );
            } else {
                assert!(
                    pixel(&x, i)
                        .iter()
                        .zip(pixel(&y, i))
                        .all(|(u, v)| (u - v).abs() < 1e-5),
                    "{tool:?} reads excluded colour"
                );
            }
        }
        let full = filtered(&a, &vec![1.0; 32 * 24], 32, 24, &op);
        let mut quarter = op.clone();
        quarter.amount *= 0.25;
        let partial = filtered(&a, &vec![1.0; 32 * 24], 32, 24, &quarter);
        assert!(
            full.iter()
                .zip(partial)
                .zip(&a)
                .all(|((p, q), v)| (q - (v + (p - v) * 0.25)).abs() < 1e-5),
            "{tool:?} invalid blend"
        );
    }
}
#[test]
fn dark_hdr_and_single_pixel_inputs_remain_finite() {
    for source in [[0.0; 3], [1e-12; 3], [8.0, 3.0, 0.4], [0.0, 1.0, 0.0]] {
        for tool in TOOLS {
            let op = operation(tool);
            assert!(
                filtered(&source, &[1.0], 1, 1, &op)
                    .iter()
                    .all(|v| v.is_finite() && *v >= 0.0),
                "{tool:?}"
            );
            assert_eq!(
                bits(&source),
                bits(&filtered(&source, &[0.0], 1, 1, &op)),
                "{tool:?}"
            );
        }
    }
}
#[test]
fn luminance_maps_are_monotone_and_chromatic_maps_hold_luminance() {
    let source = chart();
    let weights = vec![1.0; 32 * 24];
    for tool in &TOOLS[..8] {
        let mut op = operation(*tool);
        for width in [0.001, 0.2, 2.0] {
            op.width = width;
            let advanced = Advanced::new(&source, &weights, 32, 24, &op);
            let stats = Stats::new(&source, &weights);
            let mut previous = 0.0;
            for n in 0..300 {
                let y = n as f32 / 100.0;
                let mapped = luma(advanced.tone([y; 3], &op, &stats));
                assert!(mapped + 1e-5 >= previous, "{tool:?}, {width}, {y}");
                previous = mapped;
            }
        }
    }
    for tool in TOOLS
        .into_iter()
        .skip(8)
        .filter(|t| *t != Tool::LocalIlluminationFlatten)
    {
        let result = filtered(&source, &weights, 32, 24, &operation(tool));
        for (p, q) in source.chunks_exact(3).zip(result.chunks_exact(3)) {
            assert!(
                (luma([p[0], p[1], p[2]]) - luma([q[0], q[1], q[2]])).abs() < 1e-5,
                "{tool:?}"
            );
        }
    }
}
#[test]
fn gray_edge_uses_selected_neighbors_and_is_invariant_to_exposure() {
    let source = chart();
    let weights = vec![1.0; 32 * 24];
    let scaled: Vec<_> = source.iter().map(|v| v * 3.0).collect();
    let a = Measured::new(&source, &weights, 32, 24);
    let b = Measured::new(&scaled, &weights, 32, 24);
    for (left, right) in a.edge.iter().zip(b.edge) {
        assert!((left * 3.0 - right).abs() < 1e-5);
    }
}
#[test]
fn hue_pull_uses_shortest_arc_and_keeps_radius_in_gamut() {
    let mut op = operation(Tool::HueGeodesicPull);
    op.width = 2.0;
    let p = from_chroma([0.2; 3], [0.2, 0.03]);
    op.target = from_chroma([0.2; 3], [0.05, 0.2]);
    let src = [p, op.target].concat();
    let stats = Stats::new(&src, &[1.0; 2]);
    let advanced = Advanced::new(&src, &[1.0; 2], 2, 1, &op);
    let q = advanced.chromatic(p, &op, &stats, p, op.target);
    assert!((length(chroma(p)) - length(chroma(q))).abs() < 1e-6);
    assert!(chroma(q)[1] > chroma(p)[1]);
    assert!((wrapped(3.1 - (-3.1))).abs() < 0.1);
}
#[test]
fn actual_saved_new_methods_route_through_masks_and_disabled_operations() {
    let mut recipe = aura_recipe::fixtures::reference();
    recipe.extra.clear();
    recipe.extra.insert(aura_recipe::local_masks::KEY.into(),serde_json::json!([{
        "id":"left","name":"Selected","enabled":true,"amount":1.0,
        "components":[{"mode":"add","source":{"type":"radial","centre":[0.5,0.5],"radii":[1.0,1.0],"angle":0,"feather":0}}],"params":{}
    }]));
    let source = chart();
    for tool in TOOLS {
        let mut op = operation(tool);
        aura_recipe::color_lab::write(&mut recipe, &[op.clone()]).unwrap();
        let mut result = source.clone();
        crate::color_lab::apply(&mut result, 32, 24, &recipe, None);
        assert_ne!(bits(&source), bits(&result), "{tool:?}");
        op.enabled = false;
        aura_recipe::color_lab::write(&mut recipe, &[op]).unwrap();
        result = source.clone();
        crate::color_lab::apply(&mut result, 32, 24, &recipe, None);
        assert_eq!(bits(&source), bits(&result), "{tool:?}");
    }
}

#[test]
fn tied_chroma_ranks_use_the_population_midpoint() {
    let p = from_chroma([0.2; 3], [0.2, 0.1]);
    let source = [p; 4].concat();
    let mut op = operation(Tool::ChromaRadiusEqualize);
    op.amount = 1.0;
    op.width = 0.2;
    let result = filtered(&source, &[1.0; 4], 2, 2, &op);
    assert!((length(chroma(pixel(&result, 0))) - op.width * 0.5).abs() < 1e-6);
}

fn measured_skew(values: &[f32]) -> f64 {
    let values: Vec<_> = values
        .chunks_exact(3)
        .map(|p| f64::from(luma([p[0], p[1], p[2]])))
        .collect();
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
    values.iter().map(|v| (v - mean).powi(3)).sum::<f64>()
        / values.len() as f64
        / variance.powf(1.5)
}

#[test]
fn skew_correction_reduces_both_positive_and_negative_luminance_asymmetry() {
    for reverse in [false, true] {
        let source: Vec<_> = (0..1025)
            .flat_map(|i| {
                let y = (i as f32 / 1024.0).powi(2);
                [if reverse { 1.0 - y } else { y }; 3]
            })
            .collect();
        let before = measured_skew(&source);
        assert!(before.abs() > 0.6);
        for width in [0.1, 1.0, 2.0] {
            let mut op = operation(Tool::LuminanceSkewCorrect);
            op.amount = 1.0;
            op.width = width;
            let result = filtered(&source, &vec![1.0; 1025], 41, 25, &op);
            let after = measured_skew(&result);
            assert!(
                after.abs() < before.abs(),
                "reverse={reverse}, width={width}, {before} -> {after}"
            );
        }
    }
}

#[test]
fn empty_tonal_zones_do_not_invent_cast_for_constant_chromaticity() {
    let source: Vec<_> = (0..12)
        .flat_map(|i| {
            from_chroma(
                [match i % 3 {
                    0 => 0.2,
                    1 => 0.3,
                    _ => 0.4,
                }; 3],
                [0.3, -0.2],
            )
        })
        .collect();
    let mut op = operation(Tool::TonalCastEqualize);
    op.amount = 1.0;
    let result = filtered(&source, &[1.0; 12], 4, 3, &op);
    for (before, after) in source.iter().zip(result) {
        assert!((before - after).abs() < 1e-6);
    }
}
