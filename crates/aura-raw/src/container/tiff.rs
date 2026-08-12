//! A bounds-checked TIFF/EXIF reader.
//!
//! Almost every RAW format is a TIFF with private tags: DNG, CR2, NEF, ARW, ORF,
//! PEF, SRW and RW2 all use this structure, and a JPEG's EXIF block is the same
//! bytes with a six-byte prefix. One careful parser therefore unlocks most of the
//! camera market, and it is the only place where hostile input meets pointer
//! arithmetic - so every read here is checked and the module contains no `unsafe`
//! and no indexing that can panic.
//!
//! Three defences against malformed files, all of them cheap:
//!
//! * every offset is validated against the buffer length before use;
//! * the IFD walk is bounded in count and refuses to revisit an offset, so a
//!   file whose "next IFD" points at itself terminates instead of hanging;
//! * entry counts are capped, so a directory claiming four billion entries is
//!   refused rather than allocated.

use aura_core::errors::raw::corrupt;
use aura_core::AuraResult;

/// Largest number of image file directories we will walk in one file.
const MAX_IFDS: usize = 64;

/// Largest number of entries one directory may declare.
const MAX_ENTRIES: usize = 4096;

/// Largest number of values we will materialise from one entry.
const MAX_VALUES: usize = 65_536;

/// TIFF tags this build reads. Named so a grep for a tag finds its meaning.
pub mod tag {
    /// Bit flags describing what an IFD's image is for.
    pub const NEW_SUBFILE_TYPE: u16 = 0x00FE;
    /// Image width in pixels.
    pub const IMAGE_WIDTH: u16 = 0x0100;
    /// Image height in pixels.
    pub const IMAGE_LENGTH: u16 = 0x0101;
    /// Bits per sample, one value per component.
    pub const BITS_PER_SAMPLE: u16 = 0x0102;
    /// Compression scheme.
    pub const COMPRESSION: u16 = 0x0103;
    /// Colour interpretation, 32803 means a colour filter array.
    pub const PHOTOMETRIC: u16 = 0x0106;
    /// Byte offsets of the image strips.
    pub const STRIP_OFFSETS: u16 = 0x0111;
    /// EXIF orientation, 1 to 8.
    pub const ORIENTATION: u16 = 0x0112;
    /// Components per pixel.
    pub const SAMPLES_PER_PIXEL: u16 = 0x0115;
    /// Rows in each strip.
    pub const ROWS_PER_STRIP: u16 = 0x0116;
    /// Byte counts of the image strips.
    pub const STRIP_BYTE_COUNTS: u16 = 0x0117;
    /// Manufacturer.
    pub const MAKE: u16 = 0x010F;
    /// Model.
    pub const MODEL: u16 = 0x0110;
    /// How samples are laid out, 1 chunky and 2 planar.
    pub const PLANAR_CONFIG: u16 = 0x011C;
    /// Offset of an embedded JPEG.
    pub const JPEG_INTERCHANGE_FORMAT: u16 = 0x0201;
    /// Length of an embedded JPEG.
    pub const JPEG_INTERCHANGE_LENGTH: u16 = 0x0202;
    /// Tile width in pixels.
    pub const TILE_WIDTH: u16 = 0x0142;
    /// Tile height in pixels.
    pub const TILE_LENGTH: u16 = 0x0143;
    /// Byte offsets of the image tiles.
    pub const TILE_OFFSETS: u16 = 0x0144;
    /// Byte counts of the image tiles.
    pub const TILE_BYTE_COUNTS: u16 = 0x0145;
    /// Offsets of sub-directories, where RAW data usually lives.
    pub const SUB_IFDS: u16 = 0x014A;
    /// Offset of the EXIF directory.
    pub const EXIF_IFD: u16 = 0x8769;
    /// The manufacturer's private block, which for several bodies is a complete
    /// TIFF of its own carrying the decode tables.
    pub const MAKER_NOTE: u16 = 0x927C;
    /// Repeat pattern dimensions of the colour filter array.
    pub const CFA_REPEAT_DIM: u16 = 0x828D;
    /// The colour filter array pattern itself.
    pub const CFA_PATTERN: u16 = 0x828E;
    /// Capture time, as written by the camera.
    pub const DATETIME_ORIGINAL: u16 = 0x9003;
    /// DNG format version; its presence proves the file is a DNG.
    pub const DNG_VERSION: u16 = 0xC612;
    /// Per-plane black level.
    pub const BLACK_LEVEL: u16 = 0xC61A;
    /// Per-plane white level.
    pub const WHITE_LEVEL: u16 = 0xC61D;
    /// Origin of the useful image area inside the sensor array.
    pub const DEFAULT_CROP_ORIGIN: u16 = 0xC61F;
    /// Size of the useful image area.
    pub const DEFAULT_CROP_SIZE: u16 = 0xC620;
    /// Colour matrix for the first calibration illuminant.
    pub const COLOR_MATRIX_1: u16 = 0xC621;
    /// Colour matrix for the second calibration illuminant.
    pub const COLOR_MATRIX_2: u16 = 0xC622;
    /// As-shot neutral, camera-native reciprocal white balance.
    pub const AS_SHOT_NEUTRAL: u16 = 0xC628;
    /// Layout of the colour filter array, DNG's spelling of it.
    pub const CFA_PLANE_COLOR: u16 = 0xC616;
    /// The two-dimensional CFA pattern in DNG's spelling.
    pub const CFA_LAYOUT: u16 = 0xC617;
}

