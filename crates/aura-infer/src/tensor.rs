//! Precision conversion and tensor arithmetic helpers.
//!
//! The three precisions in [`Precision`] are genuinely different arithmetic, not
//! three names for the same loop. That matters because cross-provider parity is
//! the headline acceptance criterion of this phase, and a parity harness where
//! every path computes identically proves nothing at all.
//!
//! * `Fp32` is the reference: plain `f32`, fixed accumulation order.
//! * `Fp16` rounds every weight and every activation through IEEE binary16, so a
//!   model carries the real rounding behaviour of a half-precision device.
//! * `Int8` applies per-tensor affine quantisation - one scale and one zero point
//!   per tensor, chosen from the observed range - which is what the export
//!   pipeline in `ml/export_onnx/quantise.py` produces.
//!
//! The binary16 conversion is written here rather than pulled in as a dependency:
//! it is forty lines, it must round exactly the way the exporter rounds, and a
//! crate that changes its rounding in a patch release would silently move every
//! model's numbers.

use crate::contract::infer::Precision;

/// The gap between adjacent binary16 subnormals: `2^-24`.
const SUBNORMAL_STEP: f32 = 5.960_464_5e-8;

/// Round an `f32` to the nearest IEEE binary16 value and back.
///
/// Ties go to even, and the result is exactly what a half-precision device would
/// hold. Values beyond the binary16 range saturate to infinity, which is what the
/// hardware does; a model that overflows fp16 is a model whose precision policy
/// must forbid fp16, and that is recorded in its card rather than hidden here.
#[must_use]
pub fn round_to_f16(value: f32) -> f32 {
    f16_bits_to_f32(f32_to_f16_bits(value))
}

/// The binary16 bit pattern of an `f32`, rounding to nearest even.
#[must_use]
pub fn f32_to_f16_bits(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exponent = i32::try_from((bits >> 23) & 0xff).unwrap_or(0);
    let mantissa = bits & 0x007f_ffff;

    if exponent == 0xff {
        // Infinity or NaN. A NaN keeps a non-zero mantissa so it stays a NaN.
        let mantissa16 = if mantissa == 0 { 0 } else { 0x0200 };
        return sign | 0x7c00 | mantissa16;
    }

    // Rebase the exponent: 127 for binary32, 15 for binary16.
    let unbiased = exponent - 127 + 15;

    if unbiased >= 0x1f {
        return sign | 0x7c00; // Overflow saturates to infinity.
    }

    if unbiased <= 0 {
        if unbiased < -10 {
            return sign; // Underflows to a signed zero.
        }
        // Subnormal: shift the implicit leading one back into the mantissa.
        let mantissa = mantissa | 0x0080_0000;
        let shift = (14 - unbiased) as u32;
        let value = mantissa >> shift;
        let round_bit = 1u32 << (shift - 1);
        let round_up =
            (mantissa & round_bit) != 0 && ((mantissa & (round_bit - 1)) != 0 || (value & 1) != 0);
        return sign | ((value as u16) + u16::from(round_up));
    }

    let value = ((unbiased as u32) << 10) | (mantissa >> 13);
    let round_bit = 1u32 << 12;
    let round_up =
        (mantissa & round_bit) != 0 && ((mantissa & (round_bit - 1)) != 0 || (value & 1) != 0);
    sign | ((value as u16) + u16::from(round_up))
}

/// The `f32` value of a binary16 bit pattern. Exact in every case.
#[must_use]
pub fn f16_bits_to_f32(bits: u16) -> f32 {
    let sign = u32::from(bits & 0x8000) << 16;
    let exponent = u32::from((bits >> 10) & 0x1f);
    let mantissa = u32::from(bits & 0x03ff);

    if exponent == 0 {
        if mantissa == 0 {
            return f32::from_bits(sign);
        }
        // Subnormal: the value is exactly `mantissa * 2^-24`, and both factors are
        // representable in binary32, so the product is exact. Computing it rather
        // than re-normalising the bit pattern by hand is shorter and has one fewer
        // place to be off by one.
        let magnitude = mantissa as f32 * SUBNORMAL_STEP;
        return if sign == 0 { magnitude } else { -magnitude };
    }

    if exponent == 0x1f {
        return f32::from_bits(sign | 0x7f80_0000 | (mantissa << 13));
    }

    f32::from_bits(sign | ((exponent + 127 - 15) << 23) | (mantissa << 13))
}

