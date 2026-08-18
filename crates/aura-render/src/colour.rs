//! Colour: highlight recovery, white balance, and the camera matrix.
//!
//! Everything here happens **before** the creative stages, and the order is the whole point
//! of the module. ADR-0029 section 2:
//!
//! ```text
//! camera native RGB
//!   -> highlight recovery      (clipped channels reconstructed from unclipped ones)
//!   -> white balance           (temperature and tint as channel multipliers)
//!   -> camera matrix           (into linear Rec.2020 at D65)
//!   === working space ===
//! ```
//!
//! **Highlight recovery runs before white balance.** A clipped green channel reconstructed
//! after a 1.9x red multiplier is a magenta veil across every white dress and every window,
//! and it is the single most visible failure a wedding renderer can have.
//! `graph::Stage`'s ordering is asserted by a test rather than trusted to this comment.
//!
//! # What the working space is
//!
//! Linear Rec.2020 at D65, defined once in `aura_raw::colour::working_space` and inherited
//! here rather than redefined. Invariant 8, and phase 02's decision.

use aura_raw::colour::matrix::{apply, Mat3, Vec3};
use aura_raw::colour::working_space::xyz_d65_to_rec2020;

/// The reference temperature the white-balance sliders are relative to.
///
/// 5500 K: daylight, the neutral position of the slider in every editor a photographer has
/// used, and the value `fixtures::neutral` writes. A recipe at 5500/0 multiplies by one.
pub const REFERENCE_KELVIN: f32 = 5500.0;

/// Reconstruct a clipped channel from its unclipped neighbours.
///
/// The classic problem: a white dress in a window blows the green channel first, and a
/// renderer that leaves it clipped turns the highlight magenta the moment white balance
/// multiplies red and blue past it.
///
/// The reconstruction is the one a single pixel can honestly support: **a clipped channel is
/// lifted toward the brightest channel that is still carrying signal.** That is the "blown
/// highlights tend toward neutral" model every raw converter starts from, and it is right for
/// the reason the failure it fixes exists - the channels clip at *different* levels, green
/// first on nearly every sensor, so a highlight bright enough to blow green is a highlight
/// whose red is still saying how bright it really was.
///
/// Per-channel clip points are therefore not decoration: with `clip = [1.0, 1.0, 1.0]` no
/// pixel can have a clipped channel and a brighter unclipped one, and the function correctly
/// does nothing. The real clip vector comes from the sensor's saturation levels.
///
/// When all three channels are clipped there is nothing left to reconstruct from and the
/// pixel is left alone - a fully blown highlight has no colour to recover.
///
/// `strength` is `recipe.global.highlights` mapped to `0..1`; at zero the function is the
/// identity, so a photographer who wants the clipping can have it.
#[must_use]
pub fn recover_highlights(rgb: [f32; 3], clip: [f32; 3], strength: f32) -> [f32; 3] {
    if strength <= 0.0 {
        return rgb;
    }
    let mut clipped = [false; 3];
    let mut count = 0usize;
    for ((slot, value), point) in clipped.iter_mut().zip(rgb.iter()).zip(clip.iter()) {
        *slot = value >= point;
        count += usize::from(*slot);
    }
    if count == 0 || count == 3 {
        return rgb;
    }

    // The brightest channel still carrying signal is the only evidence this pixel has about
    // how bright it really was.
    let mut brightest = 0.0f32;
    for (value, is_clipped) in rgb.iter().zip(clipped.iter()) {
        if !is_clipped {
            brightest = brightest.max(*value);
        }
    }

    let mut out = rgb;
    for ((value, is_clipped), point) in out.iter_mut().zip(clipped.iter()).zip(clip.iter()) {
        if *is_clipped {
            // Never *below* the clip point: recovery lifts a blown channel toward where it
            // would have been, and a recovery that darkened one would be inventing detail in
            // the other direction.
            let estimate = brightest.max(*point);
            *value += (estimate - *value) * strength;
        }
    }
    out
}

/// Chromaticity of a black body or daylight illuminant at `kelvin`.
///
/// **Moved to `aura_raw::colour::illuminant` in phase 15** and re-exported here, because
/// phase 15's white-balance solver became its second consumer. Phase 10's rule, applied to a
/// colour transform rather than to a warp: two copies of the Planckian locus are two renders
/// that drift apart while looking identical, and the drift would show up as a temperature
/// slider that means one thing in the develop panel and another in the tone estimate behind
/// it. Every caller in this crate goes through this name, so nothing above had to change.
pub use aura_raw::colour::illuminant::cct_to_xy;

