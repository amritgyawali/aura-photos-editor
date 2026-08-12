//! FROZEN CONTRACT. The one way to run a model.
//!
//! Phase 03 section 5 freezes this before any runtime exists, and the ordering is
//! the point: phases 05 to 29 are written against `InferService`, so none of them
//! can grow a dependency on *which* runtime or *which* piece of hardware answered.
//! A caller names a model, hands over tensors and a priority, and gets back
//! tensors plus the provenance of the answer.
//!
//! What is deliberately **not** frozen is the `Backend` port inside this crate.
//! `InferService` is what callers see; how many backends sit behind it, and
//! whether one of them is ONNX Runtime, is an implementation detail that must be
//! free to change without an ADR. See `docs/adr/ADR-0007-inference-runtime.md`.
//!
//! Changing anything in this file requires an ADR.

use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use aura_core::{AuraError, Priority};
use serde::{Deserialize, Serialize};

/// A semantic version of a model artefact.
///
/// Models are pinned by name *and* version *and* digest. The version is what a
/// human reads in a model card and a ledger entry; the digest is what the loader
/// trusts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    /// Breaking change to the model's task, inputs or outputs.
    pub major: u16,
    /// New capability, same contract.
    pub minor: u16,
    /// Retraining or a fix with no contract change.
    pub patch: u16,
}

/// Versions serialise as `"1.4.2"`, not as three fields.
///
/// The `models.lock` schema in the phase document is a published format and
/// spells a version that way. A struct-shaped version would be a silent change to
/// a file other tools read.
impl Serialize for Version {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(|_| {
            serde::de::Error::custom(format!("{text} is not a major.minor.patch version"))
        })
    }
}

impl Version {
    /// Build a version from its three parts.
    #[must_use]
    pub const fn new(major: u16, minor: u16, patch: u16) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl FromStr for Version {
    type Err = AuraError;

    /// Parse `major.minor.patch`.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5001` when the text is not three numeric parts. A version we
    /// cannot parse is a version we cannot pin, and an unpinned model breaks
    /// determinism invariant 4.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let mut parts = text.split('.');
        let mut next = || -> Option<u16> { parts.next()?.parse().ok() };
        match (next(), next(), next(), parts.next()) {
            (Some(major), Some(minor), Some(patch), None) => Ok(Self {
                major,
                minor,
                patch,
            }),
            _ => Err(aura_core::errors::ml::model_unknown(text, "unparseable")),
        }
    }
}

/// The identity of a model in a request: a pinned name and version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ModelRef {
    /// Registry name, e.g. `scene_classifier`.
    pub name: &'static str,
    /// Pinned version.
    pub version: Version,
}

impl ModelRef {
    /// Name a model and a version.
    #[must_use]
    pub const fn new(name: &'static str, version: Version) -> Self {
        Self { name, version }
    }

    /// `name@major.minor.patch`, the key used in telemetry and in the pool.
    #[must_use]
    pub fn key(&self) -> String {
        format!("{}@{}", self.name, self.version)
    }
}

impl fmt::Display for ModelRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.name, self.version)
    }
}

/// Where a model ran, or could run.
///
/// All five stay in the contract whatever this build can compile. A provider that
/// is not available is reported as unavailable with a reason - never hidden -
/// because "AURA cannot see my graphics card" is a support question that needs an
/// answer on the screen rather than in a log file.
///
/// The serialised spellings are the ones the phase document's `hardware_plan.json`
/// and `models.lock` examples use - `tensorrt`, `directml`, `coreml` - rather than
/// what a derived `snake_case` rename would produce. Those files are read by
/// tooling outside this repository; the spelling is part of the format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ExecutionProvider {
    /// NVIDIA, engine-compiled. Fastest, slowest to warm up.
    #[serde(rename = "tensorrt")]
    TensorRt,
    /// NVIDIA, general.
    #[serde(rename = "cuda")]
    Cuda,
    /// Windows, any DirectX 12 device including integrated graphics.
    #[serde(rename = "directml")]
    DirectMl,
    /// Apple Silicon.
    #[serde(rename = "coreml")]
    CoreMl,
    /// Always available. The floor under every other row.
    #[serde(rename = "cpu")]
    Cpu,
}

