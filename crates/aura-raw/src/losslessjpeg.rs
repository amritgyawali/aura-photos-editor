//! Lossless JPEG (ITU T.81 annex H), the compression cameras use for sensor data.
//!
//! This is not the JPEG everyone knows. There is no DCT, no quantisation and no
//! colour transform: each sample is predicted from its already-decoded
//! neighbours and only the prediction error is Huffman coded. Canon, Adobe and
//! several others store Bayer mosaics this way, so a RAW decoder that cannot
//! read SOF3 cannot develop a CR2 or a compressed DNG at all.
//!
//! Layout note that trips everyone up once: a Bayer mosaic is normally stored as
//! **two components of half width**, interleaved one sample at a time. Decoding
//! therefore produces a plane `frame_width * components` wide, and that plane is
//! the mosaic in sensor order.
//!
//! Every read is bounds-checked and the decoder is a pure function of its input,
//! so a hostile file can produce a wrong image or an error, never a panic and
//! never an out-of-bounds read.

use aura_core::errors::raw::{corrupt, too_large};
use aura_core::AuraResult;

/// Ceiling on samples in one lossless frame, checked before allocation.
const MAX_SAMPLES: usize = 512 * 1024 * 1024 / 2;

/// One decoded lossless frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LosslessImage {
    /// Width of the interleaved plane: frame width multiplied by components.
    pub width: u32,
    /// Height in rows.
    pub height: u32,
    /// How many components the frame declared.
    pub components: usize,
    /// Sample precision in bits, 8 to 16.
    pub precision: u8,
    /// `width * height` samples in row-major order.
    pub samples: Vec<u16>,
}

#[derive(Debug, Clone, Default)]
struct HuffTable {
    /// Smallest code of each length, indexed by length - 1.
    min_code: [i32; 16],
    /// Largest code of each length, or -1 when the length is unused.
    max_code: [i32; 16],
    /// Index into `values` of the first symbol of each length.
    val_ptr: [usize; 16],
    /// The symbols, in canonical order.
    values: Vec<u8>,
}

impl HuffTable {
    fn build(counts: &[u8; 16], values: Vec<u8>) -> Self {
        let mut table = Self {
            values,
            ..Self::default()
        };
        let mut code: i32 = 0;
        let mut index: usize = 0;
        for length in 0..16 {
            let count = i32::from(counts.get(length).copied().unwrap_or(0));
            if let Some(slot) = table.val_ptr.get_mut(length) {
                *slot = index;
            }
            if let Some(slot) = table.min_code.get_mut(length) {
                *slot = code;
            }
            if count == 0 {
                if let Some(slot) = table.max_code.get_mut(length) {
                    *slot = -1;
                }
            } else {
                if let Some(slot) = table.max_code.get_mut(length) {
                    *slot = code + count - 1;
                }
                index += count as usize;
                code += count;
            }
            code <<= 1;
        }
        table
    }
}

struct BitReader<'a> {
    bytes: &'a [u8],
    at: usize,
    bits: u32,
    count: u32,
}

impl<'a> BitReader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            at: 0,
            bits: 0,
            count: 0,
        }
    }

    /// Read one bit, feeding the buffer a byte at a time and honouring the
    /// `FF00` stuffing rule. Past the end of the data the reader returns zero
    /// bits, which lets a truncated stream finish as a wrong image rather than
    /// as an error inside the innermost loop.
    fn bit(&mut self) -> u32 {
        if self.count == 0 {
            let mut byte = self.bytes.get(self.at).copied().unwrap_or(0);
            self.at += 1;
            if byte == 0xFF {
                let next = self.bytes.get(self.at).copied().unwrap_or(0);
                if next == 0x00 {
                    self.at += 1;
                } else if (0xD0..=0xD7).contains(&next) {
                    // Restart marker: skip it and carry on with the next byte.
                    self.at += 1;
                    byte = self.bytes.get(self.at).copied().unwrap_or(0);
                    self.at += 1;
                }
            }
            self.bits = u32::from(byte);
            self.count = 8;
        }
        self.count -= 1;
        (self.bits >> self.count) & 1
    }

    fn bits(&mut self, n: u32) -> u32 {
        let mut out = 0u32;
        for _ in 0..n.min(32) {
            out = (out << 1) | self.bit();
        }
        out
    }

    fn decode(&mut self, table: &HuffTable) -> AuraResult<u8> {
        let mut code = self.bit() as i32;
        for length in 0..16usize {
            let max = table.max_code.get(length).copied().unwrap_or(-1);
            if max >= 0 && code <= max {
                let min = table.min_code.get(length).copied().unwrap_or(0);
                let base = table.val_ptr.get(length).copied().unwrap_or(0);
                let index = base + (code - min).max(0) as usize;
                return table
                    .values
                    .get(index)
                    .copied()
                    .ok_or_else(|| corrupt("lossless jpeg huffman symbol is out of range"));
            }
            code = (code << 1) | self.bit() as i32;
        }
        Err(corrupt("lossless jpeg huffman code is longer than 16 bits"))
    }
}

