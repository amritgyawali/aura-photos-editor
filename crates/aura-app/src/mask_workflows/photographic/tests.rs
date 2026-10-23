#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::cast_precision_loss
)]
use super::*;
fn plane(alpha: &[u8], width: usize) -> Plane {
    Plane {
        width,
        height: alpha.len() / width,
        alpha: alpha.to_vec(),
    }
}
fn measured(action: Action, rgb: &[[u8; 3]], width: usize, options: &Options) -> Vec<u8> {
    let bytes: Vec<_> = rgb.iter().flatten().copied().collect();
    let frame = Frame::from_rgb(&bytes, width, rgb.len() / width).expect("valid frame");
    let mut p = plane(&vec![0; rgb.len()], width);
    features::apply(&mut p, action, options, &frame, &CancelToken::new()).expect("measurement");
    p.alpha
}
#[test]
fn hue_wraparound_excludes_neutrals_and_opponent_and_neutral_metrics_differ() {
    let rgb = [[255, 0, 0], [255, 0, 15], [0, 0, 255], [128, 128, 128]];
    let mut o = Options {
        angle: 0.0,
        sweep: 10.0,
        tolerance: 0.01,
        range: [0.9, 1.0],
        ..Options::default()
    };
    let hue = measured(Action::HueArc, &rgb, 4, &o);
    assert_eq!(hue[0], 255);
    assert!(hue[1] > 0);
    assert_eq!(hue[2], 0);
    assert_eq!(hue[3], 0);
    let neutral = measured(Action::NeutralAxis, &rgb, 4, &o);
    assert_eq!(neutral[3], 255);
    assert_eq!(neutral[0], 0);
    let chroma = measured(Action::OpponentChroma, &rgb, 4, &o);
    assert_eq!(chroma[0], 255);
    assert_eq!(chroma[3], 0);
    o.range = [0.95, 1.0];
    let warmth = measured(Action::TemperatureBalance, &rgb, 4, &o);
    assert_eq!(warmth[0], 255);
    assert_eq!(warmth[2], 0);
    let clipping = measured(
        Action::ChannelClipping,
        &[[0, 0, 255], [128, 128, 128]],
        2,
        &o,
    );
    assert_eq!(clipping, vec![255, 0]);
}
#[test]
fn local_statistics_handle_flat_ties_and_separate_contrast_from_entropy() {
    let flat = [[128, 128, 128]; 25];
    let mut o = Options {
        radius: 1,
        range: [0.49, 0.51],
        tolerance: 0.01,
        ..Options::default()
    };
    assert_eq!(measured(Action::LocalRank, &flat, 5, &o), vec![255; 25]);
    assert_eq!(
        measured(Action::LocalContrastPolarity, &flat, 5, &o),
        vec![255; 25]
    );
    o.range = [0.0, 0.0];
    assert_eq!(measured(Action::LocalEntropy, &flat, 5, &o), vec![255; 25]);
    assert_eq!(
        measured(Action::LaplacianDetail, &flat, 5, &o),
        vec![255; 25]
    );
    let mut impulse = [[0, 0, 0]; 25];
    impulse[12] = [255, 255, 255];
    o.range = [0.7, 1.0];
    let polarity = measured(Action::LocalContrastPolarity, &impulse, 5, &o);
    assert_eq!(polarity[12], 255);
    assert_eq!(polarity[0], 0);
    let detail = measured(Action::LaplacianDetail, &impulse, 5, &o);
    assert_eq!(detail[12], 255);
    let specular = measured(Action::SpecularResidual, &impulse, 5, &o);
    assert_eq!(specular[12], 255);
    assert_eq!(specular[0], 0);
}
#[test]
fn oriented_edges_tensor_and_corners_measure_different_structures() {
    let stripe: Vec<_> = (0..49)
        .map(|i| if i % 7 >= 3 { [255; 3] } else { [0; 3] })
        .collect();
    let mut o = Options {
        radius: 1,
        range: [0.8, 1.0],
        tolerance: 0.01,
        ..Options::default()
    };
    let horizontal = measured(Action::EdgeOrientation, &stripe, 7, &o);
    assert_eq!(horizontal[3 * 7 + 3], 255);
    o.angle = 90.0;
    assert!(measured(Action::EdgeOrientation, &stripe, 7, &o)
        .iter()
        .all(|v| *v == 0));
    assert_eq!(
        measured(Action::TensorCoherence, &stripe, 7, &o)[3 * 7 + 3],
        255
    );
    let corners: Vec<_> = (0..49)
        .map(|i| {
            if i % 7 >= 3 && i / 7 >= 3 {
                [255; 3]
            } else {
                [0; 3]
            }
        })
        .collect();
    o.range = [0.1, 1.0];
    let response = measured(Action::HarrisCorners, &corners, 7, &o);
    assert!(response[3 * 7 + 3] > 0);
    assert_eq!(
        measured(Action::HarrisCorners, &stripe, 7, &o)[3 * 7 + 3],
        0
    );
    let rgb = [[255, 0, 0], [0, 76, 0], [255, 0, 0]];
    assert!(measured(Action::ChromaticBoundary, &rgb, 3, &o)[0] > 0);
    let mut noisy = [[10, 10, 10]; 9];
    noisy[4] = [50, 0, 0];
    assert!(measured(Action::ShadowChromaDeviation, &noisy, 3, &o)[4] > 0);
}
#[test]
fn authored_shapes_and_seed_fields_create_distinct_nonempty_geometry() {
    let mut o = Options {
        tolerance: 0.02,
        corner: 0.5,
        ..Options::default()
    };
    for action in [
        Action::SuperellipseMarquee,
        Action::BezierRibbon,
        Action::StarMarquee,
        Action::SeedField,
    ] {
        let mut p = plane(&[0; 400], 20);
        authored::apply(&mut p, action, &o, &CancelToken::new()).expect("shape");
        assert!(p.alpha.iter().any(|v| *v > 0), "{action:?}");
        assert!(p.alpha.iter().any(|v| *v < 255), "{action:?}");
    }
    let mut p = plane(&[0; 400], 20);
    o.vertices = vec![[0.1, 0.5], [0.9, 0.5]];
    authored::apply(&mut p, Action::SeedField, &o, &CancelToken::new()).expect("seed field");
    assert!(p.alpha[10 * 20 + 2] > p.alpha[10 * 20 + 17]);
    assert!(authored::apply(&mut p, Action::BezierRibbon, &o, &CancelToken::new()).is_err());
}
#[test]
fn coverage_resampling_preserves_identity_and_symmetry_removes_unpaired_coverage() {
    let original = vec![0, 50, 100, 150, 200, 255, 10, 80, 160];
    let mut p = plane(&original, 3);
    let o = Options {
        radius: 1,
        ..Options::default()
    };
    coverage::apply(&mut p, Action::PeriodicSelection, &o, &CancelToken::new()).expect("identity");
    assert_eq!(p.alpha, original);
    let mut p = plane(&[0, 0, 0, 255, 255, 0, 0, 0, 0], 3);
    let o = Options { angle: 90.0, ..o };
    coverage::apply(&mut p, Action::BilateralSymmetry, &o, &CancelToken::new())
        .expect("reflection");
    assert_eq!(p.alpha, vec![0, 0, 0, 0, 255, 0, 0, 0, 0]);
    let mut p = plane(&[255; 400], 20);
    coverage::apply(&mut p, Action::PolarSelection, &o, &CancelToken::new()).expect("polar");
    assert_eq!(p.alpha[10 * 20 + 10], 255);
}
#[test]
fn quantiles_keep_background_harmonic_holds_boundaries_and_axis_strip_follows_long_axis() {
    let mut p = plane(&[0, 10, 10, 200, 255], 5);
    coverage::apply(
        &mut p,
        Action::CoverageQuantiles,
        &Options::default(),
        &CancelToken::new(),
    )
    .expect("quantiles");
    assert_eq!(p.alpha, vec![0, 64, 64, 159, 223]);
    let mut p = plane(&[255, 255, 255, 255, 0, 255, 255, 255, 255], 3);
    let o = Options {
        bounds: [0.0, 0.0, 1.0, 1.0],
        ..Options::default()
    };
    coverage::apply(&mut p, Action::HarmonicPatch, &o, &CancelToken::new()).expect("harmonic");
    assert_eq!(p.alpha, vec![255; 9]);
    let mut p = plane(&[255; 21], 7);
    coverage::apply(&mut p, Action::RegionAxisStrip, &o, &CancelToken::new()).expect("axis");
    assert_eq!(&p.alpha[7..14], &[255; 7]);
    assert!(p.alpha[..7].iter().all(|v| *v == 0));
}
#[test]
fn cancellation_never_commits_a_partial_measurement_or_coverage_plane() {
    let cancel = CancelToken::new();
    cancel.cancel();
    let original = vec![17; 9];
    let mut p = plane(&original, 3);
    let frame = Frame::from_rgb(&[128; 27], 3, 3).expect("frame");
    assert!(features::apply(
        &mut p,
        Action::LocalEntropy,
        &Options::default(),
        &frame,
        &cancel
    )
    .is_err());
    assert_eq!(p.alpha, original);
    assert!(coverage::apply(&mut p, Action::HarmonicPatch, &Options::default(), &cancel).is_err());
    assert_eq!(p.alpha, original);
    assert!(authored::apply(&mut p, Action::StarMarquee, &Options::default(), &cancel).is_err());
    assert_eq!(p.alpha, original);
}
