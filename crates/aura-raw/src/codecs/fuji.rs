//! Fujifilm's compressed RAF: the default on every X and GFX body since 2016.
//!
//! The sensor is cut into vertical strips 768 photosites wide, each coded
//! independently. Inside a strip, six sensor rows at a time are split by colour
//! into eighteen line buffers (five red, eight green, five blue), and each
//! sample is predicted from its neighbours above and coded as an adaptive
//! Golomb-like residual whose parameter is learned per gradient bucket. The
//! lossy variant adds a per-line quantiser and three coarser tables for flat
//! areas. X-Trans and Bayer bodies differ only in which samples are coded and
//! which are interpolated.
//!
//! The format is unpublished; this is an independent safe-Rust implementation
//! of the description in Alexey Danilchenko's decoder in LibRaw
//! (`fuji_compressed.cpp`), checked against it sample for sample. Strips are
//! independent, so they decode in parallel.

// The four quantiser tables, three gradient sets and 41 gradient buckets are
// fixed-size arrays indexed by values the format bounds (a table chosen by a
// loop over 1..4, a gradient of at most 9 * 4 + 4); the line buffers, which a
// hostile file could steer, are only ever reached through `Block::at`/`set`.
#![allow(clippy::indexing_slicing)]

use aura_core::errors::raw::corrupt;
use aura_core::AuraResult;
use rayon::prelude::*;

/// The 16-byte header at the start of the sensor data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FujiHeader {
    /// True for lossless compression, false for the lossy variant.
    pub lossless: bool,
    /// True for X-Trans (raw type 16), false for Bayer (raw type 0).
    pub xtrans: bool,
    /// Bits per sample: 12, 14 or 16.
    pub bits: u8,
    /// Sensor height.
    pub height: u32,
    /// Sensor width.
    pub width: u32,
    /// Strip width in photosites (always 768).
    pub block_width: u32,
    /// Strips across the sensor.
    pub blocks: u32,
    /// Six-row groups down the sensor.
    pub lines: u32,
}

impl FujiHeader {
    /// Read and validate the header at the start of `data`.
    #[must_use]
    pub fn parse(payload: &[u8]) -> Option<Self> {
        let be16 = |at: usize| {
            u32::from(payload.get(at).copied().unwrap_or(0)) << 8
                | u32::from(payload.get(at + 1).copied().unwrap_or(0))
        };
        let byte = |at: usize| payload.get(at).copied().unwrap_or(0);
        let signature = be16(0);
        let lossless = byte(2);
        let raw_type = byte(3);
        let bits = byte(4);
        let height = be16(5);
        let rounded_width = be16(7);
        let width = be16(9);
        let block = be16(11);
        let blocks = u32::from(byte(13));
        let lines = be16(14);
        let valid = signature == 0x4953
            && lossless <= 1
            && (6..=0x4002).contains(&height)
            && height % 6 == 0
            && (0x300..=0x4200).contains(&width)
            && width % 24 == 0
            && rounded_width <= 0x4200
            && block == 0x300
            && rounded_width >= block
            && rounded_width % block == 0
            && rounded_width - width < block
            && blocks > 0
            && blocks <= 0x10
            && blocks == rounded_width / block
            && lines > 0
            && lines <= 0xAAB
            && lines == height / 6
            && matches!(bits, 12 | 14 | 16)
            && matches!(raw_type, 0 | 16);
        valid.then_some(Self {
            lossless: lossless == 1,
            xtrans: raw_type == 16,
            bits,
            height,
            width,
            block_width: block,
            blocks,
            lines,
        })
    }
}

// Line buffer indices.
const R0: usize = 0;
const R2: usize = 2;
const R3: usize = 3;
const R4: usize = 4;
const G0: usize = 5;
const G2: usize = 7;
const G3: usize = 8;
const G4: usize = 9;
const G5: usize = 10;
const G6: usize = 11;
const G7: usize = 12;
const B0: usize = 13;
const B2: usize = 15;
const B3: usize = 16;
const B4: usize = 17;
const LINES: usize = 18;

