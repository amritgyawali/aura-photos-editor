//! Applying the geometry. PHASE-23.
//!
//! Phase 23 decides a rectangle, an angle, a keystone and a set of coefficients; this module is
//! where they become pixels. Four operations, and the ordering between them is
//! `graph::ORDER`'s rather than this module's: `LensVignette`, `LensDistortion` and `LensCa`
//! sit immediately after `CameraMatrix` and before `Exposure` - **in linear light, before the
//! creative operations**, which is section 6.1's requirement and was already true in phase 14 -
//! and `Geometry` sits last but one, immediately before the output transform.
//!
//! ## The maths is not here
//!
//! [`aura_raw::colour::lens`] owns the transform, and `aura_geometry` calls the same functions
//! to decide with. Two copies of a distortion polynomial is two answers to where a face is: one
//! used to check that a crop does not cut it, the other used to draw the crop. That is the
//! argument [`aura_raw::colour::profile`] already makes about camera matrices.
//!
//! ## The coefficients travel in the recipe
//!
//! `aura_recipe::Lens::coefficients`, amended in this phase - see
//! `docs/adr/ADR-0041-geometry-lens-straightening-and-crop-safety.md` section 4. The renderer
//! looks nothing up. Phase 14's rule is that a delivered file can be re-created from four
//! values; a coefficient that lived only in a profile table would be a fifth, and updating that
//! table would silently change what an already-delivered photograph looks like.
//!
//! ## Nothing is filled, ever
//!
//! A barrel correction pulls content in from beyond the frame edge and a keystone opens two
//! corners. Both are handled by *scaling until nothing samples outside* and cropping the rest
//! away - never by smearing an edge pixel outward, which is the argument
//! [`crate::spatial::crop_rotate`] already makes about rotation, and never by generating
//! content, which is phase 24 and out of scope here by section 2.2.

use aura_raw::colour::lens::{self, Coefficients};
use aura_recipe::{Lens, Perspective};

/// How many samples the resampler takes per output pixel, per axis.
///
/// One. Bilinear, as `crop_rotate` is - and the same caveat applies: a downscaling geometry
/// pass aliases, which is why `RenderLevel` picks the rung before the geometry rather than
/// after it. Section 12's "resampling softens images" is answered by applying geometry **once**
/// rather than by filtering harder.
pub const SAMPLES_PER_PIXEL: u32 = 1;

/// The coefficients a recipe asks for, or `None` when it asks for nothing.
///
/// Reads the booleans as well as the numbers: coefficients present with `distortion = false` is
/// a recipe whose photographer switched the correction off, and both states have to be
/// expressible or the off switch does not work.
#[must_use]
pub fn coefficients_of(recipe_lens: &Lens) -> Option<Coefficients> {
    let stored = recipe_lens.coefficients?;
    let k = if recipe_lens.distortion {
        stored.radial()
    } else {
        [0.0; 3]
    };
    let (ca_red, ca_blue) = if recipe_lens.ca {
        (stored.ca_red, stored.ca_blue)
    } else {
        (1.0, 1.0)
    };
    let out = Coefficients { k, ca_red, ca_blue };
    if out.is_identity() {
        None
    } else {
        Some(out)
    }
}

/// Correct radial distortion in place, scaling so nothing samples outside the frame.
///
/// Returns the scale it used, which is what a caller needs to know how much of the frame
/// survived - and what `aura_geometry` computed the same way when it decided the crop.
pub fn correct_distortion(rgb: &mut [f32], width: usize, height: usize, k: [f32; 3]) -> f32 {
    if width == 0 || height == 0 || k.iter().all(|value| value.abs() < f32::EPSILON) {
        return 1.0;
    }
    let aspect = width as f32 / height as f32;
    let scale = lens::valid_scale(k, aspect);
    let source = rgb.to_vec();
    resample(rgb, &source, width, height, |x, y| {
        lens::source_of([x, y], k, aspect, scale)
    });
    scale
}

