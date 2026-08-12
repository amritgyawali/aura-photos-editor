//! Where a cached entry lives, and why it lives there.
//!
//! Three properties are designed into the path and nothing else has to enforce
//! them:
//!
//! * **Content addressing.** The name is the BLAKE3 of the source file, so two
//!   copies of the same frame on two cards share one cached preview, and a file
//!   that moves or is renamed keeps its cache.
//! * **Sharding.** Forty thousand files in one directory makes every operation
//!   on that directory slow on every filesystem we ship to. The first two hex
//!   characters become a subdirectory, which spreads a wedding over 256 of them.
//! * **Versioning in the name.** The pipeline version is part of the file name,
//!   so bumping it invalidates every proxy at once without deleting anything by
//!   hand - and lets an old and a new pipeline coexist while a re-render runs.

use std::path::{Path, PathBuf};

/// Which artefact of a photograph an entry holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Artefact {
    /// Tier 1 thumbnail, JPEG.
    Thumb,
    /// Tier 2 proxy, 8-bit sRGB, JPEG.
    Proxy,
    /// Tier 2 proxy, 16-bit linear Rec.2020, raw samples with a small header.
    Linear,
    /// The per-image metadata sidecar, JSON.
    Meta,
}

impl Artefact {
    /// The infix used in the file name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Thumb => "thumb",
            Self::Proxy => "proxy",
            Self::Linear => "linear",
            Self::Meta => "meta",
        }
    }

    /// The extension, chosen so a support engineer can open the file directly.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Thumb | Self::Proxy => "jpg",
            Self::Linear => "bin",
            Self::Meta => "json",
        }
    }

    /// Every artefact kind, in a stable order.
    #[must_use]
    pub const fn all() -> [Self; 4] {
        [Self::Thumb, Self::Proxy, Self::Linear, Self::Meta]
    }
}

/// The identity of one cached entry.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CacheKey {
    /// BLAKE3 of the source file, lower-case hex.
    pub content_hash: String,
    /// Rendering pipeline version.
    pub pipeline_ver: u32,
    /// Which artefact.
    pub artefact: Artefact,
}

impl CacheKey {
    /// Build a key, normalising the hash to lower case.
    #[must_use]
    pub fn new(content_hash: &str, pipeline_ver: u32, artefact: Artefact) -> Self {
        Self {
            content_hash: content_hash.trim().to_ascii_lowercase(),
            pipeline_ver,
            artefact,
        }
    }

    /// The file name, without any directory.
    #[must_use]
    pub fn file_name(&self) -> String {
        format!(
            "{}.p{}.{}.{}",
            self.content_hash,
            self.pipeline_ver,
            self.artefact.as_str(),
            self.artefact.extension()
        )
    }

    /// The two-character shard this entry belongs in.
    #[must_use]
    pub fn shard(&self) -> String {
        self.content_hash.chars().take(2).collect::<String>()
    }

    /// The full path under a cache root.
    #[must_use]
    pub fn path_under(&self, root: &Path) -> PathBuf {
        root.join(self.shard()).join(self.file_name())
    }

    /// The stable text form used as the index key.
    #[must_use]
    pub fn id(&self) -> String {
        self.file_name()
    }
}

/// Recover a key from a file name written by [`CacheKey::file_name`].
///
/// Returns `None` for anything that does not match the pattern, which is how a
/// stray file dropped into the cache directory is ignored rather than trusted.
#[must_use]
pub fn parse_file_name(name: &str) -> Option<CacheKey> {
    let mut parts = name.split('.');
    let hash = parts.next()?;
    let version = parts.next()?.strip_prefix('p')?.parse::<u32>().ok()?;
    let artefact = match parts.next()? {
        "thumb" => Artefact::Thumb,
        "proxy" => Artefact::Proxy,
        "linear" => Artefact::Linear,
        "meta" => Artefact::Meta,
        _ => return None,
    };
    if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(CacheKey::new(hash, version, artefact))
}

/// The header written in front of a 16-bit linear buffer, so a `.bin` file is
/// self-describing rather than a naked array of samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinearHeader {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// Magic bytes at the start of a linear buffer file.
pub const LINEAR_MAGIC: &[u8; 4] = b"AURL";

/// Serialise a 16-bit linear buffer with its header, little-endian throughout.
#[must_use]
pub fn encode_linear(header: LinearHeader, samples: &[u16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + samples.len() * 2);
    out.extend_from_slice(LINEAR_MAGIC);
    out.extend_from_slice(&header.width.to_le_bytes());
    out.extend_from_slice(&header.height.to_le_bytes());
    for sample in samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

/// Parse a buffer written by [`encode_linear`].
///
/// Returns `None` when the magic, the header or the length is wrong - all three
/// of which mean the file is not ours and must be rebuilt rather than trusted.
#[must_use]
pub fn decode_linear(bytes: &[u8]) -> Option<(LinearHeader, Vec<u16>)> {
    if bytes.get(..4) != Some(LINEAR_MAGIC) {
        return None;
    }
    let width = u32::from_le_bytes([
        *bytes.get(4)?,
        *bytes.get(5)?,
        *bytes.get(6)?,
        *bytes.get(7)?,
    ]);
    let height = u32::from_le_bytes([
        *bytes.get(8)?,
        *bytes.get(9)?,
        *bytes.get(10)?,
        *bytes.get(11)?,
    ]);
    let expected = width as usize * height as usize * 3;
    let payload = bytes.get(12..)?;
    if payload.len() != expected * 2 {
        return None;
    }
    let samples = payload
        .chunks_exact(2)
        .map(|pair| {
            u16::from_le_bytes([
                pair.first().copied().unwrap_or(0),
                pair.get(1).copied().unwrap_or(0),
            ])
        })
        .collect();
    Some((LinearHeader { width, height }, samples))
}
