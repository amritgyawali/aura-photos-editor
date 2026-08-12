//! Deterministic directory walking. Determinism level D2 depends on this file.

use std::path::{Path, PathBuf};

use aura_core::errors::io as ioerr;
use aura_core::{AuraError, AuraResult};
use walkdir::WalkDir;

use crate::contract::ingest::{FileKind, ImportPlan};

/// Camera RAW extensions we index by default.
pub const RAW_EXT: &[&str] = &[
    "cr2", "cr3", "crw", "nef", "nrw", "arw", "srf", "sr2", "raf", "dng", "orf", "rw2", "pef",
    "srw", "x3f", "3fr", "fff", "iiq", "mrw", "erf",
];
/// JPEG extensions.
pub const JPEG_EXT: &[&str] = &["jpg", "jpeg", "jpe"];
/// HEIF family extensions.
pub const HEIF_EXT: &[&str] = &["heic", "heif", "avif"];
/// TIFF extensions.
pub const TIFF_EXT: &[&str] = &["tif", "tiff"];
/// Sidecar extensions.
pub const SIDECAR_EXT: &[&str] = &["xmp"];
/// Movie extensions. Indexed but not processed in phase 01.
pub const VIDEO_EXT: &[&str] = &["mp4", "mov", "m4v", "avi", "mts", "m2ts"];

/// Decide the family of a file from its lower-case extension.
///
/// Unknown extensions are classified as `Other` and filtered out by the allow
/// list; nothing is ever guessed from file contents in this phase.
#[must_use]
pub fn classify(ext_lower: &str) -> FileKind {
    if RAW_EXT.contains(&ext_lower) {
        FileKind::Raw
    } else if JPEG_EXT.contains(&ext_lower) {
        FileKind::Jpeg
    } else if HEIF_EXT.contains(&ext_lower) {
        FileKind::Heif
    } else if TIFF_EXT.contains(&ext_lower) {
        FileKind::Tiff
    } else if ext_lower == "png" {
        FileKind::Png
    } else if SIDECAR_EXT.contains(&ext_lower) {
        FileKind::SidecarXmp
    } else if VIDEO_EXT.contains(&ext_lower) {
        FileKind::Video
    } else {
        FileKind::Other
    }
}

/// Directory names we never descend into. Skipping these is the difference
/// between a 40-second scan and a 6-minute one on a photographer's drive.
const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "$RECYCLE.BIN",
    "System Volume Information",
    ".Trashes",
    ".Spotlight-V100",
    ".fseventsd",
    "Lightroom Catalog Previews.lrdata",
    "Lightroom Catalog Smart Previews.lrdata",
    ".aura-cache",
    "Capture One Catalog",
    "@eaDir",
    ".dtrash",
];

/// One file the walker accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedFile {
    /// Absolute path as the filesystem gave it to us.
    pub abs_path: PathBuf,
    /// Path relative to the scanned root, forward slashes, NFC.
    pub rel_path: String,
    /// File name including extension.
    pub file_name: String,
    /// Lower-case extension without the dot.
    pub ext: String,
    /// Format family.
    pub kind: FileKind,
    /// Size in bytes.
    pub size_bytes: u64,
    /// Modification time in milliseconds since the epoch.
    pub mtime_ms: i64,
}

/// What one root produced.
#[derive(Debug, Default)]
pub struct ScanOutcome {
    /// Files accepted for import, in deterministic order.
    pub files: Vec<ScannedFile>,
    /// Files we refuse to touch, with the code explaining why. They become
    /// quarantine rows, not silent omissions.
    pub rejected: Vec<(PathBuf, AuraError)>,
    /// Directories visited, for the run report.
    pub dirs_visited: u64,
    /// Total bytes of accepted files.
    pub bytes_total: u64,
}

