#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::float_cmp)]
use super::*;
fn plane(alpha: &[u8], width: usize) -> Plane {
    Plane {
        width,
        height: alpha.len() / width,
        alpha: alpha.to_vec(),
    }
}
fn frame(rgb: &[[u8; 3]], width: usize) -> Frame {
    Frame::from_rgb(
        &rgb.iter().flatten().copied().collect::<Vec<_>>(),
        width,
        rgb.len() / width,
    )
    .expect("valid frame")
}
fn run(
    action: Action,
    alpha: &[u8],
    width: usize,
    options: &Options,
    point: [f32; 2],
    frame: Option<&Frame>,
) -> Vec<u8> {
    let mut selection = plane(alpha, width);
    apply(
        &mut selection,
        action,
        options,
        point,
        frame,
        &CancelToken::new(),
    )
    .expect("valid operator");
    selection.alpha
}
fn cage(deform: bool) -> Vec<[f32; 2]> {
    let source = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let target = if deform {
        [[0.0, 0.0], [1.0, 0.1], [0.8, 1.0], [0.0, 0.9]]
    } else {
        source
    };
    source
        .into_iter()
        .zip(target)
        .flat_map(|(a, b)| [a, b])
        .collect()
}
fn options(action: Action) -> Options {
    let mut options = Options {
        radius: 1,
        ..Options::default()
    };
    if [
        Action::ThinPlateCoverageWarp,
        Action::MeanValueCageWarp,
        Action::TriangulatedCageWarp,
    ]
    .contains(&action)
    {
        options.vertices = cage(true);
    }
    if action == Action::CoverageWatershed {
        options.vertices = vec![[0.3, 0.5], [0.7, 0.5]];
    }
    options
}
#[test]
fn all_new_operators_are_deterministic_and_leave_readonly_inputs_untouched() {
    let alpha: Vec<_> = (0..81)
        .map(|i| {
            if (1..8).contains(&(i % 9)) && (1..8).contains(&(i / 9)) {
                200
            } else {
                0
            }
        })
        .collect();
    let rgb: Vec<_> = (0..81)
        .map(|i| [3, 7, 11].map(|factor| u8::try_from(i * factor % 256).expect("byte modulo")))
        .collect();
    let source = frame(&rgb, 9);
    let frozen = source.rgb.clone();
    for action in ACTIONS {
        let options = options(action);
        let first = run(action, &alpha, 9, &options, [0.5, 0.5], Some(&source));
        assert_eq!(first.len(), alpha.len());
        assert_eq!(
            first,
            run(action, &alpha, 9, &options, [0.5, 0.5], Some(&source)),
            "{action:?}"
        );
    }
    assert_eq!(source.rgb, frozen);
    assert_eq!(alpha[0], 0);
}
#[test]
fn cancellation_nonfinite_and_mismatched_grids_never_commit_partial_output() {
    let original = vec![170; 25];
    let source = frame(&[[100, 100, 100]; 25], 5);
    let cancel = CancelToken::new();
    cancel.cancel();
    for action in ACTIONS {
        let mut selection = plane(&original, 5);
        let mut options = options(action);
        assert!(apply(
            &mut selection,
            action,
            &options,
            [0.5, 0.5],
            Some(&source),
            &cancel
        )
        .is_err());
        assert_eq!(selection.alpha, original);
        options.corner = f32::NAN;
        assert!(apply(
            &mut selection,
            action,
            &options,
            [0.5, 0.5],
            Some(&source),
            &CancelToken::new()
        )
        .is_err());
        assert_eq!(selection.alpha, original);
    }
    let wrong = frame(&[[100, 100, 100]], 1);
    let mut selection = plane(&original, 5);
    assert!(apply(
        &mut selection,
        Action::MedianAbsoluteDeviation,
        &Options::default(),
        [0.5, 0.5],
        Some(&wrong),
        &CancelToken::new()
    )
    .is_err());
    assert_eq!(selection.alpha, original);
    let mut empty = Plane {
        width: 0,
        height: 0,
        alpha: vec![],
    };
    assert!(apply(
        &mut empty,
        Action::SelectionKCore,
        &Options::default(),
        [0.5, 0.5],
        None,
        &CancelToken::new()
    )
    .is_err());
    assert_eq!(sample(&selection, f32::MAX, 0.5), 0.0);
    assert_eq!(sample(&selection, -f32::MAX, 0.5), 0.0);
}
#[test]
fn single_cell_and_transparent_topology_stay_bounded() {
    for action in [
        Action::SelectionArticulations,
        Action::SelectionKCore,
        Action::CoverageAnisotropicDiffusion,
        Action::CoverageCurvatureFlow,
        Action::RegionInscribedDisk,
        Action::DirectionalRunWidth,
        Action::CoverageSeamCarve,
    ] {
        assert_eq!(
            run(action, &[0], 1, &Options::default(), [0.5, 0.5], None),
            vec![0]
        );
    }
    assert_eq!(
        run(
            Action::RegionInscribedDisk,
            &[201],
            1,
            &Options::default(),
            [0.5, 0.5],
            None
        ),
        vec![201]
    );
}
#[test]
fn robust_deviation_and_binary_transition_descriptors_have_known_answers() {
    let mut rgb = vec![[100, 100, 100]; 25];
    rgb[12] = [255, 255, 255];
    let source = frame(&rgb, 5);
    let options = Options {
        radius: 1,
        range: [0.9, 1.0],
        tolerance: 0.001,
        ..Options::default()
    };
    let result = run(
        Action::MedianAbsoluteDeviation,
        &[0; 25],
        5,
        &options,
        [0.5, 0.5],
        Some(&source),
    );
    assert_eq!(result[12], 255);
    assert_eq!(result[0], 0);
    let mut rgb = vec![[127, 127, 127]; 25];
    for (index, position) in [6, 7, 8, 13, 18, 17, 16, 11].into_iter().enumerate() {
        rgb[position] = if index.is_multiple_of(2) {
            [0, 0, 0]
        } else {
            [255, 255, 255]
        };
    }
    let source = frame(&rgb, 5);
    assert_eq!(
        run(
            Action::BinaryPatternTransitions,
            &[0; 25],
            5,
            &options,
            [0.5, 0.5],
            Some(&source)
        )[12],
        255
    );
    let flat = frame(&[[127, 127, 127]; 25], 5);
    assert_eq!(
        run(
            Action::BinaryPatternTransitions,
            &[0; 25],
            5,
            &options,
            [0.5, 0.5],
            Some(&flat)
        ),
        vec![0; 25]
    );
}
#[test]
fn source_pair_statistics_distinguish_direction_and_flat_patches() {
    let rgb: Vec<_> = (0..49)
        .map(|i| {
            if i % 2 == 0 {
                [0, 0, 0]
            } else {
                [255, 255, 255]
            }
        })
        .collect();
    let source = frame(&rgb, 7);
    let mut options = Options {
        radius: 1,
        range: [0.9, 1.0],
        tolerance: 0.001,
        ..Options::default()
    };
    let horizontal = run(
        Action::PatchAutocorrelation,
        &[0; 49],
        7,
        &options,
        [0.5, 0.5],
        Some(&source),
    );
    assert_eq!(horizontal[24], 0);
    assert_eq!(
        run(
            Action::PatchSemivariance,
            &[0; 49],
            7,
            &options,
            [0.5, 0.5],
            Some(&source)
        )[24],
        255
    );
    // Horizontal stripes repeat vertically after shifting one row by an odd width only when authored by X.
    let rgb: Vec<_> = (0..49)
        .map(|i| {
            if (i % 7) % 2 == 0 {
                [0, 0, 0]
            } else {
                [255, 255, 255]
            }
        })
        .collect();
    let source = frame(&rgb, 7);
    options.angle = 90.0;
    assert_eq!(
        run(
            Action::PatchAutocorrelation,
            &[0; 49],
            7,
            &options,
            [0.5, 0.5],
            Some(&source)
        )[24],
        255
    );
    assert_eq!(
        run(
            Action::PatchSemivariance,
            &[0; 49],
            7,
            &options,
            [0.5, 0.5],
            Some(&source)
        )[24],
        0
    );
}
#[test]
fn logarithmic_colour_ratio_preserves_shading_and_prototypes_resolve_ties() {
    let source = frame(&[[20, 40, 60], [40, 80, 120], [200, 10, 5]], 3);
    let options = Options {
        tolerance: 0.1,
        ..Options::default()
    };
    let output = run(
        Action::LogChromaDistance,
        &[0; 3],
        3,
        &options,
        [0.0, 0.5],
        Some(&source),
    );
    assert_eq!(output[0], 255);
    assert!(output[1] > 240);
    assert_eq!(output[2], 0);
    let source = frame(&[[255, 0, 0], [0, 255, 0], [0, 0, 255]], 3);
    let options = Options {
        vertices: vec![[0.0, 0.5], [1.0, 0.5]],
        seed_index: 0,
        ..Options::default()
    };
    assert_eq!(
        run(
            Action::ColourPrototypePartition,
            &[0; 3],
            3,
            &options,
            [0.5, 0.5],
            Some(&source)
        ),
        vec![255, 255, 0]
    );
}
#[test]
fn nonmaximum_edges_suppress_shallower_neighbors() {
    let rgb: Vec<_> = [0, 0, 20, 100, 240, 255, 255]
        .into_iter()
        .map(|v| [v, v, v])
        .collect();
    let source = frame(&rgb, 7);
    let options = Options {
        range: [0.1, 1.0],
        tolerance: 0.001,
        ..Options::default()
    };
    let output = run(
        Action::EdgeNonmaximum,
        &[0; 7],
        7,
        &options,
        [0.5, 0.5],
        Some(&source),
    );
    assert_eq!(output[3], 255);
    assert_eq!(output[2], 0);
    assert_eq!(output[4], 0);
    let including_zero = Options {
        range: [0.0, 1.0],
        ..options
    };
    let output = run(
        Action::EdgeNonmaximum,
        &[0; 7],
        7,
        &including_zero,
        [0.5, 0.5],
        Some(&source),
    );
    assert_eq!(output[0], 0);
    assert_eq!(output[4], 0);
    assert!(output[3] > 0);
}
#[test]
fn hysteresis_keeps_connected_weak_edges_and_excludes_disconnected_ones() {
    let mut rgb = vec![[0, 0, 0]; 117];
    for y in 0..9 {
        for x in 8..13 {
            let value = if y < 4 { 200 } else { 80 };
            rgb[y * 13 + x] = [value, value, value];
        }
    }
    rgb[7 * 13 + 1] = [80, 80, 80];
    let source = frame(&rgb, 13);
    let options = Options {
        range: [0.1, 0.3],
        ..Options::default()
    };
    let output = run(
        Action::EdgeHysteresis,
        &[0; 117],
        13,
        &options,
        [0.5, 0.5],
        Some(&source),
    );
    assert_eq!(output[13 + 8], 255);
    assert_eq!(output[7 * 13 + 8], 255);
    assert_eq!(output[7 * 13], 0);
}
#[test]
fn moran_template_and_circular_hue_measure_different_patch_properties() {
    let flat = frame(&[[128, 128, 128]; 49], 7);
    let mut options = Options {
        radius: 1,
        range: [0.49, 0.51],
        tolerance: 0.001,
        ..Options::default()
    };
    assert_eq!(
        run(
            Action::LocalMoran,
            &[0; 49],
            7,
            &options,
            [0.5, 0.5],
            Some(&flat)
        ),
        vec![255; 49]
    );
    let mut rgb = vec![[0, 0, 0]; 49];
    for position in [17, 23, 24, 25, 31] {
        rgb[position] = [255, 255, 255];
    }
    let source = frame(&rgb, 7);
    options.range = [0.95, 1.0];
    let output = run(
        Action::TemplatePatchCorrelation,
        &[0; 49],
        7,
        &options,
        [0.5, 0.5],
        Some(&source),
    );
    assert_eq!(output[24], 255);
    assert_eq!(output[0], 0);
    let rgb: Vec<_> = (0..49)
        .map(|i| {
            if i % 2 == 0 {
                [255, 0, 0]
            } else {
                [0, 255, 255]
            }
        })
        .collect();
    let source = frame(&rgb, 7);
    options.range = [0.8, 1.0];
    assert_eq!(
        run(
            Action::LocalHueDispersion,
            &[0; 49],
            7,
            &options,
            [0.5, 0.5],
            Some(&source)
        )[24],
        255
    );
    let red = frame(&[[255, 0, 0]; 49], 7);
    assert_eq!(
        run(
            Action::LocalHueDispersion,
            &[0; 49],
            7,
            &options,
            [0.5, 0.5],
            Some(&red)
        ),
        vec![0; 49]
    );
}
#[test]
fn articulations_and_graph_cores_preserve_only_selected_support() {
    let alpha = [200, 200, 200, 200, 200];
    assert_eq!(
        run(
            Action::SelectionArticulations,
            &alpha,
            5,
            &Options::default(),
            [0.5, 0.5],
            None
        ),
        vec![0, 200, 200, 200, 0]
    );
    let alpha = [
        0, 0, 0, 0, 0, 0, 180, 180, 180, 0, 0, 180, 180, 0, 0, 0, 0, 0, 0, 0,
    ];
    let options = Options {
        corner: 2.0 / 3.0,
        ..Options::default()
    };
    let output = run(
        Action::SelectionKCore,
        &alpha,
        5,
        &options,
        [0.5, 0.5],
        None,
    );
    assert_eq!(output[8], 0);
    assert_eq!(output[6], 180);
    for (before, after) in alpha.iter().zip(output) {
        assert!(after <= *before);
    }
}
#[test]
fn minimax_watershed_uses_seed_distance_ties_and_rejects_duplicate_seeds_atomically() {
    let alpha = [200; 5];
    let options = Options {
        vertices: vec![[0.0, 0.5], [1.0, 0.5]],
        seed_index: 0,
        ..Options::default()
    };
    assert_eq!(
        run(
            Action::CoverageWatershed,
            &alpha,
            5,
            &options,
            [0.5, 0.5],
            None
        ),
        vec![200, 200, 200, 0, 0]
    );
    let mut selection = plane(&alpha, 5);
    let bad = Options {
        vertices: vec![[0.0, 0.5], [0.0, 0.5]],
        ..options
    };
    assert!(apply(
        &mut selection,
        Action::CoverageWatershed,
        &bad,
        [0.5, 0.5],
        None,
        &CancelToken::new()
    )
    .is_err());
    assert_eq!(selection.alpha, alpha);
}
#[test]
fn nonlinear_diffusion_resists_strong_edges_and_curvature_preserves_straight_ramps() {
    let alpha = [0, 0, 255, 0, 0];
    let mut options = Options {
        radius: 1,
        tolerance: 0.01,
        ..Options::default()
    };
    assert_eq!(
        run(
            Action::CoverageAnisotropicDiffusion,
            &alpha,
            5,
            &options,
            [0.5, 0.5],
            None
        ),
        alpha
    );
    options.tolerance = 1.0;
    assert_ne!(
        run(
            Action::CoverageAnisotropicDiffusion,
            &alpha,
            5,
            &options,
            [0.5, 0.5],
            None
        ),
        alpha
    );
    let ramp: Vec<_> = (0..49)
        .map(|i| u8::try_from(i % 7 * 35).expect("bounded ramp"))
        .collect();
    assert_eq!(
        run(
            Action::CoverageCurvatureFlow,
            &ramp,
            7,
            &options,
            [0.5, 0.5],
            None
        ),
        ramp
    );
}
#[test]
fn exact_inscribed_disks_trim_square_corners_and_keep_component_centres() {
    let mut alpha = vec![0; 81];
    for y in 2..7 {
        for x in 2..7 {
            alpha[y * 9 + x] = 201;
        }
    }
    alpha[0] = 190;
    let output = run(
        Action::RegionInscribedDisk,
        &alpha,
        9,
        &Options::default(),
        [0.5, 0.5],
        None,
    );
    assert_eq!(output[40], 201);
    assert_eq!(output[22], 201);
    assert_eq!(output[20], 0);
    assert_eq!(output[0], 190);
    for (before, after) in alpha.iter().zip(output) {
        assert!(after <= *before);
    }
}
#[test]
fn directional_run_width_is_axis_sensitive_and_selection_limited() {
    let alpha = [0, 0, 0, 0, 0, 0, 200, 200, 200, 0, 0, 0, 0, 0, 0];
    let mut options = Options {
        radius: 3,
        range: [0.9, 1.0],
        tolerance: 0.001,
        ..Options::default()
    };
    assert_eq!(
        run(
            Action::DirectionalRunWidth,
            &alpha,
            5,
            &options,
            [0.5, 0.5],
            None
        ),
        alpha
    );
    options.angle = 90.0;
    assert_eq!(
        run(
            Action::DirectionalRunWidth,
            &alpha,
            5,
            &options,
            [0.5, 0.5],
            None
        ),
        vec![0; 15]
    );
}
#[test]
fn landmark_deformations_have_identity_and_nonidentity_known_answers() {
    let alpha: Vec<_> = (0..81)
        .map(|i| if (i % 9) % 3 == 0 { 220 } else { 0 })
        .collect();
    for action in [
        Action::ThinPlateCoverageWarp,
        Action::MeanValueCageWarp,
        Action::TriangulatedCageWarp,
    ] {
        let mut options = Options {
            vertices: cage(false),
            corner: 0.0,
            ..Options::default()
        };
        assert_eq!(
            run(action, &alpha, 9, &options, [0.5, 0.5], None),
            alpha,
            "{action:?}"
        );
        options.vertices = cage(true);
        assert_ne!(
            run(action, &alpha, 9, &options, [0.5, 0.5], None),
            alpha,
            "{action:?}"
        );
    }
}
#[test]
fn degenerate_and_nonconvex_cages_fail_atomically() {
    let alpha = [100; 25];
    for action in [
        Action::ThinPlateCoverageWarp,
        Action::MeanValueCageWarp,
        Action::TriangulatedCageWarp,
    ] {
        let mut selection = plane(&alpha, 5);
        let options = Options {
            vertices: vec![
                [0.0, 0.0],
                [0.0, 0.0],
                [0.5, 0.5],
                [0.5, 0.5],
                [1.0, 1.0],
                [1.0, 1.0],
            ],
            ..Options::default()
        };
        assert!(apply(
            &mut selection,
            action,
            &options,
            [0.5, 0.5],
            None,
            &CancelToken::new()
        )
        .is_err());
        assert_eq!(selection.alpha, alpha);
    }
}

