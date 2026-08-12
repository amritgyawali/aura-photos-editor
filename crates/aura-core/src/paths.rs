//! Where AURA is allowed to write, and where it flatly refuses to.

use std::path::{Path, PathBuf};

use crate::contract::error::{AuraError, AuraResult, Recovery, Severity};
use crate::errors::io::{IO_CLOUD_SYNCED, IO_NOT_FOUND};

/// Every location AURA writes to, resolved once at startup.
#[derive(Debug, Clone)]
pub struct AppPaths {
    /// Catalogs, previews, models. Large, local, never synced.
    pub data_dir: PathBuf,
    /// Settings and keychain references. Small.
    pub config_dir: PathBuf,
    /// Rotating logs and crash reports.
    pub log_dir: PathBuf,
    /// Preview pyramid cache. Deletable at any time without data loss.
    pub cache_dir: PathBuf,
}

impl AppPaths {
    /// Resolve the four per-user directories for this platform.
    ///
    /// # Errors
    ///
    /// Returns `AURA-IO-1001` when the platform exposes no data directory.
    pub fn resolve() -> AuraResult<Self> {
        let data_root = dirs::data_dir()
            .ok_or_else(|| {
                AuraError::new(
                    IO_NOT_FOUND,
                    Severity::Fatal,
                    Recovery::AskUser,
                    "no platform data directory",
                    "AURA could not find a place to store its files. Contact support.",
                )
            })?
            .join("AURA");
        let config = dirs::config_dir()
            .unwrap_or_else(|| data_root.clone())
            .join("AURA");
        Ok(Self {
            log_dir: data_root.join("logs"),
            cache_dir: dirs::cache_dir()
                .unwrap_or_else(|| data_root.clone())
                .join("AURA"),
            data_dir: data_root,
            config_dir: config,
        })
    }
}

/// Folder-name markers that indicate a cloud-sync engine owns this directory.
const CLOUD_MARKERS: &[&str] = &[
    "dropbox",
    "onedrive",
    "google drive",
    "googledrive",
    "icloud",
    "com~apple~clouddocs",
    "pcloud",
    "box sync",
    "mega",
    "yandexdisk",
    "nextcloud",
    "owncloud",
    "sync.com",
    "idrive",
    "tresorit",
];

/// Marker files sync clients leave behind even when the folder is renamed.
const CLOUD_PROBE_FILES: &[&str] = &[
    ".dropbox",
    ".dropbox.cache",
    ".icloud",
    "desktop.ini.onedrive",
];

/// Refuse to put a `SQLite` catalog anywhere a sync client will touch it.
///
/// A sync engine copying a WAL file mid-write produces a corrupt catalog and a
/// photographer who has lost a wedding. This is a hard refusal, not a warning.
///
/// # Errors
///
/// Returns `AURA-IO-1008` when the path, or an ancestor within eight levels,
/// looks like it belongs to a file-sync client.
pub fn assert_not_cloud_synced(path: &Path) -> AuraResult<()> {
    let lowered = path.to_string_lossy().to_lowercase();
    for marker in CLOUD_MARKERS {
        if lowered.contains(marker) {
            return Err(AuraError::new(
                IO_CLOUD_SYNCED,
                Severity::RunBlocking,
                Recovery::AskUser,
                format!("catalog path matches cloud marker '{marker}'"),
                "Catalogs cannot live in a synced folder such as Dropbox, iCloud Drive, \
                 OneDrive or Google Drive, because syncing corrupts them. Choose a folder \
                 on this computer.",
            )
            .with_context("marker", *marker));
        }
    }

    for ancestor in path.ancestors().take(8) {
        for probe in CLOUD_PROBE_FILES {
            if ancestor.join(probe).exists() {
                return Err(AuraError::new(
                    IO_CLOUD_SYNCED,
                    Severity::RunBlocking,
                    Recovery::AskUser,
                    format!("sync marker file {probe} found in an ancestor directory"),
                    "That folder is managed by a file-sync app, which corrupts catalogs. \
                     Choose a folder on this computer.",
                )
                .with_context("probe", *probe));
            }
        }
    }
    Ok(())
}

/// Windows refuses paths over 260 chars unless the extended prefix is used.
/// Applying it unconditionally on Windows removes an entire failure class.
#[must_use]
pub fn platform_safe(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let s = path.to_string_lossy();
        if s.len() > 240 && !s.starts_with("\\\\?\\") {
            return PathBuf::from(format!("\\\\?\\{s}"));
        }
    }
    path.to_path_buf()
}
