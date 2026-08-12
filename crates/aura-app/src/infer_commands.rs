//! The hardware and model half of the command surface.
//!
//! Three rules shape these commands, and they are the same three the preview
//! commands follow.
//!
//! **Nothing here blocks for long.** The probe has a fifteen-second ceiling and
//! runs once per machine; every other command reads state that is already in
//! memory. `warmup_models` is the exception and is explicitly the command that
//! pays a cost in front of the user, with the elapsed time returned so the panel
//! can say what it spent.
//!
//! **The panel tells the truth about what is missing.** Providers that are not
//! available appear with the reason rather than being hidden, because "AURA
//! cannot see my graphics card" is a support question that needs an answer on the
//! screen.
//!
//! **No paths cross the boundary.** Model files live under a user profile, and
//! Article IX rule S4 keeps paths out of anything that can reach a log or a
//! support bundle.

use aura_infer::contract::infer::{ExecutionProvider, HardwarePlan, InferService, ModelRef};

use crate::commands::IpcResult;
use crate::contract::ipc::{
    HardwarePlanDto, InferStatsDto, ModelStatusDto, ProbeScoreDto, ProviderNoteDto,
    SetExecutionProviderInput, WarmupReportDto,
};
use crate::state::AppState;

/// What the probe found, and what will answer the next request.
///
/// # Errors
///
/// `AURA-GPU-4004` when the machine cannot be measured at all.
pub fn hardware_plan(state: &AppState) -> IpcResult<HardwarePlanDto> {
    Ok(to_dto(&state.hardware_plan()?))
}

/// Measure the machine again, clearing any set-aside providers.
///
/// This is the "Re-check hardware" button, and it is the only way a provider that
/// failed its check gets another chance - which is deliberate: a machine that
/// silently retried a crashing driver on every launch would crash on every
/// launch.
///
/// # Errors
///
/// `AURA-GPU-4004` when the probe cannot finish.
pub fn recheck_hardware(state: &AppState) -> IpcResult<HardwarePlanDto> {
    Ok(to_dto(&state.recheck_hardware()?))
}

/// Choose a provider by hand, or return to the negotiated order with `auto`.
///
/// The override is honoured even when the negotiation disagrees - Article XXII
/// gives the user the right to disagree with the machine - unless the provider
/// was set aside for producing wrong numbers, which is a correctness question
/// rather than a preference.
///
/// # Errors
///
/// `AURA-GPU-4004` when there is no plan to change.
pub fn set_execution_provider(
    state: &AppState,
    input: &SetExecutionProviderInput,
) -> IpcResult<HardwarePlanDto> {
    let chosen = match input.ep.as_str() {
        "auto" | "" => None,
        "tensorrt" => Some(ExecutionProvider::TensorRt),
        "cuda" => Some(ExecutionProvider::Cuda),
        "directml" => Some(ExecutionProvider::DirectMl),
        "coreml" => Some(ExecutionProvider::CoreMl),
        _ => Some(ExecutionProvider::Cpu),
    };
    if let Some(ep) = chosen {
        tracing::info!(
            target: "infer.plan_selected",
            ep = ep.as_str(),
            overridden = true,
            "execution provider chosen by the user; this machine is unsupported in telemetry"
        );
    }
    Ok(to_dto(&state.set_execution_provider(chosen)?))
}

/// Every pinned model, with what is installed and what rolled back.
///
/// # Errors
///
/// `AURA-ML-5002` when the manifest does not verify, `AURA-ML-5001` when there is
/// no model directory at all.
pub fn list_models(state: &AppState) -> IpcResult<Vec<ModelStatusDto>> {
    let registry = state.model_registry()?;
    Ok(registry
        .lock()
        .models
        .iter()
        .map(|entry| {
            let installed = registry.state_of(&entry.name);
            ModelStatusDto {
                name: entry.name.clone(),
                version: entry.version.to_string(),
                task: entry.task.clone(),
                active_version: installed.active.map(|version| version.to_string()),
                pending_version: installed.pending.map(|version| version.to_string()),
                rejected_versions: installed.rejected.iter().map(ToString::to_string).collect(),
                model_card: entry.model_card.clone(),
                working_set_mb: entry.working_set_mb,
                file_count: u32::try_from(entry.variants.len()).unwrap_or(u32::MAX),
                int8_forbidden: entry.precision_policy.forbid_int8,
            }
        })
        .collect())
}

