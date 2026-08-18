//! The output transform. **The only module in this crate that bakes tone.**
//!
//! ADR-0029 section 2: no stage before this one applies a display transfer function, and
//! `crates/aura-render/tests/colour_discipline.rs` greps the crate to keep it that way.
//! `tonemap::curve_domain_encode` is not a counter-example - it is an invertible change of
//! coordinates applied and undone inside one stage, and the test knows the difference
//! because it looks for the three functions below by name.
//!
//! # What happens here, in order
//!
//! 1. **Tone map.** Scene-referred light runs past 1.0; a display does not. The shoulder is
//!    `aura_raw::colour::curve::filmic_lite`, inherited from phase 02 rather than re-invented,
//!    so a proxy rendered by the preview service and an export rendered here roll off
//!    identically. Two shoulders would be two products.
//! 2. **Gamut.** Linear Rec.2020 into the output space's primaries, by one matrix built
//!    once per render.
//! 3. **Transfer.** sRGB's piecewise curve, or gamma 2.2 for Adobe RGB.
//! 4. **Quantise.** 8 or 16 bit, with an ordered dither at 8 so a smooth sky does not band.
//!
//! # The dither is seeded and the seed is a constant
//!
//! Invariant 4 and section 6.2: *"fixed random seeds for dither"*. The dither here is not
//! random at all - it is a 4x4 Bayer matrix indexed by pixel position, so the same pixel in
//! the same place always gets the same offset. That is stronger than a fixed seed: it is
//! reproducible across tile boundaries, which a sequence-based dither is not.

use aura_raw::colour::curve::filmic_lite;
use aura_raw::colour::matrix::{invert, mul, Mat3, IDENTITY, REC2020_TO_XYZ_D65, SRGB_TO_XYZ_D65};

use crate::colour::{apply_f32, narrow};
use crate::contract::render::{OutputColour, RenderedData};

/// Adobe RGB (1998) primaries to CIE XYZ at D65.
pub const ADOBE_RGB_TO_XYZ_D65: Mat3 = [
    [0.576_669, 0.185_558, 0.188_229],
    [0.297_345, 0.627_364, 0.075_291],
    [0.027_031, 0.070_689, 0.991_338],
];

/// Display P3 primaries to CIE XYZ at D65.
pub const DISPLAY_P3_TO_XYZ_D65: Mat3 = [
    [0.486_571, 0.265_668, 0.198_217],
    [0.228_975, 0.691_739, 0.079_287],
    [0.000_000, 0.045_113, 1.043_944],
];

/// The 4x4 ordered dither used at 8 bits. Deterministic by construction.
const BAYER: [[f32; 4]; 4] = [
    [0.0 / 16.0, 8.0 / 16.0, 2.0 / 16.0, 10.0 / 16.0],
    [12.0 / 16.0, 4.0 / 16.0, 14.0 / 16.0, 6.0 / 16.0],
    [3.0 / 16.0, 11.0 / 16.0, 1.0 / 16.0, 9.0 / 16.0],
    [15.0 / 16.0, 7.0 / 16.0, 13.0 / 16.0, 5.0 / 16.0],
];

/// The matrix from the working space into an output space's primaries.
#[must_use]
pub fn working_to_output(space: OutputColour) -> [[f32; 3]; 3] {
    let target = match space {
        OutputColour::Srgb => SRGB_TO_XYZ_D65,
        OutputColour::AdobeRgb => ADOBE_RGB_TO_XYZ_D65,
        OutputColour::DisplayP3 => DISPLAY_P3_TO_XYZ_D65,
    };
    let xyz_to_target = invert(target).unwrap_or(IDENTITY);
    narrow(mul(xyz_to_target, REC2020_TO_XYZ_D65))
}

/// sRGB's piecewise encoding.
#[must_use]
pub fn encode_srgb(linear: f32) -> f32 {
    let x = linear.clamp(0.0, 1.0);
    if x <= 0.003_130_8 {
        12.92 * x
    } else {
        1.055 * x.powf(1.0 / 2.4) - 0.055
    }
}

