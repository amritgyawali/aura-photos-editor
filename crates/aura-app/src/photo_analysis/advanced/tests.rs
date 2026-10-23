use super::*;

fn run(w: usize, h: usize, f: impl Fn(usize, usize) -> [f32; 3]) -> Vec<Diagnostic> {
    let mut p = Vec::new();
    for row in 0..h {
        for x in 0..w {
            p.push(f(x, row));
        }
    }
    let y = p
        .iter()
        .map(|v| v[0] * 0.2627 + v[1] * 0.678 + v[2] * 0.0593)
        .collect::<Vec<_>>();
    measure(&p, &y, w, h)
}
fn get<'a>(tools: &'a [Diagnostic], id: &str) -> &'a [f32] {
    &tools
        .iter()
        .find(|v| v.id == id)
        .expect("measurement")
        .values
}
fn close(a: f32, b: f32) {
    assert!((a - b).abs() < 1e-4, "{a} != {b}");
}

#[test]
fn all_twenty_five_measurements_are_distinct_finite_and_deterministic_at_boundaries() {
    for (w, h) in [(1, 1), (1, 17), (19, 1), (63, 47)] {
        for signal in [-1e6, 0.0, 0.18, 1e6] {
            let a = run(w, h, |_, _| [signal; 3]);
            let b = run(w, h, |_, _| [signal; 3]);
            assert_eq!(a.len(), 25);
            assert_eq!(
                a.iter()
                    .map(|v| &v.id)
                    .collect::<std::collections::BTreeSet<_>>()
                    .len(),
                25
            );
            assert_eq!(
                serde_json::to_vec(&a).expect("json"),
                serde_json::to_vec(&b).expect("json")
            );
            for tool in a {
                assert_eq!(tool.width * tool.height, tool.values.len());
                assert!(tool.values.iter().all(|v| v.is_finite()), "{}", tool.id);
            }
        }
    }
}
#[test]
fn uniform_texture_has_known_zero_complexity_and_exact_cooccurrence() {
    let a = run(32, 32, |_, _| [0.25; 3]);
    for id in ["ordinal-texture-entropy", "sample-texture-entropy"] {
        close(get(&a, id)[0], 0.0);
        close(get(&a, id)[1], 1.0);
    }
    close(get(&a, "quantized-run-lengths")[7], 1.0);
    for (actual, expected) in get(&a, "cooccurrence-texture")
        .iter()
        .zip([0.0, 1.0, 1.0, 0.0])
    {
        close(*actual, expected);
    }
    for id in [
        "bright-mass-lacunarity",
        "harris-corner-response",
        "local-plane-residual",
        "line-spectral-flatness",
        "neighbour-moran-index",
        "robust-neighbour-roughness",
    ] {
        assert!(get(&a, id).iter().all(|v| v.abs() < 1e-6), "{id}");
    }
}
#[test]
fn affine_planes_fit_and_periodic_texture_separates_from_monotone_order() {
    let a = run(64, 48, |x, row| {
        [(0.02 * x as f32 + 0.01 * row as f32).sinh(); 3]
    });
    assert!(get(&a, "local-plane-residual")
        .iter()
        .all(|v| v.abs() < 1e-4));
    close(get(&a, "ordinal-texture-entropy")[0], 0.0);
    close(get(&a, "ordinal-texture-entropy")[1], 0.0);
    assert!(get(&a, "neighbour-moran-index")[0] > 0.9);
    let checker = run(32, 32, |x, row| {
        [if (x + row) % 2 == 0 { 0.1 } else { 0.9 }; 3]
    });
    close(get(&checker, "cooccurrence-texture")[0], 225.0);
    assert!(get(&checker, "thumbnail-alias-sensitivity")
        .iter()
        .all(|v| *v > 0.2));
    assert!(get(&checker, "line-spectral-flatness")
        .iter()
        .all(|v| *v < 0.01));
}
#[test]
fn colour_candidates_ratios_and_unsupported_hues_have_known_answers() {
    let a = run(32, 32, |_, _| [1.0, 2.0, 4.0]);
    let ratios = get(&a, "log-ratio-neutrality");
    for v in &ratios[..3] {
        close(*v, -2.0_f32.ln());
    }
    for v in &ratios[3..] {
        close(*v, 2.0_f32.ln());
    }
    for (actual, expected) in
        get(&a, "white-patch-illuminant")
            .iter()
            .zip([1.0 / 7.0, 2.0 / 7.0, 4.0 / 7.0])
    {
        close(*actual, expected);
    }
    for id in [
        "gray-edge-illuminant",
        "local-hue-winding",
        "chroma-light-information",
        "patch-chroma-dispersion",
    ] {
        assert!(get(&a, id).iter().all(|v| v.abs() < 1e-5), "{id}");
    }
    close(get(&a, "hue-gap-coverage")[0], 1.0 / 36.0);
    close(get(&a, "hue-gap-coverage")[1], 35.0 / 36.0);
    let neutral = run(16, 16, |_, _| [0.25; 3]);
    close(get(&neutral, "hue-gap-coverage")[0], 0.0);
    close(get(&neutral, "hue-gap-coverage")[1], 1.0);
    close(get(&neutral, "opponent-colourfulness")[0], 0.0);
    let ramp = run(64, 32, |x, _| {
        let v = x as f32 / 64.0;
        [v, 2.0 * v, 4.0 * v]
    });
    for v in get(&ramp, "channel-tail-dependence") {
        close(*v, 1.0);
    }
    close(get(&ramp, "gray-edge-illuminant").iter().sum(), 1.0);
}
#[test]
fn clipping_graph_and_robust_tail_support_count_real_samples() {
    let a = run(16, 16, |x, row| {
        [if row == 2 && (x == 2 || x == 12) {
            1.0
        } else {
            0.25
        }; 3]
    });
    let v = get(&a, "clipping-component-support");
    close(v[0], 2.0);
    close(v[1], 1.0);
    close(v[2], 0.5);
    close(get(&a, "tukey-tail-support")[0], 0.0);
    close(get(&a, "tukey-tail-support")[1], 2.0 / 256.0);
    assert!(get(&a, "edge-clipping-overlap").iter().any(|v| *v > 0.9));
    let b = run(32, 32, |_, _| [0.25; 3]);
    for v in get(&b, "patch-mean-variance-fit") {
        close(*v, 0.0);
    }
    for v in get(&b, "robust-strip-drift") {
        close(*v, 0.25_f32.asinh());
    }
}
#[test]
fn lagged_correlation_centres_only_valid_pairs_in_existing_measurement() {
    let rgb: Vec<_> = (0..16).flat_map(|x| [x as f32 / 16.0; 3]).collect();
    let a = crate::photo_analysis::measure(&rgb, 16, 1).expect("ramp report");
    for v in get(&a, "luma-autocorrelation") {
        close(*v, 1.0);
    }
    let rgb = vec![0.25; 48];
    let a = crate::photo_analysis::measure(&rgb, 16, 1).expect("flat report");
    for v in get(&a, "luma-autocorrelation") {
        close(*v, 0.0);
    }
}
