//! Nikon's compressed NEF mosaic.
//!
//! Nikon codes the sensor as a per-row differential predictor whose prediction
//! errors go through one of six fixed Huffman trees, and then puts the result
//! through a per-body linearisation curve carried in the MakerNote. The trees
//! and the metadata layout are the ones documented by the open-source decoders
//! (dcraw, LibRaw, RawTherapee and darktable all carry the same six tables);
//! this is a fresh, safe implementation of that published format, not a port.
//!
//! Two details cost more debugging than the rest of the format combined:
//!
//! * The Huffman symbol carries *two* numbers. Its low nibble is the length of
//!   the difference in bits and its high nibble is a left shift, so the "after
//!   split" trees code long differences in few bits at reduced precision.
//! * The two leftmost columns of each row are predicted from the row *two* rows
//!   above, not from the row above, because a Bayer row only resembles the row
//!   of the same colour parity.
//!
//! The linearisation curve is what turns the decoded 12- or 14-bit code values
//! into the linear code values everything downstream assumes, and it also
//! carries the body's black and white points. A NEF whose curve we could not
//! read decodes with an identity curve, which is exactly what the reference
//! decoders do.

use aura_core::errors::raw::corrupt;
use aura_core::AuraResult;

use super::bits::{BitReader, FlatHuffman};

/// How many entries the linearisation curve always holds: one per possible
/// 14-bit code value, so indexing it never needs a bounds decision.
const CURVE_LEN: usize = 0x4000;

/// The six trees, as `(counts, values)`. Counts are JPEG-canonical: `counts[n]`
/// codes of length `n + 1`.
const TREES: [([u8; 16], &[u8]); 6] = [
    // 12-bit lossy.
    (
        [0, 1, 5, 1, 1, 1, 1, 1, 1, 2, 0, 0, 0, 0, 0, 0],
        &[5, 4, 3, 6, 2, 7, 1, 0, 8, 9, 11, 10, 12],
    ),
    // 12-bit lossy, after the split row.
    (
        [0, 1, 5, 1, 1, 1, 1, 1, 1, 2, 0, 0, 0, 0, 0, 0],
        &[0x39, 0x5A, 0x38, 0x27, 0x16, 5, 4, 3, 2, 1, 0, 11, 12, 12],
    ),
    // 12-bit lossless.
    (
        [0, 1, 4, 2, 3, 1, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        &[5, 4, 6, 3, 7, 2, 8, 1, 9, 0, 10, 11, 12],
    ),
    // 14-bit lossy.
    (
        [0, 1, 4, 3, 1, 1, 1, 1, 1, 2, 0, 0, 0, 0, 0, 0],
        &[5, 6, 4, 7, 8, 3, 9, 2, 1, 0, 10, 11, 12, 13, 14],
    ),
    // 14-bit lossy, after the split row.
    (
        [0, 1, 5, 1, 1, 1, 1, 1, 1, 1, 2, 0, 0, 0, 0, 0],
        &[8, 0x5C, 0x4B, 0x3A, 0x29, 7, 6, 5, 4, 3, 2, 1, 0, 13, 14],
    ),
    // 14-bit lossless.
    (
        [0, 1, 4, 2, 2, 3, 1, 2, 0, 0, 0, 0, 0, 0, 0, 0],
        &[7, 6, 8, 5, 9, 4, 10, 3, 11, 12, 2, 0, 1, 13, 14],
    ),
];

/// Everything the NEF entropy decoder needs, read once from the MakerNote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NikonParams {
    /// Which of the six trees this file uses, before any split.
    pub tree: usize,
    /// Seed predictors, indexed `[row parity][column]`.
    pub vpred: [[u16; 2]; 2],
    /// Linearisation curve, always `CURVE_LEN` entries, identity where the file
    /// said nothing.
    pub curve: Vec<u16>,
    /// The row at which the second tree takes over; zero when the file is not
    /// split.
    pub split: u32,
    /// Declared sample precision, 12 or 14.
    pub bits: u8,
}

impl NikonParams {
    /// The body's black point, which is where its curve starts.
    #[must_use]
    pub fn black_level(&self) -> u32 {
        u32::from(self.curve.first().copied().unwrap_or(0))
    }

    /// The body's saturation point, which is where its curve stops rising.
    #[must_use]
    pub fn white_level(&self) -> u32 {
        let ceiling = if self.bits >= 14 { 0x3FFF } else { 0x0FFF };
        let highest = self
            .curve
            .iter()
            .take(ceiling as usize + 1)
            .copied()
            .max()
            .unwrap_or(0);
        u32::from(highest).max(1)
    }

    /// Build the identity-curve default, which is what an uncompressed or
    /// unreadable NEF falls back to.
    #[must_use]
    pub fn identity(bits: u8) -> Self {
        Self {
            tree: if bits >= 14 { 3 } else { 0 },
            vpred: [[0; 2]; 2],
            curve: identity_curve(),
            split: 0,
            bits,
        }
    }
}

fn identity_curve() -> Vec<u16> {
    (0..CURVE_LEN).map(|index| index as u16).collect()
}

