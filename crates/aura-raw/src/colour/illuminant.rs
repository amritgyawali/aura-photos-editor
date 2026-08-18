//! Illuminants: colour temperature, the Planckian locus, and the uniform chromaticity
//! plane a white balance is actually solved in.
//!
//! # Why this lives in `aura-raw`
//!
//! It arrived in phase 14 as `aura_render::colour::cct_to_xy`, one function serving one
//! caller. Phase 15 became its second consumer - the white-balance solver scores
//! hypotheses in `u'v'` and has to be able to turn its answer into the kelvin and tint a
//! slider shows - and phase 10's rule applies: **two copies of a colour transform are two
//! renders that drift apart while looking identical.** So the maths moved down to the
//! crate that owns colour science, `aura_render::colour` re-exports it, and there is one
//! Planckian locus in the product.
//!
//! # The three spaces, and which one to solve in
//!
//! * **Kelvin and tint** is what a photographer sets. It is not a space: a distance in
//!   kelvin is not a distance in colour, because 200 K at 2800 K is a visible shift and
//!   200 K at 9000 K is invisible.
//! * **CIE xy** is where the Planckian locus is conventionally drawn. It is not perceptually
//!   uniform either: the green region is enormous and the blue region is tiny.
//! * **CIE 1976 `u'v'`** is uniform enough that a single radius means roughly the same
//!   amount of visible difference wherever it sits, which is the property a skin-locus
//!   constraint needs and the reason phase 15 solves here.
//!
//! [`cct_to_uv`] and [`cct_from_uv`] are the bridge, and they are deliberately not exact
//! inverses of each other: the first walks a defined locus, the second finds the nearest
//! point on it, and a chromaticity off the locus has no exact temperature at all. That is
//! not an approximation error - it is the reason [`duv`] exists.

use crate::colour::matrix::{apply, Vec3, SRGB_TO_XYZ_D65};

/// Chromaticity of a black body or daylight illuminant at `kelvin`, in CIE xy.
///
/// Kang et al. (2002), the piecewise cubic approximation every raw converter uses, valid
/// from 1667 K to 25000 K. Outside that range the endpoints are held.
#[must_use]
pub fn cct_to_xy(kelvin: f32) -> (f32, f32) {
    let t = f64::from(kelvin.clamp(1667.0, 25_000.0));
    let inv = 1.0e9 / (t * t * t);
    let inv2 = 1.0e6 / (t * t);
    let inv1 = 1.0e3 / t;

    let x = if t < 4000.0 {
        -0.266_123_9 * inv - 0.234_358_9 * inv2 + 0.877_695_6 * inv1 + 0.179_910
    } else {
        -3.025_846_9 * inv + 2.107_037_9 * inv2 + 0.222_634_7 * inv1 + 0.240_390
    };

    let y = if t < 2222.0 {
        -1.106_381_4 * x * x * x - 1.348_110_2 * x * x + 2.185_558_3 * x - 0.202_196_8
    } else if t < 4000.0 {
        -0.954_947_6 * x * x * x - 1.374_185_9 * x * x + 2.091_370_1 * x - 0.167_488_7
    } else {
        3.081_758_0 * x * x * x - 5.873_386_7 * x * x + 3.751_130_0 * x - 0.370_014_8
    };

    (x as f32, y as f32)
}

/// CIE xy to CIE 1976 `u'v'`.
///
/// A degenerate `xy` - one whose denominator vanishes, which is off the diagram entirely -
/// returns D65's chromaticity rather than an infinity. A `NaN` propagating out of a
/// chromaticity conversion would reach a `total_cmp` in a solver and reorder a sort.
#[must_use]
pub fn xy_to_uv(x: f32, y: f32) -> [f32; 2] {
    let denominator = -2.0 * f64::from(x) + 12.0 * f64::from(y) + 3.0;
    if denominator.abs() < 1e-9 {
        return D65_UV;
    }
    [
        (4.0 * f64::from(x) / denominator) as f32,
        (9.0 * f64::from(y) / denominator) as f32,
    ]
}

/// CIE 1976 `u'v'` back to CIE xy.
#[must_use]
pub fn uv_to_xy(uv: [f32; 2]) -> (f32, f32) {
    let u = f64::from(uv[0]);
    let v = f64::from(uv[1]);
    let denominator = 6.0 * u - 16.0 * v + 12.0;
    if denominator.abs() < 1e-9 {
        return (0.3127, 0.3290);
    }
    (
        (9.0 * u / denominator) as f32,
        (4.0 * v / denominator) as f32,
    )
}

/// CIE XYZ to CIE 1976 `u'v'`.
#[must_use]
pub fn xyz_to_uv(xyz: Vec3) -> [f32; 2] {
    let denominator = xyz[0] + 15.0 * xyz[1] + 3.0 * xyz[2];
    if denominator.abs() < 1e-12 {
        return D65_UV;
    }
    [
        (4.0 * xyz[0] / denominator) as f32,
        (9.0 * xyz[1] / denominator) as f32,
    ]
}