#[derive(Debug, Clone, Default)]
struct QTable {
    table: Vec<i8>,
    q_base: i32,
    max_grad: i32,
    q_grad_mult: i32,
    total_values: i32,
    raw_bits: i32,
}

#[derive(Debug, Clone)]
struct Params {
    qt: [QTable; 4],
    max_bits: i32,
    min_value: i32,
    max_value: i32,
    line_width: usize,
}

fn log2ceil(value: i32) -> i32 {
    let mut result = 0;
    let mut v = value;
    if v != 0 {
        v -= 1;
        loop {
            result += 1;
            v >>= 1;
            if v == 0 {
                break;
            }
        }
    }
    result
}

fn setup_qlut(table: &mut [i8], q: [i32; 5]) {
    for (slot, value) in table.iter_mut().zip(-q[4]..=q[4]) {
        *slot = if value <= -q[3] {
            -4
        } else if value <= -q[2] {
            -3
        } else if value <= -q[1] {
            -2
        } else if value < -q[0] {
            -1
        } else if value <= q[0] {
            0
        } else if value < q[1] {
            1
        } else if value < q[2] {
            2
        } else if value < q[3] {
            3
        } else {
            4
        };
    }
}

fn init_main_qtable(params: &mut Params, q_base: i32) {
    let max_value = params.max_value + 1;
    let mut q = [
        q_base,
        3 * q_base + 0x12,
        5 * q_base + 0x43,
        7 * q_base + 0x114,
        params.max_value,
    ];
    if q[1] >= max_value || q[1] < q_base + 1 {
        q[1] = q_base + 1;
    }
    if q[2] < q[1] || q[2] >= max_value {
        q[2] = q[1];
    }
    if q[3] < q[2] || q[3] >= max_value {
        q[3] = q[2];
    }
    let main = &mut params.qt[0];
    setup_qlut(&mut main.table, q);
    main.q_base = q_base;
    main.max_grad = 0;
    main.total_values = (q[4] + 2 * q_base) / (2 * q_base + 1) + 1;
    main.raw_bits = log2ceil(main.total_values);
    main.q_grad_mult = 9;
    params.max_bits = 4 * log2ceil(q[4] + 1);
}

fn init_params(header: &FujiHeader) -> Params {
    let size = 2usize << header.bits;
    let max_value = (1i32 << header.bits) - 1;
    let mut params = Params {
        qt: Default::default(),
        max_bits: 0,
        min_value: 0x40,
        max_value,
        line_width: if header.xtrans {
            (header.block_width as usize * 2) / 3
        } else {
            header.block_width as usize >> 1
        },
    };
    params.qt[0].table = vec![0; size];
    if header.lossless {
        params.qt[0].q_base = -1;
        init_main_qtable(&mut params, 0);
    } else {
        let qp4 = max_value;
        let mut q = [0, 0, 0, 0, qp4];
        // Table 1.
        q[1] = if qp4 >= 0x12 { 0x12 } else { q[0] + 1 };
        q[2] = if qp4 >= 0x43 { 0x43 } else { q[1] };
        q[3] = if qp4 >= 0x114 { 0x114 } else { q[2] };
        let mut t1 = QTable {
            table: vec![0; size],
            q_base: 0,
            max_grad: 5,
            q_grad_mult: 3,
            total_values: qp4 + 1,
            raw_bits: log2ceil(qp4 + 1),
        };
        setup_qlut(&mut t1.table, q);
        // Table 2.
        q[0] = 1;
        q[1] = if qp4 >= 0x15 { 0x15 } else { q[0] + 1 };
        q[2] = if qp4 >= 0x48 { 0x48 } else { q[1] };
        q[3] = if qp4 >= 0x11B { 0x11B } else { q[2] };
        let mut t2 = QTable {
            table: vec![0; size],
            q_base: 1,
            max_grad: 6,
            q_grad_mult: 3,
            total_values: (qp4 + 2) / 3 + 1,
            raw_bits: log2ceil((qp4 + 2) / 3 + 1),
        };
        setup_qlut(&mut t2.table, q);
        // Table 3.
        q[0] = 2;
        q[1] = if qp4 >= 0x18 { 0x18 } else { q[0] + 1 };
        q[2] = if qp4 >= 0x4D { 0x4D } else { q[1] };
        q[3] = if qp4 >= 0x122 { 0x122 } else { q[2] };
        let mut t3 = QTable {
            table: vec![0; size],
            q_base: 2,
            max_grad: 7,
            q_grad_mult: 3,
            total_values: (qp4 + 4) / 5 + 1,
            raw_bits: log2ceil((qp4 + 4) / 5 + 1),
        };
        setup_qlut(&mut t3.table, q);
        params.qt[1] = t1;
        params.qt[2] = t2;
        params.qt[3] = t3;
        params.qt[0].q_base = -1;
    }
    params
}

