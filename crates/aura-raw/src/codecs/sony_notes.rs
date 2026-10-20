//! What a Sony ARW says about its sensor, most of it encrypted.
//!
//! IFD0's `DNGPrivateData` (`0xC634`) points at Sony's `SR2Private` directory.
//! That directory holds, in the clear, the five-segment tone curve's knots
//! (`0x7010`) and the location, length and key of `SR2SubIFD` - which is
//! encrypted with a 32-bit-word stream cipher keyed by tag `0x7221`. Inside the
//! decrypted block are the as-shot white balance (`0x7313`, or `0x7303` in
//! green-red-blue-green order on older bodies), the per-channel black level
//! (`0x7310`/`0x7300`) and the level at which each channel clips (`0x787F`).
//!
//! The camera's own crop - the picture its JPEG covers - is in the clear, as
//! `0x74C7` (left, top) and `0x74C8` (width, height).
//!
//! Without the white balance every Sony frame rendered green, and without the
//! black level its shadows sat on a 512-code pedestal. The cipher is the one
//! documented by the open-source decoders; this is an independent safe
//! implementation of it.

use crate::container::tiff::{Ifd, TiffFile};

/// What a Sony file told us. Every field is optional.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SonyNotes {
    /// As-shot white balance multipliers `[red / green, 1, blue / green]`.
    pub as_shot: Option<[f32; 3]>,
    /// Mean of the four per-channel black levels.
    pub black: Option<u32>,
    /// The lowest per-channel clipping level.
    pub white: Option<u32>,
    /// The four knots of the ARW2 tone curve.
    pub curve: Option<[u16; 4]>,
    /// The camera's crop, `[left, top, width, height]`.
    pub crop: Option<[u32; 4]>,
}

/// Tags read here.
mod tag {
    pub(super) const PRIVATE: u16 = 0xC634;
    pub(super) const TONE_CURVE: u16 = 0x7010;
    pub(super) const SUB_OFFSET: u16 = 0x7200;
    pub(super) const SUB_LENGTH: u16 = 0x7201;
    pub(super) const SUB_KEY: u16 = 0x7221;
    pub(super) const BLACK_OLD: u16 = 0x7300;
    pub(super) const WB_GRBG: u16 = 0x7303;
    pub(super) const BLACK: u16 = 0x7310;
    pub(super) const WB_RGGB: u16 = 0x7313;
    pub(super) const CROP_ORIGIN: u16 = 0x74C7;
    pub(super) const CROP_SIZE: u16 = 0x74C8;
    pub(super) const WHITE: u16 = 0x787F;
}

/// Sony's stream cipher, in place. `buffer` is whole 32-bit words.
pub fn decrypt(buffer: &mut [u8], key: u32) {
    let mut pad = [0u32; 128];
    let mut key = key;
    for slot in pad.iter_mut().take(4) {
        key = key.wrapping_mul(48_828_125).wrapping_add(1);
        *slot = key;
    }
    let get = |pad: &[u32; 128], index: usize| pad.get(index & 127).copied().unwrap_or(0);
    let third = get(&pad, 3) << 1 | (get(&pad, 0) ^ get(&pad, 2)) >> 31;
    if let Some(slot) = pad.get_mut(3) {
        *slot = third;
    }
    for index in 4..127 {
        let value = (get(&pad, index - 4) ^ get(&pad, index - 2)) << 1
            | (get(&pad, index - 3) ^ get(&pad, index - 1)) >> 31;
        if let Some(slot) = pad.get_mut(index) {
            *slot = value;
        }
    }
    // The reference stores the pad byte-swapped and XORs native words; XOR is
    // indifferent to byte order, so the same thing is the data XOR the pad's
    // big-endian bytes.
    for (position, word) in (127usize..).zip(buffer.chunks_exact_mut(4)) {
        let next = get(&pad, position + 1) ^ get(&pad, position + 65);
        if let Some(slot) = pad.get_mut(position & 127) {
            *slot = next;
        }
        for (byte, mask) in word.iter_mut().zip(next.to_be_bytes()) {
            *byte ^= mask;
        }
    }
}

/// A 32-bit value, read as four raw bytes: the key is stored as `UNDEFINED`.
fn first_u32(file: &TiffFile<'_>, ifd: &Ifd, tag: u16) -> Option<u32> {
    ifd.get(tag).and_then(|entry| file.raw_u32(entry))
}

