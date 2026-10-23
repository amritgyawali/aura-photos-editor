//! Panasonic's RW2 mosaic, in every encoding up to the S1 and S5.
//!
//! RW2 is a TIFF whose first directory uses Panasonic's own tag numbers. Tag
//! `0x002D` names the encoding:
//!
//! * **2 to 4** - nearly every Lumix from the G1 to the G9: fourteen-pixel
//!   groups of a differential code read *backwards* through 16 KiB pages that
//!   the camera writes rotated by 0x2008 bytes.
//! * **5** - the GH5S and pixel-shift G9: plain bit-packing, nine 14-bit or ten
//!   12-bit samples in every 16 bytes, through the same rotated pages.
//! * **6** - the S1, S1R, S1H and S5: eleven 14-bit (or fourteen 12-bit) samples
//!   in each 16-byte block, coded as two exact values and nine scaled deltas.
//! * **7** - the S1R's pixel-shift mode: plain packing in 16-byte blocks.
//!
//! Encoding 8 (GH6, S5 Mark II and later) is a Huffman coder of its own and lives
//! in [`super::pana8`].
//!
//! The formats are the ones documented by the open-source decoders (dcraw,
//! LibRaw); this is an independent safe implementation of them.

/// How a frame is encoded, read from the RW2's first directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanaParams {
    /// Tag `0x002D`.
    pub encoding: u16,
    /// Tag `0x000A`, 12 or 14.
    pub bits: u16,
}

/// Bytes the camera rotates each 16 KiB page by.
const ROTATION: usize = 0x2008;
const PAGE: usize = 0x4000;

/// The rotated 16 KiB page reader shared by encodings 2 to 5.
struct Pages<'a> {
    payload: &'a [u8],
    at: usize,
    page: Vec<u8>,
    /// Bit position inside the page for the backwards reader, byte position for
    /// the block reader; zero means "load the next page".
    position: usize,
}

impl<'a> Pages<'a> {
    fn new(payload: &'a [u8]) -> Self {
        Self {
            payload,
            at: 0,
            page: vec![0; PAGE + 2],
            position: 0,
        }
    }

    fn load(&mut self) {
        let copy = |from: usize, to: usize, count: usize, page: &mut Vec<u8>| {
            for offset in 0..count {
                if let Some(slot) = page.get_mut(to + offset) {
                    *slot = self.payload.get(from + offset).copied().unwrap_or(0);
                }
            }
        };
        copy(self.at, ROTATION, PAGE - ROTATION, &mut self.page);
        copy(self.at + PAGE - ROTATION, 0, ROTATION, &mut self.page);
        self.at += PAGE;
    }

    /// `count` bits, read backwards from the end of the page.
    fn bits(&mut self, count: u32) -> u32 {
        if self.position == 0 {
            self.load();
        }
        self.position = self.position.wrapping_sub(count as usize) & 0x1FFFF;
        let byte = (self.position >> 3) ^ 0x3FF0;
        let low = u32::from(self.page.get(byte).copied().unwrap_or(0));
        let high = u32::from(self.page.get(byte + 1).copied().unwrap_or(0));
        ((low | high << 8) >> (self.position & 7)) & !(u32::MAX << count)
    }

    /// The next sixteen bytes, in page order.
    fn block(&mut self) -> [u32; 16] {
        if self.position == 0 {
            self.load();
        }
        let mut out = [0u32; 16];
        for slot in &mut out {
            *slot = u32::from(self.page.get(self.position).copied().unwrap_or(0));
            self.position = (self.position + 1) & 0x3FFF;
        }
        out
    }
}

/// Decode an RW2 mosaic of `width * height` samples starting at `data`.
#[must_use]
pub fn decode(payload: &[u8], params: PanaParams, width: usize, height: usize) -> Vec<u16> {
    match params.encoding {
        5 => decode_packed_pages(payload, params.bits, width, height),
        6 => decode_c6(payload, params.bits, width, height),
        7 => decode_c7(payload, params.bits, width, height),
        _ => decode_classic(payload, width, height),
    }
}

