//! Panasonic's RW2 encoding 8: the GH6, GH7, G9 II, S5 II, S5 IIX and S9.
//!
//! The sensor is cut into up to five vertical stripes, each coded on its own.
//! The bit stream is read 64 bits at a time with every byte's bits reversed;
//! each sample's difference from the same colour in the previous 2x2 block is a
//! Huffman code whose seventeen lengths, values and shifts the file carries
//! (tags `0x0040`/`0x0041`), and the decoded values go through a six-segment
//! curve (`0x0039`/`0x003A`) that the file also carries.
//!
//! The format is unpublished; this is an independent safe-Rust implementation
//! of the description in LibRaw's `pana8.cpp`, checked against it sample for
//! sample. The arithmetic deliberately mirrors that description's integer
//! widths - a shift by 64 that x86 reduces modulo 64, a sixteen-bit mask built
//! by sign extension - because the camera's tables were written for it.

// The seventeen code tables and the four predictors are fixed-size arrays
// indexed by bounded loop counters and by `column & 3`; everything a file can
// steer - the stripe data and the output plane - goes through `get`.
#![allow(clippy::indexing_slicing)]

use rayon::prelude::*;

/// One stripe of the frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stripe {
    /// File offset of the stripe's data.
    pub offset: u64,
    /// First sensor column the stripe covers.
    pub left: u32,
    /// Compressed size in bits.
    pub bits: u32,
    /// Columns the stripe covers.
    pub width: u32,
    /// Rows the stripe covers.
    pub height: u32,
}

/// Everything the file's tags `0x0039` to `0x0048` say.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Pana8Tags {
    /// Curve breakpoints, `0x003A`.
    pub tag3a: [u32; 6],
    /// Curve shifts, `0x0039`.
    pub tag39: [u32; 6],
    /// The largest value, `0x003B`.
    pub tag3b: u32,
    /// The first 2x2 block's predictors, `0x003C` to `0x003F`.
    pub initial: [u32; 4],
    /// Code lengths, `0x0040` high halves.
    pub tag40a: [u32; 17],
    /// Code values, `0x0040` low halves.
    pub tag40b: [u32; 17],
    /// Code shifts, `0x0041`.
    pub tag41: [u32; 17],
    /// The stripes, `0x0042` to `0x0048`.
    pub stripes: Vec<Stripe>,
}

struct Tables {
    huff_coeff: [u32; 18],
    table1: [u64; 17],
    table2: [u64; 17],
    extra: Option<Vec<u8>>,
    gamma: Option<Vec<u16>>,
    initial: [u32; 4],
    datamax: u32,
}

fn get_d_bit(tables: &Tables, bits: u64) -> u32 {
    for i in 0..16 {
        if bits & tables.table2[i] == tables.table1[i] {
            return i as u32;
        }
    }
    u32::from(tables.table2[16] & bits == tables.table1[16]) ^ 0x11
}

fn gamma_curve(tags: &Pana8Tags, index: u32) -> u32 {
    let v4 = index.min(0xFFFF);
    let a = |i: usize| tags.tag3a.get(i).copied().unwrap_or(0);
    let mut v5 = 0usize;
    if v4 >= (0xFFFF & a(1)) {
        v5 = 1;
        if v4 >= (0xFFFF & a(2)) {
            v5 = 2;
            if v4 >= (0xFFFF & a(3)) {
                v5 = 3;
                if v4 >= (0xFFFF & a(4)) {
                    v5 = if v4 >= (0xFFFF & a(5)) { 5 } else { 4 };
                }
            }
        }
    }
    let v6 = a(v5);
    let v7 = tags.tag39.get(v5).copied().unwrap_or(0);
    let mut v8 = v4.wrapping_sub(v6 & 0xFFFF);
    let v9 = v7 & 0x1F;
    let result = if v9 == 31 {
        if v5 == 5 {
            0xFFFF
        } else {
            (a(v5 + 1) >> 16) & 0xFFFF
        }
    } else if v7 & 0x10 == 0 && v9 == 15 {
        (v6 >> 16) & 0xFFFF
    } else {
        if v7 & 0x10 == 0 {
            if v9 != 0 {
                v8 = (v8.wrapping_add(1 << (v9 - 1))) >> v9;
            }
        } else {
            v8 = v8.wrapping_shl(v7 & 0xF);
        }
        v8.wrapping_add((v6 >> 16) & 0xFFFF)
    };
    result.min(tags.tag3b)
}

