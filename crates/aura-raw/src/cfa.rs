//! Turning stored bytes back into sensor readings.
//!
//! A colour filter array image is one number per photosite, and cameras store
//! those numbers in whichever way was cheapest to write: sixteen bits each,
//! twelve or fourteen bits packed shoulder to shoulder, or run through lossless
//! JPEG. This module produces the same thing from all of them - a plain
//! `Vec<u16>` in sensor order - so that demosaic never has to know how the file
//! was written.
//!
//! The proprietary schemes - Nikon's Huffman coding, Sony's block coding,
//! Olympus's adaptive predictor - live one module each under `codecs/`, and are
//! dispatched from here by the [`MosaicScheme`] the container walk decided on.
//! Anything still unimplemented is refused with `AURA-RAW-2007` and falls back
//! to the embedded preview, which is a documented gap rather than a silent wrong
//! answer. See `docs/camera-support.md`.

use aura_core::errors::raw::{corrupt, mosaic_unsupported, too_large};
use aura_core::AuraResult;
use rayon::prelude::*;

use crate::codecs::{canon, crx, nikon, olympus, sony};
use crate::losslessjpeg;
use crate::meta::{MosaicRef, MosaicScheme};

/// One decoded sensor plane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mosaic {
    /// Width in photosites.
    pub width: u32,
    /// Height in photosites.
    pub height: u32,
    /// `width * height` readings in row-major order.
    pub data: Vec<u16>,
}

impl Mosaic {
    /// The reading at `(x, y)`, or zero outside the array.
    #[must_use]
    pub fn at(&self, x: u32, y: u32) -> u16 {
        if x >= self.width || y >= self.height {
            return 0;
        }
        self.data
            .get(y as usize * self.width as usize + x as usize)
            .copied()
            .unwrap_or(0)
    }
}

/// Decode the sensor plane described by `mosaic` out of `bytes`.
///
/// `little` is the container's byte order, which also governs how multi-byte
/// samples are stored.
///
/// # Errors
///
/// `AURA-RAW-2007` when the compression is not implemented, `AURA-RAW-2002` when
/// a strip runs past the end of the file, `AURA-RAW-2005` when the declared
/// dimensions exceed `max_pixels`.
pub fn decode(
    bytes: &[u8],
    mosaic: &MosaicRef,
    little: bool,
    max_pixels: u64,
) -> AuraResult<Mosaic> {
    let plane = decode_sensor(bytes, mosaic, little, max_pixels)?;
    Ok(crop_to_picture(plane, mosaic))
}

