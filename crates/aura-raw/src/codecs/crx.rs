//! Canon's CRX codec: the sensor data inside a CR3.
//!
//! Every Canon body since the EOS M50 writes CR3, and CR3's sensor data is CRX.
//! The four Bayer colours are coded as four half-size planes. Each plane is cut
//! into tiles; each tile is either coded directly (full RAW, which is lossless)
//! or first split into subbands by up to three levels of the LeGall 5/3 integer
//! wavelet (C-RAW, which is lossy). Either way the coefficients are entropy coded
//! line by line with an adaptive Golomb-Rice code whose parameter is predicted
//! from the line above, plus a run mode for flat stretches.
//!
//! The format is unpublished. This is an independent safe-Rust implementation of
//! the description the open-source community worked out - Alexey Danilchenko's
//! decoder in LibRaw (`crx.cpp`) is the reference it was checked against, sample
//! for sample, on real files (`tests/reference_decode.rs`). Where the reference
//! does something that looks unintended - a row offset that subtracts the wrong
//! add-on, a line buffer aliased onto itself at the bottom edge - this module does
//! the same thing on purpose, because the encoder in the camera was written to
//! match what decodes, not what reads well.
//!
//! Planes and tiles are independent, so they decode in parallel; nothing in here
//! indexes a slice directly, and a hostile file produces an error or a wrong
//! picture, never a panic.

use aura_core::errors::raw::corrupt;
use aura_core::{AuraError, AuraResult};
use rayon::prelude::*;

/// Tile neighbour flags.
const RIGHT: u8 = 1;
const LEFT: u8 = 2;
const BOTTOM: u8 = 4;
const TOP: u8 = 8;

/// Run-length increments, indexed by the run state.
const JS: [i32; 32] = [
    1, 1, 1, 1, 2, 2, 2, 2, 4, 4, 4, 4, 8, 8, 8, 8, 0x10, 0x10, 0x20, 0x20, 0x40, 0x40, 0x80, 0x80,
    0x100, 0x200, 0x400, 0x800, 0x1000, 0x2000, 0x4000, 0x8000,
];

/// Bits of explicit run length, indexed by the run state.
const J: [u32; 32] = [
    0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 9, 10, 11, 12, 13,
    14, 15,
];

/// Quantisation steps for one sixth of an octave.
const Q_STEP: [i32; 8] = [0x28, 0x2D, 0x33, 0x39, 0x40, 0x48, 0, 0];

/// How many extra coefficients a subband carries at a tile edge, by level count,
/// tile size modulo eight and level.
const EX_COEF: [i32; 144] = [
    1, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 1, 0,
    0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 1, 1, 1, 0, 0, 1, 0, 1, 0, 0, 0, 1, 2, 2, 1,
    0, 0, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 0, 1, 0, 1, 0, 0, 0, 1, 2, 2, 1, 0, 0, 1, 1, 1, 1, 0, 0,
    1, 1, 1, 1, 1, 1, 1, 0, 1, 0, 1, 0, 1, 2, 2, 2, 2, 1, 1, 1, 1, 2, 2, 1, 1, 1, 1, 2, 2, 1, 1, 0,
    1, 1, 1, 1, 1, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1,
];

/// The `CMP1` image header of a CR3 track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CrxHeader {
    /// 0x100, or 0x200 for the bodies that carry per-tile quantisation.
    pub version: u16,
    /// Width of the whole sensor in photosites.
    pub width: u32,
    /// Height of the whole sensor in photosites.
    pub height: u32,
    /// Tile width in photosites.
    pub tile_width: u32,
    /// Tile height in photosites.
    pub tile_height: u32,
    /// Bits per coded sample.
    pub bits: u8,
    /// Coded planes: 4 for a Bayer frame, 1 for an 8-bit image.
    pub planes: u8,
    /// Which colour the top-left photosite is: 0 RGGB, 1 GRBG, 2 GBRG, 3 BGGR.
    pub cfa_layout: u8,
    /// Sample encoding: 0 plain, 1 signed, 3 a colour transform of the four planes.
    pub encoding: u8,
    /// Wavelet levels: 0 for lossless RAW, 1 to 3 for C-RAW.
    pub levels: u8,
    /// Bytes of tile, plane and subband headers at the start of the frame.
    pub header_size: u32,
    /// Output precision of the colour-transform encoding.
    pub median_bits: u8,
}

fn be16(bytes: &[u8], at: usize) -> u32 {
    u32::from(bytes.get(at).copied().unwrap_or(0)) << 8
        | u32::from(bytes.get(at + 1).copied().unwrap_or(0))
}

fn be32(bytes: &[u8], at: usize) -> u32 {
    be16(bytes, at) << 16 | be16(bytes, at + 2)
}

impl CrxHeader {
    /// Read and validate a `CMP1` payload. `None` means this build cannot decode
    /// the track, which the caller reports rather than guesses at.
    #[must_use]
    pub fn parse(cmp1: &[u8]) -> Option<Self> {
        if cmp1.len() < 32 {
            return None;
        }
        let byte = |at: usize| cmp1.get(at).copied().unwrap_or(0);
        let mut header = Self {
            version: be16(cmp1, 4) as u16,
            width: be32(cmp1, 8),
            height: be32(cmp1, 12),
            tile_width: be32(cmp1, 16),
            tile_height: be32(cmp1, 20),
            bits: byte(24),
            planes: byte(25) >> 4,
            cfa_layout: byte(25) & 0x0F,
            encoding: byte(26) >> 4,
            levels: byte(26) & 0x0F,
            header_size: be32(cmp1, 28),
            median_bits: byte(24),
        };
        let has_tile_columns = byte(27) >> 7;
        let has_tile_rows = (byte(27) >> 6) & 1;
        let extended = byte(32) >> 7 != 0;
        if extended
            && cmp1.len() >= 56
            && header.planes == 4
            && (byte(56) >> 6) & 1 != 0
            && cmp1.len() >= 84
        {
            header.median_bits = byte(84);
        }

        if !matches!(header.version, 0x100 | 0x200) || header.header_size == 0 {
            return None;
        }
        if header.encoding == 1 {
            if header.bits > 15 {
                return None;
            }
        } else if !matches!(header.encoding, 0 | 3) || header.bits > 14 {
            return None;
        }
        if header.planes == 1 {
            if header.cfa_layout != 0 || header.encoding != 0 || header.bits != 8 {
                return None;
            }
        } else if header.planes != 4
            || header.width & 1 != 0
            || header.height & 1 != 0
            || header.tile_width & 1 != 0
            || header.tile_height & 1 != 0
            || header.cfa_layout > 3
            || header.bits == 8
        {
            return None;
        }
        if header.tile_width > header.width
            || header.tile_height > header.height
            || header.levels > 3
            || has_tile_columns > 1
            || has_tile_rows > 1
            || header.bits == 0
        {
            return None;
        }
        Some(header)
    }

    /// The sample precision the decoded values occupy.
    #[must_use]
    pub const fn output_bits(&self) -> u8 {
        if self.encoding == 3 {
            self.median_bits
        } else {
            self.bits
        }
    }
}

// ---------------------------------------------------------------------------
// Bit stream
// ---------------------------------------------------------------------------

/// MSB-first reader over one subband's bytes.
struct Stream<'a> {
    payload: &'a [u8],
    at: usize,
    buffer: u64,
    held: u32,
}

impl<'a> Stream<'a> {
    const fn new(payload: &'a [u8]) -> Self {
        Self {
            payload,
            at: 0,
            buffer: 0,
            held: 0,
        }
    }

    #[inline]
    fn refill(&mut self) {
        while self.held <= 56 {
            let byte = self.payload.get(self.at).copied().unwrap_or(0);
            self.at += 1;
            self.buffer |= u64::from(byte) << (56 - self.held);
            self.held += 8;
        }
    }

    #[inline]
    fn bits(&mut self, count: u32) -> u32 {
        if count == 0 {
            return 0;
        }
        if self.held < count {
            self.refill();
        }
        let value = (self.buffer >> (64 - count)) as u32;
        self.buffer <<= count;
        self.held -= count;
        value
    }

    /// Count zero bits up to and including the next one bit; returns the count of zeros.
    #[inline]
    fn zeros(&mut self) -> AuraResult<u32> {
        let mut count = 0u32;
        loop {
            if self.held < 32 {
                self.refill();
            }
            let top = (self.buffer >> 32) as u32;
            if top != 0 {
                let leading = top.leading_zeros();
                self.buffer <<= leading + 1;
                self.held -= leading + 1;
                return Ok(count + leading);
            }
            self.buffer <<= 32;
            self.held -= 32;
            count += 32;
            if self.at > self.payload.len() + 8 {
                return Err(truncated());
            }
        }
    }
}

fn truncated() -> AuraError {
    corrupt("crx entropy data ended before the frame was complete")
}

#[inline]
fn signed(code: u32) -> i32 {
    (-((code & 1) as i32)) ^ ((code >> 1) as i32)
}