fn tree(index: usize) -> AuraResult<FlatHuffman> {
    let (counts, values) = TREES
        .get(index)
        .ok_or_else(|| corrupt(format!("nikon tree {index} does not exist")))?;
    Ok(FlatHuffman::canonical(counts, values))
}

/// Read the decode parameters out of a Nikon MakerNote.
///
/// `meta` is the byte position of the `NEFDecodeTable` payload (MakerNote tag
/// `0x0096`, or `0x008C` on older bodies) and `little` is the MakerNote's own
/// byte order, which is not always the container's.
///
/// # Errors
///
/// `AURA-RAW-2002` when the table runs past the end of the file.
pub fn read_params(bytes: &[u8], meta: usize, little: bool, bits: u8) -> AuraResult<NikonParams> {
    let short = |at: usize| -> u16 {
        let a = bytes.get(at).copied().unwrap_or(0);
        let b = bytes.get(at + 1).copied().unwrap_or(0);
        if little {
            u16::from_le_bytes([a, b])
        } else {
            u16::from_be_bytes([a, b])
        }
    };

    let version_0 = bytes
        .get(meta)
        .copied()
        .ok_or_else(|| corrupt("nikon decode table starts past the end of the file"))?;
    let version_1 = bytes.get(meta + 1).copied().unwrap_or(0);

    let mut at = meta + 2;
    // Two bodies prepend a fixed block before the predictors; the version pair
    // is the only thing that distinguishes them.
    if version_0 == 0x49 || version_1 == 0x58 {
        at += 2110;
    }

    let mut tree_index = if version_0 == 0x46 { 2 } else { 0 };
    if bits >= 14 {
        tree_index += 3;
    }

    let vpred = [[short(at), short(at + 2)], [short(at + 4), short(at + 6)]];
    at += 8;
    let table_size = usize::from(short(at));
    at += 2;

    let max = if bits >= 14 {
        1usize << 14
    } else {
        1usize << 12
    };
    let step = if table_size > 1 {
        max / (table_size - 1)
    } else {
        0
    };

    let mut curve = identity_curve();
    let mut split = 0u32;

    if version_0 == 0x44 && version_1 == 0x20 && step > 0 {
        // A sparse curve: `table_size` knots, linearly interpolated between.
        for index in 0..table_size {
            let knot = index * step;
            if let Some(slot) = curve.get_mut(knot) {
                *slot = short(at + index * 2);
            }
        }
        for index in 0..max {
            let base = index - index % step;
            let fraction = index % step;
            let low = u32::from(curve.get(base).copied().unwrap_or(0));
            let high = u32::from(curve.get(base + step).copied().unwrap_or(0));
            let blended = (low * (step - fraction) as u32 + high * fraction as u32) / step as u32;
            if let Some(slot) = curve.get_mut(index) {
                *slot = u16::try_from(blended).unwrap_or(u16::MAX);
            }
        }
        split = u32::from(short(meta + 562));
    } else if version_0 != 0x46 && table_size <= 0x4001 {
        // A dense curve: one entry per code value, as far as the table goes.
        for index in 0..table_size.min(CURVE_LEN) {
            if let Some(slot) = curve.get_mut(index) {
                *slot = short(at + index * 2);
            }
        }
        // Past the end of a dense curve the sensor is saturated, so hold the
        // last value rather than falling back to the identity ramp - an
        // identity ramp there would read as detail in blown highlights.
        let last = curve
            .get(table_size.saturating_sub(1))
            .copied()
            .unwrap_or(0);
        for index in table_size..CURVE_LEN {
            if let Some(slot) = curve.get_mut(index) {
                *slot = last;
            }
        }
    }

    Ok(NikonParams {
        tree: tree_index,
        vpred,
        curve,
        split,
        bits,
    })
}

/// Decode one compressed NEF mosaic into sensor-order code values.
///
/// # Errors
///
/// `AURA-RAW-2002` when a Huffman code cannot be resolved or the entropy data
/// ends before the frame does.
pub fn decode(
    bytes: &[u8],
    params: &NikonParams,
    width: usize,
    height: usize,
) -> AuraResult<Vec<u16>> {
    let mut plane = vec![0u16; width.saturating_mul(height)];
    let mut table = tree(params.tree)?;
    let mut reader = BitReader::new(bytes);
    let mut vpred = params.vpred;
    let mut hpred = [0i32; 2];

    for row in 0..height {
        if params.split > 0 && row as u32 == params.split {
            table = tree(params.tree + 1)?;
        }
        for col in 0..width {
            let symbol = table
                .decode(&mut reader)
                .ok_or_else(|| corrupt("nikon huffman code matches no symbol"))?;
            let length = u32::from(symbol & 0x0F);
            let shift = u32::from(symbol >> 4);

            let difference = if length == 0 {
                0i32
            } else {
                let raw = reader.bits(length.saturating_sub(shift)) as i32;
                let widened = (((raw << 1) + 1) << shift) >> 1;
                if widened & (1i32 << (length - 1)) == 0 {
                    widened - (1i32 << length) + i32::from(shift == 0)
                } else {
                    widened
                }
            };

            if col < 2 {
                if let Some(slot) = vpred.get_mut(row & 1).and_then(|pair| pair.get_mut(col)) {
                    *slot = slot.wrapping_add_signed(difference as i16);
                    if let Some(target) = hpred.get_mut(col) {
                        *target = i32::from(*slot);
                    }
                }
            } else if let Some(slot) = hpred.get_mut(col & 1) {
                *slot = slot.wrapping_add(difference);
            }

            let accumulated = hpred.get(col & 1).copied().unwrap_or(0);
            let index = i32::from(accumulated as i16).clamp(0, 0x3FFF) as usize;
            let value = params.curve.get(index).copied().unwrap_or(0);
            if let Some(slot) = plane.get_mut(row * width + col) {
                *slot = value;
            }
        }
    }

    if reader.overrun() {
        return Err(corrupt(
            "nikon entropy data ended before the frame was complete",
        ));
    }
    Ok(plane)
}

