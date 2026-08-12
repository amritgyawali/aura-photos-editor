//! The registry: what is pinned, what is installed, and what may be loaded.
//!
//! Opening a registry verifies the manifest signature once. Resolving a model
//! verifies that model's file once - the result is remembered, because digesting
//! a 400 MB file on every request would cost more than the inference - and checks
//! its card, its precision policy and its rollback state before handing back a
//! path.
//!
//! The registry is also the only thing in the product that writes into the models
//! directory. Article II rule A5: one writer per data domain.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use aura_core::AuraResult;
use aura_infer::contract::infer::{ExecutionProvider, ModelClass, ModelRef, Precision, Version};
use aura_infer::source::{ModelSource, ResolvedModel};
use parking_lot::Mutex;

use crate::contract::manifest::{ModelEntry, ModelsLock, LOCK_SCHEMA};
use crate::download::{fetch, TransferProgress, Transport};
use crate::rollback::InstallState;
use crate::swap;
use crate::verify::{from_hex, verify_file, verify_manifest};

/// The manifest file name inside the models directory.
pub const LOCK_FILE: &str = "models.lock";

/// The detached signature file name.
pub const SIGNATURE_FILE: &str = "manifest.sig";

/// The development signing key's public half.
///
/// Derived from a public seed phrase printed in `tools/model-sign`, so anybody
/// with the repository can re-sign the placeholder models - and so nobody can
/// mistake a development signature for a release one. See ADR-0007.
pub const DEV_PUBLIC_KEY_HEX: &str =
    "cde7625190d25313f3e3f4e64f25a30331564b5e2bfe71678d489fcb23953d78";

/// The public key this build verifies manifests against.
///
/// A shipping build sets `AURA_RELEASE_PUBLIC_KEY` at compile time to the release
/// key, whose private half never leaves the offline signing machine. A build that
/// does not set it trusts the development key, which is the honest description of
/// a development build.
#[must_use]
pub fn trusted_public_key() -> &'static str {
    match option_env!("AURA_RELEASE_PUBLIC_KEY") {
        Some(key) => key,
        None => DEV_PUBLIC_KEY_HEX,
    }
}

/// A verified, pinned model set on disk.
#[derive(Debug)]
pub struct ModelRegistry {
    directory: PathBuf,
    card_root: PathBuf,
    lock: ModelsLock,
    state: Mutex<InstallState>,
    verified: Mutex<BTreeSet<String>>,
}

impl ModelRegistry {
    /// Open a models directory and verify its manifest.
    ///
    /// `card_root` is where `model_card` paths are resolved from - the workspace
    /// root in development, the installation directory in a shipped build.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5002` when the manifest or its signature is missing, malformed or
    /// does not verify against the release key, and `AURA-ML-5010` when the
    /// manifest is signed but not parseable, which means a signed release is
    /// broken and is a different conversation from a tampered one.
    pub fn open(directory: &Path, card_root: &Path, public_key_hex: &str) -> AuraResult<Self> {
        let lock_path = directory.join(LOCK_FILE);
        let signature_path = directory.join(SIGNATURE_FILE);

        let manifest_bytes = fs::read(&lock_path).map_err(|err| {
            aura_core::errors::ml::signature_invalid(format!(
                "{LOCK_FILE} could not be read: {err}"
            ))
        })?;
        let signature_text = fs::read_to_string(&signature_path).map_err(|err| {
            aura_core::errors::ml::signature_invalid(format!(
                "{SIGNATURE_FILE} could not be read: {err}"
            ))
        })?;

        let signature = from_hex(signature_text.trim())?;
        let public_key = from_hex(public_key_hex.trim())?;
        verify_manifest(&manifest_bytes, &signature, &public_key)?;

        let lock: ModelsLock = serde_json::from_slice(&manifest_bytes).map_err(|err| {
            aura_core::errors::ml::parse_failed(format!(
                "{LOCK_FILE} is signed but unreadable: {err}"
            ))
        })?;
        if lock.schema != LOCK_SCHEMA {
            return Err(aura_core::errors::ml::parse_failed(format!(
                "{LOCK_FILE} schema {} is not the {LOCK_SCHEMA} this build reads",
                lock.schema
            )));
        }

        let state = InstallState::load(directory);
        Ok(Self {
            directory: directory.to_path_buf(),
            card_root: card_root.to_path_buf(),
            lock,
            state: Mutex::new(state),
            verified: Mutex::new(BTreeSet::new()),
        })
    }

    /// The pinned model set.
    #[must_use]
    pub const fn lock(&self) -> &ModelsLock {
        &self.lock
    }

    /// Where the model files live.
    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Verify every file of every pinned model.
    ///
    /// This is what the release gate and the support bundle run. A normal launch
    /// does not: it verifies each model the first time it is resolved, so start-up
    /// does not digest models nobody is going to use.
    ///
    /// # Errors
    ///
    /// The first failure: `AURA-ML-5003` for a digest, `AURA-ML-5005` for a card.
    pub fn verify_all(&self) -> AuraResult<usize> {
        let mut checked = 0usize;
        for entry in &self.lock.models {
            self.check_card(entry)?;
            for variant in &entry.variants {
                let path = self.directory.join(&variant.file);
                verify_file(&path, &variant.sha256, variant.bytes)?;
                self.verified.lock().insert(variant.sha256.clone());
                checked += 1;
            }
        }
        Ok(checked)
    }

