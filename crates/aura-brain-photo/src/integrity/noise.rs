//! How noisy, relative to what this scene on this body at this ISO is allowed to be.
//!
//! PHASE-09 section 6.3: "noise sigma is measured in flat regions (low local gradient)
//! and normalised by ISO and camera; expressed relative to the scene profile's tolerance
//! so a dance frame is not punished for being a dance frame."
//!
//! Three separate ideas in that sentence, and each one is a different failure if it is
//! left out:
//!
//! * **Flat regions only.** A sigma measured over a whole frame measures the lace on the
//!   dress. Every noise estimator that skips this step reports "detailed photographs are
//!   noisy", which is true of the numbers and useless about photographs.
//! * **Normalised by ISO and camera.** ISO 6400 on a 2015 body and ISO 6400 on a 2023
//!   body are two different amounts of noise. Without the normalisation the product
//!   would tell a photographer their camera is broken.
//! * **Relative to the scene.** Invariant 7. A dance floor at ISO 12800 is a dance
//!   floor; a family portrait at ISO 12800 is a mistake. The same measured sigma is a
//!   different verdict, and [`NoiseResult::sigma_rel`] is the number that carries the
//!   difference: **1.0 is exactly the scene's tolerance**, so the flag is a comparison
//!   against one rather than against a table.
//!
//! ## The estimator
//!
//! Immerkær's, which convolves with
//!
//! ```text
//!     1  -2   1
//!    -2   4  -2
//!     1  -2   1
//! ```
//!
//! and takes `σ = √(π/2) · Σ|response| / (6·N)`. It is chosen over a local-variance
//! estimator for one reason that matters here: the mask is orthogonal to both constant
//! and linear intensity, so a smooth gradient - a wall lit from one side, an
//! out-of-focus background, a sky - contributes nothing. A variance estimator counts
//! that gradient as noise, and the flattest region in a wedding photograph is almost
//! always a smooth gradient rather than a genuinely constant surface.

use aura_vision::embed::descriptors::LumaPlane;

use crate::integrity::calibration::Calibration;

/// Side of the tile the flatness test works on, in pixels.
///
/// Thirty-two. Large enough that the estimator has 900 usable positions inside it, small
/// enough that a 2048 px proxy has four thousand tiles to choose the flattest from.
pub const TILE: usize = 32;

/// What fraction of tiles counts as "flat".
///
/// The quietest quarter by gradient energy. A quarter rather than a tenth because a
/// dance-floor frame may genuinely have no large flat area, and an estimator that
/// insisted on one would fall back to the whole frame - which is the failure this whole
/// module is written to avoid.
pub const FLAT_FRACTION: f32 = 0.25;

/// The fewest tiles an estimate will be made from.
///
/// Below this the sample is too small to distinguish noise from one unlucky region, and
/// [`analyse`] returns `measured: false` rather than a number. A caller that treats an
/// unmeasured frame as a clean one is making the mistake this flag exists to prevent.
pub const MIN_TILES: usize = 6;

/// Everything the noise analysis produced.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct NoiseResult {
    /// Absolute sigma in the proxy's `0..1` luminance units.
    pub sigma: f32,
    /// Sigma relative to what this scene, body and ISO tolerate. **1.0 is the
    /// tolerance.**
    pub sigma_rel: f32,
    /// How many flat tiles the estimate came from.
    pub tiles: usize,
    /// False when there were too few flat tiles to say anything.
    pub measured: bool,
}

/// Immerkær's sigma over one tile.
///
/// Returns `None` when the tile is smaller than the mask needs.
#[must_use]
pub fn tile_sigma(plane: &LumaPlane, x0: usize, y0: usize, side: usize) -> Option<f32> {
    if side < 3 || x0 + side > plane.width || y0 + side > plane.height {
        return None;
    }
    let at = |x: usize, y: usize| -> f32 {
        plane
            .values
            .get(y * plane.width + x)
            .copied()
            .unwrap_or(0.0)
    };
    let mut total = 0.0f64;
    let mut count = 0u32;
    for y in (y0 + 1)..(y0 + side - 1) {
        for x in (x0 + 1)..(x0 + side - 1) {
            let response = 4.0f32.mul_add(
                at(x, y),
                -2.0 * (at(x - 1, y) + at(x + 1, y) + at(x, y - 1) + at(x, y + 1)),
            ) + at(x - 1, y - 1)
                + at(x + 1, y - 1)
                + at(x - 1, y + 1)
                + at(x + 1, y + 1);
            total += f64::from(response.abs());
            count += 1;
        }
    }
    if count == 0 {
        return None;
    }
    // √(π/2) / 6, the constant that turns a mean absolute response into a standard
    // deviation for Gaussian noise under this mask.
    Some((total / (6.0 * f64::from(count)) * 1.253_314) as f32)
}