/// Decode the whole sensor, masked borders included, with any linearisation
/// table applied - the plane a reference decoder calls the raw image.
///
/// [`decode`] is this followed by the crop to the picture; the two are separate
/// so the masked border can be measured and so a decode can be compared with
/// another decoder's sample for sample.
///
/// # Errors
///
/// The same as [`decode`].
pub fn decode_sensor(
    bytes: &[u8],
    mosaic: &MosaicRef,
    little: bool,
    max_pixels: u64,
) -> AuraResult<Mosaic> {
    let pixels = u64::from(mosaic.width) * u64::from(mosaic.height);
    if pixels == 0 {
        return Err(corrupt("mosaic declares a zero dimension"));
    }
    if pixels > max_pixels {
        return Err(too_large(pixels, max_pixels));
    }

    let plane = match &mosaic.scheme {
        MosaicScheme::Packed => decode_packed(bytes, mosaic, little),
        MosaicScheme::LosslessJpeg => decode_lossless(bytes, mosaic),
        MosaicScheme::SonyLossless => decode_sony_lossless(bytes, mosaic),
        MosaicScheme::NikonPacked14 => decode_whole(bytes, mosaic, |payload, width, height| {
            Ok(decode_nikon_packed14(payload, width, height))
        }),
        MosaicScheme::PackedWords => decode_packed_words(bytes, mosaic, little),
        MosaicScheme::PackedLsb => decode_whole(bytes, mosaic, |payload, width, height| {
            Ok(decode_packed_lsb(
                payload,
                mosaic.bits_per_sample,
                width,
                height,
            ))
        }),
        MosaicScheme::Fuji(layout) => decode_whole(bytes, mosaic, |payload, _, _| {
            crate::codecs::fuji::decode(payload, &layout.header, &layout.pattern, layout.bayer)
        }),
        MosaicScheme::Pana8(tags) => Ok(Mosaic {
            width: mosaic.width,
            height: mosaic.height,
            data: crate::codecs::pana8::decode(
                bytes,
                tags,
                mosaic.width as usize,
                mosaic.height as usize,
            ),
        }),
        MosaicScheme::Panasonic(params) => {
            let params = *params;
            decode_whole(bytes, mosaic, |payload, width, height| {
                Ok(crate::codecs::panasonic::decode(
                    payload, params, width, height,
                ))
            })
        }
        MosaicScheme::Nikon(params) => decode_whole(bytes, mosaic, |strip, width, height| {
            nikon::decode(strip, params, width, height)
        }),
        MosaicScheme::SonyArw2 { curve } => {
            let curve = sony::build_curve(*curve);
            decode_whole(bytes, mosaic, |strip, width, height| {
                Ok(sony::decode(strip, width, height, &curve))
            })
        }
        MosaicScheme::Olympus => decode_whole(bytes, mosaic, olympus::decode),
        MosaicScheme::Olympus14(params) => decode_whole(bytes, mosaic, |payload, width, height| {
            crate::codecs::olympus14::decode(payload, params, width, height)
        }),
        MosaicScheme::CanonLossless { slices } => {
            let slices = *slices;
            decode_whole(bytes, mosaic, |strip, width, height| {
                let frame = losslessjpeg::decode(strip)?;
                Ok(canon::unslice(
                    &frame.samples,
                    slices,
                    width as u32,
                    height as u32,
                ))
            })
        }
        MosaicScheme::Crx(header) => decode_whole(bytes, mosaic, |frame, width, height| {
            let plane = crx::decode(frame, header)?;
            if plane.len() != width * height {
                return Err(corrupt(format!(
                    "crx frame decoded {} samples, the track declares {width}x{height}",
                    plane.len()
                )));
            }
            Ok(plane)
        }),
        MosaicScheme::Unsupported(code) => Err(mosaic_unsupported(format!(
            "mosaic compression {code} is not implemented"
        ))),
    }?;
    let mut plane = plane;
    if let Some(table) = &mosaic.linearization {
        let last = table.last().copied().unwrap_or(0);
        plane.data.par_iter_mut().for_each(|code| {
            *code = table.get(usize::from(*code)).copied().unwrap_or(last);
        });
    }
    Ok(plane)
}

/// Keep the photosites that belong to the picture: the active area, then the default crop.
///
/// The CFA pattern is defined from the active area's top-left corner, so that crop is exact.
/// The default crop's origin is rounded down to an even photosite so the 2x2 pattern keeps its
/// phase; one photosite at the edge is invisible, a swapped red and blue is not.
fn crop_to_picture(plane: Mosaic, mosaic: &MosaicRef) -> Mosaic {
    let [top, left, bottom, right] =
        mosaic
            .active_area
            .unwrap_or([0, 0, plane.height, plane.width]);
    let (mut x, mut y) = (left, top);
    let (mut w, mut h) = (
        right.min(plane.width).saturating_sub(left),
        bottom.min(plane.height).saturating_sub(top),
    );
    if let Some([cx, cy, cw, ch]) = mosaic.default_crop {
        let (cx, cy) = (cx & !1, cy & !1);
        if cx < w && cy < h {
            x += cx;
            y += cy;
            w = cw.min(w - cx) & !1;
            h = ch.min(h - cy) & !1;
        }
    }
    if w == 0 || h == 0 || (x == 0 && y == 0 && w == plane.width && h == plane.height) {
        return plane;
    }
    let stride = plane.width as usize;
    let mut pixels = Vec::with_capacity(w as usize * h as usize);
    for row in y as usize..(y + h) as usize {
        let start = row * stride + x as usize;
        if let Some(line) = plane.data.get(start..start + w as usize) {
            pixels.extend_from_slice(line);
        }
    }
    if pixels.len() != w as usize * h as usize {
        return plane;
    }
    Mosaic {
        width: w,
        height: h,
        data: pixels,
    }
}

