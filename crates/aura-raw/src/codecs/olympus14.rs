//! Olympus and OM System's parameterised ORF coder, 12 and 14 bit.
//!
//! The same predictive scheme as [`super::olympus`], but with its constants -
//! how many literal low bits each sample carries, the adaptation thresholds,
//! the escape length - read from the file's ImageProcessing directory (tags
//! `0x0640` to `0x0653`) instead of fixed. Every current body writes them, and
//! the high-resolution and 14-bit modes depend on them. The data starts with a
//! `0x0653`-byte header that reads as the number one, then two zero bytes.
//!
//! An independent safe-Rust implementation of the description in LibRaw's
//! `olympus14.cpp`, checked against it sample for sample.

use aura_core::errors::raw::corrupt;
use aura_core::AuraResult;

/// Tags `0x0640` to `0x0653` in order, and the valid bits (`0x0611`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Olympus14 {
    /// `0x0640` to `0x0653`.
    pub tags: [u32; 20],
    /// `0x0611`.
    pub valid_bits: u32,
}

struct Pump<'a> {
    payload: &'a [u8],
    at: usize,
    storage: u32,
    count: u32,
}

impl Pump<'_> {
    fn byte(&mut self) -> u32 {
        let value = u32::from(self.payload.get(self.at).copied().unwrap_or(0));
        self.at += 1;
        value
    }

    fn get2(&mut self) -> u32 {
        (self.byte() << 8) | self.byte()
    }

    fn get4(&mut self) -> u32 {
        (self.get2() << 16) | self.get2()
    }

    fn replace(&mut self) {
        self.storage = self.get4();
        self.count = 32;
    }

    fn add(&mut self) -> u32 {
        let next = self.get2();
        self.storage = (self.storage << 16) | next;
        self.count += 16;
        self.count
    }

    fn skip(&mut self, visible: &mut u32) {
        if *visible == 0 {
            self.replace();
            self.count -= 1;
            *visible = self.count;
        } else {
            *visible -= 1;
            self.count = *visible;
        }
    }

    fn bits(&mut self, wanted: u32) -> u32 {
        while self.count < wanted {
            self.add();
        }
        let value = self
            .storage
            .wrapping_shl(32 - (self.count & 0x1F))
            .wrapping_shr(32 - wanted);
        self.count -= wanted;
        value
    }
}

/// One residual: returns `(signed code, magnitude, t640 bits, t643 bits)`.
fn code(pump: &mut Pump<'_>, wbits: u32, t640: u32, t643: u32, t652: u32) -> (i32, i32, u32, u32) {
    if pump.count == 0 {
        pump.replace();
    }
    pump.count -= 1;
    let sign = (pump.storage >> pump.count) & 1;
    let t643_bits = if t643 != 0 { pump.bits(t643) } else { 0 };
    let t640_bits = if t640 != 0 { pump.bits(t640) } else { 0 };
    let mut visible = pump.count;
    let mut high = 0u32;
    loop {
        pump.skip(&mut visible);
        if (pump.storage >> visible) & 1 != 0 {
            break;
        }
        high += 1;
        if high == t652 {
            high = 0;
            if wbits != 15 {
                let extra = 15 - wbits;
                while visible < extra {
                    visible = pump.add();
                }
                high = pump
                    .storage
                    .wrapping_shl(32 - (visible & 0x1F))
                    .wrapping_shr(wbits + 17);
                visible -= extra;
                pump.count = visible;
            }
            pump.skip(&mut visible);
            break;
        }
    }
    let low = if wbits < 1 {
        0
    } else {
        while visible < wbits {
            visible = pump.add();
        }
        let value = pump
            .storage
            .wrapping_shl(32 - (visible & 0x1F))
            .wrapping_shr(32 - (wbits & 0x1F));
        pump.count = visible - wbits;
        value
    };
    let magnitude = (low | high.wrapping_shl(wbits)) as i32;
    (
        magnitude ^ (sign.wrapping_neg() as i32),
        magnitude,
        t640_bits,
        t643_bits,
    )
}