/// The white-balance multipliers a temperature and tint imply, in the working space.
///
/// Green is normalised to exactly 1.0, so the slider changes colour and never exposure -
/// which is what makes the exposure stage's job independent of it and what stops a
/// photographer chasing brightness while correcting a colour cast.
///
/// `tint` is Adobe's convention: **positive is magenta**, so a positive tint scales green
/// down. The divisor of 300 is chosen so that the slider's documented range of +/-150 spans
/// roughly a factor of two in the green channel, which matches what the control does in
/// every editor a photographer has used.
#[must_use]
pub fn white_balance(kelvin: f32, tint: f32) -> [f32; 3] {
    let target = illuminant_rgb(kelvin);
    let reference = illuminant_rgb(REFERENCE_KELVIN);

    let mut m = [0.0f32; 3];
    for i in 0..3 {
        let t = target.get(i).copied().unwrap_or(1.0).max(1e-6);
        let r = reference.get(i).copied().unwrap_or(1.0);
        if let Some(slot) = m.get_mut(i) {
            *slot = (r / t) as f32;
        }
    }

    let green_scale = 1.0 / (1.0 + tint / 300.0).max(1e-3);
    if let Some(slot) = m.get_mut(1) {
        *slot *= green_scale;
    }

    // Normalise on green. See the doc comment: this is what keeps the slider chromatic.
    let g = m.get(1).copied().unwrap_or(1.0).max(1e-6);
    [
        m.first().copied().unwrap_or(1.0) / g,
        1.0,
        m.get(2).copied().unwrap_or(1.0) / g,
    ]
}

/// The working-space RGB of an illuminant at `kelvin`, normalised to `Y = 1`.
#[must_use]
fn illuminant_rgb(kelvin: f32) -> Vec3 {
    let (x, y) = cct_to_xy(kelvin);
    let y = f64::from(y).max(1e-6);
    let xyz: Vec3 = [f64::from(x) / y, 1.0, (1.0 - f64::from(x) - y) / y];
    apply(xyz_d65_to_rec2020(), xyz)
}

/// Apply a 3x3 matrix to one pixel, in `f32`.
///
/// The matrix arithmetic in `aura_raw::colour::matrix` is `f64` because it builds and
/// inverts profiles once; per-pixel work is `f32`, which is ADR-0029 section 3's decision.
/// Converting the matrix once per image rather than once per pixel is the whole reason this
/// function exists separately.
#[must_use]
pub fn apply_f32(m: [[f32; 3]; 3], rgb: [f32; 3]) -> [f32; 3] {
    let mut out = [0.0f32; 3];
    for (row, slot) in m.iter().zip(out.iter_mut()) {
        *slot = row.first().copied().unwrap_or(0.0) * rgb.first().copied().unwrap_or(0.0)
            + row.get(1).copied().unwrap_or(0.0) * rgb.get(1).copied().unwrap_or(0.0)
            + row.get(2).copied().unwrap_or(0.0) * rgb.get(2).copied().unwrap_or(0.0);
    }
    out
}

/// Narrow a `f64` matrix to `f32`, once per image.
#[must_use]
pub fn narrow(m: Mat3) -> [[f32; 3]; 3] {
    let mut out = [[0.0f32; 3]; 3];
    for (src, dst) in m.iter().zip(out.iter_mut()) {
        for (a, b) in src.iter().zip(dst.iter_mut()) {
            *b = *a as f32;
        }
    }
    out
}

/// Rec.2020 luminance of a working-space pixel.
///
/// The coefficients are the second row of `REC2020_TO_XYZ_D65`, which is what "luminance"
/// means in this space. Spelled out as constants rather than read from the matrix because
/// they are on the inner loop of six stages.
#[must_use]
pub fn luma(rgb: [f32; 3]) -> f32 {
    0.262_700 * rgb.first().copied().unwrap_or(0.0)
        + 0.677_998 * rgb.get(1).copied().unwrap_or(0.0)
        + 0.059_302 * rgb.get(2).copied().unwrap_or(0.0)
}