/// Load the models the next stage needs, and report what it cost.
///
/// # Errors
///
/// Whatever the loader raised. A pack with one broken model is a broken pack, and
/// finding that out at warmup is the entire point of having a warmup.
pub fn warmup_models(state: &AppState) -> IpcResult<WarmupReportDto> {
    let engine = state.infer_engine()?;
    let registry = state.model_registry()?;

    let models: Vec<ModelRef> = registry
        .lock()
        .models
        .iter()
        .map(|entry| ModelRef::new(intern(&entry.name), entry.version))
        .collect();

    let started = state.clock().monotonic_ms();
    engine.warmup(&models)?;
    let elapsed_ms = state.clock().monotonic_ms().saturating_sub(started);

    Ok(WarmupReportDto {
        loaded: u32::try_from(models.len()).unwrap_or(u32::MAX),
        elapsed_ms,
        ep_used: engine.selected_ep().as_str().to_string(),
    })
}

/// Runtime counters: what the pool and the scheduler have been doing.
///
/// # Errors
///
/// Whatever building the engine raised.
pub fn infer_stats(state: &AppState) -> IpcResult<InferStatsDto> {
    let engine = state.infer_engine()?;
    let pool = engine.pool_stats();
    let scheduler = engine.scheduler_stats();
    Ok(InferStatsDto {
        resident_sessions: u32::try_from(pool.resident).unwrap_or(u32::MAX),
        pool_hits: pool.hits,
        pool_loads: pool.loads,
        requests: scheduler.requests,
        downshifts: scheduler.downshifts,
        mean_overhead_ms: scheduler.mean_overhead_ms(),
        peak_memory_mb: engine.peak_memory_bytes() / (1024 * 1024),
    })
}

/// Intern a model name so it can be named by a `ModelRef`.
///
/// `ModelRef` holds a `&'static str` because phases 05 to 29 name their models as
/// compile-time constants. A runtime listing has owned strings, so they are
/// interned here rather than leaked per call: the set is bounded by the size of
/// the registry, and repeated names reuse one entry. This is the same mechanism
/// `ErrorCode` uses for codes arriving from the event stream.
fn intern(name: &str) -> &'static str {
    use std::collections::BTreeMap;

    use parking_lot::Mutex;

    static NAMES: Mutex<BTreeMap<String, &'static str>> = Mutex::new(BTreeMap::new());

    let mut names = NAMES.lock();
    if let Some(found) = names.get(name) {
        return found;
    }
    let leaked: &'static str = Box::leak(name.to_owned().into_boxed_str());
    names.insert(name.to_owned(), leaked);
    leaked
}

/// Project the internal plan onto the DTO the panel reads.
fn to_dto(plan: &HardwarePlan) -> HardwarePlanDto {
    HardwarePlanDto {
        gpu: plan.gpu.as_ref().map(|gpu| gpu.name.clone()),
        ep_order: plan
            .ep_order
            .iter()
            .map(|ep| ep.as_str().to_string())
            .collect(),
        selected_ep: plan.selected_ep().as_str().to_string(),
        override_ep: plan.ep_override.map(|ep| ep.as_str().to_string()),
        unavailable: plan
            .unavailable
            .iter()
            .map(|entry| ProviderNoteDto {
                ep: entry.ep.as_str().to_string(),
                reason: entry.reason.clone(),
            })
            .collect(),
        set_aside: plan
            .set_aside
            .iter()
            .map(|entry| ProviderNoteDto {
                ep: entry.ep.as_str().to_string(),
                reason: format!("{} ({})", entry.reason, entry.at),
            })
            .collect(),
        vram_budget_mb: plan.vram_budget_mb,
        cpu_threads: plan.cpu_threads,
        probe_scores_ms: plan
            .probe_scores_ms
            .iter()
            .map(|(ep, median_ms)| ProbeScoreDto {
                ep: ep.clone(),
                median_ms: *median_ms,
            })
            .collect(),
        probed_at: plan.probed_at.clone(),
        probed: plan.probed,
    }
}