impl ExecutionProvider {
    /// Stable text for `hardware_plan.json` and telemetry.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TensorRt => "tensorrt",
            Self::Cuda => "cuda",
            Self::DirectMl => "directml",
            Self::CoreMl => "coreml",
            Self::Cpu => "cpu",
        }
    }

    /// The probe order from section 6.1, most preferred first.
    #[must_use]
    pub const fn probe_order() -> [Self; 5] {
        [
            Self::TensorRt,
            Self::Cuda,
            Self::DirectMl,
            Self::CoreMl,
            Self::Cpu,
        ]
    }

    /// True when work on this provider is accounted against video memory.
    #[must_use]
    pub const fn uses_vram(self) -> bool {
        !matches!(self, Self::Cpu)
    }
}

/// The numeric precision a session runs at.
///
/// Precision is a property of the loaded variant, not of the request: a caller
/// asks for a model, and the plan plus the model's own precision policy decide
/// what it runs as. Retouch and colour models may forbid `Int8` outright.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Precision {
    /// The reference. Every parity comparison is against this.
    Fp32,
    /// Half precision. Parity tolerance 1e-3.
    Fp16,
    /// Per-tensor affine quantisation. Parity tolerance 1e-2.
    Int8,
}

impl Precision {
    /// Stable text for the manifest and telemetry.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fp32 => "fp32",
            Self::Fp16 => "fp16",
            Self::Int8 => "int8",
        }
    }

    /// The parity tolerance this precision must hold against `Fp32`.
    ///
    /// Section 6.4: 1e-3 for half precision, 1e-2 for int8.
    #[must_use]
    pub const fn parity_tolerance(self) -> f32 {
        match self {
            Self::Fp32 => 0.0,
            Self::Fp16 => 1e-3,
            Self::Int8 => 1e-2,
        }
    }
}

/// An owned tensor crossing the service boundary.
///
/// Everything at this boundary is `f32` in row-major order, whatever the session
/// runs at internally. Quantisation is the runtime's business, not the caller's:
/// a caller that had to know a model was int8 would have to change when the model
/// is requantised, which is precisely the coupling this crate exists to prevent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tensor {
    /// Dimensions, outermost first. `[1, 3, 384, 384]` is NCHW.
    pub shape: Vec<usize>,
    /// `shape.iter().product()` samples in row-major order.
    pub data: Vec<f32>,
}

impl Tensor {
    /// Wrap data in a shape, checking that the two agree.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5007` when the element count does not match the shape. A tensor
    /// whose shape lies is worse than no tensor: it produces a plausible number.
    pub fn new(shape: Vec<usize>, samples: Vec<f32>) -> Result<Self, InferError> {
        let want: usize = shape.iter().product();
        if want != samples.len() {
            return Err(aura_core::errors::ml::shape_mismatch(
                &format!("{shape:?} ({want} samples)"),
                &format!("{} samples", samples.len()),
            ));
        }
        Ok(Self {
            shape,
            data: samples,
        })
    }

    /// A tensor of zeros with the given shape.
    #[must_use]
    pub fn zeros(shape: Vec<usize>) -> Self {
        let count: usize = shape.iter().product();
        Self {
            shape,
            data: vec![0.0; count],
        }
    }

    /// Total number of samples.
    #[must_use]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// True when the tensor holds no samples.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Borrow this tensor as a view.
    #[must_use]
    pub fn view(&self) -> TensorView<'_> {
        TensorView {
            shape: self.shape.clone(),
            data: &self.data,
        }
    }

    /// Human-readable shape, for error messages and logs.
    #[must_use]
    pub fn shape_text(&self) -> String {
        format!("{:?}", self.shape)
    }
}

/// A borrowed tensor handed to the runtime.
///
/// Inputs are borrowed rather than owned so that a batch of 32 proxies is not
/// copied into the request. The runtime copies once, into the session's bound
/// input buffer, and never again.
#[derive(Debug, Clone, PartialEq)]
pub struct TensorView<'a> {
    /// Dimensions, outermost first.
    pub shape: Vec<usize>,
    /// Borrowed samples, row-major.
    pub data: &'a [f32],
}

impl<'a> TensorView<'a> {
    /// Borrow a slice as a tensor of the given shape.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5007` when the element count does not match the shape.
    pub fn new(shape: Vec<usize>, samples: &'a [f32]) -> Result<Self, InferError> {
        let want: usize = shape.iter().product();
        if want != samples.len() {
            return Err(aura_core::errors::ml::shape_mismatch(
                &format!("{shape:?} ({want} samples)"),
                &format!("{} samples", samples.len()),
            ));
        }
        Ok(Self {
            shape,
            data: samples,
        })
    }

    /// Copy into an owned tensor.
    #[must_use]
    pub fn to_owned_tensor(&self) -> Tensor {
        Tensor {
            shape: self.shape.clone(),
            data: self.data.to_vec(),
        }
    }