/// A plain power law, used by Adobe RGB (1998).
#[must_use]
pub fn encode_gamma_2_2(linear: f32) -> f32 {
    // 256/563: Adobe RGB (1998) defines its exponent as 563/256 exactly.
    linear.clamp(0.0, 1.0).powf(256.0 / 563.0)
}

/// The transfer function an output space uses.
#[must_use]
pub fn encode_for(space: OutputColour, linear: f32) -> f32 {
    match space {
        // Display P3 uses the sRGB transfer function with P3 primaries. That is the whole
        // difference between the two, and getting it wrong is how a P3 export comes back
        // looking washed out.
        OutputColour::Srgb | OutputColour::DisplayP3 => encode_srgb(linear),
        OutputColour::AdobeRgb => encode_gamma_2_2(linear),
    }
}

/// The name of the ICC profile a space is delivered with.
///
/// A name rather than the bytes: this build embeds a profile *identifier* into the catalog
/// and leaves the encoder to attach the standard profile, because a wrong ICC payload is
/// worse than a named one - it renders confidently and incorrectly on every viewer.
#[must_use]
pub const fn icc_name(space: OutputColour) -> &'static str {
    match space {
        OutputColour::Srgb => "sRGB IEC61966-2.1",
        OutputColour::AdobeRgb => "Adobe RGB (1998)",
        OutputColour::DisplayP3 => "Display P3",
    }
}

