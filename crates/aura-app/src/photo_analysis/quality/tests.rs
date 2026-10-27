use super::*;

fn tools(w: usize, h: usize, pixel: impl Fn(usize, usize) -> [f32; 3]) -> Vec<Diagnostic> {
    let pixels: Vec<_> = (0..h)
        .flat_map(|y| {
            (0..w).map({
                let pixel = &pixel;
                move |x| pixel(x, y)
            })
        })
        .collect();
    let luma: Vec<_> = pixels
        .iter()
        .map(|p| p[0] * 0.2627 + p[1] * 0.678 + p[2] * 0.0593)
        .collect();
    measure(&pixels, &luma, w, h)
}
fn values<'a>(tools: &'a [Diagnostic], id: &str) -> &'a [f32] {
    &tools
        .iter()
        .find(|t| t.id == id)
        .expect("registered diagnostic")
        .values
}

#[test]
fn twenty_five_distinct_reports_are_finite_deterministic_and_read_only() {
    let pixels = vec![[0.18; 3]; 63 * 47];
    let input = pixels.clone();
    let luma = vec![0.18; pixels.len()];
    let result = measure(&pixels, &luma, 63, 47);
    assert_eq!(pixels, input);
    assert_eq!(result.len(), 25);
    let ids: std::collections::BTreeSet<_> = result.iter().map(|t| &t.id).collect();
    assert_eq!(ids.len(), 25);
    for tool in &result {
        assert_eq!(tool.values.len(), tool.width * tool.height);
        assert!(tool.values.iter().all(|v| v.is_finite()));
        assert!(tool.values.len() <= 256);
    }
    assert_eq!(
        serde_json::to_vec(&result).expect("encode"),
        serde_json::to_vec(&measure(&pixels, &luma, 63, 47)).expect("encode")
    );
    for id in [
        "channel-information",
        "chroma-covariance",
        "tonal-moments",
        "edge-overshoot",
        "haar-energy",
        "mirror-residual",
    ] {
        assert!(values(&result, id).iter().all(|v| v.abs() < 1e-5), "{id}");
    }
}

#[test]
fn information_and_moments_match_balanced_two_level_population() {
    let result = tools(64, 32, |x, _| [if x < 32 { 0.1 } else { 0.9 }; 3]);
    assert!(values(&result, "channel-information")
        .iter()
        .all(|v| (*v - 1.0).abs() < 1e-5));
    let moments = values(&result, "tonal-moments");
    assert!(moments[0].abs() < 1e-5);
    assert!((moments[1] + 2.0).abs() < 1e-5);
    assert!((values(&result, "palette-cube").iter().sum::<f32>() - 100.0).abs() < 0.01);
    assert!(values(&result, "edge-effective-width")[0] > 0.0);
    assert!(values(&result, "edge-overshoot")
        .iter()
        .all(|v| v.abs() < 1e-6));
}

#[test]
fn known_composition_locates_light_and_measures_symmetry() {
    let result = tools(64, 48, |x, y| {
        [if (16..48).contains(&x) && (12..36).contains(&y) {
            0.9
        } else {
            0.0
        }; 3]
    });
    assert!(values(&result, "light-centroid")
        .iter()
        .all(|v| (*v - 0.5).abs() < 1e-5));
    assert!(values(&result, "mirror-residual")
        .iter()
        .all(|v| v.abs() < 1e-5));
    assert!((values(&result, "bright-components")[0] - 1.0).abs() < 1e-5);
    assert!((values(&result, "bright-components")[1] - 25.0).abs() < 1e-5);
    let right = tools(64, 48, |x, _| [if x >= 48 { 1.0 } else { 0.0 }; 3]);
    assert!(values(&right, "light-centroid")[0] > 0.8);
    assert!(values(&right, "mirror-residual")[0] > 0.4);
    assert!(values(&right, "border-light-energy")[1] > 0.9);
}

#[test]
fn alternating_pattern_has_negative_one_lag_correlation_and_haar_energy() {
    let result = tools(64, 48, |x, _| [if x % 2 == 0 { 0.2 } else { 0.8 }; 3]);
    assert!(values(&result, "luma-autocorrelation")[0] < -0.999);
    assert!(values(&result, "luma-autocorrelation")[1] > 0.999);
    assert!(values(&result, "haar-energy")[0] > 0.1);
    assert!(values(&result, "residual-correlation")[0] < -0.9);
}

#[test]
fn tiny_black_and_high_dynamic_range_frames_remain_bounded() {
    for (w, h) in [(1, 1), (1, 9), (7, 1), (3, 2)] {
        let black = tools(w, h, |_, _| [0.0; 3]);
        assert_eq!(black.len(), 25);
        assert!(black.iter().flat_map(|t| &t.values).all(|v| v.is_finite()));
        assert_eq!(
            black
                .iter()
                .find(|t| t.id == "light-centroid")
                .expect("centroid")
                .samples,
            0
        );
        let hdr = tools(w, h, |x, y| [if (x + y) % 2 == 0 { 1e6 } else { -1e6 }; 3]);
        assert!(hdr.iter().flat_map(|t| &t.values).all(|v| v.is_finite()));
    }
}

#[test]
fn odd_preview_percentages_close_and_haar_omits_incomplete_blocks() {
    let result = tools(511, 511, |_, _| [0.18; 3]);
    assert!((values(&result, "palette-cube").iter().sum::<f32>() - 100.0).abs() < 1e-5);
    let tiny = tools(3, 3, |x, y| [if x == y { 1.0 } else { 0.0 }; 3]);
    assert!(values(&tiny, "haar-energy")[1..]
        .iter()
        .all(|v| v.abs() < 1e-8));
    // Alternation inside each 2x2 quadrant cancels before the coarser transform.
    let fine = tools(16, 16, |x, _| [if x % 2 == 0 { 0.0 } else { 1.0 }; 3]);
    assert!(values(&fine, "haar-energy")[1..]
        .iter()
        .all(|v| v.abs() < 1e-8));
}