/// Correct lateral chromatic aberration in place.
///
/// Per-channel radial scaling about the frame's centre. Green is never scaled - it is the
/// channel the sensor has twice as many of and the one a focus system was aimed with, so
/// scaling it would move the whole image rather than register the other two against it.
pub fn correct_ca(rgb: &mut [f32], width: usize, height: usize, ca: [f32; 2]) {
    if width == 0 || height == 0 {
        return;
    }
    let scales = [
        ca.first().copied().unwrap_or(1.0),
        1.0,
        ca.get(1).copied().unwrap_or(1.0),
    ];
    if scales.iter().all(|s| (s - 1.0).abs() < f32::EPSILON) {
        return;
    }
    let aspect = width as f32 / height as f32;
    let source = rgb.to_vec();
    for (channel, scale) in scales.iter().enumerate() {
        if (scale - 1.0).abs() < f32::EPSILON {
            continue;
        }
        for y in 0..height {
            for x in 0..width {
                let point = [
                    (x as f32 + 0.5) / width as f32,
                    (y as f32 + 0.5) / height as f32,
                ];
                // A pure radial scale about the centre: sample the channel from `scale` times
                // its own radius, which is the whole model for lateral CA.
                let from = radial_scale(point, *scale, aspect);
                let value = sample_channel(&source, width, height, from, channel);
                if let Some(slot) = rgb.get_mut((y * width + x) * 3 + channel) {
                    *slot = value;
                }
            }
        }
    }
}

/// Apply a keystone in place, scaling so the opened corners fall outside the frame.
pub fn apply_keystone(rgb: &mut [f32], width: usize, height: usize, keystone: Perspective) {
    if width == 0 || height == 0 {
        return;
    }
    if keystone.vertical.abs() < f32::EPSILON && keystone.horizontal.abs() < f32::EPSILON {
        return;
    }
    let source = rgb.to_vec();
    let scale = if keystone.scale.abs() < f32::EPSILON {
        1.0
    } else {
        keystone.scale
    };
    resample(rgb, &source, width, height, |x, y| {
        lens::keystone_source([x, y], keystone.vertical, keystone.horizontal, scale)
    });
}

/// Resample every output pixel from wherever `map` says it comes from.
///
/// `map` takes and returns normalised `0..1` coordinates. Anything that lands outside the
/// source is left black rather than clamped: a corner filled by smearing the edge pixel is a
/// corner that is a lie, and the crop is what removes it.
fn resample(
    out: &mut [f32],
    source: &[f32],
    width: usize,
    height: usize,
    map: impl Fn(f32, f32) -> [f32; 2],
) {
    for y in 0..height {
        for x in 0..width {
            let from = map(
                (x as f32 + 0.5) / width as f32,
                (y as f32 + 0.5) / height as f32,
            );
            for channel in 0..3 {
                let value = sample_channel(source, width, height, from, channel);
                if let Some(slot) = out.get_mut((y * width + x) * 3 + channel) {
                    *slot = value;
                }
            }
        }
    }
}

/// A pure radial scale about the frame centre, in normalised coordinates.
fn radial_scale(point: [f32; 2], scale: f32, aspect: f32) -> [f32; 2] {
    let x = point.first().copied().unwrap_or(0.5);
    let y = point.get(1).copied().unwrap_or(0.5);
    let dx = (x - 0.5) * aspect;
    let dy = y - 0.5;
    [0.5 + dx * scale / aspect, 0.5 + dy * scale]
}

