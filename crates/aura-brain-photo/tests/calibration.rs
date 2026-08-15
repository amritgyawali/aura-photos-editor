//! The camera calibration table: it loads, it refuses, and its fallback is honest.
//!
//! The shipped file is embedded in the binary, so a malformed one is a build bug rather
//! than a deployment one - which is why the first test here loads it, exactly as phase
//! 07's and phase 08's config tests load theirs.

use aura_brain_photo::integrity::calibration::{normalise, CalibrationTable, MIN_RATIONALE};

#[test]
fn the_shipped_table_loads_and_carries_twenty_bodies() {
    let table = CalibrationTable::embedded().expect("the embedded table must load");
    assert!(table.version() >= 1);
    assert_eq!(
        table.rows(),
        20,
        "section 8 step 1 asks for the top 20 bodies"
    );
    assert!(!table.fallback().measured);
    assert!(table.fallback().confidence_penalty > 0.0);
}

#[test]
fn every_shipped_row_is_a_measurement_and_the_fallback_is_not() {
    let table = CalibrationTable::embedded().expect("embedded");
    for (make, model) in table.bodies() {
        let row = table.get(&make, &model, 24.0);
        assert!(
            row.measured,
            "{make} {model} should have matched its own row"
        );
        assert!(
            row.confidence_penalty <= f32::EPSILON,
            "{make} {model} is measured, so it must cost no confidence"
        );
    }
}

#[test]
fn a_body_nobody_measured_gets_a_cautious_fallback() {
    let table = CalibrationTable::embedded().expect("embedded");
    let unknown = table.get("Hasselblad", "X2D 100C", 100.0);
    assert!(!unknown.measured);
    assert!(unknown.confidence_penalty > 0.0);
    // Section 12's fourth failure mode: normalised by sensor resolution alone. A 100 MP
    // body should expect a *lower* per-output-pixel edge response than the 24 MP
    // fallback baseline, for the reason the module header spends a paragraph on.
    assert!(unknown.expected_mtf50 < table.fallback().expected_mtf50);
}

#[test]
fn fallback_scaling_agrees_with_the_measured_rows() {
    // The a7 III and the a7R IV are the same generation of sensor at 24 and 61 MP, and
    // the shipped rows put them at 0.34 and 0.27 - a ratio of 0.794. The fourth-root
    // rule the fallback uses predicts (24/61)^0.25 = 0.795. If those two ever diverge,
    // either the rows are wrong or the exponent is.
    let table = CalibrationTable::embedded().expect("embedded");
    let low = table.get("SONY", "ILCE-7M3", 24.2);
    let high = table.get("SONY", "ILCE-7RM4", 61.0);
    let measured_ratio = high.expected_mtf50 / low.expected_mtf50;

    let predicted = (24.2f32 / 61.0).powf(0.25);
    assert!(
        (measured_ratio - predicted).abs() < 0.03,
        "measured {measured_ratio:.3} against predicted {predicted:.3}"
    );
}

#[test]
fn make_and_model_matching_survives_how_manufacturers_write_exif() {
    let table = CalibrationTable::embedded().expect("embedded");
    // Nikon writes an underscore, pads with spaces, and shouts its own name.
    for (make, model) in [
        ("NIKON CORPORATION", "NIKON Z 6_2"),
        ("nikon corporation", "nikon z 6-2"),
        ("  NIKON CORPORATION  ", "NIKON  Z  6_2  "),
    ] {
        assert!(
            table.get(make, model, 24.5).measured,
            "{make} / {model} should have matched the Z6 II row"
        );
    }
    assert_eq!(normalise("NIKON  Z  6_2"), "nikon z 6 2");
}

#[test]
fn noise_and_headroom_move_with_iso_in_the_documented_direction() {
    let table = CalibrationTable::embedded().expect("embedded");
    let body = table.get("SONY", "ILCE-7M3", 24.2);
    assert!(body.expected_noise_e(6400) > body.expected_noise_e(100));
    assert!(body.shadow_headroom_at(6400) < body.shadow_headroom_at(100));
    // Never negative: a body pushed to its ceiling has no headroom, not anti-headroom.
    assert!(body.shadow_headroom_at(409_600) >= 0.0);
}

#[test]
fn an_older_body_gives_back_less_shadow_than_a_modern_one() {
    // Section 1's example, as an assertion: "a 2-stop-under dance frame from a modern
    // sensor is fine, the same from an older body is not".
    let table = CalibrationTable::embedded().expect("embedded");
    let modern = table.get("SONY", "ILCE-7M3", 24.2);
    let older = table.get("Canon", "EOS 5D Mark IV", 30.4);
    assert!(modern.shadow_headroom_at(3200) > older.shadow_headroom_at(3200));
    assert!(older.read_noise_e > modern.read_noise_e);
}

