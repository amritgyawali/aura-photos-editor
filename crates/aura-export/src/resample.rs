//! Output resize and output sharpening.
//!
//! ## The resize happens in linear light and the sharpening does not
//!
//! Two operations, two domains, and the reason is different in each.
//!
//! **Resampling is averaging**, and averaging encoded samples is wrong. sRGB's transfer curve is
//! convex, so the mean of two encoded values is darker than the encoded mean of the two linear
//! values they stand for. On a fine high-contrast texture - a lace veil, a beaded sari, backlit
//! hair - the effect is that every downscale darkens, and it compounds with contrast. This is the
//! classic gamma-incorrect downscale and it is why a badly resized wedding looks muddier than the
//! original at the same time as looking softer.
//!
//! **Sharpening is not averaging.** An unsharp mask's amount is defined against display response:
//! "how visible is this edge" is a perceptual question, and a mask applied in linear light lifts a
//! highlight edge several times harder than a shadow edge of the same perceptual strength. Which is
//! a halo around every bright dress and nothing around the groom's suit.
//!
//! So: linearise, filter, re-encode, then sharpen the encoded result.
//!
//! ## The filter is a triangle over the exact source footprint
//!
//! Not a box, and not Lanczos.
//!
//! A box filter over the source footprint is what most fast resizers do and it aliases: a
//! chequered floor or a striped suit at a 4:1 reduction produces moiré that no amount of output
//! sharpening removes. A triangle - each output pixel a weighted average over twice its own
//! footprint, weights falling linearly to zero - costs about twice as much and does not.
//!
//! Lanczos is sharper and rings. Ringing is a dark line beside every bright edge, which on a white
//! dress against a dark suit is exactly where a photographer looks. Section 6.1 says output
//! sharpening is applied *after* resize, which is where the sharpness is supposed to come from -
//! and a sharpening a person chose is recoverable, while ringing baked into a delivered JPEG is
//! not.

use aura_core::contract::delivery::{DeliveryColour, OutputSharpen};

use crate::read::{Rendered, Samples};

/// Linearise one encoded sample, `0..=1` in, `0..=1` out.
///
/// The inverse of what phase 14's output transform baked. Two curves, because Adobe RGB is a pure
/// power law and the other two use the sRGB piecewise curve, whose near-black linear segment is the
/// whole difference on a candle-lit ceremony.
#[must_use]
pub fn to_linear(space: DeliveryColour, encoded: f32) -> f32 {
    let v = encoded.clamp(0.0, 1.0);
    match space {
        DeliveryColour::AdobeRgb => v.powf(563.0 / 256.0),
        DeliveryColour::Srgb | DeliveryColour::DisplayP3 => {
            if v <= 0.040_45 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        }
    }
}

/// Encode one linear sample. The inverse of [`to_linear`].
#[must_use]
pub fn from_linear(space: DeliveryColour, linear: f32) -> f32 {
    let v = linear.clamp(0.0, 1.0);
    match space {
        DeliveryColour::AdobeRgb => v.powf(256.0 / 563.0),
        DeliveryColour::Srgb | DeliveryColour::DisplayP3 => {
            if v <= 0.003_130_8 {
                v * 12.92
            } else {
                1.055 * v.powf(1.0 / 2.4) - 0.055
            }
        }
    }
}

