//! Binary16 and int8 arithmetic, checked against values a model actually holds.
//!
//! These are the numerics the whole parity story rests on: if `round_to_f16`
//! rounds differently from a half-precision device, every fp16 comparison in the
//! product is measuring our own mistake.

use aura_infer::tensor::{max_abs_diff, round_to_f16, QuantParams};

#[test]
fn binary16_round_trips_the_values_a_model_actually_holds() {
    for value in [0.0f32, 1.0, -1.0, 0.5, 0.25, 65504.0, -65504.0, 6.1e-5] {
        let round_tripped = round_to_f16(value);
        assert!(
            (round_tripped - value).abs() <= value.abs() * 1e-3 + 1e-7,
            "{value} became {round_tripped}"
        );
    }
}

#[test]
fn binary16_saturates_rather_than_wrapping() {
    assert!(round_to_f16(1.0e9).is_infinite());
    assert!(round_to_f16(-1.0e9).is_infinite());
    assert_eq!(round_to_f16(1.0e-9), 0.0);
}

#[test]
fn binary16_loses_precision_where_a_half_device_would() {
    // 1 + 2^-11 is not representable in binary16; 1 + 2^-10 is.
    assert!((round_to_f16(1.0 + 2.0f32.powi(-11)) - 1.0).abs() < 1e-6);
    assert!((round_to_f16(1.0 + 2.0f32.powi(-10)) - 1.0).abs() > 1e-6);
}

#[test]
fn quantisation_represents_zero_exactly() {
    // Padded convolutions insert real zeros. A grid that renders zero as -0.004
    // puts a grey halo around every padded edge.
    let params = QuantParams::from_range(-0.7, 3.2);
    assert!(params.round_trip(0.0).abs() < 1e-6);
}

#[test]
fn quantisation_stays_inside_its_documented_tolerance() {
    let values: Vec<f32> = (0..256).map(|i| f32::from(i as i16) / 255.0).collect();
    let params = QuantParams::observe(&values);
    for value in &values {
        assert!((params.round_trip(*value) - value).abs() <= 1e-2);
    }
}

#[test]
fn a_constant_tensor_quantises_without_dividing_by_zero() {
    let params = QuantParams::observe(&[0.0, 0.0, 0.0]);
    assert!(params.round_trip(0.0).abs() < f32::EPSILON);
}

#[test]
fn parity_comparison_refuses_mismatched_lengths() {
    assert!(max_abs_diff(&[1.0], &[1.0, 2.0]).is_none());
    assert_eq!(max_abs_diff(&[1.0, 2.0], &[1.0, 2.5]), Some(0.5));
}