/// MSB-first reader over one strip; zeros past the end.
struct Bits<'a> {
    payload: &'a [u8],
    bit: usize,
}

impl Bits<'_> {
    #[inline]
    fn get(&self, index: usize) -> u32 {
        u32::from((self.payload.get(index >> 3).copied().unwrap_or(0) >> (7 - (index & 7))) & 1)
    }

    #[inline]
    fn zeros(&mut self) -> i32 {
        let mut count = 0;
        // A strip of zeros past the end would never terminate; stop well past it.
        let limit = self.payload.len() * 8 + 64 * 1024 * 8;
        while self.get(self.bit) == 0 {
            self.bit += 1;
            count += 1;
            if self.bit > limit {
                return count;
            }
        }
        self.bit += 1;
        count
    }

    #[inline]
    fn read(&mut self, count: i32) -> i32 {
        let mut value = 0i32;
        for _ in 0..count.max(0) {
            value = (value << 1) | self.get(self.bit) as i32;
            self.bit += 1;
        }
        value
    }
}

#[derive(Debug, Clone, Copy)]
struct Grad {
    value1: i32,
    value2: i32,
}

#[derive(Debug, Clone)]
struct Grads {
    grads: [Grad; 41],
    lossy: [[Grad; 5]; 3],
}

impl Grads {
    fn new() -> Self {
        let zero = Grad {
            value1: 0,
            value2: 0,
        };
        Self {
            grads: [zero; 41],
            lossy: [[zero; 5]; 3],
        }
    }
}

struct Block<'a> {
    bits: Bits<'a>,
    even: [Grads; 3],
    odd: [Grads; 3],
    lines: Vec<u16>,
    stride: usize,
}

impl Block<'_> {
    #[inline]
    fn at(&self, line: usize, pos: isize) -> i32 {
        let index = (line * self.stride) as isize + 1 + pos;
        usize::try_from(index)
            .ok()
            .and_then(|i| self.lines.get(i))
            .map_or(0, |v| i32::from(*v))
    }

    #[inline]
    fn set(&mut self, line: usize, pos: isize, value: u16) {
        let index = (line * self.stride) as isize + 1 + pos;
        if let Some(slot) = usize::try_from(index)
            .ok()
            .and_then(|i| self.lines.get_mut(i))
        {
            *slot = value;
        }
    }
}

fn bit_diff(value1: i32, value2: i32) -> i32 {
    let mut bits = 0;
    if value2 < value1 {
        while bits <= 14 {
            bits += 1;
            if (value2 << bits) >= value1 {
                break;
            }
        }
    }
    bits
}

#[inline]
fn abs(value: i32) -> i32 {
    value.wrapping_abs()
}

fn quant_gradient(max: i32, q: &QTable, v1: i32, v2: i32) -> i32 {
    let lookup = |v: i32| {
        usize::try_from(max + v)
            .ok()
            .and_then(|i| q.table.get(i))
            .map_or(0, |v| i32::from(*v))
    };
    q.q_grad_mult * lookup(v1) + lookup(v2)
}

