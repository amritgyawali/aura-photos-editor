//! The colour pipeline, checked against arithmetic rather than against itself.

use aura_raw::colour::curve::{
    filmic_lite, filmic_lite_inverse, linear_u16_to_scene, scene_to_linear_u16, srgb_decode,
    srgb_encode, KNEE,
};
use aura_raw::colour::de2000::{ciede2000, xyz_d65_to_lab, Lab};
use aura_raw::colour::matrix::{
    apply, bradford, determinant, invert, mul, Mat3, IDENTITY, REC2020_TO_XYZ_D65, SRGB_TO_XYZ_D65,
    WHITE_D50, WHITE_D65,
};
use aura_raw::colour::profile::{self, ProfileSource, BENCH_MAKE};
use aura_raw::colour::working_space::{camera_to_working, rec2020_to_srgb, xyz_d65_to_rec2020};

fn close(left: f64, right: f64, tolerance: f64) -> bool {
    (left - right).abs() <= tolerance
}

fn matrices_close(left: Mat3, right: Mat3, tolerance: f64) -> bool {
    (0..3).all(|row| {
        (0..3).all(|column| {
            let a = left[row][column];
            let b = right[row][column];
            close(a, b, tolerance)
        })
    })
}

// --------------------------------------------------------------- dE2000

/// Worked pairs from Sharma, Wu and Dalal, table 1. If this test ever fails,
/// every colour tolerance in the project has been measuring the wrong thing.
#[test]
fn de2000_matches_the_published_worked_examples() {
    let cases = [
        ((50.0, 2.6772, -79.7751), (50.0, 0.0, -82.7485), 2.0425),
        ((50.0, 3.1571, -77.2803), (50.0, 0.0, -82.7485), 2.8615),
        ((50.0, 2.8361, -74.0200), (50.0, 0.0, -82.7485), 3.4412),
        ((50.0, -1.3802, -84.2814), (50.0, 0.0, -82.7485), 1.0000),
        ((50.0, 2.5, 0.0), (50.0, 0.0, -2.5), 4.3065),
        (
            (60.2574, -34.0099, 36.2677),
            (60.4626, -34.1751, 39.4387),
            1.2644,
        ),
        (
            (22.7233, 20.0904, -46.6940),
            (23.0331, 14.9730, -42.5619),
            2.0373,
        ),
    ];
    for ((l1, a1, b1), (l2, a2, b2), expected) in cases {
        let got = ciede2000(
            Lab {
                l: l1,
                a: a1,
                b: b1,
            },
            Lab {
                l: l2,
                a: a2,
                b: b2,
            },
        );
        assert!(
            close(got, expected, 1e-3),
            "dE2000 for {l1},{a1},{b1} vs {l2},{a2},{b2}: got {got}, expected {expected}"
        );
    }
}

#[test]
fn a_colour_does_not_differ_from_itself() {
    let colour = Lab {
        l: 42.0,
        a: -7.5,
        b: 13.25,
    };
    assert!(ciede2000(colour, colour) < 1e-9);
}

// --------------------------------------------------------------- matrices

#[test]
fn inverting_a_matrix_twice_returns_the_original() {
    let inverse = invert(SRGB_TO_XYZ_D65).expect("sRGB primaries are invertible");
    let round_trip = invert(inverse).expect("and so is the inverse");
    assert!(matrices_close(round_trip, SRGB_TO_XYZ_D65, 1e-9));
    assert!(matrices_close(
        mul(inverse, SRGB_TO_XYZ_D65),
        IDENTITY,
        1e-9
    ));
}

#[test]
fn a_singular_matrix_is_refused_rather_than_approximated() {
    let singular: Mat3 = [[1.0, 2.0, 3.0], [2.0, 4.0, 6.0], [1.0, 1.0, 1.0]];
    assert!(close(determinant(singular), 0.0, 1e-12));
    assert!(invert(singular).is_none());
}

#[test]
fn bradford_adaptation_maps_one_white_onto_the_other() {
    let adapted = apply(bradford(WHITE_D50, WHITE_D65), WHITE_D50);
    for index in 0..3 {
        assert!(
            close(adapted[index], WHITE_D65[index], 1e-6),
            "channel {index}: {} vs {}",
            adapted[index],
            WHITE_D65[index]
        );
    }
}

/// The published primaries matrices are rounded to seven decimal places, so
/// their white points agree with each other and with the CIE illuminant to
/// about one part in ten thousand rather than exactly. That residue is the
/// tolerance here; anything larger means a matrix, not a rounding.
const WHITE_TOLERANCE: f64 = 5e-4;

#[test]
fn white_stays_white_through_the_whole_working_space_chain() {
    // sRGB white -> XYZ -> Rec.2020 -> back to sRGB must be white again.
    let white_srgb = [1.0, 1.0, 1.0];
    let xyz = apply(SRGB_TO_XYZ_D65, white_srgb);
    let working = apply(xyz_d65_to_rec2020(), xyz);
    for channel in working {
        assert!(
            close(channel, 1.0, WHITE_TOLERANCE),
            "Rec.2020 white drifted: {channel}"
        );
    }
    let back = apply(rec2020_to_srgb(), working);
    for channel in back {
        assert!(
            close(channel, 1.0, WHITE_TOLERANCE),
            "sRGB white drifted: {channel}"
        );
    }
}

#[test]
fn rec2020_primaries_round_trip_through_xyz() {
    let inverse = xyz_d65_to_rec2020();
    assert!(matrices_close(
        mul(inverse, REC2020_TO_XYZ_D65),
        IDENTITY,
        1e-9
    ));
}

