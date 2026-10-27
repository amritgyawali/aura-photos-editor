//! Everything the decoder needs to know about a file, read once.
//!
//! Opening a RAW twice - once for metadata, once for pixels - doubles the IO on
//! a 220 GB wedding. So each container is walked exactly once and produces a
//! [`RawMeta`]: where the previews are, where the mosaic is, what the sensor's
//! black and white levels are, and which colour matrix applies. Every later
//! stage works from this struct and never re-parses the container.

use aura_core::errors::raw::{corrupt, unsupported};
use aura_core::AuraResult;

use crate::colour::matrix::Mat3;
use crate::container::{bmff, jpeg, raf, tiff};
use crate::format::{sniff, MosaicSupport, RawFormat};

/// Where one embedded preview lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreviewRef {
    /// Byte offset in the file.
    pub offset: usize,
    /// Length in bytes.
    pub len: usize,
    /// Width in pixels, from the JPEG frame header.
    pub width: u32,
    /// Height in pixels, from the JPEG frame header.
    pub height: u32,
}

impl PreviewRef {
    /// Pixel count, the ranking key when a file carries several previews.
    #[must_use]
    pub const fn pixels(&self) -> u64 {
        self.width as u64 * self.height as u64
    }
}

/// How a mosaic's samples are actually encoded.
///
/// The TIFF compression code is not enough on its own: Nikon, Sony and Olympus
/// all reuse codes that mean something else in baseline TIFF, and two files with
/// the same code can need different decoders depending on how many bytes they
/// stored. This enum is the decision, made once while the container is walked,
/// so the pixel path never has to re-derive it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MosaicScheme {
    /// Unpacked or bit-packed samples, most significant bit first.
    Packed,
    /// Lossless JPEG (SOF3), which is what DNG and CR2 use.
    LosslessJpeg,
    /// Nikon's Huffman coding plus a per-body linearisation curve.
    Nikon(Box<crate::codecs::nikon::NikonParams>),
    /// Sony's sixteen-photosites-in-sixteen-bytes block coding.
    SonyArw2 {
        /// The four knots of the body's linearisation curve, when the file let
        /// us read them. `None` means a plain linear expansion was used and the
        /// render is flagged as approximate.
        curve: Option<[u16; 4]>,
    },
    /// Olympus's adaptive predictive coding.
    Olympus,
    /// The same coding with its constants read from the file, which every
    /// current Olympus and OM System body writes.
    Olympus14(Box<crate::codecs::olympus14::Olympus14>),
    /// Canon's CR2: one lossless JPEG frame written as vertical slices. The
    /// payload is tag `0xC640` - full slices, their width, the last one's width -
    /// and is all zeros when the frame is not sliced.
    CanonLossless {
        /// The slice layout.
        slices: [u32; 3],
    },
    /// Nikon's uncompressed fourteen-bit layout: four samples in every seven
    /// bytes, least significant bit first, each row padded to sixteen bytes.
    NikonPacked14,
    /// Samples packed least significant bit first, as a little-endian Nikon
    /// writes its uncompressed twelve-bit frames.
    PackedLsb,
    /// Twelve-bit samples packed most significant bit first inside 32-bit
    /// little-endian words: Nikon's COOLPIX P and B bodies.
    PackedWords,
    /// Panasonic's RW2 encoding 8 (GH6, S5 II and later), with its tables.
    Pana8(Box<crate::codecs::pana8::Pana8Tags>),
    /// Panasonic's RW2, in the encoding tag `0x002D` names.
    Panasonic(crate::codecs::panasonic::PanaParams),
    /// Fujifilm's compressed RAF, X-Trans or Bayer, lossless or lossy.
    Fuji(Box<crate::codecs::fuji::FujiLayout>),
    /// Sony's lossless ARW: lossless JPEG tiles whose four components are one
    /// 2x2 Bayer block each.
    SonyLossless,
    /// Canon's CR3: the CRX wavelet and Golomb-Rice coder, with its image header.
    Crx(Box<crate::codecs::crx::CrxHeader>),
    /// A compression this build does not implement; the payload is the code the
    /// file declared, so the message can name it.
    Unsupported(u16),
}

impl MosaicScheme {
    /// Stable text for telemetry and the sidecar.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Packed => "packed",
            Self::LosslessJpeg => "lossless-jpeg",
            Self::Nikon(_) => "nikon",
            Self::SonyArw2 { .. } => "sony-arw2",
            Self::Olympus => "olympus",
            Self::Olympus14(_) => "olympus-14",
            Self::CanonLossless { .. } => "canon-lossless",
            Self::SonyLossless => "sony-lossless",
            Self::Panasonic(_) => "panasonic",
            Self::Pana8(_) => "panasonic-8",
            Self::NikonPacked14 => "nikon-packed14",
            Self::PackedLsb => "packed-lsb",
            Self::PackedWords => "packed-words",
            Self::Fuji(_) => "fuji-compressed",
            Self::Crx(_) => "crx",
            Self::Unsupported(_) => "unsupported",
        }
    }
}

/// Where the sensor mosaic lives and how it is encoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MosaicRef {
    /// Width of the mosaic in sensor pixels.
    pub width: u32,
    /// Height of the mosaic in sensor pixels.
    pub height: u32,
    /// Bits in each sample as declared by the file, 8 to 16.
    pub bits_per_sample: u16,
    /// TIFF compression code, kept for diagnostics and the support matrix.
    pub compression: u16,
    /// Which decoder these bytes need.
    pub scheme: MosaicScheme,
    /// Strips or tiles, as `(offset, length)` pairs in file order.
    pub segments: Vec<(usize, usize)>,
    /// Rows in each strip; equals `height` when the image is a single strip.
    pub rows_per_strip: u32,
    /// `(tile width, tile length)` when `segments` are tiles rather than strips.
    ///
    /// Adobe DNG Converter writes lossless-JPEG DNGs as 256 x 256 tiles, so without this every
    /// converted DNG - the most common DNG there is - was read as if its first tile were the
    /// first rows of the frame and refused.
    pub tile: Option<(u32, u32)>,
    /// The rectangle that holds image data, `[top, left, bottom, right]` in photosites.
    ///
    /// A DNG converted from a Canon or Nikon RAW keeps the sensor's masked optical-black border
    /// outside this rectangle; rendering it produces black strips and the wrong aspect ratio.
    pub active_area: Option<[u32; 4]>,
    /// The picture the camera intended, `[x, y, width, height]` inside the active area.
    pub default_crop: Option<[u32; 4]>,
    /// DNG `LinearizationTable`: stored code `n` means linear sensor value `table[n]`.
    ///
    /// A DNG converted from a compressed NEF stores the camera's tone-curve indices, not sensor
    /// values - on a Nikon D70s the stored codes stop at 682 while white is 4,095. Rendering
    /// the indices as values produced a dark, clipped, magenta frame.
    pub linearization: Option<Vec<u16>>,
}

/// Fujifilm's 6x6 X-Trans layout, as the bodies write it: 0 red, 1 green,
/// 2 blue, row-major from the top-left photosite.
///
/// Files carry their own copy in the RAF block directory; this is the layout
/// every X-Trans body has shipped with, used when that block is missing.
pub const XTRANS_DEFAULT: [u8; 36] = [
    1, 1, 0, 1, 1, 2, //
    1, 1, 2, 1, 1, 0, //
    2, 0, 1, 0, 2, 1, //
    1, 1, 2, 1, 1, 0, //
    1, 1, 0, 1, 1, 2, //
    0, 2, 1, 2, 0, 1, //
];

/// The colour filter array layout, named by the colours of the top-left 2x2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CfaPattern {
    /// Red, green / green, blue.
    Rggb,
    /// Blue, green / green, red.
    Bggr,
    /// Green, red / blue, green.
    Grbg,
    /// Green, blue / red, green.
    Gbrg,
    /// Fujifilm X-Trans: a 6x6 array with no 2x2 period, carried inline because
    /// the layout is per-file rather than per-manufacturer.
    XTrans([u8; 36]),
    /// A pattern we did not recognise.
    Unknown,
}

impl CfaPattern {
    /// The side of the smallest block that is guaranteed to contain all three
    /// colours: two for Bayer, three for X-Trans. This is the binning factor the
    /// proxy path uses, and it is why an X-Trans proxy is a third of the sensor
    /// rather than a half.
    #[must_use]
    pub const fn block(self) -> u32 {
        match self {
            Self::XTrans(_) => 3,
            _ => 2,
        }
    }

    /// How far interpolation has to look to be sure of finding every colour.
    ///
    /// A Bayer array has all three colours inside any 3x3 window; X-Trans does
    /// not - its sparsest red and blue spacing needs 5x5.
    #[must_use]
    pub const fn radius(self) -> i64 {
        match self {
            Self::XTrans(_) => 2,
            _ => 1,
        }
    }