/// Shared tail of the two sample decoders: read the residual and update the
/// gradient statistics; returns the signed code and the table used.
fn residual(
    block: &mut Block<'_>,
    params: &Params,
    parity_odd: bool,
    table: usize,
    gradient_set: usize,
    gradient: usize,
) -> i32 {
    let qt = params.qt.get(gradient_set).unwrap_or(&params.qt[0]);
    let grads_owner = if parity_odd {
        &mut block.odd
    } else {
        &mut block.even
    };
    let grads = &mut grads_owner[table];
    let grad = if gradient_set == 0 {
        grads.grads.get_mut(gradient)
    } else {
        grads
            .lossy
            .get_mut(gradient_set - 1)
            .and_then(|set| set.get_mut(gradient))
    };
    let Some(grad) = grad else {
        return 0;
    };
    let sample = block.bits.zeros();
    let mut code = if sample < params.max_bits - qt.raw_bits - 1 {
        let bits = bit_diff(grad.value1, grad.value2);
        let read = block.bits.read(bits);
        read + (sample << bits)
    } else {
        block.bits.read(qt.raw_bits) + 1
    };
    code = if code & 1 != 0 {
        -1 - code / 2
    } else {
        code / 2
    };
    grad.value1 += abs(code);
    if grad.value2 == params.min_value {
        grad.value1 >>= 1;
        grad.value2 >>= 1;
    }
    grad.value2 += 1;
    code
}

fn choose_table(params: &Params, diff: i32) -> usize {
    let mut chosen = 0;
    let mut i = 1;
    while params.qt[0].q_base >= i as i32 && i < 4 {
        if diff <= params.qt[i].max_grad {
            chosen = i;
            break;
        }
        i += 1;
    }
    chosen
}

fn finish(params: &Params, qt: &QTable, mut value: i32) -> u16 {
    let span = 2 * qt.q_base + 1;
    if value < -qt.q_base {
        value += qt.total_values * span;
    } else if value > qt.q_base + params.max_value {
        value -= qt.total_values * span;
    }
    if value >= 0 {
        value.min(params.max_value) as u16
    } else {
        0
    }
}

fn decode_even(block: &mut Block<'_>, params: &Params, line: usize, pos: isize, table: usize) {
    let lw = params.line_width as isize;
    let rb = block.at(line, pos - 2 - lw);
    let rc = block.at(line, pos - 3 - lw);
    let rd = block.at(line, pos - 1 - lw);
    let rf = block.at(line, pos - 4 - 2 * lw);
    let diff_rc_rb = abs(rc - rb);
    let diff_rf_rb = abs(rf - rb);
    let diff_rd_rb = abs(rd - rb);
    let set = choose_table(params, diff_rf_rb + diff_rc_rb);
    let qt = &params.qt[set];
    let grad = quant_gradient(params.max_value, qt, rb - rf, rc - rb);
    let gradient = abs(grad) as usize;
    let interp = if diff_rc_rb > diff_rf_rb && diff_rc_rb > diff_rd_rb {
        rf + rd + 2 * rb
    } else if diff_rd_rb > diff_rc_rb && diff_rd_rb > diff_rf_rb {
        rf + rc + 2 * rb
    } else {
        rd + rc + 2 * rb
    };
    let code = residual(block, params, false, table, set, gradient);
    let qt = &params.qt[set];
    let step = code * (2 * qt.q_base + 1);
    let value = if grad < 0 {
        (interp >> 2) - step
    } else {
        (interp >> 2) + step
    };
    let out = finish(params, qt, value);
    block.set(line, pos, out);
}