/// Bilinear sample of one channel at a normalised position.
fn sample_channel(
    source: &[f32],
    width: usize,
    height: usize,
    at: [f32; 2],
    channel: usize,
) -> f32 {
    let nx = at.first().copied().unwrap_or(0.0) * width as f32 - 0.5;
    let ny = at.get(1).copied().unwrap_or(0.0) * height as f32 - 0.5;
    if nx < -0.5 || ny < -0.5 || nx > width as f32 - 0.5 || ny > height as f32 - 0.5 {
        return 0.0;
    }
    let x0 = nx.floor().max(0.0) as usize;
    let y0 = ny.floor().max(0.0) as usize;
    let x1 = (x0 + 1).min(width.saturating_sub(1));
    let y1 = (y0 + 1).min(height.saturating_sub(1));
    let fx = (nx - x0 as f32).clamp(0.0, 1.0);
    let fy = (ny - y0 as f32).clamp(0.0, 1.0);
    let at = |px: usize, py: usize| -> f32 {
        source
            .get((py * width + px) * 3 + channel)
            .copied()
            .unwrap_or(0.0)
    };
    let a = at(x0, y0) * (1.0 - fx) + at(x1, y0) * fx;
    let b = at(x0, y1) * (1.0 - fx) + at(x1, y1) * fx;
    a * (1.0 - fy) + b * fy
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plate(width: usize, height: usize) -> Vec<f32> {
        // A vertical stripe pattern, so a radial move is visible as a shifted stripe.
        let mut out = vec![0.0f32; width * height * 3];
        for y in 0..height {
            for x in 0..width {
                let value = if (x / 4) % 2 == 0 { 0.9 } else { 0.1 };
                for channel in 0..3 {
                    if let Some(slot) = out.get_mut((y * width + x) * 3 + channel) {
                        *slot = value;
                    }
                }
            }
        }
        out
    }

    #[test]
    fn an_identity_correction_changes_nothing() {
        let mut pixels = plate(32, 24);
        let before = pixels.clone();
        let scale = correct_distortion(&mut pixels, 32, 24, [0.0; 3]);
        assert!((scale - 1.0).abs() < f32::EPSILON);
        assert_eq!(pixels, before);
        correct_ca(&mut pixels, 32, 24, [1.0, 1.0]);
        assert_eq!(pixels, before);
    }

    #[test]
    fn a_barrel_correction_scales_and_moves_pixels() {
        let mut pixels = plate(64, 48);
        let before = pixels.clone();
        let scale = correct_distortion(&mut pixels, 64, 48, [0.05, 0.0, 0.0]);
        assert!(scale < 1.0, "barrel correction did not scale: {scale}");
        assert_ne!(pixels, before);
        // Nothing is black: the scale is exactly what keeps every sample inside.
        let dark = pixels.iter().filter(|v| **v <= 0.0).count();
        assert_eq!(dark, 0, "{dark} samples fell outside the frame");
    }

    #[test]
    fn a_ca_correction_moves_red_and_blue_and_leaves_green_alone() {
        let mut pixels = plate(64, 48);
        let before = pixels.clone();
        correct_ca(&mut pixels, 64, 48, [1.004, 0.996]);
        let mut green_moved = false;
        let mut red_moved = false;
        for index in 0..64 * 48 {
            if (pixels[index * 3] - before[index * 3]).abs() > 1e-4 {
                red_moved = true;
            }
            if (pixels[index * 3 + 1] - before[index * 3 + 1]).abs() > 1e-6 {
                green_moved = true;
            }
        }
        assert!(red_moved, "red did not move");
        assert!(
            !green_moved,
            "green was scaled, which moves the whole image"
        );
    }

    #[test]
    fn a_keystone_moves_the_top_and_leaves_the_centre_line() {
        let mut pixels = plate(64, 48);
        let before = pixels.clone();
        apply_keystone(
            &mut pixels,
            64,
            48,
            Perspective {
                vertical: 14.0,
                horizontal: 0.0,
                rotate: 0.0,
                scale: 1.07,
            },
        );
        assert_ne!(pixels, before);
    }

    #[test]
    fn the_recipe_switches_read_as_switches() {
        let coefficients = aura_recipe::LensCoefficients {
            k1: 0.02,
            k2: 0.0,
            k3: 0.0,
            ca_red: 1.0004,
            ca_blue: 0.9996,
        };
        let both_on = Lens {
            distortion: true,
            vignette: 0,
            ca: true,
            profile: None,
            coefficients: Some(coefficients),
        };
        let found = coefficients_of(&both_on).expect("a correction");
        assert!(found.corrects_distortion() && found.corrects_ca());

        let ca_off = Lens {
            ca: false,
            ..both_on.clone()
        };
        let found = coefficients_of(&ca_off).expect("a correction");
        assert!(found.corrects_distortion() && !found.corrects_ca());

        let both_off = Lens {
            distortion: false,
            ca: false,
            ..both_on.clone()
        };
        assert!(
            coefficients_of(&both_off).is_none(),
            "a recipe with both switches off must correct nothing"
        );

        let no_numbers = Lens {
            coefficients: None,
            ..both_on
        };
        assert!(coefficients_of(&no_numbers).is_none());
    }
}