/// Encode a mosaic the way a Nikon body would, used only by the fixture writer
/// so the decoder is tested by round trip rather than by one sample file.
///
/// The encoder deliberately shares nothing with the decoder except the trees:
/// it re-derives the symbol for each difference, so a mistake in the shared
/// tables shows up as a failed round trip rather than cancelling out.
#[must_use]
pub fn encode(plane: &[u16], width: usize, height: usize, tree_index: usize) -> Vec<u8> {
    let Some((counts, values)) = TREES.get(tree_index) else {
        return Vec::new();
    };
    // Invert the canonical table: symbol -> (code, length).
    let mut codes = [(0u32, 0u32); 256];
    let mut code = 0u32;
    let mut taken = 0usize;
    for length in 1..=16u32 {
        let count = usize::from(counts.get(length as usize - 1).copied().unwrap_or(0));
        for _ in 0..count {
            let symbol = usize::from(values.get(taken).copied().unwrap_or(0));
            taken += 1;
            // The published tables are zero-padded to a fixed width, so a
            // symbol can appear twice; the first, shorter code is the real one.
            if let Some(slot) = codes.get_mut(symbol) {
                if slot.1 == 0 {
                    *slot = (code, length);
                }
            }
            code += 1;
        }
        code <<= 1;
    }

    let mut writer = BitWriter::default();
    let mut vpred = [[0i32; 2]; 2];
    let mut hpred = [0i32; 2];

    for row in 0..height {
        for col in 0..width {
            let sample = i32::from(plane.get(row * width + col).copied().unwrap_or(0));
            let previous = if col < 2 {
                vpred.get(row & 1).and_then(|pair| pair.get(col)).copied()
            } else {
                hpred.get(col & 1).copied()
            }
            .unwrap_or(0);
            let difference = sample - previous;

            // The shortest symbol that can carry this difference with no shift.
            let mut length = 0u32;
            while length < 15 && !fits(difference, length) {
                length += 1;
            }
            let symbol = length as usize;
            let (bits, bit_length) = codes.get(symbol).copied().unwrap_or((0, 0));
            if bit_length == 0 {
                // The tree cannot express this length; emit zero and let the
                // round-trip test fail loudly rather than write a silent lie.
                continue;
            }
            writer.push(bits, bit_length);
            if length > 0 {
                let payload = if difference < 0 {
                    difference - 1 + (1i32 << length)
                } else {
                    difference
                };
                writer.push(payload as u32, length);
            }

            if col < 2 {
                if let Some(slot) = vpred.get_mut(row & 1).and_then(|pair| pair.get_mut(col)) {
                    *slot = sample;
                }
                if let Some(slot) = hpred.get_mut(col) {
                    *slot = sample;
                }
            } else if let Some(slot) = hpred.get_mut(col & 1) {
                *slot = sample;
            }
        }
    }
    writer.finish()
}

fn fits(difference: i32, length: u32) -> bool {
    if length == 0 {
        return difference == 0;
    }
    let span = 1i32 << (length - 1);
    (difference >= span && difference < span * 2) || (difference > -span * 2 && difference <= -span)
}

#[derive(Default)]
struct BitWriter {
    out: Vec<u8>,
    accumulator: u32,
    held: u32,
}

impl BitWriter {
    fn push(&mut self, value: u32, length: u32) {
        if length == 0 {
            return;
        }
        let mask = if length >= 32 {
            u32::MAX
        } else {
            (1u32 << length) - 1
        };
        self.accumulator = (self.accumulator << length) | (value & mask);
        self.held += length;
        while self.held >= 8 {
            self.held -= 8;
            self.out.push((self.accumulator >> self.held) as u8);
        }
        self.accumulator &= (1u32 << self.held).saturating_sub(1);
    }

    fn finish(mut self) -> Vec<u8> {
        if self.held > 0 {
            let pad = 8 - self.held;
            self.push(0, pad);
        }
        // A few trailing bytes so the reader never runs off the end while
        // finishing the last symbol.
        self.out.extend_from_slice(&[0u8; 8]);
        self.out
    }
}