/// A per-tensor affine quantisation: `real = scale * (quantised - zero_point)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuantParams {
    /// Step between adjacent quantised values.
    pub scale: f32,
    /// The quantised value that represents real zero.
    pub zero_point: i8,
}

impl QuantParams {
    /// Choose parameters covering an observed range, asymmetric int8.
    ///
    /// Zero is always representable exactly. That is not an aesthetic choice:
    /// padded convolutions insert real zeros, and a quantisation that renders
    /// zero as -0.004 puts a grey halo around every padded edge.
    #[must_use]
    pub fn from_range(min: f32, max: f32) -> Self {
        let min = min.min(0.0);
        let max = max.max(0.0);
        let span = max - min;
        if span <= f32::EPSILON {
            return Self {
                scale: 1.0,
                zero_point: 0,
            };
        }
        let scale = span / 255.0;
        let zero_point = (-128.0 - min / scale).round().clamp(-128.0, 127.0) as i8;
        Self { scale, zero_point }
    }

    /// Parameters covering the values actually present in a tensor.
    #[must_use]
    pub fn observe(values: &[f32]) -> Self {
        let mut min = 0.0f32;
        let mut max = 0.0f32;
        for value in values {
            min = min.min(*value);
            max = max.max(*value);
        }
        Self::from_range(min, max)
    }

    /// Round one real value to its quantised representation.
    #[must_use]
    pub fn quantise(self, value: f32) -> i8 {
        let raw = (value / self.scale).round() + f32::from(self.zero_point);
        raw.clamp(-128.0, 127.0) as i8
    }

    /// Recover the real value a quantised value stands for.
    #[must_use]
    pub fn dequantise(self, value: i8) -> f32 {
        (f32::from(value) - f32::from(self.zero_point)) * self.scale
    }

    /// Round a real value through the quantised grid and back.
    #[must_use]
    pub fn round_trip(self, value: f32) -> f32 {
        self.dequantise(self.quantise(value))
    }
}

/// Apply a precision to a whole tensor in place.
///
/// This is how the reference backend makes `Fp16` and `Int8` real: weights are
/// rounded once at load, activations are rounded after every operator, and the
/// error that accumulates is the error the parity harness measures.
pub fn apply_precision(values: &mut [f32], precision: Precision) {
    match precision {
        Precision::Fp32 => {}
        Precision::Fp16 => {
            for value in values.iter_mut() {
                *value = round_to_f16(*value);
            }
        }
        Precision::Int8 => {
            let params = QuantParams::observe(values);
            for value in values.iter_mut() {
                *value = params.round_trip(*value);
            }
        }
    }
}

/// The largest absolute difference between two tensors of the same length.
///
/// Returns `None` when the lengths differ, because a parity comparison between
/// differently shaped outputs is a bug in the harness, not a large error.
#[must_use]
pub fn max_abs_diff(left: &[f32], right: &[f32]) -> Option<f32> {
    if left.len() != right.len() {
        return None;
    }
    let mut worst = 0.0f32;
    for (a, b) in left.iter().zip(right.iter()) {
        worst = worst.max((a - b).abs());
    }
    Some(worst)
}

/// Dot product with a fixed accumulation order.
///
/// Every inner product in this crate goes through here. The loop is deliberately
/// sequential: a parallel reduction would sum in an order that depends on the
/// core count, and D7 makes floating-point order part of our contract.
#[must_use]
pub fn dot(left: &[f32], right: &[f32]) -> f32 {
    let mut total = 0.0f32;
    for (a, b) in left.iter().zip(right.iter()) {
        total += a * b;
    }
    total
}