/// The shape every proprietary codec shares: one contiguous run of bytes that
/// decodes to the whole frame in one pass.
fn decode_whole<F>(bytes: &[u8], mosaic: &MosaicRef, decode: F) -> AuraResult<Mosaic>
where
    F: FnOnce(&[u8], usize, usize) -> AuraResult<Vec<u16>>,
{
    let (offset, length) = mosaic
        .segments
        .first()
        .copied()
        .ok_or_else(|| corrupt("mosaic declares no data segment"))?;
    let end = offset.saturating_add(length).min(bytes.len());
    let strip = bytes
        .get(offset..end)
        .ok_or_else(|| corrupt("mosaic data offset is outside the file"))?;
    let plane = decode(strip, mosaic.width as usize, mosaic.height as usize)?;
    Ok(Mosaic {
        width: mosaic.width,
        height: mosaic.height,
        data: plane,
    })
}

/// Samples below which unpacking stays on one thread.
const PARALLEL_MIN: usize = 1 << 20;

/// Unpacked or bit-packed samples, MSB first, each row starting on a byte
/// boundary as TIFF requires.
///
/// Rows are independent by construction - that byte alignment is exactly what
/// makes them so - which is why this is a two-step function: the strips are
/// walked once, serially, to turn them into one borrowed slice per row, and then
/// the rows are unpacked in parallel. All the bounds checking lives in the first
/// step, and the second cannot fail.
fn decode_packed(bytes: &[u8], mosaic: &MosaicRef, little: bool) -> AuraResult<Mosaic> {
    let bits = u32::from(mosaic.bits_per_sample);
    if !(8..=16).contains(&bits) {
        return Err(mosaic_unsupported(format!(
            "{bits}-bit samples are not supported"
        )));
    }
    let width = mosaic.width as usize;
    let height = mosaic.height as usize;
    let row_bytes = (width * bits as usize).div_ceil(8);

    // An empty slice means "this row was not covered by any strip"; it unpacks
    // to zeros, which is what a short strip produced before as well.
    let mut lines: Vec<&[u8]> = vec![&[]; height];
    let mut row = 0usize;
    for (offset, length) in &mosaic.segments {
        let rows_here = (mosaic.rows_per_strip as usize).min(height.saturating_sub(row));
        if rows_here == 0 {
            break;
        }
        let needed = rows_here * row_bytes;
        let available = (*length).max(needed);
        let strip = bytes
            .get(*offset..offset.saturating_add(available.min(needed)))
            .ok_or_else(|| corrupt("mosaic strip runs past the end of the file"))?;

        for local_row in 0..rows_here {
            let start = local_row * row_bytes;
            let Some(line) = strip.get(start..start + row_bytes) else {
                break;
            };
            if let Some(slot) = lines.get_mut(row + local_row) {
                *slot = line;
            }
        }
        row += rows_here;
    }

    if row < height {
        return Err(corrupt(format!(
            "mosaic declares {height} rows but the strips cover {row}"
        )));
    }

    let mut plane = vec![0u16; width * height];
    if plane.len() >= PARALLEL_MIN {
        plane
            .par_chunks_mut(width.max(1))
            .zip(lines.par_iter())
            .for_each(|(out, line)| unpack_row(line, bits, little, width, out));
    } else {
        for (out, line) in plane.chunks_mut(width.max(1)).zip(lines.iter()) {
            unpack_row(line, bits, little, width, out);
        }
    }

    Ok(Mosaic {
        width: mosaic.width,
        height: mosaic.height,
        data: plane,
    })
}

