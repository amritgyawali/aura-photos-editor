# ADR-0008 - Extending the frozen IPC surface for hardware and models

- **Status:** accepted
- **Date:** 2026-08-12
- **Deciders:** CTO, TLC, SFE (Senior Frontend Engineer), SEC
- **Phase:** 03

## Context

`crates/aura-app/src/contract/ipc.rs` and `ui/src/ipc/types.ts` are frozen
contracts, digested in `contracts.lock`. Section 9 of the phase document assigns
SFE a "Settings > Hardware panel (detected GPU, EP in use, override, warmup
progress, model versions)", and acceptance criterion 2 requires that "Settings
shows the selected EP and lets the user override". Neither is possible without
new commands. Changing a frozen contract requires an ADR first and a re-lock
second; this is that ADR, and it follows the pattern ADR-0005 set in phase 02.

## Decision

Six commands and one event stream are added. Nothing existing changes, so every
phase 01 and phase 02 caller keeps working.

| Command | Returns | Why it exists |
|---|---|---|
| `hardware_plan` | `HardwarePlanDto` | What the probe found and what will run |
| `recheck_hardware` | `HardwarePlanDto` | Re-measure after a driver update; clears the set-aside list |
| `set_execution_provider` | `HardwarePlanDto` | The user's override, honoured and marked |
| `list_models` | `ModelStatusDto[]` | Versions, precision policy, and what rolled back |
| `warmup_models` | `WarmupReportDto` | Pay the load cost where the user can see it |
| `infer_stats` | `InferStatsDto` | Sessions resident, downshifts, peak memory |

`InferEvent` (`warmupProgress`, `planChanged`, `modelRejected`) mirrors
`PreviewEvent` and `IngestEvent`.

### What the panel deliberately shows

**Unavailable providers, with their reasons.** A panel that lists only what works
answers none of the support questions people actually ask. Every provider appears
with either a probe score or a sentence saying why it does not - "not compiled
into this build", "set aside: mismatch", "no CUDA driver".

**The override, marked as unsupported.** Article XXII gives the user the right to
disagree with the machine, so an override is honoured. It is also recorded, so a
crash report from an overridden machine is not mistaken for a defect in the
negotiation.

### What it deliberately does not carry

- **No file paths.** Model files live under the user's profile; S4 keeps paths
  out of anything that can reach a log or a support bundle, and a DTO is exactly
  such a thing. The panel shows names, versions and digests-in-brief.
- **No private key material, ever**, and no signature bytes: the panel says
  `verified` or names the refusal code, and nothing more.
- **No tensors.** Inference results never cross IPC in this phase; phases 05 to
  29 return decisions with `confidence` and `reasons[]`, not raw outputs.

### Events are typed now and emitted later

`InferEvent` is defined on both sides and is not emitted in this phase, for the
same reason `IngestEvent` was not in phase 01 and `PreviewEvent` was not in phase
02: the Tauri shell has never been launched on the development machine, so an
emitter would be code nobody has run. The UI subscribes already, warmup runs
synchronously behind `warmup_models`, and the exit report lists this as a known
gap rather than a completed item.

## Consequences

- `contracts.lock` is re-locked in the same commit as this ADR.
- `AppState` grows a lazily-built inference engine and a cached hardware plan.
  Both are per-process rather than per-project: hardware is a property of the
  machine, and models are shared by every wedding on it.
- The Settings panel is the first UI surface that can report a *security*
  refusal (`AURA-ML-5002`). It shows the photographer-facing sentence and the
  runbook link, exactly like every other error path.

## Options rejected

- **Reuse `preview_stats` for inference counters.** Two unrelated subsystems
  behind one command is how a stats endpoint becomes a junk drawer.
- **Expose `HardwarePlan` verbatim.** The plan is an internal structure with a
  schema version and a set-aside list; freezing it as an IPC type would mean an
  ADR every time the probe learns something new. The DTO is a projection.
- **Let the UI trigger a model download.** Deferred: the transport in this phase
  is a local directory (ADR-0007), so there is nothing for a button to do until
  phase 04 brings the network.