    /// Which colour plane the pixel at `(x, y)` belongs to: 0 red, 1 green,
    /// 2 blue. Unknown patterns are treated as green, which is neutral.
    #[must_use]
    pub fn colour_at(self, x: u32, y: u32) -> u8 {
        if let Self::XTrans(pattern) = self {
            let index = (y % 6) as usize * 6 + (x % 6) as usize;
            return pattern.get(index).copied().unwrap_or(1);
        }
        let even_row = y.is_multiple_of(2);
        let even_column = x.is_multiple_of(2);
        match self {
            Self::Rggb => match (even_row, even_column) {
                (true, true) => 0,
                (true, false) | (false, true) => 1,
                (false, false) => 2,
            },
            Self::Bggr => match (even_row, even_column) {
                (true, true) => 2,
                (true, false) | (false, true) => 1,
                (false, false) => 0,
            },
            Self::Grbg => match (even_row, even_column) {
                (true, true) | (false, false) => 1,
                (true, false) => 0,
                (false, true) => 2,
            },
            Self::Gbrg => match (even_row, even_column) {
                (true, true) | (false, false) => 1,
                (true, false) => 2,
                (false, true) => 0,
            },
            Self::XTrans(_) | Self::Unknown => 1,
        }
    }

    /// The same array seen from photosite `(dx, dy)`: the pattern a crop that
    /// starts there has at its own origin.
    #[must_use]
    pub fn shifted(self, dx: u32, dy: u32) -> Self {
        if let Self::XTrans(pattern) = self {
            let mut out = [1u8; 36];
            for (index, slot) in out.iter_mut().enumerate() {
                let (x, y) = (index as u32 % 6, index as u32 / 6);
                let source = ((y + dy) % 6) * 6 + (x + dx) % 6;
                *slot = pattern.get(source as usize).copied().unwrap_or(1);
            }
            return Self::XTrans(out);
        }
        if matches!(self, Self::Unknown) {
            return self;
        }
        let at = |x: u32, y: u32| self.colour_at(x + dx, y + dy);
        Self::from_codes(&[at(0, 0), at(1, 0), at(0, 1), at(1, 1)].map(u64::from))
    }

    /// Stable text for the sidecar.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rggb => "rggb",
            Self::Bggr => "bggr",
            Self::Grbg => "grbg",
            Self::Gbrg => "gbrg",
            Self::XTrans(_) => "xtrans",
            Self::Unknown => "unknown",
        }
    }

    fn from_codes(codes: &[u64]) -> Self {
        // A 6x6 pattern is X-Trans. Adobe's converter writes Fujifilm files as
        // DNGs exactly this way, so this is the path a converted RAF takes.
        if codes.len() == 36 {
            let mut pattern = [1u8; 36];
            for (slot, code) in pattern.iter_mut().zip(codes.iter()) {
                *slot = u8::try_from(*code).unwrap_or(1).min(2);
            }
            return Self::XTrans(pattern);
        }
        match (
            codes.first().copied(),
            codes.get(1).copied(),
            codes.get(2).copied(),
            codes.get(3).copied(),
        ) {
            (Some(0), Some(1), Some(1), Some(2)) => Self::Rggb,
            (Some(2), Some(1), Some(1), Some(0)) => Self::Bggr,
            (Some(1), Some(0), Some(2), Some(1)) => Self::Grbg,
            (Some(1), Some(2), Some(0), Some(1)) => Self::Gbrg,
            _ => Self::Unknown,
        }
    }
}

/// Everything one file tells us about itself.
#[derive(Debug, Clone, PartialEq)]
pub struct RawMeta {
    /// The container this file turned out to be.
    pub format: RawFormat,
    /// Manufacturer as EXIF spells it, empty when absent.
    pub make: String,
    /// Model as EXIF spells it, empty when absent.
    pub model: String,
    /// EXIF orientation, 1 to 8. Defaults to 1 rather than guessing.
    pub orientation: u16,
    /// Previews found in the file, largest first.
    pub previews: Vec<PreviewRef>,
    /// The sensor mosaic, when one was located.
    pub mosaic: Option<MosaicRef>,
    /// `XYZ(D50) -> camera` matrix carried by the file, when it carries one.
    pub colour_matrix: Option<Mat3>,
    /// As-shot neutral in camera native RGB, normalised so green is 1.0.
    pub as_shot_neutral: [f32; 3],
    /// Sensor black level in raw code values.
    pub black_level: u32,
    /// Sensor white level in raw code values.
    pub white_level: u32,
    /// Colour filter array layout.
    pub cfa: CfaPattern,
    /// True when multi-byte samples in this container are little-endian.
    pub little_endian: bool,
}

impl RawMeta {
    /// The largest preview, which is the one tier 1 uses.
    #[must_use]
    pub fn best_preview(&self) -> Option<&PreviewRef> {
        self.previews.first()
    }

    /// How far this file can be taken from its sensor data.
    #[must_use]
    pub fn mosaic_support(&self) -> MosaicSupport {
        match &self.mosaic {
            Some(mosaic) => crate::format::mosaic_support_for(&mosaic.scheme),
            None => MosaicSupport::NoMosaic,
        }
    }
}

fn default_meta(format: RawFormat) -> RawMeta {
    RawMeta {
        format,
        make: String::new(),
        model: String::new(),
        orientation: 1,
        previews: Vec::new(),
        mosaic: None,
        colour_matrix: None,
        as_shot_neutral: [1.0, 1.0, 1.0],
        // Sensible defaults for a file that declares neither: a 14-bit sensor
        // with no pedestal. Both are overwritten whenever the file says.
        black_level: 0,
        white_level: 16_383,
        cfa: CfaPattern::Rggb,
        little_endian: true,
    }
}

/// Read one file's metadata.
///
/// # Errors
///
/// Returns `AURA-RAW-2001` when the container is not one we recognise, and
/// `AURA-RAW-2002` when it is recognised but inconsistent.
pub fn read(bytes: &[u8], path: &std::path::Path) -> AuraResult<RawMeta> {
    match sniff(bytes) {
        RawFormat::Jpeg => read_jpeg(bytes),
        RawFormat::Png => Ok(default_meta(RawFormat::Png)),
        RawFormat::Raf => read_raf(bytes),
        RawFormat::Cr3 => read_cr3(bytes),
        format if format.is_tiff_family() => read_tiff_family(bytes, format),
        RawFormat::Heif => Err(aura_core::errors::raw::heif(path)),
        _ => Err(unsupported(path, "no recognised container magic")),
    }
}

fn read_jpeg(bytes: &[u8]) -> AuraResult<RawMeta> {
    let info = jpeg::probe(bytes)?;
    let mut meta = default_meta(RawFormat::Jpeg);
    meta.previews.push(PreviewRef {
        offset: 0,
        len: info.byte_len.unwrap_or(bytes.len()),
        width: info.width,
        height: info.height,
    });
    if let Some(exif_at) = jpeg::exif_tiff_offset(bytes) {
        if let Ok(file) = tiff::TiffFile::parse_at(bytes, exif_at) {
            apply_common_tags(&file, &mut meta);
        }
    }
    Ok(meta)
}

fn read_raf(bytes: &[u8]) -> AuraResult<RawMeta> {
    let header = raf::header(bytes)?;
    let mut meta = default_meta(RawFormat::Raf);
    let sensor = raf::sensor(bytes, header.cfa_header_offset, header.cfa_header_len);
    meta.cfa = CfaPattern::XTrans(sensor.xtrans.unwrap_or(XTRANS_DEFAULT));

    if let Some(raw) = raf_raw_directory(bytes, header.cfa_offset) {
        // Current bodies wrap the sensor data in a small TIFF of Fujifilm tags.
        let start = header.cfa_offset + raw.offset;
        let payload = bytes
            .get(start..start.saturating_add(raw.length).min(bytes.len()))
            .unwrap_or(&[]);
        let (width, height, bits, scheme) = match crate::codecs::fuji::FujiHeader::parse(payload) {
            Some(compressed) => (
                compressed.width,
                compressed.height,
                u16::from(compressed.bits),
                MosaicScheme::Fuji(Box::new(crate::codecs::fuji::FujiLayout {
                    header: compressed,
                    pattern: sensor.xtrans.unwrap_or(XTRANS_DEFAULT),
                    bayer: [[0, 1], [1, 2]],
                })),
            ),
            None if raw.length >= raw.width as usize * raw.height as usize * 2 => {
                (raw.width, raw.height, 16, MosaicScheme::Packed)
            }
            None => (
                raw.width,
                raw.height,
                raw.bits,
                MosaicScheme::Unsupported(0),
            ),
        };
        // A RAF without an X-Trans layout is one of the Bayer bodies (GFX).
        if sensor.xtrans.is_none() {
            meta.cfa = CfaPattern::Rggb;
        }
        let active_area = sensor
            .crop
            .filter(|[top, left, h, w]| *h > 0 && *w > 0 && top + h <= height && left + w <= width)
            .map(|[top, left, h, w]| [top, left, top + h, left + w]);
        if let Some([top, left, _, _]) = active_area {
            meta.cfa = meta.cfa.shifted(left, top);
        }
        meta.white_level = (1u32 << raw.bits.clamp(8, 16)) - 1;
        if let Some(black) = sensor.black.or(raw.black) {
            meta.black_level = black;
        }
        if let Some(white_balance) = sensor.as_shot.or(raw.as_shot) {
            meta.as_shot_neutral = white_balance;
        }
        meta.mosaic = Some(MosaicRef {
            width,
            height,
            bits_per_sample: bits,
            compression: 0,
            scheme,
            segments: vec![(start, payload.len())],
            rows_per_strip: height,
            tile: None,
            active_area,
            default_crop: None,
            linearization: None,
        });
        meta.little_endian = true;
    } else if sensor.width > 0 && sensor.height > 0 && header.cfa_len > 0 {
        let pixels = sensor.width as usize * sensor.height as usize;
        let scheme = if header.cfa_len >= pixels * 2 {
            MosaicScheme::Packed
        } else {
            MosaicScheme::Unsupported(0)
        };
        meta.mosaic = Some(MosaicRef {
            width: sensor.width,
            height: sensor.height,
            bits_per_sample: 16,
            compression: tiff::compression::NONE,
            scheme,
            segments: vec![(header.cfa_offset, header.cfa_len)],
            rows_per_strip: sensor.height,
            tile: None,
            active_area: None,
            default_crop: None,
            linearization: None,
        });
        meta.little_endian = true;
    }

    let preview = bytes
        .get(header.jpeg_offset..header.jpeg_offset.saturating_add(header.jpeg_len))
        .ok_or_else(|| corrupt("raf preview range is outside the file"))?;
    let info = jpeg::probe(preview)?;
    meta.previews.push(PreviewRef {
        offset: header.jpeg_offset,
        len: info.byte_len.unwrap_or(header.jpeg_len),
        width: info.width,
        height: info.height,
    });

    if let Some(exif_at) = jpeg::exif_tiff_offset(preview) {
        if let Ok(file) = tiff::TiffFile::parse_at(bytes, header.jpeg_offset + exif_at) {
            apply_common_tags(&file, &mut meta);
        }
    }
    Ok(meta)
}