/// Scale a pixel so its luminance becomes `target`, holding its colour.
///
/// Every tone stage works this way rather than operating per channel: a shadow lift applied
/// independently to R, G and B desaturates as it lifts, and a wedding rendered that way has
/// grey shadows under a red mandap.
#[must_use]
pub fn set_luma(rgb: [f32; 3], target: f32) -> [f32; 3] {
    let current = luma(rgb);
    if current <= 1e-6 {
        return [target.max(0.0); 3];
    }
    let gain = (target / current).max(0.0);
    [
        rgb.first().copied().unwrap_or(0.0) * gain,
        rgb.get(1).copied().unwrap_or(0.0) * gain,
        rgb.get(2).copied().unwrap_or(0.0) * gain,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Element-wise, because comparing float arrays with `==` is a lint and a habit worth
    /// not forming: these values are exact by construction and the tolerance costs nothing.
    fn same(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() < 1e-6)
    }

    #[test]
    fn the_reference_temperature_multiplies_by_one() {
        let m = white_balance(REFERENCE_KELVIN, 0.0);
        for channel in m {
            assert!((channel - 1.0).abs() < 1e-3, "{m:?}");
        }
    }

    #[test]
    fn a_warmer_setting_lifts_red_relative_to_blue() {
        let warm = white_balance(7500.0, 0.0);
        let cool = white_balance(3500.0, 0.0);
        assert!(
            warm[0] > cool[0],
            "raising the temperature renders warmer: {warm:?} vs {cool:?}"
        );
        assert!(warm[2] < cool[2]);
    }

    #[test]
    fn green_is_always_exactly_one_so_the_slider_is_chromatic() {
        for kelvin in [2000.0, 3200.0, 5500.0, 9000.0, 20_000.0] {
            for tint in [-150.0, 0.0, 150.0] {
                let m = white_balance(kelvin, tint);
                assert!((m[1] - 1.0).abs() < 1e-6, "{kelvin} {tint} {m:?}");
            }
        }
    }

    #[test]
    fn positive_tint_is_magenta() {
        let magenta = white_balance(5500.0, 100.0);
        let green = white_balance(5500.0, -100.0);
        // Green is pinned at 1, so "less green" shows up as more of everything else.
        assert!(magenta[0] > green[0] && magenta[2] > green[2]);
    }

    #[test]
    fn highlight_recovery_lifts_a_clipped_channel_and_never_lowers_one() {
        // Green saturates first, as it does on nearly every sensor: its clip point is 1.0
        // while red's is 1.6 and blue's 1.5. A highlight bright enough to blow green still
        // has red at 1.4, and that is what says how bright the pixel really was.
        let clip = [1.6, 1.0, 1.5];
        let out = recover_highlights([1.4, 1.0, 1.2], clip, 1.0);
        assert!(out[1] > 1.0, "the clipped channel was lifted: {out:?}");
        assert!(
            (out[0] - 1.4).abs() < 1e-6,
            "unclipped channels are untouched"
        );
        assert!((out[2] - 1.2).abs() < 1e-6);
    }

    #[test]
    fn recovery_at_zero_strength_is_the_identity() {
        let rgb = [1.4, 1.0, 1.2];
        assert!(same(recover_highlights(rgb, [1.6, 1.0, 1.5], 0.0), rgb));
    }

    #[test]
    fn a_uniform_clip_vector_can_never_produce_a_lift() {
        // With one clip point for all three channels, a clipped channel can never have a
        // brighter unclipped neighbour. The function correctly does nothing, and that is why
        // the sensor's real per-channel saturation levels matter.
        assert!(same(
            recover_highlights([1.4, 1.0, 1.2], [1.0; 3], 1.0),
            [1.4, 1.0, 1.2]
        ));
    }

    #[test]
    fn a_fully_blown_pixel_is_left_neutral() {
        let rgb = [2.0, 2.0, 2.0];
        assert!(
            same(recover_highlights(rgb, [1.0; 3], 1.0), rgb),
            "there is no colour left to recover from"
        );
    }

    #[test]
    fn set_luma_holds_the_colour_ratio() {
        let rgb = [0.4, 0.2, 0.1];
        let out = set_luma(rgb, luma(rgb) * 2.0);
        assert!((out[0] / out[1] - rgb[0] / rgb[1]).abs() < 1e-4);
        assert!((luma(out) / luma(rgb) - 2.0).abs() < 1e-4);
    }

    #[test]
    fn the_daylight_locus_moves_the_right_way() {
        let (x_warm, _) = cct_to_xy(2700.0);
        let (x_cool, _) = cct_to_xy(9000.0);
        assert!(x_warm > x_cool, "a low temperature sits further along x");
    }
}
