//! [`InferEngine`]: the implementation of the frozen service.
//!
//! Everything else in this crate is a part; this is the assembly. A request
//! arrives, the plan decides which provider answers, the source produces a
//! verified file, the pool produces a session, the scheduler admits the work, and
//! the result comes back with the provenance of the answer attached.
//!
//! Two behaviours are worth reading the code for.
//!
//! **Batching is transparent and reversible.** `run_batch` stacks inputs along
//! the batch axis, runs them as one, then splits the outputs back apart in the
//! order the caller supplied them - never in completion order. Determinism
//! invariant 4 covers ordering as much as arithmetic.
//!
//! **A batch that does not fit halves rather than fails.** The memory ledger
//! refuses the admission, the engine halves the chunk and retries, down to a
//! batch of one, and remembers what worked so the next run starts there.

use std::collections::BTreeMap;
use std::sync::Arc;

use aura_core::clock::Clock;
use aura_core::{AuraResult, Priority};
use parking_lot::Mutex;

use crate::backend::LoadedModel;
use crate::batch::{BatchScheduler, CancelToken, MemoryLedger, SchedulerStats};
use crate::contract::infer::{
    ExecutionProvider, HardwarePlan, InferError, InferRequest, InferResult, InferService,
    ModelClass, ModelRef, Tensor, TensorView,
};
use crate::ep::BackendRegistry;
use crate::session::{PoolStats, SessionPool};
use crate::source::{ModelSource, ResolvedModel};

/// The runtime.
#[derive(Debug)]
pub struct InferEngine {
    registry: BackendRegistry,
    source: Arc<dyn ModelSource>,
    pool: SessionPool,
    scheduler: BatchScheduler,
    plan: HardwarePlan,
    clock: Arc<dyn Clock>,
    cancel: CancelToken,
    learned_batch: Mutex<BTreeMap<String, u16>>,
}

impl InferEngine {
    /// Assemble a runtime from its parts.
    #[must_use]
    pub fn new(
        registry: BackendRegistry,
        source: Arc<dyn ModelSource>,
        plan: HardwarePlan,
        clock: Arc<dyn Clock>,
    ) -> Self {
        let ledger = if plan.vram_budget_mb == 0 {
            Arc::new(MemoryLedger::new(0))
        } else {
            Arc::new(MemoryLedger::new(
                u64::from(plan.vram_budget_mb) * 1024 * 1024,
            ))
        };
        Self {
            registry,
            source,
            pool: SessionPool::new(Arc::clone(&clock)),
            scheduler: BatchScheduler::new(Arc::clone(&clock), ledger),
            plan,
            clock,
            cancel: CancelToken::new(),
            learned_batch: Mutex::new(BTreeMap::new()),
        }
    }

    /// The provider that will answer the next request.
    #[must_use]
    pub fn selected_ep(&self) -> ExecutionProvider {
        self.registry.select(&self.plan)
    }

    /// The cancellation token every request checks.
    #[must_use]
    pub fn cancel_token(&self) -> CancelToken {
        self.cancel.clone()
    }

    /// Session pool counters, for Settings.
    #[must_use]
    pub fn pool_stats(&self) -> PoolStats {
        self.pool.stats()
    }

    /// Scheduler counters, for Settings and the performance tests.
    #[must_use]
    pub fn scheduler_stats(&self) -> SchedulerStats {
        self.scheduler.stats()
    }

    /// The memory high-water mark, in bytes.
    #[must_use]
    pub fn peak_memory_bytes(&self) -> u64 {
        self.scheduler.ledger().peak_bytes()
    }

    /// Batch sizes learned the hard way, to be folded into the plan on save.
    #[must_use]
    pub fn learned_batches(&self) -> BTreeMap<String, u16> {
        self.learned_batch.lock().clone()
    }

    /// Load a model without running it, and report what it cost.
    ///
    /// # Errors
    ///
    /// Whatever resolution or loading raised.
    pub fn load(&self, model: ModelRef) -> AuraResult<Arc<dyn LoadedModel>> {
        let ep = self.selected_ep();
        let resolved = self.source.resolve(model, ep)?;
        self.session(model, ep, &resolved)
    }

    /// The class a model belongs to, which decides its starting batch size.
    ///
    /// # Errors
    ///
    /// Whatever resolution raised.
    pub fn model_class(&self, model: ModelRef) -> AuraResult<ModelClass> {
        Ok(self.source.resolve(model, self.selected_ep())?.class)
    }

    fn session(
        &self,
        model: ModelRef,
        ep: ExecutionProvider,
        resolved: &ResolvedModel,
    ) -> AuraResult<Arc<dyn LoadedModel>> {
        let backend = self.registry.get(ep).ok_or_else(|| {
            crate::errors::no_accelerator(format!("no backend registered for {}", ep.as_str()))
        })?;
        self.pool.session(model, backend.as_ref(), resolved)
    }