/// What the TIFF inside a RAF's sensor block says.
struct RafRaw {
    width: u32,
    height: u32,
    bits: u16,
    offset: usize,
    length: usize,
    black: Option<u32>,
    as_shot: Option<[f32; 3]>,
}

/// Read the Fujifilm raw directory (`0xF000`) inside a RAF's sensor block.
fn raf_raw_directory(bytes: &[u8], base: usize) -> Option<RafRaw> {
    let file = tiff::TiffFile::parse_at(bytes, base).ok()?;
    let pointer = file.find(0xF000).and_then(|entry| file.raw_u32(entry))?;
    let ifd = file.directory_at(base + pointer as usize)?;
    let value = |tag: u16| {
        ifd.get(tag)
            .and_then(|entry| file.u64s(entry).first().copied())
            .map(|v| v as u32)
    };
    let black = ifd
        .get(0xF00A)
        .map(|entry| file.u64s(entry))
        .filter(|levels| !levels.is_empty())
        .map(|levels| (levels.iter().sum::<u64>() / levels.len() as u64) as u32);
    let as_shot = ifd
        .get(0xF00E)
        .map(|entry| file.u64s(entry))
        .and_then(|levels| match levels.as_slice() {
            [green, red, blue, ..] if *green > 0 && *red > 0 && *blue > 0 => Some([
                *red as f32 / *green as f32,
                1.0,
                *blue as f32 / *green as f32,
            ]),
            _ => None,
        });
    Some(RafRaw {
        width: value(0xF001)?,
        height: value(0xF002)?,
        bits: value(0xF003).unwrap_or(14) as u16,
        offset: value(0xF007)? as usize,
        length: value(0xF008).unwrap_or(0) as usize,
        black,
        as_shot,
    })
}

#[allow(clippy::too_many_lines)] // Keep ordered format decoding and its validation together.
fn read_cr3(bytes: &[u8]) -> AuraResult<RawMeta> {
    let boxes = bmff::walk(bytes)?;
    let mut meta = default_meta(RawFormat::Cr3);

    // CMT1 is IFD0 in TIFF form: make, model and orientation live there.
    if let Some(cmt1) = bmff::find(&boxes, b"CMT1") {
        if let Ok(file) = tiff::TiffFile::parse_at(bytes, cmt1.start) {
            apply_common_tags(&file, &mut meta);
        }
    }
    // CMT3 is the Canon MakerNote, again as a TIFF whose first directory it is.
    let notes = bmff::find(&boxes, b"CMT3")
        .and_then(|cmt3| tiff::TiffFile::parse_at(bytes, cmt3.start).ok())
        .and_then(|file| {
            file.ifds()
                .first()
                .map(|ifd| crate::codecs::canon::read_notes(&file, ifd))
        })
        .unwrap_or_default();

    // PRVW and THMB each wrap a complete JPEG after a short private header, so
    // the stream is located by scanning rather than by trusting an offset.
    for name in [b"PRVW", b"THMB"] {
        let Some(found) = bmff::find(&boxes, name) else {
            continue;
        };
        let Some(payload) = bytes.get(found.start..found.start.saturating_add(found.len)) else {
            continue;
        };
        if let Some((at, len)) = jpeg::find_stream(payload) {
            if let Some(stream) = payload.get(at..at + len) {
                if let Ok(info) = jpeg::probe(stream) {
                    meta.previews.push(PreviewRef {
                        offset: found.start + at,
                        len,
                        width: info.width,
                        height: info.height,
                    });
                }
            }
        }
    }

    // The tracks: one is a full-size camera JPEG, one is the sensor, and one is
    // timed metadata whose MakerNote parts complete what CMT3 left out.
    let tracks = bmff::tracks(bytes, &boxes);
    let mut notes = notes;
    for track in tracks.iter().filter(|track| track.ctmd) {
        if let (Some(offset), Some(size)) = (track.offset, track.size) {
            let (Ok(offset), Ok(size)) = (usize::try_from(offset), usize::try_from(size)) else {
                continue;
            };
            if let Some(record) = bytes.get(offset..offset.saturating_add(size)) {
                notes = notes.or(ctmd_notes(bytes, offset, record));
            }
        }
    }
    let mut sensor: Option<(crate::codecs::crx::CrxHeader, usize, usize)> = None;
    for track in &tracks {
        let (Some(offset), Some(size)) = (track.offset, track.size) else {
            continue;
        };
        let (Ok(offset), Ok(size)) = (usize::try_from(offset), usize::try_from(size)) else {
            continue;
        };
        if track.jpeg {
            push_preview(bytes, offset, size, &mut meta);
            continue;
        }
        let Some(header) = track
            .cmp1
            .as_deref()
            .and_then(crate::codecs::crx::CrxHeader::parse)
        else {
            continue;
        };
        let weight = |h: &crate::codecs::crx::CrxHeader| {
            u64::from(h.bits) * u64::from(h.width) * u64::from(h.height)
        };
        if sensor
            .as_ref()
            .is_none_or(|(best, _, _)| weight(&header) > weight(best))
        {
            sensor = Some((header, offset, size));
        }
    }

    if let Some((header, offset, size)) = sensor {
        meta.cfa = match header.cfa_layout {
            1 => CfaPattern::Grbg,
            2 => CfaPattern::Gbrg,
            3 => CfaPattern::Bggr,
            _ => CfaPattern::Rggb,
        };
        let bits = header.output_bits();
        meta.white_level = (1u32 << bits.min(16)) - 1;
        let active_area = crate::codecs::canon::active_area(&notes, header.width, header.height);
        if let Some([top, left, _, _]) = active_area {
            meta.cfa = meta.cfa.shifted(left, top);
        }
        meta.mosaic = Some(MosaicRef {
            width: header.width,
            height: header.height,
            bits_per_sample: u16::from(bits),
            compression: 0,
            scheme: MosaicScheme::Crx(Box::new(header)),
            segments: vec![(offset, size)],
            rows_per_strip: header.height,
            tile: None,
            active_area,
            default_crop: None,
            linearization: None,
        });
    }
    apply_canon_notes(&notes, &mut meta);

    if meta.previews.is_empty() && meta.mosaic.is_none() {
        return Err(corrupt(
            "cr3 container carries neither a preview nor a sensor track",
        ));
    }
    sort_previews(&mut meta);
    Ok(meta)
}

/// The Canon MakerNote parts inside a CTMD sample: little-endian records of
/// `(size, type)`, and inside types 7 and 8 tagged blocks of `(length, tag)`
/// where tag `0x927C` is a MakerNote TIFF.
fn ctmd_notes(bytes: &[u8], base: usize, record: &[u8]) -> crate::codecs::canon::CanonNotes {
    let le32 = |at: usize| {
        record
            .get(at..at + 4)
            .and_then(|b| b.try_into().ok())
            .map_or(0, |b| u32::from_le_bytes(b) as usize)
    };
    let le16 = |at: usize| {
        record
            .get(at..at + 2)
            .and_then(|b| b.try_into().ok())
            .map_or(0, u16::from_le_bytes)
    };
    let mut notes = crate::codecs::canon::CanonNotes::default();
    let mut at = 0usize;
    while at + 6 < record.len() {
        let size = le32(at);
        let kind = le16(at + 4);
        if size < 1 || at + size > record.len() {
            break;
        }
        if matches!(kind, 7 | 8) {
            let mut inner = at + 12;
            while inner + 8 < at + size {
                let length = le32(inner);
                let tag = le32(inner + 4);
                if length < 8 || inner + length > at + size {
                    break;
                }
                if tag == 0x927C {
                    if let Ok(file) = tiff::TiffFile::parse_at(bytes, base + inner + 8) {
                        if let Some(ifd) = file.ifds().first() {
                            notes = notes.or(crate::codecs::canon::read_notes(&file, ifd));
                        }
                    }
                }
                inner += length;
            }
        }
        at += size;
    }
    notes
}

