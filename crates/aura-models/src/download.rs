//! Resumable transfers, against a transport port rather than against HTTP.
//!
//! No crate in this workspace opens a socket, and phase 03 is not where that
//! changes: Article IX rule S3 is default-deny egress, and the strongest form of
//! that rule is a build with nowhere to send anything. So the update path is
//! written against [`Transport`] - "give me these bytes of that key" - and the
//! implementation that ships is [`FileTransport`], which reads from a directory
//! or a mounted share. That is what the offline installer bundle uses, and it is
//! what the tests use to simulate a link that dies halfway through.
//!
//! The resume logic is the part worth having under test either way. A transfer
//! lands in a `.part` file next to its destination, resumes from that file's
//! length, and is verified whole before anything is renamed into place. Killing
//! the process at any point costs one chunk.

use std::fmt;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use aura_core::AuraResult;

/// Bytes moved per read. Small enough that a cancelled transfer stops promptly,
/// large enough that a 400 MB model is not four hundred thousand syscalls.
pub const CHUNK_BYTES: u64 = 1024 * 1024;

/// How far a transfer has got.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferProgress {
    /// The key being transferred.
    pub key: String,
    /// Bytes on disk so far.
    pub have: u64,
    /// Bytes expected in total.
    pub total: u64,
}

/// Somewhere model bytes come from.
pub trait Transport: Send + Sync + fmt::Debug {
    /// Total size of a key's payload.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5004` when the key cannot be reached.
    fn size(&self, key: &str) -> AuraResult<u64>;

    /// Read a byte range. A short read is not an error: it ends the transfer,
    /// and the caller's verification decides whether what arrived is complete.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5004` when the range cannot be read at all.
    fn read_range(&self, key: &str, from: u64, len: u64) -> AuraResult<Vec<u8>>;
}

/// A transport over a directory: the offline bundle, and the test double.
#[derive(Debug, Clone)]
pub struct FileTransport {
    root: PathBuf,
    /// When set, the transport refuses to serve past this offset.
    ///
    /// This is how a test produces a link that dies halfway through without a
    /// network, a server or a sleep.
    truncate_at: Option<u64>,
}

impl FileTransport {
    /// Serve from a directory.
    #[must_use]
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            truncate_at: None,
        }
    }

    /// Serve from a directory, but stop after this many bytes of any file.
    #[must_use]
    pub fn failing_after(root: &Path, bytes: u64) -> Self {
        Self {
            root: root.to_path_buf(),
            truncate_at: Some(bytes),
        }
    }

    fn path_for(&self, key: &str) -> PathBuf {
        self.root.join(key)
    }
}

impl Transport for FileTransport {
    fn size(&self, key: &str) -> AuraResult<u64> {
        let path = self.path_for(key);
        let metadata = fs::metadata(&path).map_err(|err| {
            aura_core::errors::ml::transfer_incomplete(key, 0, 0)
                .with_context("cause", err.to_string())
        })?;
        Ok(metadata.len())
    }

    fn read_range(&self, key: &str, from: u64, len: u64) -> AuraResult<Vec<u8>> {
        let path = self.path_for(key);
        let mut file = fs::File::open(&path).map_err(|err| {
            aura_core::errors::ml::transfer_incomplete(key, from, from + len)
                .with_context("cause", err.to_string())
        })?;
        file.seek(SeekFrom::Start(from)).map_err(|err| {
            aura_core::errors::ml::transfer_incomplete(key, from, from + len)
                .with_context("cause", err.to_string())
        })?;

        let allowed = match self.truncate_at {
            Some(limit) => limit.saturating_sub(from).min(len),
            None => len,
        };
        if allowed == 0 {
            return Ok(Vec::new());
        }

        let mut buffer = vec![0u8; usize::try_from(allowed).unwrap_or(0)];
        let read = file.read(&mut buffer).map_err(|err| {
            aura_core::errors::ml::transfer_incomplete(key, from, from + len)
                .with_context("cause", err.to_string())
        })?;
        buffer.truncate(read);
        Ok(buffer)
    }
}

/// Fetch a key into a `.part` file, resuming whatever is already there.
///
/// Returns the path of the completed `.part` file. It is deliberately *not*
/// renamed here: installing is [`crate::swap::install`]'s job, and it only runs
/// after verification, so a file can never appear at its final name unverified.
///
/// # Errors
///
/// `AURA-ML-5004` when the transfer ends short of the expected size, which is the
/// resumable case, or when the partial file cannot be written.
pub fn fetch(
    transport: &dyn Transport,
    key: &str,
    part_path: &Path,
    progress: &dyn Fn(TransferProgress),
) -> AuraResult<PathBuf> {
    let total = transport.size(key)?;

    if let Some(parent) = part_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            aura_core::errors::ml::transfer_incomplete(key, 0, total)
                .with_context("cause", err.to_string())
        })?;
    }

    let mut have = fs::metadata(part_path).map_or(0, |meta| meta.len());
    if have > total {
        // A partial file longer than the payload is not a resume point, it is a
        // different payload. Start again rather than reason about it.
        drop(fs::remove_file(part_path));
        have = 0;
    }

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(part_path)
        .map_err(|err| {
            aura_core::errors::ml::transfer_incomplete(key, have, total)
                .with_context("cause", err.to_string())
        })?;

    while have < total {
        let want = CHUNK_BYTES.min(total - have);
        let chunk = transport.read_range(key, have, want)?;
        if chunk.is_empty() {
            break;
        }
        file.write_all(&chunk).map_err(|err| {
            aura_core::errors::ml::transfer_incomplete(key, have, total)
                .with_context("cause", err.to_string())
        })?;
        have += chunk.len() as u64;
        progress(TransferProgress {
            key: key.to_string(),
            have,
            total,
        });
    }

    file.flush().map_err(|err| {
        aura_core::errors::ml::transfer_incomplete(key, have, total)
            .with_context("cause", err.to_string())
    })?;

    if have < total {
        return Err(aura_core::errors::ml::transfer_incomplete(key, have, total));
    }
    Ok(part_path.to_path_buf())
}
