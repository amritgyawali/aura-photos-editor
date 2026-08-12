//! ISO base media boxes, which is what a Canon CR3 is.
//!
//! CR3 broke with twenty years of TIFF-derived RAW containers and used the MP4
//! box structure instead. The parts we need are shallow: `CMT1` holds the EXIF
//! directory in TIFF form, and `PRVW`/`THMB` hold JPEG previews. Everything else
//! - the CTMD metadata track, the CRX-compressed sensor data - is not read by
//! this build (see `docs/camera-support.md`).
//!
//! The walk is depth- and count-bounded, and a box whose declared size does not
//! advance the cursor terminates the walk, so a malformed file cannot loop.

use aura_core::errors::raw::corrupt;
use aura_core::AuraResult;

/// Deepest nesting we will follow.
const MAX_DEPTH: usize = 8;

/// Most boxes we will visit in one file.
const MAX_BOXES: usize = 4096;

/// One box header plus the range of its payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoxRef {
    /// Four-character box type, for example `moov`.
    pub kind: [u8; 4],
    /// Offset of the payload inside the file.
    pub start: usize,
    /// Length of the payload in bytes.
    pub len: usize,
    /// How deeply nested the box is; 0 is top level.
    pub depth: usize,
}

impl BoxRef {
    /// True when the type matches the given four characters.
    #[must_use]
    pub fn is(&self, name: &[u8; 4]) -> bool {
        self.kind == *name
    }
}

/// True when the buffer looks like an ISO base media file.
#[must_use]
pub fn looks_like_bmff(bytes: &[u8]) -> bool {
    matches!(bytes.get(4..8), Some(b"ftyp"))
}

/// The brand declared by the `ftyp` box, for example `crx ` for a Canon CR3.
#[must_use]
pub fn brand(bytes: &[u8]) -> Option<[u8; 4]> {
    let raw = bytes.get(8..12)?;
    let mut out = [0u8; 4];
    for (slot, byte) in out.iter_mut().zip(raw.iter()) {
        *slot = *byte;
    }
    Some(out)
}

/// Walk every box in the file, in file order.
///
/// Container boxes are descended into by name rather than by guessing, because
/// a payload that merely looks like a box header is common in compressed data.
///
/// # Errors
///
/// Returns `AURA-RAW-2002` when a box declares a size that does not fit.
pub fn walk(bytes: &[u8]) -> AuraResult<Vec<BoxRef>> {
    let mut out = Vec::new();
    walk_range(bytes, 0, bytes.len(), 0, &mut out)?;
    Ok(out)
}

fn walk_range(
    bytes: &[u8],
    from: usize,
    to: usize,
    depth: usize,
    out: &mut Vec<BoxRef>,
) -> AuraResult<()> {
    if depth > MAX_DEPTH {
        return Ok(());
    }
    let mut at = from;
    while at + 8 <= to && out.len() < MAX_BOXES {
        let size32 = read_u32(bytes, at);
        let mut kind = [0u8; 4];
        for (slot, byte) in kind
            .iter_mut()
            .zip(bytes.get(at + 4..at + 8).unwrap_or(&[]).iter())
        {
            *slot = *byte;
        }

        let (header, size) = match size32 {
            // 0 means "to the end of the file".
            0 => (8usize, to.saturating_sub(at)),
            // 1 means a 64-bit size follows the type.
            1 => {
                let high = u64::from(read_u32(bytes, at + 8));
                let low = u64::from(read_u32(bytes, at + 12));
                let big = (high << 32) | low;
                (16usize, usize::try_from(big).unwrap_or(0))
            }
            other => (8usize, other as usize),
        };

        if size < header || at + size > to {
            return Err(corrupt(format!(
                "bmff box at {at} declares {size} bytes, which does not fit"
            )));
        }

        let payload_start = at + header;
        let payload_len = size - header;
        out.push(BoxRef {
            kind,
            start: payload_start,
            len: payload_len,
            depth,
        });

        // Containers whose payload is a sequence of further boxes. `uuid` is
        // included because Canon nests the interesting boxes inside one, after
        // a sixteen-byte identifier.
        match &kind {
            b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl" | b"moof" | b"traf" => {
                walk_range(bytes, payload_start, payload_start + payload_len, depth + 1, out)?;
            }
            b"uuid" => {
                let after_uuid = payload_start.saturating_add(16);
                if after_uuid < payload_start + payload_len {
                    walk_range(bytes, after_uuid, payload_start + payload_len, depth + 1, out)?;
                }
            }
            _ => {}
        }

        at += size;
    }
    Ok(())
}

/// The payload range of the first box with this type.
#[must_use]
pub fn find<'a>(boxes: &'a [BoxRef], name: &[u8; 4]) -> Option<&'a BoxRef> {
    boxes.iter().find(|found| found.is(name))
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