/// Apply what a Canon MakerNote said, over the container's defaults.
fn apply_canon_notes(notes: &crate::codecs::canon::CanonNotes, meta: &mut RawMeta) {
    if let Some(white_balance) = notes.as_shot {
        meta.as_shot_neutral = white_balance;
    }
    if let Some(black) = notes.black {
        meta.black_level = black;
    }
    if let Some(white) = notes.white {
        meta.white_level = white.max(meta.black_level + 1);
    }
}

/// Columns of garbage some bodies write past the picture, and the picture's
/// position, for files that do not state a crop of their own. The figures are
/// the open-source decoders' per-body tables.
fn vendor_trim(make: &str, model: &str, width: u32, height: u32) -> Option<[u32; 4]> {
    let make = make.to_ascii_uppercase();
    let model = model
        .trim()
        .trim_start_matches("NIKON ")
        .trim_start_matches("Nikon ")
        .to_string();
    let (mut left, mut top, mut w, mut h) = (0u32, 0u32, width, height);
    if make.starts_with("NIKON") {
        match model.as_str() {
            "D1X" => w -= 4,
            "D40X" | "D60" | "D80" | "D3000" => {
                h -= 3;
                w -= 4;
            }
            "D3" | "D3S" | "D700" => {
                w -= 4;
                left = 2;
            }
            "D3100" => {
                w -= 28;
                left = 6;
            }
            "D5000" | "D90" => w -= 42,
            "D5100" | "D7000" | "COOLPIX A" => w -= 44,
            "D3200" | "D600" | "D610" | "D800" | "D800E" => w -= 46,
            "D4" | "Df" => {
                w -= 52;
                left = 2;
            }
            "D40" | "D50" | "D70" | "D70s" => w -= 1,
            "D200" => {
                left = 1;
                w -= 4;
            }
            m if m.starts_with("D2H") => {
                left = 6;
                w -= 14;
            }
            m if m.starts_with("D2X") => w -= if width == 3264 { 32 } else { 8 },
            m if m.starts_with("D300") => w -= 32,
            _ => return None,
        }
    } else if make.starts_with("SONY") {
        let rx1_or_a99 = model.contains("RX1") || model.contains("A99");
        match width {
            4928 if height < 3280 => w -= 8,
            5504 => w -= if height > 3664 { 8 } else { 32 },
            6048 => w -= if rx1_or_a99 { 30 } else { 24 },
            7392 => w -= 30,
            4288 | 8000 | 9600 => w -= 32,
            8672 => w -= 12,
            _ => return None,
        }
    } else {
        return None;
    }
    let _ = &mut top;
    (w > 0 && h > 0).then_some([
        top,
        left,
        top + h.min(height - top),
        left + w.min(width - left),
    ])
}

/// Panasonic's own tags in an RW2's first directory.
mod pana_tag {
    pub(super) const WIDTH: u16 = 0x0002;
    pub(super) const HEIGHT: u16 = 0x0003;
    pub(super) const TOP: u16 = 0x0004;
    pub(super) const LEFT: u16 = 0x0005;
    pub(super) const BOTTOM: u16 = 0x0006;
    pub(super) const RIGHT: u16 = 0x0007;
    pub(super) const CFA: u16 = 0x0009;
    pub(super) const BITS: u16 = 0x000A;
    pub(super) const LINEARITY: [u16; 3] = [0x000E, 0x000F, 0x0010];
    pub(super) const RED_BALANCE: u16 = 0x0011;
    pub(super) const BLUE_BALANCE: u16 = 0x0012;
    pub(super) const BLACK: [u16; 3] = [0x001C, 0x001D, 0x001E];
    pub(super) const WB_LEVELS: [u16; 3] = [0x0024, 0x0025, 0x0026];
    pub(super) const ENCODING: u16 = 0x002D;
    pub(super) const PREVIEW: u16 = 0x002E;
    pub(super) const RAW_OFFSET: u16 = 0x0118;
}

/// The sensor frame of a Panasonic RW2, and everything Panasonic's first
/// directory says about it: borders, CFA phase, levels, white balance.
#[allow(clippy::too_many_lines)] // Keep ordered format decoding and its validation together.
fn rw2_mosaic(bytes: &[u8], file: &tiff::TiffFile<'_>, meta: &mut RawMeta) -> Option<MosaicRef> {
    let ifd = file.ifds().first()?;
    let value = |tag: u16| {
        ifd.get(tag)
            .and_then(|entry| file.u64s(entry).first().copied())
            .map(|v| v as u32)
    };
    if let Some(entry) = ifd.get(pana_tag::PREVIEW) {
        if let Some(at) = file.payload_at(entry) {
            push_preview(bytes, at, entry.count as usize, meta);
        }
    }
    let width = value(pana_tag::WIDTH)?;
    let height = value(pana_tag::HEIGHT)?;
    let bits = value(pana_tag::BITS).unwrap_or(12) as u16;
    let encoding = value(pana_tag::ENCODING).unwrap_or(4) as u16;
    let offset = value(pana_tag::RAW_OFFSET).or_else(|| value(tiff::tag::STRIP_OFFSETS))? as usize;
    if width == 0 || height == 0 || offset >= bytes.len() {
        return None;
    }

    let cfa = match value(pana_tag::CFA).unwrap_or(1) {
        2 => CfaPattern::Grbg,
        3 => CfaPattern::Gbrg,
        4 => CfaPattern::Bggr,
        _ => CfaPattern::Rggb,
    };
    let (top, left) = (
        value(pana_tag::TOP).unwrap_or(0),
        value(pana_tag::LEFT).unwrap_or(0),
    );
    let (bottom, right) = (
        value(pana_tag::BOTTOM).unwrap_or(height).min(height),
        value(pana_tag::RIGHT).unwrap_or(width).min(width),
    );
    let active_area = (bottom > top && right > left).then_some([top, left, bottom, right]);
    meta.cfa = match active_area {
        Some([top, left, _, _]) => cfa.shifted(left, top),
        None => cfa,
    };

    let levels: Vec<u32> = pana_tag::WB_LEVELS
        .iter()
        .filter_map(|tag| value(*tag))
        .collect();
    if let [red, green, blue] = levels.as_slice() {
        if *red > 0 && *green > 0 && *blue > 0 {
            meta.as_shot_neutral = [
                *red as f32 / *green as f32,
                1.0,
                *blue as f32 / *green as f32,
            ];
        }
    } else if let (Some(red), Some(blue)) =
        (value(pana_tag::RED_BALANCE), value(pana_tag::BLUE_BALANCE))
    {
        if red > 0 && blue > 0 {
            meta.as_shot_neutral = [red as f32 / 256.0, 1.0, blue as f32 / 256.0];
        }
    }
    let blacks: Vec<u32> = pana_tag::BLACK
        .iter()
        .filter_map(|tag| value(*tag))
        .collect();
    if blacks.len() == 3 && blacks.iter().all(|b| *b > 0) {
        // Encoding 4 states its black fifteen codes low.
        let add = if encoding == 4 { 15 } else { 0 };
        // Green counts twice in a Bayer array.
        let sum = blacks.first().copied().unwrap_or(0)
            + 2 * blacks.get(1).copied().unwrap_or(0)
            + blacks.get(2).copied().unwrap_or(0);
        meta.black_level = sum / 4 + add;
    }
    let maximum = (1u32 << bits.min(16)) - 1;
    let linear: Vec<u32> = pana_tag::LINEARITY
        .iter()
        .filter_map(|tag| value(*tag))
        .map(|level| match level {
            16_383 => level - 64,
            4_095 => level - 16,
            _ => level,
        })
        .filter(|level| *level > meta.black_level)
        .collect();
    meta.white_level = linear.iter().copied().min().unwrap_or(maximum);

    let scheme = if encoding == 8 {
        match pana8_tags(file, ifd) {
            Some(tags) => MosaicScheme::Pana8(Box::new(tags)),
            None => MosaicScheme::Unsupported(8),
        }
    } else {
        MosaicScheme::Panasonic(crate::codecs::panasonic::PanaParams { encoding, bits })
    };
    Some(MosaicRef {
        width,
        height,
        bits_per_sample: bits,
        compression: encoding,
        scheme,
        segments: vec![(offset, bytes.len() - offset)],
        rows_per_strip: height,
        tile: None,
        active_area,
        default_crop: None,
        linearization: None,
    })
}