#[test]
fn a_malformed_table_is_refused_by_key_and_by_rule() {
    let cases = [
        (
            "version = 0\n[fallback]\nmegapixels=24.0\nexpected_mtf50=0.24\nread_noise_e=3.4\n\
             noise_slope=1.0\nhighlight_headroom_ev=1.0\nshadow_headroom_ev=3.5\nbase_iso=100\n\
             confidence_penalty=0.15\nrationale='a long enough sentence'\n",
            "version",
        ),
        (
            "version = 1\n[fallback]\nmegapixels=24.0\nexpected_mtf50=0.24\nread_noise_e=3.4\n\
             noise_slope=1.0\nhighlight_headroom_ev=1.0\nshadow_headroom_ev=3.5\nbase_iso=100\n\
             confidence_penalty=0.15\nrationale='ok'\n",
            "rationale",
        ),
        (
            "version = 1\n[fallback]\nmegapixels=24.0\nexpected_mtf50=0.24\nread_noise_e=3.4\n\
             noise_slope=1.0\nhighlight_headroom_ev=1.0\nshadow_headroom_ev=3.5\nbase_iso=100\n\
             confidence_penalty=0.0\nrationale='a long enough sentence'\n",
            "confidence_penalty",
        ),
        (
            "version = 1\n[fallback]\nmegapixels=24.0\nexpected_mtf50=0.24\nread_noise_e=3.4\n\
             noise_slope=1.0\nhighlight_headroom_ev=3.4\nshadow_headroom_ev=3.5\nbase_iso=100\n\
             confidence_penalty=0.15\nrationale='a long enough sentence'\n",
            "highlight_headroom_ev",
        ),
    ];
    for (text, expected_key) in cases {
        let refusal = CalibrationTable::parse("test.toml", text).expect_err("must be refused");
        assert_eq!(refusal.code.0, "AURA-ML-5036");
        assert!(
            refusal.detail.contains(expected_key),
            "the message must name the key: {}",
            refusal.detail
        );
    }
    // Three config files, one rule: `scene_profiles`, `moment_profiles` and this one all
    // refuse a rationale shorter than nine characters. A measurement nobody can explain
    // is a magic number even when it came off a chart.
    assert_eq!(MIN_RATIONALE, 9);
}

#[test]
fn one_body_may_not_appear_twice() {
    let text = "version = 1
[fallback]
megapixels=24.0
expected_mtf50=0.24
read_noise_e=3.4
noise_slope=1.0
highlight_headroom_ev=1.0
shadow_headroom_ev=3.5
base_iso=100
confidence_penalty=0.15
rationale='a long enough sentence'

[[camera]]
make='SONY'
model='ILCE-7M3'
megapixels=24.2
expected_mtf50=0.34
read_noise_e=3.1
noise_slope=0.92
highlight_headroom_ev=1.3
shadow_headroom_ev=4.5
base_iso=100
rationale='a long enough sentence'

[[camera]]
make='sony'
model='ilce 7m3'
megapixels=24.2
expected_mtf50=0.20
read_noise_e=3.1
noise_slope=0.92
highlight_headroom_ev=1.3
shadow_headroom_ev=4.5
base_iso=100
rationale='a long enough sentence'
";
    let refusal = CalibrationTable::parse("test.toml", text).expect_err("must be refused");
    assert_eq!(refusal.code.0, "AURA-ML-5036");
    assert!(refusal.detail.contains("twice"), "{}", refusal.detail);
}

#[test]
fn a_broken_override_falls_back_to_the_shipped_table() {
    let directory = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        directory.path().join("camera_calibration.toml"),
        "this is not toml at all {{{",
    )
    .expect("write");

    let (table, refusal) =
        CalibrationTable::load_or_embedded(directory.path()).expect("the baseline must load");
    assert_eq!(table.rows(), 20, "the shipped table is in force");
    let refusal = refusal.expect("the refusal must be reported, not swallowed");
    assert_eq!(refusal.code.0, "AURA-ML-5036");
}

#[test]
fn no_override_is_not_a_refusal() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (table, refusal) = CalibrationTable::load_or_embedded(directory.path()).expect("load");
    assert_eq!(table.rows(), 20);
    assert!(refusal.is_none());
}
