//! JPEG marker walking: dimensions, the EXIF block and validity.
//!
//! We never hand a byte range to the JPEG decoder without walking its markers
//! first. A preview offset in a RAW file points into a buffer the camera wrote;
//! if the length tag is wrong - and on some bodies it is - the decoder would be
//! handed a truncated stream. Walking to the end-of-image marker gives the real
//! extent, and refusing a stream with no start-of-frame is how a garbage offset
//! becomes `AURA-RAW-2002` instead of a decoder crash.

use aura_core::errors::raw::corrupt;
use aura_core::AuraResult;

/// Start of image.
const SOI: u8 = 0xD8;
/// End of image.
const EOI: u8 = 0xD9;
/// Start of scan; entropy-coded data follows and markers stop being aligned.
const SOS: u8 = 0xDA;
/// Application segment 1, where EXIF lives.
const APP1: u8 = 0xE1;

/// What a JPEG stream declares about itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JpegInfo {
    /// Width in pixels from the start-of-frame header.
    pub width: u32,
    /// Height in pixels from the start-of-frame header.
    pub height: u32,
    /// Components: 1 for greyscale, 3 for colour.
    pub components: u8,
    /// Start-of-frame marker code, which distinguishes baseline (0xC0),
    /// progressive (0xC2) and lossless (0xC3) streams.
    pub sof: u8,
    /// Total length of the stream from `FFD8` to `FFD9` inclusive, when the end
    /// marker was found.
    pub byte_len: Option<usize>,
}

impl JpegInfo {
    /// True when this is the lossless (SOF3) profile cameras use for sensor
    /// data rather than for previews.
    #[must_use]
    pub const fn is_lossless(&self) -> bool {
        self.sof == 0xC3
    }
}

/// True when the buffer starts with a JPEG start-of-image marker.
#[must_use]
pub fn looks_like_jpeg(bytes: &[u8]) -> bool {
    matches!(bytes.get(..3), Some([0xFF, 0xD8, 0xFF]))
}

/// Walk the markers of a JPEG stream.
///
/// # Errors
///
/// Returns `AURA-RAW-2002` when the stream does not start with `FFD8`, when a
/// segment length is impossible, or when no start-of-frame header is present.
pub fn probe(bytes: &[u8]) -> AuraResult<JpegInfo> {
    if !matches!(bytes.get(..2), Some([0xFF, SOI])) {
        return Err(corrupt("jpeg stream does not start with FFD8"));
    }
    let mut at = 2usize;
    let mut info: Option<JpegInfo> = None;

    while at + 1 < bytes.len() {
        if bytes.get(at).copied() != Some(0xFF) {
            // Fill bytes and entropy data: resynchronise on the next marker.
            at += 1;
            continue;
        }
        let Some(marker) = bytes.get(at + 1).copied() else {
            break;
        };
        at += 2;
        match marker {
            // Padding, restart and standalone markers carry no payload.
            0xFF | 0x01 | 0xD0..=0xD7 | SOI => continue,
            EOI => {
                if let Some(found) = info.as_mut() {
                    found.byte_len = Some(at);
                }
                break;
            }
            _ => {}
        }

        let length = match (bytes.get(at).copied(), bytes.get(at + 1).copied()) {
            (Some(high), Some(low)) => usize::from(u16::from_be_bytes([high, low])),
            _ => break,
        };
        if length < 2 || at + length > bytes.len() {
            return Err(corrupt(format!(
                "jpeg segment {marker:#04X} declares {length} bytes past the end"
            )));
        }

        // Start-of-frame markers: C0-CF except the four that are not frames.
        let is_sof = (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC);
        if is_sof && info.is_none() {
            let height = read_u16(bytes, at + 3);
            let width = read_u16(bytes, at + 5);
            let components = bytes.get(at + 7).copied().unwrap_or(0);
            if width == 0 || height == 0 {
                return Err(corrupt("jpeg frame declares a zero dimension"));
            }
            info = Some(JpegInfo {
                width: u32::from(width),
                height: u32::from(height),
                components,
                sof: marker,
                byte_len: None,
            });
        }

        if marker == SOS {
            // Entropy-coded data follows. Scan for the end marker rather than
            // trying to interpret it, skipping the stuffed FF00 pairs and the
            // restart markers that may appear inside the scan.
            let mut scan = at + length;
            while scan + 1 < bytes.len() {
                if bytes.get(scan).copied() == Some(0xFF) {
                    match bytes.get(scan + 1).copied() {
                        Some(0x00 | 0xFF) => scan += 2,
                        Some(value) if (0xD0..=0xD7).contains(&value) => scan += 2,
                        Some(EOI) => {
                            if let Some(found) = info.as_mut() {
                                found.byte_len = Some(scan + 2);
                            }
                            scan += 2;
                            break;
                        }
                        _ => break,
                    }
                } else {
                    scan += 1;
                }
            }
            at = scan;
            continue;
        }

        at += length;
    }

    info.ok_or_else(|| corrupt("jpeg stream has no start-of-frame header"))
}

/// Extract the TIFF block from an `Exif\0\0` APP1 segment, if there is one.
///
/// Returns the offset of the TIFF header inside `bytes`, which is what
/// [`crate::container::tiff::TiffFile::parse_at`] expects.
#[must_use]
pub fn exif_tiff_offset(bytes: &[u8]) -> Option<usize> {
    let mut at = 2usize;
    while at + 3 < bytes.len() {
        if bytes.get(at).copied() != Some(0xFF) {
            at += 1;
            continue;
        }
        let marker = bytes.get(at + 1).copied()?;
        if marker == SOS || marker == EOI {
            return None;
        }
        let length = usize::from(u16::from_be_bytes([
            bytes.get(at + 2).copied()?,
            bytes.get(at + 3).copied()?,
        ]));
        if length < 2 {
            return None;
        }
        if marker == APP1 {
            let payload = at + 4;
            if bytes.get(payload..payload + 6) == Some(b"Exif\0\0") {
                return Some(payload + 6);
            }
        }
        at += 2 + length;
    }
    None
}

/// Find the first complete JPEG stream inside a buffer, which is how previews
/// are located in containers that do not declare their length reliably.
#[must_use]
pub fn find_stream(bytes: &[u8]) -> Option<(usize, usize)> {
    let mut at = 0usize;
    while at + 2 < bytes.len() {
        if matches!(bytes.get(at..at + 3), Some([0xFF, 0xD8, 0xFF])) {
            if let Some(rest) = bytes.get(at..) {
                if let Ok(info) = probe(rest) {
                    let len = info.byte_len.unwrap_or(rest.len());
                    return Some((at, len));
                }
            }
        }
        at += 1;
    }
    None
}

fn read_u16(bytes: &[u8], at: usize) -> u16 {
    match (bytes.get(at).copied(), bytes.get(at + 1).copied()) {
        (Some(high), Some(low)) => u16::from_be_bytes([high, low]),
        _ => 0,
    }
}