/// Sign-extend a Huffman magnitude, T.81 figure F.12.
fn extend(value: u32, magnitude: u32) -> i32 {
    if magnitude == 0 {
        return 0;
    }
    let threshold = 1i32 << (magnitude - 1);
    let raw = value as i32;
    if raw < threshold {
        raw - (1i32 << magnitude) + 1
    } else {
        raw
    }
}

#[derive(Debug, Clone, Copy)]
struct Component {
    /// Which Huffman table this component's differences use.
    table: usize,
}

/// Decode a complete lossless JPEG stream.
///
/// # Errors
///
/// Returns `AURA-RAW-2002` when the stream is not a lossless frame, when a
/// required header is missing, or when a Huffman code cannot be resolved, and
/// `AURA-RAW-2005` when the declared frame exceeds the sample ceiling.
pub fn decode(bytes: &[u8]) -> AuraResult<LosslessImage> {
    if !matches!(bytes.get(..2), Some([0xFF, 0xD8])) {
        return Err(corrupt("lossless jpeg does not start with FFD8"));
    }

    let mut tables: Vec<HuffTable> = vec![HuffTable::default(); 4];
    let mut precision = 0u8;
    let mut frame_width = 0u32;
    let mut frame_height = 0u32;
    let mut components: Vec<Component> = Vec::new();
    let mut predictor = 1u8;
    let mut point_transform = 0u8;
    let mut scan_start: Option<usize> = None;

    let mut at = 2usize;
    while at + 3 < bytes.len() {
        if bytes.get(at).copied() != Some(0xFF) {
            at += 1;
            continue;
        }
        let marker = bytes.get(at + 1).copied().unwrap_or(0);
        at += 2;
        if matches!(marker, 0xFF | 0x01 | 0xD0..=0xD7 | 0xD8) {
            continue;
        }
        if marker == 0xD9 {
            break;
        }
        let length = usize::from(u16::from_be_bytes([
            bytes.get(at).copied().unwrap_or(0),
            bytes.get(at + 1).copied().unwrap_or(0),
        ]));
        if length < 2 || at + length > bytes.len() {
            return Err(corrupt("lossless jpeg segment length is out of range"));
        }
        let payload = bytes.get(at + 2..at + length).unwrap_or(&[]);

        match marker {
            // DHT - one or more Huffman tables.
            0xC4 => {
                let mut cursor = 0usize;
                while cursor + 17 <= payload.len() {
                    let class_and_id = payload.get(cursor).copied().unwrap_or(0);
                    let id = usize::from(class_and_id & 0x0F);
                    let mut counts = [0u8; 16];
                    let mut total = 0usize;
                    for (index, slot) in counts.iter_mut().enumerate() {
                        *slot = payload.get(cursor + 1 + index).copied().unwrap_or(0);
                        total += usize::from(*slot);
                    }
                    let values_start = cursor + 17;
                    let values = payload
                        .get(values_start..values_start + total)
                        .ok_or_else(|| corrupt("huffman table runs past its segment"))?
                        .to_vec();
                    if let Some(slot) = tables.get_mut(id.min(3)) {
                        *slot = HuffTable::build(&counts, values);
                    }
                    cursor = values_start + total;
                }
            }
            // SOF3 - the lossless frame header.
            0xC3 => {
                precision = payload.first().copied().unwrap_or(0);
                frame_height = u32::from(u16::from_be_bytes([
                    payload.get(1).copied().unwrap_or(0),
                    payload.get(2).copied().unwrap_or(0),
                ]));
                frame_width = u32::from(u16::from_be_bytes([
                    payload.get(3).copied().unwrap_or(0),
                    payload.get(4).copied().unwrap_or(0),
                ]));
                let count = usize::from(payload.get(5).copied().unwrap_or(0));
                components = (0..count).map(|_| Component { table: 0 }).collect();
            }
            // Any other start-of-frame is a profile we do not decode here.
            0xC0..=0xCF if !matches!(marker, 0xC4 | 0xC8 | 0xCC) => {
                return Err(corrupt(format!(
                    "expected a lossless frame, found start-of-frame {marker:#04X}"
                )));
            }
            // SOS - the scan header; entropy data follows immediately.
            0xDA => {
                let count = usize::from(payload.first().copied().unwrap_or(0));
                for index in 0..count {
                    let table = usize::from(payload.get(2 + index * 2).copied().unwrap_or(0) >> 4);
                    if let Some(slot) = components.get_mut(index) {
                        slot.table = table.min(3);
                    }
                }
                predictor = payload.get(1 + count * 2).copied().unwrap_or(1);
                point_transform = payload.get(3 + count * 2).copied().unwrap_or(0) & 0x0F;
                scan_start = Some(at + length);
                break;
            }
            _ => {}
        }
        at += length;
    }

    let scan_at = scan_start.ok_or_else(|| corrupt("lossless jpeg has no scan"))?;
    if components.is_empty() || frame_width == 0 || frame_height == 0 {
        return Err(corrupt("lossless jpeg frame header is incomplete"));
    }
    if !(2..=16).contains(&precision) {
        return Err(corrupt(format!(
            "lossless jpeg declares {precision}-bit samples"
        )));
    }

    let component_count = components.len();
    let width = frame_width
        .checked_mul(u32::try_from(component_count).unwrap_or(1))
        .ok_or_else(|| corrupt("lossless jpeg width overflows"))?;
    let total = (width as usize)
        .checked_mul(frame_height as usize)
        .ok_or_else(|| corrupt("lossless jpeg sample count overflows"))?;
    if total > MAX_SAMPLES {
        return Err(too_large(total as u64, MAX_SAMPLES as u64));
    }

    let mut samples = vec![0u16; total];
    let mut reader = BitReader::new(bytes.get(scan_at..).unwrap_or(&[]));

    // The value the first pixel of the image is predicted from, T.81 H.2.1.
    let seed = 1i32 << (precision - 1 - point_transform.min(precision - 1));

    for y in 0..frame_height as usize {
        for x in 0..frame_width as usize {
            for (index, component) in components.iter().enumerate() {
                let position = y * width as usize + x * component_count + index;
                let table = tables
                    .get(component.table)
                    .ok_or_else(|| corrupt("scan names a huffman table that was never defined"))?;
                let magnitude = u32::from(reader.decode(table)?);
                let difference = if magnitude == 16 {
                    // T.81 H.1.2.2: SSSS = 16 codes a difference of 32768.
                    32_768i32
                } else {
                    let raw = reader.bits(magnitude);
                    extend(raw, magnitude)
                };

                let left = if x > 0 {
                    samples
                        .get(position - component_count)
                        .copied()
                        .unwrap_or(0)
                } else {
                    0
                };
                let above = if y > 0 {
                    samples
                        .get(position - width as usize)
                        .copied()
                        .unwrap_or(0)
                } else {
                    0
                };
                let above_left = if x > 0 && y > 0 {
                    samples
                        .get(position - width as usize - component_count)
                        .copied()
                        .unwrap_or(0)
                } else {
                    0
                };

                let prediction = predict(
                    predictor,
                    x,
                    y,
                    seed,
                    i32::from(left),
                    i32::from(above),
                    i32::from(above_left),
                );
                let value = (prediction + difference) & 0xFFFF;
                if let Some(slot) = samples.get_mut(position) {
                    *slot = u16::try_from(value).unwrap_or(0);
                }
            }
        }
    }

    Ok(LosslessImage {
        width,
        height: frame_height,
        components: component_count,
        precision,
        samples,
    })
}

/// The seven predictors of T.81 table H.1, plus the edge rules.
fn predict(selector: u8, x: usize, y: usize, seed: i32, ra: i32, rb: i32, rc: i32) -> i32 {
    if x == 0 {
        // First column: predict from the row above, or from the seed on row 0.
        return if y == 0 { seed } else { rb };
    }
    if y == 0 {
        // First row: only the left neighbour exists.
        return ra;
    }
    match selector {
        1 => ra,
        2 => rb,
        3 => rc,
        4 => ra + rb - rc,
        5 => ra + ((rb - rc) >> 1),
        6 => rb + ((ra - rc) >> 1),
        7 => (ra + rb) >> 1,
        // Selector 0 is "no prediction", used only in hierarchical mode.
        _ => 0,
    }
}