fn decode_classic(payload: &[u8], width: usize, height: usize) -> Vec<u16> {
    let mut plane = vec![0u16; width * height];
    let mut pages = Pages::new(payload);
    let (mut pred, mut nonzero) = ([0i32; 2], [0u32; 2]);
    let mut shift = 0u32;
    for row in 0..height {
        for column in 0..width {
            let i = column % 14;
            let parity = i & 1;
            if i == 0 {
                pred = [0, 0];
                nonzero = [0, 0];
            }
            if i % 3 == 2 {
                shift = 4 >> (3 - pages.bits(2));
            }
            let (Some(p), Some(nz)) = (pred.get_mut(parity), nonzero.get_mut(parity)) else {
                continue;
            };
            if *nz != 0 {
                let j = pages.bits(8);
                if j != 0 {
                    *p -= 0x80 << shift;
                    if *p < 0 || shift == 4 {
                        *p = (*p as u32 & !(u32::MAX << shift)) as i32;
                    }
                    *p += (j << shift) as i32;
                }
            } else {
                *nz = pages.bits(8);
                if *nz != 0 || i > 11 {
                    *p = (*nz << 4 | pages.bits(4)) as i32;
                }
            }
            if let Some(slot) = plane.get_mut(row * width + column) {
                *slot = pred.get(column & 1).copied().unwrap_or(0) as u16;
            }
        }
    }
    plane
}

/// Unpack one 16-byte block of plain little-endian packing: ten 12-bit or nine
/// 14-bit samples.
fn unpack_block(bytes: &[u32; 16], bits: u16, out: &mut [u16]) {
    let b = |i: usize| bytes.get(i).copied().unwrap_or(0);
    let values: Vec<u32> = if bits == 12 {
        vec![
            ((b(1) & 0xF) << 8) + b(0),
            16 * b(2) + (b(1) >> 4),
            ((b(4) & 0xF) << 8) + b(3),
            16 * b(5) + (b(4) >> 4),
            ((b(7) & 0xF) << 8) + b(6),
            16 * b(8) + (b(7) >> 4),
            ((b(10) & 0xF) << 8) + b(9),
            16 * b(11) + (b(10) >> 4),
            ((b(13) & 0xF) << 8) + b(12),
            16 * b(14) + (b(13) >> 4),
        ]
    } else {
        vec![
            b(0) + ((b(1) & 0x3F) << 8),
            (b(1) >> 6) + 4 * b(2) + ((b(3) & 0xF) << 10),
            (b(3) >> 4) + 16 * b(4) + ((b(5) & 3) << 12),
            ((b(5) & 0xFC) >> 2) + (b(6) << 6),
            b(7) + ((b(8) & 0x3F) << 8),
            (b(8) >> 6) + 4 * b(9) + ((b(10) & 0xF) << 10),
            (b(10) >> 4) + 16 * b(11) + ((b(12) & 3) << 12),
            ((b(12) & 0xFC) >> 2) + (b(13) << 6),
            b(14) + ((b(15) & 0x3F) << 8),
        ]
    };
    for (slot, value) in out.iter_mut().zip(values) {
        *slot = value as u16;
    }
}

fn decode_packed_pages(payload: &[u8], bits: u16, width: usize, height: usize) -> Vec<u16> {
    let per_block = if bits == 12 { 10 } else { 9 };
    let mut plane = vec![0u16; width * height];
    let mut pages = Pages::new(payload);
    for row in 0..height {
        let mut column = 0;
        while column < width {
            let block = pages.block();
            let mut samples = [0u16; 10];
            unpack_block(&block, bits, &mut samples);
            for (offset, value) in samples.iter().take(per_block).enumerate() {
                if column + offset < width {
                    if let Some(slot) = plane.get_mut(row * width + column + offset) {
                        *slot = *value;
                    }
                }
            }
            column += per_block;
        }
    }
    plane
}

fn decode_c7(payload: &[u8], bits: u16, width: usize, height: usize) -> Vec<u16> {
    let per_block = if bits == 14 { 9 } else { 10 };
    let row_bytes = width / per_block * 16;
    let mut plane = vec![0u16; width * height];
    // The reference reads sixteen rows at a time and drops a final partial group.
    let rows = height - height % 16;
    for row in 0..rows {
        let mut at = row * row_bytes;
        let mut column = 0;
        while column + per_block <= width {
            let mut block = [0u32; 16];
            for (index, slot) in block.iter_mut().enumerate() {
                *slot = u32::from(payload.get(at + index).copied().unwrap_or(0));
            }
            let mut samples = [0u16; 10];
            unpack_block(&block, if bits == 14 { 14 } else { 12 }, &mut samples);
            for (offset, value) in samples.iter().take(per_block).enumerate() {
                if let Some(slot) = plane.get_mut(row * width + column + offset) {
                    *slot = *value;
                }
            }
            column += per_block;
            at += 16;
        }
    }
    plane
}