    /// Install a model's files from a transport, staging it as pending.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5001` when the version is not pinned, `AURA-ML-5004` when a
    /// transfer ends short, `AURA-ML-5003` when what arrived does not match the
    /// manifest.
    pub fn install(
        &self,
        transport: &dyn Transport,
        name: &str,
        version: Version,
        progress: &dyn Fn(TransferProgress),
    ) -> AuraResult<()> {
        let entry = self
            .lock
            .entry(name, version)
            .ok_or_else(|| aura_core::errors::ml::model_unknown(name, &version.to_string()))?;
        self.check_card(entry)?;

        for variant in &entry.variants {
            let destination = self.directory.join(&variant.file);
            let part = swap::part_path(&destination);
            fetch(transport, &variant.file, &part, progress)?;
            swap::install(&part, &destination, &variant.sha256, variant.bytes)?;
            self.verified.lock().insert(variant.sha256.clone());
        }

        let mut state = self.state.lock();
        state.stage(name, version);
        state.save(&self.directory)?;
        tracing::info!(
            target: "model.update",
            name,
            to = version.to_string().as_str(),
            delta_used = false,
            ok = true,
            "model installed and staged"
        );
        Ok(())
    }

    /// Mark a version as proved after its first successful use.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5004` when the state file cannot be written.
    pub fn confirm(&self, name: &str, version: Version) -> AuraResult<()> {
        let mut state = self.state.lock();
        state.confirm(name, version);
        state.save(&self.directory)?;
        if let Some(entry) = self.lock.entry(name, version) {
            for variant in &entry.variants {
                swap::forget_previous(&self.directory.join(&variant.file));
            }
        }
        Ok(())
    }

    /// Roll a version back after it failed its first real use.
    ///
    /// Returns the version that is live again, if any.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5009` when the previous files cannot be restored, which leaves the
    /// machine on a version that does not work and needs an operator.
    pub fn reject(&self, name: &str, version: Version) -> AuraResult<Option<Version>> {
        let restored = {
            let mut state = self.state.lock();
            let restored = state.reject(name, version);
            state.save(&self.directory)?;
            restored
        };

        if let Some(entry) = self.lock.entry(name, version) {
            for variant in &entry.variants {
                let destination = self.directory.join(&variant.file);
                if swap::previous_path(&destination).exists() {
                    swap::restore_previous(&destination)?;
                }
            }
        }

        tracing::warn!(
            target: "model.update",
            name,
            from = version.to_string().as_str(),
            to = restored.map(|v| v.to_string()).unwrap_or_default().as_str(),
            ok = false,
            "model rolled back after a failed first use"
        );
        Ok(restored)
    }

    /// What is installed for a model.
    #[must_use]
    pub fn state_of(&self, name: &str) -> crate::rollback::VersionState {
        self.state.lock().state_of(name)
    }

    /// Article VI rule M1: no card, no model.
    fn check_card(&self, entry: &ModelEntry) -> AuraResult<()> {
        let path = self.card_root.join(&entry.model_card);
        let readable = fs::metadata(&path).is_ok_and(|meta| meta.len() > 0);
        if readable {
            Ok(())
        } else {
            Err(aura_core::errors::ml::card_missing(
                &entry.name,
                &entry.model_card,
            ))
        }
    }

    /// Verify a variant's file once, then remember that it was verified.
    fn ensure_verified(&self, path: &Path, sha256: &str, bytes: u64) -> AuraResult<()> {
        if self.verified.lock().contains(sha256) {
            return Ok(());
        }
        verify_file(path, sha256, bytes)?;
        self.verified.lock().insert(sha256.to_string());
        Ok(())
    }
}

impl ModelSource for ModelRegistry {
    fn resolve(
        &self,
        model: ModelRef,
        ep: ExecutionProvider,
    ) -> Result<ResolvedModel, aura_infer::InferError> {
        // A version that failed here is not served again, whatever the caller
        // asked for: the rollback exists to keep the photographer working.
        let state = self.state.lock().state_of(model.name);
        let version = if state.was_rejected(model.version) {
            state.active.ok_or_else(|| {
                aura_core::errors::ml::rolled_back(model.name, &model.version.to_string(), "none")
            })?
        } else {
            model.version
        };

        let entry = self.lock.entry(model.name, version).ok_or_else(|| {
            aura_core::errors::ml::model_unknown(model.name, &version.to_string())
        })?;

        self.check_card(entry)?;

        let variant = entry.variant_for(ep).ok_or_else(|| {
            aura_core::errors::ml::model_unknown(
                model.name,
                &format!("{version} has no variant this build may run"),
            )
        })?;

        let path = self.directory.join(&variant.file);
        self.ensure_verified(&path, &variant.sha256, variant.bytes)?;

        Ok(ResolvedModel {
            path,
            precision: precision_for(entry, variant.precision),
            class: entry.class,
            working_set_mb: entry.working_set_mb,
        })
    }

    fn available(&self) -> Vec<(String, Version)> {
        self.lock
            .models
            .iter()
            .map(|entry| (entry.name.clone(), entry.version))
            .collect()
    }
}

/// The precision a model actually runs at, after its policy has had its say.
fn precision_for(entry: &ModelEntry, declared: Precision) -> Precision {
    if entry.precision_policy.allows(declared) {
        declared
    } else {
        Precision::Fp32
    }
}

/// The class a model belongs to, for callers that only have a name.
#[must_use]
pub fn class_of(lock: &ModelsLock, name: &str) -> ModelClass {
    lock.newest(name)
        .map_or(ModelClass::Embedding, |entry| entry.class)
}