fn build(tags: &Pana8Tags) -> Tables {
    let mut huff_coeff = [0u32; 18];
    for (i, slot) in huff_coeff.iter_mut().take(17).enumerate() {
        *slot = (tags.tag41[i] << 24) | (tags.tag40a[i] << 16) | tags.tag40b[i];
    }
    let mut gamma = vec![0u16; 0x10000];
    let mut identity = true;
    for (i, slot) in gamma.iter_mut().enumerate() {
        let value = gamma_curve(tags, i as u32);
        *slot = (value & 0xFFFF) as u16;
        if value != i as u32 {
            identity = false;
        }
    }
    let (mut table1, mut table2) = ([0u64; 17], [0u64; 17]);
    let mut longest = 0u32;
    for index in 0..17 {
        let hc = huff_coeff[index];
        let low = (hc >> 16) & 0x1F;
        let mut v8: i16 = 0;
        if hc & 0x1F_0000 != 0 {
            let mut h7 = ((hc >> 16) & 0xFFFF) & 7;
            if low.wrapping_sub(1) >= 7 {
                let mut diff = h7.wrapping_sub(low);
                loop {
                    v8 = ((i32::from(v8) << 8) | 0xFF) as i16;
                    diff = diff.wrapping_add(8);
                    if diff == 0 {
                        break;
                    }
                }
            }
            while h7 != 0 {
                v8 = (2 * i32::from(v8) + 1) as i16;
                h7 -= 1;
            }
        }
        let v9 = (hc & (i32::from(v8) as u32)) as u16;
        if longest < low {
            longest = low;
        }
        // Shifting by 64 reduces modulo 64 on the hardware the tables target.
        table2[index] = 0xFFFFu64.wrapping_shl(64 - low);
        table1[index] = u64::from(v9).wrapping_shl(64 - low);
    }
    let mut tables = Tables {
        huff_coeff,
        table1,
        table2,
        extra: None,
        gamma: (!identity).then_some(gamma),
        initial: tags.initial.map(|v| v & 0xFFFF),
        datamax: tags.tag3b,
    };
    if longest < 17 {
        let extra: Vec<u8> = (0..0x10000u64)
            .map(|j| (get_d_bit(&tables, j << 48) & 0xFF) as u8)
            .collect();
        tables.extra = Some(extra);
    }
    tables
}

/// The stripe's bytes as 64-bit words with every byte's bits reversed.
fn words(payload: &[u8], bytes: usize) -> Vec<u64> {
    let count = bytes.div_ceil(8);
    (0..count)
        .map(|i| {
            let mut word = 0u64;
            for k in 0..8usize {
                let byte = payload.get(i * 8 + k).copied().unwrap_or(0).reverse_bits();
                word |= u64::from(byte) << (56 - 8 * k);
            }
            word
        })
        .collect()
}