/// Mean gradient energy over one tile. The flatness test.
#[must_use]
pub fn tile_energy(plane: &LumaPlane, x0: usize, y0: usize, side: usize) -> f32 {
    let at = |x: usize, y: usize| -> f32 {
        plane
            .values
            .get(y * plane.width + x)
            .copied()
            .unwrap_or(0.0)
    };
    let mut total = 0.0f64;
    let mut count = 0u32;
    for y in y0..(y0 + side).min(plane.height).saturating_sub(1) {
        for x in x0..(x0 + side).min(plane.width).saturating_sub(1) {
            let here = at(x, y);
            total += f64::from((at(x + 1, y) - here).abs() + (at(x, y + 1) - here).abs());
            count += 1;
        }
    }
    if count == 0 {
        return 0.0;
    }
    (total / f64::from(count)) as f32
}

/// Measure a frame's noise.
///
/// `scene_tolerance` is `SceneProfile::max_acceptable_noise`, `0..1`, higher meaning
/// noisier is acceptable. `iso` and `calibration` turn an absolute sigma into a figure
/// that is comparable between bodies.
#[must_use]
pub fn analyse(
    plane: &LumaPlane,
    iso: u32,
    calibration: &Calibration,
    scene_tolerance: f32,
) -> NoiseResult {
    if plane.width < TILE || plane.height < TILE {
        return NoiseResult::default();
    }

    // One pass to score every tile, then the quietest quarter. Sorting a few thousand
    // f32s costs microseconds and is deterministic; picking "the flattest tile" by a
    // running minimum would make the answer depend on scan order when two tiles tie.
    let mut tiles: Vec<(f32, usize, usize)> = Vec::new();
    let mut y = 0;
    while y + TILE <= plane.height {
        let mut x = 0;
        while x + TILE <= plane.width {
            tiles.push((tile_energy(plane, x, y, TILE), x, y));
            x += TILE;
        }
        y += TILE;
    }
    if tiles.is_empty() {
        return NoiseResult::default();
    }
    tiles.sort_by(|left, right| {
        left.0
            .total_cmp(&right.0)
            .then(left.2.cmp(&right.2))
            .then(left.1.cmp(&right.1))
    });
    let wanted = ((tiles.len() as f32 * FLAT_FRACTION).ceil() as usize).max(1);
    let flat = tiles.get(..wanted.min(tiles.len())).unwrap_or(&[]);

    let mut sigmas: Vec<f32> = flat
        .iter()
        .filter_map(|(_, x, y)| tile_sigma(plane, *x, *y, TILE))
        .collect();
    if sigmas.len() < MIN_TILES {
        return NoiseResult {
            tiles: sigmas.len(),
            ..NoiseResult::default()
        };
    }

    // The median of the flat tiles' sigmas, not the mean. One tile that happened to
    // contain a hair, a dust spot or a sensor defect would drag a mean up by a third,
    // and the whole point of taking the quietest quarter is to be robust.
    sigmas.sort_by(f32::total_cmp);
    let sigma = sigmas.get(sigmas.len() / 2).copied().unwrap_or_default();

    NoiseResult {
        sigma,
        sigma_rel: relative(sigma, iso, calibration, scene_tolerance),
        tiles: sigmas.len(),
        measured: true,
    }
}

/// The sigma this body is expected to produce at base ISO, in proxy luminance units.
///
/// Read noise in electrons is not a fraction of full scale, so it has to be anchored
/// somewhere. The anchor is a body with 3.4 e⁻ of read noise producing a sigma of
/// 0.0030 at base ISO on the proxy - about three quarters of a level in 8 bits, which is
/// what a clean base-ISO frame from a full-frame sensor actually measures after phase
/// 02's downsample averages sixteen sensor pixels into each proxy pixel.
///
/// The constant is a calibration of the *pipeline* rather than of any camera, which is
/// why it lives here and not in the per-body table.
pub const BASE_SIGMA_PER_ELECTRON: f32 = 0.003 / 3.4;

/// Turn an absolute sigma into a scene-relative one.
///
/// `1.0` is exactly the tolerance. The scene multiplier spans 0.6 to 2.2 across the
/// tolerance range, so a `family_portrait` at tolerance 0.2 will accept about 0.9 of the
/// expected noise and a `dance_floor` at 0.85 will accept a little over twice it -
/// which is the ratio a photographer would recognise between those two situations.
#[must_use]
pub fn relative(sigma: f32, iso: u32, calibration: &Calibration, scene_tolerance: f32) -> f32 {
    let expected = calibration.expected_noise_e(iso) * BASE_SIGMA_PER_ELECTRON;
    if expected <= f32::EPSILON {
        return 0.0;
    }
    let scene = 1.6f32.mul_add(scene_tolerance.clamp(0.0, 1.0), 0.6);
    (sigma / (expected * scene)).max(0.0)
}
