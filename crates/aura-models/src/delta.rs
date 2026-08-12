//! `AURADLT1`: a block delta between two model files.
//!
//! Retraining usually moves a fraction of a model's weights, so shipping the
//! whole file again wastes most of a photographer's download. A delta names the
//! blocks that are unchanged and carries only the rest.
//!
//! ## The format
//!
//! ```text
//! magic      8 bytes  "AURADLT1"
//! block_size u32 LE   bytes per matched block
//! base_len   u64 LE   length the base file must have
//! target_len u64 LE   length the result must have
//! base_sha   32 bytes sha256 of the base file
//! target_sha 32 bytes sha256 of the reconstructed file
//! ops        until end of stream:
//!   0x01 COPY   u64 offset, u32 length   - take bytes from the base
//!   0x02 INSERT u32 length, bytes        - literal bytes from the delta
//! ```
//!
//! Both digests are in the header, and both are checked: the base before any work
//! and the target after. A delta that reconstructs *something* is worthless; a
//! delta that proves it reconstructed the intended file is an optimisation we can
//! trust. Anything that does not check out raises `AURA-ML-5012` and the caller
//! fetches the whole file instead, which is why this path can never make an
//! update worse than not having it.
//!
//! The encoder is here too, and it is not only for tests: the release process
//! builds deltas with it, so the pair is exercised on every release.

use std::collections::BTreeMap;

use aura_core::AuraResult;

use crate::verify::sha256_hex;

/// File magic.
pub const MAGIC: &[u8; 8] = b"AURADLT1";

/// Default block size.
///
/// 4 KiB is small enough to match around an inserted tensor and large enough that
/// the index of a 400 MB model is a hundred thousand entries rather than millions.
pub const BLOCK_BYTES: usize = 4096;

const OP_COPY: u8 = 0x01;
const OP_INSERT: u8 = 0x02;

/// Build a delta that turns `base` into `target`.
///
/// The matcher is deliberately simple: index every aligned block of the base by
/// its digest, then walk the target looking for a block that matches. Rolling
/// hashes would match at unaligned offsets and save a few more bytes; they would
/// also be a second thing that can be subtly wrong in a path whose failure mode
/// is a corrupt model.
#[must_use]
pub fn encode(base: &[u8], target: &[u8]) -> Vec<u8> {
    let mut index: BTreeMap<String, u64> = BTreeMap::new();
    let mut offset = 0usize;
    while offset + BLOCK_BYTES <= base.len() {
        if let Some(block) = base.get(offset..offset + BLOCK_BYTES) {
            index.entry(sha256_hex(block)).or_insert(offset as u64);
        }
        offset += BLOCK_BYTES;
    }

    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&block_bytes_field().to_le_bytes());
    out.extend_from_slice(&(base.len() as u64).to_le_bytes());
    out.extend_from_slice(&(target.len() as u64).to_le_bytes());
    out.extend_from_slice(sha256_hex(base).as_bytes());
    out.extend_from_slice(sha256_hex(target).as_bytes());

    let mut literal: Vec<u8> = Vec::new();
    let mut at = 0usize;
    while at < target.len() {
        let matched = target
            .get(at..at + BLOCK_BYTES)
            .and_then(|block| index.get(&sha256_hex(block)).copied());

        if let Some(base_offset) = matched {
            flush_literal(&mut out, &mut literal);
            out.push(OP_COPY);
            out.extend_from_slice(&base_offset.to_le_bytes());
            out.extend_from_slice(&block_bytes_field().to_le_bytes());
            at += BLOCK_BYTES;
        } else {
            if let Some(byte) = target.get(at) {
                literal.push(*byte);
            }
            at += 1;
        }
    }
    flush_literal(&mut out, &mut literal);
    out
}

fn flush_literal(out: &mut Vec<u8>, literal: &mut Vec<u8>) {
    if literal.is_empty() {
        return;
    }
    out.push(OP_INSERT);
    let length = u32::try_from(literal.len()).unwrap_or(u32::MAX);
    out.extend_from_slice(&length.to_le_bytes());
    out.extend_from_slice(literal);
    literal.clear();
}

/// The block size as it appears in the header.
///
/// A named function rather than a cast at three call sites: the header field is
/// 32 bits, `BLOCK_BYTES` is a `usize`, and a delta whose header disagreed with
/// its own encoder would be unreadable everywhere at once.
fn block_bytes_field() -> u32 {
    u32::try_from(BLOCK_BYTES).unwrap_or(u32::MAX)
}