    /// Run one already-batched set of inputs, under admission control.
    ///
    /// The memory charged is the session's fixed cost plus the model card's
    /// declared per-image working set multiplied by the batch. That split is what
    /// makes halving a batch relieve memory pressure: a charge that did not scale
    /// with the batch would refuse the same amount however small the retry.
    fn run_admitted(
        &self,
        admission: &Admission<'_>,
        inputs: &[TensorView<'_>],
    ) -> AuraResult<InferResult> {
        let Admission {
            model,
            session,
            prio,
            batch,
            per_item_bytes,
            ep,
            deadline,
        } = admission;
        let (batch, ep) = (*batch, *ep);

        let working_set = session.working_set_bytes()
            + input_bytes(inputs)
            + per_item_bytes * u64::from(batch.max(1));
        let started_us = self.clock.monotonic_us();
        let key = model.key();

        let outputs =
            self.scheduler
                .admit(*prio, working_set, &key, batch, &self.cancel, || {
                    session.run(inputs)
                })?;

        let elapsed_us = self.clock.monotonic_us().saturating_sub(started_us);
        let latency_ms = elapsed_us as f32 / 1000.0;

        if let Some(deadline) = deadline {
            let deadline_ms = u64::try_from(deadline.as_millis()).unwrap_or(u64::MAX);
            if elapsed_us / 1000 > deadline_ms {
                return Err(crate::errors::deadline_exceeded(
                    &key,
                    elapsed_us / 1000,
                    deadline_ms,
                ));
            }
        }

        tracing::debug!(
            target: "infer.run",
            model = key.as_str(),
            ep = ep.as_str(),
            batch,
            latency_ms,
            "inference complete"
        );

        Ok(InferResult {
            outputs,
            ep_used: ep,
            latency_ms,
            batch_size: batch,
        })
    }
}

impl InferService for InferEngine {
    fn run(&self, req: InferRequest<'_>) -> Result<InferResult, InferError> {
        let ep = self.selected_ep();
        let resolved = self.source.resolve(req.model, ep)?;
        let session = self.session(req.model, ep, &resolved)?;
        let batch = req
            .inputs
            .first()
            .and_then(|view| view.shape.first().copied())
            .unwrap_or(1) as u16;
        self.run_admitted(
            &Admission {
                model: req.model,
                session: &session,
                prio: req.prio,
                batch: batch.max(1),
                per_item_bytes: per_item_bytes(&resolved),
                ep,
                deadline: req.deadline,
            },
            &req.inputs,
        )
    }

    fn run_batch(
        &self,
        model: ModelRef,
        inputs: Vec<Vec<TensorView<'_>>>,
        prio: Priority,
    ) -> Result<Vec<InferResult>, InferError> {
        if inputs.is_empty() {
            return Ok(Vec::new());
        }
        let ep = self.selected_ep();
        let resolved = self.source.resolve(model, ep)?;
        let session = self.session(model, ep, &resolved)?;

        let key = model.key();
        let mut batch_size = self
            .learned_batch
            .lock()
            .get(&key)
            .copied()
            .unwrap_or_else(|| self.plan.start_batch(model.name, resolved.class));
        if let Some(limit) = session.max_batch() {
            batch_size = batch_size.min(limit).max(1);
        }

        let mut results: Vec<Option<InferResult>> = (0..inputs.len()).map(|_| None).collect();

        let mut position = 0usize;
        while position < inputs.len() {
            if self.cancel.is_cancelled() {
                self.scheduler.note_cancellation();
                return Err(crate::errors::cancelled(format!(
                    "{key} cancelled after {position} of {} items",
                    inputs.len()
                )));
            }

            let end = (position + usize::from(batch_size.max(1))).min(inputs.len());
            let chunk = inputs.get(position..end).unwrap_or_default();
            let stacked = stack(chunk)?;
            let views: Vec<TensorView<'_>> = stacked.iter().map(Tensor::view).collect();

            let admission = Admission {
                model,
                session: &session,
                prio,
                batch: u16::try_from(end - position).unwrap_or(u16::MAX),
                per_item_bytes: per_item_bytes(&resolved),
                ep,
                deadline: None,
            };
            match self.run_admitted(&admission, &views) {
                Ok(result) => {
                    let produced = split(&result, end - position)?;
                    for (offset, single) in produced.into_iter().enumerate() {
                        if let Some(slot) = results.get_mut(position + offset) {
                            *slot = Some(single);
                        }
                    }
                    self.learned_batch.lock().insert(key.clone(), batch_size);
                    position = end;
                }
                Err(err) if err.code.0 == "AURA-GPU-4003" && batch_size > 1 => {
                    // Halve and retry the same items. The successful size is
                    // remembered only when a chunk completes, so a machine never
                    // learns a size that did not work.
                    let halved = (batch_size / 2).max(1);
                    tracing::warn!(
                        target: "infer.oom_downshift",
                        model = key.as_str(),
                        from_batch = batch_size,
                        to_batch = halved,
                        "batch did not fit the memory budget"
                    );
                    batch_size = halved;
                }
                Err(err) => return Err(err),
            }
        }

        results
            .into_iter()
            .enumerate()
            .map(|(index, result)| {
                result.ok_or_else(|| {
                    crate::errors::shape_mismatch(
                        &format!("a result for item {index}"),
                        "nothing produced",
                    )
                })
            })
            .collect()
    }

