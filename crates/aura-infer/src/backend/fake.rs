//! The test double: a backend that can be told to misbehave.
//!
//! Every failure this phase promises to survive belongs to hardware we do not
//! have. A provider that crashes on one driver revision, a session that runs out
//! of memory at batch 32 and succeeds at 16, a device whose numbers drift far
//! enough to fail the parity check - none of those can be produced on demand by a
//! real machine, and all of them have to be exercised.
//!
//! So this backend claims whichever provider a test asks it to claim, wraps the
//! real interpreter so its answers are real, and applies exactly one instructed
//! fault on top. It lives in `src/` rather than in `tests/` because the phase gate
//! binary uses it too: the gate proves the scheduler survives an out-of-memory
//! downshift on a machine that has no video memory to exhaust.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use aura_core::AuraResult;

use crate::backend::{Backend, LoadedModel};
use crate::contract::infer::{ExecutionProvider, Precision, Tensor, TensorView};
use crate::source::ResolvedModel;

/// What the fake backend does when it is asked to run.
#[derive(Debug, Clone, PartialEq)]
pub enum FakeBehaviour {
    /// Behave exactly like the reference backend.
    Correct,
    /// Refuse any batch larger than this, as a device out of memory would.
    OutOfMemoryAbove(u16),
    /// Add a constant to every output sample, as a miscompiled kernel would.
    Drift(f32),
    /// Fail every run, as an unstable driver would.
    Crash,
    /// Fail to load at all, as a missing runtime library would.
    Unavailable(String),
}

/// A backend that claims any provider and can be told to fail.
#[derive(Debug)]
pub struct FakeBackend {
    provider: ExecutionProvider,
    behaviour: FakeBehaviour,
    inner: crate::backend::reference::ReferenceBackend,
    loads: AtomicU64,
}

impl FakeBackend {
    /// A backend claiming `provider` and behaving as instructed.
    #[must_use]
    pub fn new(provider: ExecutionProvider, behaviour: FakeBehaviour) -> Self {
        Self {
            provider,
            behaviour,
            inner: crate::backend::reference::ReferenceBackend::new(),
            loads: AtomicU64::new(0),
        }
    }

    /// How many times a model has been loaded onto this backend.
    ///
    /// The session pool's whole purpose is to make this number small; a test that
    /// asserts pooling works asserts on this.
    #[must_use]
    pub fn load_count(&self) -> u64 {
        self.loads.load(Ordering::SeqCst)
    }
}

impl Backend for FakeBackend {
    fn provider(&self) -> ExecutionProvider {
        self.provider
    }

    fn availability(&self) -> Result<(), String> {
        match &self.behaviour {
            FakeBehaviour::Unavailable(reason) => Err(reason.clone()),
            _ => Ok(()),
        }
    }

    fn load(&self, model: &ResolvedModel) -> AuraResult<Arc<dyn LoadedModel>> {
        if let FakeBehaviour::Unavailable(reason) = &self.behaviour {
            return Err(crate::errors::provider_set_aside(
                self.provider.as_str(),
                reason,
            ));
        }
        self.loads.fetch_add(1, Ordering::SeqCst);
        let inner = self.inner.load(model)?;
        Ok(Arc::new(FakeSession {
            inner,
            behaviour: self.behaviour.clone(),
            provider: self.provider,
        }))
    }
}

/// One loaded model on the fake backend.
#[derive(Debug)]
struct FakeSession {
    inner: Arc<dyn LoadedModel>,
    behaviour: FakeBehaviour,
    provider: ExecutionProvider,
}

impl LoadedModel for FakeSession {
    fn run(&self, inputs: &[TensorView<'_>]) -> AuraResult<Vec<Tensor>> {
        match &self.behaviour {
            FakeBehaviour::Crash => Err(crate::errors::provider_set_aside(
                self.provider.as_str(),
                "crash",
            )),
            FakeBehaviour::OutOfMemoryAbove(limit) => {
                let batch = inputs
                    .first()
                    .and_then(|view| view.shape.first().copied())
                    .unwrap_or(1);
                if batch > usize::from(*limit) {
                    return Err(crate::errors::memory_downshift(
                        "fake",
                        batch as u16,
                        (batch as u16 / 2).max(1),
                    ));
                }
                self.inner.run(inputs)
            }
            FakeBehaviour::Drift(amount) => {
                let mut outputs = self.inner.run(inputs)?;
                for tensor in &mut outputs {
                    for value in &mut tensor.data {
                        *value += *amount;
                    }
                }
                Ok(outputs)
            }
            FakeBehaviour::Correct | FakeBehaviour::Unavailable(_) => self.inner.run(inputs),
        }
    }

    fn working_set_bytes(&self) -> u64 {
        self.inner.working_set_bytes()
    }

    fn precision(&self) -> Precision {
        self.inner.precision()
    }

    fn max_batch(&self) -> Option<u16> {
        match self.behaviour {
            FakeBehaviour::OutOfMemoryAbove(limit) => Some(limit),
            _ => self.inner.max_batch(),
        }
    }
}
