//! Olympus/OM compressed ORF.
//!
//! Olympus predicts each photosite from its same-colour neighbours to the left
//! and above with a gradient-aware chooser, then codes the residual in a
//! Golomb-like form whose width adapts to how noisy the row has been so far.
//! There is no Huffman table in the file: the code lengths come from a fixed
//! unary-style tree and the adaptive width, which is why an ORF has no decode
//! table to parse and why the decoder is entirely self-contained.
//!
//! The adaptation is the part that has to be exactly right. `carry[0]` holds the
//! last residual, `carry[1]` a decayed running mean and `carry[2]` a count of
//! how many small residuals have gone by; together they pick the number of
//! literal bits for the next sample. Get any one of them wrong and the image
//! decodes correctly for a few hundred photosites and then turns to noise, which
//! is why the round-trip fixture drives the encoder through the same state
//! machine rather than trusting a table.
//!
//! This is an independent safe implementation of the format as documented by the
//! open-source decoders.

use aura_core::errors::raw::corrupt;
use aura_core::AuraResult;

use super::bits::{BitReader, FlatHuffman};

/// Bytes of private header before the entropy data starts.
const HEADER_BYTES: usize = 7;

/// The fixed tree: symbol `s` is `s` zero bits followed by a one, for `s` up to
/// eleven, and twelve zero bits is the escape that means "the next sample needs
/// more room than the tree can name".
fn tree() -> FlatHuffman {
    let mut entries = vec![(0u8, 0u16); 4096];
    if let Some(slot) = entries.first_mut() {
        *slot = (12, 12);
    }
    let mut at = 1usize;
    for symbol in (0..12u16).rev() {
        let count = 2048usize >> symbol;
        for _ in 0..count {
            if let Some(slot) = entries.get_mut(at) {
                *slot = ((symbol + 1) as u8, symbol);
            }
            at += 1;
        }
    }
    FlatHuffman::from_entries(12, entries)
}

/// How many literal bits the next residual gets, from the row's carry state.
fn width_from_carry(carry: &[i32; 3]) -> u32 {
    let boost = u32::from(carry.get(2).copied().unwrap_or(0) < 3) * 2;
    let previous = carry.first().copied().unwrap_or(0) as u16;
    let mut bits = 2 + boost;
    while u32::from(previous) >> (bits + boost) != 0 {
        bits += 1;
    }
    bits
}

/// The gradient-aware predictor: the same-colour neighbours two columns left and
/// two rows up, chosen by which side has the flatter gradient.
fn predict(plane: &[u16], width: usize, row: usize, col: usize) -> i32 {
    let at =
        |r: usize, c: usize| -> i32 { i32::from(plane.get(r * width + c).copied().unwrap_or(0)) };
    if row < 2 && col < 2 {
        return 0;
    }
    if row < 2 {
        return at(row, col - 2);
    }
    if col < 2 {
        return at(row - 2, col);
    }
    let west = at(row, col - 2);
    let north = at(row - 2, col);
    let north_west = at(row - 2, col - 2);
    if (west < north_west && north_west < north) || (north < north_west && north_west < west) {
        if (west - north_west).abs() > 32 || (north - north_west).abs() > 32 {
            west + north - north_west
        } else {
            (west + north) >> 1
        }
    } else if (west - north_west).abs() > (north - north_west).abs() {
        west
    } else {
        north
    }
}