#[inline]
fn predict_k(previous: i32, code: i32, ceiling: i32) -> i32 {
    let half = 1i32.checked_shl(previous as u32).unwrap_or(0) >> 1;
    let shifted = code.checked_shr(previous as u32).unwrap_or(0);
    let next = previous - i32::from(code < half) + i32::from(shifted > 2) + i32::from(shifted > 5);
    if ceiling == 0 || next < ceiling {
        next
    } else {
        ceiling
    }
}

#[inline]
fn abs(value: i32) -> i32 {
    value.wrapping_abs()
}

/// One adaptive Golomb-Rice code: zeros, then `k` more bits, with an escape to a
/// fixed-length code for long runs of zeros.
#[inline]
fn golomb(stream: &mut Stream<'_>, k: i32, escape: u32, escape_bits: u32) -> AuraResult<u32> {
    let code = stream.zeros()?;
    Ok(if code >= escape {
        stream.bits(escape_bits)
    } else if k > 0 {
        stream.bits(k as u32) | (code << k)
    } else {
        code
    })
}

// ---------------------------------------------------------------------------
// One subband's line decoder
// ---------------------------------------------------------------------------

struct BandParam<'a> {
    stream: Stream<'a>,
    width: usize,
    height: usize,
    rounded_mask: i32,
    rounded_bits: i32,
    line: usize,
    /// Two lines of `width + 2`, alternating between "above" and "current".
    payload: Vec<i32>,
    /// The per-sample k history the non-progressive coder reads.
    history: Vec<i32>,
    l0: usize,
    l1: usize,
    l2: usize,
    s: usize,
    k: i32,
    partial: bool,
}

impl<'a> BandParam<'a> {
    fn new(
        payload: &'a [u8],
        width: usize,
        height: usize,
        partial: bool,
        rounded_mask: i32,
    ) -> Self {
        Self {
            stream: Stream::new(payload),
            width,
            height,
            rounded_mask,
            rounded_bits: 0,
            line: 0,
            payload: vec![0; 2 * width + 4],
            history: if partial {
                Vec::new()
            } else {
                vec![0; width + 1]
            },
            l0: 0,
            l1: 0,
            l2: 0,
            s: 0,
            k: 0,
            partial,
        }
    }

    #[inline]
    fn g0(&self, offset: usize) -> i32 {
        self.payload.get(self.l0 + offset).copied().unwrap_or(0)
    }

    #[inline]
    fn g1(&self, offset: usize) -> i32 {
        self.payload.get(self.l1 + offset).copied().unwrap_or(0)
    }

    #[inline]
    fn set0(&mut self, offset: usize, value: i32) {
        if let Some(slot) = self.payload.get_mut(self.l0 + offset) {
            *slot = value;
        }
    }

    #[inline]
    fn set1(&mut self, offset: usize, value: i32) {
        if let Some(slot) = self.payload.get_mut(self.l1 + offset) {
            *slot = value;
        }
    }

    #[inline]
    fn g2(&self, offset: usize) -> i32 {
        self.history.get(self.l2 + offset).copied().unwrap_or(0)
    }

    #[inline]
    fn set2(&mut self, offset: usize, value: i32) {
        if let Some(slot) = self.history.get_mut(self.l2 + offset) {
            *slot = value;
        }
    }

    #[inline]
    fn code(&mut self) -> AuraResult<u32> {
        golomb(&mut self.stream, self.k, 41, 21)
    }

    /// The length of a run of repeated values, in the run-mode code. `limit` is
    /// how far the run may reach; the result can exceed it only on corrupt data.
    fn run(&mut self, limit: i32) -> i32 {
        let mut count = 1i32;
        while self.stream.bits(1) != 0 {
            count += JS.get(self.s).copied().unwrap_or(1);
            if count > limit {
                count = limit;
                break;
            }
            if self.s < 31 {
                self.s += 1;
            }
            if count == limit {
                break;
            }
        }
        if count < limit {
            let extra = J.get(self.s).copied().unwrap_or(0);
            if extra > 0 {
                count += self.stream.bits(extra) as i32;
            }
            self.s = self.s.saturating_sub(1);
        }
        count
    }

    fn median(&self) -> i32 {
        let delta = self.g0(1).wrapping_sub(self.g0(0));
        let left = self.g1(0);
        let symbols = [
            delta.wrapping_add(left),
            delta.wrapping_add(left),
            left,
            self.g0(1),
        ];
        let index = (usize::from((self.g0(0) < left) ^ (delta < 0)) << 1)
            + usize::from((left < self.g0(1)) ^ (delta < 0));
        symbols.get(index).copied().unwrap_or(0)
    }

    fn symbol_l1(&mut self, median: bool, not_end: bool) -> AuraResult<()> {
        let predicted = if median { self.median() } else { self.g0(1) };
        let mut code = self.code()?;
        self.set1(1, predicted.wrapping_add(signed(code)));
        if not_end {
            let next = self.g0(2).wrapping_sub(self.g0(1)).wrapping_shl(1);
            code = code.wrapping_add(abs(next) as u32) >> 1;
            self.l0 += 1;
        }
        self.k = predict_k(self.k, code as i32, 15);
        self.l1 += 1;
        Ok(())
    }

    fn line_l1(&mut self) -> AuraResult<()> {
        let mut length = self.width as i32;
        self.set1(0, self.g0(1));
        while length > 1 {
            if self.g1(0) != self.g0(1) || self.g1(0) != self.g0(2) {
                self.symbol_l1(true, true)?;
            } else {
                if self.stream.bits(1) != 0 {
                    let count = self.run(length);
                    if count > length {
                        return Err(corrupt("crx run is longer than the line"));
                    }
                    length -= count;
                    self.l0 += count as usize;
                    for _ in 0..count {
                        self.set1(1, self.g1(0));
                        self.l1 += 1;
                    }
                }
                if length > 0 {
                    self.symbol_l1(false, length > 1)?;
                }
            }
            length -= 1;
        }
        if length == 1 {
            self.symbol_l1(true, false)?;
        }
        self.set1(1, self.g1(0).wrapping_add(1));
        Ok(())
    }

    fn symbol_rounded(&mut self, use_symbol: bool, use_code: bool) -> AuraResult<()> {
        let predicted = if use_symbol {
            self.median()
        } else {
            self.g0(1)
        };
        let code = self.code()?;
        let value = signed(code);
        self.set1(
            1,
            self.rounded_mask
                .wrapping_mul(2)
                .wrapping_mul(value)
                .wrapping_add(value >> 31)
                .wrapping_add(predicted),
        );
        if use_code {
            let next = if self.g0(2) > self.g0(1) {
                (self.g0(2) - self.g0(1) + self.rounded_mask - 1) >> self.rounded_bits
            } else {
                -((self.g0(1) - self.g0(2) + self.rounded_mask) >> self.rounded_bits)
            };
            let blended = code.wrapping_add(2u32.wrapping_mul(abs(next) as u32)) >> 1;
            self.k = predict_k(self.k, blended as i32, 15);
        } else {
            self.k = predict_k(self.k, code as i32, 15);
        }
        self.l1 += 1;
        Ok(())
    }

    fn line_rounded(&mut self) -> AuraResult<()> {
        let mask = self.rounded_mask;
        let mut reached = false;
        self.set0(0, self.g0(1));
        self.set1(0, self.g0(1));
        let mut length = self.width as i32;
        while length > 1 {
            if abs(self.g0(2).wrapping_sub(self.g0(1))) > mask {
                self.symbol_rounded(true, true)?;
                self.l0 += 1;
                reached = true;
            } else if reached || abs(self.g0(0).wrapping_sub(self.g1(0))) > mask {
                self.symbol_rounded(true, true)?;
                self.l0 += 1;
                reached = false;
            } else {
                let mut count = 0;
                if self.stream.bits(1) != 0 {
                    count = self.run(length);
                    if count > length {
                        return Err(corrupt("crx run is longer than the line"));
                    }
                }
                length -= count;
                self.l0 += count as usize;
                for _ in 0..count {
                    self.set1(1, self.g1(0));
                    self.l1 += 1;
                }
                if length > 1 {
                    self.symbol_rounded(false, true)?;
                    self.l0 += 1;
                    reached = abs(self.g0(1).wrapping_sub(self.g0(0))) > mask;
                } else if length == 1 {
                    self.symbol_rounded(false, false)?;
                }
            }
            length -= 1;
        }
        if length == 1 {
            self.symbol_rounded(true, false)?;
        }
        self.set1(1, self.g1(0).wrapping_add(1));
        Ok(())
    }

    fn adapt_from_history(&mut self, offset: usize) {
        if self.g2(offset) - self.k <= 1 {
            if self.k >= 15 {
                self.k = 15;
            }
        } else {
            self.k += 1;
        }
    }

