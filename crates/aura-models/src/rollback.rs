//! Which version is live, and what happens when a new one does not work.
//!
//! A newly installed model is `pending`, not `active`. It becomes active only
//! after it has completed one real inference; if that first use throws, times out
//! or produces the wrong shape, the previous version is restored and the new one
//! is marked rejected so it is not retried on every launch.
//!
//! The alternative - trusting a model because it downloaded correctly - is how a
//! bad release ruins a photographer's Saturday. A correct download of a broken
//! model is exactly the failure this guards against, and no amount of signature
//! or digest checking can see it.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use aura_core::AuraResult;
use aura_infer::contract::infer::Version;
use serde::{Deserialize, Serialize};

/// The file inside the models directory that records what is live.
pub const STATE_FILE: &str = "installed.json";

/// What is installed for one model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct VersionState {
    /// The version in use, once it has proved itself.
    pub active: Option<Version>,
    /// A version installed but not yet proved.
    pub pending: Option<Version>,
    /// Versions that failed their first real use, newest last.
    pub rejected: Vec<Version>,
}

impl VersionState {
    /// The version a caller should load: the pending one if there is one, so it
    /// gets its chance, otherwise the active one.
    #[must_use]
    pub fn candidate(&self) -> Option<Version> {
        self.pending.or(self.active)
    }

    /// True when this version has already failed here.
    #[must_use]
    pub fn was_rejected(&self, version: Version) -> bool {
        self.rejected.contains(&version)
    }
}

/// What is installed, for every model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct InstallState {
    /// Schema version of this file.
    pub schema: u32,
    /// Model name to its state.
    pub models: BTreeMap<String, VersionState>,
}

impl InstallState {
    /// The schema this build writes.
    pub const SCHEMA: u32 = 1;

    /// An empty state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            schema: Self::SCHEMA,
            models: BTreeMap::new(),
        }
    }

    /// Read the state from a models directory, or start an empty one.
    ///
    /// A state file that will not parse is replaced rather than repaired: it is
    /// derivable from what is on disk, so losing it costs a re-verification and
    /// nothing else.
    #[must_use]
    pub fn load(directory: &Path) -> Self {
        let path = state_path(directory);
        let Ok(text) = fs::read_to_string(path) else {
            return Self::new();
        };
        match serde_json::from_str::<Self>(&text) {
            Ok(state) if state.schema == Self::SCHEMA => state,
            _ => Self::new(),
        }
    }

    /// Write the state atomically.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5004` when the file cannot be written or renamed.
    pub fn save(&self, directory: &Path) -> AuraResult<()> {
        let rendered = serde_json::to_string_pretty(self).map_err(|err| {
            aura_core::errors::ml::transfer_incomplete("installed.json", 0, 0)
                .with_context("cause", err.to_string())
        })?;
        fs::create_dir_all(directory).map_err(|err| {
            aura_core::errors::ml::transfer_incomplete("installed.json", 0, 0)
                .with_context("cause", err.to_string())
        })?;
        let target = state_path(directory);
        let temporary = directory.join(format!("{STATE_FILE}.tmp"));
        fs::write(&temporary, rendered.as_bytes()).map_err(|err| {
            aura_core::errors::ml::transfer_incomplete("installed.json", 0, 0)
                .with_context("cause", err.to_string())
        })?;
        if target.exists() {
            drop(fs::remove_file(&target));
        }
        fs::rename(&temporary, &target).map_err(|err| {
            drop(fs::remove_file(&temporary));
            aura_core::errors::ml::transfer_incomplete("installed.json", 0, 0)
                .with_context("cause", err.to_string())
        })
    }

    /// Record a freshly installed version as pending.
    pub fn stage(&mut self, name: &str, version: Version) {
        let entry = self.models.entry(name.to_string()).or_default();
        entry.pending = Some(version);
        entry.rejected.retain(|rejected| *rejected != version);
    }

    /// Promote the pending version after a successful first use.
    pub fn confirm(&mut self, name: &str, version: Version) {
        let entry = self.models.entry(name.to_string()).or_default();
        if entry.pending == Some(version) {
            entry.pending = None;
        }
        entry.active = Some(version);
    }

    /// Reject the pending version. Returns the version that is live again.
    pub fn reject(&mut self, name: &str, version: Version) -> Option<Version> {
        let entry = self.models.entry(name.to_string()).or_default();
        if entry.pending == Some(version) {
            entry.pending = None;
        }
        if !entry.rejected.contains(&version) {
            entry.rejected.push(version);
        }
        if entry.active == Some(version) {
            entry.active = None;
        }
        entry.active
    }

    /// The state of one model.
    #[must_use]
    pub fn state_of(&self, name: &str) -> VersionState {
        self.models.get(name).cloned().unwrap_or_default()
    }
}

/// Where the state file lives inside a models directory.
#[must_use]
pub fn state_path(directory: &Path) -> PathBuf {
    directory.join(STATE_FILE)
}