#[test]
fn a_camera_matrix_takes_a_neutral_sensor_signal_to_neutral_light() {
    for body in 1..=8usize {
        let model = aura_raw::fixtures::bench_model(body);
        let found = profile::lookup(BENCH_MAKE, &model).expect("bundled bench profile");
        let chain = camera_to_working(found.xyz_to_camera, profile::illuminant_white(&found))
            .expect("bench matrices are invertible");
        let neutral = apply(chain, [1.0, 1.0, 1.0]);
        // Equal channels in the working space is what "neutral" means there.
        assert!(
            close(neutral[0], neutral[1], WHITE_TOLERANCE)
                && close(neutral[1], neutral[2], WHITE_TOLERANCE),
            "{model} leaves a cast on neutral: {neutral:?}"
        );
    }
}

// --------------------------------------------------------------- profiles

#[test]
fn a_file_matrix_beats_the_bundled_table() {
    let embedded: Mat3 = [[2.0, 0.1, 0.0], [0.0, 1.8, 0.1], [0.1, 0.0, 1.6]];
    let resolved = profile::resolve(
        BENCH_MAKE,
        &aura_raw::fixtures::bench_model(1),
        Some(embedded),
    );
    assert_eq!(resolved.source, ProfileSource::EmbeddedDng);
    assert!(matrices_close(resolved.xyz_to_camera, embedded, 1e-12));
}

#[test]
fn an_unknown_camera_falls_back_to_the_generic_matrix() {
    let resolved = profile::resolve("Acme", "Wedding-o-tron 9000", None);
    assert_eq!(resolved.source, ProfileSource::Generic);
    assert_eq!(resolved.signed_by, "");
    assert!(
        invert(resolved.xyz_to_camera).is_some(),
        "the fallback must still be usable"
    );
}

#[test]
fn a_singular_file_matrix_is_ignored_in_favour_of_the_table() {
    let singular: Mat3 = [[1.0, 1.0, 1.0], [2.0, 2.0, 2.0], [3.0, 3.0, 3.0]];
    let model = aura_raw::fixtures::bench_model(3);
    let resolved = profile::resolve(BENCH_MAKE, &model, Some(singular));
    assert_eq!(resolved.source, ProfileSource::Bundled);
}

#[test]
fn every_bundled_profile_is_signed_and_invertible() {
    let table = profile::bundled();
    assert_eq!(table.len(), 8);
    for entry in table {
        assert!(
            !entry.signed_by.is_empty(),
            "{} {} is unsigned and must not ship",
            entry.make,
            entry.model
        );
        assert!(invert(entry.xyz_to_camera).is_some());
    }
}

// ------------------------------------------------------------------ curve

#[test]
fn the_preview_curve_is_linear_below_the_knee() {
    for step in 0..=60u16 {
        let x = f32::from(step) / 100.0;
        assert!(
            (filmic_lite(x) - x).abs() < 1e-6,
            "shadows must not be crushed: f({x}) = {}",
            filmic_lite(x)
        );
    }
    assert!((filmic_lite(KNEE) - KNEE).abs() < 1e-6);
}

#[test]
fn the_preview_curve_rolls_highlights_off_without_clipping() {
    let mut previous = 0.0f32;
    for step in 0..400u16 {
        let x = f32::from(step) / 100.0;
        let y = filmic_lite(x);
        assert!(y >= previous, "the curve must be monotonic at {x}");
        assert!(y < 1.0, "the curve must never reach hard white at {x}");
        previous = y;
    }
    // Four stops above diffuse white is still distinguishable.
    assert!(filmic_lite(4.0) > filmic_lite(2.0));
}

#[test]
fn the_preview_curve_is_invertible() {
    for step in 1..390u16 {
        let x = f32::from(step) / 100.0;
        let round_trip = filmic_lite_inverse(filmic_lite(x));
        assert!(
            (round_trip - x).abs() < 1e-3,
            "inverse of f({x}) came back as {round_trip}"
        );
    }
}

#[test]
fn the_srgb_transfer_function_round_trips() {
    for step in 0..=255u16 {
        let encoded = f32::from(step) / 255.0;
        let round_trip = srgb_encode(srgb_decode(encoded));
        assert!((round_trip - encoded).abs() < 1e-5, "code {step} drifted");
    }
}

#[test]
fn the_sixteen_bit_linear_buffer_keeps_four_stops_of_headroom() {
    // Diffuse white sits a quarter of the way up, so specular highlights have
    // somewhere to live instead of being clipped into the same value.
    let white = scene_to_linear_u16(1.0);
    let specular = scene_to_linear_u16(3.5);
    assert!(
        white < specular,
        "headroom collapsed: {white} vs {specular}"
    );
    assert_eq!(scene_to_linear_u16(4.0), u16::MAX);

    for scene in [0.0f32, 0.05, 0.18, 0.5, 1.0, 2.0, 3.9] {
        let round_trip = linear_u16_to_scene(scene_to_linear_u16(scene));
        assert!(
            (round_trip - scene).abs() < 1e-3,
            "{scene} came back as {round_trip}"
        );
    }
}

#[test]
fn a_neutral_scene_value_stays_neutral_as_a_lab_colour() {
    let grey = [0.18, 0.18, 0.18];
    let xyz = apply(REC2020_TO_XYZ_D65, grey);
    let lab = xyz_d65_to_lab(xyz);
    assert!(
        lab.a.abs() < 0.02 && lab.b.abs() < 0.02,
        "grey has a cast: {lab:?}"
    );
}
