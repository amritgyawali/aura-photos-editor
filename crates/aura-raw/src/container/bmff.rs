//! ISO base media boxes, which is what a Canon CR3 is.
//!
//! CR3 broke with twenty years of TIFF-derived RAW containers and used the MP4
//! box structure instead. What a decoder needs from it:
//!
//! * inside `moov`, Canon's own `uuid` box holds `CMT1` (IFD0 in TIFF form:
//!   make, model, orientation), `CMT2` (EXIF), `CMT3` (the Canon MakerNote,
//!   which carries white balance, black and white levels and the sensor's
//!   borders) and `THMB`;
//! * each `trak` describes one stream; the sensor's is the track whose sample
//!   description is a `CRAW` whose `CMP1` child is the CRX image header, and
//!   its `co64`/`stsz` boxes say where in `mdat` the compressed frame is;
//! * a second top-level `uuid` box holds the `PRVW` preview JPEG.
//!
//! Other `uuid` boxes - XMP above all - hold text, not boxes, and are never
//! descended into: reading an XML packet as box headers is how a perfectly good
//! file came to be reported as damaged.
//!
//! The walk is depth- and count-bounded, and a box whose declared size does not
//! advance the cursor terminates the walk, so a malformed file cannot loop.

use aura_core::errors::raw::corrupt;
use aura_core::AuraResult;

/// Deepest nesting we will follow.
const MAX_DEPTH: usize = 8;

/// Most boxes we will visit in one file.
const MAX_BOXES: usize = 4096;

/// Canon's metadata box inside `moov`.
const UUID_CANON: [u8; 16] = [
    0x85, 0xc0, 0xb6, 0x87, 0x82, 0x0f, 0x11, 0xe0, 0x81, 0x11, 0xf4, 0xce, 0x46, 0x2b, 0x6a, 0x48,
];

/// Canon's top-level preview box, which wraps `PRVW` after eight bytes of its own.
const UUID_PREVIEW: [u8; 16] = [
    0xea, 0xf4, 0x2b, 0x5e, 0x1c, 0x98, 0x4b, 0x88, 0xb9, 0xfb, 0xb7, 0xdc, 0x40, 0x6e, 0x4d, 0x16,
];

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

    /// One past the last payload byte.
    #[must_use]
    pub const fn end(&self) -> usize {
        self.start + self.len
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
/// Returns `AURA-RAW-2002` when a top-level box declares a size that does not fit.
pub fn walk(bytes: &[u8]) -> AuraResult<Vec<BoxRef>> {
    let mut out = Vec::new();
    walk_range(bytes, 0, bytes.len(), 0, &mut out)?;
    Ok(out)
}

/// Read one box header at `at`, bounded by `to`: `(kind, payload start, total size)`.
fn header(bytes: &[u8], at: usize, to: usize) -> AuraResult<([u8; 4], usize, usize)> {
    let size32 = read_u32(bytes, at);
    let mut kind = [0u8; 4];
    for (slot, byte) in kind
        .iter_mut()
        .zip(bytes.get(at + 4..at + 8).unwrap_or(&[]).iter())
    {
        *slot = *byte;
    }
    let (head, size) = match size32 {
        // 0 means "to the end of the enclosing range".
        0 => (8usize, to.saturating_sub(at)),
        // 1 means a 64-bit size follows the type.
        1 => {
            let big =
                (u64::from(read_u32(bytes, at + 8)) << 32) | u64::from(read_u32(bytes, at + 12));
            (16usize, usize::try_from(big).unwrap_or(0))
        }
        other => (8usize, other as usize),
    };
    if size < head || at.checked_add(size).is_none_or(|end| end > to) {
        return Err(corrupt(format!(
            "bmff box at {at} declares {size} bytes, which does not fit"
        )));
    }
    Ok((kind, at + head, size))
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
        let (kind, payload_start, size) = match header(bytes, at, to) {
            Ok(found) => found,
            // A damaged top-level box is a damaged file; a damaged nested one only
            // ends the walk of its parent, so the boxes already found still count.
            Err(err) if depth == 0 => return Err(err),
            Err(_) => return Ok(()),
        };
        let payload_end = at + size;
        out.push(BoxRef {
            kind,
            start: payload_start,
            len: payload_end - payload_start,
            depth,
        });

        match &kind {
            b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl" | b"moof" | b"traf" => {
                walk_range(bytes, payload_start, payload_end, depth + 1, out)?;
            }
            b"uuid" => {
                let id = bytes.get(payload_start..payload_start + 16);
                let skip = if id == Some(&UUID_CANON[..]) {
                    Some(16)
                } else if id == Some(&UUID_PREVIEW[..]) {
                    Some(24)
                } else {
                    None
                };
                if let Some(skip) = skip {
                    let inner = payload_start.saturating_add(skip);
                    if inner < payload_end {
                        walk_range(bytes, inner, payload_end, depth + 1, out)?;
                    }
                }
            }
            _ => {}
        }
        at = payload_end;
    }
    Ok(())
}

