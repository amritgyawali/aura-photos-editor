# Phase 03 - Inference Runtime Layer & Signed Model Package Manager

> **Single feature shipped by this phase:** One local AI runtime that picks the best hardware path (TensorRT / CUDA / DirectML / CoreML / CPU) automatically, plus a signed model registry with delta updates.
>
> **Mission:** Every one of the ~25 models in this product must load, run and be benchmarked through a single abstraction, so model work never turns into per-GPU firefighting.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 03 of 30 |
| Epic | E1 - Foundation |
| Feature | One local AI runtime that picks the best hardware path (TensorRT / CUDA / DirectML / CoreML / CPU) automatically, plus a signed model registry with delta updates. |
| Depends on | Phases 01-02 |
| Unlocks | Every AI phase (05-29) |
| Duration | 2 weeks |
| Primary owners | ML Lead - Vision, MLOps / Model Packaging Engineer, Senior Engineer - GPU & Render (Rust / wgpu / CUDA), Security & Privacy Engineer |
| Risk level | High - hardware diversity is where desktop AI products die |
| Headline KPI | same model, same output within 1e-3 across EPs; cold model load <= 900 ms; batch throughput within 15 % of hand-tuned baseline |
| Competitor being beaten | Aperty/Topaz offline performance across mixed hardware |

## 1. Why this phase exists

Photographers own wildly different machines: RTX laptops, M-series Macs, and old Intel desktops. A product that only flies on NVIDIA loses half the market; a product that runs everything on CPU is unusable.

With ~25 specialist models, ad-hoc loading code would multiply into an unmaintainable mess. One runtime layer, one registry, one benchmark harness, one warmup policy.

Model integrity is a security boundary: models are executable-ish artefacts downloaded over the network, so they must be signed, hash-pinned and versioned, and the app must refuse anything unsigned.

## 2. Scope contract

### 2.1 In scope

- `aura-infer`: ONNX Runtime wrapper with execution-provider negotiation, session pooling, IO binding, pinned-memory staging, batch scheduler.
- EP support matrix: TensorRT, CUDA, DirectML, CoreML/MPS, and an always-available CPU path with quantised models.
- Automatic hardware probe on first run: enumerate GPUs, VRAM, driver, compute capability; run a 15-second micro-benchmark; persist a `hardware_plan.json`.
- Model registry: `models.lock` with name, version, task, sha256, EP-specific variants (fp32/fp16/int8), input spec, licence, and a signed manifest.
- Downloader with resume, delta updates, integrity verification, signature verification (ed25519), atomic swap, rollback.
- Batch inference scheduler with dynamic batch sizing driven by free VRAM and observed latency, plus graceful degradation to smaller batches or CPU.
- Warmup and cache: TensorRT engine cache keyed by GPU + driver + model hash; CoreML compiled-model cache.
- Benchmark harness (`just bench-models`) producing a per-machine table used by PERF and by the scheduler's cost model.
- Deterministic mode for CI: fixed EP, fixed batch, fixed seeds, tolerance-based output comparison.

### 2.2 Explicitly out of scope (do not build it here)

- Training (that lives in `ml/`, first used in Phase 05-07).
- Cloud inference (Phase 04).
- Any specific model's semantics.

## 3. Architecture and data flow

```text
first run --> HardwareProbe --> hardware_plan.json (EP order, batch sizes, VRAM budget)
                                     |
  models.lock (signed) --> ModelRegistry --> Downloader (resume, ed25519 verify, atomic swap)
                                     |
                                     v
                              +--------------+     +--------------------+
  callers (Phases 05-29) ---> | InferService | --> | ORT Session Pool   | -> TensorRT | CUDA
     run(model, tensors)      +--------------+     |  + IO binding      |    DirectML | CoreML | CPU
                                     |             +--------------------+
                                     v
                        BatchScheduler (VRAM-aware, latency-aware, cancellable)
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-infer/src/{lib,service,session,ep,probe,plan,batch,tensor,warmup,errors}.rs` | Runtime abstraction, EP negotiation, batching, tensor helpers. |
| `crates/aura-models/src/{lib,registry,manifest,download,verify,swap,rollback}.rs` | Model registry, signed manifest handling, updates. |
| `models/models.lock` + `models/manifest.sig` | Pinned model set and detached signature. |
| `crates/aura-infer/benches/model_bench.rs` | Per-EP latency/throughput benchmarks. |
| `ml/export_onnx/{export.py,verify_parity.py,quantise.py}` | PyTorch -> ONNX export, parity check, int8 calibration. |
| `tools/model-sign/` | Offline signing tool (release key stays out of CI). |
| `docs/adr/ADR-0003-inference-runtime.md` | Why ORT + EP order + quantisation policy. |
| `docs/model-cards/TEMPLATE.md` | Mandatory model card format for every shipped model. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Inference service (frozen)**

