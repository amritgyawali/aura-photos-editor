//! Which execution provider answers, and what happens when one misbehaves.
//!
//! The registry holds one backend per provider and hands out the first one the
//! plan permits. Two rules make that safe on hardware nobody in this project
//! owns:
//!
//! * **A provider must prove itself.** Before it is used it runs a small model
//!   and its output is compared against the processor's within 1e-3. A driver
//!   that returns wrong numbers is worse than one that crashes, because wrong
//!   numbers reach the ledger and the album.
//! * **A provider that fails is set aside per machine, not per run.** The entry
//!   is written into `hardware_plan.json` with the reason, survives restarts, and
//!   is cleared only by an explicit re-check. A machine that crashes on every
//!   launch is a machine whose owner uninstalls the product.

use std::sync::Arc;

use aura_core::AuraResult;

use crate::backend::Backend;
use crate::contract::infer::{ExecutionProvider, HardwarePlan, UnavailableProvider};
use crate::source::ResolvedModel;
use crate::tensor::max_abs_diff;

/// How far a provider's answer may drift from the processor's before it is set
/// aside. Section 6.1 fixes the number; it is also `Precision::Fp16`'s parity
/// tolerance, which is not a coincidence - a device running half precision is the
/// normal case.
pub const CORRECTNESS_TOLERANCE: f32 = 1e-3;

/// How many times the probe runs a candidate before taking a median.
pub const PROBE_RUNS: usize = 20;

/// Every backend this build registered, in probe order.
#[derive(Debug, Default)]
pub struct BackendRegistry {
    backends: Vec<Arc<dyn Backend>>,
}

impl BackendRegistry {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            backends: Vec::new(),
        }
    }

    /// The registry this build ships with: the reference interpreter only.
    ///
    /// When an ONNX Runtime backend lands it registers here and nothing else in
    /// the crate changes - which is the whole reason the port exists.
    #[must_use]
    pub fn with_reference() -> Self {
        Self::new().register(Arc::new(crate::backend::ReferenceBackend::new()))
    }

    /// Add a backend. A second backend for the same provider replaces the first,
    /// so a test can substitute a fake for the reference.
    #[must_use]
    pub fn register(mut self, backend: Arc<dyn Backend>) -> Self {
        self.backends
            .retain(|existing| existing.provider() != backend.provider());
        self.backends.push(backend);
        self
    }

    /// The backend claiming a provider, if any.
    #[must_use]
    pub fn get(&self, ep: ExecutionProvider) -> Option<&Arc<dyn Backend>> {
        self.backends
            .iter()
            .find(|backend| backend.provider() == ep)
    }

    /// Providers with a registered backend, in probe order.
    #[must_use]
    pub fn providers(&self) -> Vec<ExecutionProvider> {
        ExecutionProvider::probe_order()
            .into_iter()
            .filter(|ep| self.get(*ep).is_some())
            .collect()
    }

    /// Providers with no backend, or whose backend says it cannot run here.
    #[must_use]
    pub fn unavailable(&self) -> Vec<UnavailableProvider> {
        ExecutionProvider::probe_order()
            .into_iter()
            .filter_map(|ep| match self.get(ep) {
                None => Some(UnavailableProvider {
                    ep,
                    reason: "not compiled into this build".to_string(),
                }),
                Some(backend) => backend
                    .availability()
                    .err()
                    .map(|reason| UnavailableProvider { ep, reason }),
            })
            .collect()
    }

    /// The first usable provider for a plan.
    ///
    /// Honours a user override when the chosen provider is registered and not set
    /// aside - Article XXII says a user may disagree with the machine - and falls
    /// through the plan's order otherwise. The processor path is the floor, so
    /// this never returns nothing.
    #[must_use]
    pub fn select(&self, plan: &HardwarePlan) -> ExecutionProvider {
        if let Some(chosen) = plan.ep_override {
            if !plan.is_set_aside(chosen) && self.get(chosen).is_some() {
                return chosen;
            }
        }
        for ep in &plan.ep_order {
            if !plan.is_set_aside(*ep) && self.get(*ep).is_some() {
                return *ep;
            }
        }
        ExecutionProvider::Cpu
    }

    /// How many backends are registered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.backends.len()
    }

    /// True when nothing is registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.backends.is_empty()
    }
}

/// The verdict on one candidate provider.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    /// Usable, with a median run time in milliseconds.
    Usable {
        /// Median of [`PROBE_RUNS`] runs.
        median_ms: f32,
    },
    /// Set aside, with the reason recorded in the plan.
    SetAside {
        /// `crash`, `mismatch` or `timeout`.
        reason: String,
    },
    /// Nothing claims this provider on this machine.
    Unavailable {
        /// A sentence for the Settings panel.
        reason: String,
    },
}

/// Run one candidate against a reference result and time it.
///
/// The reference is the processor's own answer, computed by the caller once. A
/// provider that disagrees beyond [`CORRECTNESS_TOLERANCE`], throws, or cannot
/// load is set aside with the reason rather than being tried again next launch.
#[must_use]
pub fn assess(
    backend: &dyn Backend,
    model: &ResolvedModel,
    input: &crate::contract::infer::Tensor,
    reference: &[f32],
    clock: &dyn aura_core::clock::Clock,
) -> Verdict {
    if let Err(reason) = backend.availability() {
        return Verdict::Unavailable { reason };
    }

    let session = match backend.load(model) {
        Ok(session) => session,
        Err(err) => {
            return Verdict::SetAside {
                reason: format!("crash: {}", err.code),
            }
        }
    };

    let mut timings_us = Vec::with_capacity(PROBE_RUNS);
    let mut last: Vec<f32> = Vec::new();
    for _ in 0..PROBE_RUNS {
        let started = clock.monotonic_us();
        match session.run(&[input.view()]) {
            Ok(outputs) => {
                last = outputs.first().map(|t| t.data.clone()).unwrap_or_default();
            }
            Err(_) => {
                return Verdict::SetAside {
                    reason: "crash".to_string(),
                }
            }
        }
        timings_us.push(clock.monotonic_us().saturating_sub(started));
    }

    match max_abs_diff(&last, reference) {
        Some(worst) if worst <= CORRECTNESS_TOLERANCE => {}
        Some(worst) => {
            return Verdict::SetAside {
                reason: format!(
                    "mismatch: worst {worst:.2e} against a {CORRECTNESS_TOLERANCE:.0e} tolerance"
                ),
            }
        }
        None => {
            return Verdict::SetAside {
                reason: "mismatch: output shape differs from the processor path".to_string(),
            }
        }
    }

    timings_us.sort_unstable();
    let median_us = timings_us
        .get(timings_us.len() / 2)
        .copied()
        .unwrap_or_default();
    Verdict::Usable {
        median_ms: median_us as f32 / 1000.0,
    }
}

/// Compute the processor's answer, which every other provider is judged against.
///
/// # Errors
///
/// Whatever loading or running the reference model raised.
pub fn reference_output(
    backend: &dyn Backend,
    model: &ResolvedModel,
    input: &crate::contract::infer::Tensor,
) -> AuraResult<Vec<f32>> {
    let session = backend.load(model)?;
    let outputs = session.run(&[input.view()])?;
    Ok(outputs.first().map(|t| t.data.clone()).unwrap_or_default())
}