/// Read everything Sony stores about the sensor.
#[must_use]
pub fn read_notes(file: &TiffFile<'_>) -> SonyNotes {
    let mut notes = SonyNotes::default();
    if let (Some(origin), Some(size)) = (file.find(tag::CROP_ORIGIN), file.find(tag::CROP_SIZE)) {
        let (origin, size) = (file.u64s(origin), file.u64s(size));
        if let ([left, top], [width, height]) = (origin.as_slice(), size.as_slice()) {
            if *width > 0 && *height > 0 {
                notes.crop = Some([*left, *top, *width, *height].map(|v| v as u32));
            }
        }
    }
    let private = file
        .find(tag::PRIVATE)
        .and_then(|entry| file.raw_u32(entry))
        .and_then(|offset| file.directory_at(file.base() + offset as usize));
    let curve_entry = private
        .as_ref()
        .and_then(|ifd| ifd.get(tag::TONE_CURVE).copied())
        .or_else(|| file.find(tag::TONE_CURVE).copied());
    if let Some(entry) = curve_entry {
        notes.curve = knots(&file.u64s(&entry));
    }
    let Some(private) = private else {
        return notes;
    };
    let (Some(offset), Some(length), Some(key)) = (
        first_u32(file, &private, tag::SUB_OFFSET),
        first_u32(file, &private, tag::SUB_LENGTH),
        first_u32(file, &private, tag::SUB_KEY),
    ) else {
        return notes;
    };
    let (offset, length) = (offset as usize, (length as usize) & !3);
    if length == 0 || length > 10 << 20 {
        return notes;
    }
    let Some(encrypted) = file
        .bytes()
        .get(file.base() + offset..file.base() + offset + length)
    else {
        return notes;
    };
    let mut block = encrypted.to_vec();
    decrypt(&mut block, key);
    read_sub_ifd(&block, offset, file.little(), &mut notes);
    notes
}

/// The four knots, which must rise strictly or the curve is not a curve.
fn knots(values: &[u64]) -> Option<[u16; 4]> {
    if values.len() < 4 {
        return None;
    }
    let mut out = [0u16; 4];
    for (slot, value) in out.iter_mut().zip(values) {
        *slot = u16::try_from((*value >> 2) & 0x0FFF).unwrap_or(0);
    }
    if out.windows(2).any(|pair| pair.first() >= pair.get(1)) {
        return None;
    }
    Some(out)
}

/// Walk the decrypted `SR2SubIFD`. Its value offsets are file offsets, and the
/// block starts at file offset `origin`.
fn read_sub_ifd(block: &[u8], origin: usize, little: bool, notes: &mut SonyNotes) {
    let u16_at = |at: usize| {
        block
            .get(at..at + 2)
            .and_then(|b| b.try_into().ok())
            .map_or(0, |b| {
                if little {
                    u16::from_le_bytes(b)
                } else {
                    u16::from_be_bytes(b)
                }
            })
    };
    let u32_at = |at: usize| {
        block
            .get(at..at + 4)
            .and_then(|b| b.try_into().ok())
            .map_or(0, |b| {
                if little {
                    u32::from_le_bytes(b)
                } else {
                    u32::from_be_bytes(b)
                }
            })
    };
    let count = usize::from(u16_at(0)).min(512);
    let (mut grbg, mut rggb, mut black, mut black_old, mut white) = (None, None, None, None, None);
    for index in 0..count {
        let entry = 2 + index * 12;
        let tag = u16_at(entry);
        let kind = u16_at(entry + 2);
        let values = u32_at(entry + 4) as usize;
        if !matches!(kind, 3 | 8) || values == 0 || values > 16 {
            continue;
        }
        let at = if values * 2 <= 4 {
            entry + 8
        } else {
            (u32_at(entry + 8) as usize).wrapping_sub(origin)
        };
        let shorts: Vec<u32> = (0..values).map(|i| u32::from(u16_at(at + 2 * i))).collect();
        match tag {
            tag::WB_GRBG => grbg = Some(shorts),
            tag::WB_RGGB => rggb = Some(shorts),
            tag::BLACK => black = Some(shorts),
            tag::BLACK_OLD => black_old = Some(shorts),
            tag::WHITE => white = Some(shorts),
            _ => {}
        }
    }
    let balance = |red: u32, green1: u32, green2: u32, blue: u32| {
        let green = (green1 + green2) as f32 / 2.0;
        (red > 0 && blue > 0 && green > 0.0).then(|| [red as f32 / green, 1.0, blue as f32 / green])
    };
    notes.as_shot = match (rggb.as_deref(), grbg.as_deref()) {
        (Some([r, g1, g2, b, ..]), _) | (None, Some([g1, r, b, g2, ..])) => {
            balance(*r, *g1, *g2, *b)
        }
        _ => None,
    };
    notes.black = black
        .or(black_old)
        .filter(|levels| levels.len() >= 4)
        .map(|levels| levels.iter().take(4).sum::<u32>() / 4)
        .filter(|level| *level > 0);
    notes.white = white.and_then(|levels| levels.into_iter().filter(|level| *level > 0).min());
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// The cipher is an XOR stream: applying it twice with the same key is the
    /// identity, and it changes every word in between.
    #[test]
    fn decrypting_twice_gives_the_block_back() {
        let original: Vec<u8> = (0..64u8).collect();
        let mut block = original.clone();
        decrypt(&mut block, 0x1234_5678);
        assert_ne!(block, original);
        decrypt(&mut block, 0x1234_5678);
        assert_eq!(block, original);
    }

    #[test]
    fn knots_must_rise() {
        assert_eq!(
            knots(&[4000, 8000, 12000, 16000]),
            Some([1000, 2000, 3000, 4000])
        );
        assert_eq!(knots(&[8000, 4000, 12000, 16000]), None);
    }
}
