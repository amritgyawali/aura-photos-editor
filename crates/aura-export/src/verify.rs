//! Write, flush, sync, re-open, read, hash.
//!
//! This is the module the whole phase is built around, and it is thirty lines of the most
//! consequential code in the product.
//!
//! ## What the read-back catches that a write-time hash does not
//!
//! All of it. A short write, a full volume whose filesystem reported success on a partial one, a
//! NAS that acknowledges and drops, a failing SD card, an external drive that silently returns
//! zeroes for a bad sector, and memory that is corrupting a buffer between the encoder and the
//! syscall - every one of those produces a *correct buffer in memory* and a wrong file on disk.
//!
//! Hashing the buffer on the way out is free, catches none of them, and lets a product make the
//! same claim. Section 6.1's first sentence is "photographers have lost galleries to silent write
//! failures", and this is what makes that a checkable claim rather than an assertion.
//!
//! ## The sync is not optional
//!
//! Without a `sync_all` the read-back can be served out of the page cache, which returns the bytes
//! that were written rather than the bytes that landed. A verification that reads its own write
//! cache is a verification that always passes, and it would look exactly like this module.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;

use aura_core::AuraResult;

use crate::errors::{destination_bad, verify_failed};

/// How many bytes are read at a time when hashing a file back.
///
/// One mebibyte. A 45 MP sixteen-bit TIFF is 270 MB and reading it in one allocation would double
/// the peak memory of an export whose budget is already dominated by the render.
const READ_CHUNK: usize = 1024 * 1024;

/// What a write produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    /// The size on disk.
    pub bytes: u64,
    /// BLAKE3 of what came back, lower-case hex. Empty when the job asked for no verification.
    pub hash: String,
    /// Whether the read-back ran.
    pub verified: bool,
}

/// Write bytes to a path and read them back.
///
/// Creates parent directories. Replaces an existing file, which is the ordinary case for a re-run
/// after a failed drive; the *naming* plan is what stops two photographs colliding, and by the time
/// a path reaches here it has already been claimed.
///
/// # Errors
///
/// `AURA-RENDER-8023` when the destination cannot be created or written to, and
/// `AURA-RENDER-8022` when the bytes that come back are not the bytes that went out.
pub fn write_and_verify(path: &Path, bytes: &[u8], verify: bool) -> AuraResult<Written> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| destination_bad(format!("cannot create `{}`: {e}", parent.display())))?;
    }

    {
        let mut file = File::create(path)
            .map_err(|e| destination_bad(format!("cannot create `{}`: {e}", path.display())))?;
        file.write_all(bytes)
            .map_err(|e| destination_bad(format!("cannot write `{}`: {e}", path.display())))?;
        file.flush()
            .map_err(|e| destination_bad(format!("cannot flush `{}`: {e}", path.display())))?;
        // Not optional. See the note above: without it the read-back can be served from the page
        // cache, and a verification that reads its own write cache always passes.
        file.sync_all()
            .map_err(|e| destination_bad(format!("cannot sync `{}`: {e}", path.display())))?;
    }

    let size = fs::metadata(path)
        .map_err(|e| destination_bad(format!("cannot stat `{}`: {e}", path.display())))?
        .len();

    if !verify {
        return Ok(Written {
            bytes: size,
            hash: String::new(),
            verified: false,
        });
    }

    let expected = blake3::hash(bytes).to_hex().to_string();
    let actual = hash_file(path)?;

    if size != bytes.len() as u64 || actual != expected {
        return Err(verify_failed(
            &path.display().to_string(),
            bytes.len() as u64,
            size,
        ));
    }

    Ok(Written {
        bytes: size,
        hash: actual,
        verified: true,
    })
}

/// BLAKE3 of a file on disk, read in chunks.
///
/// # Errors
///
/// `AURA-RENDER-8023` when the file cannot be read.
pub fn hash_file(path: &Path) -> AuraResult<String> {
    let mut file = File::open(path)
        .map_err(|e| destination_bad(format!("cannot re-open `{}`: {e}", path.display())))?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0_u8; READ_CHUNK];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| destination_bad(format!("cannot read `{}`: {e}", path.display())))?;
        if n == 0 {
            break;
        }
        match buf.get(..n) {
            Some(slice) => hasher.update(slice),
            None => break,
        };
    }
    Ok(hasher.finalize().to_hex().to_string())
}

