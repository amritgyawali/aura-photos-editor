//! Putting a verified file into place, atomically.
//!
//! The invariant this module exists to hold: **a file at its final name has been
//! verified**. Not "was verified when it was written", not "will be verified
//! before use" - at no point does an unverified byte exist under a name the
//! loader would open. Everything arrives as `.part`, is verified whole, and is
//! then renamed, which is atomic within a filesystem.
//!
//! The previous version is kept beside the new one until the new one has proved
//! itself. That is what makes [`crate::rollback`] possible without a download.

use std::fs;
use std::path::{Path, PathBuf};

use aura_core::AuraResult;

use crate::verify::verify_file;

/// Suffix for the copy kept while a new version proves itself.
pub const PREVIOUS_SUFFIX: &str = ".previous";

/// Verify a completed transfer and move it into place.
///
/// # Errors
///
/// `AURA-ML-5003` when the file does not match its manifest entry - the `.part`
/// file is removed in that case, because a file that failed verification is not a
/// resume point - or when the rename fails.
pub fn install(
    part_path: &Path,
    destination: &Path,
    expected_sha256: &str,
    expected_bytes: u64,
) -> AuraResult<()> {
    if let Err(err) = verify_file(part_path, expected_sha256, expected_bytes) {
        drop(fs::remove_file(part_path));
        return Err(err);
    }

    if destination.exists() {
        let kept = previous_path(destination);
        drop(fs::remove_file(&kept));
        fs::rename(destination, &kept).map_err(|err| {
            aura_core::errors::ml::digest_mismatch(
                &aura_core::redact::redact_path(destination),
                "movable",
                &format!("previous version could not be set aside: {err}"),
            )
        })?;
    }

    fs::rename(part_path, destination).map_err(|err| {
        aura_core::errors::ml::digest_mismatch(
            &aura_core::redact::redact_path(destination),
            "installable",
            &format!("rename into place failed: {err}"),
        )
    })
}

/// Put the kept previous version back.
///
/// # Errors
///
/// `AURA-ML-5009` when there is no previous version to restore, or the restore
/// itself fails - both mean the machine is now on a version that failed, which is
/// the situation the operator has to know about.
pub fn restore_previous(destination: &Path) -> AuraResult<()> {
    let kept = previous_path(destination);
    if !kept.exists() {
        return Err(aura_core::errors::ml::rolled_back(
            &aura_core::redact::redact_path(destination),
            "new",
            "none: no previous version was kept",
        ));
    }
    drop(fs::remove_file(destination));
    fs::rename(&kept, destination).map_err(|err| {
        aura_core::errors::ml::rolled_back(
            &aura_core::redact::redact_path(destination),
            "new",
            &format!("previous could not be restored: {err}"),
        )
    })
}

/// Delete the kept previous version, once the new one is confirmed.
pub fn forget_previous(destination: &Path) {
    drop(fs::remove_file(previous_path(destination)));
}

/// Where the previous version is kept.
#[must_use]
pub fn previous_path(destination: &Path) -> PathBuf {
    let mut name = destination.as_os_str().to_os_string();
    name.push(PREVIOUS_SUFFIX);
    PathBuf::from(name)
}

/// Where an in-flight transfer is written.
#[must_use]
pub fn part_path(destination: &Path) -> PathBuf {
    let mut name = destination.as_os_str().to_os_string();
    name.push(".part");
    PathBuf::from(name)
}
