//! Fujifilm RAF, a container that is neither TIFF nor ISO base media.
//!
//! A RAF starts with the ASCII magic `FUJIFILMCCD-RAW`, then a fixed header of
//! offsets: the JPEG preview (a complete JFIF file including its own EXIF
//! block), a block directory describing the sensor, and the sensor data itself.
//!
//! The block directory is a flat list of `(tag, length, payload)` records. Two
//! of them decide whether the file can be developed: `0x0100` gives the sensor
//! dimensions and `0x0131` gives the 6x6 X-Trans layout, which is per-file
//! rather than per-manufacturer. See `docs/camera-support.md` for which RAF
//! variants reach tier 2 from the mosaic.

use aura_core::errors::raw::corrupt;
use aura_core::AuraResult;

/// The magic every RAF begins with.
pub const MAGIC: &[u8] = b"FUJIFILM";

/// Largest number of block-directory records we will walk.
const MAX_BLOCKS: usize = 256;

/// The parts of a RAF header this build uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RafHeader {
    /// Offset of the embedded JPEG.
    pub jpeg_offset: usize,
    /// Declared length of the embedded JPEG.
    pub jpeg_len: usize,
    /// Offset of the CFA header block.
    pub cfa_header_offset: usize,
    /// Declared length of the CFA header block.
    pub cfa_header_len: usize,
    /// Offset of the sensor data.
    pub cfa_offset: usize,
    /// Declared length of the sensor data.
    pub cfa_len: usize,
}

/// What the block directory said about the sensor.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RafSensor {
    /// Sensor width in photosites.
    pub width: u32,
    /// Sensor height in photosites.
    pub height: u32,
    /// The 6x6 colour layout, when the file carried one.
    pub xtrans: Option<[u8; 36]>,
    /// The picture inside the sensor, `[top, left, height, width]`.
    pub crop: Option<[u32; 4]>,
    /// As-shot white balance multipliers `[red / green, 1, blue / green]`.
    pub as_shot: Option<[f32; 3]>,
    /// Mean of the per-channel black levels.
    pub black: Option<u32>,
}

/// Walk the block directory at `offset`.
///
/// Unknown records are skipped by their declared length, so a body that adds a
/// block we have never seen still reads.
#[must_use]
pub fn sensor(bytes: &[u8], offset: usize, len: usize) -> RafSensor {
    let mut out = RafSensor::default();
    let end = offset.saturating_add(len).min(bytes.len());
    let count = read_u32(bytes, offset) as usize;
    let mut at = offset + 4;

    for _ in 0..count.min(MAX_BLOCKS) {
        if at + 4 > end {
            break;
        }
        let tag = read_u16(bytes, at);
        let size = read_u16(bytes, at + 2) as usize;
        let payload = at + 4;
        match tag {
            // Sensor dimensions, height first.
            0x0100 => {
                out.height = u32::from(read_u16(bytes, payload));
                out.width = u32::from(read_u16(bytes, payload + 2));
            }
            // Where the picture sits, then how big it is.
            0x0110 => {
                let (top, left) = (read_u16(bytes, payload), read_u16(bytes, payload + 2));
                let crop = out.crop.get_or_insert([0; 4]);
                crop[0] = u32::from(top);
                crop[1] = u32::from(left);
            }
            0x0111 => {
                let (height, width) = (read_u16(bytes, payload), read_u16(bytes, payload + 2));
                let crop = out.crop.get_or_insert([0; 4]);
                crop[2] = u32::from(height);
                crop[3] = u32::from(width);
            }
            // As-shot white balance, green-red-green-blue.
            0x2FF0 => {
                let level = |i: usize| f32::from(read_u16(bytes, payload + 2 * i));
                let green = f32::midpoint(level(0), level(2));
                if green > 0.0 && level(1) > 0.0 && level(3) > 0.0 {
                    out.as_shot = Some([level(1) / green, 1.0, level(3) / green]);
                }
            }
            // Per-channel black levels, four or eight of them.
            0x4000 if size >= 8 => {
                let count = size / 2;
                let sum: u32 = (0..count)
                    .map(|i| u32::from(read_u16(bytes, payload + 2 * i)))
                    .sum();
                out.black = Some(sum / count as u32);
            }
            // The colour layout, stored back to front.
            0x0131 => {
                let mut pattern = [1u8; 36];
                for (index, slot) in pattern.iter_mut().enumerate() {
                    *slot = bytes.get(payload + 35 - index).copied().unwrap_or(1) & 3;
                }
                out.xtrans = Some(pattern);
            }
            _ => {}
        }
        at = payload.saturating_add(size);
    }
    out
}

/// True when the buffer starts with the Fujifilm magic.
#[must_use]
pub fn looks_like_raf(bytes: &[u8]) -> bool {
    bytes.get(..MAGIC.len()) == Some(MAGIC)
}

/// Read the fixed header.
///
/// # Errors
///
/// Returns `AURA-RAW-2002` when the magic is absent or an offset does not fit
/// inside the file.
pub fn header(bytes: &[u8]) -> AuraResult<RafHeader> {
    if !looks_like_raf(bytes) {
        return Err(corrupt("not a fujifilm raf container"));
    }
    let jpeg_offset = read_u32(bytes, 84) as usize;
    let jpeg_len = read_u32(bytes, 88) as usize;
    let cfa_header_offset = read_u32(bytes, 92) as usize;
    let cfa_header_len = read_u32(bytes, 96) as usize;
    let cfa_offset = read_u32(bytes, 100) as usize;
    let cfa_len = read_u32(bytes, 104) as usize;

    if jpeg_offset == 0 || jpeg_offset >= bytes.len() {
        return Err(corrupt("raf preview offset is outside the file"));
    }
    let end = jpeg_offset.saturating_add(jpeg_len);
    if end > bytes.len() {
        return Err(corrupt("raf preview length runs past the end of the file"));
    }

    Ok(RafHeader {
        jpeg_offset,
        jpeg_len,
        cfa_header_offset,
        cfa_header_len,
        cfa_offset,
        cfa_len,
    })
}

fn read_u16(bytes: &[u8], at: usize) -> u16 {
    let a = bytes.get(at).copied().unwrap_or(0);
    let b = bytes.get(at + 1).copied().unwrap_or(0);
    u16::from_be_bytes([a, b])
}

fn read_u32(bytes: &[u8], at: usize) -> u32 {
    let Some(raw) = bytes.get(at..at + 4) else {
        return 0;
    };
    let mut quad = [0u8; 4];
    for (slot, byte) in quad.iter_mut().zip(raw.iter()) {
        *slot = *byte;
    }
    u32::from_be_bytes(quad)
}