```rust
pub struct ModelRef { pub name: &'static str, pub version: Version }

pub struct InferRequest<'a> {
    pub model: ModelRef,
    pub inputs: Vec<TensorView<'a>>,
    pub prio: Priority,
    pub deadline: Option<Duration>,
}

pub struct InferResult {
    pub outputs: Vec<Tensor>,
    pub ep_used: ExecutionProvider,
    pub latency_ms: f32,
    pub batch_size: u16,
}

pub trait InferService: Send + Sync {
    fn run(&self, req: InferRequest<'_>) -> Result<InferResult, InferError>;
    fn run_batch(&self, model: ModelRef, inputs: Vec<Vec<TensorView<'_>>>, prio: Priority)
        -> Result<Vec<InferResult>, InferError>;
    fn plan(&self) -> &HardwarePlan;
    fn warmup(&self, models: &[ModelRef]) -> Result<(), InferError>;
}
```

**models.lock entry**

```json
{
  "name": "scene_classifier",
  "version": "1.4.2",
  "task": "multiclass+multilabel",
  "input": { "shape": [1, 3, 384, 384], "layout": "NCHW", "range": "0..1", "colour": "srgb" },
  "output": { "scene_logits": [1, 22], "attr_logits": [1, 14] },
  "variants": [
    { "ep": "tensorrt", "precision": "fp16", "file": "scene_1.4.2.fp16.onnx", "sha256": "a91c...", "bytes": 41_233_112 },
    { "ep": "coreml",   "precision": "fp16", "file": "scene_1.4.2.fp16.onnx", "sha256": "a91c..." },
    { "ep": "cpu",      "precision": "int8", "file": "scene_1.4.2.int8.onnx", "sha256": "77b0..." }
  ],
  "licence": "proprietary",
  "model_card": "docs/model-cards/scene_classifier.md",
  "min_app_version": "0.3.0"
}
```

**hardware_plan.json (written on first run)**

```json
{
  "gpu": { "vendor": "nvidia", "name": "RTX 4070 Laptop", "vram_mb": 8188, "driver": "555.85" },
  "ep_order": ["tensorrt", "cuda", "cpu"],
  "vram_budget_mb": 5600,
  "default_batch": { "embedding": 32, "segmentation": 8, "retouch": 4 },
  "cpu_threads": 10,
  "probe_scores_ms": { "tensorrt": 3.1, "cuda": 4.4, "cpu": 61.0 },
  "probed_at": "2026-08-09T11:20:03Z"
}
```

## 6. Algorithm, model and implementation design

### 6.1 Execution-provider negotiation

- Probe order: TensorRT (if engine cache buildable within 60 s) -> CUDA -> DirectML -> CoreML -> CPU.
- Each candidate runs a tiny reference model 20 times; median latency and a correctness check against the CPU result (tolerance 1e-3) decide viability.
- A crashed or unstable EP is blacklisted per machine in `hardware_plan.json` and the app silently uses the next one, reporting it in Settings > Hardware.
- Users can override the plan; overrides are respected but marked as unsupported in telemetry.

### 6.2 Batch scheduling and VRAM safety

- Maintain a VRAM ledger: every session declares peak working set measured at warmup; the scheduler never oversubscribes the budget (default 70 % of free VRAM).
- Dynamic batch: start at the planned batch size, halve on OOM, and remember the successful size per model per machine.
- Priority preemption: interactive requests (user clicked a photo) jump ahead of batch analysis; batch work is chunked so preemption latency stays < 80 ms.
- Cancellation is cooperative and immediate at chunk boundaries; a cancelled batch releases VRAM within one chunk.

### 6.3 Model integrity and updates

- `models.lock` is signed with an offline ed25519 release key; the app verifies the manifest signature and then each file's sha256 before use.
- Download with HTTP range resume, then atomic rename into place, then a verification pass; rollback keeps the previous version until the new one has run successfully once.
- Delta updates via bsdiff on the ONNX payload for small architecture-stable updates.
- Every model needs a model card (data, metrics, known failure modes, bias notes) or CI blocks the release.