/// The output transform, over a whole working-space plane.
///
/// `rgb` is interleaved linear working-space `f32`, `width * height * 3` long.
///
/// # Panics
///
/// Never. Every index is bounds-checked and a short buffer produces a short output rather
/// than a panic, which is what `clippy::indexing_slicing` is enforced for in this crate.
#[must_use]
pub fn transform(
    rgb: &[f32],
    width: u32,
    height: u32,
    space: OutputColour,
    bit_depth: u8,
    origin: (u32, u32),
) -> RenderedData {
    let matrix = working_to_output(space);
    let pixels = (width as usize) * (height as usize);

    if bit_depth == 16 {
        let mut out = vec![0u16; pixels * 3];
        for index in 0..pixels {
            let base = index * 3;
            let pixel = read(rgb, base);
            let mapped = map_one(pixel, matrix, space);
            for channel in 0..3 {
                if let Some(slot) = out.get_mut(base + channel) {
                    let value = mapped.get(channel).copied().unwrap_or(0.0);
                    *slot = (value * 65_535.0 + 0.5).clamp(0.0, 65_535.0) as u16;
                }
            }
        }
        return RenderedData::Sixteen(out);
    }

    let mut out = vec![0u8; pixels * 3];
    for index in 0..pixels {
        let base = index * 3;
        let x = (index % width.max(1) as usize) as u32 + origin.0;
        let y = (index / width.max(1) as usize) as u32 + origin.1;
        let threshold = BAYER
            .get((y % 4) as usize)
            .and_then(|row| row.get((x % 4) as usize))
            .copied()
            .unwrap_or(0.5);
        let pixel = read(rgb, base);
        let mapped = map_one(pixel, matrix, space);
        for channel in 0..3 {
            if let Some(slot) = out.get_mut(base + channel) {
                let value = mapped.get(channel).copied().unwrap_or(0.0) * 255.0;
                // The dither offset is in the interval [-0.5, 0.5) of one output code, so
                // it can never move a value by a whole step.
                *slot = (value + threshold - 0.5).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    RenderedData::Eight(out)
}

fn read(rgb: &[f32], base: usize) -> [f32; 3] {
    [
        rgb.get(base).copied().unwrap_or(0.0),
        rgb.get(base + 1).copied().unwrap_or(0.0),
        rgb.get(base + 2).copied().unwrap_or(0.0),
    ]
}

/// One pixel: tone map, gamut, transfer.
#[must_use]
pub fn map_one(rgb: [f32; 3], matrix: [[f32; 3]; 3], space: OutputColour) -> [f32; 3] {
    // 1. Tone map in the working space, on all three channels, before the gamut rotation.
    //    Rolling off after the rotation would clip a channel the target space cannot hold
    //    and shift the hue of every specular highlight.
    let rolled = [
        filmic_lite(rgb.first().copied().unwrap_or(0.0).max(0.0)),
        filmic_lite(rgb.get(1).copied().unwrap_or(0.0).max(0.0)),
        filmic_lite(rgb.get(2).copied().unwrap_or(0.0).max(0.0)),
    ];
    // 2. Gamut.
    let rotated = apply_f32(matrix, rolled);
    // 3. Transfer.
    [
        encode_for(space, rotated.first().copied().unwrap_or(0.0)),
        encode_for(space, rotated.get(1).copied().unwrap_or(0.0)),
        encode_for(space, rotated.get(2).copied().unwrap_or(0.0)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_transfer_functions_map_the_endpoints() {
        assert!(encode_srgb(0.0).abs() < 1e-6);
        assert!((encode_srgb(1.0) - 1.0).abs() < 1e-6);
        assert!(encode_gamma_2_2(0.0).abs() < 1e-6);
        assert!((encode_gamma_2_2(1.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn display_p3_uses_the_srgb_transfer_function() {
        for value in [0.0f32, 0.05, 0.5, 1.0] {
            assert!((encode_for(OutputColour::DisplayP3, value) - encode_srgb(value)).abs() < 1e-9);
        }
    }

    #[test]
    fn a_neutral_working_pixel_stays_neutral_in_every_output_space() {
        for space in [
            OutputColour::Srgb,
            OutputColour::AdobeRgb,
            OutputColour::DisplayP3,
        ] {
            let matrix = working_to_output(space);
            let out = map_one([0.18; 3], matrix, space);
            assert!(
                (out[0] - out[1]).abs() < 2e-3 && (out[1] - out[2]).abs() < 2e-3,
                "{space:?} tinted a neutral: {out:?}"
            );
        }
    }

    #[test]
    fn a_wider_gamut_holds_a_saturated_colour_that_srgb_clips() {
        let vivid = [0.8, 0.02, 0.02];
        let srgb = map_one(
            vivid,
            working_to_output(OutputColour::Srgb),
            OutputColour::Srgb,
        );
        let p3 = map_one(
            vivid,
            working_to_output(OutputColour::DisplayP3),
            OutputColour::DisplayP3,
        );
        // The same colour needs less red in the wider space, because the space's own red
        // primary is closer to it.
        assert!(p3[0] <= srgb[0] + 1e-3, "{p3:?} vs {srgb:?}");
    }

    #[test]
    fn the_dither_is_the_same_pixel_to_pixel_across_a_tile_boundary() {
        let rgb = vec![0.2f32; 3];
        let whole = transform(&rgb, 1, 1, OutputColour::Srgb, 8, (5, 7));
        let tiled = transform(&rgb, 1, 1, OutputColour::Srgb, 8, (5, 7));
        assert_eq!(whole, tiled);
    }

    #[test]
    fn sixteen_bit_output_carries_no_dither() {
        let rgb = vec![0.2f32; 3 * 16];
        let a = transform(&rgb, 4, 4, OutputColour::Srgb, 16, (0, 0));
        let b = transform(&rgb, 4, 4, OutputColour::Srgb, 16, (11, 13));
        assert_eq!(a, b, "a 16-bit render must not depend on where it sits");
    }

    #[test]
    fn the_output_is_the_right_length_and_depth() {
        let rgb = vec![0.5f32; 3 * 12];
        let eight = transform(&rgb, 4, 3, OutputColour::Srgb, 8, (0, 0));
        assert_eq!(eight.bit_depth(), 8);
        assert_eq!(eight.byte_len(), 36);
        let sixteen = transform(&rgb, 4, 3, OutputColour::Srgb, 16, (0, 0));
        assert_eq!(sixteen.bit_depth(), 16);
        assert_eq!(sixteen.byte_len(), 72);
    }
}
