//! The optics transform. PHASE-23 section 6.1.
//!
//! Radial distortion and lateral chromatic aberration, as pure functions over normalised
//! coordinates. **One implementation, in the lowest crate both sides can reach** - which is the
//! argument [`crate::colour::profile`] already makes about camera matrices and
//! [`crate::colour::curve`] makes about monotone interpolation: two copies of a correction is
//! two renderers that drift apart while looking identical.
//!
//! The two sides here are `aura_geometry`, which *decides* the coefficients and has to know
//! where a face box lands once they are applied, and `aura_render`, which *applies* them. Both
//! call these functions.
//!
//! ## The convention every coefficient in the product is expressed in
//!
//! Radius is normalised by the **half-diagonal**, so `r = 1` is exactly the corner of the
//! frame whatever its aspect ratio. That is what makes a coefficient measured on a 3:2 body
//! usable on the same lens mounted to a 4:3 one, and it is the convention
//! `assets/lens_profiles/` documents.
//!
//! ## The direction the maths runs in
//!
//! [`source_of`] maps an **undistorted** point to the **distorted** one it should be sampled
//! from, because that is the direction a resampler walks: for every output pixel, where in the
//! source does it come from. [`dest_of`] is its inverse, and is what a caller needs to ask
//! where a *thing it already found* has moved to.

/// Brown-Conrady radial terms and the two chromatic scales.
///
/// The shape that travels in `aura_recipe::Lens`, so that a delivered file can be re-created
/// from the four values phase 14 names - the RAW's hash, the recipe, the engine and the output
/// spec - without a fifth. A profile table update then changes what AURA *would decide*, and
/// leaves every already-delivered photograph exactly as it was.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Coefficients {
    /// `k1, k2, k3` in normalised radius. Positive `k1` is barrel.
    pub k: [f32; 3],
    /// Radial scale for red relative to green.
    pub ca_red: f32,
    /// Radial scale for blue relative to green.
    pub ca_blue: f32,
}

impl Default for Coefficients {
    fn default() -> Self {
        Self {
            k: [0.0; 3],
            ca_red: 1.0,
            ca_blue: 1.0,
        }
    }
}

impl Coefficients {
    /// True when this would move a pixel.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.k.iter().all(|value| value.abs() < f32::EPSILON)
            && (self.ca_red - 1.0).abs() < f32::EPSILON
            && (self.ca_blue - 1.0).abs() < f32::EPSILON
    }

    /// True when the distortion model would move a pixel.
    #[must_use]
    pub fn corrects_distortion(&self) -> bool {
        self.k.iter().any(|value| value.abs() >= f32::EPSILON)
    }

    /// True when the chromatic model would move a pixel.
    #[must_use]
    pub fn corrects_ca(&self) -> bool {
        (self.ca_red - 1.0).abs() >= f32::EPSILON || (self.ca_blue - 1.0).abs() >= f32::EPSILON
    }

    /// The per-channel radial scale, green first as it always is.
    #[must_use]
    pub const fn channel_scale(&self, channel: usize) -> f32 {
        match channel {
            0 => self.ca_red,
            2 => self.ca_blue,
            // Green is never scaled: it is the channel the sensor has twice as many of and the
            // one a focus system was aimed with, so scaling it would move the whole image.
            _ => 1.0,
        }
    }
}

/// The radial distortion model, in normalised radius where one is the corner.
///
/// Maps an undistorted radius to the distorted one it should be sampled from.
#[must_use]
pub fn radial(k: [f32; 3], r: f32) -> f32 {
    let r2 = r * r;
    let (k1, k2, k3) = (
        k.first().copied().unwrap_or(0.0),
        k.get(1).copied().unwrap_or(0.0),
        k.get(2).copied().unwrap_or(0.0),
    );
    r * (1.0 + k1 * r2 + k2 * r2 * r2 + k3 * r2 * r2 * r2)
}

