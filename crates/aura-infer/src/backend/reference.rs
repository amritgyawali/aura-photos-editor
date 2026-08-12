//! The reference backend: the interpreter from [`crate::onnx`], wired to the port.
//!
//! It claims [`ExecutionProvider::Cpu`] and is always available, which is what
//! makes the processor path the floor under every other row in the plan. Loading
//! reads the file, parses it, compiles it at the variant's precision and measures
//! the resident weight bytes so the scheduler has a real number rather than an
//! estimate.

use std::fs;
use std::sync::Arc;

use aura_core::AuraResult;

use crate::backend::{Backend, LoadedModel};
use crate::contract::infer::{ExecutionProvider, Precision, Tensor, TensorView};
use crate::errors::malformed;
use crate::onnx::{parse, Executable};
use crate::source::ResolvedModel;

/// Bytes of overhead a session holds beyond its weights.
///
/// This is the *fixed* part only - the parsed graph, the node list and the value
/// map's own structure. Activations dominate a real session and scale with the
/// batch, so they are charged per item by the engine from the model card's
/// declared working set rather than guessed at here. Splitting the two is what
/// makes halving a batch actually relieve memory pressure.
const SESSION_OVERHEAD_BYTES: u64 = 256 * 1024;

/// The deterministic interpreter, as a backend.
#[derive(Debug, Default)]
pub struct ReferenceBackend;

impl ReferenceBackend {
    /// A backend claiming the processor path.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Backend for ReferenceBackend {
    fn provider(&self) -> ExecutionProvider {
        ExecutionProvider::Cpu
    }

    fn availability(&self) -> Result<(), String> {
        Ok(())
    }

    fn load(&self, model: &ResolvedModel) -> AuraResult<Arc<dyn LoadedModel>> {
        // The path is redacted rather than logged: S4 forbids a file path in a
        // log line, and a model directory sits under a user's profile.
        let bytes = fs::read(&model.path).map_err(|err| {
            malformed(format!(
                "model file {} could not be read: {err}",
                aura_core::redact::redact_path(&model.path)
            ))
        })?;
        let parsed = parse(&bytes)?;
        let executable = Executable::compile(&parsed, model.precision)?;
        let weight_bytes = (executable.parameter_count() * std::mem::size_of::<f32>()) as u64;
        Ok(Arc::new(ReferenceSession {
            executable,
            working_set_bytes: weight_bytes + SESSION_OVERHEAD_BYTES,
        }))
    }
}

/// One compiled model, ready to run.
#[derive(Debug)]
pub struct ReferenceSession {
    executable: Executable,
    working_set_bytes: u64,
}

impl ReferenceSession {
    /// The compiled graph, for the parity harness and the benchmarks.
    #[must_use]
    pub const fn executable(&self) -> &Executable {
        &self.executable
    }
}

impl LoadedModel for ReferenceSession {
    fn run(&self, inputs: &[TensorView<'_>]) -> AuraResult<Vec<Tensor>> {
        self.executable.run(inputs)
    }

    fn working_set_bytes(&self) -> u64 {
        self.working_set_bytes
    }

    fn precision(&self) -> Precision {
        self.executable.precision()
    }
}