/// Linear sRGB to CIE 1976 `u'v'`.
///
/// The path every pixel statistic in phase 15 takes: a proxy is sRGB-encoded, the analyser
/// linearises it once, and every chromaticity in the solve comes through here.
#[must_use]
pub fn linear_srgb_to_uv(rgb: [f32; 3]) -> [f32; 2] {
    let xyz = apply(
        SRGB_TO_XYZ_D65,
        [f64::from(rgb[0]), f64::from(rgb[1]), f64::from(rgb[2])],
    );
    xyz_to_uv(xyz)
}

/// Chromaticity of a black body or daylight illuminant at `kelvin`, in `u'v'`.
#[must_use]
pub fn cct_to_uv(kelvin: f32) -> [f32; 2] {
    let (x, y) = cct_to_xy(kelvin);
    xy_to_uv(x, y)
}

/// D65 in CIE 1976 `u'v'`. The reference white of the working space.
pub const D65_UV: [f32; 2] = [0.197_83, 0.468_32];

/// The linear sRGB of an illuminant at a chromaticity, normalised so `Y = 1`.
///
/// What a von Kries division needs: to divide an observed colour by the light that lit it,
/// the light has to be expressed in the same primaries the observation is. Normalised on
/// luminance rather than on green so that dividing by it changes colour and not brightness -
/// the same discipline `aura_render::colour::white_balance` keeps for the slider.
///
/// A chromaticity that maps to a negative primary - outside the sRGB gamut, which most
/// saturated stage lighting is - has its channels floored at a small positive number rather
/// than clamped to zero. A zero would make the division below infinite, and an infinity in a
/// chromaticity reaches a sort comparator and reorders it.
#[must_use]
pub fn uv_to_linear_srgb(uv: [f32; 2]) -> [f32; 3] {
    let (x, y) = uv_to_xy(uv);
    let y = f64::from(y).max(1e-6);
    let xyz: Vec3 = [f64::from(x) / y, 1.0, (1.0 - f64::from(x) - y) / y];
    let Some(inverse) = crate::colour::matrix::invert(SRGB_TO_XYZ_D65) else {
        return [1.0; 3];
    };
    let rgb = apply(inverse, xyz);
    let mut out = [0.0f32; 3];
    for (slot, value) in out.iter_mut().zip(rgb.iter()) {
        *slot = (*value as f32).max(1e-4);
    }
    // Re-normalise on luminance: the inversion above preserves Y only when the chromaticity
    // is in gamut, and the floor just applied can move it.
    let luma = 0.2126 * out[0] + 0.7152 * out[1] + 0.0722 * out[2];
    if luma > 1e-6 {
        for slot in &mut out {
            *slot /= luma;
        }
    }
    out
}

/// Divide an observed linear colour by the light that lit it.
///
/// The von Kries transform, done in the observation's own primaries rather than in a cone
/// space. That is a deliberate simplification and it is worth naming: a full CAT02 or
/// Bradford adaptation is more accurate for large temperature moves, and this is used to ask
/// a *relative* question - "does this surface look like the same surface it looked like
/// under the other light" - where the two agree to well inside the tolerance the answer is
/// read at. `aura_raw::colour::matrix::bradford` is here for the cases where the difference
/// matters, and building a camera profile is one of them.
#[must_use]
pub fn adapt(observed: [f32; 3], illuminant_uv: [f32; 2]) -> [f32; 3] {
    let light = uv_to_linear_srgb(illuminant_uv);
    let mut out = [0.0f32; 3];
    for ((slot, value), divisor) in out.iter_mut().zip(observed.iter()).zip(light.iter()) {
        *slot = *value / divisor.max(1e-4);
    }
    out
}

/// The nearest correlated colour temperature to a chromaticity, in kelvin.
///
/// McCamy's cubic approximation, which is accurate to a few kelvin between 2000 K and
/// 12500 K and degrades gracefully outside it. The result is **clamped to the range
/// [`cct_to_xy`] is valid over**, because a caller that received 47000 K from a chromaticity
/// that is simply not near the locus would then feed it back through a conversion that
/// silently held the endpoint anyway.
///
/// A correlated colour temperature is only meaningful within about `0.05` [`duv`] of the
/// locus. Past that, ask [`duv`] rather than trusting this number.
#[must_use]
pub fn cct_from_uv(uv: [f32; 2]) -> f32 {
    let (x, y) = uv_to_xy(uv);
    let denominator = 0.1858 - f64::from(y);
    if denominator.abs() < 1e-9 {
        return 6500.0;
    }
    let n = (f64::from(x) - 0.3320) / denominator;
    let cct = 449.0 * n * n * n + 3525.0 * n * n + 6823.3 * n + 5520.33;
    (cct as f32).clamp(1667.0, 25_000.0)
}

/// Signed distance from the Planckian locus, in `u'v'`.
///
/// Positive is above the locus (green); negative is below it (magenta). This is the number
/// that separates a tungsten bulb from a fluorescent tube at the same nominal temperature,
/// and it is what makes a CCT-only description of reception lighting wrong.
///
/// Measured against the nearest locus point found by [`cct_from_uv`], which is why the two
/// belong together: the temperature and the distance off it are one answer with two halves.
#[must_use]
pub fn duv(uv: [f32; 2]) -> f32 {
    let locus = cct_to_uv(cct_from_uv(uv));
    let du = uv[0] - locus[0];
    let dv = uv[1] - locus[1];
    let magnitude = du.hypot(dv);
    if dv >= 0.0 {
        magnitude
    } else {
        -magnitude
    }
}