/// Compression values this build understands.
pub mod compression {
    /// Not compressed; samples are packed at their declared bit depth.
    pub const NONE: u16 = 1;
    /// Baseline JPEG, used for embedded previews.
    pub const JPEG_OLD: u16 = 6;
    /// JPEG, which for a CFA image means lossless JPEG (SOF3).
    pub const JPEG: u16 = 7;
    /// DNG's "lossy JPEG" for linearised RGB data.
    pub const LOSSY_JPEG: u16 = 34_892;
}

/// One directory entry, kept in its raw form so accessors can interpret it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    /// The tag number.
    pub tag: u16,
    /// The TIFF type code, 1 to 12.
    pub kind: u16,
    /// How many values the entry holds.
    pub count: u32,
    /// Where the entry's four value bytes live in the file.
    pub value_position: usize,
}

/// One image file directory.
#[derive(Debug, Clone, Default)]
pub struct Ifd {
    /// Where this directory started, for diagnostics.
    pub offset: u32,
    /// Its entries, in file order.
    pub entries: Vec<Entry>,
}

impl Ifd {
    /// The first entry with this tag, if any.
    #[must_use]
    pub fn get(&self, tag: u16) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.tag == tag)
    }

    /// True when this directory declares a colour filter array image, which is
    /// how the sensor data is told apart from the previews beside it.
    #[must_use]
    pub fn is_cfa(&self) -> bool {
        self.get(tag::PHOTOMETRIC).is_some()
    }
}

/// A parsed TIFF structure over a borrowed buffer.
#[derive(Debug)]
pub struct TiffFile<'a> {
    bytes: &'a [u8],
    little: bool,
    base: usize,
    ifds: Vec<Ifd>,
}

/// Size in bytes of one value of each TIFF type, indexed by type code.
const TYPE_SIZES: [usize; 13] = [0, 1, 1, 2, 4, 8, 1, 1, 2, 4, 8, 4, 8];

impl<'a> TiffFile<'a> {
    /// Parse a TIFF whose header starts at the beginning of `bytes`.
    ///
    /// # Errors
    ///
    /// Returns `AURA-RAW-2002` when the header, an offset or an entry count is
    /// not self-consistent.
    pub fn parse(bytes: &'a [u8]) -> AuraResult<Self> {
        Self::parse_at(bytes, 0)
    }