/// The payload range of the first box with this type.
#[must_use]
pub fn find<'a>(boxes: &'a [BoxRef], name: &[u8; 4]) -> Option<&'a BoxRef> {
    boxes.iter().find(|found| found.is(name))
}

/// One stream of a CR3, as far as a decoder cares.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Track {
    /// The `CMP1` image header, when the track is a CRX-coded frame.
    pub cmp1: Option<Vec<u8>>,
    /// True when the sample description is a JPEG.
    pub jpeg: bool,
    /// True when the track is Canon's timed metadata, `CTMD`, whose records
    /// carry the MakerNote parts that newer bodies no longer put in `CMT3`.
    pub ctmd: bool,
    /// Offset of the first sample in the file.
    pub offset: Option<u64>,
    /// Size of the first sample in bytes.
    pub size: Option<u64>,
}

/// Every `trak` under `moov`, in file order.
#[must_use]
pub fn tracks(bytes: &[u8], boxes: &[BoxRef]) -> Vec<Track> {
    boxes
        .iter()
        .filter(|found| found.is(b"trak"))
        .map(|trak| {
            let mut track = Track::default();
            for child in boxes
                .iter()
                .filter(|child| child.start >= trak.start && child.end() <= trak.end())
            {
                if child.is(b"stsd") {
                    read_sample_description(bytes, child, &mut track);
                } else if child.is(b"stsz") {
                    // version/flags, the common sample size, the count, then sizes.
                    let common = read_u32(bytes, child.start + 4);
                    track.size = Some(u64::from(if common != 0 {
                        common
                    } else {
                        read_u32(bytes, child.start + 12)
                    }));
                } else if child.is(b"co64") {
                    track.offset = Some(
                        (u64::from(read_u32(bytes, child.start + 8)) << 32)
                            | u64::from(read_u32(bytes, child.start + 12)),
                    );
                } else if child.is(b"stco") {
                    track.offset = Some(u64::from(read_u32(bytes, child.start + 8)));
                }
            }
            track
        })
        .collect()
}

/// `stsd`: version and flags, an entry count, then the entries as boxes. A `CRAW`
/// entry carries 82 bytes of video sample description before its own children.
fn read_sample_description(bytes: &[u8], stsd: &BoxRef, track: &mut Track) {
    let mut at = stsd.start + 8;
    while at + 8 <= stsd.end() {
        let Ok((kind, start, size)) = header(bytes, at, stsd.end()) else {
            return;
        };
        let end = at + size;
        if &kind == b"CTMD" {
            track.ctmd = true;
        }
        if &kind == b"CRAW" {
            let mut inner = start + 82;
            while inner + 8 <= end {
                let Ok((child, child_start, child_size)) = header(bytes, inner, end) else {
                    break;
                };
                match &child {
                    b"CMP1" => {
                        track.cmp1 = bytes
                            .get(child_start..inner + child_size)
                            .map(<[u8]>::to_vec);
                    }
                    b"JPEG" => track.jpeg = true,
                    _ => {}
                }
                inner += child_size;
            }
        }
        at = end;
    }
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