/// Walk a root deterministically. Sorted by file name at each level, so the
/// order of insertion into the catalog is identical on every platform and on
/// every rerun.
///
/// # Errors
///
/// Returns `AURA-IO-1001` when the root itself is gone. Anything smaller is
/// collected into [`ScanOutcome::rejected`] so one bad folder cannot abort a
/// 4,000-file import.
pub fn scan_root(root: &Path, plan: &ImportPlan, now_ms: i64) -> AuraResult<ScanOutcome> {
    if !root.exists() {
        return Err(ioerr::not_found(root));
    }

    let mut out = ScanOutcome::default();
    let allowed: Vec<String> = if plan.extensions.is_empty() {
        RAW_EXT
            .iter()
            .chain(JPEG_EXT)
            .chain(HEIF_EXT)
            .chain(TIFF_EXT)
            .chain(SIDECAR_EXT)
            .map(|s| (*s).to_string())
            .collect()
    } else {
        plan.extensions.clone()
    };

    let walker = WalkDir::new(root)
        .follow_links(false) // a symlink loop is not our problem to solve
        .sort_by_file_name() // DETERMINISM: stable traversal order
        .max_depth(32);

    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                // A single unreadable directory must not abort a 4,000-file import.
                let path = e.path().unwrap_or(root).to_path_buf();
                let err = e.io_error().map_or_else(
                    || ioerr::unreadable(&path, &DirectoryWalkError),
                    |io| ioerr::from_io(io, &path),
                );
                out.rejected.push((path, err));
                continue;
            }
        };

        if entry.file_type().is_dir() {
            out.dirs_visited += 1;
            continue;
        }
        if !entry.file_type().is_file() {
            continue; // sockets, fifos, devices
        }

        let path = entry.path().to_path_buf();
        let file_name = entry.file_name().to_string_lossy().to_string();

        // macOS AppleDouble stubs and hidden junk.
        if file_name.starts_with("._") || file_name == ".DS_Store" {
            continue;
        }
        if path.components().any(|c| {
            SKIP_DIRS
                .iter()
                .any(|s| s.eq_ignore_ascii_case(&c.as_os_str().to_string_lossy()))
        }) {
            continue;
        }

        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if !allowed.iter().any(|a| a == &ext) {
            continue;
        }

        // Windows path limit. We prepend the extended prefix where we can, and
        // quarantine when even that is not enough.
        if cfg!(windows) && path.as_os_str().len() > 32_000 {
            out.rejected
                .push((path.clone(), ioerr::path_too_long(&path)));
            continue;
        }

        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(e) => {
                let err = e.io_error().map_or_else(
                    || ioerr::unreadable(&path, &MetadataUnavailable),
                    |io| ioerr::from_io(io, &path),
                );
                out.rejected.push((path, err));
                continue;
            }
        };

        if meta.len() == 0 {
            out.rejected.push((path.clone(), ioerr::zero_byte(&path)));
            continue;
        }

        let mtime_ms = crate::util::mtime_ms(&meta);

        // Still being written? Card offloads and tethered shoots do this.
        // Skip now, pick it up on the next pass rather than hashing a partial file.
        //
        // Only an age inside the window counts. A negative age means the file is
        // stamped in the future relative to our clock - a camera with a wrong date,
        // or a frozen clock in a test - and skipping those would silently drop a
        // whole card.
        let settle = i64::try_from(plan.settle_window_ms).unwrap_or(2000);
        let age_ms = now_ms.saturating_sub(mtime_ms);
        if settle > 0 && (0..settle).contains(&age_ms) {
            continue;
        }

        out.bytes_total += meta.len();
        out.files.push(ScannedFile {
            rel_path: crate::util::rel_path_forward_slash(root, &path),
            file_name,
            kind: classify(&ext),
            ext,
            size_bytes: meta.len(),
            mtime_ms,
            abs_path: path,
        });
    }

    Ok(out)
}

/// Cause type for a traversal failure that carried no `io::Error`.
#[derive(Debug)]
struct DirectoryWalkError;

impl std::fmt::Display for DirectoryWalkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("directory traversal error")
    }
}

impl std::error::Error for DirectoryWalkError {}

/// Cause type for metadata that the platform refused to provide.
#[derive(Debug)]
struct MetadataUnavailable;

impl std::fmt::Display for MetadataUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("metadata unavailable")
    }
}

impl std::error::Error for MetadataUnavailable {}
