//! Canon: the CR2 slice layout, and what the MakerNote says about the sensor.
//!
//! A CR2's sensor data is one lossless JPEG frame, but Canon does not store the
//! frame's rows as sensor rows. The run of decoded samples is cut into vertical
//! slices - tag `0xC640` gives how many full slices, their width and the width
//! of the last one - and each slice is written top to bottom before the next
//! begins. Decoding without undoing that produces a picture made of narrow
//! strips side by side, each one the whole height of the frame.
//!
//! Everything a render needs beyond the samples lives in the MakerNote, which a
//! CR2 carries in its EXIF directory and a CR3 carries as `CMT3`:
//!
//! * **ColorData** (`0x4001`) holds the as-shot white balance, the per-channel
//!   black level and the specular white level. Its layout changed a dozen times
//!   across the line-up, and the only reliable discriminator is the entry's
//!   length - the table below is that mapping, as the open-source decoders have
//!   documented it body by body.
//! * **SensorInfo** (`0x00E0`) holds the sensor's full size and the rectangle
//!   that is the picture; outside it is the masked optical-black border.
//!
//! Without the white balance every Canon frame rendered green, and without the
//! black level its shadows were lifted to a grey haze.

use crate::container::tiff::{Ifd, TiffFile};

/// What the MakerNote told us. Every field is optional: a body we have no row
/// for keeps the container's defaults rather than borrowing another body's.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CanonNotes {
    /// As-shot white balance multipliers `[red / green, 1, blue / green]`.
    pub as_shot: Option<[f32; 3]>,
    /// Mean of the four per-channel black levels.
    pub black: Option<u32>,
    /// The level at which the sensor clips.
    pub white: Option<u32>,
    /// The sensor's full size in photosites, borders included.
    pub sensor: Option<(u32, u32)>,
    /// The picture inside the sensor, `[left, top, right, bottom]` as Canon writes it.
    pub crop: Option<[u32; 4]>,
}

impl CanonNotes {
    /// Fill every field this one lacks from `other`.
    #[must_use]
    pub fn or(self, other: Self) -> Self {
        Self {
            as_shot: self.as_shot.or(other.as_shot),
            black: self.black.or(other.black),
            white: self.white.or(other.white),
            sensor: self.sensor.or(other.sensor),
            crop: self.crop.or(other.crop),
        }
    }
}

/// Where the fields sit inside ColorData, in 16-bit words.
#[derive(Debug, Clone, Copy)]
struct ColourLayout {
    white_balance: usize,
    black: Option<usize>,
    black_fallback: Option<usize>,
    white: Option<usize>,
}

/// The ColorData layout for an entry of `count` words whose first word is
/// `version`. `None` for a layout this build has no row for.
fn colour_layout(count: u32, version: u16) -> Option<ColourLayout> {
    let plain = |white_balance, black, white| ColourLayout {
        white_balance,
        black,
        black_fallback: None,
        white,
    };
    let cr3 = |base: usize, black: usize, black2: usize, white: usize| ColourLayout {
        white_balance: base,
        black: Some(base + black),
        black_fallback: Some(base + black2),
        white: Some(base + white),
    };
    Some(match count {
        // 20D, 350D
        582 => plain(0x19, Some(0xa6), None),
        // 1D Mark II, 1Ds Mark II
        653 => plain(0x22, Some(0x11e), None),
        // 1D Mark II N, 5D, 30D, 400D
        796 => plain(0x3f, Some(0xc4), None),
        // 1D Mark III to 60D
        674 | 692 | 702 | 1227 | 1250 | 1251 | 1337 | 1338 | 1346 => match version {
            4 | 5 => plain(0x3f, Some(0x2b4), Some(0x2b8)),
            6 | 7 => plain(0x3f, Some(0x2cb), Some(0x2cf)),
            9 => plain(0x3f, Some(0x2cf), Some(0x2d3)),
            _ => plain(0x3f, Some(0xe7), None),
        },
        // PowerShots and the early EOS M bodies
        5120 => match version {
            0xfffc => plain(0x47, Some(0x14d), Some(0x569)),
            0xfffd => plain(0x47, Some(0x108), None),
            _ => plain(0x47, None, None),
        },
        // 600D, 1200D
        1273 | 1275 => plain(0x3f, Some(0x1df), Some(0x1e3)),
        // 5D Mark III, 6D, 70D, 7D Mark II, 1D X and their siblings
        1312 | 1313 | 1316 | 1506 => match version {
            10 => plain(0x3f, Some(0x1f8), Some(0x1fc)),
            11 => plain(0x3f, Some(0x2d8), Some(0x2dc)),
            _ => plain(0x3f, None, None),
        },
        // 5DS, 5D Mark IV, 80D, 1D X Mark II, 6D Mark II, 200D, 77D, 800D, 1300D
        1560 | 1592 | 1353 | 1602 => match version {
            14 => plain(0x3f, Some(0x22c), Some(0x230)),
            _ => plain(0x3f, Some(0x30a), Some(0x30e)),
        },
        // M50, R, Ra, RP, 250D, 90D, M6 Mark II, M200, 850D and the 2019 compacts
        1820 | 1824 | 1816 => cr3(0x47, 0x2d1, 0x102, 0x2d5),
        // R5, R6, 1D X Mark III, R5 C
        1770 | 2024 | 3656 => cr3(0x55, 0x2d1, 0x102, 0x2d5),
        // R3, R6 Mark II, R7, R8, R10, R50, R100
        3973 | 3778 => cr3(0x69, 0x213, 0x102, 0x217),
        // R1, R5 Mark II
        4528 => cr3(0x69, 0x227, 0x116, 0x22b),
        _ => return None,
    })
}