    fn line_no_reference(&mut self) -> AuraResult<()> {
        let width = self.width;
        let mut i = 0usize;
        while i + 1 < width {
            if self.g0(i + 2) | self.g0(i + 1) | self.g1(i) != 0 {
                let code = self.code()?;
                self.set1(i + 1, signed(code));
                self.k = predict_k(self.k, code as i32, 0);
                self.adapt_from_history(i + 1);
            } else {
                let mut count = 0usize;
                if self.stream.bits(1) != 0 {
                    count = 1;
                    let mut run = 1i32;
                    let limit = (width - i) as i32;
                    // The same run code, measured against the end of the line.
                    while self.stream.bits(1) != 0 {
                        run += JS.get(self.s).copied().unwrap_or(1);
                        if run > limit {
                            run = limit;
                            break;
                        }
                        if self.s < 31 {
                            self.s += 1;
                        }
                        if run == limit {
                            break;
                        }
                    }
                    if run < limit {
                        let extra = J.get(self.s).copied().unwrap_or(0);
                        if extra > 0 {
                            run += self.stream.bits(extra) as i32;
                        }
                        self.s = self.s.saturating_sub(1);
                    }
                    if run > limit {
                        return Err(corrupt("crx run is longer than the line"));
                    }
                    count = count.max(run as usize);
                }
                if count > 0 {
                    for j in 0..count {
                        self.set1(i + 1 + j, 0);
                        self.set2(i + j, 0);
                    }
                    i += count;
                }
                if i + 1 >= width {
                    if i + 1 == width {
                        let code = self.code()?;
                        self.set1(i + 1, signed(code.wrapping_add(1)));
                        self.k = predict_k(self.k, code as i32, 15);
                        self.set2(i, self.k);
                    }
                    i += 1;
                    continue;
                }
                let code = self.code()?;
                self.set1(i + 1, signed(code.wrapping_add(1)));
                self.k = predict_k(self.k, code as i32, 0);
                self.adapt_from_history(i + 1);
            }
            self.set2(i, self.k);
            i += 1;
        }
        if i + 1 == width {
            let code = self.code()?;
            self.set1(i + 1, signed(code));
            self.k = predict_k(self.k, code as i32, 15);
            self.set2(i, self.k);
        }
        Ok(())
    }

    fn top_line(&mut self) -> AuraResult<()> {
        self.set1(0, 0);
        let mut length = self.width as i32;
        while length > 1 {
            if self.g1(0) != 0 {
                self.set1(1, self.g1(0));
            } else {
                if self.stream.bits(1) != 0 {
                    let count = self.run(length);
                    if count > length {
                        return Err(corrupt("crx run is longer than the line"));
                    }
                    length -= count;
                    for _ in 0..count {
                        self.set1(1, self.g1(0));
                        self.l1 += 1;
                    }
                    if length <= 0 {
                        break;
                    }
                }
                self.set1(1, 0);
            }
            let code = self.code()?;
            self.set1(1, self.g1(1).wrapping_add(signed(code)));
            self.k = predict_k(self.k, code as i32, 15);
            self.l1 += 1;
            length -= 1;
        }
        if length == 1 {
            self.set1(1, self.g1(0));
            let code = self.code()?;
            self.set1(1, self.g1(1).wrapping_add(signed(code)));
            self.k = predict_k(self.k, code as i32, 15);
            self.l1 += 1;
        }
        self.set1(1, self.g1(0).wrapping_add(1));
        Ok(())
    }

    fn rounded_value(&self, value: i32) -> i32 {
        self.rounded_mask
            .wrapping_mul(2)
            .wrapping_mul(value)
            .wrapping_add(value >> 31)
    }

    fn top_line_rounded(&mut self) -> AuraResult<()> {
        self.set1(0, 0);
        let mut length = self.width as i32;
        while length > 1 {
            if abs(self.g1(0)) > self.rounded_mask {
                self.set1(1, self.g1(0));
            } else {
                let mut count = 0;
                if self.stream.bits(1) != 0 {
                    count = self.run(length);
                    if count > length {
                        return Err(corrupt("crx run is longer than the line"));
                    }
                }
                length -= count;
                for _ in 0..count {
                    self.set1(1, self.g1(0));
                    self.l1 += 1;
                }
                if length <= 0 {
                    break;
                }
                self.set1(1, 0);
            }
            let code = self.code()?;
            let add = self.rounded_value(signed(code));
            self.set1(1, self.g1(1).wrapping_add(add));
            self.k = predict_k(self.k, code as i32, 15);
            self.l1 += 1;
            length -= 1;
        }
        if length == 1 {
            let code = self.code()?;
            let add = self.rounded_value(signed(code));
            self.set1(1, self.g1(1).wrapping_add(add));
            self.k = predict_k(self.k, code as i32, 15);
            self.l1 += 1;
        }
        self.set1(1, self.g1(0).wrapping_add(1));
        Ok(())
    }

    fn top_line_no_reference(&mut self) -> AuraResult<()> {
        self.set0(0, 0);
        self.set1(0, 0);
        let mut length = self.width as i32;
        while length > 1 {
            if self.g1(0) != 0 {
                let code = self.code()?;
                self.set1(1, signed(code));
                self.k = predict_k(self.k, code as i32, 15);
            } else {
                let mut count = 0;
                if self.stream.bits(1) != 0 {
                    count = self.run(length);
                    if count > length {
                        return Err(corrupt("crx run is longer than the line"));
                    }
                }
                length -= count;
                for _ in 0..count {
                    self.set2(0, 0);
                    self.set1(1, 0);
                    self.l1 += 1;
                    self.l2 += 1;
                }
                if length <= 0 {
                    break;
                }
                let code = self.code()?;
                self.set1(1, signed(code.wrapping_add(1)));
                self.k = predict_k(self.k, code as i32, 15);
            }
            self.set2(0, self.k);
            self.l2 += 1;
            self.l1 += 1;
            length -= 1;
        }
        if length == 1 {
            let code = self.code()?;
            self.set1(1, signed(code));
            self.k = predict_k(self.k, code as i32, 15);
            self.set2(0, self.k);
            self.l1 += 1;
        }
        self.set1(1, 0);
        Ok(())
    }