/// The tables and stripes of an RW2 in encoding 8: `UNDEFINED` blobs of a
/// 16-bit count followed by that many values, in the file's byte order.
fn pana8_tags(
    file: &tiff::TiffFile<'_>,
    ifd: &tiff::Ifd,
) -> Option<crate::codecs::pana8::Pana8Tags> {
    let bytes = file.bytes();
    let little = file.little();
    let u16_at = |at: usize| -> u32 {
        bytes
            .get(at..at + 2)
            .and_then(|b| b.try_into().ok())
            .map_or(0, |b| {
                u32::from(if little {
                    u16::from_le_bytes(b)
                } else {
                    u16::from_be_bytes(b)
                })
            })
    };
    let u32_at = |at: usize| -> u32 {
        bytes
            .get(at..at + 4)
            .and_then(|b| b.try_into().ok())
            .map_or(0, |b| {
                if little {
                    u32::from_le_bytes(b)
                } else {
                    u32::from_be_bytes(b)
                }
            })
    };
    let blob = |tag: u16| ifd.get(tag).and_then(|entry| file.payload_at(entry));
    let scalar = |tag: u16| {
        ifd.get(tag)
            .and_then(|entry| file.u64s(entry).first().copied())
            .map(|v| v as u32)
    };
    let mut tags = crate::codecs::pana8::Pana8Tags::default();
    if let Some(at) = blob(0x0039) {
        let count = u16_at(at).min(6) as usize;
        for (i, slot) in tags.tag39.iter_mut().take(count).enumerate() {
            *slot = u32_at(at + 2 + 4 * i);
        }
    }
    if let Some(at) = blob(0x003A) {
        let count = u16_at(at).min(6) as usize;
        for (i, slot) in tags.tag3a.iter_mut().take(count).enumerate() {
            *slot = u16_at(at + 2 + 4 * i + 2);
        }
    }
    tags.tag3b = scalar(0x003B)?;
    for (slot, tag) in tags
        .initial
        .iter_mut()
        .zip([0x003Cu16, 0x003D, 0x003E, 0x003F])
    {
        *slot = scalar(tag).unwrap_or(0);
    }
    let at = blob(0x0040)?;
    let count = u16_at(at).min(17) as usize;
    for (i, (first, second)) in tags
        .tag40a
        .iter_mut()
        .zip(&mut tags.tag40b)
        .take(count)
        .enumerate()
    {
        *first = u16_at(at + 2 + 4 * i).min(16);
        *second = u16_at(at + 2 + 4 * i + 2).min(0xFFF);
    }
    let at = blob(0x0041)?;
    let count = u16_at(at).min(17) as usize;
    for (i, slot) in tags.tag41.iter_mut().take(count).enumerate() {
        *slot = u16_at(at + 2 + 2 * i).min(64);
    }
    let stripes = scalar(0x0042)?.min(5) as usize;
    let list32 = |tag: u16| -> Vec<u32> {
        blob(tag).map_or_else(Vec::new, |at| {
            let n = u16_at(at).min(5) as usize;
            (0..n).map(|i| u32_at(at + 2 + 4 * i)).collect()
        })
    };
    let list16 = |tag: u16| -> Vec<u32> {
        blob(tag).map_or_else(Vec::new, |at| {
            let n = u16_at(at).min(5) as usize;
            (0..n).map(|i| u16_at(at + 2 + 2 * i)).collect()
        })
    };
    let (offsets, lefts, sizes, widths, heights) = (
        list32(0x0044),
        list32(0x0045),
        list32(0x0046),
        list16(0x0047),
        list16(0x0048),
    );
    for i in 0..stripes {
        tags.stripes.push(crate::codecs::pana8::Stripe {
            offset: u64::from(*offsets.get(i)?),
            left: *lefts.get(i)?,
            bits: *sizes.get(i)?,
            width: *widths.get(i)?,
            height: *heights.get(i)?,
        });
    }
    Some(tags)
}

/// Canon's slice layout tag in a CR2's sensor directory.
const CR2_SLICES: u16 = 0xC640;

/// The sensor frame of a CR2: the lossless JPEG strip that is neither a preview
/// nor a Canon sRAW, sized the way the camera wrote it.
fn cr2_mosaic(bytes: &[u8], file: &tiff::TiffFile<'_>) -> Option<MosaicRef> {
    let mut best: Option<MosaicRef> = None;
    for ifd in file.ifds() {
        let compression = ifd
            .get(tiff::tag::COMPRESSION)
            .and_then(|entry| file.u64s(entry).first().copied())
            .unwrap_or(0);
        if compression != u64::from(tiff::compression::JPEG_OLD) {
            continue;
        }
        let Some(offset) = ifd
            .get(tiff::tag::STRIP_OFFSETS)
            .and_then(|entry| file.u64s(entry).first().copied())
        else {
            continue;
        };
        let offset = offset as usize;
        let length = ifd
            .get(tiff::tag::STRIP_BYTE_COUNTS)
            .and_then(|entry| file.u64s(entry).first().copied())
            .unwrap_or(0) as usize;
        let Some(strip) = bytes.get(offset..offset.saturating_add(length).min(bytes.len())) else {
            continue;
        };
        let Ok(frame) = crate::losslessjpeg::frame_header(strip) else {
            continue;
        };
        if frame.subsampled {
            // sRAW and mRAW hold luma and chroma, not a mosaic.
            continue;
        }
        let components = frame.components as u32;
        let mut width = frame.width * components;
        let mut height = frame.height;
        if width > 4 * height && components.is_multiple_of(2) {
            width /= 2;
            height *= 2;
        }
        let slices = ifd
            .get(CR2_SLICES)
            .map(|entry| file.u64s(entry))
            .filter(|values| values.len() >= 3)
            .map_or([0, 0, 0], |values| {
                [0usize, 1, 2].map(|index| values.get(index).copied().unwrap_or(0) as u32)
            });
        if slices[0] > 0 && slices[1] > 0 {
            let sliced = slices[0] * slices[1] + slices[2];
            let total = u64::from(width) * u64::from(height);
            if sliced > 0 && total % u64::from(sliced) == 0 {
                height = (total / u64::from(sliced)) as u32;
                width = sliced;
            }
        }
        let candidate = MosaicRef {
            width,
            height,
            bits_per_sample: u16::from(frame.precision),
            compression: tiff::compression::JPEG_OLD,
            scheme: MosaicScheme::CanonLossless { slices },
            segments: vec![(offset, length)],
            rows_per_strip: height,
            tile: None,
            active_area: None,
            default_crop: None,
            linearization: None,
        };
        if best.as_ref().is_none_or(|current| {
            candidate.width * candidate.height > current.width * current.height
        }) {
            best = Some(candidate);
        }
    }
    best
}

/// Compression codes the manufacturers use for their own schemes. None of these
/// mean what baseline TIFF says they mean.
///
/// Panasonic's 34316 is deliberately absent: RW2's scheme is not implemented, so
/// it falls through to `Unsupported` and its files render tier 2 from the
/// embedded preview. See `docs/camera-support.md`.
mod private_compression {
    /// Nikon's compressed NEF.
    pub(super) const NIKON: u16 = 34_713;
    /// Sony's ARW block coding.
    pub(super) const SONY: u16 = 32_767;
}