fn unpack_row(line: &[u8], bits: u32, little: bool, width: usize, out: &mut [u16]) {
    if bits == 16 {
        for (index, pair) in line.chunks_exact(2).take(width).enumerate() {
            let a = pair.first().copied().unwrap_or(0);
            let b = pair.get(1).copied().unwrap_or(0);
            let value = if little {
                u16::from_le_bytes([a, b])
            } else {
                u16::from_be_bytes([a, b])
            };
            if let Some(slot) = out.get_mut(index) {
                *slot = value;
            }
        }
        return;
    }
    if bits == 8 {
        for (index, byte) in line.iter().take(width).enumerate() {
            if let Some(slot) = out.get_mut(index) {
                *slot = u16::from(*byte);
            }
        }
        return;
    }

    // 10, 12 or 14 bits: a continuous MSB-first bit stream across the row.
    let mut accumulator: u32 = 0;
    let mut held: u32 = 0;
    let mut produced = 0usize;
    for byte in line {
        accumulator = (accumulator << 8) | u32::from(*byte);
        held += 8;
        while held >= bits && produced < width {
            let shift = held - bits;
            let value = (accumulator >> shift) & ((1u32 << bits) - 1);
            if let Some(slot) = out.get_mut(produced) {
                *slot = u16::try_from(value).unwrap_or(0);
            }
            produced += 1;
            held -= bits;
            accumulator &= (1u32 << held).saturating_sub(1);
        }
    }
}

/// Lossless JPEG tiles, in TIFF order: left to right, then top to bottom.
///
/// Each tile is its own lossless JPEG frame, `tile_width` samples wide once its components are
/// interleaved. Tiles on the right and bottom edges overhang the frame and are cropped. Tiles are
/// independent, so they decode in parallel and are then placed serially.
fn decode_lossless_tiles(
    bytes: &[u8],
    mosaic: &MosaicRef,
    (tile_width, tile_length): (u32, u32),
) -> AuraResult<Mosaic> {
    let width = mosaic.width as usize;
    let height = mosaic.height as usize;
    let (tw, th) = (tile_width as usize, tile_length as usize);
    let across = width.div_ceil(tw);
    let down = height.div_ceil(th);
    if mosaic.segments.len() < across * down {
        return Err(corrupt(format!(
            "mosaic declares {} tiles but needs {}",
            mosaic.segments.len(),
            across * down
        )));
    }
    let frames: Vec<AuraResult<losslessjpeg::LosslessImage>> = mosaic
        .segments
        .par_iter()
        .take(across * down)
        .map(|(offset, length)| {
            let end = offset.saturating_add(*length).min(bytes.len());
            let tile = bytes
                .get(*offset..end)
                .ok_or_else(|| corrupt("mosaic tile offset is outside the file"))?;
            losslessjpeg::decode(tile)
        })
        .collect();

    let mut plane = vec![0u16; width * height];
    for (index, frame) in frames.into_iter().enumerate() {
        let frame = frame?;
        let stride = frame.width as usize;
        if stride < tw.min(width) {
            return Err(corrupt(format!(
                "lossless tile is {stride} samples wide, the file declares {tw}"
            )));
        }
        let (x0, y0) = ((index % across) * tw, (index / across) * th);
        let columns = tw.min(width - x0).min(stride);
        let rows = th.min(height - y0).min(frame.height as usize);
        for local in 0..rows {
            let source = local * stride;
            let target = (y0 + local) * width + x0;
            let (Some(line), Some(slot)) = (
                frame.samples.get(source..source + columns),
                plane.get_mut(target..target + columns),
            ) else {
                return Err(corrupt("lossless tile is shorter than it declares"));
            };
            slot.copy_from_slice(line);
        }
    }
    Ok(Mosaic {
        width: mosaic.width,
        height: mosaic.height,
        data: plane,
    })
}

