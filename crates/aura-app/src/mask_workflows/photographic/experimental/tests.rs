#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::float_cmp
)]
use super::*;
fn plane(alpha: Vec<u8>, w: usize) -> Plane {
    Plane {
        width: w,
        height: alpha.len() / w,
        alpha,
    }
}
fn run(
    action: Action,
    alpha: &[u8],
    w: usize,
    o: &Options,
    point: [f32; 2],
    frame: Option<&Frame>,
) -> Vec<u8> {
    let mut p = plane(alpha.to_vec(), w);
    apply(&mut p, action, o, point, frame, &CancelToken::new()).expect("valid operator");
    p.alpha
}
fn frame(rgb: &[[u8; 3]], w: usize) -> Frame {
    Frame::from_rgb(
        &rgb.iter().flatten().copied().collect::<Vec<_>>(),
        w,
        rgb.len() / w,
    )
    .expect("source frame")
}
#[test]
fn all_twenty_five_operators_are_deterministic_bounded_and_source_isolated() {
    let alpha: Vec<u8> = (0..81)
        .map(|i| {
            if i % 9 >= 2 && i % 9 <= 6 && i / 9 >= 2 && i / 9 <= 6 {
                200
            } else {
                0
            }
        })
        .collect();
    let rgb: Vec<_> = (0..81)
        .map(|i| {
            [3, 7, 11].map(|factor| u8::try_from(i * factor % 256).expect("byte after modulo"))
        })
        .collect();
    let source = frame(&rgb, 9);
    let frozen_rgb = source.rgb.clone();
    let frozen_luminance = source.luminance.clone();
    let options = Options {
        radius: 1,
        min_area: 3,
        angle: 30.0,
        ..Options::default()
    };
    assert_eq!(ACTIONS.len(), 25);
    for action in ACTIONS {
        let first = run(action, &alpha, 9, &options, [0.5, 0.5], Some(&source));
        assert_eq!(first.len(), 81, "{action:?}");
        assert_eq!(
            first,
            run(action, &alpha, 9, &options, [0.5, 0.5], Some(&source)),
            "{action:?}"
        );
        let p = plane(alpha.clone(), 9);
        assert_eq!(p.alpha, alpha, "{action:?} changed the original input");
    }
    assert_eq!(source.rgb, frozen_rgb);
    assert_eq!(source.luminance, frozen_luminance);
}
#[test]
fn every_operator_handles_single_cell_and_transparent_coverage_without_panicking() {
    let f = frame(&[[0, 0, 0]], 1);
    for action in ACTIONS {
        let mut p = plane(vec![0], 1);
        let original = p.alpha.clone();
        let result = apply(
            &mut p,
            action,
            &Options::default(),
            [1.0, 1.0],
            Some(&f),
            &CancelToken::new(),
        );
        if action == Action::GeodesicRings {
            assert!(result.is_err());
            assert_eq!(p.alpha, original);
        } else {
            assert!(result.is_ok(), "{action:?}");
            assert_eq!(p.alpha.len(), 1);
        }
    }
}
#[test]
fn cancellation_invalid_geometry_finite_and_frame_checks_preserve_coverage() {
    let mut p = plane(vec![128; 9], 3);
    let original = p.alpha.clone();
    let f = frame(&[[128, 128, 128]; 9], 3);
    let cancel = CancelToken::new();
    cancel.cancel();
    for action in ACTIONS {
        assert!(apply(
            &mut p,
            action,
            &Options::default(),
            [0.5, 0.5],
            Some(&f),
            &cancel
        )
        .is_err());
        assert_eq!(p.alpha, original);
        let o = Options {
            tolerance: f32::NAN,
            ..Options::default()
        };
        assert!(apply(
            &mut p,
            action,
            &o,
            [0.5, 0.5],
            Some(&f),
            &CancelToken::new()
        )
        .is_err());
        assert_eq!(p.alpha, original);
    }
    for bad in [
        Options {
            bounds: [0.7, 0.0, 0.2, 1.0],
            ..Options::default()
        },
        Options {
            radius: 0,
            ..Options::default()
        },
        Options {
            angle: f32::INFINITY,
            ..Options::default()
        },
        Options {
            range: [0.8, 0.2],
            ..Options::default()
        },
        Options {
            radius: 9,
            ..Options::default()
        },
    ] {
        assert!(apply(
            &mut p,
            Action::LocalOtsu,
            &bad,
            [0.5, 0.5],
            Some(&f),
            &CancelToken::new()
        )
        .is_err());
        assert_eq!(p.alpha, original);
    }
    let mismatch = frame(&[[0, 0, 0]], 1);
    assert!(apply(
        &mut p,
        Action::LabSampleDistance,
        &Options::default(),
        [0.5, 0.5],
        Some(&mismatch),
        &CancelToken::new()
    )
    .is_err());
    let mut nonfinite = frame(&[[0, 0, 0]; 9], 3);
    nonfinite.rgb[0][0] = f32::NAN;
    assert!(apply(
        &mut p,
        Action::LabSampleDistance,
        &Options::default(),
        [0.5, 0.5],
        Some(&nonfinite),
        &CancelToken::new()
    )
    .is_err());
    assert_eq!(p.alpha, original);
}

