//! Full-content BLAKE3. This is the identity of a file in AURA.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use aura_core::errors::io as ioerr;
use aura_core::{AuraResult, ContentHash};

/// 1 MiB buffer. Larger buffers stop helping once the disk is saturated and
/// start hurting when eight threads each hold one.
const BUF: usize = 1024 * 1024;

/// Hash a file end to end and return the digest with the byte count read.
///
/// Why not sample the first and last megabyte, which would be roughly fifty
/// times faster? Because two RAW files from the same camera in the same second
/// share their header and can share their tail, and a false duplicate silently
/// deletes a photograph.
///
/// # Errors
///
/// * `AURA-IO-1006` when the file cannot be read.
/// * `AURA-IO-1007` when the file grew or shrank during the read, which is
///   retryable rather than a corruption.
pub fn hash_file(path: &Path, expected_size: u64) -> AuraResult<(ContentHash, u64)> {
    let mut file = File::open(path).map_err(|e| ioerr::from_io(&e, path))?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; BUF];
    let mut read_total: u64 = 0;

    loop {
        let n = file.read(&mut buf).map_err(|e| ioerr::from_io(&e, path))?;
        if n == 0 {
            break;
        }
        let chunk = buf
            .get(..n)
            .ok_or_else(|| ioerr::unreadable(path, &ShortBuffer))?;
        hasher.update(chunk);
        read_total += n as u64;
    }

    if read_total != expected_size {
        return Err(ioerr::changed_while_reading(path)
            .with_context("expected_bytes", expected_size.to_string())
            .with_context("read_bytes", read_total.to_string()));
    }

    Ok((
        ContentHash::from_bytes(*hasher.finalize().as_bytes()),
        read_total,
    ))
}

/// Hash a batch in parallel with a bounded pool.
///
/// The `rayon` default pool is sized to the core count, which is wrong for IO-bound
/// work on a spinning disk and roughly right on `NVMe`, so the caller passes a
/// thread count from [`crate::tuning`].
#[must_use]
pub fn hash_batch(
    files: &[crate::scan::ScannedFile],
    threads: usize,
) -> Vec<(usize, AuraResult<(ContentHash, u64)>)> {
    use rayon::prelude::*;

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads.max(1))
        .build();

    let Ok(pool) = pool else {
        return files
            .iter()
            .enumerate()
            .map(|(i, f)| (i, hash_file(&f.abs_path, f.size_bytes)))
            .collect();
    };

    pool.install(|| {
        files
            .par_iter()
            .enumerate()
            .map(|(i, f)| (i, hash_file(&f.abs_path, f.size_bytes)))
            .collect::<Vec<_>>()
    })
}

/// Cause type for the impossible-but-typed short-buffer case.
#[derive(Debug)]
struct ShortBuffer;

impl std::fmt::Display for ShortBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("short buffer while hashing")
    }
}

impl std::error::Error for ShortBuffer {}