#[allow(clippy::too_many_lines)] // Keep ordered format decoding and its validation together.
fn read_tiff_family(bytes: &[u8], format: RawFormat) -> AuraResult<RawMeta> {
    let file = tiff::TiffFile::parse(bytes)?;
    let mut meta = default_meta(format);
    meta.little_endian = file.little();
    apply_common_tags(&file, &mut meta);

    // Per-manufacturer decode parameters, read once before the mosaic hunt so
    // the scheme decision has everything it needs. Both are keyed off what the
    // file actually contains rather than off the container we sniffed, because a
    // converter can put Nikon's decode table inside a DNG.
    let nikon = nikon_table(bytes, &file);
    let sony = (format == RawFormat::Arw).then(|| crate::codecs::sony_notes::read_notes(&file));
    let sony_curve = sony
        .and_then(|notes| notes.curve)
        .or_else(|| sony_curve(&file));

    let mut best_mosaic: Option<MosaicRef> = None;

    for ifd in file.ifds() {
        // A directory with a JPEG interchange offset holds a preview.
        if let (Some(offset), Some(length)) = (
            ifd.get(tiff::tag::JPEG_INTERCHANGE_FORMAT),
            ifd.get(tiff::tag::JPEG_INTERCHANGE_LENGTH),
        ) {
            let at = file.u64s(offset).first().copied().unwrap_or(0) as usize;
            let len = file.u64s(length).first().copied().unwrap_or(0) as usize;
            push_preview(bytes, at, len, &mut meta);
        }

        let photometric = ifd
            .get(tiff::tag::PHOTOMETRIC)
            .and_then(|entry| file.u64s(entry).first().copied());
        let compression = ifd
            .get(tiff::tag::COMPRESSION)
            .and_then(|entry| file.u64s(entry).first().copied())
            .unwrap_or(0) as u16;

        // Old-style JPEG strips are how several bodies store their previews.
        if compression == tiff::compression::JPEG_OLD {
            if let Some(offsets) = ifd.get(tiff::tag::STRIP_OFFSETS) {
                let counts = ifd
                    .get(tiff::tag::STRIP_BYTE_COUNTS)
                    .map(|entry| file.u64s(entry))
                    .unwrap_or_default();
                for (index, at) in file.u64s(offsets).into_iter().enumerate() {
                    let len = counts.get(index).copied().unwrap_or(0) as usize;
                    push_preview(bytes, at as usize, len, &mut meta);
                }
            }
        }

        // Photometric 32803 is "colour filter array": the sensor itself. An
        // Olympus ORF calls its sensor directory plain greyscale (1), and is
        // recognised by being the uncompressed-declared strip of the main image.
        let orf_sensor = format == RawFormat::Orf
            && photometric == Some(1)
            && compression == tiff::compression::NONE
            && ifd.get(tiff::tag::STRIP_OFFSETS).is_some();
        if photometric == Some(32_803) || orf_sensor {
            if let Some(mosaic) =
                mosaic_from_ifd(&file, ifd, compression, format, nikon, sony_curve)
            {
                let better = best_mosaic.as_ref().is_none_or(|current| {
                    mosaic.width * mosaic.height > current.width * current.height
                });
                if better {
                    best_mosaic = Some(mosaic);
                }
            }
        }
    }

    // A Nikon curve carries the body's black and white points, and for a real
    // NEF it is the only place they are stated. A file that also carries the
    // DNG level tags has said so explicitly, and an explicit statement wins.
    let declares_levels = file.find(tiff::tag::BLACK_LEVEL).is_some();
    if let (false, Some(MosaicScheme::Nikon(params))) = (
        declares_levels,
        best_mosaic.as_ref().map(|mosaic| &mosaic.scheme),
    ) {
        meta.black_level = params.black_level();
        meta.white_level = params.white_level().max(meta.black_level + 1);
    }

    if let Some(notes) = sony {
        if let Some(white_balance) = notes.as_shot {
            meta.as_shot_neutral = white_balance;
        }
        if let Some(black) = notes.black {
            meta.black_level = black;
        }
        if let Some(white) = notes.white {
            meta.white_level = white.max(meta.black_level + 1);
        }
        if let (Some([left, top, width, height]), Some(mosaic)) = (notes.crop, best_mosaic.as_mut())
        {
            if left + width <= mosaic.width && top + height <= mosaic.height {
                mosaic.active_area = Some([top, left, top + height, left + width]);
                meta.cfa = meta.cfa.shifted(left, top);
            }
        }
    }

    if format == RawFormat::Rw2 {
        if let Some(mosaic) = rw2_mosaic(bytes, &file, &mut meta) {
            best_mosaic = Some(mosaic);
        }
    }

    if format == RawFormat::Cr2 {
        let notes = file
            .find(tiff::tag::MAKER_NOTE)
            .and_then(|entry| file.payload_at(entry))
            .and_then(|at| file.directory_at(at))
            .map(|ifd| crate::codecs::canon::read_notes(&file, &ifd))
            .unwrap_or_default();
        if let Some(mut mosaic) = cr2_mosaic(bytes, &file) {
            mosaic.active_area =
                crate::codecs::canon::active_area(&notes, mosaic.width, mosaic.height);
            meta.white_level = (1u32 << mosaic.bits_per_sample.min(16)) - 1;
            // Every Canon body since the 300D puts red at the sensor's origin;
            // the pattern recorded is the one at the picture's origin.
            meta.cfa = match mosaic.active_area {
                Some([top, left, _, _]) => CfaPattern::Rggb.shifted(left, top),
                None => CfaPattern::Rggb,
            };
            best_mosaic = Some(mosaic);
        }
        apply_canon_notes(&notes, &mut meta);
    }

    if format == RawFormat::Orf {
        if let Some(notes) = olympus_notes(bytes, &file) {
            if let Some(white_balance) = notes.as_shot {
                meta.as_shot_neutral = white_balance;
            }
            if let Some(black) = notes.black {
                meta.black_level = black;
            }
            if let Some(bits) = notes.valid_bits {
                meta.white_level = (1u32 << bits.clamp(8, 16)) - 1;
            }
            if let (Some(coder), Some(mosaic)) = (notes.coder, best_mosaic.as_mut()) {
                if matches!(mosaic.scheme, MosaicScheme::Olympus) {
                    mosaic.scheme =
                        MosaicScheme::Olympus14(Box::new(crate::codecs::olympus14::Olympus14 {
                            tags: coder,
                            valid_bits: notes.valid_bits.unwrap_or(12),
                        }));
                }
            }
            if let (Some([left, top, width, height]), Some(mosaic)) =
                (notes.crop, best_mosaic.as_mut())
            {
                if width > 0
                    && height > 0
                    && left + width <= mosaic.width
                    && top + height <= mosaic.height
                {
                    mosaic.active_area = Some([top, left, top + height, left + width]);
                    meta.cfa = meta.cfa.shifted(left, top);
                }
            }
        }
    }

    if format == RawFormat::Nef {
        let bits = best_mosaic
            .as_ref()
            .map_or(14, |mosaic| mosaic.bits_per_sample);
        let (white_balance, black) = nikon_levels(bytes, &file, bits);
        if let Some(white_balance) = white_balance {
            meta.as_shot_neutral = white_balance;
        }
        if let Some(black) = black {
            meta.black_level = black;
            meta.white_level = meta.white_level.max(black + 1);
        }
    }

    // Nikon's COOLPIX P and B bodies pack twelve-bit samples in 32-bit words.
    if let Some(mosaic) = best_mosaic.as_mut() {
        let model = meta.model.trim_start_matches("NIKON ").to_string();
        if format == RawFormat::Nef
            && mosaic.scheme == MosaicScheme::Packed
            && mosaic.bits_per_sample == 12
            && ((model.starts_with("COOLPIX P") && mosaic.width != 4032)
                || model.starts_with("COOLPIX B700"))
        {
            mosaic.scheme = MosaicScheme::PackedWords;
        }
        // A little-endian NEF packs its uncompressed samples the same way round.
        if format == RawFormat::Nef
            && mosaic.compression == private_compression::NIKON
            && mosaic.scheme == MosaicScheme::Packed
            && mosaic.bits_per_sample == 12
            && file.little()
        {
            mosaic.scheme = MosaicScheme::PackedLsb;
        }
        // A file that states no white level clips where its samples run out.
        if file.find(tiff::tag::WHITE_LEVEL).is_none()
            && matches!(format, RawFormat::Nef)
            && matches!(
                mosaic.scheme,
                MosaicScheme::Packed
                    | MosaicScheme::PackedLsb
                    | MosaicScheme::PackedWords
                    | MosaicScheme::NikonPacked14
            )
            && matches!(mosaic.bits_per_sample, 10 | 12 | 14)
        {
            meta.white_level = (1u32 << mosaic.bits_per_sample) - 1;
        }
        if mosaic.active_area.is_none() && mosaic.default_crop.is_none() {
            if let Some(area) = vendor_trim(&meta.make, &meta.model, mosaic.width, mosaic.height) {
                mosaic.active_area = Some(area);
                meta.cfa = meta.cfa.shifted(area[1], area[0]);
            }
        }
    }

    meta.mosaic = best_mosaic;
    sort_previews(&mut meta);

    if meta.previews.is_empty() && meta.mosaic.is_none() {
        return Err(corrupt(
            "tiff container holds neither a preview nor a mosaic",
        ));
    }
    Ok(meta)
}

/// Decide which decoder a mosaic needs.
///
/// The declared compression code is the first input, but not the only one: a
/// file that stores fewer bytes than its declared bit depth would need is
/// compressed whatever it says, and that is exactly how Olympus marks its own
/// scheme.
fn scheme_for(
    compression: u16,
    format: RawFormat,
    pixels: usize,
    stored_bytes: usize,
    bits: u16,
    nikon: Option<NikonTable<'_>>,
    sony_curve: Option<[u16; 4]>,
) -> MosaicScheme {
    match compression {
        tiff::compression::NONE => {
            // An ORF that declares no compression but stores fewer bytes than
            // its own bit depth needs is compressed whatever it says: that is
            // how Olympus marks its scheme, and it is the only file in the
            // world that lies this particular way.
            if format == RawFormat::Orf && stored_bytes * 8 < pixels * usize::from(bits.max(1)) {
                MosaicScheme::Olympus
            } else {
                MosaicScheme::Packed
            }
        }
        tiff::compression::JPEG | tiff::compression::JPEG_OLD if format == RawFormat::Arw => {
            MosaicScheme::SonyLossless
        }
        tiff::compression::JPEG => MosaicScheme::LosslessJpeg,
        // Nikon declares its own compression even for uncompressed data; the
        // stored size is what tells the two apart.
        private_compression::NIKON if stored_bytes * 2 == pixels * 3 => MosaicScheme::Packed,
        private_compression::NIKON => match nikon.and_then(|table| table.read(bits as u8)) {
            Some(params) => MosaicScheme::Nikon(Box::new(params)),
            // A compressed NEF whose decode table we could not read is not
            // guessable: the curve is per body. Say so rather than render it
            // through an identity curve that would be quietly wrong.
            None => MosaicScheme::Unsupported(compression),
        },
        private_compression::SONY if stored_bytes == pixels => {
            MosaicScheme::SonyArw2 { curve: sony_curve }
        }
        private_compression::SONY if stored_bytes * 8 == pixels * usize::from(bits.max(1)) => {
            MosaicScheme::Packed
        }
        other => MosaicScheme::Unsupported(other),
    }
}