/// How far apart two chromaticities are, in `u'v'`.
///
/// One function rather than a hypot written out in six places, because six copies is six
/// chances to write it as a squared distance in one of them.
#[must_use]
pub fn uv_distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_daylight_locus_moves_the_right_way() {
        let (x_warm, _) = cct_to_xy(2700.0);
        let (x_cool, _) = cct_to_xy(9000.0);
        assert!(x_warm > x_cool, "a low temperature sits further along x");
    }

    #[test]
    fn xy_and_uv_round_trip() {
        for kelvin in [2000.0f32, 2800.0, 4000.0, 5500.0, 6500.0, 9000.0, 20_000.0] {
            let (x, y) = cct_to_xy(kelvin);
            let uv = xy_to_uv(x, y);
            let (x2, y2) = uv_to_xy(uv);
            assert!((x - x2).abs() < 1e-4, "{kelvin}: {x} vs {x2}");
            assert!((y - y2).abs() < 1e-4, "{kelvin}: {y} vs {y2}");
        }
    }

    #[test]
    fn a_locus_point_recovers_its_own_temperature() {
        // McCamy is an approximation of a lookup of an approximation; a few per cent is
        // what it is worth, and claiming better would be claiming something false.
        for kelvin in [2500.0f32, 3200.0, 4500.0, 5500.0, 6500.0, 8000.0] {
            let back = cct_from_uv(cct_to_uv(kelvin));
            let error = (back - kelvin).abs() / kelvin;
            assert!(error < 0.04, "{kelvin} came back as {back}");
        }
    }

    #[test]
    fn a_locus_point_has_almost_no_duv() {
        for kelvin in [2800.0f32, 4000.0, 5500.0, 7000.0] {
            assert!(duv(cct_to_uv(kelvin)).abs() < 0.004, "{kelvin}");
        }
    }

    #[test]
    fn a_green_cast_reads_as_positive_duv() {
        // A fluorescent tube sits above the locus. Push a 4000 K point up in v' and the
        // sign has to say so, because that sign is what tells a solver the light is not a
        // black body at all.
        let mut uv = cct_to_uv(4000.0);
        uv[1] += 0.02;
        assert!(duv(uv) > 0.0, "{uv:?} -> {}", duv(uv));
    }

    #[test]
    fn a_neutral_linear_pixel_lands_on_d65() {
        let uv = linear_srgb_to_uv([0.5, 0.5, 0.5]);
        assert!(uv_distance(uv, D65_UV) < 1e-3, "{uv:?}");
    }

    #[test]
    fn a_degenerate_chromaticity_returns_d65_rather_than_a_nan() {
        let uv = xyz_to_uv([0.0, 0.0, 0.0]);
        assert!(uv[0].is_finite() && uv[1].is_finite(), "{uv:?}");
    }

    #[test]
    fn d65_light_is_equal_energy_in_linear_srgb() {
        let rgb = uv_to_linear_srgb(D65_UV);
        for channel in rgb {
            assert!((channel - 1.0).abs() < 0.02, "{rgb:?}");
        }
    }

    #[test]
    fn adapting_a_surface_by_its_own_light_makes_it_neutral() {
        // A white wall under tungsten is orange in the file. Divide by the tungsten and it
        // is a white wall again - which is the whole of what the skin constraint asks.
        let tungsten = cct_to_uv(2800.0);
        let light = uv_to_linear_srgb(tungsten);
        let observed = [light[0] * 0.6, light[1] * 0.6, light[2] * 0.6];
        let reflectance = adapt(observed, tungsten);
        let uv = linear_srgb_to_uv(reflectance);
        assert!(uv_distance(uv, D65_UV) < 5e-3, "{uv:?}");
    }

    #[test]
    fn adapting_by_the_wrong_light_leaves_a_cast() {
        let tungsten = cct_to_uv(2800.0);
        let light = uv_to_linear_srgb(tungsten);
        let observed = [light[0] * 0.6, light[1] * 0.6, light[2] * 0.6];
        let reflectance = adapt(observed, cct_to_uv(6500.0));
        let uv = linear_srgb_to_uv(reflectance);
        assert!(
            uv_distance(uv, D65_UV) > 0.02,
            "a 3700 K error has to be visible: {uv:?}"
        );
    }

    #[test]
    fn a_saturated_stage_light_does_not_produce_an_infinity() {
        // Far outside the sRGB gamut: a deep magenta wash. The floor in
        // `uv_to_linear_srgb` is what keeps the division finite.
        let rgb = uv_to_linear_srgb([0.45, 0.20]);
        assert!(rgb.iter().all(|c| c.is_finite() && *c > 0.0), "{rgb:?}");
        let out = adapt([0.3, 0.1, 0.4], [0.45, 0.20]);
        assert!(out.iter().all(|c| c.is_finite()), "{out:?}");
    }
}