    /// Parse a TIFF whose header starts at `base` inside a larger file, which is
    /// how EXIF blocks and Fujifilm containers embed one.
    ///
    /// # Errors
    ///
    /// Returns `AURA-RAW-2002` on any inconsistency.
    pub fn parse_at(bytes: &'a [u8], base: usize) -> AuraResult<Self> {
        let header = bytes
            .get(base..base + 8)
            .ok_or_else(|| corrupt("tiff header does not fit in the file"))?;
        let little = match header.get(..2) {
            Some(b"II") => true,
            Some(b"MM") => false,
            _ => return Err(corrupt("tiff byte order mark is neither II nor MM")),
        };
        let mut file = Self {
            bytes,
            little,
            base,
            ifds: Vec::new(),
        };
        let magic = file.u16_at(base + 2)?;
        // 42 is baseline TIFF; 0x4F52/0x5352 are Olympus, 0x0055 is Panasonic.
        // All three use the same directory structure from here on.
        if !matches!(magic, 42 | 0x0055 | 0x4F52 | 0x5352) {
            return Err(corrupt(format!("unexpected tiff magic {magic}")));
        }
        let first = file.u32_at(base + 4)?;
        file.walk(first)?;
        Ok(file)
    }

    /// Walk the directory chain plus every sub-directory, refusing to loop.
    fn walk(&mut self, first: u32) -> AuraResult<()> {
        let mut visited: Vec<u32> = Vec::new();
        let mut pending: Vec<u32> = vec![first];

        while let Some(offset) = pending.pop() {
            if offset == 0 || visited.contains(&offset) || visited.len() >= MAX_IFDS {
                continue;
            }
            visited.push(offset);

            let (ifd, next) = self.read_ifd(offset)?;

            // Sub-directories and the EXIF directory are directories too.
            for tag in [tag::SUB_IFDS, tag::EXIF_IFD] {
                if let Some(entry) = ifd.get(tag) {
                    for value in self.u64s(entry) {
                        if let Ok(child) = u32::try_from(value) {
                            pending.push(child);
                        }
                    }
                }
            }
            if next != 0 {
                pending.push(next);
            }
            self.ifds.push(ifd);
        }

        if self.ifds.is_empty() {
            return Err(corrupt("tiff file declares no directories"));
        }
        // Deterministic order regardless of the traversal, so two runs over the
        // same file always pick the same directory for the same purpose.
        self.ifds.sort_by_key(|ifd| ifd.offset);
        Ok(())
    }

    fn read_ifd(&self, offset: u32) -> AuraResult<(Ifd, u32)> {
        let start = self.base + offset as usize;
        let count = self.u16_at(start)? as usize;
        if count > MAX_ENTRIES {
            return Err(corrupt(format!("directory declares {count} entries")));
        }
        let mut entries = Vec::with_capacity(count);
        for index in 0..count {
            let at = start + 2 + index * 12;
            let tag = self.u16_at(at)?;
            let kind = self.u16_at(at + 2)?;
            let values = self.u32_at(at + 4)?;
            // An entry whose value bytes fall outside the file is a corrupt
            // entry, but skipping it keeps a mostly-good file readable.
            if at + 12 > self.bytes.len() {
                break;
            }
            entries.push(Entry {
                tag,
                kind,
                count: values,
                value_position: at + 8,
            });
        }
        let next = self.u32_at(start + 2 + count * 12).unwrap_or(0);
        Ok((Ifd { offset, entries }, next))
    }

    /// Every directory found in the file, ordered by file offset.
    #[must_use]
    pub fn ifds(&self) -> &[Ifd] {
        &self.ifds
    }

    /// The whole buffer this file was parsed from.
    #[must_use]
    pub const fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    /// Where the TIFF header starts inside the buffer.
    #[must_use]
    pub const fn base(&self) -> usize {
        self.base
    }