/// Nikon's fourteen-bit uncompressed rows: seven bytes give four samples,
/// least significant bit first, and every row is padded to sixteen bytes.
fn decode_nikon_packed14(payload: &[u8], width: usize, height: usize) -> Vec<u16> {
    let row_bytes = (width * 7 / 4).div_ceil(16) * 16;
    let mut plane = vec![0u16; width * height];
    plane
        .par_chunks_mut(width.max(1))
        .enumerate()
        .for_each(|(row, out)| {
            let line = payload
                .get(row * row_bytes..(row + 1) * row_bytes)
                .unwrap_or(&[]);
            for (group, (source, target)) in line.chunks_exact(7).zip(out.chunks_mut(4)).enumerate()
            {
                if group * 4 + 3 >= width {
                    break;
                }
                let b = |i: usize| u16::from(source.get(i).copied().unwrap_or(0));
                let values = [
                    ((b(1) & 0x3F) << 8) | b(0),
                    ((b(3) & 0xF) << 10) | (b(2) << 2) | (b(1) >> 6),
                    ((b(5) & 0x3) << 12) | (b(4) << 4) | (b(3) >> 4),
                    (b(6) << 6) | (b(5) >> 2),
                ];
                for (slot, value) in target.iter_mut().zip(values) {
                    *slot = value;
                }
            }
        });
    plane
}

/// Samples packed least significant bit first in one continuous stream.
fn decode_packed_lsb(payload: &[u8], bits: u16, width: usize, height: usize) -> Vec<u16> {
    let bits = u32::from(bits.clamp(1, 16));
    let row_bits = width * bits as usize;
    let mask = (1u32 << bits) - 1;
    let mut plane = vec![0u16; width * height];
    plane
        .par_chunks_mut(width.max(1))
        .enumerate()
        .for_each(|(row, out)| {
            let start = row * row_bits;
            for (column, slot) in out.iter_mut().enumerate() {
                let at = start + column * bits as usize;
                let byte = at >> 3;
                let mut word = 0u32;
                for k in 0..4usize {
                    word |= u32::from(payload.get(byte + k).copied().unwrap_or(0)) << (8 * k);
                }
                *slot = ((word >> (at & 7)) & mask) as u16;
            }
        });
    plane
}

/// Twelve-bit samples inside 32-bit little-endian words: swap each word into
/// byte order and the result is an ordinary most-significant-bit-first stream.
fn decode_packed_words(bytes: &[u8], mosaic: &MosaicRef, little: bool) -> AuraResult<Mosaic> {
    let (offset, length) = mosaic
        .segments
        .first()
        .copied()
        .ok_or_else(|| corrupt("mosaic declares no data segment"))?;
    let payload = bytes
        .get(offset..offset.saturating_add(length).min(bytes.len()))
        .ok_or_else(|| corrupt("mosaic data offset is outside the file"))?;
    let mut swapped = Vec::with_capacity(payload.len() + 4);
    for word in payload.chunks(4) {
        let mut quad = [0u8; 4];
        for (slot, byte) in quad.iter_mut().zip(word) {
            *slot = *byte;
        }
        quad.reverse();
        swapped.extend_from_slice(&quad);
    }
    let local = MosaicRef {
        segments: vec![(0, swapped.len())],
        rows_per_strip: mosaic.height,
        scheme: MosaicScheme::Packed,
        ..mosaic.clone()
    };
    decode_packed(&swapped, &local, little)
}