/// Where one normalised point in the corrected frame comes from in the source.
///
/// Coordinates are `0..1` across the frame in each axis; `aspect` is width over height.
#[must_use]
pub fn source_of(point: [f32; 2], k: [f32; 3], aspect: f32, scale: f32) -> [f32; 2] {
    let (x, y) = (
        point.first().copied().unwrap_or(0.0),
        point.get(1).copied().unwrap_or(0.0),
    );
    let half_diag = (aspect * aspect + 1.0).sqrt() / 2.0;
    let dx = (x - 0.5) * aspect * scale;
    let dy = (y - 0.5) * scale;
    let r = (dx * dx + dy * dy).sqrt() / half_diag;
    if r <= f32::EPSILON {
        return [0.5, 0.5];
    }
    let ratio = radial(k, r) / r;
    [0.5 + dx * ratio / aspect, 0.5 + dy * ratio]
}

/// Where a point in the frame as shot lands in the corrected frame. The inverse of
/// [`source_of`].
///
/// Inverted by bisection rather than in closed form: the polynomial is monotone over the range
/// the coefficients are bounded to, and twenty-four steps is exact to well under a pixel on a
/// 45 MP frame. A closed form exists for `k1` alone and does not for three terms.
#[must_use]
pub fn dest_of(point: [f32; 2], k: [f32; 3], aspect: f32, scale: f32) -> [f32; 2] {
    let (x, y) = (
        point.first().copied().unwrap_or(0.0),
        point.get(1).copied().unwrap_or(0.0),
    );
    let half_diag = (aspect * aspect + 1.0).sqrt() / 2.0;
    let dx = (x - 0.5) * aspect;
    let dy = y - 0.5;
    let r_d = (dx * dx + dy * dy).sqrt() / half_diag;
    if r_d <= f32::EPSILON {
        return [0.5, 0.5];
    }
    let (mut lo, mut hi) = (0.0f32, 2.0f32);
    for _ in 0..BISECTION_STEPS {
        let mid = f32::midpoint(lo, hi);
        if radial(k, mid) < r_d {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let r_u = f32::midpoint(lo, hi);
    let ratio = r_u / r_d;
    let scale = if scale.abs() < f32::EPSILON { 1.0 } else { scale };
    [0.5 + dx * ratio / (aspect * scale), 0.5 + dy * ratio / scale]
}

/// How many bisection steps [`dest_of`] takes. Shared with the shader.
pub const BISECTION_STEPS: usize = 24;

/// How many points of the destination boundary [`valid_scale`] checks.
pub const BOUNDARY_STEPS: usize = 64;

/// The largest scale at which every corrected pixel still comes from inside the source.
///
/// Barrel correction pulls content inward from beyond the frame edge, which leaves the
/// corrected frame's corners sampling off the image. Rather than smearing an edge pixel into
/// them - a corner that is a lie, which is the argument `aura_render::spatial::crop_rotate`
/// already makes about rotation - the frame is scaled until nothing samples outside.
///
/// A binary search over the destination boundary rather than a closed form, because the valid
/// region is the pre-image of a **rectangle** under a radially symmetric map and that is not a
/// disc: the binding constraint is at an edge midpoint for barrel and at a corner for
/// pincushion, and a closed form has to know which in advance.
#[must_use]
pub fn valid_scale(k: [f32; 3], aspect: f32) -> f32 {
    if k.iter().all(|value| value.abs() < f32::EPSILON) {
        return 1.0;
    }
    let inside = |scale: f32| -> bool {
        for step in 0..=BOUNDARY_STEPS {
            let t = step as f32 / BOUNDARY_STEPS as f32;
            for point in [[t, 0.0], [t, 1.0], [0.0, t], [1.0, t]] {
                let source = source_of(point, k, aspect, scale);
                let (sx, sy) = (
                    source.first().copied().unwrap_or(0.0),
                    source.get(1).copied().unwrap_or(0.0),
                );
                if !(-1e-4..=1.0 + 1e-4).contains(&sx) || !(-1e-4..=1.0 + 1e-4).contains(&sy) {
                    return false;
                }
            }
        }
        true
    };
    if inside(1.0) {
        return 1.0;
    }
    let (mut lo, mut hi) = (0.25f32, 1.0f32);
    for _ in 0..BISECTION_STEPS {
        let mid = f32::midpoint(lo, hi);
        if inside(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

/// Where one normalised point in a keystoned frame comes from in the source.
///
/// The correction is applied **symmetrically** - the narrow end widened by the square root of
/// the stretch and the wide end narrowed by the same - so the frame's own scale does not move
/// and the inscribed rectangle is bounded by exactly that square root. `vertical` and
/// `horizontal` are in the recipe's `-100..100` units, positive `vertical` widening the top.
#[must_use]
pub fn keystone_source(point: [f32; 2], vertical: f32, horizontal: f32, scale: f32) -> [f32; 2] {
    let (x, y) = (
        point.first().copied().unwrap_or(0.0),
        point.get(1).copied().unwrap_or(0.0),
    );
    let scale = if scale.abs() < f32::EPSILON { 1.0 } else { scale };
    // Undo the output scale first: the destination was enlarged to hide the opened corners.
    let cx = (x - 0.5) / scale;
    let cy = (y - 0.5) / scale;
    // A vertical keystone scales x by a factor that varies linearly down the frame, and a
    // horizontal one scales y by a factor that varies across it.
    let v = vertical / 100.0;
    let h = horizontal / 100.0;
    let width_at = 1.0 + v * (-2.0 * cy);
    let height_at = 1.0 + h * (-2.0 * cx);
    let sx = if width_at.abs() < 1e-4 {
        cx
    } else {
        cx / width_at
    };
    let sy = if height_at.abs() < 1e-4 {
        cy
    } else {
        cy / height_at
    };
    [0.5 + sx, 0.5 + sy]
}

#[cfg(test)]
mod tests {
    use super::*;

    const ASPECT: f32 = 1.5;

    #[test]
    fn the_transform_round_trips_at_every_radius() {
        let k = [0.03, -0.008, 0.0];
        for point in [[0.1, 0.2], [0.5, 0.5], [0.9, 0.85], [0.0, 1.0], [1.0, 0.0]] {
            let there = source_of(point, k, ASPECT, 1.0);
            let back = dest_of(there, k, ASPECT, 1.0);
            assert!(
                (back[0] - point[0]).abs() < 1e-3 && (back[1] - point[1]).abs() < 1e-3,
                "{point:?} -> {there:?} -> {back:?}"
            );
        }
    }

    #[test]
    fn the_corner_is_radius_one_whatever_the_aspect() {
        for aspect in [1.5f32, 1.0, 0.6667, 2.0] {
            let half_diag = (aspect * aspect + 1.0).sqrt() / 2.0;
            let dx = 0.5 * aspect;
            let dy = 0.5;
            let r = (dx * dx + dy * dy).sqrt() / half_diag;
            assert!((r - 1.0).abs() < 1e-5, "{aspect}: corner radius {r}");
        }
    }

    #[test]
    fn barrel_needs_a_scale_and_pincushion_does_not() {
        assert!(valid_scale([0.04, 0.0, 0.0], ASPECT) < 1.0);
        assert!((valid_scale([0.0; 3], ASPECT) - 1.0).abs() < f32::EPSILON);
        assert!((valid_scale([-0.02, 0.0, 0.0], ASPECT) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn green_is_never_scaled() {
        let coefficients = Coefficients {
            k: [0.0; 3],
            ca_red: 1.0004,
            ca_blue: 0.9996,
        };
        assert!((coefficients.channel_scale(1) - 1.0).abs() < f32::EPSILON);
        assert!(coefficients.corrects_ca());
        assert!(!coefficients.corrects_distortion());
    }

    #[test]
    fn a_keystone_widens_the_top_and_narrows_the_bottom() {
        // Positive `vertical` widens the top, so a point near the top edge samples from
        // *nearer the centre* of the source than it would without the correction.
        let top = keystone_source([0.9, 0.05], 12.0, 0.0, 1.0);
        let bottom = keystone_source([0.9, 0.95], 12.0, 0.0, 1.0);
        assert!(
            top[0] < 0.9 && bottom[0] > 0.9,
            "top {top:?} bottom {bottom:?}"
        );
        // The centre line is untouched, which is what makes it symmetric.
        let middle = keystone_source([0.9, 0.5], 12.0, 0.0, 1.0);
        assert!((middle[0] - 0.9).abs() < 1e-4, "{middle:?}");
    }

    #[test]
    fn an_identity_keystone_moves_nothing() {
        for point in [[0.0, 0.0], [0.5, 0.5], [1.0, 1.0], [0.2, 0.8]] {
            let out = keystone_source(point, 0.0, 0.0, 1.0);
            assert!((out[0] - point[0]).abs() < 1e-6 && (out[1] - point[1]).abs() < 1e-6);
        }
    }
}
