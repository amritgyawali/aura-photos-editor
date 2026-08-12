//! The backend port: what an execution provider has to be able to do.
//!
//! This is deliberately **not** a frozen contract. `InferService` is what phases
//! 05 to 29 see; how many backends sit behind it, and whether one of them wraps
//! ONNX Runtime, has to be free to change without an ADR and without re-locking
//! `contracts.lock`. ADR-0007 says so explicitly.
//!
//! Two implementations ship: [`ReferenceBackend`], the deterministic interpreter
//! that answers every request in this build, and [`FakeBackend`], the test double
//! that can be told to crash, to run out of memory, or to return numbers that
//! drift from the processor's. Article II rule A3 asks for exactly that pair - if
//! it cannot be faked, it cannot be tested, and a GPU failure path that has never
//! executed is a failure path we do not have.

pub mod fake;
pub mod reference;

use std::fmt;
use std::sync::Arc;

use aura_core::AuraResult;

use crate::contract::infer::{ExecutionProvider, Precision, Tensor, TensorView};
use crate::source::ResolvedModel;

pub use fake::{FakeBackend, FakeBehaviour};
pub use reference::ReferenceBackend;

/// A model that has been loaded onto a provider and can be run.
pub trait LoadedModel: Send + Sync + fmt::Debug {
    /// Run one batch through the model.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5007` on a shape disagreement, `AURA-GPU-4003` when the batch
    /// does not fit and the caller must retry smaller, `AURA-ML-5010` when the
    /// graph turns out to be impossible to execute.
    fn run(&self, inputs: &[TensorView<'_>]) -> AuraResult<Vec<Tensor>>;

    /// Peak resident bytes this session holds, measured at warmup.
    ///
    /// The scheduler admits work against this number. A backend that guesses low
    /// here is how a machine runs out of memory at image 2,800 of 3,000.
    fn working_set_bytes(&self) -> u64;

    /// The precision this session runs at.
    fn precision(&self) -> Precision;

    /// The largest batch this session will accept, if it has a limit.
    ///
    /// `None` means "whatever memory allows"; a fixed-shape session returns the
    /// batch its graph was built for.
    fn max_batch(&self) -> Option<u16> {
        None
    }
}

/// One execution provider's implementation.
pub trait Backend: Send + Sync + fmt::Debug {
    /// Which provider this backend claims.
    fn provider(&self) -> ExecutionProvider;

    /// Whether this machine and this build can use it.
    ///
    /// # Errors
    ///
    /// A sentence for the Settings panel - "no CUDA driver", "not compiled into
    /// this build" - not a stack trace. It is shown to a photographer.
    fn availability(&self) -> Result<(), String>;

    /// Load a verified model file.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5010` when the file cannot be read or parsed, `AURA-ML-5006` when
    /// it uses an operator this backend does not implement.
    fn load(&self, model: &ResolvedModel) -> AuraResult<Arc<dyn LoadedModel>>;
}
