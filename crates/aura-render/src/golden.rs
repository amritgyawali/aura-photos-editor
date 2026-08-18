//! The golden suite: what a render looked like last time, and how far it may move.
//!
//! Section 10.1: *"Golden renders match the reference within tolerance for 12 camera bodies
//! across 6 scene types"*, and the standing matrix's *"dE2000 mean <= 0.5, max <= 2.0 unless
//! intentionally changed and re-blessed"*.
//!
//! # What this suite proves, and what it does not
//!
//! It proves that a render is **stable**: that the same recipe over the same pixels through
//! the same engine produces the same file, that a refactor did not move a colour, and that a
//! stage's arithmetic did not drift. That is a regression gate and it is worth having.
//!
//! It does **not** prove colour accuracy. The fixtures are authored synthetic pixels, the
//! bodies are eight synthetic bench profiles, and there is no photographed ColorChecker in
//! this repository - phase 02's exit conditions, still open. A golden test cannot tell you
//! that a real bride's dress renders correctly; it can only tell you it renders the same way
//! it did yesterday. That is condition C2 in the phase 14 exit report and it is stated on
//! every run rather than buried here.

use aura_raw::colour::de2000::{ciede2000, xyz_d65_to_lab, Lab};
use aura_raw::colour::matrix::{apply, Vec3};
use aura_raw::colour::working_space::working_to_xyz;

use crate::contract::render::RenderedData;

/// The mean dE2000 a golden render may move by.
pub const MEAN_TOLERANCE: f64 = 0.5;

/// The maximum dE2000 any one pixel may move by.
pub const MAX_TOLERANCE: f64 = 2.0;

/// What comparing a render against its golden found.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GoldenResult {
    /// Mean dE2000 across the compared pixels.
    pub mean: f64,
    /// Largest dE2000 at any pixel.
    pub max: f64,
    /// Pixels compared.
    pub pixels: usize,
    /// True when both are inside their tolerances.
    pub within_tolerance: bool,
    /// True when the two renders are byte-identical, which is the usual case and the one
    /// the determinism gate actually wants.
    pub identical: bool,
}

/// A stable digest of a rendered image.
///
/// What the golden files store, so a regression is caught by comparing 64 characters rather
/// than by shipping images into the repository. The dE2000 comparison below is what runs
/// when a digest *does* differ, so that a review can see whether the change is a
/// floating-point last-bit or a visible shift.
#[must_use]
pub fn digest(image: &RenderedData) -> String {
    let mut hasher = blake3::Hasher::new();
    match image {
        RenderedData::Eight(bytes) => {
            hasher.update(b"8");
            hasher.update(bytes);
        }
        RenderedData::Sixteen(words) => {
            hasher.update(b"16");
            for word in words {
                hasher.update(&word.to_le_bytes());
            }
        }
    }
    hasher.finalize().to_hex().to_string()
}

/// Compare two renders perceptually.
///
/// Both must be the same size and depth. The samples are treated as sRGB-encoded, decoded to
/// linear, taken into XYZ through the working space and compared in Lab - which is the same
/// chain `aura_raw::colour::de2000` uses for the phase 02 ColorChecker suite, so a dE here
/// and a dE there mean the same thing.
#[must_use]
pub fn compare(reference: &RenderedData, candidate: &RenderedData) -> GoldenResult {
    let identical = reference == candidate;
    let (a, b) = match (reference, candidate) {
        (RenderedData::Eight(x), RenderedData::Eight(y)) => (
            x.iter().map(|v| f32::from(*v) / 255.0).collect::<Vec<_>>(),
            y.iter().map(|v| f32::from(*v) / 255.0).collect::<Vec<_>>(),
        ),
        (RenderedData::Sixteen(x), RenderedData::Sixteen(y)) => (
            x.iter()
                .map(|v| f32::from(*v) / 65_535.0)
                .collect::<Vec<_>>(),
            y.iter()
                .map(|v| f32::from(*v) / 65_535.0)
                .collect::<Vec<_>>(),
        ),
        _ => {
            return GoldenResult {
                mean: f64::INFINITY,
                max: f64::INFINITY,
                pixels: 0,
                within_tolerance: false,
                identical: false,
            }
        }
    };

    if a.len() != b.len() {
        return GoldenResult {
            mean: f64::INFINITY,
            max: f64::INFINITY,
            pixels: 0,
            within_tolerance: false,
            identical: false,
        };
    }

    let pixels = a.len() / 3;
    let mut total = 0.0f64;
    let mut max = 0.0f64;
    for index in 0..pixels {
        let base = index * 3;
        let left = lab_of(&a, base);
        let right = lab_of(&b, base);
        let delta = ciede2000(left, right);
        total += delta;
        if delta > max {
            max = delta;
        }
    }

    let mean = if pixels == 0 {
        0.0
    } else {
        total / pixels as f64
    };
    GoldenResult {
        mean,
        max,
        pixels,
        within_tolerance: mean <= MEAN_TOLERANCE && max <= MAX_TOLERANCE,
        identical,
    }
}

