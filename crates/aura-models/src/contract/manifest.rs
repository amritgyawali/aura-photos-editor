//! FROZEN CONTRACT. The `models.lock` schema.
//!
//! This is the file the release process signs and every machine verifies, so its
//! shape is a published format rather than an internal type. The fields come
//! verbatim from section 5 of the phase document; the four this repository adds -
//! `class`, `working_set_mb`, `precision_policy` and `opset` - are additive and
//! each exists because something downstream refuses to work without it:
//!
//! * `class` decides which batch-size column the scheduler starts from;
//! * `working_set_mb` is what the memory ledger admits against, and a model that
//!   does not declare it is a model that can exhaust a device;
//! * `precision_policy` is how a colour-critical or skin-critical model forbids
//!   int8, which section 12 requires and which no code can infer;
//! * `opset` lets a build refuse a model at *read* time rather than at load time.
//!
//! Changing anything here requires an ADR and a re-signed manifest.

use std::collections::BTreeMap;

use aura_infer::contract::infer::{ExecutionProvider, ModelClass, Precision, Version};
use serde::{Deserialize, Serialize};

/// The schema version of `models.lock`.
pub const LOCK_SCHEMA: u32 = 1;

/// The whole pinned model set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelsLock {
    /// Schema version of this file.
    pub schema: u32,
    /// RFC 3339 timestamp of the release that produced it.
    pub generated_at: String,
    /// Every model this release ships, sorted by name then version.
    pub models: Vec<ModelEntry>,
}

impl ModelsLock {
    /// Find an entry by name and version.
    #[must_use]
    pub fn entry(&self, name: &str, version: Version) -> Option<&ModelEntry> {
        self.models
            .iter()
            .find(|entry| entry.name == name && entry.version == version)
    }

    /// The newest pinned version of a model, if any.
    #[must_use]
    pub fn newest(&self, name: &str) -> Option<&ModelEntry> {
        self.models
            .iter()
            .filter(|entry| entry.name == name)
            .max_by_key(|entry| entry.version)
    }
}

/// One model, at one version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelEntry {
    /// Registry name, e.g. `scene_classifier`.
    pub name: String,
    /// Pinned version.
    pub version: Version,
    /// Free text, e.g. `multiclass+multilabel`.
    pub task: String,
    /// Which batch-size column this model belongs in.
    pub class: ModelClass,
    /// What the model expects.
    pub input: InputSpec,
    /// Output names to shapes.
    pub output: BTreeMap<String, Vec<usize>>,
    /// One entry per shipped file.
    pub variants: Vec<Variant>,
    /// SPDX identifier or `proprietary`.
    pub licence: String,
    /// Repository-relative path to the model card. Article VI rule M1.
    pub model_card: String,
    /// Oldest application version that may load this model.
    pub min_app_version: String,
    /// Peak resident megabytes, measured at export.
    pub working_set_mb: u32,
    /// What precisions this model is allowed to run at.
    pub precision_policy: PrecisionPolicy,
    /// ONNX opset the files were exported at.
    pub opset: i64,
}

impl ModelEntry {
    /// The variant to use on a provider.
    ///
    /// Exact provider match first, then a CPU variant, then anything the policy
    /// permits. The fall-through matters: a machine with an accelerator whose
    /// variant is not installed must still run the model, only slower.
    #[must_use]
    pub fn variant_for(&self, ep: ExecutionProvider) -> Option<&Variant> {
        self.variants
            .iter()
            .find(|variant| variant.ep == ep && self.precision_policy.allows(variant.precision))
            .or_else(|| {
                self.variants.iter().find(|variant| {
                    variant.ep == ExecutionProvider::Cpu
                        && self.precision_policy.allows(variant.precision)
                })
            })
            .or_else(|| {
                self.variants
                    .iter()
                    .find(|variant| self.precision_policy.allows(variant.precision))
            })
    }
}

/// What the model expects to be handed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputSpec {
    /// Declared shape, with the batch dimension as written at export.
    pub shape: Vec<usize>,
    /// `NCHW` or `NHWC`.
    pub layout: String,
    /// Value range, e.g. `0..1`.
    pub range: String,
    /// Colour space of the pixels, e.g. `srgb`.
    pub colour: String,
}

/// One shipped file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Variant {
    /// Which provider this file is for.
    pub ep: ExecutionProvider,
    /// What precision it was exported at.
    pub precision: Precision,
    /// File name, relative to the models directory.
    pub file: String,
    /// Lower-case hex sha256 of the file.
    pub sha256: String,
    /// Size in bytes, checked before the digest so a truncated file fails fast.
    pub bytes: u64,
}

/// Which precisions a model may run at.
///
/// Section 12: "int8 quantisation degrades skin-critical models... retouch and
/// colour models default to fp16 and may forbid int8". A policy is the only place
/// that knowledge can live, because no measurement at load time can recover it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrecisionPolicy {
    /// True when this model must never be quantised to int8.
    pub forbid_int8: bool,
    /// True when this model must run at full precision.
    pub require_fp32: bool,
}

impl PrecisionPolicy {
    /// The default: any precision is acceptable.
    #[must_use]
    pub const fn permissive() -> Self {
        Self {
            forbid_int8: false,
            require_fp32: false,
        }
    }

    /// For skin, colour and retouch models.
    #[must_use]
    pub const fn no_int8() -> Self {
        Self {
            forbid_int8: true,
            require_fp32: false,
        }
    }

    /// True when a precision is permitted.
    #[must_use]
    pub const fn allows(self, precision: Precision) -> bool {
        match precision {
            Precision::Fp32 => true,
            Precision::Fp16 => !self.require_fp32,
            Precision::Int8 => !self.forbid_int8 && !self.require_fp32,
        }
    }
}

impl Default for PrecisionPolicy {
    fn default() -> Self {
        Self::permissive()
    }
}