fn decode_odd(block: &mut Block<'_>, params: &Params, line: usize, pos: isize, table: usize) {
    let lw = params.line_width as isize;
    let ra = block.at(line, pos - 1);
    let rb = block.at(line, pos - 2 - lw);
    let rc = block.at(line, pos - 3 - lw);
    let rd = block.at(line, pos - 1 - lw);
    let rg = block.at(line, pos + 1);
    let diff_rc_ra = abs(rc - ra);
    let diff_rb_rc = abs(rb - rc);
    let set = choose_table(params, diff_rb_rc + diff_rc_ra);
    let qt = &params.qt[set];
    let grad = quant_gradient(params.max_value, qt, rb - rc, rc - ra);
    let gradient = abs(grad) as usize;
    let interp = if (rb > rc && rb > rd) || (rb < rc && rb < rd) {
        (rg + ra + 2 * rb) >> 2
    } else {
        (ra + rg) >> 1
    };
    let code = residual(block, params, true, table, set, gradient);
    let qt = &params.qt[set];
    let step = code * (2 * qt.q_base + 1);
    let value = if grad < 0 {
        interp - step
    } else {
        interp + step
    };
    let out = finish(params, qt, value);
    block.set(line, pos, out);
}

fn interpolate_even(block: &mut Block<'_>, line_width: usize, line: usize, pos: isize) {
    let lw = line_width as isize;
    let rb = block.at(line, pos - 2 - lw);
    let rc = block.at(line, pos - 3 - lw);
    let rd = block.at(line, pos - 1 - lw);
    let rf = block.at(line, pos - 4 - 2 * lw);
    let diff_rc_rb = abs(rc - rb);
    let diff_rf_rb = abs(rf - rb);
    let diff_rd_rb = abs(rd - rb);
    let value = if diff_rc_rb > diff_rf_rb && diff_rc_rb > diff_rd_rb {
        (rf + rd + 2 * rb) >> 2
    } else if diff_rd_rb > diff_rc_rb && diff_rd_rb > diff_rf_rb {
        (rf + rc + 2 * rb) >> 2
    } else {
        (rd + rc + 2 * rb) >> 2
    };
    block.set(line, pos, value as u16);
}

fn extend(block: &mut Block<'_>, line_width: usize, from: usize, to: usize) {
    for line in from..=to {
        let left = block.at(line - 1, 0) as u16;
        let right = block.at(line - 1, line_width as isize - 1) as u16;
        block.set(line, -1, left);
        block.set(line, line_width as isize, right);
    }
}

/// What to do with one of the two even-position samples in a pass.
#[derive(Clone, Copy)]
enum Even {
    Code,
    Interpolate,
}

/// One of the six passes: two colour lines decoded in lockstep, even positions
/// first and odd positions trailing by eight.
#[allow(clippy::too_many_arguments)]
fn pass(
    block: &mut Block<'_>,
    params: &Params,
    first: (usize, &dyn Fn(isize) -> Even),
    second: (usize, &dyn Fn(isize) -> Even),
    table_even: usize,
    table_odd: usize,
) {
    let width = params.line_width as isize;
    let (mut a_even, mut a_odd, mut b_even, mut b_odd) = (0isize, 1isize, 0isize, 1isize);
    // The second line's even position drives the loop, as in the reference.
    while b_even < width || b_odd < width {
        if b_even < width {
            for (line, decide, pos) in [
                (first.0, first.1, &mut a_even),
                (second.0, second.1, &mut b_even),
            ] {
                match decide(*pos) {
                    Even::Code => decode_even(block, params, line, *pos, table_even),
                    Even::Interpolate => interpolate_even(block, params.line_width, line, *pos),
                }
                *pos += 2;
            }
        }
        if b_even > 8 {
            decode_odd(block, params, first.0, a_odd, table_odd);
            a_odd += 2;
            decode_odd(block, params, second.0, b_odd, table_odd);
            b_odd += 2;
        }
    }
}

