//! Turning stored bytes back into sensor readings.
//!
//! A colour filter array image is one number per photosite, and cameras store
//! those numbers in whichever way was cheapest to write: sixteen bits each,
//! twelve or fourteen bits packed shoulder to shoulder, or run through lossless
//! JPEG. This module produces the same thing from all of them - a plain
//! `Vec<u16>` in sensor order - so that demosaic never has to know how the file
//! was written.
//!
//! Formats whose packing is proprietary (Nikon's compressed NEF, Sony's ARW
//! delta coding, Panasonic's RW2, Fujifilm's X-Trans) are refused here with
//! `AURA-RAW-2007` and fall back to the embedded preview, which is a documented
//! gap rather than a silent wrong answer. See `docs/camera-support.md`.

use aura_core::errors::raw::{corrupt, mosaic_unsupported, too_large};
use aura_core::AuraResult;

use crate::container::tiff::compression;
use crate::losslessjpeg;
use crate::meta::MosaicRef;

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
    let pixels = u64::from(mosaic.width) * u64::from(mosaic.height);
    if pixels == 0 {
        return Err(corrupt("mosaic declares a zero dimension"));
    }
    if pixels > max_pixels {
        return Err(too_large(pixels, max_pixels));
    }

    match mosaic.compression {
        compression::NONE => decode_packed(bytes, mosaic, little),
        compression::JPEG => decode_lossless(bytes, mosaic),
        other => Err(mosaic_unsupported(format!(
            "mosaic compression {other} is not implemented"
        ))),
    }
}

/// Unpacked or bit-packed samples, MSB first, each row starting on a byte
/// boundary as TIFF requires.
fn decode_packed(bytes: &[u8], mosaic: &MosaicRef, little: bool) -> AuraResult<Mosaic> {
    let bits = u32::from(mosaic.bits_per_sample);
    if !(8..=16).contains(&bits) {
        return Err(mosaic_unsupported(format!(
            "{bits}-bit samples are not supported"
        )));
    }
    let width = mosaic.width as usize;
    let height = mosaic.height as usize;
    let mut data = vec![0u16; width * height];
    let row_bytes = (width * bits as usize).div_ceil(8);

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
            let target = (row + local_row) * width;
            unpack_row(line, bits, little, width, &mut data, target);
        }
        row += rows_here;
    }

    if row < height {
        return Err(corrupt(format!(
            "mosaic declares {height} rows but the strips cover {row}"
        )));
    }

    Ok(Mosaic {
        width: mosaic.width,
        height: mosaic.height,
        data,
    })
}

fn unpack_row(line: &[u8], bits: u32, little: bool, width: usize, out: &mut [u16], target: usize) {
    if bits == 16 {
        for (index, pair) in line.chunks_exact(2).take(width).enumerate() {
            let a = pair.first().copied().unwrap_or(0);
            let b = pair.get(1).copied().unwrap_or(0);
            let value = if little {
                u16::from_le_bytes([a, b])
            } else {
                u16::from_be_bytes([a, b])
            };
            if let Some(slot) = out.get_mut(target + index) {
                *slot = value;
            }
        }
        return;
    }
    if bits == 8 {
        for (index, byte) in line.iter().take(width).enumerate() {
            if let Some(slot) = out.get_mut(target + index) {
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
            if let Some(slot) = out.get_mut(target + produced) {
                *slot = u16::try_from(value).unwrap_or(0);
            }
            produced += 1;
            held -= bits;
            accumulator &= (1u32 << held).saturating_sub(1);
        }
    }
}

/// Lossless JPEG strips or tiles, which is what DNG and CR2 use.
fn decode_lossless(bytes: &[u8], mosaic: &MosaicRef) -> AuraResult<Mosaic> {
    let width = mosaic.width as usize;
    let height = mosaic.height as usize;
    let mut data = vec![0u16; width * height];
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
            if let Some(slot) = data.get_mut(target..target + width) {
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
        data,
    })
}