/// Decode one stripe into `width * height` samples.
#[allow(clippy::too_many_lines)]
fn decode_stripe(tables: &Tables, payload: &[u64], width: usize, height: usize) -> Vec<u16> {
    let half_width = width >> 1;
    let half_height = height >> 1;
    let mut out = vec![0u16; width * height];
    if half_width == 0 || half_height == 0 || payload.len() < 2 {
        return out;
    }
    let jobs = payload.len();
    let word = |i: usize| payload.get(i).copied().unwrap_or(0);
    let double_width = 4 * half_width;
    let mut line = vec![0u32; double_width];
    let mut line_base = tables.initial;
    let mut tail: i64 = 0;
    let mut portion: i32 = 0;
    let mut at = 0usize;
    let datamax = tables.datamax as i32;
    let limit = |v: i32| v.clamp(0, datamax.max(0)) as u32;

    for row in 0..half_height {
        let mut base = line_base;
        for column in 0..double_width {
            let bits: u64;
            if portion < 0 {
                let next = at + 1;
                if next >= jobs {
                    return out;
                }
                portion += 64;
                let shift = (portion & 0xFF) as u32;
                bits = (word(next) >> portion as u32) | word(at).wrapping_shl(64 - shift);
                at = next;
            } else {
                if at >= jobs {
                    return out;
                }
                bits = word(at).wrapping_shr(portion as u32) | tail as u64;
                if portion == 0 {
                    portion = 64;
                    at += 1;
                }
            }
            let index = match &tables.extra {
                Some(extra) => extra
                    .get(((bits >> 48) & 0xFFFF) as usize)
                    .copied()
                    .unwrap_or(0) as usize,
                None => get_d_bit(tables, bits) as usize,
            };
            let hc = tables.huff_coeff.get(index).copied().unwrap_or(0);
            let v37 = ((hc >> 24) & 0x1F) as i32;
            let v38: i64 = (bits << ((hc >> 16) & 0x1F)) as i64;
            let mut v90: u64 = u64::from((index as i32 - v37) as u32);
            let right = ((v37 as u8).wrapping_sub(index as u8) & 63) as u32;
            let mut v39: i32 =
                (((v38 as u64) >> right) as u16 as i32).wrapping_shl((hc >> 24) & 63);
            if index as i32 - v37 <= 0 {
                v39 = (v39 as u32 & 0xFFFF_0000) as i32;
            }
            let delta1: i32 = if v38 < 0 {
                i32::from(v39 as u16)
            } else if index != 0 {
                let v40 = (-1i32).wrapping_shl(index as u32);
                if v37 as u8 != 0 {
                    i32::from(v39 as u16).wrapping_add(v40)
                } else {
                    i32::from(v39 as u16).wrapping_add(v40).wrapping_add(1)
                }
            } else {
                0
            };
            let v42 = (portion as u32).wrapping_sub((hc >> 16) & 0x1F);
            let delta2 = if v37 as u8 != 0 {
                1i32.wrapping_shl((v37 - 1) as u32)
            } else {
                0
            };
            let delta = delta1.wrapping_add(delta2);
            let group = column >> 2;
            let (slot, coefficient) = match column & 3 {
                0 => (0, 0),
                1 => (2, 2),
                2 => (1, 1),
                _ => (3, 3),
            };
            let value = limit((base[coefficient] as i32).wrapping_add(delta));
            if let Some(target) = line.get_mut(group * 4 + slot) {
                *target = value;
            }
            if index as i32 <= v37 {
                v90 = 0;
            }
            tail = v38.wrapping_shl(v90 as u32);
            portion = v42.wrapping_sub(v90 as u32) as i32;
            if column & 3 == 3 {
                for (k, slot) in base.iter_mut().enumerate() {
                    *slot = line.get(group * 4 + k).copied().unwrap_or(0);
                }
            }
            if column == 3 {
                for (k, slot) in line_base.iter_mut().enumerate() {
                    *slot = line.get(k).copied().unwrap_or(0);
                }
            }
        }
        let map = |v: u32| -> u16 {
            let v = (v & 0xFFFF) as u16;
            match &tables.gamma {
                Some(gamma) => gamma.get(usize::from(v)).copied().unwrap_or(v),
                None => v,
            }
        };
        let mut column = 0;
        while column + 1 < width {
            let group = column / 2;
            let get = |k: usize| line.get(group * 4 + k).copied().unwrap_or(0);
            let r0 = row * 2 * width;
            let r1 = r0 + width;
            for (index, value) in [
                (r0 + column, get(0)),
                (r0 + column + 1, get(1)),
                (r1 + column, get(2)),
                (r1 + column + 1, get(3)),
            ] {
                if let Some(slot) = out.get_mut(index) {
                    *slot = map(value);
                }
            }
            column += 2;
        }
    }
    out
}

/// Decode a whole encoding-8 frame from the file, `width * height` samples.
#[must_use]
pub fn decode(file: &[u8], tags: &Pana8Tags, width: usize, height: usize) -> Vec<u16> {
    let tables = build(tags);
    let stripes: Vec<(Stripe, Vec<u16>)> = tags
        .stripes
        .par_iter()
        .map(|stripe| {
            let start = usize::try_from(stripe.offset).unwrap_or(usize::MAX);
            let bytes = (stripe.bits as usize).div_ceil(8);
            let payload = file
                .get(start..start.saturating_add(bytes).min(file.len()))
                .unwrap_or(&[]);
            let words = words(payload, bytes);
            (
                *stripe,
                decode_stripe(
                    &tables,
                    &words,
                    stripe.width as usize,
                    stripe.height as usize,
                ),
            )
        })
        .collect();
    let mut plane = vec![0u16; width * height];
    for (stripe, samples) in stripes {
        let (w, left) = (stripe.width as usize, stripe.left as usize);
        for row in 0..(stripe.height as usize).min(height) {
            let (Some(from), Some(to)) = (
                samples.get(row * w..(row + 1) * w),
                plane.get_mut(row * width + left..(row * width + left + w).min((row + 1) * width)),
            ) else {
                continue;
            };
            let n = to.len().min(from.len());
            to[..n].copy_from_slice(&from[..n]);
        }
    }
    plane
}