fn decode_xtrans(block: &mut Block<'_>, params: &Params) {
    let lw = params.line_width;
    let code = |_: isize| Even::Code;
    let interp = |_: isize| Even::Interpolate;
    // Pass 1: R2 interpolated, G2 coded. The reference decodes R then G, with G
    // driving the loop.
    pass(block, params, (R2, &interp), (G2, &code), 0, 0);
    extend(block, lw, R2, R4);
    extend(block, lw, G2, G7);
    // Pass 2: G3 coded, B2 interpolated - G drives the loop and comes first.
    pass_g_first(block, params, (G3, &code), (B2, &interp), 1, 1);
    extend(block, lw, G2, G7);
    extend(block, lw, B2, B4);
    // Pass 3: R3 coded at positions not divisible by four, G4 interpolated.
    let r3 = |pos: isize| {
        if pos & 3 != 0 {
            Even::Code
        } else {
            Even::Interpolate
        }
    };
    pass(block, params, (R3, &r3), (G4, &interp), 2, 2);
    extend(block, lw, R2, R4);
    extend(block, lw, G2, G7);
    // Pass 4: G5 coded, B3 interpolated where position % 4 == 2.
    let b3 = |pos: isize| {
        if pos & 3 == 2 {
            Even::Interpolate
        } else {
            Even::Code
        }
    };
    pass_g_first(block, params, (G5, &code), (B3, &b3), 0, 0);
    extend(block, lw, G2, G7);
    extend(block, lw, B2, B4);
    // Pass 5: R4 interpolated where position % 4 == 2, G6 coded.
    let r4 = |pos: isize| {
        if pos & 3 == 2 {
            Even::Interpolate
        } else {
            Even::Code
        }
    };
    pass(block, params, (R4, &r4), (G6, &code), 1, 1);
    extend(block, lw, R2, R4);
    extend(block, lw, G2, G7);
    // Pass 6: G7 interpolated, B4 coded at positions not divisible by four.
    let b4 = |pos: isize| {
        if pos & 3 != 0 {
            Even::Code
        } else {
            Even::Interpolate
        }
    };
    pass_g_first(block, params, (G7, &interp), (B4, &b4), 2, 2);
    extend(block, lw, G2, G7);
    extend(block, lw, B2, B4);
}

/// A pass whose green line comes first and drives the loop.
fn pass_g_first(
    block: &mut Block<'_>,
    params: &Params,
    first: (usize, &dyn Fn(isize) -> Even),
    second: (usize, &dyn Fn(isize) -> Even),
    table_even: usize,
    table_odd: usize,
) {
    let width = params.line_width as isize;
    let (mut a_even, mut a_odd, mut b_even, mut b_odd) = (0isize, 1isize, 0isize, 1isize);
    while a_even < width || a_odd < width {
        if a_even < width {
            for (line, decide, pos) in [
                (first.0, first.1, &mut a_even),
                (second.0, second.1, &mut b_even),
            ] {
                match decide(*pos) {
                    Even::Code => decode_even(block, params, line, *pos, table_even),
                    Even::Interpolate => interpolate_even(block, params.line_width, line, *pos),
                }
                *pos += 2;
            }
        }
        if a_even > 8 {
            decode_odd(block, params, first.0, a_odd, table_odd);
            a_odd += 2;
            decode_odd(block, params, second.0, b_odd, table_odd);
            b_odd += 2;
        }
    }
}

fn decode_bayer(block: &mut Block<'_>, params: &Params) {
    let lw = params.line_width;
    let code = |_: isize| Even::Code;
    pass(block, params, (R2, &code), (G2, &code), 0, 0);
    extend(block, lw, R2, R4);
    extend(block, lw, G2, G7);
    pass_g_first(block, params, (G3, &code), (B2, &code), 1, 1);
    extend(block, lw, G2, G7);
    extend(block, lw, B2, B4);
    pass(block, params, (R3, &code), (G4, &code), 2, 2);
    extend(block, lw, R2, R4);
    extend(block, lw, G2, G7);
    pass_g_first(block, params, (G5, &code), (B3, &code), 0, 0);
    extend(block, lw, G2, G7);
    extend(block, lw, B2, B4);
    pass(block, params, (R4, &code), (G6, &code), 1, 1);
    extend(block, lw, R2, R4);
    extend(block, lw, G2, G7);
    pass_g_first(block, params, (G7, &code), (B4, &code), 2, 2);
    extend(block, lw, G2, G7);
    extend(block, lw, B2, B4);
}