#[allow(clippy::too_many_lines)]
fn mosaic_from_ifd(
    file: &tiff::TiffFile<'_>,
    ifd: &tiff::Ifd,
    compression: u16,
    format: RawFormat,
    nikon: Option<NikonTable<'_>>,
    sony_curve: Option<[u16; 4]>,
) -> Option<MosaicRef> {
    let width = ifd
        .get(tiff::tag::IMAGE_WIDTH)
        .and_then(|entry| file.u64s(entry).first().copied())? as u32;
    let height = ifd
        .get(tiff::tag::IMAGE_LENGTH)
        .and_then(|entry| file.u64s(entry).first().copied())? as u32;
    if width == 0 || height == 0 {
        return None;
    }
    let bits = ifd
        .get(tiff::tag::BITS_PER_SAMPLE)
        .and_then(|entry| file.u64s(entry).first().copied())
        .unwrap_or(16) as u16;

    let tiled = ifd.get(tiff::tag::TILE_OFFSETS).is_some();
    let (offsets_tag, counts_tag) = if tiled {
        (tiff::tag::TILE_OFFSETS, tiff::tag::TILE_BYTE_COUNTS)
    } else {
        (tiff::tag::STRIP_OFFSETS, tiff::tag::STRIP_BYTE_COUNTS)
    };
    let tile = if tiled {
        let dimension = |tag| {
            ifd.get(tag)
                .and_then(|entry| file.u64s(entry).first().copied())
                .and_then(|v| u32::try_from(v).ok())
                .filter(|v| *v > 0)
        };
        Some((
            dimension(tiff::tag::TILE_WIDTH)?,
            dimension(tiff::tag::TILE_LENGTH)?,
        ))
    } else {
        None
    };
    let offsets = file.u64s(ifd.get(offsets_tag)?);
    let counts = ifd
        .get(counts_tag)
        .map(|entry| file.u64s(entry))
        .unwrap_or_default();
    if offsets.is_empty() {
        return None;
    }
    let segments: Vec<(usize, usize)> = offsets
        .iter()
        .enumerate()
        .map(|(index, at)| {
            (
                *at as usize,
                counts.get(index).copied().unwrap_or(0) as usize,
            )
        })
        .collect();

    let rational = |tag| {
        ifd.get(tag)
            .map(|entry| file.f64s(entry))
            .unwrap_or_default()
    };
    let whole = |v: f64| {
        if v.is_finite() && v >= 0.0 {
            v.round() as u32
        } else {
            0
        }
    };
    let active_area = match rational(tiff::tag::ACTIVE_AREA).as_slice() {
        [top, left, bottom, right]
            if whole(*bottom) > whole(*top)
                && whole(*right) > whole(*left)
                && whole(*bottom) <= height
                && whole(*right) <= width =>
        {
            Some([whole(*top), whole(*left), whole(*bottom), whole(*right)])
        }
        _ => None,
    };
    let default_crop = match (
        rational(tiff::tag::DEFAULT_CROP_ORIGIN).as_slice(),
        rational(tiff::tag::DEFAULT_CROP_SIZE).as_slice(),
    ) {
        ([x, y], [w, h]) if whole(*w) > 0 && whole(*h) > 0 => {
            Some([whole(*x), whole(*y), whole(*w), whole(*h)])
        }
        _ => None,
    };

    let linearization = ifd
        .get(tiff::tag::LINEARIZATION_TABLE)
        .map(|entry| file.u64s(entry))
        .filter(|table| table.len() >= 2)
        .map(|table| {
            table
                .into_iter()
                .map(|v| u16::try_from(v).unwrap_or(u16::MAX))
                .collect::<Vec<u16>>()
        });

    let rows_per_strip = ifd
        .get(tiff::tag::ROWS_PER_STRIP)
        .and_then(|entry| file.u64s(entry).first().copied())
        .unwrap_or(u64::from(height)) as u32;

    let pixels = width as usize * height as usize;
    let stored_bytes = segments.iter().map(|(_, len)| *len).sum::<usize>();
    let nikon_packed14 = compression == private_compression::NIKON
        && bits == 14
        && stored_bytes == (width as usize * 7 / 4).div_ceil(16) * 16 * height as usize;
    // Twelve- and fourteen-bit samples are often stored one per 16-bit word -
    // every uncompressed Sony ARW and Nikon NEF does it. The declared depth is
    // the sensor's, not the container's, and unpacking the words as a packed
    // fourteen-bit stream scrambles the whole frame.
    let bits = if compression == tiff::compression::NONE && bits < 16 && stored_bytes >= pixels * 2
    {
        16
    } else {
        bits
    };
    let scheme = scheme_for(
        compression,
        format,
        pixels,
        stored_bytes,
        bits,
        nikon,
        sony_curve,
    );

    let scheme = if nikon_packed14 {
        MosaicScheme::NikonPacked14
    } else {
        scheme
    };
    Some(MosaicRef {
        width,
        height,
        bits_per_sample: bits,
        compression,
        scheme,
        segments,
        rows_per_strip: rows_per_strip.max(1),
        tile,
        active_area,
        default_crop,
        linearization,
    })
}

/// Where Nikon's decode table sits, and which byte order to read it in.
///
/// Located separately from being read, because the sample precision that picks
/// the Huffman tree is a property of the *image* directory, and the MakerNote is
/// usually found before that directory is.
#[derive(Debug, Clone, Copy)]
struct NikonTable<'a> {
    bytes: &'a [u8],
    at: usize,
    little: bool,
}

impl NikonTable<'_> {
    fn read(self, bits: u8) -> Option<crate::codecs::nikon::NikonParams> {
        crate::codecs::nikon::read_params(self.bytes, self.at, self.little, bits).ok()
    }
}

/// Find Nikon's decode table inside the MakerNote.
///
/// The MakerNote is a complete TIFF of its own, ten bytes into the entry, with
/// its own byte order - which is often the opposite of the container's. Getting
/// that wrong produces a decode table full of plausible nonsense, so the header
/// is read from the MakerNote itself rather than inherited.
fn nikon_table<'a>(bytes: &'a [u8], file: &tiff::TiffFile<'a>) -> Option<NikonTable<'a>> {
    let entry = file.find(tiff::tag::MAKER_NOTE)?;
    let start = file.payload_at(entry)?;
    if bytes.get(start..start + 6) != Some(b"Nikon\0") {
        return None;
    }
    let inner = tiff::TiffFile::parse_at(bytes, start + 10).ok()?;
    // 0x0096 on current bodies, 0x008C on the ones that predate it.
    let table = inner
        .find(nikon_tag::DECODE_TABLE_2)
        .or_else(|| inner.find(nikon_tag::DECODE_TABLE_1))?;
    Some(NikonTable {
        bytes,
        at: inner.payload_at(table)?,
        little: inner.little(),
    })
}

/// What an Olympus MakerNote's ImageProcessing directory says.
struct OlympusNotes {
    as_shot: Option<[f32; 3]>,
    black: Option<u32>,
    valid_bits: Option<u32>,
    crop: Option<[u32; 4]>,
    /// Tags `0x0640` to `0x0653`, when the file carries the coder's constants.
    coder: Option<[u32; 20]>,
}

/// Read Olympus's ImageProcessing directory (`0x2040`): white balance
/// (`0x0100`, red and blue over 256), per-channel black (`0x0600`), the bits a
/// sample really uses (`0x0611`) and the picture inside the sensor
/// (`0x0612`-`0x0615`). Current bodies write `OLYMPUS\0` and a byte-order mark,
/// with offsets counted from the MakerNote itself; older ones write `OLYMP\0`
/// with offsets from the file.
fn olympus_notes(bytes: &[u8], file: &tiff::TiffFile<'_>) -> Option<OlympusNotes> {
    let start = file
        .find(tiff::tag::MAKER_NOTE)
        .and_then(|entry| file.payload_at(entry))?;
    let (notes, directory) = if bytes.get(start..start + 10) == Some(b"OM SYSTEM\0") {
        let little = bytes.get(start + 12..start + 14) == Some(b"II");
        (tiff::TiffFile::bare(bytes, start, little), start + 16)
    } else if bytes.get(start..start + 8) == Some(b"OLYMPUS\0") {
        let little = bytes.get(start + 8..start + 10) == Some(b"II");
        (tiff::TiffFile::bare(bytes, start, little), start + 12)
    } else if bytes.get(start..start + 6) == Some(b"OLYMP\0") {
        (
            tiff::TiffFile::bare(bytes, file.base(), file.little()),
            start + 8,
        )
    } else {
        return None;
    };
    let top = notes.directory_at(directory)?;
    let pointer = top.get(0x2040).and_then(|entry| notes.raw_u32(entry))?;
    let ifd = notes.directory_at(notes.base() + pointer as usize)?;
    let values = |tag: u16| {
        ifd.get(tag)
            .map(|entry| notes.u64s(entry))
            .unwrap_or_default()
    };
    let first = |tag: u16| values(tag).first().map(|v| *v as u32);
    let as_shot = match values(0x0100).as_slice() {
        [red, blue, ..] if *red > 0 && *blue > 0 => {
            Some([*red as f32 / 256.0, 1.0, *blue as f32 / 256.0])
        }
        _ => None,
    };
    let black = Some(values(0x0600))
        .filter(|levels| levels.len() >= 4)
        .map(|levels| (levels.iter().take(4).sum::<u64>() / 4) as u32);
    let crop = match (first(0x0612), first(0x0613), first(0x0614), first(0x0615)) {
        (Some(left), Some(top), Some(width), Some(height)) => Some([left, top, width, height]),
        _ => None,
    };
    let mut coder = [0u32; 20];
    for (i, slot) in coder.iter_mut().enumerate() {
        *slot = first(0x0640 + i as u16).unwrap_or(0);
    }
    Some(OlympusNotes {
        as_shot,
        black,
        valid_bits: first(0x0611).filter(|bits| (8..=16).contains(bits)),
        crop,
        coder: (coder[13] > 0).then_some(coder),
    })
}