### 6.4 Cross-EP parity and CI determinism

- `verify_parity.py` runs every model on CPU fp32 and on each available EP over a fixed 200-sample tensor set; max abs diff must be <= 1e-3 (fp16) or <= 1e-2 (int8) with a task-level metric guard.
- CI runs the CPU EP with fixed batch sizes so results are reproducible; GPU EPs are validated nightly on self-hosted runners.
- Quantisation is per-model opt-in with an accuracy budget: int8 may lose at most 1 % of the task metric.

### 6.5 Warmup policy

- On project open, warm only the models needed by the next stage (embedding, face, scene) to keep first-analysis latency low.
- TensorRT engines are compiled in the background on first run and cached by (GPU, driver, model hash); until ready, CUDA EP serves requests.
- Warmup progress is visible so the user understands the one-time cost.

## 7. Cloud AI usage (bring-your-own API key)

No cloud AI call in this phase. The phase must work with the network cable unplugged; the Cloud AI Gateway from Phase 04 stays idle here.

## 8. Implementation order (execute literally, in this order)

1. Write ADR-0003 (runtime, EP order, quantisation and signing policy).
2. Implement the ORT wrapper with session pooling and IO binding; run a dummy model end to end.
3. Implement the hardware probe and `hardware_plan.json` with blacklisting.
4. Implement the batch scheduler with the VRAM ledger, dynamic batch and preemption.
5. Implement the model registry, signature verification, download/resume, atomic swap and rollback.
6. Build `ml/export_onnx` and `verify_parity.py`; ship two placeholder models to exercise the whole path.
7. Build the benchmark harness and publish the first per-machine table.
8. Add the model-card template and the CI gate that blocks unsigned or card-less models.
9. Wire warmup into project open with visible progress.
10. Write runbooks: 'GPU not detected', 'model update failed', 'how to add a model'.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `MLL` | ML Lead - Vision | Own the model portfolio spec, precision policy, parity tolerances and model-card gate | Signed ADR-0003 + policy | 3 d |
| `MLOPS` | MLOps / Model Packaging Engineer | Implement registry, signing, download/resume, delta updates, rollback and CI hooks | `aura-models` + tooling | 6 d |
| `SRG` | Senior Engineer - GPU & Render (Rust / wgpu / CUDA) | ORT wrapper, IO binding, VRAM ledger, batch scheduler, TensorRT engine cache | `aura-infer` core | 6 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Hardware probe, plan persistence, EP blacklisting, CPU fallback path | Probe + plan | 3 d |
| `SRML` | Senior ML Engineer | ONNX export pipeline, quantisation, parity verifier, two reference models | `ml/export_onnx` + parity report | 4 d |
| `SEC` | Security & Privacy Engineer | Key management, ed25519 verification, download hardening, sandbox limits, supply-chain notes | Security review sign-off | 3 d |
| `PERF` | Performance Engineer | Benchmark harness, per-machine tables, cost model inputs for the scheduler | `just bench-models` + report | 4 d |
| `QAL` | QA Lead - Automation | Parity tests in CI, OOM simulation, corrupt-model tests, rollback tests, cancel tests | CI gates | 4 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Settings > Hardware panel (detected GPU, EP in use, override, warmup progress, model versions) | Hardware UI | 2 d |
| `MBE` | Mid-Level Backend / Cloud Engineer | Model CDN layout, versioned paths, signed manifest hosting, staged rollout flags | Distribution endpoint | 3 d |
| `DEVOPS` | DevOps / Release Engineer | Self-hosted GPU runners (NVIDIA + Apple Silicon), nightly EP matrix job | Nightly matrix green | 3 d |
| `CTO` | Chief Architect / CTO Agent | Review that no phase bypasses `InferService`; add a lint that forbids direct ORT usage | Architecture lint | 1 d |
| `DOC` | Technical Writer | Model-card template, 'adding a model' guide, hardware troubleshooting runbook | Docs merged | 2 d |

### 9.1 Handoff chain for this phase

```text
MLL policy --> SRG (runtime) + MLOPS (registry) --> SRML (export/parity)
                    |                    |
                    v                    v
            PERF (bench tables)   SEC (signing review)
                    \_______ QAL (parity/OOM/rollback CI) _______/ --> CTO gate
```

### How this agent team runs a phase (identical every time)