    /// Decode the next line into `out`, `width` coefficients.
    fn decode_line(&mut self, out: &mut [i32]) -> AuraResult<()> {
        if self.line >= self.height {
            return Err(corrupt("crx subband decoded past its last line"));
        }
        let line_length = self.width + 2;
        if self.line == 0 {
            self.s = 0;
            self.k = 0;
            self.l0 = 0;
            self.l1 = line_length;
            self.l2 = 0;
            if self.partial {
                if self.rounded_mask <= 0 {
                    self.top_line()?;
                } else {
                    self.rounded_bits = 1;
                    if self.rounded_mask & !1 != 0 {
                        while self.rounded_mask >> self.rounded_bits != 0 {
                            self.rounded_bits += 1;
                        }
                    }
                    self.top_line_rounded()?;
                }
            } else {
                self.top_line_no_reference()?;
            }
        } else {
            self.l2 = 0;
            if self.line & 1 == 1 {
                self.l1 = 0;
                self.l0 = line_length;
            } else {
                self.l0 = 0;
                self.l1 = line_length;
            }
            if !self.partial {
                self.line_no_reference()?;
            } else if self.rounded_mask <= 0 {
                self.line_l1()?;
            } else {
                self.line_rounded()?;
            }
        }
        let start = if self.line & 1 == 1 {
            1
        } else {
            line_length + 1
        };
        let source = self
            .payload
            .get(start..start + self.width)
            .ok_or_else(|| corrupt("crx line buffer is shorter than its subband"))?;
        for (slot, value) in out.iter_mut().zip(source) {
            *slot = *value;
        }
        self.line += 1;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Headers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
struct BandInfo {
    width: usize,
    height: usize,
    data_offset: usize,
    data_size: usize,
    q_param: i32,
    partial: bool,
    q_step_base: i32,
    q_step_mult: u32,
    row_start: i32,
    row_end: i32,
    col_start: i32,
    col_end: i32,
    level_shift: i32,
}

#[derive(Debug, Clone, Default)]
struct CompInfo {
    data_offset: usize,
    partial: bool,
    rounded_mask: i32,
    bands: Vec<BandInfo>,
}

#[derive(Debug, Clone)]
struct QStep {
    table: Vec<u32>,
    width: usize,
}

#[derive(Debug, Clone, Default)]
struct TileInfo {
    width: usize,
    height: usize,
    flag: u8,
    data_offset: usize,
    qp_size: usize,
    extra_size: usize,
    has_qp: bool,
    comps: Vec<CompInfo>,
    q_steps: Option<Vec<QStep>>,
}

struct Image {
    planes: usize,
    plane_width: usize,
    plane_height: usize,
    levels: usize,
    bands: usize,
    columns: usize,
    rows: usize,
    tiles: Vec<TileInfo>,
}

fn setup_band_index(
    version: u16,
    band: &mut BandInfo,
    level: i32,
    col_start: i32,
    col_end: i32,
    row_start: i32,
    row_end: i32,
) {
    if version == 0x200 {
        band.row_start = row_start;
        band.row_end = row_end;
        band.col_start = col_start;
        band.col_end = col_end;
        band.level_shift = 3 - level;
    } else {
        band.row_start = 0;
        band.row_end = 0;
        band.col_start = 0;
        band.col_end = 0;
        band.level_shift = 0;
    }
}

fn ex_coef(index: usize) -> i32 {
    EX_COEF.get(index).copied().unwrap_or(0)
}

fn size_subbands(version: u16, levels: usize, tile: &TileInfo, comp: &mut CompInfo) {
    let flag = tile.flag;
    let mut band = comp.bands.len().saturating_sub(1);
    let mut height = tile.height as i32;
    let mut width = tile.width as i32;
    let mut width_extra = 0;
    let mut height_extra = 0;
    if levels > 0 {
        let row_table = 0x30 * (levels - 1) + 6 * (tile.width & 7);
        let col_table = 0x30 * (levels - 1) + 6 * (tile.height & 7);
        for level in 0..levels {
            let width_odd = width & 1;
            let height_odd = height & 1;
            width = (width_odd + width) >> 1;
            height = (height_odd + height) >> 1;
            let (mut wx0, mut wx1, mut hx0, mut hx1, mut col_start, mut row_start) =
                (0, 0, 0, 0, 0, 0);
            if flag & RIGHT != 0 {
                wx0 = ex_coef(row_table + 2 * level);
                wx1 = ex_coef(row_table + 2 * level + 1);
            }
            if flag & LEFT != 0 {
                wx0 += 1;
                col_start = 1;
            }
            if flag & BOTTOM != 0 {
                hx0 = ex_coef(col_table + 2 * level);
                hx1 = ex_coef(col_table + 2 * level + 1);
            }
            if flag & TOP != 0 {
                hx0 += 1;
                row_start = 1;
            }
            let level_number = level as i32 + 1;
            if let Some(b) = comp.bands.get_mut(band) {
                b.width = (width + wx0 - width_odd).max(0) as usize;
                b.height = (height + hx0 - height_odd).max(0) as usize;
                setup_band_index(
                    version,
                    b,
                    level_number,
                    col_start,
                    wx0 - col_start,
                    row_start,
                    hx0 - row_start,
                );
            }
            if let Some(b) = band
                .checked_sub(1)
                .and_then(|index| comp.bands.get_mut(index))
            {
                b.width = (width + wx1).max(0) as usize;
                b.height = (height + hx0 - height_odd).max(0) as usize;
                setup_band_index(version, b, level_number, 0, wx1, row_start, hx0 - row_start);
            }
            if let Some(b) = band
                .checked_sub(2)
                .and_then(|index| comp.bands.get_mut(index))
            {
                b.width = (width + wx0 - width_odd).max(0) as usize;
                b.height = (height + hx1).max(0) as usize;
                setup_band_index(version, b, level_number, col_start, wx0 - col_start, 0, hx1);
            }
            band = band.saturating_sub(3);
        }
        if flag & RIGHT != 0 {
            width_extra = ex_coef(row_table + 2 * levels - 1);
        }
        if flag & BOTTOM != 0 {
            height_extra = ex_coef(col_table + 2 * levels - 1);
        }
    }
    if let Some(b) = comp.bands.get_mut(band) {
        b.width = (width_extra + width).max(0) as usize;
        b.height = (height_extra + height).max(0) as usize;
        if levels > 0 {
            setup_band_index(version, b, levels as i32, 0, width_extra, 0, height_extra);
        }
    }
}

#[allow(clippy::too_many_lines)]
fn read_headers(header: &CrxHeader, frame: &[u8]) -> AuraResult<Image> {
    let (plane_width, plane_height, tile_width, tile_height) = if header.planes == 4 {
        (
            header.width as usize / 2,
            header.height as usize / 2,
            header.tile_width as usize / 2,
            header.tile_height as usize / 2,
        )
    } else {
        (
            header.width as usize,
            header.height as usize,
            header.tile_width as usize,
            header.tile_height as usize,
        )
    };
    if tile_width < 0x16 || tile_height < 0x16 || plane_width > 0x7FFF || plane_height > 0x7FFF {
        return Err(corrupt("crx tile or plane size is out of range"));
    }
    let columns = plane_width.div_ceil(tile_width);
    let rows = plane_height.div_ceil(tile_height);
    if columns > 0xFF
        || rows > 0xFF
        || plane_width - tile_width * (columns - 1) < 0x16
        || plane_height - tile_height * (rows - 1) < 0x16
    {
        return Err(corrupt("crx tiling is out of range"));
    }
    let levels = usize::from(header.levels);
    let band_count = 3 * levels + 1;
    let planes = usize::from(header.planes);
    let count = columns * rows;

    let header_bytes = frame
        .get(..header.header_size as usize)
        .ok_or_else(|| corrupt("crx frame is shorter than its header"))?;
    let mut at = 0usize;
    let mut remaining = header_bytes.len() as i64;
    let mut tiles = Vec::with_capacity(count);
    let mut tile_offset = 0usize;

    for index in 0..count {
        let mut tile = TileInfo::default();
        if (index + 1) % columns != 0 {
            tile.width = tile_width;
            if columns > 1 {
                tile.flag = RIGHT;
                if index % columns != 0 {
                    tile.flag |= LEFT;
                }
            }
        } else {
            tile.width = plane_width - tile_width * (columns - 1);
            if columns > 1 {
                tile.flag = LEFT;
            }
        }
        if index < count - columns {
            tile.height = tile_height;
            if rows > 1 {
                tile.flag |= BOTTOM;
                if index >= columns {
                    tile.flag |= TOP;
                }
            }
        } else {
            tile.height = plane_height - tile_height * (rows - 1);
            if rows > 1 {
                tile.flag |= TOP;
            }
        }

        if remaining < 4 {
            return Err(corrupt("crx tile header is truncated"));
        }
        let sign = be16(header_bytes, at);
        let size = be16(header_bytes, at + 2) as usize;
        if !((sign == 0xFF01 && size == 8) || (sign == 0xFF11 && (size == 8 || size == 16))) {
            return Err(corrupt("crx tile header has the wrong signature"));
        }
        if remaining < size as i64 + 4 {
            return Err(corrupt("crx tile header is truncated"));
        }
        let tail = be16(header_bytes, at + 10);
        if (size == 8 && tail != 0) || (size == 16 && tail != 0x4000) {
            return Err(corrupt("crx tile header has the wrong tail"));
        }
        if be16(header_bytes, at + 8) as usize != index {
            return Err(corrupt("crx tile headers are out of order"));
        }
        remaining -= size as i64 + 4;
        let tile_size = be32(header_bytes, at + 4) as usize;
        tile.data_offset = tile_offset;
        if size == 16 {
            if be16(header_bytes, at + 18) != 0 {
                return Err(corrupt("crx extended tile header is not terminated"));
            }
            tile.has_qp = true;
            tile.qp_size = be32(header_bytes, at + 12) as usize;
            tile.extra_size = be16(header_bytes, at + 16) as usize;
        }
        at += size + 4;
        tile_offset += tile_size;

        let mut comp_offset = 0usize;
        for comp_number in 0..planes {
            if remaining < 0xC {
                return Err(corrupt("crx plane header is truncated"));
            }
            let sign = be16(header_bytes, at);
            let size = be16(header_bytes, at + 2);
            if !matches!(sign, 0xFF02 | 0xFF12) || size != 8 {
                return Err(corrupt("crx plane header has the wrong signature"));
            }
            let flags = header_bytes.get(at + 8).copied().unwrap_or(0);
            if usize::from(flags >> 4) != comp_number {
                return Err(corrupt("crx plane headers are out of order"));
            }
            if (be32(header_bytes, at + 8) & 0x00FF_FFFF) != 0 {
                return Err(corrupt("crx plane header is not terminated"));
            }
            let comp_size = be32(header_bytes, at + 4) as usize;
            let rounded_bits = i32::from((flags >> 1) & 3);
            let mut comp = CompInfo {
                data_offset: comp_offset,
                partial: flags & 8 != 0,
                rounded_mask: 0,
                bands: vec![BandInfo::default(); band_count],
            };
            comp_offset += comp_size;
            remaining -= 0xC;
            at += 0xC;
            if rounded_bits != 0 {
                if levels > 0 || !comp.partial {
                    return Err(corrupt("crx rounded coding without progressive data"));
                }
                comp.rounded_mask = 1 << (rounded_bits - 1);
            }

            let mut band_offset = 0usize;
            for (band_number, band) in comp.bands.iter_mut().enumerate() {
                if remaining < 4 {
                    return Err(corrupt("crx subband header is truncated"));
                }
                let sign = be16(header_bytes, at);
                let size = be16(header_bytes, at + 2) as usize;
                if remaining < size as i64 + 4 {
                    return Err(corrupt("crx subband header is truncated"));
                }
                if !((sign == 0xFF03 && size == 8) || (sign == 0xFF13 && size == 16)) {
                    return Err(corrupt("crx subband header has the wrong signature"));
                }
                let band_size = be32(header_bytes, at + 4) as usize;
                let number = usize::from(header_bytes.get(at + 8).copied().unwrap_or(0) >> 4);
                if number != band_number {
                    return Err(corrupt("crx subband headers are out of order"));
                }
                band.data_offset = band_offset;
                if sign == 0xFF03 {
                    let bits = be32(header_bytes, at + 8);
                    band.data_size = band_size.saturating_sub((bits & 0x7FFFF) as usize);
                    band.partial = bits & 0x0800_0000 != 0;
                    band.q_param = ((bits >> 19) & 0xFF) as i32;
                } else {
                    if be16(header_bytes, at + 8) & 0xFFF != 0 || be16(header_bytes, at + 18) != 0 {
                        return Err(corrupt("crx subband header uses an unsupported mode"));
                    }
                    band.partial = false;
                    band.q_param = 0;
                    band.data_size = band_size.saturating_sub(be16(header_bytes, at + 16) as usize);
                    band.q_step_base = be32(header_bytes, at + 12) as i32;
                    band.q_step_mult = be16(header_bytes, at + 10);
                }
                band_offset += band_size;
                at += size + 4;
                remaining -= size as i64 + 4;
            }
            size_subbands(header.version, levels, &tile, &mut comp);
            tile.comps.push(comp);
        }
        tiles.push(tile);
    }

    let mut image = Image {
        planes,
        plane_width,
        plane_height,
        levels,
        bands: band_count,
        columns,
        rows,
        tiles,
    };
    if header.version == 0x200 {
        let payload = frame.get(header.header_size as usize..).unwrap_or(&[]);
        for tile in &mut image.tiles {
            if tile.has_qp {
                let qp = payload
                    .get(tile.data_offset..tile.data_offset + tile.qp_size)
                    .ok_or_else(|| corrupt("crx quantisation data is outside the frame"))?;
                tile.q_steps = Some(quantisation(qp, tile, levels)?);
            }
        }
    }
    Ok(image)
}

fn prediction(left: i32, top: i32, delta_h: i32, delta_v: i32) -> i32 {
    let symbols = [
        left.wrapping_add(delta_h),
        left.wrapping_add(delta_h),
        left,
        top,
    ];
    let index = (usize::from((delta_v < 0) ^ (delta_h < 0)) << 1)
        + usize::from((left < top) ^ (delta_h < 0));
    symbols.get(index).copied().unwrap_or(0)
}

fn step_of(value: i32) -> u32 {
    let tier = value / 6;
    let index = value % 6;
    if tier >= 6 {
        (Q_STEP.get(index.max(0) as usize).copied().unwrap_or(0) as u32)
            .wrapping_shl(((tier - 6) & 0x1F) as u32)
    } else {
        let base = Q_STEP.get(index.max(0) as usize).copied().unwrap_or(0);
        base.checked_shr((6 - tier) as u32).unwrap_or(0) as u32
    }
}

/// Decode a tile's quantisation map and turn it into one step table per level.
#[allow(clippy::too_many_lines)] // Keep ordered format decoding and its validation together.
fn quantisation(qp: &[u8], tile: &TileInfo, levels: usize) -> AuraResult<Vec<QStep>> {
    if !(1..=3).contains(&levels) {
        return Err(corrupt(
            "crx quantisation needs one to three wavelet levels",
        ));
    }
    let width = (tile.width >> 3) + usize::from(tile.width & 7 != 0);
    let height = (tile.height >> 1) + (tile.height & 1);
    let mut stream = Stream::new(qp);
    let mut table = Vec::with_capacity(width * height);
    let mut lines = vec![0i32; 2 * (width + 2)];
    let mut k = 0i32;
    for row in 0..height {
        let (first, second) = lines.split_at_mut(width + 2);
        let (above, current) = if row & 1 == 1 {
            (second, first)
        } else {
            (first, second)
        };
        let read = |stream: &mut Stream<'_>, k: i32| golomb(stream, k, 23, 8);
        if row == 0 {
            if let Some(slot) = current.first_mut() {
                *slot = 0;
            }
            for i in 0..width {
                let previous = current.get(i).copied().unwrap_or(0);
                let code = read(&mut stream, k)?;
                if let Some(slot) = current.get_mut(i + 1) {
                    *slot = previous.wrapping_add(signed(code));
                }
                k = predict_k(k, code as i32, 7);
            }
        } else {
            let a = |i: usize| above.get(i).copied().unwrap_or(0);
            if let Some(slot) = current.first_mut() {
                *slot = a(1);
            }
            let mut delta_h = a(1).wrapping_sub(a(0));
            for i in 0..width {
                let left = current.get(i).copied().unwrap_or(0);
                let predicted = prediction(left, a(i + 1), delta_h, a(i).wrapping_sub(left));
                let code = read(&mut stream, k)?;
                if let Some(slot) = current.get_mut(i + 1) {
                    *slot = predicted.wrapping_add(signed(code));
                }
                if i + 1 < width {
                    delta_h = a(i + 2).wrapping_sub(a(i + 1));
                    let blended = code.wrapping_add(2u32.wrapping_mul(abs(delta_h) as u32)) >> 1;
                    k = predict_k(k, blended as i32, 7);
                } else {
                    k = predict_k(k, code as i32, 7);
                }
            }
        }
        let last = current.get(width).copied().unwrap_or(0);
        if let Some(slot) = current.get_mut(width + 1) {
            *slot = last.wrapping_add(1);
        }
        for i in 0..width {
            table.push(current.get(i + 1).copied().unwrap_or(0).wrapping_add(4));
        }
    }

    let at = |row: usize, column: usize| table.get(row * width + column).copied().unwrap_or(0);
    let quarter = (tile.height >> 2) + usize::from(tile.height & 3 != 0);
    let eighth = (tile.height >> 3) + usize::from(tile.height & 7 != 0);
    let last_row = height.saturating_sub(1);
    let mut steps = Vec::with_capacity(levels);
    if levels == 3 {
        let mut level = Vec::with_capacity(eighth * width);
        for row in 0..eighth {
            for column in 0..width {
                let sum: i32 = (0..4)
                    .map(|offset| at((4 * row + offset).min(last_row), column))
                    .fold(0i32, i32::wrapping_add);
                let value = (i32::from(sum < 0) * 3 + sum) >> 2;
                level.push(step_of(value));
            }
        }
        steps.push(QStep {
            table: level,
            width,
        });
    }
    if levels >= 2 {
        let mut level = Vec::with_capacity(quarter * width);
        for row in 0..quarter {
            for column in 0..width {
                let value = at((2 * row).min(last_row), column)
                    .wrapping_add(at((2 * row + 1).min(last_row), column))
                    / 2;
                level.push(step_of(value));
            }
        }
        steps.push(QStep {
            table: level,
            width,
        });
    }
    steps.push(QStep {
        table: table.iter().map(|value| step_of(*value)).collect(),
        width,
    });
    Ok(steps)
}

// ---------------------------------------------------------------------------
// Decoding a plane
// ---------------------------------------------------------------------------

struct Band<'a> {
    info: BandInfo,
    param: Option<BandParam<'a>>,
    q_param: i32,
    k_param: i32,
}

struct Wavelet {
    width: usize,
    height: i32,
    lines: Vec<i32>,
    slots: [usize; 8],
    current_line: i32,
    current_h: i32,
    tap: i32,
    /// Physical line of the level below that is this level's low band; `None`
    /// at level 0, whose low band is subband 0.
    low: Option<usize>,
}

impl Wavelet {
    fn take_line(&mut self) -> usize {
        let slot = (self.tap - self.current_h).rem_euclid(5) as usize + 3;
        self.current_h -= 1;
        slot
    }
}

struct Comp<'a> {
    bands: Vec<Band<'a>>,
    /// Every band's coefficient line, contiguous, as the reference lays them out.
    buffer: Vec<i32>,
    starts: Vec<usize>,
    wavelets: Vec<Wavelet>,
    flag: u8,
}

