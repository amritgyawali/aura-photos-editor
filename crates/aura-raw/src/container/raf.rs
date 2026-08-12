//! Fujifilm RAF, a container that is neither TIFF nor ISO base media.
//!
//! A RAF starts with the ASCII magic `FUJIFILMCCD-RAW`, then a fixed header of
//! offsets. Two of them matter here: the JPEG preview, which is a complete JFIF
//! file including its own EXIF block, and the CFA header. Fujifilm's X-Trans
//! sensors do not use a Bayer pattern at all, which is why this build reads RAF
//! previews and metadata but does not demosaic RAF sensor data - see
//! `docs/camera-support.md`.

use aura_core::errors::raw::corrupt;
use aura_core::AuraResult;

/// The magic every RAF begins with.
pub const MAGIC: &[u8] = b"FUJIFILM";

/// The parts of a RAF header this build uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RafHeader {
    /// Offset of the embedded JPEG.
    pub jpeg_offset: usize,
    /// Declared length of the embedded JPEG.
    pub jpeg_len: usize,
    /// Offset of the CFA header block.
    pub cfa_header_offset: usize,
    /// Declared length of the CFA header block.
    pub cfa_header_len: usize,
}

/// True when the buffer starts with the Fujifilm magic.
#[must_use]
pub fn looks_like_raf(bytes: &[u8]) -> bool {
    bytes.get(..MAGIC.len()) == Some(MAGIC)
}

/// Read the fixed header.
///
/// # Errors
///
/// Returns `AURA-RAW-2002` when the magic is absent or an offset does not fit
/// inside the file.
pub fn header(bytes: &[u8]) -> AuraResult<RafHeader> {
    if !looks_like_raf(bytes) {
        return Err(corrupt("not a fujifilm raf container"));
    }
    let jpeg_offset = read_u32(bytes, 84) as usize;
    let jpeg_len = read_u32(bytes, 88) as usize;
    let cfa_header_offset = read_u32(bytes, 92) as usize;
    let cfa_header_len = read_u32(bytes, 96) as usize;

    if jpeg_offset == 0 || jpeg_offset >= bytes.len() {
        return Err(corrupt("raf preview offset is outside the file"));
    }
    let end = jpeg_offset.saturating_add(jpeg_len);
    if end > bytes.len() {
        return Err(corrupt("raf preview length runs past the end of the file"));
    }

    Ok(RafHeader {
        jpeg_offset,
        jpeg_len,
        cfa_header_offset,
        cfa_header_len,
    })
}

fn read_u32(bytes: &[u8], at: usize) -> u32 {
    let Some(raw) = bytes.get(at..at + 4) else {
        return 0;
    };
    let mut quad = [0u8; 4];
    for (slot, byte) in quad.iter_mut().zip(raw.iter()) {
        *slot = *byte;
    }
    u32::from_be_bytes(quad)
}
