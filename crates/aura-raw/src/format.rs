//! What kind of file is this, decided by its bytes.
//!
//! The extension is a hint, never evidence. Cards get renamed, converters rewrite
//! containers, and a recovery tool will happily write a text file called
//! `IMG_0421.CR2`. Ingest already classified files by extension for *pairing*;
//! the decoder classifies by magic for *decoding*, and the two disagreeing is
//! itself useful information.

use crate::container::{bmff, jpeg, raf, tiff};

/// The containers this build recognises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RawFormat {
    /// Adobe DNG. The best case: the file carries its own colour matrices.
    Dng,
    /// Canon CR2, a TIFF whose sensor data is lossless JPEG.
    Cr2,
    /// Canon CR3, an ISO base media container.
    Cr3,
    /// Nikon NEF.
    Nef,
    /// Sony ARW.
    Arw,
    /// Olympus ORF.
    Orf,
    /// Panasonic RW2.
    Rw2,
    /// Fujifilm RAF, X-Trans rather than Bayer.
    Raf,
    /// Pentax PEF.
    Pef,
    /// Samsung SRW.
    Srw,
    /// A plain TIFF, including RAW formats we have not learned to name.
    Tiff,
    /// A JPEG, which a photographer may well have shot alongside the RAWs.
    Jpeg,
    /// HEIF or HEIC.
    Heif,
    /// Nothing we recognise.
    Unknown,
}

impl RawFormat {
    /// Stable text for telemetry and the support matrix.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dng => "dng",
            Self::Cr2 => "cr2",
            Self::Cr3 => "cr3",
            Self::Nef => "nef",
            Self::Arw => "arw",
            Self::Orf => "orf",
            Self::Rw2 => "rw2",
            Self::Raf => "raf",
            Self::Pef => "pef",
            Self::Srw => "srw",
            Self::Tiff => "tiff",
            Self::Jpeg => "jpeg",
            Self::Heif => "heif",
            Self::Unknown => "unknown",
        }
    }

    /// True when the container is one of the TIFF-derived family, which is the
    /// question the metadata reader actually cares about.
    #[must_use]
    pub const fn is_tiff_family(self) -> bool {
        matches!(
            self,
            Self::Dng
                | Self::Cr2
                | Self::Nef
                | Self::Arw
                | Self::Orf
                | Self::Rw2
                | Self::Pef
                | Self::Srw
                | Self::Tiff
        )
    }

    /// True when this build can locate an embedded preview in the container.
    /// Tier 1 works for every one of these; see `docs/camera-support.md` for
    /// which of them also reach tier 2 from the mosaic.
    #[must_use]
    pub const fn has_embedded_preview(self) -> bool {
        !matches!(self, Self::Unknown)
    }
}

/// Decide the format from the first bytes of the file.
///
/// The TIFF family needs a second look: every one of them starts with the same
/// eight bytes, so the discriminator is a private magic, the DNG version tag or
/// the manufacturer string.
#[must_use]
pub fn sniff(bytes: &[u8]) -> RawFormat {
    if jpeg::looks_like_jpeg(bytes) {
        return RawFormat::Jpeg;
    }
    if raf::looks_like_raf(bytes) {
        return RawFormat::Raf;
    }
    if bmff::looks_like_bmff(bytes) {
        return match bmff::brand(bytes).as_ref() {
            Some(b"crx ") => RawFormat::Cr3,
            Some(b"heic" | b"heix" | b"mif1" | b"msf1" | b"avif") => RawFormat::Heif,
            _ => RawFormat::Unknown,
        };
    }

    let order = bytes.get(..2);
    let little = match order {
        Some(b"II") => true,
        Some(b"MM") => false,
        _ => return RawFormat::Unknown,
    };
    let magic = match (bytes.get(2).copied(), bytes.get(3).copied()) {
        (Some(a), Some(b)) if little => u16::from_le_bytes([a, b]),
        (Some(a), Some(b)) => u16::from_be_bytes([a, b]),
        _ => return RawFormat::Unknown,
    };

    match magic {
        // Panasonic keeps the TIFF layout but changes the version number.
        0x0055 => return RawFormat::Rw2,
        // Olympus does the same, with two different values across generations.
        0x4F52 | 0x5352 => return RawFormat::Orf,
        42 => {}
        _ => return RawFormat::Unknown,
    }

    // Canon CR2 writes its own magic straight after the TIFF header.
    if matches!(bytes.get(8..10), Some(b"CR")) {
        return RawFormat::Cr2;
    }

    let Ok(file) = tiff::TiffFile::parse(bytes) else {
        return RawFormat::Tiff;
    };
    if file.find(tiff::tag::DNG_VERSION).is_some() {
        return RawFormat::Dng;
    }

    match file.text(tiff::tag::MAKE).unwrap_or_default().to_ascii_uppercase() {
        make if make.starts_with("NIKON") => RawFormat::Nef,
        make if make.starts_with("SONY") => RawFormat::Arw,
        make if make.starts_with("OLYMPUS") || make.starts_with("OM ") => RawFormat::Orf,
        make if make.starts_with("PENTAX") || make.starts_with("RICOH") => RawFormat::Pef,
        make if make.starts_with("SAMSUNG") => RawFormat::Srw,
        make if make.starts_with("PANASONIC") => RawFormat::Rw2,
        _ => RawFormat::Tiff,
    }
}

/// How far up the pyramid a file can be taken from its sensor data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MosaicSupport {
    /// The mosaic can be decoded and demosaiced by this build.
    Decodable,
    /// The mosaic was located but its compression is not implemented; tier 2
    /// falls back to the embedded preview and raises `AURA-RAW-2007`.
    CompressionUnsupported,
    /// No colour filter array image was found in the container at all.
    NoMosaic,
}

/// Whether this build can decode a mosaic with the given TIFF compression code.
#[must_use]
pub const fn mosaic_support_for(compression: u16) -> MosaicSupport {
    match compression {
        tiff::compression::NONE | tiff::compression::JPEG => MosaicSupport::Decodable,
        _ => MosaicSupport::CompressionUnsupported,
    }
}
