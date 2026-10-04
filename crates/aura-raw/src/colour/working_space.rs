//! The working space, defined once and converted at the boundaries.
//!
//! Invariant 8 of the operating manual says colour maths happens in linear
//! scene-referred light. This module is where that sentence becomes numbers:
//! **linear Rec.2020, D65 white, no transfer function**. Everything that enters
//! the pipeline is converted into it once, and everything that leaves converts
//! out of it once.
//!
//! Rec.2020 rather than sRGB because a wide gamut cannot clip a saturated
//! stage light or a sari into a hue shift before the grade even starts; sRGB
//! primaries would quietly discard colours that the sensor really recorded.

use std::sync::OnceLock;

use crate::colour::matrix::{
    apply, invert, mul, normalise_to_white, Mat3, Vec3, IDENTITY, REC2020_TO_XYZ_D65,
    SRGB_TO_XYZ_D65, WHITE_D50, WHITE_D65,
};
use crate::contract::pixels::ColourSpace;

/// The one working space. Named so that a grep for it finds every conversion.
pub const WORKING_SPACE: ColourSpace = ColourSpace::LinearRec2020;

/// CIE XYZ (D65) to linear Rec.2020.
#[must_use]
pub fn xyz_d65_to_rec2020() -> Mat3 {
    static CACHE: OnceLock<Mat3> = OnceLock::new();
    *CACHE.get_or_init(|| invert(REC2020_TO_XYZ_D65).unwrap_or(IDENTITY))
}

/// CIE XYZ (D65) to linear sRGB.
#[must_use]
pub fn xyz_d65_to_srgb() -> Mat3 {
    static CACHE: OnceLock<Mat3> = OnceLock::new();
    *CACHE.get_or_init(|| invert(SRGB_TO_XYZ_D65).unwrap_or(IDENTITY))
}

/// Linear Rec.2020 to linear sRGB, the last matrix before the display curve.
#[must_use]
pub fn rec2020_to_srgb() -> Mat3 {
    static CACHE: OnceLock<Mat3> = OnceLock::new();
    *CACHE.get_or_init(|| mul(xyz_d65_to_srgb(), REC2020_TO_XYZ_D65))
}

/// Build the camera-to-working matrix from a DNG-style `XYZ(illuminant) ->
/// camera` matrix, which is the form both DNG files and the bundled table store.
///
/// The chain is: invert to get camera -> XYZ(illuminant), adapt that illuminant
/// to D65 with Bradford, normalise so a neutral camera signal lands exactly on
/// white, then rotate into Rec.2020 primaries.
///
/// Returns `None` when the supplied matrix is singular; a profile that cannot be
/// inverted is refused rather than approximated.
#[must_use]
pub fn camera_to_working(xyz_to_camera: Mat3, source_white: Vec3) -> Option<Mat3> {
    let camera_to_xyz = invert(xyz_to_camera)?;
    let adapted = mul(
        crate::colour::matrix::bradford(source_white, WHITE_D65),
        camera_to_xyz,
    );
    let neutralised = normalise_to_white(adapted, WHITE_D65);
    Some(mul(xyz_d65_to_rec2020(), neutralised))
}

/// The D50 white point, re-exported so callers building a chain do not have to
/// reach into the matrix module for one constant.
pub const D50: Vec3 = WHITE_D50;

/// Convert one triple from the working space into linear sRGB.
#[must_use]
pub fn working_to_linear_srgb(rgb: Vec3) -> Vec3 {
    apply(rec2020_to_srgb(), rgb)
}

/// Convert one triple from the working space into CIE XYZ (D65), the input of
/// the dE2000 comparison used by the golden suite.
#[must_use]
pub fn working_to_xyz(rgb: Vec3) -> Vec3 {
    apply(REC2020_TO_XYZ_D65, rgb)
}