    /// Human-readable shape, for error messages and logs.
    #[must_use]
    pub fn shape_text(&self) -> String {
        format!("{:?}", self.shape)
    }
}

/// One inference request.
#[derive(Debug, Clone)]
pub struct InferRequest<'a> {
    /// Which model, pinned.
    pub model: ModelRef,
    /// Inputs in the order the model declares them.
    pub inputs: Vec<TensorView<'a>>,
    /// Scheduling class. `Visible` and `Interactive` preempt batch work.
    pub prio: Priority,
    /// Optional wall-clock ceiling. `None` means "as long as it takes".
    pub deadline: Option<Duration>,
}

impl<'a> InferRequest<'a> {
    /// A request with no deadline, at the given priority.
    #[must_use]
    pub fn new(model: ModelRef, inputs: Vec<TensorView<'a>>, prio: Priority) -> Self {
        Self {
            model,
            inputs,
            prio,
            deadline: None,
        }
    }

    /// Attach a deadline.
    #[must_use]
    pub fn with_deadline(mut self, deadline: Duration) -> Self {
        self.deadline = Some(deadline);
        self
    }
}

/// One inference result, with the provenance of the answer.
///
/// `ep_used` and `batch_size` are not diagnostics: invariant 2 says every AI
/// decision carries its reasons, and "which provider produced this number" is
/// part of the reason when two machines disagree by 1e-4.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InferResult {
    /// Outputs in the order the model declares them.
    pub outputs: Vec<Tensor>,
    /// Which provider ran it.
    pub ep_used: ExecutionProvider,
    /// Wall-clock time inside the runtime, excluding queue time.
    pub latency_ms: f32,
    /// How many items were in the batch this request rode in.
    pub batch_size: u16,
}

/// The error type of this crate.
///
/// Article III has exactly one error type crossing a module boundary, so the
/// `InferError` named in the phase document is an alias rather than a second
/// taxonomy. Codes are `AURA-GPU-4xxx` for hardware and `AURA-ML-5xxx` for
/// models and execution.
pub type InferError = AuraError;

/// The one way to run a model.
pub trait InferService: Send + Sync + fmt::Debug {
    /// Run one request, waiting for it.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5001` when the model is not pinned, `AURA-ML-5007` on a shape
    /// mismatch, `AURA-ML-5008` when a deadline elapses, `AURA-ML-5011` when the
    /// work is cancelled, and whatever the loader raised if the model has not
    /// been loaded before.
    fn run(&self, req: InferRequest<'_>) -> Result<InferResult, InferError>;

    /// Run many inputs through one model, batched by the scheduler.
    ///
    /// Results are returned in the order the inputs were given, whatever order
    /// the batches completed in - determinism invariant 4 applies to ordering as
    /// much as to arithmetic.
    ///
    /// # Errors
    ///
    /// As [`InferService::run`]. A batch that will not fit is halved rather than
    /// failed, so `AURA-GPU-4003` is a warning in the log, not an error here.
    fn run_batch(
        &self,
        model: ModelRef,
        inputs: Vec<Vec<TensorView<'_>>>,
        prio: Priority,
    ) -> Result<Vec<InferResult>, InferError>;

    /// The hardware plan in force: provider order, budgets, batch sizes.
    fn plan(&self) -> &HardwarePlan;

    /// Load and prepare models before they are needed.
    ///
    /// # Errors
    ///
    /// Whatever the loader raised. Warmup failing is a real failure: it is how
    /// the product learns that a model pack is broken *before* a photographer is
    /// waiting on it.
    fn warmup(&self, models: &[ModelRef]) -> Result<(), InferError>;
}

/// What the probe found and decided. Persisted as `hardware_plan.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HardwarePlan {
    /// Schema version of this file. Bumped when fields change meaning.
    pub schema: u32,
    /// The accelerator, when one was found.
    pub gpu: Option<GpuInfo>,
    /// Providers to try, most preferred first. Never empty: `cpu` is the floor.
    pub ep_order: Vec<ExecutionProvider>,
    /// Ceiling the scheduler admits work against, in megabytes.
    pub vram_budget_mb: u32,
    /// Starting batch sizes per model class.
    pub default_batch: BatchDefaults,
    /// Worker threads for processor-side work.
    pub cpu_threads: u16,
    /// Median micro-benchmark time per provider, in milliseconds.
    pub probe_scores_ms: std::collections::BTreeMap<String, f32>,
    /// RFC 3339 timestamp of the probe, from the injected clock.
    pub probed_at: String,
    /// False when the probe was abandoned and this is the conservative plan.
    pub probed: bool,
    /// Providers set aside on this machine, with the reason.
    pub set_aside: Vec<SetAsideProvider>,
    /// Providers found unavailable, with the reason shown in Settings.
    pub unavailable: Vec<UnavailableProvider>,
    /// Batch sizes that were learned the hard way, keyed by model name.
    pub remembered_batch: std::collections::BTreeMap<String, u16>,
    /// A user's explicit choice, honoured but marked unsupported in telemetry.
    pub ep_override: Option<ExecutionProvider>,
}