/// Sony's lossless ARW: tiles of lossless JPEG with four components, where
/// sample `k` of frame pixel `(jx, jy)` is photosite `(2jx + k % 2, 2jy + k / 2)`
/// of the tile. Tiles are independent, so they decode in parallel.
fn decode_sony_lossless(bytes: &[u8], mosaic: &MosaicRef) -> AuraResult<Mosaic> {
    let (width, height) = (mosaic.width as usize, mosaic.height as usize);
    let (tile_width, tile_length) = mosaic
        .tile
        .map_or((width, height), |(w, h)| (w as usize, h as usize));
    let across = width.div_ceil(tile_width.max(1));
    let frames: Vec<AuraResult<losslessjpeg::LosslessImage>> = mosaic
        .segments
        .par_iter()
        .map(|(offset, length)| {
            let end = offset.saturating_add(*length).min(bytes.len());
            let tile = bytes
                .get(*offset..end)
                .ok_or_else(|| corrupt("mosaic tile offset is outside the file"))?;
            losslessjpeg::decode(tile)
        })
        .collect();
    let mut plane = vec![0u16; width * height];
    for (index, frame) in frames.into_iter().enumerate() {
        let frame = frame?;
        let components = frame.components.max(1);
        let frame_width = frame.width as usize / components;
        let (x0, y0) = (
            (index % across) * tile_width,
            (index / across) * tile_length,
        );
        for jy in 0..frame.height as usize {
            let row = y0 + 2 * jy;
            if row + 1 >= height {
                break;
            }
            for jx in 0..frame_width {
                let column = x0 + 2 * jx;
                if column + 1 >= width {
                    break;
                }
                let base = (jy * frame_width + jx) * components;
                for k in 0..4usize.min(components) {
                    let target = (row + k / 2) * width + column + k % 2;
                    if let (Some(slot), Some(value)) =
                        (plane.get_mut(target), frame.samples.get(base + k))
                    {
                        *slot = *value;
                    }
                }
            }
        }
    }
    Ok(Mosaic {
        width: mosaic.width,
        height: mosaic.height,
        data: plane,
    })
}

/// Lossless JPEG strips or tiles, which is what DNG and CR2 use.
fn decode_lossless(bytes: &[u8], mosaic: &MosaicRef) -> AuraResult<Mosaic> {
    if let Some(tile) = mosaic.tile {
        return decode_lossless_tiles(bytes, mosaic, tile);
    }
    let width = mosaic.width as usize;
    let height = mosaic.height as usize;
    let mut plane = vec![0u16; width * height];
    let mut row = 0usize;

    for (offset, length) in &mosaic.segments {
        let end = offset.saturating_add(*length);
        let strip = bytes
            .get(*offset..end.min(bytes.len()))
            .ok_or_else(|| corrupt("mosaic strip offset is outside the file"))?;
        let frame = losslessjpeg::decode(strip)?;

        if frame.width as usize != width {
            return Err(corrupt(format!(
                "lossless frame is {} samples wide, mosaic declares {width}",
                frame.width
            )));
        }
        for local_row in 0..frame.height as usize {
            if row + local_row >= height {
                break;
            }
            let source = local_row * width;
            let target = (row + local_row) * width;
            let Some(line) = frame.samples.get(source..source + width) else {
                break;
            };
            if let Some(slot) = plane.get_mut(target..target + width) {
                slot.copy_from_slice(line);
            }
        }
        row += frame.height as usize;
    }

    if row < height {
        return Err(corrupt(format!(
            "mosaic declares {height} rows but the lossless frames cover {row}"
        )));
    }

    Ok(Mosaic {
        width: mosaic.width,
        height: mosaic.height,
        data: plane,
    })
}

#[cfg(test)]
// The panic family is how a test asserts; none of this is compiled into the library.
#[allow(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::indexing_slicing
)]
mod tile_tests {
    use super::*;