fn lab_of(samples: &[f32], base: usize) -> Lab {
    let decode = |value: f32| -> f64 {
        let x = f64::from(value.clamp(0.0, 1.0));
        if x <= 0.040_45 {
            x / 12.92
        } else {
            ((x + 0.055) / 1.055).powf(2.4)
        }
    };
    let linear: Vec3 = [
        decode(samples.get(base).copied().unwrap_or(0.0)),
        decode(samples.get(base + 1).copied().unwrap_or(0.0)),
        decode(samples.get(base + 2).copied().unwrap_or(0.0)),
    ];
    // The samples are sRGB-encoded, so they decode into linear sRGB rather than into the
    // working space. `SRGB_TO_XYZ_D65` is the right matrix here; using the working space's
    // would report a dE for every pixel of an unchanged image.
    let xyz = apply(aura_raw::colour::matrix::SRGB_TO_XYZ_D65, linear);
    let _ = working_to_xyz;
    xyz_d65_to_lab(xyz)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(values: &[u8]) -> RenderedData {
        RenderedData::Eight(values.to_vec())
    }

    #[test]
    fn an_identical_render_has_zero_delta() {
        let a = image(&[10, 120, 200, 30, 40, 50]);
        let result = compare(&a, &a);
        assert!(result.identical);
        assert!(result.mean < 1e-9);
        assert!(result.max < 1e-9);
        assert!(result.within_tolerance);
    }

    #[test]
    fn a_one_code_difference_is_inside_the_per_pixel_bound_and_not_free() {
        let a = image(&[120, 120, 120]);
        let b = image(&[121, 120, 120]);
        let result = compare(&a, &b);
        assert!(!result.identical);
        assert!(
            result.max <= MAX_TOLERANCE,
            "one code is dE {:.3}, which must be inside the per-pixel bound",
            result.max
        );
        // And it is deliberately *not* free against the mean bound. One code on one pixel is
        // noise; one code on every pixel is a systematic shift somebody changed on purpose
        // and has to re-bless, which is exactly what the mean gate is for.
        assert!(result.mean > MEAN_TOLERANCE);
    }

    #[test]
    fn a_visible_shift_fails() {
        let a = image(&[120, 120, 120]);
        let b = image(&[160, 110, 100]);
        assert!(!compare(&a, &b).within_tolerance);
    }

    #[test]
    fn renders_of_different_sizes_never_pass() {
        let a = image(&[1, 2, 3]);
        let b = image(&[1, 2, 3, 4, 5, 6]);
        assert!(!compare(&a, &b).within_tolerance);
    }

    #[test]
    fn renders_of_different_depths_never_pass() {
        let a = image(&[1, 2, 3]);
        let b = RenderedData::Sixteen(vec![1, 2, 3]);
        assert!(!compare(&a, &b).within_tolerance);
    }

    #[test]
    fn the_digest_is_stable_and_depth_aware() {
        let eight = image(&[1, 2, 3]);
        let sixteen = RenderedData::Sixteen(vec![1, 2, 3]);
        assert_eq!(digest(&eight), digest(&image(&[1, 2, 3])));
        assert_ne!(
            digest(&eight),
            digest(&sixteen),
            "an 8-bit and a 16-bit render are different renders"
        );
    }
}