/// What was found on the other end of the graphics driver.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuInfo {
    /// `nvidia`, `amd`, `intel`, `apple`.
    pub vendor: String,
    /// Marketing name as reported by the driver.
    pub name: String,
    /// Total video memory in megabytes.
    pub vram_mb: u32,
    /// Driver version string, verbatim.
    pub driver: String,
}

/// Starting batch sizes, per model class.
///
/// Three classes rather than per-model numbers, because a per-model table would
/// be a configuration file nobody maintains. The scheduler learns the real number
/// per model at runtime and stores it in `remembered_batch`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchDefaults {
    /// Embedding and classification: small tensors, many of them.
    pub embedding: u16,
    /// Segmentation and masks: large activations.
    pub segmentation: u16,
    /// Retouch and restoration: the largest working sets.
    pub retouch: u16,
}

impl BatchDefaults {
    /// The smallest plan that is always safe.
    #[must_use]
    pub const fn conservative() -> Self {
        Self {
            embedding: 4,
            segmentation: 1,
            retouch: 1,
        }
    }
}

/// A provider that failed its check on this machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetAsideProvider {
    /// Which provider.
    pub ep: ExecutionProvider,
    /// `crash`, `mismatch` or `timeout`.
    pub reason: String,
    /// RFC 3339 timestamp of the decision.
    pub at: String,
}

/// A provider that is not present on this machine or in this build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnavailableProvider {
    /// Which provider.
    pub ep: ExecutionProvider,
    /// A sentence for the Settings panel, not a stack trace.
    pub reason: String,
}

impl HardwarePlan {
    /// The current schema version of `hardware_plan.json`.
    pub const SCHEMA: u32 = 1;

    /// The plan used when the probe could not finish: processor only, smallest
    /// batches, half the detected cores.
    #[must_use]
    pub fn conservative(cpu_threads: u16, probed_at: String) -> Self {
        Self {
            schema: Self::SCHEMA,
            gpu: None,
            ep_order: vec![ExecutionProvider::Cpu],
            vram_budget_mb: 0,
            default_batch: BatchDefaults::conservative(),
            cpu_threads: cpu_threads.max(1),
            probe_scores_ms: std::collections::BTreeMap::new(),
            probed_at,
            probed: false,
            set_aside: Vec::new(),
            unavailable: Vec::new(),
            remembered_batch: std::collections::BTreeMap::new(),
            ep_override: None,
        }
    }

    /// The provider that will actually be used: the override when the user set
    /// one and it is not set aside, otherwise the head of the order.
    #[must_use]
    pub fn selected_ep(&self) -> ExecutionProvider {
        if let Some(chosen) = self.ep_override {
            if !self.is_set_aside(chosen) {
                return chosen;
            }
        }
        self.ep_order
            .first()
            .copied()
            .unwrap_or(ExecutionProvider::Cpu)
    }

    /// True when this provider failed a check on this machine.
    #[must_use]
    pub fn is_set_aside(&self, ep: ExecutionProvider) -> bool {
        self.set_aside.iter().any(|entry| entry.ep == ep)
    }

    /// The batch size to start with for a model, preferring what was learned.
    #[must_use]
    pub fn start_batch(&self, model: &str, class: ModelClass) -> u16 {
        if let Some(remembered) = self.remembered_batch.get(model) {
            return (*remembered).max(1);
        }
        match class {
            ModelClass::Embedding => self.default_batch.embedding,
            ModelClass::Segmentation => self.default_batch.segmentation,
            ModelClass::Retouch => self.default_batch.retouch,
        }
        .max(1)
    }
}

/// Which batch-size column a model belongs in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelClass {
    /// Embeddings, classifiers, small heads.
    Embedding,
    /// Segmentation and mask models.
    Segmentation,
    /// Retouch, restoration and anything generative.
    Retouch,
}

impl ModelClass {
    /// Stable text for the manifest.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Embedding => "embedding",
            Self::Segmentation => "segmentation",
            Self::Retouch => "retouch",
        }
    }
}