/// Scale a rendered frame down to a target size, in linear light.
///
/// Returns the input unchanged when the target equals the source, which is the ordinary case for a
/// full-size gallery export and is worth not paying for.
///
/// **Never upscales**: the caller has already resolved that through
/// [`aura_core::Resize::target`], and this asserts it by clamping rather than by trusting.
#[must_use]
pub fn downscale(src: &Rendered, target_w: u32, target_h: u32) -> Rendered {
    let (tw, th) = (
        target_w.min(src.width).max(1),
        target_h.min(src.height).max(1),
    );
    if tw == src.width && th == src.height {
        return src.clone();
    }

    let sw = src.width as usize;
    let sh = src.height as usize;
    let tw_u = tw as usize;
    let th_u = th as usize;

    // Horizontal then vertical, which is the separable form and turns O(w*h*kw*kh) into
    // O(w*h*(kw+kh)). On a 45 MP frame at 4:1 that is the difference between a second and a minute.
    let x_scale = sw as f32 / tw as f32;
    let y_scale = sh as f32 / th as f32;

    let mut intermediate = vec![0.0_f32; tw_u * sh * 3];
    let xw = weights(sw, tw_u, x_scale);
    for y in 0..sh {
        for (ox, (start, ws)) in xw.iter().enumerate() {
            let mut acc = [0.0_f32; 3];
            let mut total = 0.0_f32;
            for (k, w) in ws.iter().enumerate() {
                let sx = start + k;
                if sx >= sw {
                    break;
                }
                let base = (y * sw + sx) * 3;
                for c in 0..3 {
                    if let Some(u) = src.data.unit(base + c) {
                        if let Some(slot) = acc.get_mut(c) {
                            *slot += to_linear(src.colour, u) * w;
                        }
                    }
                }
                total += w;
            }
            if total > 0.0 {
                let base = (y * tw_u + ox) * 3;
                for c in 0..3 {
                    if let (Some(slot), Some(v)) = (intermediate.get_mut(base + c), acc.get(c)) {
                        *slot = v / total;
                    }
                }
            }
        }
    }

    let mut linear = vec![0.0_f32; tw_u * th_u * 3];
    let yw = weights(sh, th_u, y_scale);
    for (oy, (start, ws)) in yw.iter().enumerate() {
        for ox in 0..tw_u {
            let mut acc = [0.0_f32; 3];
            let mut total = 0.0_f32;
            for (k, w) in ws.iter().enumerate() {
                let sy = start + k;
                if sy >= sh {
                    break;
                }
                let base = (sy * tw_u + ox) * 3;
                for c in 0..3 {
                    if let Some(v) = intermediate.get(base + c) {
                        if let Some(slot) = acc.get_mut(c) {
                            *slot += v * w;
                        }
                    }
                }
                total += w;
            }
            if total > 0.0 {
                let base = (oy * tw_u + ox) * 3;
                for c in 0..3 {
                    if let (Some(slot), Some(v)) = (linear.get_mut(base + c), acc.get(c)) {
                        *slot = v / total;
                    }
                }
            }
        }
    }

    let samples = encode(&linear, src.colour, src.data.bit_depth());
    Rendered {
        width: tw,
        height: th,
        data: samples,
        colour: src.colour,
        render_hash: src.render_hash.clone(),
    }
}

/// Triangle weights for one axis: for each output sample, where its footprint starts and how much
/// each source sample contributes.
///
/// The support is twice the scale factor, which is what makes the filter anti-alias rather than
/// merely average: at 4:1 each output pixel reads eight source pixels with linearly falling
/// weights, so a stripe at the Nyquist frequency cancels instead of beating.
fn weights(src_len: usize, dst_len: usize, scale: f32) -> Vec<(usize, Vec<f32>)> {
    let support = scale.max(1.0);
    let mut out = Vec::with_capacity(dst_len);
    for i in 0..dst_len {
        let centre = (i as f32 + 0.5) * scale - 0.5;
        let lo = (centre - support).floor().max(0.0) as usize;
        let hi = ((centre + support).ceil() as isize).max(0) as usize;
        let hi = hi.min(src_len.saturating_sub(1));
        let mut ws = Vec::with_capacity(hi.saturating_sub(lo) + 1);
        for s in lo..=hi {
            let d = (s as f32 - centre).abs() / support;
            ws.push((1.0 - d).max(0.0));
        }
        out.push((lo, ws));
    }
    out
}

fn encode(linear: &[f32], space: DeliveryColour, bit_depth: u8) -> Samples {
    if bit_depth == 16 {
        Samples::Sixteen(
            linear
                .iter()
                .map(|&v| {
                    (from_linear(space, v) * 65535.0)
                        .round()
                        .clamp(0.0, 65535.0) as u16
                })
                .collect(),
        )
    } else {
        Samples::Eight(
            linear
                .iter()
                .map(|&v| (from_linear(space, v) * 255.0).round().clamp(0.0, 255.0) as u8)
                .collect(),
        )
    }
}