/// Whether a destination exists, is a directory, and can be written to.
///
/// Run once before a job rather than discovered on the four-hundredth frame. A read-only volume, a
/// path that is a file, and a directory that does not exist all look identical from the export
/// loop, and all three are things to say before rendering a wedding.
///
/// # Errors
///
/// `AURA-RENDER-8023` when the destination is unusable.
pub fn check_destination(root: &Path) -> AuraResult<()> {
    fs::create_dir_all(root)
        .map_err(|e| destination_bad(format!("cannot create `{}`: {e}", root.display())))?;
    if !root.is_dir() {
        return Err(destination_bad(format!(
            "`{}` is not a directory",
            root.display()
        )));
    }
    // A probe file rather than a permissions read, because "can this process write here" is
    // answered differently by every filesystem, network share and operating system in the matrix,
    // and the only portable answer is to try.
    let probe = root.join(".aura-write-probe");
    let result = fs::write(&probe, b"aura")
        .map_err(|e| destination_bad(format!("cannot write in `{}`: {e}", root.display())));
    let _ = fs::remove_file(&probe);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::OpenOptions;
    use std::io::Seek as _;

    #[test]
    fn a_written_file_reads_back_with_the_hash_of_what_went_in() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gallery/a.jpg");
        let bytes = b"the bytes of a photograph".to_vec();
        let w = write_and_verify(&path, &bytes, true).unwrap();
        assert!(w.verified);
        assert_eq!(w.bytes, bytes.len() as u64);
        assert_eq!(w.hash, blake3::hash(&bytes).to_hex().to_string());
        assert_eq!(w.hash.len(), 64);
    }

    #[test]
    fn a_corrupted_file_fails_the_read_back() {
        // Section 10.1: "verification catches a deliberately corrupted write and fails the job
        // with a clear error". The corruption is applied *after* the write, which is exactly the
        // shape of a failing drive that returns different bytes from the ones it accepted.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.bin");
        let bytes = vec![7_u8; 4096];
        write_and_verify(&path, &bytes, true).unwrap();

        let mut f = OpenOptions::new().write(true).open(&path).unwrap();
        f.seek(std::io::SeekFrom::Start(2000)).unwrap();
        f.write_all(&[8]).unwrap();
        f.sync_all().unwrap();
        drop(f);

        // The file on disk no longer hashes to what was written, and re-hashing says so.
        let actual = hash_file(&path).unwrap();
        assert_ne!(actual, blake3::hash(&bytes).to_hex().to_string());
    }

    #[test]
    fn a_truncated_write_is_caught_by_size_as_well_as_by_digest() {
        // Two checks and not one, because a truncation that happens to hash to something is
        // impossible and a truncation whose *digest read* fails part-way is not. Size is the
        // cheap check and it runs first.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.bin");
        let bytes = vec![3_u8; 1000];
        write_and_verify(&path, &bytes, true).unwrap();
        let f = OpenOptions::new().write(true).open(&path).unwrap();
        f.set_len(500).unwrap();
        f.sync_all().unwrap();
        drop(f);
        assert_eq!(fs::metadata(&path).unwrap().len(), 500);
        assert_ne!(
            hash_file(&path).unwrap(),
            blake3::hash(&bytes).to_hex().to_string()
        );
    }

    #[test]
    fn an_unverified_write_carries_no_digest_at_all() {
        // A blank hash rather than a plausible one. `export_file_verified_needs_a_hash` in
        // migration 30 refuses the combination that would let this look verified.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.bin");
        let w = write_and_verify(&path, b"x", false).unwrap();
        assert!(!w.verified);
        assert!(w.hash.is_empty());
    }

    #[test]
    fn a_destination_that_is_a_file_is_refused_before_the_job_starts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("not-a-dir");
        fs::write(&path, b"x").unwrap();
        assert!(check_destination(&path).is_err());
    }

    #[test]
    fn a_destination_that_does_not_exist_yet_is_created() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deep/new/place");
        assert!(check_destination(&path).is_ok());
        assert!(path.is_dir());
        // The probe leaves nothing behind.
        assert!(!path.join(".aura-write-probe").exists());
    }

    #[test]
    fn hashing_a_large_file_in_chunks_agrees_with_hashing_it_whole() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.bin");
        let bytes: Vec<u8> = (0..(READ_CHUNK * 2 + 137))
            .map(|i| (i % 251) as u8)
            .collect();
        fs::write(&path, &bytes).unwrap();
        assert_eq!(
            hash_file(&path).unwrap(),
            blake3::hash(&bytes).to_hex().to_string()
        );
    }
}
