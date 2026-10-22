#[cfg(test)]
use super::*;

#[cfg(test)]
fn run(w: usize, h: usize, f: impl Fn(usize, usize) -> [f32; 3]) -> Vec<Diagnostic> {
    let mut p = Vec::new();
    for row in 0..h {
        for x in 0..w {
            p.push(f(x, row));
        }
    }
    let y: Vec<_> = p
        .iter()
        .map(|v| v[0] * 0.2627 + v[1] * 0.678 + v[2] * 0.0593)
        .collect();
    measure(&p, &y, w, h)
}
#[cfg(test)]
fn values<'a>(tools: &'a [Diagnostic], id: &str) -> &'a [f32] {
    &tools.iter().find(|t| t.id == id).expect("tool").values
}

#[test]
fn twenty_five_reports_are_distinct_finite_and_deterministic_in_extreme_domains() {
    for (w, h) in [(1, 1), (1, 16), (17, 1), (63, 47), (32, 32)] {
        for signal in [-1e6, 0.0, 0.18, 1e6] {
            let tools = run(w, h, |_, _| [signal; 3]);
            assert_eq!(tools.len(), 25);
            assert_eq!(
                tools
                    .iter()
                    .map(|t| &t.id)
                    .collect::<std::collections::BTreeSet<_>>()
                    .len(),
                25
            );
            for tool in &tools {
                assert_eq!(tool.values.len(), tool.width * tool.height);
                assert!(tool.values.iter().all(|v| v.is_finite()), "{}", tool.id);
            }
            assert_eq!(
                serde_json::to_vec(&tools).expect("json"),
                serde_json::to_vec(&run(w, h, |_, _| [signal; 3])).expect("json")
            );
            assert!(values(&tools, "normalized-total-variation")
                .iter()
                .all(|v| v.abs() < 1e-5));
            assert!(values(&tools, "dct-energy").iter().all(|v| v.abs() < 1e-6));
        }
    }
}
#[test]
fn statistical_landmarks_and_range_energy_have_known_answers() {
    let tools = run(32, 32, |x, _| [if x < 16 { 0.0 } else { 1.0 }; 3]);
    assert!((values(&tools, "luminance-inequality")[0] - 0.5).abs() < 1e-5);
    assert!((values(&tools, "robust-tone-quantiles")[4] - 0.5).abs() < 1e-5);
    assert!((values(&tools, "median-deviation")[0] - 0.5).abs() < 1e-5);
    assert!(values(&tools, "hue-concentration")
        .iter()
        .all(|v| v.abs() < 1e-5));
    let hdr = run(8, 8, |_, _| [2.0, -0.5, 0.5]);
    assert_eq!(values(&hdr, "gamut-excursion-energy"), &[1.0, 0.5, 0.0]);
}
#[test]
fn directional_pattern_separates_spatial_mechanisms() {
    let ramp = run(64, 48, |x, _| [(x as f32 / 64.0).sinh(); 3]);
    assert!((values(&ramp, "illumination-plane")[0] - 1.0).abs() < 1e-4);
    assert!(values(&ramp, "illumination-plane")[1].abs() < 1e-5);
    assert!(values(&ramp, "illumination-curvature")
        .iter()
        .all(|v| v.abs() < 1e-4));
    assert!(values(&ramp, "structure-coherence")
        .iter()
        .all(|v| *v > 0.99));
    let stripes = run(64, 48, |x, _| [if x % 2 == 0 { 0.1 } else { 0.8 }; 3]);
    assert!(values(&stripes, "dct-energy")[7] > 0.1);
    assert!(values(&stripes, "normalized-total-variation")[0] > 0.5);
    let coloured = run(64, 48, |x, _| {
        if x % 4 < 2 {
            [0.8, 0.2, 0.1]
        } else {
            [0.1, 0.3, 0.8]
        }
    });
    assert!(values(&coloured, "local-colour-jumps")
        .iter()
        .any(|v| *v > 0.1));
    assert!(values(&coloured, "colour-variogram")[0] > 0.1);
}