/// Read access to one line of coefficients: a band's buffer or a lower level's line.
#[derive(Clone, Copy)]
struct Line<'a> {
    payload: &'a [i32],
    start: usize,
}

impl Line<'_> {
    #[inline]
    fn at(&self, index: usize) -> i32 {
        self.payload.get(self.start + index).copied().unwrap_or(0)
    }
}

fn band_line<'a>(buffer: &'a [i32], starts: &[usize], band: usize) -> Line<'a> {
    Line {
        payload: buffer,
        start: starts.get(band).copied().unwrap_or(0),
    }
}

/// One output row of a 5/3 horizontal inverse from a low and a high band.
fn horizontal(
    out: &mut [i32],
    out_start: usize,
    low: Line<'_>,
    high: Line<'_>,
    width: usize,
    flag: u8,
    always_write_tail: bool,
) {
    let mut put = |index: usize, value: i32| {
        if let Some(slot) = out.get_mut(out_start + index) {
            *slot = value;
        }
    };
    let get_out = |out: &[i32], index: usize| out.get(out_start + index).copied().unwrap_or(0);
    if width <= 1 {
        put(0, low.at(0));
        return;
    }
    let (mut l, mut h) = (0usize, 0usize);
    if flag & LEFT != 0 {
        put(0, low.at(0) - ((high.at(0) + high.at(1) + 2) >> 2));
        h += 1;
    } else {
        put(0, low.at(0) - ((high.at(0) + 1) >> 1));
    }
    l += 1;
    let mut o = 0usize;
    let mut i = 0usize;
    while i + 3 < width {
        let delta = low.at(l) - ((high.at(h) + high.at(h + 1) + 2) >> 2);
        let previous = get_out(out, o);
        if let Some(slot) = out.get_mut(out_start + o + 1) {
            *slot = high.at(h) + ((previous + delta) >> 1);
        }
        if let Some(slot) = out.get_mut(out_start + o + 2) {
            *slot = delta;
        }
        l += 1;
        h += 1;
        o += 2;
        i += 2;
    }
    let previous = get_out(out, o);
    if flag & RIGHT != 0 {
        let delta = low.at(l) - ((high.at(h) + high.at(h + 1) + 2) >> 2);
        if let Some(slot) = out.get_mut(out_start + o + 1) {
            *slot = high.at(h) + ((previous + delta) >> 1);
        }
        if width & 1 == 1 || always_write_tail {
            if let Some(slot) = out.get_mut(out_start + o + 2) {
                *slot = delta;
            }
        }
    } else if width & 1 == 1 {
        let delta = low.at(l) - ((high.at(h) + 1) >> 1);
        if let Some(slot) = out.get_mut(out_start + o + 1) {
            *slot = high.at(h) + ((previous + delta) >> 1);
        }
        if let Some(slot) = out.get_mut(out_start + o + 2) {
            *slot = delta;
        }
    } else if let Some(slot) = out.get_mut(out_start + o + 1) {
        *slot = high.at(h) + previous;
    }
}

