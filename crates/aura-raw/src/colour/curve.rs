//! The neutral preview curve, and the sRGB transfer function.
//!
//! Every camera bakes in a look: Canon's warm midtones, Fujifilm's film
//! simulations, Sony's flat-then-contrasty rendering. If the AI sees those
//! looks, it learns the camera instead of the wedding, and phase 26 can never
//! match two bodies. So AURA disables every camera-baked look and applies one
//! curve, the same curve, to every frame from every brand.
//!
//! The curve is deliberately dull:
//!
//! * **No toe.** Shadows stay linear, because crushing them destroys exactly the
//!   separation that face and integrity models need in a dark ceremony.
//! * **A soft shoulder above the knee.** Highlights roll off asymptotically to
//!   1.0 rather than clipping, so a white dress in sunlight keeps its texture.
//! * **Continuous slope at the knee**, so no gradient shows a seam.
//! * **No exposure guessing.** LibRaw-style auto brightness is off; the mapping
//!   from scene to display is a pure function of the pixel, nothing else.
//!
//! Above the knee `K`:
//!
//! ```text
//! f(x) = K + (1 - K) * (1 - exp(-(x - K) / (1 - K)))
//! ```
//!
//! which has slope 1 at `x = K` and slope 0 at infinity.

use std::sync::OnceLock;

/// Where the shoulder starts, in scene-linear units where 1.0 is diffuse white.
pub const KNEE: f32 = 0.6;

/// Apply the neutral preview curve to one scene-linear channel value.
///
/// Negative input (possible after a matrix rotates an out-of-gamut colour) is
/// clamped to zero rather than reflected, because a negative radiance is not a
/// colour and must not become a bright pixel.
#[must_use]
pub fn filmic_lite(x: f32) -> f32 {
    if !x.is_finite() || x <= 0.0 {
        return 0.0;
    }
    if x <= KNEE {
        return x;
    }
    let span = 1.0 - KNEE;
    KNEE + span * (1.0 - (-(x - KNEE) / span).exp())
}

/// The exact inverse of [`filmic_lite`] on `[0, 1)`.
///
/// Used by the golden suite to prove the curve is invertible, and by phase 15
/// to reason about how much headroom a preview still has.
#[must_use]
pub fn filmic_lite_inverse(y: f32) -> f32 {
    if !y.is_finite() || y <= 0.0 {
        return 0.0;
    }
    if y <= KNEE {
        return y;
    }
    let span = 1.0 - KNEE;
    let inner = 1.0 - (y - KNEE) / span;
    if inner <= f32::EPSILON {
        return f32::INFINITY;
    }
    KNEE - span * inner.ln()
}

/// The sRGB opto-electronic transfer function: linear light to sRGB code.
#[must_use]
pub fn srgb_encode(linear: f32) -> f32 {
    let x = linear.clamp(0.0, 1.0);
    if x <= 0.003_130_8 {
        x * 12.92
    } else {
        1.055 * x.powf(1.0 / 2.4) - 0.055
    }
}

/// The inverse of [`srgb_encode`]: sRGB code to linear light.
#[must_use]
pub fn srgb_decode(encoded: f32) -> f32 {
    let x = encoded.clamp(0.0, 1.0);
    if x <= 0.040_45 {
        x / 12.92
    } else {
        ((x + 0.055) / 1.055).powf(2.4)
    }
}

/// Entries in the scene-linear to sRGB-byte lookup table.
const LUT_SIZE: usize = 4096;

/// The highest scene-linear value the lookup table covers. Above this the
/// shoulder has converged to within half a code value of 1.0.
const LUT_MAX: f32 = 4.0;

fn lut() -> &'static [u8; LUT_SIZE] {
    static CACHE: OnceLock<[u8; LUT_SIZE]> = OnceLock::new();
    CACHE.get_or_init(|| {
        let mut table = [0u8; LUT_SIZE];
        for (index, slot) in table.iter_mut().enumerate() {
            let scene = LUT_MAX * index as f32 / (LUT_SIZE - 1) as f32;
            let display = srgb_encode(filmic_lite(scene));
            *slot = quantise_u8(display);
        }
        table
    })
}

/// Scene-linear working value to an sRGB byte, through the cached table.
///
/// The table is built once and is identical on every platform, so two machines
/// rendering the same RAW produce byte-identical previews - which is what makes
/// the golden suite meaningful.
#[must_use]
pub fn scene_to_srgb_byte(scene: f32) -> u8 {
    if !scene.is_finite() || scene <= 0.0 {
        return 0;
    }
    let table = lut();
    let position = scene / LUT_MAX * (LUT_SIZE - 1) as f32;
    if position >= (LUT_SIZE - 1) as f32 {
        return table.last().copied().unwrap_or(255);
    }
    // Round to nearest entry: the table is dense enough that interpolation
    // would change no code value, and rounding keeps the mapping a pure
    // integer function of the input.
    let index = (position + 0.5) as usize;
    table.get(index).copied().unwrap_or(0)
}

/// Scene-linear working value to a 16-bit linear code.
///
/// The linear buffer is **not** tone mapped: it is the scene as measured, scaled
/// so that 1.0 (diffuse white) sits at 65535 / [`LINEAR_HEADROOM`]. The headroom
/// keeps specular highlights representable instead of clipped, which is the
/// whole reason the 16-bit buffer exists.
#[must_use]
pub fn scene_to_linear_u16(scene: f32) -> u16 {
    if !scene.is_finite() || scene <= 0.0 {
        return 0;
    }
    let scaled = scene / LINEAR_HEADROOM * f32::from(u16::MAX);
    if scaled >= f32::from(u16::MAX) {
        return u16::MAX;
    }
    (scaled + 0.5) as u16
}

/// How much above diffuse white the 16-bit linear buffer can represent.
pub const LINEAR_HEADROOM: f32 = 4.0;

/// The inverse of [`scene_to_linear_u16`].
#[must_use]
pub fn linear_u16_to_scene(code: u16) -> f32 {
    f32::from(code) / f32::from(u16::MAX) * LINEAR_HEADROOM
}

/// Round a 0-1 value to a byte, half-up, saturating.
#[must_use]
pub fn quantise_u8(value: f32) -> u8 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    if value >= 1.0 {
        return 255;
    }
    (value * 255.0 + 0.5) as u8
}
