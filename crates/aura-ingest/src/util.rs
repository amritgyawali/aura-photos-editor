//! Path and time helpers shared by the scan, hash and import stages.

use std::fs::Metadata;
use std::path::Path;

use time::OffsetDateTime;
use unicode_normalization::UnicodeNormalization;

/// Compare-form of a path: NFC-normalised, forward slashes, case preserved.
///
/// Used for duplicate detection only. The catalog always stores the exact bytes
/// the filesystem gave us, so we can always reopen the file.
#[must_use]
pub fn compare_key(rel_path: &str) -> String {
    rel_path.replace('\\', "/").nfc().collect::<String>()
}

/// A case-insensitive filesystem can present `IMG_1.CR2` and `img_1.cr2` as one
/// file. On a case-sensitive one they are two. We record both and let content
/// hashing decide whether they are actually the same bytes.
#[must_use]
pub fn case_fold_key(rel_path: &str) -> String {
    compare_key(rel_path).to_lowercase()
}

/// Path relative to a root, always with forward slashes and NFC-normalised.
#[must_use]
pub fn rel_path_forward_slash(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    compare_key(&relative.to_string_lossy())
}

/// Modification time in milliseconds since the Unix epoch, or zero when the
/// platform refuses to answer.
#[must_use]
pub fn mtime_ms(meta: &Metadata) -> i64 {
    meta.modified()
        .ok()
        .map(OffsetDateTime::from)
        .map_or(0, |t| {
            i64::try_from(t.unix_timestamp_nanos() / 1_000_000).unwrap_or(0)
        })
}

/// Modification time as an RFC-3339 UTC string for the catalog.
#[must_use]
pub fn mtime_rfc3339(meta: &Metadata) -> String {
    meta.modified()
        .ok()
        .map(OffsetDateTime::from)
        .map_or_else(|| "1970-01-01T00:00:00Z".to_string(), aura_catalog::rfc3339)
}

/// The cheap change detector stored on every file row.
#[must_use]
pub fn fast_key(size_bytes: u64, mtime_ms_value: i64) -> String {
    format!("{size_bytes}:{mtime_ms_value}")
}

/// The stem of a file name with a trailing sidecar extension removed.
///
/// Adobe writes both `IMG_0421.xmp` and `IMG_0421.CR2.xmp`, so the stem is
/// stripped twice before grouping.
#[must_use]
pub fn grouping_stem(file_name: &str) -> String {
    let once = file_name
        .rsplit_once('.')
        .map_or(file_name, |(stem, _)| stem);
    let twice = once.rsplit_once('.').map_or(once, |(stem, _)| stem);
    twice.to_string()
}