    fn plan(&self) -> &HardwarePlan {
        &self.plan
    }

    fn warmup(&self, models: &[ModelRef]) -> Result<(), InferError> {
        crate::warmup::warmup(self, models, &|_| {})
    }
}

/// One request's admission parameters, gathered so the runner takes three
/// arguments rather than nine.
#[derive(Debug)]
struct Admission<'a> {
    model: ModelRef,
    session: &'a Arc<dyn LoadedModel>,
    prio: Priority,
    batch: u16,
    per_item_bytes: u64,
    ep: ExecutionProvider,
    deadline: Option<std::time::Duration>,
}

/// The per-image working set a model card declares, in bytes.
///
/// The card's number is the peak for one image; a batch of eight holds eight of
/// them. A model that declares nothing is charged a megabyte, which is wrong in
/// the safe direction.
fn per_item_bytes(resolved: &ResolvedModel) -> u64 {
    u64::from(resolved.working_set_mb.max(1)) * 1024 * 1024
}

/// Total bytes of a request's inputs.
fn input_bytes(inputs: &[TensorView<'_>]) -> u64 {
    inputs
        .iter()
        .map(|view| std::mem::size_of_val(view.data) as u64)
        .sum()
}

/// Stack per-item inputs into batched tensors, one per model input.
///
/// # Errors
///
/// `AURA-ML-5007` when the items disagree about how many inputs there are, or
/// about the shape of one of them.
fn stack(items: &[Vec<TensorView<'_>>]) -> AuraResult<Vec<Tensor>> {
    let first = items
        .first()
        .ok_or_else(|| crate::errors::shape_mismatch("at least one item", "an empty batch"))?;

    let mut stacked = Vec::with_capacity(first.len());
    for index in 0..first.len() {
        let reference = first
            .get(index)
            .ok_or_else(|| crate::errors::shape_mismatch("an input", "a missing input"))?;
        let per_item: Vec<usize> = reference.shape.iter().skip(1).copied().collect();
        let per_item_len: usize = per_item.iter().product();

        let mut samples = Vec::with_capacity(per_item_len * items.len());
        for item in items {
            let view = item.get(index).ok_or_else(|| {
                crate::errors::shape_mismatch(
                    &format!("{} inputs", first.len()),
                    &format!("{} inputs", item.len()),
                )
            })?;
            let tail: Vec<usize> = view.shape.iter().skip(1).copied().collect();
            if tail != per_item {
                return Err(crate::errors::shape_mismatch(
                    &format!("{per_item:?} per item"),
                    &format!("{tail:?}"),
                ));
            }
            samples.extend_from_slice(view.data);
        }

        let mut shape = vec![items.len()];
        shape.extend(per_item);
        stacked.push(Tensor::new(shape, samples)?);
    }
    Ok(stacked)
}

/// Split a batched result back into one result per item, in order.
///
/// # Errors
///
/// `AURA-ML-5007` when an output's leading dimension is not the batch size, which
/// would mean the model does not batch the way its manifest says it does.
fn split(result: &InferResult, count: usize) -> AuraResult<Vec<InferResult>> {
    let mut per_item: Vec<Vec<Tensor>> = (0..count).map(|_| Vec::new()).collect();

    for output in &result.outputs {
        let leading = output.shape.first().copied().unwrap_or(0);
        if leading != count {
            return Err(crate::errors::shape_mismatch(
                &format!("a leading dimension of {count}"),
                &output.shape_text(),
            ));
        }
        let stride = output.data.len() / count.max(1);
        for index in 0..count {
            let samples = output
                .data
                .get(index * stride..(index + 1) * stride)
                .unwrap_or_default();
            let mut shape = vec![1usize];
            shape.extend(output.shape.iter().skip(1).copied());
            if let Some(slot) = per_item.get_mut(index) {
                slot.push(Tensor::new(shape, samples.to_vec())?);
            }
        }
    }

    Ok(per_item
        .into_iter()
        .map(|outputs| InferResult {
            outputs,
            ep_used: result.ep_used,
            latency_ms: result.latency_ms / count.max(1) as f32,
            batch_size: result.batch_size,
        })
        .collect())
}