/// Output sharpening: an unsharp mask on the **encoded** samples.
///
/// The radius is fixed at one pixel and the amount comes from
/// [`OutputSharpen::amount`], which is a function of how far the frame was scaled. A radius that
/// grew with the output size would sharpen a print file and a web file at different *scales* of
/// detail, and the whole point of resolution-aware output sharpening is that both should look
/// sharp at the size they are viewed at.
///
/// The threshold is what stops it sharpening noise: a difference below it is left alone, so flat
/// sky and evenly-lit skin do not acquire texture the sensor did not record. Phase 22 spends a
/// whole module deciding how much noise a frame has; this is not the place to undo that.
#[must_use]
pub fn sharpen(image: &Rendered, mode: OutputSharpen, scale: f32) -> Rendered {
    let amount = mode.amount(scale);
    if amount <= 0.0 {
        return image.clone();
    }
    // Two per cent of range. Below this a difference is noise or a smooth gradient, and the
    // gradient is the case that matters: an unthresholded mask puts a visible step in a sky.
    let threshold = 0.02_f32;

    let w = image.width as usize;
    let h = image.height as usize;
    let mut out = vec![0.0_f32; w * h * 3];

    for y in 0..h {
        for x in 0..w {
            for c in 0..3 {
                let idx = (y * w + x) * 3 + c;
                let Some(centre) = image.data.unit(idx) else {
                    continue;
                };
                // A 3x3 tent blur. Nine taps rather than a separable pass, because the kernel is
                // fixed at one pixel and nine multiplies is cheaper than two passes over a
                // 45 MP buffer.
                let mut blur = 0.0_f32;
                let mut total = 0.0_f32;
                for dy in -1_i32..=1 {
                    for dx in -1_i32..=1 {
                        let sy = y as i32 + dy;
                        let sx = x as i32 + dx;
                        if sy < 0 || sx < 0 || sy >= h as i32 || sx >= w as i32 {
                            continue;
                        }
                        let weight = if dx == 0 && dy == 0 {
                            4.0
                        } else if dx == 0 || dy == 0 {
                            2.0
                        } else {
                            1.0
                        };
                        let si = (sy as usize * w + sx as usize) * 3 + c;
                        if let Some(v) = image.data.unit(si) {
                            blur += v * weight;
                            total += weight;
                        }
                    }
                }
                let blur = if total > 0.0 { blur / total } else { centre };
                let diff = centre - blur;
                let applied = if diff.abs() < threshold {
                    centre
                } else {
                    centre + diff * amount
                };
                if let Some(slot) = out.get_mut(idx) {
                    *slot = applied.clamp(0.0, 1.0);
                }
            }
        }
    }

    let samples = if image.data.bit_depth() == 16 {
        Samples::Sixteen(
            out.iter()
                .map(|&v| (v * 65535.0).round().clamp(0.0, 65535.0) as u16)
                .collect(),
        )
    } else {
        Samples::Eight(
            out.iter()
                .map(|&v| (v * 255.0).round().clamp(0.0, 255.0) as u8)
                .collect(),
        )
    };

    Rendered {
        width: image.width,
        height: image.height,
        data: samples,
        colour: image.colour,
        render_hash: image.render_hash.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plate(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 3]) -> Rendered {
        let mut data = Vec::with_capacity((w * h * 3) as usize);
        for y in 0..h {
            for x in 0..w {
                data.extend_from_slice(&f(x, y));
            }
        }
        Rendered {
            width: w,
            height: h,
            data: Samples::Eight(data),
            colour: DeliveryColour::Srgb,
            render_hash: "0".repeat(64),
        }
    }

    #[test]
    fn the_transfer_curves_round_trip() {
        for space in DeliveryColour::ALL {
            for i in 0_u8..=255 {
                let v = f32::from(i) / 255.0;
                let back = from_linear(space, to_linear(space, v));
                assert!((back - v).abs() < 1e-4, "{space:?} at {v}: {back}");
            }
        }
    }

    #[test]
    fn a_downscale_in_linear_light_does_not_darken_a_high_contrast_texture() {
        // The defect this module exists to avoid. A one-pixel black/white chequer averages to a
        // *linear* half, which encodes to about 188 in sRGB - not to 128, which is what averaging
        // the encoded values gives. A resizer that produced 128 has darkened every fine texture
        // in the wedding by nearly a stop.
        let src = plate(64, 64, |x, y| {
            if (x + y).is_multiple_of(2) {
                [255, 255, 255]
            } else {
                [0, 0, 0]
            }
        });
        let out = downscale(&src, 8, 8);
        let mid = out.data.unit(3 * 3 * 3).unwrap() * 255.0;
        assert!(
            mid > 180.0 && mid < 195.0,
            "a linear-light average of black and white encodes near 188, got {mid}"
        );
    }

    #[test]
    fn a_downscale_preserves_a_flat_field_exactly() {
        let src = plate(40, 30, |_, _| [120, 130, 140]);
        let out = downscale(&src, 10, 8);
        assert_eq!(out.width, 10);
        assert_eq!(out.height, 8);
        for i in 0..(10 * 8) {
            let r = (out.data.unit(i * 3).unwrap() * 255.0).round();
            let g = (out.data.unit(i * 3 + 1).unwrap() * 255.0).round();
            let b = (out.data.unit(i * 3 + 2).unwrap() * 255.0).round();
            assert!((r - 120.0).abs() <= 1.0, "{r}");
            assert!((g - 130.0).abs() <= 1.0, "{g}");
            assert!((b - 140.0).abs() <= 1.0, "{b}");
        }
    }

    #[test]
    fn a_downscale_never_upscales_even_when_asked() {
        let src = plate(20, 10, |_, _| [10, 10, 10]);
        let out = downscale(&src, 200, 100);
        assert_eq!((out.width, out.height), (20, 10));
    }

    #[test]
    fn sharpening_raises_an_edge_and_leaves_a_flat_field_alone() {
        let flat = plate(16, 16, |_, _| [100, 100, 100]);
        let same = sharpen(&flat, OutputSharpen::Screen, 0.5);
        for i in 0..(16 * 16 * 3) {
            assert_eq!(same.data.unit(i), flat.data.unit(i), "flat field moved");
        }

        let edge = plate(
            16,
            16,
            |x, _| if x < 8 { [40, 40, 40] } else { [200, 200, 200] },
        );
        let sharp = sharpen(&edge, OutputSharpen::Screen, 0.25);
        // The dark side of the edge goes darker and the bright side brighter: that is what an
        // unsharp mask *is*, and it is the property a threshold must not remove.
        let dark_before = edge.data.unit((8 * 16 + 7) * 3).unwrap();
        let dark_after = sharp.data.unit((8 * 16 + 7) * 3).unwrap();
        let light_before = edge.data.unit((8 * 16 + 8) * 3).unwrap();
        let light_after = sharp.data.unit((8 * 16 + 8) * 3).unwrap();
        assert!(dark_after < dark_before, "{dark_after} !< {dark_before}");
        assert!(
            light_after > light_before,
            "{light_after} !> {light_before}"
        );
    }

    #[test]
    fn no_sharpening_is_a_pass_through() {
        let src = plate(8, 8, |x, y| [(x * 30) as u8, (y * 30) as u8, 128]);
        let out = sharpen(&src, OutputSharpen::None, 0.3);
        assert_eq!(out.data, src.data);
    }

    #[test]
    fn a_stripe_at_the_nyquist_frequency_does_not_beat() {
        // The reason the filter is a triangle rather than a box. A one-pixel vertical stripe
        // reduced 4:1 should average to a flat mid grey; a box filter over the exact footprint
        // produces alternating light and dark columns, which is moiré on a striped suit.
        let src = plate(64, 8, |x, _| {
            if x.is_multiple_of(2) {
                [230, 230, 230]
            } else {
                [30, 30, 30]
            }
        });
        let out = downscale(&src, 16, 2);
        let row: Vec<f32> = (0..16).map(|x| out.data.unit(x * 3).unwrap()).collect();
        let lo = row.iter().copied().fold(f32::MAX, f32::min);
        let hi = row.iter().copied().fold(f32::MIN, f32::max);
        assert!(hi - lo < 0.04, "moiré: spread {} across {row:?}", hi - lo);
    }

    #[test]
    fn sixteen_bit_stays_sixteen_bit_through_both_operations() {
        let src = Rendered {
            width: 8,
            height: 8,
            data: Samples::Sixteen(vec![30000_u16; 8 * 8 * 3]),
            colour: DeliveryColour::AdobeRgb,
            render_hash: "0".repeat(64),
        };
        let small = downscale(&src, 4, 4);
        assert_eq!(small.data.bit_depth(), 16);
        let sharp = sharpen(&small, OutputSharpen::Print, 0.5);
        assert_eq!(sharp.data.bit_depth(), 16);
    }
}
