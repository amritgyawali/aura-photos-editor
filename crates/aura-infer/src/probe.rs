//! Measuring the machine once, and writing down what was found.
//!
//! Section 6.1 of the phase document specifies the procedure and this module
//! follows it literally: enumerate the candidates in preference order, run a tiny
//! reference model twenty times on each, compare the result against the processor
//! within 1e-3, and take the median as the score. A candidate that crashes,
//! disagrees or hangs is set aside for this machine.
//!
//! Two details are ours rather than the document's.
//!
//! The first is the **ceiling**. The whole probe has fifteen seconds; past that it
//! is abandoned and the conservative plan is used for the session without being
//! persisted, so the next launch measures again rather than inheriting a
//! pessimistic guess. A probe that hangs on a broken driver must not become a
//! product that never starts.
//!
//! The second is that the probe **writes its reference model to disk**. Backends
//! load from a path, and a probe that special-cased an in-memory model would be
//! testing a path no real request takes.

use std::fs;
use std::sync::Arc;

use aura_core::clock::Clock;
use aura_core::AuraResult;

use crate::contract::infer::{
    BatchDefaults, ExecutionProvider, HardwarePlan, ModelClass, Precision,
};
use crate::ep::{assess, reference_output, BackendRegistry, Verdict};
use crate::onnx::{fixtures, serialise};
use crate::plan::PlanPaths;
use crate::source::ResolvedModel;

/// The ceiling on the whole probe, from section 13: "first run probes hardware in
/// <= 15 s".
pub const PROBE_CEILING_MS: u64 = 15_000;

/// Batch sizes for a machine with no accelerator.
///
/// Larger than the conservative plan because processor memory is measured in tens
/// of gigabytes, and smaller than an accelerator's because there is no throughput
/// to be won by batching a loop that is already sequential.
const CPU_BATCH: BatchDefaults = BatchDefaults {
    embedding: 8,
    segmentation: 2,
    retouch: 1,
};

/// The result of measuring a machine.
#[derive(Debug, Clone)]
pub struct ProbeOutcome {
    /// The plan to use.
    pub plan: HardwarePlan,
    /// True when the plan should be written to disk.
    ///
    /// False for the conservative plan: an abandoned probe must not persist its
    /// pessimism.
    pub persist: bool,
}

/// Measure a machine and produce a plan.
///
/// # Errors
///
/// `AURA-GPU-4004` when the probe's own scaffolding fails - the reference model
/// cannot be written, or no backend at all is registered. A *provider* failing is
/// not an error: that is what the set-aside list is for.
pub fn probe(
    registry: &BackendRegistry,
    paths: &PlanPaths,
    clock: &Arc<dyn Clock>,
) -> AuraResult<ProbeOutcome> {
    let started_ms = clock.monotonic_ms();
    let probed_at = format_utc(clock.as_ref());

    if registry.is_empty() {
        return Err(crate::errors::probe_failed(
            "no execution provider backend is registered in this build",
        ));
    }

    let model_path = write_probe_model(paths)?;
    let resolved = ResolvedModel {
        path: model_path,
        precision: Precision::Fp32,
        class: ModelClass::Embedding,
        working_set_mb: 1,
    };
    let input = fixtures::sample_input(1);

    // The processor path is the reference every other provider is judged against,
    // so it is measured first and never set aside: it is the floor.
    let cpu_backend = registry.get(ExecutionProvider::Cpu).ok_or_else(|| {
        crate::errors::probe_failed("no processor backend is registered in this build")
    })?;
    let reference = reference_output(cpu_backend.as_ref(), &resolved, &input)?;

    let mut plan = HardwarePlan::conservative(cpu_threads(), probed_at.clone());
    plan.default_batch = CPU_BATCH;
    plan.ep_order.clear();
    plan.unavailable = registry.unavailable();

    for ep in ExecutionProvider::probe_order() {
        if clock.monotonic_ms().saturating_sub(started_ms) > PROBE_CEILING_MS {
            let mut conservative = HardwarePlan::conservative(cpu_threads(), probed_at);
            conservative.unavailable = plan.unavailable;
            tracing::warn!(
                target: "infer.plan_selected",
                probed = false,
                reason = "ceiling",
                "hardware probe abandoned after its ceiling; using the conservative plan"
            );
            return Ok(ProbeOutcome {
                plan: conservative,
                persist: false,
            });
        }

        let Some(backend) = registry.get(ep) else {
            continue;
        };
        match assess(
            backend.as_ref(),
            &resolved,
            &input,
            &reference,
            clock.as_ref(),
        ) {
            Verdict::Usable { median_ms } => {
                plan.ep_order.push(ep);
                plan.probe_scores_ms
                    .insert(ep.as_str().to_string(), median_ms);
            }
            Verdict::SetAside { reason } => {
                crate::plan::set_aside(&mut plan, ep, &reason, &probed_at);
                tracing::warn!(
                    target: "infer.plan_selected",
                    ep = ep.as_str(),
                    reason = reason.as_str(),
                    "execution provider set aside for this machine"
                );
            }
            Verdict::Unavailable { reason } => {
                if !plan.unavailable.iter().any(|entry| entry.ep == ep) {
                    plan.unavailable
                        .push(crate::contract::infer::UnavailableProvider { ep, reason });
                }
            }
        }
    }

    if plan.ep_order.is_empty() {
        plan.ep_order.push(ExecutionProvider::Cpu);
    }
    plan.probed = true;

    if plan.ep_order == vec![ExecutionProvider::Cpu] {
        let accelerator_reasons = plan
            .unavailable
            .iter()
            .map(|entry| format!("{}: {}", entry.ep.as_str(), entry.reason))
            .collect::<Vec<_>>()
            .join("; ");
        tracing::info!(
            target: "infer.plan_selected",
            ep_order = "cpu",
            probe_ms = clock.monotonic_ms().saturating_sub(started_ms),
            "no accelerator available: {accelerator_reasons}"
        );
    }

    Ok(ProbeOutcome {
        plan,
        persist: true,
    })
}

/// Write the probe's reference model beside the plan, if it is not already there.
fn write_probe_model(paths: &PlanPaths) -> AuraResult<std::path::PathBuf> {
    fs::create_dir_all(&paths.directory).map_err(|err| {
        crate::errors::probe_failed(format!("probe directory could not be created: {err}"))
    })?;
    let path = paths.probe_model();
    let bytes = serialise(&fixtures::tiny_embedding());
    fs::write(&path, &bytes).map_err(|err| {
        crate::errors::probe_failed(format!("probe model could not be written: {err}"))
    })?;
    Ok(path)
}

/// Worker threads for processor-side work: every core but one.
///
/// The spare core is the same decision phase 02 made for the preview pool. A
/// machine whose every core is inside a batch is a machine whose interface stops
/// responding, and preemption that cannot get a core is not preemption.
#[must_use]
pub fn cpu_threads() -> u16 {
    let cores = std::thread::available_parallelism().map_or(2, std::num::NonZeroUsize::get);
    u16::try_from(cores.saturating_sub(1).max(1)).unwrap_or(1)
}

/// RFC 3339, seconds resolution, from the injected clock.
fn format_utc(clock: &dyn Clock) -> String {
    let now = clock.now_utc();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}