/// Nikon's as-shot white balance and black level, from the MakerNote.
///
/// `0x000C` is four rationals - red, blue and the two greens - and `0x003D` the
/// four per-channel black levels, which the camera states at fourteen bits even
/// in a twelve-bit file.
fn nikon_levels(
    bytes: &[u8],
    file: &tiff::TiffFile<'_>,
    bits: u16,
) -> (Option<[f32; 3]>, Option<u32>) {
    let inner = file
        .find(tiff::tag::MAKER_NOTE)
        .and_then(|entry| file.payload_at(entry))
        .filter(|start| bytes.get(*start..start + 6) == Some(b"Nikon\0"))
        .and_then(|start| tiff::TiffFile::parse_at(bytes, start + 10).ok());
    let Some(inner) = inner else {
        return (None, None);
    };
    let white_balance = inner
        .find(nikon_tag::WB_LEVELS)
        .filter(|entry| entry.count == 4)
        .map(|entry| inner.f64s(entry))
        .and_then(|levels| match levels.as_slice() {
            [red, blue, green, ..] if *red > 0.0 && *blue > 0.0 => {
                let green = if *green > 0.0 { *green } else { 1.0 };
                Some([(red / green) as f32, 1.0, (blue / green) as f32])
            }
            _ => None,
        });
    let black = inner
        .find(nikon_tag::BLACK_LEVEL)
        .map(|entry| inner.u64s(entry))
        .filter(|levels| levels.len() >= 4)
        .map(|levels| (levels.iter().take(4).sum::<u64>() / 4) as u32)
        .map(|level| if bits == 12 { level / 4 } else { level })
        .filter(|level| *level > 0);
    (white_balance, black)
}

/// Nikon MakerNote tags this build reads.
mod nikon_tag {
    /// The older decode table.
    pub(super) const DECODE_TABLE_1: u16 = 0x008C;
    /// The current decode table: Huffman version, predictors and curve.
    pub(super) const DECODE_TABLE_2: u16 = 0x0096;
    /// As-shot white balance: red, blue and the two greens, as rationals.
    pub(super) const WB_LEVELS: u16 = 0x000C;
    /// The four per-channel black levels.
    pub(super) const BLACK_LEVEL: u16 = 0x003D;
}

/// Read the four knots of Sony's linearisation curve, when the file exposes
/// them outside its encrypted sub-directory.
///
/// Returning `None` is not a failure: it means the render will use a plain
/// linear expansion and be flagged as approximate, which is the honest
/// alternative to inventing a curve we cannot verify.
fn sony_curve(file: &tiff::TiffFile<'_>) -> Option<[u16; 4]> {
    const SONY_TONE_CURVE: u16 = 0x7010;
    let entry = file.find(SONY_TONE_CURVE)?;
    let values = file.u64s(entry);
    if values.len() < 4 {
        return None;
    }
    let mut knots = [0u16; 4];
    for (slot, value) in knots.iter_mut().zip(values.iter()) {
        *slot = u16::try_from((*value >> 2) & 0x0FFF).unwrap_or(0);
    }
    // The knots must be strictly increasing or the curve is not a curve.
    if knots
        .windows(2)
        .any(|pair| pair.first().copied().unwrap_or(0) >= pair.get(1).copied().unwrap_or(0))
    {
        return None;
    }
    Some(knots)
}

/// Read the tags every container spells the same way.
fn apply_common_tags(file: &tiff::TiffFile<'_>, meta: &mut RawMeta) {
    if let Some(make) = file.text(tiff::tag::MAKE) {
        meta.make = make;
    }
    if let Some(model) = file.text(tiff::tag::MODEL) {
        meta.model = model;
    }
    if let Some(orientation) = file.scalar(tiff::tag::ORIENTATION) {
        if (1..=8).contains(&orientation) {
            meta.orientation = orientation as u16;
        }
    }
    // ColorMatrix2 is the D65-ish illuminant and is preferred when present,
    // because weddings are mostly not lit by tungsten.
    if let Some(matrix) = file
        .matrix3(tiff::tag::COLOR_MATRIX_2)
        .or_else(|| file.matrix3(tiff::tag::COLOR_MATRIX_1))
    {
        meta.colour_matrix = Some(matrix);
    }
    if let Some(entry) = file.find(tiff::tag::AS_SHOT_NEUTRAL) {
        let values = file.f64s(entry);
        if values.len() >= 3 {
            // DNG stores the *camera-native reading of a neutral subject*, so
            // the multipliers that make that subject neutral are its
            // reciprocals. Getting this backwards tints every image by the
            // square of the true cast, which looks plausible enough on a
            // daylight frame to survive review - hence the explicit note.
            let green = values.get(1).copied().unwrap_or(1.0);
            let scale = if green.abs() > 1e-9 { green } else { 1.0 };
            let neutral_r = values.first().copied().unwrap_or(1.0) / scale;
            let neutral_b = values.get(2).copied().unwrap_or(1.0) / scale;
            let reciprocal = |value: f64| -> f32 {
                if value.abs() > 1e-9 {
                    (1.0 / value) as f32
                } else {
                    1.0
                }
            };
            meta.as_shot_neutral = [reciprocal(neutral_r), 1.0, reciprocal(neutral_b)];
        }
    }
    if let Some(entry) = file.find(tiff::tag::BLACK_LEVEL) {
        // DNG allows one black level per CFA position, stored as rationals, plus per-row and
        // per-column offsets. Adobe DNG Converter writes a zero here and the real black -
        // about 1,024 codes on a 14-bit Canon - into BlackLevelDeltaV, so reading only the
        // first value left every converted Canon DNG with a lifted, magenta shadow haze.
        let mean = |values: Vec<f64>| {
            if values.is_empty() {
                0.0
            } else {
                values.iter().sum::<f64>() / values.len() as f64
            }
        };
        let base = mean(file.f64s(entry));
        let rows = file
            .find(tiff::tag::BLACK_LEVEL_DELTA_V)
            .map_or(0.0, |e| mean(file.f64s(e)));
        let columns = file
            .find(tiff::tag::BLACK_LEVEL_DELTA_H)
            .map_or(0.0, |e| mean(file.f64s(e)));
        let black = base + rows + columns;
        if black.is_finite() && black >= 0.0 {
            meta.black_level = black.round() as u32;
        }
    }
    if let Some(white) = file.scalar(tiff::tag::WHITE_LEVEL) {
        if white > 0 {
            meta.white_level = white as u32;
        }
    }
    if let Some(entry) = file.find(0xA302) {
        let codes = file.u64s(entry);
        if codes.len() == 8 {
            meta.cfa = CfaPattern::from_codes(codes.get(4..).unwrap_or(&[]));
        }
    }
    if let Some(entry) = file.find(tiff::tag::CFA_PATTERN) {
        let codes = file.u64s(entry);
        // EXIF's spelling prefixes the pattern with its two-byte dimensions;
        // DNG's does not, and a 6x6 X-Trans pattern is 36 codes either way.
        let pattern = if codes.len() == 36 || codes.len() == 4 {
            codes
        } else if codes.len() >= 6 {
            codes.get(2..).map(<[u64]>::to_vec).unwrap_or_default()
        } else {
            codes
        };
        meta.cfa = CfaPattern::from_codes(&pattern);
    }
}

fn push_preview(bytes: &[u8], at: usize, len: usize, meta: &mut RawMeta) {
    if at == 0 || len == 0 {
        return;
    }
    let Some(stream) = bytes.get(at..at.saturating_add(len)) else {
        return;
    };
    let Ok(info) = jpeg::probe(stream) else {
        return;
    };
    // A lossless stream here is sensor data, not a preview.
    if info.is_lossless() {
        return;
    }
    meta.previews.push(PreviewRef {
        offset: at,
        len: info.byte_len.unwrap_or(len),
        width: info.width,
        height: info.height,
    });
}

fn sort_previews(meta: &mut RawMeta) {
    // Largest first, then by offset so the order is stable for identical sizes.
    meta.previews
        .sort_by(|a, b| b.pixels().cmp(&a.pixels()).then(a.offset.cmp(&b.offset)));
    meta.previews.dedup_by_key(|preview| preview.offset);
}