1. **Kickoff (PM + CTO + EM).** PM restates the feature as user stories, CTO writes/updates the ADR, EM cuts the task list from section 9 into the tracker.
2. **Design review (CTO + TLC + MLL + COL + UX).** Interfaces from section 5 are frozen before code. Any change after freeze needs an ADR amendment.
3. **Build in parallel lanes.** Core lane (TLC/SRC/SRG), ML lane (MLL/SRML/MLR/MLOPS), agent lane (AGT), UI lane (SFE/MFE/UX), data lane (DATA), platform lane (DEVOPS/SEC).
4. **Contract-first handoff.** A lane may only consume another lane's work through the frozen interface, using a stub/fixture until the real implementation lands.
5. **Code review chain.** Author -> peer in same lane -> lane lead -> CTO for anything touching an invariant. Two approvals minimum, one must be a lead.
6. **QA gate (QAL + QAIQ + PERF).** Unit + integration + golden-image + perceptual + performance suites must be green on the reference weddings.
7. **Phase gate (CTO + PM + EM).** All acceptance criteria in section 13 pass, telemetry is live, docs updated, demo recorded. Only then does the next phase start.
8. **Escalation.** Any blocker older than one working day goes to EM; any invariant conflict goes to CTO; any "we should ship it slightly broken" goes to PM and is written down.

### Branch, commit and PR rules

- Branch: `feat/phase-NN-<slug>`; one PR per task group, never one giant PR per phase.
- Conventional Commits (`feat(core): ...`, `fix(ml): ...`, `perf(render): ...`, `test(qa): ...`, `docs: ...`).
- Every PR states: what changed, which acceptance criterion it advances, benchmark delta, and screenshots or golden-image diffs when pixels change.
- CI must be green: `fmt`, `clippy -D warnings`, `cargo test`, `pytest`, `vitest`, golden-image diff, benchmark regression guard (<= 5 % slower), model-hash check.


## 10. Test plan

### 10.1 Phase-specific tests

- Parity: every model matches CPU fp32 within tolerance on all available EPs.
- OOM: artificially shrink the VRAM budget; the scheduler halves batches and completes without crashing.
- Corrupt model file, wrong signature, truncated download, mid-download kill: all rejected or resumed, never used.
- Rollback: a model that throws on first real use is automatically reverted to the previous version.
- Preemption: interactive request served within 80 ms during a saturated batch run.
- Cold start: warmup of the three startup models completes within 2.5 s on reference machines.
- No-GPU machine: everything still runs on CPU int8 with correct results, only slower.

### 10.2 Standing test matrix (applies to every phase)

| Layer | What it proves |
|---|---|
| Unit | Pure functions, thresholds, scoring maths, serialisation round-trips, error taxonomy. |
| Property/fuzz | Corrupt RAWs, truncated previews, absurd EXIF, 0-face and 60-face frames, 1-image and 6,000-image projects. |
| Golden image | Frozen fixture set rendered and compared pixel-wise; dE2000 mean <= 0.5, max <= 2.0 unless intentionally changed and re-blessed. |
| Perceptual (human) | QAIQ blind A/B against the previous build and against the named competitor for this feature; >= 60 % preference required. |
| Performance | Throughput, wall clock, peak RAM, peak VRAM on the three reference machines. |
| Resume/kill | Kill the process at 10 %, 50 %, 90 %; restart must continue without recomputation or corruption. |
| Regression | Full previous-phase suite must stay green; no acceptance criterion from an earlier phase may regress. |

Reference machines: RTX 4070 laptop (Win 11, 32 GB), M3 Pro MacBook (18 GB), Intel iGPU desktop (Win 11, 16 GB, DirectML fallback).

## 11. Performance budget and telemetry

| Metric | Budget |
|---|---|
| Cold model load (cached engine) | <= 900 ms |
| Embedding model throughput (RTX 4070) | >= 220 img/s at 384 px |
| Embedding model throughput (M3 Pro) | >= 110 img/s at 384 px |
| Embedding model throughput (CPU int8, 8 cores) | >= 18 img/s |
| Scheduler overhead per request | <= 0.4 ms |
| VRAM overshoot events | 0 in a 2-hour soak test |

Telemetry events (local-first, opt-in aggregation):