/// MakerNote tags read here.
mod tag {
    pub(super) const SENSOR_INFO: u16 = 0x00E0;
    pub(super) const COLOR_DATA: u16 = 0x4001;
}

/// Read the MakerNote directory `notes` of `file`.
#[must_use]
pub fn read_notes(file: &TiffFile<'_>, notes: &Ifd) -> CanonNotes {
    let mut out = CanonNotes::default();
    if let Some(entry) = notes.get(tag::SENSOR_INFO) {
        let words = file.u64s(entry);
        let word = |index: usize| words.get(index).copied().unwrap_or(0) as u32;
        if words.len() >= 9 {
            let (width, height) = (word(1), word(2));
            if width > 0 && height > 0 {
                out.sensor = Some((width, height));
            }
            let crop = [word(5), word(6), word(7), word(8)];
            if crop[2] > crop[0] && crop[3] > crop[1] {
                out.crop = Some(crop);
            }
        }
    }
    if let Some(entry) = notes.get(tag::COLOR_DATA) {
        if entry.count > 500 {
            let words = file.u64s(entry);
            let word = |index: usize| words.get(index).copied().unwrap_or(0) as u32;
            let version = word(0) as u16;
            if let Some(layout) = colour_layout(entry.count, version) {
                let at = layout.white_balance;
                let (red, green1, green2, blue) =
                    (word(at), word(at + 1), word(at + 2), word(at + 3));
                let green = (green1 + green2) as f32 / 2.0;
                if red > 0 && blue > 0 && green > 0.0 {
                    out.as_shot = Some([red as f32 / green, 1.0, blue as f32 / green]);
                }
                let mean = |at: usize| (word(at) + word(at + 1) + word(at + 2) + word(at + 3)) / 4;
                let mut black = layout.black.map_or(0, mean);
                if black == 0 {
                    black = layout.black_fallback.map_or(0, mean);
                }
                if black > 0 {
                    out.black = Some(black);
                }
                // NormalWhiteLevel, then SpecularWhiteLevel: the second is where
                // the sensor actually clips.
                if let Some(white_at) = layout.white {
                    let specular = word(white_at + 1);
                    if specular > black {
                        out.white = Some(specular);
                    }
                }
            }
        }
    }
    out
}

/// The picture inside the sensor as `[top, left, bottom, right]`, the way the
/// reference decoders derive it: origin rounded up to an even photosite so the
/// CFA phase is kept, far edge as Canon wrote it.
#[must_use]
pub fn active_area(notes: &CanonNotes, width: u32, height: u32) -> Option<[u32; 4]> {
    let (sensor_width, sensor_height) = notes.sensor?;
    let [left, top, right, bottom] = notes.crop?;
    if (sensor_width, sensor_height) != (width, height) {
        return None;
    }
    let left = (left + 1) & !1;
    let top = (top + 1) & !1;
    // Canon's far edges are inclusive: 6,720 x 4,480 is written as 147..=6866.
    let (right, bottom) = ((right + 1).min(width), (bottom + 1).min(height));
    if right <= left || bottom <= top {
        return None;
    }
    Some([top, left, bottom, right])
}

/// Undo Canon's slicing: `stream` is the frame's samples in decode order and the
/// result is the sensor in row-major order, `width * height` samples.
///
/// `slices` is the `0xC640` triple: count of full slices, their width, the width
/// of the last one. A zero count means the frame is not sliced.
#[must_use]
pub fn unslice(stream: &[u16], slices: [u32; 3], width: u32, height: u32) -> Vec<u16> {
    let (width, height) = (width as usize, height as usize);
    let [count, slice, last] = slices.map(|value| value as usize);
    if count == 0 || slice == 0 {
        let mut out = stream.to_vec();
        out.resize(width * height, 0);
        return out;
    }
    let mut out = vec![0u16; width * height];
    let mut taken = 0usize;
    for index in 0..=count {
        let slice_width = if index < count { slice } else { last };
        let x0 = index * slice;
        if slice_width == 0 || x0 >= width {
            continue;
        }
        let columns = slice_width.min(width - x0);
        for row in 0..height {
            let source = taken + row * slice_width;
            let target = row * width + x0;
            if let (Some(from), Some(to)) = (
                stream.get(source..source + columns),
                out.get_mut(target..target + columns),
            ) {
                to.copy_from_slice(from);
            }
        }
        taken += slice_width * height;
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    /// Slicing is a permutation: write a sensor out slice by slice the way the
    /// camera does and unslicing gives the sensor back.
    #[test]
    fn unslicing_inverts_the_camera_layout() {
        let (width, height) = (11u32, 5u32);
        let slices = [2u32, 4, 3];
        let sensor: Vec<u16> = (0..width * height).map(|v| v as u16).collect();
        let mut stream = Vec::new();
        for index in 0..=2usize {
            let (x0, w) = if index < 2 { (index * 4, 4) } else { (8, 3) };
            for row in 0..height as usize {
                for column in 0..w {
                    stream.push(sensor[row * width as usize + x0 + column]);
                }
            }
        }
        assert_eq!(unslice(&stream, slices, width, height), sensor);
    }

    #[test]
    fn the_crop_origin_keeps_the_cfa_phase() {
        let notes = CanonNotes {
            sensor: Some((100, 80)),
            crop: Some([13, 9, 99, 79]),
            ..CanonNotes::default()
        };
        assert_eq!(active_area(&notes, 100, 80), Some([10, 14, 80, 100]));
        assert_eq!(active_area(&notes, 102, 80), None);
    }
}