impl Comp<'_> {
    fn decode_band(&mut self, index: usize, q_step: Option<&QStep>) -> AuraResult<()> {
        let start = self.starts.get(index).copied().unwrap_or(0);
        let Some(band) = self.bands.get_mut(index) else {
            return Err(corrupt("crx names a subband that does not exist"));
        };
        let width = band.info.width;
        let Some(out) = self.buffer.get_mut(start..start + width) else {
            return Err(corrupt("crx subband buffer is out of range"));
        };
        let Some(param) = band.param.as_mut() else {
            out.fill(0);
            return Ok(());
        };
        if band.info.partial && q_step.is_none() {
            let code = golomb(&mut param.stream, band.k_param, 23, 8)?;
            band.q_param = band.q_param.wrapping_add(signed(code));
            band.k_param = predict_k(band.k_param, code as i32, 0);
            if band.k_param > 7 {
                return Err(corrupt("crx quantisation parameter is out of range"));
            }
        }
        param.decode_line(out)?;
        if width == 0 {
            return Ok(());
        }
        let info = &band.info;
        if let Some(step) = q_step {
            let line = param.line as i32 - 1;
            let row = if line < info.row_start {
                0
            } else if line < info.height as i32 - info.row_end {
                line - info.row_end
            } else {
                info.height as i32 - info.row_end - info.row_start - 1
            };
            let base = step.width * row.max(0) as usize;
            let entry = |column: usize| step.table.get(base + column).copied().unwrap_or(0);
            let scale = |value: u32| {
                let quant = info
                    .q_step_base
                    .wrapping_add((value.wrapping_mul(info.q_step_mult) >> 3) as i32);
                quant.clamp(1, 0x0016_8000)
            };
            let col_start = info.col_start.max(0) as usize;
            let col_end = info.col_end.max(0) as usize;
            let shift = info.level_shift.max(0) as u32;
            let inner_end = width.saturating_sub(col_end);
            for (i, value) in out.iter_mut().enumerate() {
                let column = if i < col_start {
                    0
                } else if i < inner_end {
                    (i - col_start) >> shift
                } else {
                    (width.saturating_sub(col_end + col_start + 1)) >> shift
                };
                *value = value.wrapping_mul(scale(entry(column)));
            }
        } else {
            let q = band.q_param;
            let scale = if q / 6 >= 6 {
                Q_STEP
                    .get((q % 6) as usize)
                    .copied()
                    .unwrap_or(0)
                    .wrapping_mul(1i32.wrapping_shl((q / 6 + 26) as u32))
            } else {
                Q_STEP
                    .get((q % 6).max(0) as usize)
                    .copied()
                    .unwrap_or(0)
                    .checked_shr((6 - q / 6) as u32)
                    .unwrap_or(0)
            };
            if scale != 1 {
                for value in out.iter_mut() {
                    *value = value.wrapping_mul(scale);
                }
            }
        }
        Ok(())
    }

    fn filter_decode(&mut self, level: usize, steps: Option<&[QStep]>) -> AuraResult<()> {
        let Some(wavelet) = self.wavelets.get(level) else {
            return Err(corrupt("crx wavelet level is out of range"));
        };
        if wavelet.current_h != 0 {
            return Ok(());
        }
        let step = steps.and_then(|all| all.get(level));
        let band = 3 * level;
        if wavelet.height - 3 <= wavelet.current_line && self.flag & BOTTOM == 0 {
            if wavelet.height & 1 == 1 {
                if level > 0 {
                    self.filter_decode(level - 1, steps)?;
                } else {
                    self.decode_band(band, step)?;
                }
                self.decode_band(band + 1, step)?;
            }
        } else {
            if level > 0 {
                self.filter_decode(level - 1, steps)?;
            } else {
                self.decode_band(band, step)?;
            }
            self.decode_band(band + 1, step)?;
            self.decode_band(band + 2, step)?;
            self.decode_band(band + 3, step)?;
        }
        Ok(())
    }

    /// The low band of `level`, taking a fresh line from the level below if needed.
    fn pull_low(&mut self, level: usize) -> AuraResult<()> {
        if level == 0 {
            return Ok(());
        }
        let below_idle = self
            .wavelets
            .get(level - 1)
            .is_some_and(|w| w.current_h == 0);
        if below_idle {
            self.transform(level - 1)?;
        }
        let slot = self
            .wavelets
            .get_mut(level - 1)
            .map(Wavelet::take_line)
            .ok_or_else(|| corrupt("crx wavelet level is out of range"))?;
        if let Some(wavelet) = self.wavelets.get_mut(level) {
            wavelet.low = Some(slot);
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn transform(&mut self, level: usize) -> AuraResult<()> {
        let Some(current) = self.wavelets.get(level) else {
            return Err(corrupt("crx wavelet level is out of range"));
        };
        if current.current_h != 0 {
            return Ok(());
        }
        let at_end = current.current_line >= current.height - 3;
        let odd = current.height & 1 == 1;
        if at_end && self.flag & BOTTOM != 0 {
            return Ok(());
        }
        if !at_end || odd {
            self.pull_low(level)?;
        }
        let flag = self.flag;
        let (below, rest) = self.wavelets.split_at_mut(level);
        let Some(wavelet) = rest.first_mut() else {
            return Err(corrupt("crx wavelet level is out of range"));
        };
        let width = wavelet.width;
        let low_line = match wavelet.low {
            Some(slot) => {
                let Some(lower) = below.last() else {
                    return Err(corrupt("crx wavelet level has no level below"));
                };
                Line {
                    payload: &lower.lines,
                    start: slot * lower.width,
                }
            }
            None => band_line(&self.buffer, &self.starts, 0),
        };
        let b1 = band_line(&self.buffer, &self.starts, 3 * level + 1);
        let b2 = band_line(&self.buffer, &self.starts, 3 * level + 2);
        let b3 = band_line(&self.buffer, &self.starts, 3 * level + 3);
        let tap = wavelet.tap as usize;
        let h0 = tap + 3;
        let h1 = (tap + 1) % 5 + 3;
        let h2 = (tap + 2) % 5 + 3;
        let row = |slot: usize| slot * width;

        if at_end {
            if odd {
                let l0 = wavelet.slots[0];
                wavelet.slots.swap(1, 2);
                horizontal(
                    &mut wavelet.lines,
                    row(l0),
                    low_line,
                    b1,
                    width,
                    flag,
                    false,
                );
                let (p0, p1) = (wavelet.slots[0], wavelet.slots[1]);
                for i in 0..width {
                    let v0 = wavelet.lines.get(row(p0) + i).copied().unwrap_or(0);
                    let v1 = wavelet.lines.get(row(p1) + i).copied().unwrap_or(0);
                    let delta = v0 - ((v1 + 1) >> 1);
                    let hv = wavelet.lines.get(row(h0) + i).copied().unwrap_or(0);
                    if let Some(slot) = wavelet.lines.get_mut(row(h1) + i) {
                        *slot = v1 + ((delta + hv) >> 1);
                    }
                    if let Some(slot) = wavelet.lines.get_mut(row(h2) + i) {
                        *slot = delta;
                    }
                }
                wavelet.current_h += 3;
                wavelet.current_line += 3;
                wavelet.tap = (wavelet.tap + 3) % 5;
            } else {
                let l2 = wavelet.slots[2];
                // The reference aliases both line slots onto the same buffer here.
                wavelet.slots[1] = l2;
                wavelet.slots[2] = wavelet.slots[1];
                for i in 0..width {
                    let v = wavelet.lines.get(row(h0) + i).copied().unwrap_or(0)
                        + wavelet.lines.get(row(l2) + i).copied().unwrap_or(0);
                    if let Some(slot) = wavelet.lines.get_mut(row(h1) + i) {
                        *slot = v;
                    }
                }
                wavelet.current_h += 2;
                wavelet.current_line += 2;
                wavelet.tap = (wavelet.tap + 2) % 5;
            }
            return Ok(());
        }

        let l0 = wavelet.slots[0];
        let l1 = wavelet.slots[1];
        wavelet.slots.swap(1, 2);
        horizontal(
            &mut wavelet.lines,
            row(l0),
            low_line,
            b1,
            width,
            flag,
            false,
        );
        horizontal(&mut wavelet.lines, row(l1), b2, b3, width, flag, false);
        let (p0, p1, p2) = (wavelet.slots[0], wavelet.slots[1], wavelet.slots[2]);
        for i in 0..width {
            let v0 = wavelet.lines.get(row(p0) + i).copied().unwrap_or(0);
            let v1 = wavelet.lines.get(row(p1) + i).copied().unwrap_or(0);
            let v2 = wavelet.lines.get(row(p2) + i).copied().unwrap_or(0);
            let delta = v0 - ((v2 + v1 + 2) >> 2);
            let hv = wavelet.lines.get(row(h0) + i).copied().unwrap_or(0);
            if let Some(slot) = wavelet.lines.get_mut(row(h1) + i) {
                *slot = v1 + ((delta + hv) >> 1);
            }
            if let Some(slot) = wavelet.lines.get_mut(row(h2) + i) {
                *slot = delta;
            }
        }
        if wavelet.current_line >= wavelet.height - 3 && odd {
            wavelet.current_h += 3;
            wavelet.current_line += 3;
            wavelet.tap = (wavelet.tap + 3) % 5;
        } else {
            wavelet.current_h += 2;
            wavelet.current_line += 2;
            wavelet.tap = (wavelet.tap + 2) % 5;
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn initialise(&mut self, levels: usize, steps: Option<&[QStep]>) -> AuraResult<()> {
        for level in 0..levels {
            let step = steps.and_then(|all| all.get(level));
            let band = 3 * level;
            if level > 0 {
                let slot = self
                    .wavelets
                    .get_mut(level - 1)
                    .map(Wavelet::take_line)
                    .ok_or_else(|| corrupt("crx wavelet level is out of range"))?;
                if let Some(wavelet) = self.wavelets.get_mut(level) {
                    wavelet.low = Some(slot);
                }
            } else {
                self.decode_band(band, step)?;
            }
            let tall = self.wavelets.get(level).is_some_and(|w| w.height > 1);
            if tall {
                self.decode_band(band + 1, step)?;
                self.decode_band(band + 2, step)?;
                self.decode_band(band + 3, step)?;
                let top = self.flag & TOP != 0;
                if top {
                    self.horizontal_pair(level, 1)?;
                    self.decode_band(band + 3, step)?;
                    self.decode_band(band + 2, step)?;
                }
                {
                    let flag = self.flag;
                    let (_, rest) = self.wavelets.split_at_mut(level);
                    let Some(wavelet) = rest.first_mut() else {
                        return Err(corrupt("crx wavelet level is out of range"));
                    };
                    let width = wavelet.width;
                    let h0 = (wavelet.tap as usize + 3) * width;
                    let (l0, l1, l2) = (
                        wavelet.slots[0] * width,
                        wavelet.slots[1] * width,
                        wavelet.slots[2] * width,
                    );
                    if top {
                        let b2 = band_line(&self.buffer, &self.starts, band + 2);
                        let b3 = band_line(&self.buffer, &self.starts, band + 3);
                        horizontal(&mut wavelet.lines, l2, b2, b3, width, flag, false);
                        for i in 0..width {
                            let v0 = wavelet.lines.get(l0 + i).copied().unwrap_or(0);
                            let v1 = wavelet.lines.get(l1 + i).copied().unwrap_or(0);
                            let v2 = wavelet.lines.get(l2 + i).copied().unwrap_or(0);
                            if let Some(slot) = wavelet.lines.get_mut(h0 + i) {
                                *slot = v0 - ((v1 + v2 + 2) >> 2);
                            }
                        }
                    }
                }
                if !top {
                    self.horizontal_pair(level, 2)?;
                    let (_, rest) = self.wavelets.split_at_mut(level);
                    let Some(wavelet) = rest.first_mut() else {
                        return Err(corrupt("crx wavelet level is out of range"));
                    };
                    let width = wavelet.width;
                    let h0 = (wavelet.tap as usize + 3) * width;
                    let (l0, l2) = (wavelet.slots[0] * width, wavelet.slots[2] * width);
                    for i in 0..width {
                        let v0 = wavelet.lines.get(l0 + i).copied().unwrap_or(0);
                        let v2 = wavelet.lines.get(l2 + i).copied().unwrap_or(0);
                        if let Some(slot) = wavelet.lines.get_mut(h0 + i) {
                            *slot = v0 - ((v2 + 1) >> 1);
                        }
                    }
                }
                self.filter_decode(level, steps)?;
                self.transform(level)?;
            } else {
                self.decode_band(band + 1, step)?;
                let flag = self.flag;
                let (below, rest) = self.wavelets.split_at_mut(level);
                let Some(wavelet) = rest.first_mut() else {
                    return Err(corrupt("crx wavelet level is out of range"));
                };
                let width = wavelet.width;
                let low = match wavelet.low {
                    Some(slot) => match below.last() {
                        Some(lower) => Line {
                            payload: &lower.lines,
                            start: slot * lower.width,
                        },
                        None => return Err(corrupt("crx wavelet level has no level below")),
                    },
                    None => band_line(&self.buffer, &self.starts, 0),
                };
                let b1 = band_line(&self.buffer, &self.starts, band + 1);
                let h0 = (wavelet.tap as usize + 3) * width;
                horizontal(&mut wavelet.lines, h0, low, b1, width, flag, true);
                wavelet.current_line += 1;
                wavelet.current_h += 1;
                wavelet.tap = (wavelet.tap + 1) % 5;
            }
        }
        Ok(())
    }

    /// The reference's `crxHorizontal53`: the low pair into line slot 0 and the
    /// high pair into the named line slot.
    fn horizontal_pair(&mut self, level: usize, second_slot: usize) -> AuraResult<()> {
        let flag = self.flag;
        let (below, rest) = self.wavelets.split_at_mut(level);
        let Some(wavelet) = rest.first_mut() else {
            return Err(corrupt("crx wavelet level is out of range"));
        };
        let width = wavelet.width;
        let low = match wavelet.low {
            Some(slot) => match below.last() {
                Some(lower) => Line {
                    payload: &lower.lines,
                    start: slot * lower.width,
                },
                None => return Err(corrupt("crx wavelet level has no level below")),
            },
            None => band_line(&self.buffer, &self.starts, 0),
        };
        let band = 3 * level;
        let b1 = band_line(&self.buffer, &self.starts, band + 1);
        let b2 = band_line(&self.buffer, &self.starts, band + 2);
        let b3 = band_line(&self.buffer, &self.starts, band + 3);
        let first = wavelet.slots[0] * width;
        let second = *wavelet
            .slots
            .get(second_slot)
            .ok_or_else(|| corrupt("crx invalid second wavelet slot"))?
            * width;
        if width <= 1 {
            if let Some(slot) = wavelet.lines.get_mut(first) {
                *slot = low.at(0);
            }
            if let Some(slot) = wavelet.lines.get_mut(second) {
                *slot = b2.at(0);
            }
            return Ok(());
        }
        horizontal(&mut wavelet.lines, first, low, b1, width, flag, false);
        horizontal(&mut wavelet.lines, second, b2, b3, width, flag, false);
        Ok(())
    }

    fn line(&self, level: usize, slot: usize, width: usize) -> &[i32] {
        self.wavelets
            .get(level)
            .and_then(|w| w.lines.get(slot * w.width..slot * w.width + width))
            .unwrap_or(&[])
    }
}

/// How a decoded coefficient becomes a stored sample.
#[derive(Clone, Copy)]
struct Output {
    encoding: u8,
    bits: u8,
    planes: usize,
}

impl Output {
    #[inline]
    fn sample(self, value: i32) -> u16 {
        match self.encoding {
            1 => {
                let max = (1i32 << (self.bits - 1)) - 1;
                let min = -(1i32 << (self.bits - 1));
                value.clamp(min, max) as i16 as u16
            }
            // Kept as a 16-bit signed intermediate until all four planes exist.
            3 => value as i16 as u16,
            _ => {
                let median = 1i32 << (self.bits - 1);
                let max = (1i32 << self.bits) - 1;
                (median.wrapping_add(value)).clamp(0, max) as u16
            }
        }
    }
}

/// Decode one tile of one plane into its rectangle of `plane`.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_lines)] // Keep ordered format decoding and its validation together.
fn decode_tile(
    image: &Image,
    payload: &[u8],
    tile: &TileInfo,
    plane_index: usize,
    row0: usize,
    col0: usize,
    output: Output,
    plane: &mut [u16],
) -> AuraResult<bool> {
    let Some(comp_info) = tile.comps.get(plane_index) else {
        return Err(corrupt("crx tile is missing a plane"));
    };
    let tile_offset = tile.data_offset + tile.qp_size + tile.extra_size + comp_info.data_offset;
    let mut bands = Vec::with_capacity(image.bands);
    let mut starts = Vec::with_capacity(image.bands);
    let mut total = 0usize;
    for (index, info) in comp_info.bands.iter().enumerate() {
        starts.push(total);
        total += info.width;
        let param = if info.data_size > 0 {
            let start = tile_offset + info.data_offset;
            let bytes = payload.get(start..start + info.data_size).unwrap_or(&[]);
            let (partial, mask) = if comp_info.partial && index == 0 {
                (true, comp_info.rounded_mask)
            } else {
                (false, 0)
            };
            Some(BandParam::new(
                bytes,
                info.width,
                info.height,
                partial,
                mask,
            ))
        } else {
            None
        };
        bands.push(Band {
            info: info.clone(),
            param,
            q_param: info.q_param,
            k_param: 0,
        });
    }
    let mut wavelets = Vec::with_capacity(image.levels);
    for level in 0..image.levels {
        let band = 3 * level + 1;
        let (height, width) = if level + 1 >= image.levels {
            (tile.height as i32, tile.width)
        } else {
            (
                comp_info.bands.get(band + 3).map_or(0, |b| b.height) as i32,
                comp_info.bands.get(band + 4).map_or(0, |b| b.width),
            )
        };
        wavelets.push(Wavelet {
            width,
            height,
            lines: vec![0; 8 * width],
            slots: [0, 1, 2, 3, 4, 5, 6, 7],
            current_line: 0,
            current_h: 0,
            tap: 0,
            low: None,
        });
    }
    let mut comp = Comp {
        bands,
        buffer: vec![0; total],
        starts,
        wavelets,
        flag: tile.flag,
    };

    let mut store = |row: usize, line: &[i32]| {
        let base = (row0 + row) * image.plane_width + col0;
        if let Some(out) = plane.get_mut(base..base + tile.width.min(line.len())) {
            for (slot, value) in out.iter_mut().zip(line) {
                *slot = output.sample(*value);
            }
        }
    };

    if image.levels > 0 {
        let steps = tile.q_steps.as_deref();
        comp.initialise(image.levels, steps)?;
        let top = image.levels - 1;
        for row in 0..tile.height {
            comp.filter_decode(top, steps)?;
            comp.transform(top)?;
            let slot = comp
                .wavelets
                .get_mut(top)
                .map(Wavelet::take_line)
                .ok_or_else(|| corrupt("crx wavelet level is out of range"))?;
            let line = comp.line(top, slot, tile.width).to_vec();
            store(row, &line);
        }
        Ok(true)
    } else {
        let Some(band) = comp.bands.first_mut() else {
            return Err(corrupt("crx plane has no subband"));
        };
        let Some(param) = band.param.as_mut() else {
            // The reference stops decoding this plane at an empty tile.
            return Ok(false);
        };
        let mut line = vec![0i32; band.info.width];
        for row in 0..tile.height {
            param.decode_line(&mut line)?;
            store(row, &line);
        }
        Ok(true)
    }
}

/// Decode a CRX frame - `frame` is the whole sample from the track, headers
/// included - into the full sensor mosaic, `width * height` samples.
///
/// # Errors
///
/// `AURA-RAW-2002` when a header is inconsistent or the entropy data runs out.
#[allow(clippy::too_many_lines)] // Keep ordered format decoding and its validation together.
pub fn decode(frame: &[u8], header: &CrxHeader) -> AuraResult<Vec<u16>> {
    let image = read_headers(header, frame)?;
    let payload = frame.get(header.header_size as usize..).unwrap_or(&[]);
    let output = Output {
        encoding: header.encoding,
        bits: header.bits,
        planes: image.planes,
    };

    // Each (plane, tile) is independent: decode them all in parallel into one
    // buffer per plane, then lay the planes out as the Bayer mosaic.
    let plane_len = image.plane_width * image.plane_height;
    let mut jobs = Vec::new();
    let mut row0 = 0usize;
    for tile_row in 0..image.rows {
        let mut col0 = 0usize;
        for tile_col in 0..image.columns {
            let index = tile_row * image.columns + tile_col;
            if let Some(tile) = image.tiles.get(index) {
                jobs.push((index, row0, col0));
                col0 += tile.width;
            }
        }
        row0 += image
            .tiles
            .get(tile_row * image.columns)
            .map_or(0, |tile| tile.height);
    }

    let planes: Vec<AuraResult<Vec<u16>>> = (0..image.planes)
        .into_par_iter()
        .map(|plane_index| {
            let mut plane = vec![0u16; plane_len];
            // Tiles are decoded in order so an empty tile stops the plane exactly
            // where the reference stops it.
            for (index, row0, col0) in &jobs {
                let Some(tile) = image.tiles.get(*index) else {
                    continue;
                };
                if !decode_tile(
                    &image,
                    payload,
                    tile,
                    plane_index,
                    *row0,
                    *col0,
                    output,
                    &mut plane,
                )? {
                    break;
                }
            }
            Ok(plane)
        })
        .collect();
    let planes: Vec<Vec<u16>> = planes.into_iter().collect::<AuraResult<_>>()?;

    if output.planes == 1 {
        return planes
            .into_iter()
            .next()
            .ok_or_else(|| corrupt("crx frame has no plane"));
    }
    let width = image.plane_width * 2;
    let mut mosaic = vec![0u16; width * image.plane_height * 2];
    // Where plane k (R, G1, G2, B) sits in each 2x2 cell, by CFA layout.
    let positions: [[(usize, usize); 4]; 4] = [
        [(0, 0), (1, 0), (0, 1), (1, 1)],
        [(1, 0), (0, 0), (1, 1), (0, 1)],
        [(0, 1), (1, 1), (0, 0), (1, 0)],
        [(1, 1), (0, 1), (1, 0), (0, 0)],
    ];
    let place = positions
        .get(usize::from(header.cfa_layout))
        .copied()
        .unwrap_or(positions[0]);
    let median_bits = header.median_bits;
    mosaic
        .par_chunks_mut(width * 2)
        .enumerate()
        .for_each(|(row, pair)| {
            let read = |plane: usize, column: usize| {
                planes
                    .get(plane)
                    .and_then(|p| p.get(row * image.plane_width + column))
                    .copied()
                    .unwrap_or(0)
            };
            for column in 0..image.plane_width {
                let values = if header.encoding == 3 {
                    colour_transform(
                        [
                            read(0, column) as i16,
                            read(1, column) as i16,
                            read(2, column) as i16,
                            read(3, column) as i16,
                        ],
                        median_bits,
                    )
                } else {
                    [
                        read(0, column),
                        read(1, column),
                        read(2, column),
                        read(3, column),
                    ]
                };
                for (plane, (dx, dy)) in place.iter().enumerate() {
                    if let Some(slot) = pair.get_mut(dy * width + 2 * column + dx) {
                        *slot = values.get(plane).copied().unwrap_or(0);
                    }
                }
            }
        });
    Ok(mosaic)
}

/// Encoding 3 codes luma and three colour differences; this is the inverse the
/// reference documents, in its own fixed-point arithmetic.
fn colour_transform(planes: [i16; 4], median_bits: u8) -> [u16; 4] {
    let [p0, p1, p2, p3] = planes.map(i32::from);
    let median = (1i32 << (median_bits.max(1) - 1)) << 10;
    let max = (1i32 << median_bits) - 1;
    let mut green = median + (p0 << 10) - 168 * p1 - 585 * p3;
    green = if green < 0 {
        -(((abs(green) + 512) >> 9) & !1)
    } else {
        ((abs(green) + 512) >> 9) & !1
    };
    let red = (median + (p0 << 10) + 1510 * p3 + 512) >> 10;
    let green1 = (p2 + green + 1) >> 1;
    let green2 = (green - p2 + 1) >> 1;
    let blue = (median + (p0 << 10) + 1927 * p1 + 512) >> 10;
    [red, green1, green2, blue].map(|value| value.clamp(0, max) as u16)
}