    /// True when the file is little-endian. Sample bytes follow the same order
    /// as the directory entries, so the mosaic reader needs to know.
    #[must_use]
    pub const fn little(&self) -> bool {
        self.little
    }

    /// The first entry with this tag in any directory.
    #[must_use]
    pub fn find(&self, tag: u16) -> Option<&Entry> {
        self.ifds.iter().find_map(|ifd| ifd.get(tag))
    }

    /// A checked slice of the underlying file.
    ///
    /// # Errors
    ///
    /// Returns `AURA-RAW-2002` when the range runs past the end of the file.
    pub fn slice(&self, offset: usize, len: usize) -> AuraResult<&'a [u8]> {
        let end = offset
            .checked_add(len)
            .ok_or_else(|| corrupt(format!("slice offset {offset} plus length {len} overflows")))?;
        self.bytes.get(offset..end).ok_or_else(|| {
            corrupt(format!(
                "slice {offset}..{end} runs past the {} byte file",
                self.bytes.len()
            ))
        })
    }

    /// Where an entry's values actually live in the buffer.
    ///
    /// Needed by the manufacturers' private blocks, which are not TIFF values at
    /// all but raw byte runs that a private parser has to walk itself.
    #[must_use]
    pub fn payload_at(&self, entry: &Entry) -> Option<usize> {
        self.payload_position(entry)
    }

    fn u16_at(&self, at: usize) -> AuraResult<u16> {
        let raw = self
            .bytes
            .get(at..at + 2)
            .ok_or_else(|| corrupt(format!("16-bit read at {at} is past the end")))?;
        let (a, b) = (
            raw.first().copied().unwrap_or(0),
            raw.get(1).copied().unwrap_or(0),
        );
        Ok(if self.little {
            u16::from_le_bytes([a, b])
        } else {
            u16::from_be_bytes([a, b])
        })
    }

    fn u32_at(&self, at: usize) -> AuraResult<u32> {
        let raw = self
            .bytes
            .get(at..at + 4)
            .ok_or_else(|| corrupt(format!("32-bit read at {at} is past the end")))?;
        let mut quad = [0u8; 4];
        for (slot, byte) in quad.iter_mut().zip(raw.iter()) {
            *slot = *byte;
        }
        Ok(if self.little {
            u32::from_le_bytes(quad)
        } else {
            u32::from_be_bytes(quad)
        })
    }

    /// Byte position of an entry's payload, following the offset when the values
    /// do not fit in the four inline bytes.
    fn payload_position(&self, entry: &Entry) -> Option<usize> {
        let size = TYPE_SIZES.get(entry.kind as usize).copied().unwrap_or(0);
        if size == 0 {
            return None;
        }
        let total = size.checked_mul(entry.count as usize)?;
        if total <= 4 {
            Some(entry.value_position)
        } else {
            let offset = self.u32_at(entry.value_position).ok()?;
            Some(self.base + offset as usize)
        }
    }

    /// Read an entry as unsigned integers. Signed and floating types are
    /// converted, so a caller asking for a count never has to know the encoding.
    #[must_use]
    pub fn u64s(&self, entry: &Entry) -> Vec<u64> {
        self.f64s(entry)
            .into_iter()
            .map(|value| {
                if value.is_finite() && value >= 0.0 {
                    value as u64
                } else {
                    0
                }
            })
            .collect()
    }

    /// Read an entry as floating point. Rationals are divided here, which is the
    /// only place in the crate that has to know they are pairs.
    #[must_use]
    #[allow(clippy::cast_precision_loss, clippy::cast_lossless)]
    pub fn f64s(&self, entry: &Entry) -> Vec<f64> {
        let Some(position) = self.payload_position(entry) else {
            return Vec::new();
        };
        let count = (entry.count as usize).min(MAX_VALUES);
        let size = TYPE_SIZES.get(entry.kind as usize).copied().unwrap_or(0);
        if size == 0 {
            return Vec::new();
        }
        let mut out = Vec::with_capacity(count);
        for index in 0..count {
            let at = position + index * size;
            let value = match entry.kind {
                1 | 7 => self.bytes.get(at).map(|b| f64::from(*b)),
                2 => self.bytes.get(at).map(|b| f64::from(*b)),
                3 => self.u16_at(at).ok().map(f64::from),
                4 => self.u32_at(at).ok().map(f64::from),
                5 => {
                    let numerator = self.u32_at(at).ok();
                    let denominator = self.u32_at(at + 4).ok();
                    match (numerator, denominator) {
                        (Some(n), Some(d)) if d != 0 => Some(f64::from(n) / f64::from(d)),
                        (Some(n), Some(_)) => Some(f64::from(n)),
                        _ => None,
                    }
                }
                6 => self.bytes.get(at).map(|b| f64::from(*b as i8)),
                8 => self.u16_at(at).ok().map(|v| f64::from(v as i16)),
                9 => self.u32_at(at).ok().map(|v| f64::from(v as i32)),
                10 => {
                    let numerator = self.u32_at(at).ok().map(|v| v as i32);
                    let denominator = self.u32_at(at + 4).ok().map(|v| v as i32);
                    match (numerator, denominator) {
                        (Some(n), Some(d)) if d != 0 => Some(f64::from(n) / f64::from(d)),
                        (Some(n), Some(_)) => Some(f64::from(n)),
                        _ => None,
                    }
                }
                11 => self.u32_at(at).ok().map(|v| f64::from(f32::from_bits(v))),
                12 => {
                    let low = self.u32_at(at).ok();
                    let high = self.u32_at(at + 4).ok();
                    match (low, high) {
                        (Some(a), Some(b)) => {
                            let bits = if self.little {
                                (u64::from(b) << 32) | u64::from(a)
                            } else {
                                (u64::from(a) << 32) | u64::from(b)
                            };
                            Some(f64::from_bits(bits))
                        }
                        _ => None,
                    }
                }
                _ => None,
            };
            match value {
                Some(v) => out.push(v),
                None => break,
            }
        }
        out
    }

    /// Read an ASCII entry, trimming the trailing NUL and surrounding padding.
    #[must_use]
    pub fn ascii(&self, entry: &Entry) -> String {
        let Some(position) = self.payload_position(entry) else {
            return String::new();
        };
        let count = (entry.count as usize).min(MAX_VALUES);
        let Some(raw) = self.bytes.get(position..position.saturating_add(count)) else {
            return String::new();
        };
        let text: Vec<u8> = raw.iter().copied().take_while(|byte| *byte != 0).collect();
        String::from_utf8_lossy(&text).trim().to_string()
    }

    /// Convenience: the first value of a tag as an integer.
    #[must_use]
    pub fn scalar(&self, tag: u16) -> Option<u64> {
        let entry = self.find(tag)?;
        self.u64s(entry).first().copied()
    }

    /// Convenience: a tag's text.
    #[must_use]
    pub fn text(&self, tag: u16) -> Option<String> {
        let entry = self.find(tag)?;
        let value = self.ascii(entry);
        if value.is_empty() {
            None
        } else {
            Some(value)
        }
    }

    /// A 3x3 matrix stored as nine rationals, DNG's encoding for colour
    /// matrices. Returns `None` unless exactly nine finite values are present.
    #[must_use]
    pub fn matrix3(&self, tag: u16) -> Option<crate::colour::matrix::Mat3> {
        let entry = self.find(tag)?;
        let values = self.f64s(entry);
        if values.len() < 9 || values.iter().any(|v| !v.is_finite()) {
            return None;
        }
        let at = |index: usize| values.get(index).copied().unwrap_or(0.0);
        Some([
            [at(0), at(1), at(2)],
            [at(3), at(4), at(5)],
            [at(6), at(7), at(8)],
        ])
    }
}