/// Apply a delta to a base file.
///
/// # Errors
///
/// `AURA-ML-5012` for every failure: wrong magic, a base that is not the base the
/// delta was built against, a truncated operation stream, a copy that points
/// outside the base, or a result whose digest is not the promised one. The caller
/// answers all of them the same way - fetch the whole file - so they share a code.
pub fn apply(base: &[u8], delta: &[u8]) -> AuraResult<Vec<u8>> {
    let header = Header::parse(delta)?;

    if base.len() as u64 != header.base_len {
        return Err(aura_core::errors::ml::delta_failed(format!(
            "delta expects a {} byte base, this one is {}",
            header.base_len,
            base.len()
        )));
    }
    if sha256_hex(base) != header.base_sha {
        return Err(aura_core::errors::ml::delta_failed(
            "delta was built against a different base version".to_string(),
        ));
    }

    let mut out: Vec<u8> = Vec::with_capacity(usize::try_from(header.target_len).unwrap_or(0));
    let mut at = header.body_offset;

    while at < delta.len() {
        let op = delta.get(at).copied().unwrap_or(0);
        at += 1;
        match op {
            OP_COPY => {
                let offset = read_u64(delta, at)?;
                at += 8;
                let length = read_u32(delta, at)? as usize;
                at += 4;
                let from = usize::try_from(offset).map_err(|_| {
                    // A 64-bit offset that does not fit this machine's pointer is
                    // a delta built for a different world, not a delta to apply.
                    aura_core::errors::ml::delta_failed("copy offset does not fit".to_string())
                })?;
                let slice = base.get(from..from + length).ok_or_else(|| {
                    aura_core::errors::ml::delta_failed(format!(
                        "copy of {length} bytes at {from} runs past the base"
                    ))
                })?;
                out.extend_from_slice(slice);
            }
            OP_INSERT => {
                let length = read_u32(delta, at)? as usize;
                at += 4;
                let slice = delta.get(at..at + length).ok_or_else(|| {
                    aura_core::errors::ml::delta_failed(
                        "insert runs past the end of the delta".to_string(),
                    )
                })?;
                out.extend_from_slice(slice);
                at += length;
            }
            other => {
                return Err(aura_core::errors::ml::delta_failed(format!(
                    "unknown delta operation {other:#04x}"
                )))
            }
        }
    }

    if out.len() as u64 != header.target_len {
        return Err(aura_core::errors::ml::delta_failed(format!(
            "delta produced {} bytes, expected {}",
            out.len(),
            header.target_len
        )));
    }
    if sha256_hex(&out) != header.target_sha {
        return Err(aura_core::errors::ml::delta_failed(
            "reconstructed file does not match the promised digest".to_string(),
        ));
    }
    Ok(out)
}

/// The fixed-size part of a delta.
#[derive(Debug)]
struct Header {
    base_len: u64,
    target_len: u64,
    base_sha: String,
    target_sha: String,
    body_offset: usize,
}

impl Header {
    /// 8 magic + 4 block + 8 `base_len` + 8 `target_len` + two 64-byte digests.
    const SIZE: usize = 8 + 4 + 8 + 8 + 64 + 64;

    fn parse(delta: &[u8]) -> AuraResult<Self> {
        if delta.len() < Self::SIZE {
            return Err(aura_core::errors::ml::delta_failed(format!(
                "delta is {} bytes, shorter than its header",
                delta.len()
            )));
        }
        if delta.get(..8) != Some(MAGIC.as_slice()) {
            return Err(aura_core::errors::ml::delta_failed(
                "delta does not start with the AURADLT1 magic".to_string(),
            ));
        }
        let base_len = read_u64(delta, 12)?;
        let target_len = read_u64(delta, 20)?;
        let base_sha = read_hex(delta, 28)?;
        let target_sha = read_hex(delta, 92)?;
        Ok(Self {
            base_len,
            target_len,
            base_sha,
            target_sha,
            body_offset: Self::SIZE,
        })
    }
}

fn read_u32(bytes: &[u8], at: usize) -> AuraResult<u32> {
    let slice = bytes.get(at..at + 4).ok_or_else(|| {
        aura_core::errors::ml::delta_failed("delta ends inside a length field".to_string())
    })?;
    let mut buf = [0u8; 4];
    buf.copy_from_slice(slice);
    Ok(u32::from_le_bytes(buf))
}

fn read_u64(bytes: &[u8], at: usize) -> AuraResult<u64> {
    let slice = bytes.get(at..at + 8).ok_or_else(|| {
        aura_core::errors::ml::delta_failed("delta ends inside an offset field".to_string())
    })?;
    let mut buf = [0u8; 8];
    buf.copy_from_slice(slice);
    Ok(u64::from_le_bytes(buf))
}

fn read_hex(bytes: &[u8], at: usize) -> AuraResult<String> {
    let slice = bytes.get(at..at + 64).ok_or_else(|| {
        aura_core::errors::ml::delta_failed("delta ends inside a digest field".to_string())
    })?;
    std::str::from_utf8(slice)
        .map(ToOwned::to_owned)
        .map_err(|_| aura_core::errors::ml::delta_failed("digest field is not text".to_string()))
}