#[test]
fn empty_frame_rejects_all_experimental_actions() {
    let mut empty = Plane {
        width: 0,
        height: 0,
        alpha: vec![],
    };
    for action in ACTIONS {
        assert!(apply(
            &mut empty,
            action,
            &Options::default(),
            [0.5, 0.5],
            None,
            &CancelToken::new()
        )
        .is_err());
    }
}
#[test]
fn lab_and_covariance_distances_identify_sample_colour_and_reject_distant_colours() {
    let f = frame(
        &[
            [32, 32, 32],
            [32, 32, 32],
            [32, 32, 32],
            [250, 10, 10],
            [255, 255, 255],
        ],
        5,
    );
    let o = Options {
        radius: 1,
        tolerance: 0.1,
        ..Options::default()
    };
    for action in [Action::LabSampleDistance, Action::MahalanobisColour] {
        let a = run(action, &[0; 5], 5, &o, [0.0, 0.5], Some(&f));
        assert_eq!(a[0], 255);
        assert_eq!(a[1], 255);
        assert_eq!(a[3], 0);
        assert_eq!(a[4], 0);
    }
}
#[test]
fn colour_line_residual_excludes_a_shading_ramp_but_selects_off_axis_colour() {
    let f = frame(
        &[
            [20, 20, 20],
            [60, 60, 60],
            [100, 100, 100],
            [150, 150, 150],
            [255, 0, 0],
        ],
        5,
    );
    let o = Options {
        radius: 1,
        range: [0.15, 1.0],
        tolerance: 0.001,
        ..Options::default()
    };
    let a = run(
        Action::ColourLineResidual,
        &[0; 5],
        5,
        &o,
        [0.2, 0.5],
        Some(&f),
    );
    assert_eq!(a[0], 0);
    assert_eq!(a[3], 0);
    assert!(a[4] > 200);
}
#[test]
fn geodesic_colour_cannot_cross_an_expensive_colour_wall() {
    let mut rgb = vec![[30, 30, 30]; 21];
    for y in 0..3 {
        rgb[y * 7 + 3] = [255, 255, 255];
    }
    let f = frame(&rgb, 7);
    let o = Options {
        radius: 1,
        tolerance: 0.01,
        ..Options::default()
    };
    let a = run(
        Action::GeodesicColour,
        &[0; 21],
        7,
        &o,
        [0.0, 0.5],
        Some(&f),
    );
    assert_eq!(a[7], 255);
    assert!(a[9] > 0);
    assert_eq!(a[10], 0);
    assert_eq!(a[13], 0);
}
#[test]
fn otsu_sauvola_and_gabor_have_distinct_flat_and_pattern_responses() {
    let flat = frame(&[[128, 128, 128]; 25], 5);
    let mut o = Options {
        radius: 2,
        range: [0.01, 1.0],
        tolerance: 0.001,
        ..Options::default()
    };
    assert_eq!(
        run(Action::LocalOtsu, &[0; 25], 5, &o, [0.5, 0.5], Some(&flat)),
        vec![0; 25]
    );
    assert_eq!(
        run(
            Action::SauvolaThreshold,
            &[0; 25],
            5,
            &o,
            [0.5, 0.5],
            Some(&flat)
        ),
        vec![255; 25]
    );
    assert_eq!(
        run(
            Action::GaborResponse,
            &[0; 25],
            5,
            &o,
            [0.5, 0.5],
            Some(&flat)
        ),
        vec![0; 25]
    );
    let mut rgb = vec![[0, 0, 0]; 25];
    for y in 0..5 {
        rgb[y * 5 + 2] = [255, 255, 255];
    }
    let f = frame(&rgb, 5);
    let a = run(Action::LocalOtsu, &[0; 25], 5, &o, [0.5, 0.5], Some(&f));
    assert_eq!(a[12], 255);
    assert_eq!(a[10], 0);
    o.corner = 0.0;
    let g = run(Action::GaborResponse, &[0; 25], 5, &o, [0.5, 0.5], Some(&f));
    assert!(g[12] > 0);
    let black = frame(&[[0, 0, 0]; 25], 5);
    assert_eq!(
        run(
            Action::SauvolaThreshold,
            &[0; 25],
            5,
            &o,
            [0.5, 0.5],
            Some(&black)
        ),
        vec![0; 25]
    );
}
#[test]
fn hessian_ridge_orientation_and_higher_moments_measure_structures() {
    let mut rgb = vec![[0, 0, 0]; 25];
    for y in 0..5 {
        rgb[y * 5 + 2] = [255, 255, 255];
    }
    let f = frame(&rgb, 5);
    let mut o = Options {
        radius: 2,
        range: [0.5, 1.0],
        tolerance: 0.001,
        ..Options::default()
    };
    assert_eq!(
        run(
            Action::DirectionalRidge,
            &[0; 25],
            5,
            &o,
            [0.5, 0.5],
            Some(&f)
        )[12],
        255
    );
    o.angle = 90.0;
    assert_eq!(
        run(
            Action::DirectionalRidge,
            &[0; 25],
            5,
            &o,
            [0.5, 0.5],
            Some(&f)
        )[12],
        0
    );
    let mut impulse = vec![[0, 0, 0]; 25];
    impulse[12] = [255, 255, 255];
    let f = frame(&impulse, 5);
    o.range = [0.8, 1.0];
    for action in [Action::LocalSkewness, Action::LocalKurtosis] {
        assert_eq!(run(action, &[0; 25], 5, &o, [0.5, 0.5], Some(&f))[12], 255);
    }
    o.range = [0.49, 0.51];
    let flat = frame(&[[30, 30, 30]; 25], 5);
    for action in [Action::LocalSkewness, Action::LocalKurtosis] {
        assert_eq!(
            run(action, &[0; 25], 5, &o, [0.5, 0.5], Some(&flat)),
            vec![255; 25]
        );
    }
}
#[test]
fn opening_reconstruction_and_closing_treat_thin_attachments_and_holes_differently() {
    let mut alpha = vec![0; 81];
    for y in 3..6 {
        for x in 3..6 {
            alpha[y * 9 + x] = 220;
        }
    }
    alpha[4 * 9 + 6] = 220;
    alpha[4 * 9 + 7] = 220;
    alpha[10] = 255;
    let o = Options {
        radius: 1,
        ..Options::default()
    };
    let opened = run(Action::CoverageOpening, &alpha, 9, &o, [0.5, 0.5], None);
    let rebuilt = run(
        Action::MorphologicalReconstruction,
        &alpha,
        9,
        &o,
        [0.5, 0.5],
        None,
    );
    assert_eq!(opened[4 * 9 + 7], 0);
    assert_eq!(opened[40], 220);
    assert_eq!(opened[10], 0);
    assert_eq!(rebuilt[4 * 9 + 7], 220);
    assert_eq!(rebuilt[10], 0);
    let mut ring = vec![0; 81];
    for y in 2..7 {
        for x in 2..7 {
            ring[y * 9 + x] = 200;
        }
    }
    ring[40] = 0;
    let closed = run(Action::CoverageClosing, &ring, 9, &o, [0.5, 0.5], None);
    assert_eq!(closed[40], 200);
}
#[test]
fn grayscale_max_tree_suppresses_small_peaks_without_binary_flattening() {
    let alpha = [0, 80, 80, 80, 0, 0, 80, 240, 80, 0, 0, 80, 80, 80, 0];
    let o = Options {
        min_area: 2,
        ..Options::default()
    };
    let a = run(
        Action::GrayscaleAreaOpening,
        &alpha,
        5,
        &o,
        [0.5, 0.5],
        None,
    );
    assert_eq!(a[7], 80);
    assert_eq!(a[6], 80);
    assert_eq!(a[0], 0);
    let identity = Options {
        min_area: 1,
        ..Options::default()
    };
    assert_eq!(
        run(
            Action::GrayscaleAreaOpening,
            &alpha,
            5,
            &identity,
            [0.5, 0.5],
            None
        ),
        alpha
    );
}
#[test]
fn border_components_and_hole_bearing_regions_preserve_only_matching_components() {
    let mut alpha = vec![0; 121];
    for y in 2..7 {
        for x in 2..7 {
            if x == 2 || x == 6 || y == 2 || y == 6 {
                alpha[y * 11 + x] = 190;
            }
        }
    }
    alpha[0] = 255;
    alpha[1] = 255;
    alpha[9 * 11 + 8] = 210;
    let o = Options::default();
    let border = run(Action::BorderConnected, &alpha, 11, &o, [0.5, 0.5], None);
    assert_eq!(border[0], 255);
    assert_eq!(border[24], 0);
    let holes = run(Action::HoleBearingRegions, &alpha, 11, &o, [0.5, 0.5], None);
    assert_eq!(holes[24], 190);
    assert_eq!(holes[0], 0);
    assert_eq!(holes[107], 0);
}
#[test]
fn geodesic_rings_follow_selection_connectivity_and_regional_maxima_keep_plateaus() {
    let alpha = [
        255, 255, 0, 255, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 0,
    ];
    let o = Options {
        radius: 4,
        range: [0.5, 0.75],
        tolerance: 0.001,
        ..Options::default()
    };
    let a = run(Action::GeodesicRings, &alpha, 5, &o, [0.0, 0.0], None);
    assert_eq!(a[0], 0);
    assert_eq!(a[6], 255);
    assert_eq!(a[11], 255);
    assert_eq!(a[3], 0);
    let plateau = [20, 60, 60, 20, 90, 20, 20, 20, 20, 90];
    let m = run(
        Action::RegionalMaxima,
        &plateau,
        5,
        &Options::default(),
        [0.5, 0.5],
        None,
    );
    assert_eq!(m, vec![0, 60, 60, 0, 90, 0, 0, 0, 0, 90]);
}
#[test]
fn spiral_and_barycentric_constructions_have_real_geometry_and_reject_collinearity() {
    let o = Options {
        bounds: [0.0, 0.0, 1.0, 1.0],
        radius: 2,
        tolerance: 0.03,
        ..Options::default()
    };
    let spiral = run(
        Action::SpiralMarquee,
        &vec![0; 441],
        21,
        &o,
        [0.5, 0.5],
        None,
    );
    assert!(spiral.iter().any(|v| *v > 0));
    assert!(spiral.contains(&0));
    assert_eq!(spiral[0], 0);
    let mut triangle = o.clone();
    triangle.vertices = vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
    triangle.corner = 0.2;
    let t = run(
        Action::BarycentricTriangle,
        &[0; 100],
        10,
        &triangle,
        [0.5, 0.5],
        None,
    );
    assert!(t[0] > t[4]);
    assert_eq!(t[99], 0);
    triangle.vertices = vec![[0.0, 0.0], [0.5, 0.5], [1.0, 1.0]];
    let mut p = plane(vec![77; 9], 3);
    assert!(apply(
        &mut p,
        Action::BarycentricTriangle,
        &triangle,
        [0.5, 0.5],
        None,
        &CancelToken::new()
    )
    .is_err());
    assert_eq!(p.alpha, vec![77; 9]);
}
#[test]
fn nonlinear_warps_and_gradient_displacement_obey_identity_and_change_nontrivial_inputs() {
    let alpha: Vec<_> = (0..121)
        .map(|i| {
            if i % 11 == 6 && i / 11 >= 2 && i / 11 <= 8 {
                255
            } else {
                0
            }
        })
        .collect();
    let mut o = Options {
        bounds: [0.0, 0.0, 1.0, 1.0],
        corner: 0.0,
        angle: 0.0,
        ..Options::default()
    };
    for action in [
        Action::CoverageShear,
        Action::CoverageTwirl,
        Action::CoverageSpherize,
    ] {
        assert_eq!(
            run(action, &alpha, 11, &o, [0.5, 0.5], None),
            alpha,
            "{action:?}"
        );
    }
    o.angle = 70.0;
    o.corner = 1.0;
    for action in [
        Action::CoverageShear,
        Action::CoverageTwirl,
        Action::CoverageSpherize,
    ] {
        assert_ne!(
            run(action, &alpha, 11, &o, [0.5, 0.5], None),
            alpha,
            "{action:?}"
        );
    }
    let flat = frame(&[[128, 128, 128]; 121], 11);
    assert_eq!(
        run(
            Action::CoverageDisplace,
            &alpha,
            11,
            &o,
            [0.5, 0.5],
            Some(&flat)
        ),
        alpha
    );
    let rgb: Vec<_> = (0..121)
        .map(|i| {
            let v = u8::try_from((i % 11) * 25).expect("eleven-column ramp is at most 250");
            [v, v, v]
        })
        .collect();
    let f = frame(&rgb, 11);
    assert_ne!(
        run(
            Action::CoverageDisplace,
            &alpha,
            11,
            &o,
            [0.5, 0.5],
            Some(&f)
        ),
        alpha
    );
}
#[test]
fn signed_distance_profiles_extend_both_sides_and_keep_constant_extremes() {
    let o = Options {
        radius: 2,
        ..Options::default()
    };
    let a = run(
        Action::SignedDistanceSmooth,
        &[0, 0, 255, 255, 255, 0, 0],
        7,
        &o,
        [0.5, 0.5],
        None,
    );
    assert!(a[0] < a[1] && a[1] < 128 && a[2] > 128 && a[3] > a[2]);
    assert_eq!(a[0], a[6]);
    for v in [0, 255] {
        assert_eq!(
            run(
                Action::SignedDistanceSmooth,
                &[v; 7],
                7,
                &o,
                [0.5, 0.5],
                None
            ),
            vec![v; 7]
        );
    }
}