/// Decode a compressed ORF mosaic into sensor-order code values.
///
/// # Errors
///
/// `AURA-RAW-2002` when a code matches no symbol or the data ends early.
pub fn decode(bytes: &[u8], width: usize, height: usize) -> AuraResult<Vec<u16>> {
    let mut plane = vec![0u16; width.saturating_mul(height)];
    let table = tree();
    let mut reader = BitReader::new(bytes.get(HEADER_BYTES..).unwrap_or(&[]));

    for row in 0..height {
        let mut carries = [[0i32; 3]; 2];
        for col in 0..width {
            let parity = col & 1;
            let Some(carry) = carries.get_mut(parity) else {
                continue;
            };
            let bits = width_from_carry(carry);

            let marker = reader.bits(3);
            let low = (marker & 3) as i32;
            let negative = marker & 4 != 0;

            let mut high = i32::from(
                table
                    .decode(&mut reader)
                    .ok_or_else(|| corrupt("olympus code matches no symbol"))?,
            );
            if high == 12 {
                high = (reader.bits(16u32.saturating_sub(bits)) >> 1) as i32;
            }

            let residual = (high << bits) | reader.bits(bits) as i32;
            let signed = if negative { !residual } else { residual };
            let previous_mean = carry.get(1).copied().unwrap_or(0);
            let difference = signed + previous_mean;

            if let Some(slot) = carry.first_mut() {
                *slot = residual;
            }
            if let Some(slot) = carry.get_mut(1) {
                *slot = (difference * 3 + previous_mean) >> 5;
            }
            if let Some(slot) = carry.get_mut(2) {
                *slot = if residual > 16 { 0 } else { *slot + 1 };
            }

            let prediction = predict(&plane, width, row, col);
            let value = prediction + ((difference << 2) | low);
            if let Some(slot) = plane.get_mut(row * width + col) {
                *slot = u16::try_from(value.clamp(0, 0xFFFF)).unwrap_or(0);
            }
        }
    }

    if reader.overrun() {
        return Err(corrupt(
            "olympus entropy data ended before the frame was complete",
        ));
    }
    Ok(plane)
}

/// Encode a mosaic the way an Olympus body would, for the fixture writer.
///
/// The encoder walks the identical carry state machine and re-derives every
/// field from the target sample, so a disagreement between the two shows up as
/// a failed round trip instead of cancelling out.
#[must_use]
pub fn encode(plane: &[u16], width: usize, height: usize) -> Vec<u8> {
    let mut writer = BitWriter::default();
    for _ in 0..HEADER_BYTES {
        writer.byte(0);
    }

    for row in 0..height {
        let mut carries = [[0i32; 3]; 2];
        for col in 0..width {
            let parity = col & 1;
            let Some(carry) = carries.get_mut(parity) else {
                continue;
            };
            let bits = width_from_carry(carry);

            let target = i32::from(plane.get(row * width + col).copied().unwrap_or(0));
            let prediction = predict(plane, width, row, col);
            let delta = target - prediction;
            let low = delta & 3;
            let difference = delta >> 2;

            let previous_mean = carry.get(1).copied().unwrap_or(0);
            let signed = difference - previous_mean;
            let (negative, residual) = if signed >= 0 {
                (false, signed)
            } else {
                (true, !signed)
            };

            writer.push((u32::from(negative) << 2) | low as u32, 3);

            let high = residual >> bits;
            if high < 12 {
                writer.push(1, (high + 1) as u32);
            } else {
                writer.push(0, 12);
                writer.push((high as u32) << 1, 16u32.saturating_sub(bits));
            }
            let mask = (1u32 << bits) - 1;
            writer.push(residual as u32 & mask, bits);

            if let Some(slot) = carry.first_mut() {
                *slot = residual;
            }
            if let Some(slot) = carry.get_mut(1) {
                *slot = (difference * 3 + previous_mean) >> 5;
            }
            if let Some(slot) = carry.get_mut(2) {
                *slot = if residual > 16 { 0 } else { *slot + 1 };
            }
        }
    }
    writer.finish()
}

#[derive(Default)]
struct BitWriter {
    out: Vec<u8>,
    accumulator: u64,
    held: u32,
}

impl BitWriter {
    fn byte(&mut self, value: u8) {
        self.push(u32::from(value), 8);
    }

    fn push(&mut self, value: u32, length: u32) {
        if length == 0 {
            return;
        }
        let length = length.min(32);
        let mask = if length >= 32 {
            u64::from(u32::MAX)
        } else {
            (1u64 << length) - 1
        };
        self.accumulator = (self.accumulator << length) | (u64::from(value) & mask);
        self.held += length;
        while self.held >= 8 {
            self.held -= 8;
            self.out.push((self.accumulator >> self.held) as u8);
        }
        self.accumulator &= (1u64 << self.held) - 1;
    }

    fn finish(mut self) -> Vec<u8> {
        if self.held > 0 {
            let pad = 8 - self.held;
            self.push(0, pad);
        }
        // Slack so the reader can finish the last symbol without overrunning.
        self.out.extend_from_slice(&[0u8; 8]);
        self.out
    }
}