/// Encoding 6's fields, most significant first from the block's last byte.
fn c6_fields(block: &[u8], twelve: bool) -> [u32; 18] {
    let w = |i: usize| u32::from(block.get(15 - i).copied().unwrap_or(0));
    let mut p = [0u32; 18];
    if twelve {
        p[0] = (w(0) << 4) | (w(1) >> 4);
        p[1] = (((w(1) & 0xF) << 8) | w(2)) & 0xFFF;
        p[2] = (w(3) >> 6) & 0x3;
        p[3] = ((w(3) & 0x3F) << 2) | (w(4) >> 6);
        p[4] = ((w(4) & 0x3F) << 2) | (w(5) >> 6);
        p[5] = ((w(5) & 0x3F) << 2) | (w(6) >> 6);
        p[6] = (w(6) >> 4) & 0x3;
        p[7] = ((w(6) & 0xF) << 4) | (w(7) >> 4);
        p[8] = ((w(7) & 0xF) << 4) | (w(8) >> 4);
        p[9] = ((w(8) & 0xF) << 4) | (w(9) >> 4);
        p[10] = (w(9) >> 2) & 0x3;
        p[11] = ((w(9) & 0x3) << 6) | (w(10) >> 2);
        p[12] = ((w(10) & 0x3) << 6) | (w(11) >> 2);
        p[13] = ((w(11) & 0x3) << 6) | (w(12) >> 2);
        p[14] = w(12) & 0x3;
        p[15] = w(13);
        p[16] = w(14);
        p[17] = w(15);
    } else {
        p[0] = (w(0) << 6) | (w(1) >> 2);
        p[1] = (((w(1) & 0x3) << 12) | (w(2) << 4) | (w(3) >> 4)) & 0x3FFF;
        p[2] = (w(3) >> 2) & 0x3;
        p[3] = ((w(3) & 0x3) << 8) | w(4);
        p[4] = (w(5) << 2) | (w(6) >> 6);
        p[5] = ((w(6) & 0x3F) << 4) | (w(7) >> 4);
        p[6] = (w(7) >> 2) & 0x3;
        p[7] = ((w(7) & 0x3) << 8) | w(8);
        p[8] = ((w(9) << 2) & 0x3FC) | (w(10) >> 6);
        p[9] = ((w(10) << 4) | (w(11) >> 4)) & 0x3FF;
        p[10] = (w(11) >> 2) & 0x3;
        p[11] = ((w(11) & 0x3) << 8) | w(12);
        p[12] = (((w(13) << 2) & 0x3FC) | w(14) >> 6) & 0x3FF;
        p[13] = ((w(14) << 4) | (w(15) >> 4)) & 0x3FF;
    }
    p
}

#[allow(clippy::indexing_slicing)] // fixed-size arrays indexed by `% 2` and by a bounded counter
fn decode_c6(payload: &[u8], bits: u16, width: usize, height: usize) -> Vec<u16> {
    let twelve = bits == 12;
    let per_block = if twelve { 14 } else { 11 };
    let fields = if twelve { 18 } else { 14 };
    let blocks = width / per_block;
    let row_bytes = blocks * 16;
    let base0: u32 = if twelve { 0x80 } else { 0x200 };
    let base_limit: u32 = if twelve { 0x800 } else { 0x2000 };
    let sample_limit: u32 = if twelve { 0x3FFF } else { 0xFFFF };
    let mask: u32 = if twelve { 0xFFF } else { 0x3FFF };
    let mut plane = vec![0u16; width * height];
    let rows = height - height % 16;
    for row in 0..rows {
        let mut column = 0;
        for block_index in 0..blocks {
            let at = row * row_bytes + block_index * 16;
            let block = payload.get(at..at + 16).unwrap_or(&[]);
            let p = c6_fields(block, twelve);
            let mut next = 0usize;
            let mut take = || {
                let value = if next < fields { p[next] } else { 0 };
                next += 1;
                value
            };
            let (mut odd_even, mut nonzero) = ([0u32; 2], [0u32; 2]);
            let (mut multiplier, mut base) = (0u32, 0u32);
            for pixel in 0..per_block {
                if pixel % 3 == 2 {
                    let mut exponent = take();
                    if exponent == 3 {
                        exponent = 4;
                    }
                    base = base0 << exponent;
                    multiplier = 1 << exponent;
                }
                let mut value = take();
                let parity = pixel % 2;
                if odd_even[parity] != 0 {
                    value = value.wrapping_mul(multiplier);
                    if base < base_limit && nonzero[parity] > base {
                        value = value.wrapping_add(nonzero[parity] - base);
                    }
                    nonzero[parity] = value;
                } else {
                    odd_even[parity] = value;
                    if value != 0 {
                        nonzero[parity] = value;
                    } else {
                        value = nonzero[parity];
                    }
                }
                let sample = value.wrapping_sub(0xF);
                let stored = if sample <= sample_limit {
                    sample & sample_limit
                } else {
                    (((value.wrapping_add(0x7FFF_FFF1)) as i32) >> 31) as u32 & mask
                };
                if let Some(slot) = plane.get_mut(row * width + column) {
                    *slot = stored as u16;
                }
                column += 1;
            }
        }
    }
    plane
}