fn init_grads(params: &Params, grads: &mut [Grads; 3]) {
    let max_diff = 2.max((params.qt[0].total_values + 0x20) >> 6);
    for set in grads.iter_mut() {
        for grad in &mut set.grads {
            *grad = Grad {
                value1: max_diff,
                value2: 1,
            };
        }
    }
}

/// Decode one strip into its columns of the sensor, `block_width * height`.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_lines)] // Keep ordered format decoding and its validation together.
fn decode_strip(
    header: &FujiHeader,
    shared: &Params,
    payload: &[u8],
    q_bases: Option<&[u8]>,
    strip_width: usize,
    pattern: &[u8; 36],
    bayer: [[u8; 2]; 2],
) -> Vec<u16> {
    let mut params = shared.clone();
    let line_width = params.line_width;
    let stride = line_width + 2;
    let mut block = Block {
        bits: Bits { payload, bit: 0 },
        even: [Grads::new(), Grads::new(), Grads::new()],
        odd: [Grads::new(), Grads::new(), Grads::new()],
        lines: vec![0; LINES * stride],
        stride,
    };
    if header.lossless {
        init_grads(&params, &mut block.even);
        init_grads(&params, &mut block.odd);
    } else {
        for k in 0..3 {
            let max_diff = 2.max((params.qt[k + 1].total_values + 0x20) >> 6);
            for set in block.even.iter_mut().chain(block.odd.iter_mut()) {
                for grad in &mut set.lossy[k] {
                    *grad = Grad {
                        value1: max_diff,
                        value2: 1,
                    };
                }
            }
        }
    }

    let rows = header.lines as usize * 6;
    let mut out = vec![0u16; strip_width * rows];
    let moves = [
        (R0, R3),
        (R0 + 1, R4),
        (G0, G6),
        (G0 + 1, G7),
        (B0, B3),
        (B0 + 1, B4),
    ];
    let zeros = [(R2, 3usize), (G2, 6), (B2, 3)];
    for line in 0..header.lines as usize {
        if !header.lossless {
            let q_base = q_bases
                .and_then(|q| q.get(line))
                .map_or(0, |v| i32::from(*v));
            if line == 0 || q_base != params.qt[0].q_base {
                init_main_qtable(&mut params, q_base);
                init_grads(&params, &mut block.even);
                init_grads(&params, &mut block.odd);
            }
        }
        if header.xtrans {
            decode_xtrans(&mut block, &params);
        } else {
            decode_bayer(&mut block, &params);
        }
        for (to, from) in moves {
            let source: Vec<u16> = block
                .lines
                .get(from * stride..(from + 1) * stride)
                .map(<[u16]>::to_vec)
                .unwrap_or_default();
            if let Some(target) = block.lines.get_mut(to * stride..(to + 1) * stride) {
                target.copy_from_slice(&source);
            }
        }
        // Copy the six decoded rows into the strip.
        for row in 0..6usize {
            for x in 0..strip_width {
                let (line_index, position) = if header.xtrans {
                    let colour = pattern.get(row * 6 + x % 6).copied().unwrap_or(1);
                    let index = (((x * 2 / 3) & 0x7FFF_FFFE) | ((x % 3) & 1)) + ((x % 3) >> 1);
                    let line_index = match colour {
                        0 => R2 + (row >> 1),
                        2 => B2 + (row >> 1),
                        _ => G2 + row,
                    };
                    (line_index, index)
                } else {
                    let colour = bayer[row & 1][x & 1];
                    let line_index = match colour {
                        0 => R2 + (row >> 1),
                        2 => B2 + (row >> 1),
                        _ => G2 + row,
                    };
                    (line_index, x >> 1)
                };
                let value = block.at(line_index, position as isize) as u16;
                if let Some(slot) = out.get_mut((line * 6 + row) * strip_width + x) {
                    *slot = value;
                }
            }
        }
        for (first, count) in zeros {
            if let Some(span) = block
                .lines
                .get_mut(first * stride..(first + count) * stride)
            {
                span.fill(0);
            }
            let left = block.at(first - 1, 0) as u16;
            let right = block.at(first - 1, line_width as isize - 1) as u16;
            block.set(first, -1, left);
            block.set(first, line_width as isize, right);
        }
    }
    out
}

