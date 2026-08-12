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

/// Where the sensor mosaic lives and how it is encoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MosaicRef {
    /// Width of the mosaic in sensor pixels.
    pub width: u32,
    /// Height of the mosaic in sensor pixels.
    pub height: u32,
    /// Bits in each sample as declared by the file, 8 to 16.
    pub bits_per_sample: u16,
    /// TIFF compression code.
    pub compression: u16,
    /// Strips or tiles, as `(offset, length)` pairs in file order.
    pub segments: Vec<(usize, usize)>,
    /// Rows in each strip; equals `height` when the image is a single strip.
    pub rows_per_strip: u32,
}

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
    /// Not a 2x2 Bayer array - X-Trans, or a pattern we did not recognise.
    Unknown,
}

impl CfaPattern {
    /// Which colour plane the pixel at `(x, y)` belongs to: 0 red, 1 green,
    /// 2 blue. Unknown patterns are treated as green, which is neutral.
    #[must_use]
    pub const fn colour_at(self, x: u32, y: u32) -> u8 {
        let even_row = y % 2 == 0;
        let even_column = x % 2 == 0;
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
            Self::Unknown => 1,
        }
    }

    /// Stable text for the sidecar.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rggb => "rggb",
            Self::Bggr => "bggr",
            Self::Grbg => "grbg",
            Self::Gbrg => "gbrg",
            Self::Unknown => "unknown",
        }
    }

    fn from_codes(codes: &[u64]) -> Self {
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
            Some(mosaic) => crate::format::mosaic_support_for(mosaic.compression),
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
        RawFormat::Raf => read_raf(bytes),
        RawFormat::Cr3 => read_cr3(bytes),
        format if format.is_tiff_family() => read_tiff_family(bytes, format),
        RawFormat::Heif => Err(unsupported(
            path,
            "HEIF decoding is not part of this build; see docs/camera-support.md",
        )),
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
    // X-Trans is not a 2x2 array, and this build does not demosaic it.
    meta.cfa = CfaPattern::Unknown;

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

fn read_cr3(bytes: &[u8]) -> AuraResult<RawMeta> {
    let boxes = bmff::walk(bytes)?;
    let mut meta = default_meta(RawFormat::Cr3);

    // CMT1 is IFD0 in TIFF form: make, model and orientation live there.
    if let Some(cmt1) = bmff::find(&boxes, b"CMT1") {
        if let Ok(file) = tiff::TiffFile::parse_at(bytes, cmt1.start) {
            apply_common_tags(&file, &mut meta);
        }
    }

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

    if meta.previews.is_empty() {
        return Err(corrupt("cr3 container carries no readable preview"));
    }
    sort_previews(&mut meta);
    Ok(meta)
}

fn read_tiff_family(bytes: &[u8], format: RawFormat) -> AuraResult<RawMeta> {
    let file = tiff::TiffFile::parse(bytes)?;
    let mut meta = default_meta(format);
    meta.little_endian = file.little();
    apply_common_tags(&file, &mut meta);

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

        // Photometric 32803 is "colour filter array": the sensor itself.
        if photometric == Some(32_803) {
            if let Some(mosaic) = mosaic_from_ifd(&file, ifd, compression) {
                let better = best_mosaic
                    .as_ref()
                    .is_none_or(|current| mosaic.width * mosaic.height > current.width * current.height);
                if better {
                    best_mosaic = Some(mosaic);
                }
            }
        }
    }

    meta.mosaic = best_mosaic;
    sort_previews(&mut meta);

    if meta.previews.is_empty() && meta.mosaic.is_none() {
        return Err(corrupt("tiff container holds neither a preview nor a mosaic"));
    }
    Ok(meta)
}

fn mosaic_from_ifd(file: &tiff::TiffFile<'_>, ifd: &tiff::Ifd, compression: u16) -> Option<MosaicRef> {
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

    let (offsets_tag, counts_tag) = if ifd.get(tiff::tag::TILE_OFFSETS).is_some() {
        (tiff::tag::TILE_OFFSETS, tiff::tag::TILE_BYTE_COUNTS)
    } else {
        (tiff::tag::STRIP_OFFSETS, tiff::tag::STRIP_BYTE_COUNTS)
    };
    let offsets = file.u64s(ifd.get(offsets_tag)?);
    let counts = ifd
        .get(counts_tag)
        .map(|entry| file.u64s(entry))
        .unwrap_or_default();
    if offsets.is_empty() {
        return None;
    }
    let segments = offsets
        .iter()
        .enumerate()
        .map(|(index, at)| (*at as usize, counts.get(index).copied().unwrap_or(0) as usize))
        .collect();

    let rows_per_strip = ifd
        .get(tiff::tag::ROWS_PER_STRIP)
        .and_then(|entry| file.u64s(entry).first().copied())
        .unwrap_or(u64::from(height)) as u32;

    Some(MosaicRef {
        width,
        height,
        bits_per_sample: bits,
        compression,
        segments,
        rows_per_strip: rows_per_strip.max(1),
    })
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
    if let Some(black) = file.scalar(tiff::tag::BLACK_LEVEL) {
        meta.black_level = black as u32;
    }
    if let Some(white) = file.scalar(tiff::tag::WHITE_LEVEL) {
        if white > 0 {
            meta.white_level = white as u32;
        }
    }
    if let Some(entry) = file.find(tiff::tag::CFA_PATTERN) {
        let codes = file.u64s(entry);
        // Some writers prefix the pattern with its two-byte dimensions.
        let pattern = if codes.len() >= 6 {
            codes.get(2..6).map(<[u64]>::to_vec).unwrap_or_default()
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