/// Decode a parameterised ORF frame; `data` starts at the strip.
///
/// # Errors
///
/// `AURA-RAW-2002` when the header is not the one the tags announce, or a code
/// asks for sixteen or more literal bits.
#[allow(clippy::too_many_lines)]
pub fn decode(
    payload: &[u8],
    params: &Olympus14,
    width: usize,
    height: usize,
) -> AuraResult<Vec<u16>> {
    let t = |i: usize| params.tags.get(i).copied().unwrap_or(0);
    let (t640, t641, t642, t643, t644, t645) = (t(0), t(1), t(2) as i32, t(3), t(4), t(5));
    let (t646, t647, t648, t649, t650, t651) = (
        t(6) as i32,
        t(7) as i32,
        t(8) as i32,
        t(9) as i32,
        t(10) as i32,
        t(11) as i32,
    );
    let t652 = t(12).max(1);
    let header = t(13) as usize;
    if header == 0 {
        return Err(corrupt("olympus frame header length is zero"));
    }
    let mut value = 0u64;
    for i in 0..header {
        value = (value << 8) | u64::from(payload.get(i).copied().unwrap_or(0));
    }
    if value != 1
        || payload.get(header).copied().unwrap_or(1) | payload.get(header + 1).copied().unwrap_or(1)
            != 0
    {
        return Err(corrupt(
            "olympus frame header is not the one its tags announce",
        ));
    }
    let mut pump = Pump {
        payload: payload.get(header + 2..).unwrap_or(&[]),
        at: 0,
        storage: 0,
        count: 0,
    };
    let one_shl_641 = 1i32.wrapping_shl(t641);
    let one_shl_645 = 1i32.wrapping_shl(t645 & 0x1F);
    let mut bit_counts = vec![0i32; 65_536];
    let mut n = 1usize;
    for i in 0..16 {
        for _ in 0..(1usize << i) {
            if let Some(slot) = bit_counts.get_mut(n) {
                *slot = i + 1;
            }
            n += 1;
        }
    }
    let mut out = vec![0u16; width * height];
    let mut context = [0i32; 4];
    let (mut lpred, mut pred1, mut glc, mut gcode_outer) = (0i32, 0i32, 0i32, 0i32);
    for row in 0..height {
        for col in 0..width {
            let p = gcode_outer;
            gcode_outer = glc;
            let psel = if col > 1 && one_shl_645 >= p {
                context[3] + 1
            } else {
                0
            };
            context[3] = context[2];
            context[2] = psel;
            let mut wbits = if col > 1 {
                let high_bits = bit_counts.get((p & 0xFFFF) as usize).copied().unwrap_or(0);
                let (t1, t2) = if psel >= t646 {
                    (t648, t647)
                } else {
                    (t650, t649)
                };
                t2 + (high_bits - t1).max(-1) + 1
            } else {
                t649
            };
            if wbits > t651 {
                wbits = t651;
            }
            if !(0..16).contains(&wbits) {
                return Err(corrupt("olympus code asks for too many literal bits"));
            }
            let (mut gcode, magnitude, t640_bits, t643_bits) =
                code(&mut pump, wbits as u32, t640, t643, t652);
            glc = magnitude;
            let vbits = if t643 != 0 {
                let v = if col < 2 { 0 } else { pred1 };
                if t643 == 1 {
                    gcode = t643_bits as i32 + 2 * (v + gcode);
                    v + (gcode >> 1)
                } else {
                    gcode = t643_bits as i32 + 4 * (v + gcode);
                    ((gcode >> 1) & !1) + v + (gcode >> 2)
                }
            } else {
                0
            };
            pred1 = context[0];
            context[0] = if t644 == 15 { 0 } else { vbits >> t644 };
            let at = |r: usize, c: usize| {
                i32::from(out.get(r * width + c).copied().unwrap_or(0)) >> t640
            };
            let w = if col < 2 { t642 } else { context[1] };
            let mut north = if row < 2 { t642 } else { at(row - 2, col) };
            let north_west = if row < 2 || col < 2 {
                t642
            } else {
                at(row - 2, col - 2)
            };
            context[1] = lpred;
            if w < north || north_west < w {
                if north_west <= north && w >= north {
                    north = w;
                } else if north_west < north || w >= north {
                    if north_west > w || w >= north {
                        if (north - north_west).wrapping_abs() > one_shl_641 {
                            north += w - north_west;
                        } else if (w - north_west).wrapping_abs() <= one_shl_641 {
                            north = (north + w) >> 1;
                        } else {
                            north = north + w - north_west;
                        }
                    }
                } else {
                    north = w;
                }
            }
            lpred = gcode + north;
            let pixel = t640_bits.wrapping_add(((gcode + north) as u32).wrapping_shl(t640));
            if let Some(slot) = out.get_mut(row * width + col) {
                *slot = pixel as u16;
            }
        }
    }
    let _ = params.valid_bits;
    Ok(out)
}