    /// A tiled lossless DNG - Adobe DNG Converter's layout - round-trips through the real encoder,
    /// including the right and bottom tiles that overhang the frame.
    #[test]
    fn tiled_lossless_jpeg_places_every_tile_and_crops_the_overhang() {
        let (width, height, tile) = (70_u32, 45_u32, 32_u32);
        let plane: Vec<u16> = (0..width * height)
            .map(|i| u16::try_from((i * 37) % 4096).unwrap())
            .collect();
        let mut bytes = Vec::new();
        let mut segments = Vec::new();
        for ty in 0..height.div_ceil(tile) {
            for tx in 0..width.div_ceil(tile) {
                // A tile is always full size in the file; the overhang is padding.
                let mut samples = Vec::new();
                for y in 0..tile {
                    for x in 0..tile {
                        let (gx, gy) = (tx * tile + x, ty * tile + y);
                        let v = if gx < width && gy < height {
                            plane[(gy * width + gx) as usize]
                        } else {
                            0
                        };
                        samples.push(v);
                    }
                }
                let encoded = crate::fixtures::encode_lossless_jpeg(&samples, tile, tile, 12);
                segments.push((bytes.len(), encoded.len()));
                bytes.extend(encoded);
            }
        }
        let mosaic = MosaicRef {
            width,
            height,
            bits_per_sample: 12,
            compression: 7,
            scheme: MosaicScheme::LosslessJpeg,
            segments,
            rows_per_strip: tile,
            tile: Some((tile, tile)),
            active_area: None,
            default_crop: None,
            linearization: None,
        };
        let decoded = decode(&bytes, &mosaic, true, u64::MAX).unwrap();
        assert_eq!(decoded.data, plane);
    }
}

#[cfg(test)]
// The panic family is how a test asserts; none of this is compiled into the library.
#[allow(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::indexing_slicing
)]
mod crop_tests {
    use super::*;

    #[test]
    fn the_masked_border_and_the_default_crop_are_removed_with_the_cfa_phase_kept() {
        let plane = Mosaic {
            width: 10,
            height: 8,
            data: (0..80).collect(),
        };
        let mosaic = MosaicRef {
            width: 10,
            height: 8,
            bits_per_sample: 16,
            compression: 1,
            scheme: MosaicScheme::Packed,
            segments: Vec::new(),
            rows_per_strip: 8,
            tile: None,
            active_area: Some([2, 2, 8, 10]),
            default_crop: Some([1, 1, 5, 4]),
            linearization: None,
        };
        let cropped = crop_to_picture(plane, &mosaic);
        // Origin (1, 1) rounds down to (0, 0) inside the active area; the size rounds to even.
        assert_eq!((cropped.width, cropped.height), (4, 4));
        assert_eq!(cropped.data.first().copied(), Some(22));
        assert_eq!(cropped.at(1, 1), 33);
    }
}

#[cfg(test)]
// The panic family is how a test asserts; none of this is compiled into the library.
#[allow(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::indexing_slicing
)]
mod linearization_tests {
    use super::*;

    #[test]
    fn a_linearization_table_maps_stored_codes_to_sensor_values() {
        let (width, height) = (4_u32, 2_u32);
        let stored: Vec<u16> = vec![0, 1, 2, 3, 3, 2, 1, 9];
        let bytes: Vec<u8> = stored.iter().flat_map(|v| v.to_le_bytes()).collect();
        let mosaic = MosaicRef {
            width,
            height,
            bits_per_sample: 16,
            compression: 1,
            scheme: MosaicScheme::Packed,
            segments: vec![(0, bytes.len())],
            rows_per_strip: height,
            tile: None,
            active_area: None,
            default_crop: None,
            linearization: Some(vec![0, 100, 1000, 4095]),
        };
        let decoded = decode(&bytes, &mosaic, true, u64::MAX).unwrap();
        // A code past the end of the table saturates at the table's last value.
        assert_eq!(
            decoded.data,
            vec![0, 100, 1000, 4095, 4095, 1000, 100, 4095]
        );
    }
}