- `infer.plan_selected` {ep_order, gpu, vram_mb, probe_ms}
- `infer.run` {model, ep, batch, latency_ms, queue_ms}
- `infer.oom_downshift` {model, from_batch, to_batch}
- `model.update` {name, from, to, bytes, delta_used, ok}
- `model.rejected` {name, reason}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Driver bugs cause EP crashes on specific GPUs | Per-machine EP blacklist, crash-loop detection, automatic downgrade to the next EP, telemetry-driven denylist shipped in `models.lock`. |
| TensorRT engine build takes minutes and blocks first use | Background compile with CUDA EP serving meanwhile; engine cache shared across app updates when hashes match. |
| int8 quantisation degrades skin-critical models | Per-model precision policy; retouch and colour models default to fp16 and may forbid int8. |
| Model downloads are large and users have slow links | Tiered install (core models bundled, advanced downloaded), delta updates, resumable transfer, offline installer bundle. |
| Phases bypass the abstraction for speed | CI lint forbidding direct `ort::` usage outside `aura-infer`, enforced by CTO review. |

## 13. Acceptance criteria

- [ ] The same model produces equivalent results (within tolerance) on TensorRT, CUDA, DirectML, CoreML and CPU.
- [ ] First run probes hardware in <= 15 s and writes a plan; Settings shows the selected EP and lets the user override.
- [ ] An unsigned, mismatched or corrupt model is refused with a clear message and the previous version keeps working.
- [ ] Under a forced VRAM squeeze the app degrades batch size instead of crashing, and finishes the job.
- [ ] `just bench-models` produces a table for all three reference machines and is stored as a CI artefact.
- [ ] Every shipped model has a model card; CI fails without one.
- [ ] No code outside `aura-infer` links ONNX Runtime directly.

## 14. Definition of Done (phase gate)

- [ ] All acceptance criteria in section 13 verified by QA on the three reference weddings (indoor Hindu night ceremony, outdoor daylight Christian wedding, mixed-light Nepali reception).
- [ ] Unit, integration, golden-image, perceptual and performance suites green in CI on Windows (NVIDIA), Windows (integrated/DirectML) and macOS (Apple Silicon).
- [ ] Performance budget in section 11 met or a signed waiver from PERF + CTO recorded in the ADR.
- [ ] Telemetry events from section 11 visible in the local metrics dashboard and in the opt-in aggregate pipeline.
- [ ] Every new AI decision surface returns `confidence` + `reasons[]` and is rendered in the Explain panel.
- [ ] Docs updated: module README, model card (if a model shipped), in-app help string, CHANGELOG entry.
- [ ] Rollback path exists: feature flag off, previous model version pinnable, catalog migration reversible.
- [ ] Demo recording of the feature running on a real 3,000-image wedding attached to the phase gate.

Inherited invariants that this phase must not break:

- **Never mutate a RAW file.** Every decision is a row in SQLite plus a JSON edit recipe. Originals are opened read-only.
- **Every AI decision carries `confidence` (0-1) and `reasons[]`.** A decision without an explanation is a bug.
- **Three-tier compute.** Cheap analysis on embedded previews, medium analysis on 2048 px proxies, expensive work only on survivors.
- **Determinism.** Same inputs + same model versions + same seed = byte-identical recipe JSON. All models are pinned by hash.
- **Resumability.** Any job can be killed at any moment and resumed without recomputing finished work.
- **Local-first.** The product must complete a full wedding with no network. Cloud AI is an accelerator, never a dependency.
- **Scene-conditioned everything.** No threshold is global; every threshold is a function of the detected scene and subject role.
- **Colour discipline.** Work in linear scene-referred space, convert once, and never let a grade move skin outside its guarded region.
- **No silent failure.** Every module emits a typed error, a fallback path and a telemetry event.

## 15. Claude Code execution prompt (copy-paste this)

```text
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 03 - Inference Runtime Layer & Signed Model Package Manager.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-03-INFERENCE-RUNTIME-MODEL-REGISTRY.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - One local AI runtime that picks the best hardware path (TensorRT / CUDA / DirectML / CoreML / CPU) automatically, plus a signed model registry with delta updates.

Rules:
  - Do not start Phase 4. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-infer/src/{lib,service,session,ep,probe,plan,batch,tensor,warmup,errors}.rs`, `crates/aura-models/src/{lib,registry,manifest,download,verify,swap,rollback}.rs`, `models/models.lock` + `models/manifest.sig`, `crates/aura-infer/benches/model_bench.rs`, `ml/export_onnx/{export.py,verify_parity.py,quantise.py}`, `tools/model-sign/`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-03-inference-runtime-model-registry and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-03.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-03-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-03-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-03-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 03 of 30 - Inference Runtime Layer & Signed Model Package Manager - part of the AURA Wedding AI master build plan.*