/// Decode a compressed RAF sensor block: `data` starts at the 16-byte header.
///
/// `pattern` is the 6x6 X-Trans layout at the sensor's origin (red 0, green 1,
/// blue 2); `bayer` the 2x2 layout for the Bayer bodies.
///
/// # Errors
///
/// `AURA-RAW-2002` when the header or the strip table is inconsistent.
pub fn decode(
    payload: &[u8],
    header: &FujiHeader,
    pattern: &[u8; 36],
    bayer: [[u8; 2]; 2],
) -> AuraResult<Vec<u16>> {
    let blocks = header.blocks as usize;
    let mut at = 16usize;
    let sizes: Vec<usize> = (0..blocks)
        .map(|i| {
            let p = at + 4 * i;
            payload
                .get(p..p + 4)
                .and_then(|b| b.try_into().ok())
                .map_or(0, |b| u32::from_be_bytes(b) as usize)
        })
        .collect();
    at += (4 * blocks + 0xF) & !0xF;
    let line_step = (header.lines as usize + 0xF) & !0xF;
    let q_table = if header.lossless {
        None
    } else {
        let total = blocks * line_step;
        let q = payload
            .get(at..at + total)
            .ok_or_else(|| corrupt("fuji quantiser table is outside the file"))?;
        at += total;
        Some(q)
    };
    let mut offsets = Vec::with_capacity(blocks);
    for size in &sizes {
        offsets.push(at);
        at += size;
    }
    let params = init_params(header);
    let width = header.width as usize;
    let height = header.height as usize;
    let block_width = header.block_width as usize;
    let strips: Vec<(usize, Vec<u16>)> = (0..blocks)
        .into_par_iter()
        .map(|index| {
            let start = offsets.get(index).copied().unwrap_or(0);
            let size = sizes.get(index).copied().unwrap_or(0);
            let bytes = payload
                .get(start..(start + size).min(payload.len()))
                .unwrap_or(&[]);
            let strip_width = if index + 1 == blocks {
                width.saturating_sub(block_width * index)
            } else {
                block_width
            };
            let q = q_table.and_then(|q| q.get(index * line_step..(index + 1) * line_step));
            (
                strip_width,
                decode_strip(header, &params, bytes, q, strip_width, pattern, bayer),
            )
        })
        .collect();
    let mut plane = vec![0u16; width * height];
    for (index, (strip_width, strip)) in strips.iter().enumerate() {
        let x0 = index * block_width;
        for row in 0..height {
            let (Some(from), Some(to)) = (
                strip.get(row * strip_width..(row + 1) * strip_width),
                plane.get_mut(row * width + x0..row * width + x0 + strip_width),
            ) else {
                continue;
            };
            to.copy_from_slice(from);
        }
    }
    Ok(plane)
}

/// Everything the decoder needs: the header and the colour layout at the
/// sensor's own origin (X-Trans 6x6, or 2x2 for the Bayer bodies).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FujiLayout {
    /// The 16-byte header.
    pub header: FujiHeader,
    /// X-Trans layout, red 0, green 1, blue 2.
    pub pattern: [u8; 36],
    /// Bayer layout for the GFX and X-A bodies.
    pub bayer: [[u8; 2]; 2],
}
