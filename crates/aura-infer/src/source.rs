//! Where a model file comes from.
//!
//! The runtime does not know what a registry is, what a signature is, or where
//! model files live. It asks a [`ModelSource`] to turn a pinned [`ModelRef`] plus
//! a chosen provider into a verified file on disk, and refuses to run anything it
//! was not handed that way.
//!
//! That boundary is why `aura-models` can own signing, downloads, atomic swaps
//! and rollback without this crate growing a network dependency or a notion of
//! trust. It is also what lets the tests here run against a directory of
//! fixtures: [`StaticModelSource`] is the test double Article II rule A3 requires.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::contract::infer::{
    ExecutionProvider, InferError, ModelClass, ModelRef, Precision, Version,
};

/// One model file, already verified, ready to load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedModel {
    /// Absolute path to the ONNX file.
    pub path: PathBuf,
    /// The precision this variant was exported at.
    pub precision: Precision,
    /// Which batch-size column this model belongs in.
    pub class: ModelClass,
    /// Peak working set the model card declares, in megabytes.
    ///
    /// The scheduler admits work against this number, so a card that understates
    /// it is how a machine ends up out of memory at image 2,800 of 3,000.
    pub working_set_mb: u32,
}

/// Turns a pinned model reference into a verified file.
pub trait ModelSource: Send + Sync + fmt::Debug {
    /// Resolve a model for a provider.
    ///
    /// Implementations pick the best variant for the provider - fp16 for
    /// accelerators, int8 for the processor path where the model's precision
    /// policy permits it - and verify it before returning.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5001` when nothing is pinned under that name and version,
    /// `AURA-ML-5002` or `AURA-ML-5003` when integrity checks fail, and
    /// `AURA-ML-5005` when the model card is missing.
    fn resolve(&self, model: ModelRef, ep: ExecutionProvider) -> Result<ResolvedModel, InferError>;

    /// Every model this source can serve, for warmup and for Settings.
    fn available(&self) -> Vec<(String, Version)>;
}

/// A model source backed by an explicit table. The test double.
///
/// It performs no verification, which is exactly why it is not usable in the
/// application: the only way to get a file into it is to name it in code.
#[derive(Debug, Default)]
pub struct StaticModelSource {
    entries: BTreeMap<String, ResolvedModel>,
}

impl StaticModelSource {
    /// An empty source.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    /// Register one model file under a reference.
    #[must_use]
    pub fn with(
        mut self,
        model: ModelRef,
        path: &Path,
        precision: Precision,
        class: ModelClass,
        working_set_mb: u32,
    ) -> Self {
        self.entries.insert(
            model.key(),
            ResolvedModel {
                path: path.to_path_buf(),
                precision,
                class,
                working_set_mb,
            },
        );
        self
    }
}

impl ModelSource for StaticModelSource {
    fn resolve(
        &self,
        model: ModelRef,
        _ep: ExecutionProvider,
    ) -> Result<ResolvedModel, InferError> {
        self.entries.get(&model.key()).cloned().ok_or_else(|| {
            aura_core::errors::ml::model_unknown(model.name, &model.version.to_string())
        })
    }

    fn available(&self) -> Vec<(String, Version)> {
        self.entries
            .values()
            .zip(self.entries.keys())
            .filter_map(|(_, key)| {
                let (name, version) = key.split_once('@')?;
                let version = version.parse().ok()?;
                Some((name.to_string(), version))
            })
            .collect()
    }
}