#[test]
fn crossing_star_cages_are_rejected_even_when_local_turns_share_a_sign() {
    let star = [
        [0.5, 0.05],
        [0.765, 0.864],
        [0.072, 0.36],
        [0.928, 0.36],
        [0.235, 0.864],
    ];
    let vertices = star.into_iter().flat_map(|point| [point, point]).collect();
    let options = Options {
        vertices,
        ..Options::default()
    };
    for action in [Action::MeanValueCageWarp, Action::TriangulatedCageWarp] {
        let mut selection = plane(&[130; 25], 5);
        assert!(apply(
            &mut selection,
            action,
            &options,
            [0.5, 0.5],
            None,
            &CancelToken::new()
        )
        .is_err());
        assert_eq!(selection.alpha, vec![130; 25]);
    }
}
#[test]
fn mobius_and_path_maps_match_identity_and_reject_zero_length_paths() {
    let alpha: Vec<_> = (0..81)
        .map(|i| u8::try_from(i * 3).expect("bounded gradient"))
        .collect();
    let mut options = Options {
        bounds: [0.0, 0.0, 1.0, 1.0],
        corner: 0.0,
        vertices: vec![[0.0, 0.5], [1.0, 0.5]],
        tolerance: 0.5,
        ..Options::default()
    };
    assert_eq!(
        run(
            Action::MobiusCoverageWarp,
            &alpha,
            9,
            &options,
            [0.5, 0.5],
            None
        ),
        alpha
    );
    assert_eq!(
        run(
            Action::PathNormalUnwrap,
            &alpha,
            9,
            &options,
            [0.5, 0.5],
            None
        ),
        alpha
    );
    options.corner = 0.6;
    assert_ne!(
        run(
            Action::MobiusCoverageWarp,
            &alpha,
            9,
            &options,
            [0.5, 0.5],
            None
        ),
        alpha
    );
    options.vertices = vec![[0.5, 0.5], [0.5, 0.5]];
    let mut selection = plane(&alpha, 9);
    assert!(apply(
        &mut selection,
        Action::PathNormalUnwrap,
        &options,
        [0.5, 0.5],
        None,
        &CancelToken::new()
    )
    .is_err());
    assert_eq!(selection.alpha, alpha);
}
#[test]
fn seam_compression_preserves_uniform_coverage_and_changes_nonuniform_support() {
    let options = Options {
        radius: 2,
        ..Options::default()
    };
    let alpha = [0, 0, 255, 0, 0, 0, 0, 255, 0, 0, 0, 0, 255, 0, 0];
    assert_ne!(
        run(
            Action::CoverageSeamCarve,
            &alpha,
            5,
            &options,
            [0.5, 0.5],
            None
        ),
        alpha
    );
    assert_eq!(
        run(
            Action::CoverageSeamCarve,
            &[230; 15],
            5,
            &options,
            [0.5, 0.5],
            None
        ),
        vec![230; 15]
    );
}
#[test]
fn lissajous_ribbon_has_bounded_geometric_support() {
    let options = Options {
        bounds: [0.2, 0.2, 0.8, 0.8],
        radius: 2,
        tolerance: 0.03,
        ..Options::default()
    };
    let output = run(
        Action::LissajousRibbon,
        &[0; 441],
        21,
        &options,
        [0.5, 0.5],
        None,
    );
    assert!(output.contains(&0));
    assert!(output.iter().any(|v| *v > 0));
    assert_eq!(output[0], 0);
    assert_eq!(output[440], 0);
}
#[test]
fn resource_and_finite_geometry_limits_reject_before_output_changes() {
    let original = [150; 25];
    let source = frame(&[[128, 128, 128]; 25], 5);
    let mut selection = plane(&original, 5);
    for options in [
        Options {
            radius: 5,
            ..Options::default()
        },
        Options {
            radius: 32,
            ..Options::default()
        },
        Options {
            angle: f32::INFINITY,
            ..Options::default()
        },
        Options {
            bounds: [0.9, 0.0, 0.1, 1.0],
            ..Options::default()
        },
    ] {
        assert!(apply(
            &mut selection,
            Action::TemplatePatchCorrelation,
            &options,
            [0.5, 0.5],
            Some(&source),
            &CancelToken::new()
        )
        .is_err());
        assert_eq!(selection.alpha, original);
    }
}

#[test]
fn source_radius_preflight_runs_independently_of_preview_loading() {
    let over_budget = Options {
        radius: 9,
        ..Options::default()
    };
    for action in [
        Action::LocalEntropy,
        Action::MahalanobisColour,
        Action::MedianAbsoluteDeviation,
    ] {
        assert!(super::super::validate_source_radius(action, &over_budget).is_err());
    }
    let template = Options {
        radius: 5,
        ..Options::default()
    };
    assert!(
        super::super::validate_source_radius(Action::TemplatePatchCorrelation, &template).is_err()
    );
    assert!(super::super::validate_source_radius(Action::CoverageSeamCarve, &over_budget).is_ok());
}
