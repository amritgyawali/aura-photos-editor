# Phase 28 - Zero-Touch Wedding Autopilot Orchestrator

> **Single feature shipped by this phase:** One button - EDIT COMPLETE WEDDING - runs ingest, analysis, culling, editing, retouching, restoration, consistency, QC and export as a single resumable, cancellable, observable job.
>
> **Mission:** Turn 29 phases of capability into the promise the product is sold on: shoot the wedding, import the RAWs, click once, deliver.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 28 of 30 |
| Epic | E5 - Gallery Brain & Autonomy |
| Feature | One button - EDIT COMPLETE WEDDING - runs ingest, analysis, culling, editing, retouching, restoration, consistency, QC and export as a single resumable, cancellable, observable job. |
| Depends on | Phases 01-27 |
| Unlocks | Phases 29, 30 |
| Duration | 3 weeks |
| Primary owners | Tech Lead - Imaging Core (Rust), Engineering Manager / Delivery Lead Agent, Performance Engineer, Senior Engineer - Core Pipeline (Rust) |
| Risk level | Critical - it is the product's headline |
| Headline KPI | 3,000-image wedding fully processed in <= 2.5 h on reference GPU; crash-free resumption 100 %; zero-touch galleries need human intervention on <= 8 % of frames |
| Competitor being beaten | Aftershoot's end-to-end workflow; Topaz Autopilot |

## 1. Why this phase exists

Every capability so far is worthless to a tired photographer at 1 a.m. unless it runs as one reliable job. Orchestration is what converts features into a product.

Long-running jobs on consumer hardware fail in mundane ways - sleep, thermal throttling, full disks, unplugged drives, driver resets. Handling those gracefully is the actual engineering work, and it is what makes the promise credible.

## 2. Scope contract

### 2.1 In scope

- Pipeline orchestrator: a declarative DAG of stages with dependencies, per-stage checkpointing, resumability, cancellation and back-pressure.
- Zero-Touch configuration surface: the checklist from the product vision (cull, edit, retouch, cleanup, QC, re-edit, export) with per-item autonomy and a single primary action.
- Confidence-gated autonomy using Phase 13 bands, including the raised bands for irreversible operations.
- Resource governance: VRAM/RAM budgets, thermal-aware throttling, disk-space pre-flight, battery-aware pausing on laptops, and a 'quiet mode' that yields to foreground work.
- Failure handling: per-stage retry with backoff, stage isolation (a failed cleanup stage must not fail the wedding), degraded-mode completion, and a clear final status.
- Resumption: kill the app at any point and resume from the last checkpoint with no duplicated work and no corrupted state.
- Progress and observability: per-stage progress with ETA, current image thumbnail, throughput, spend meter, and a live log; notification on completion.
- Pre-flight validation: disk space, GPU availability, model packs present, project integrity, cloud budget - fail fast with actionable messages before starting a two-hour job.
- Post-run summary: what was decided, what needs review, QC report, spend, timings, and the delivery bundle location.

### 2.2 Explicitly out of scope (do not build it here)

- Curation outputs (Phase 29 runs as an optional stage).
- Delivery integrations (Phase 30).
- Multi-machine distribution (post-V1).

## 3. Architecture and data flow

```text
[ EDIT COMPLETE WEDDING ]
     |
  PRE-FLIGHT: disk, GPU, models, budget, project integrity
     |
  STAGE DAG (checkpointed, resumable, cancellable):
   ingest -> previews -> embed -> faces -> scene/story -> moments -> integrity
        -> emotion -> composition -> CULL -> masks -> tone/colour -> style
        -> local light -> retouch -> micro -> restoration -> geometry
        -> cleanup -> camera match -> gallery consistency -> QC (+ re-edit)
        -> [curation] -> EXPORT
     |
  ResourceGovernor (VRAM, thermal, battery, disk, quiet mode) throttles stages
     |
  progress + ETA + spend + live log  ->  completion notification
     |
  POST-RUN SUMMARY: counts, review queue, QC report, timings, spend, output path
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-jobs/src/{lib,dag,stage,checkpoint,resume,cancel,retry,governor,preflight,progress,summary}.rs` | Orchestrator. |
| `crates/aura-jobs/src/stages/*.rs` | One thin adapter per pipeline stage. |
| `config/autopilot.toml` | Stage toggles, autonomy defaults, resource budgets. |
| `apps/desktop/src/routes/autopilot/{Autopilot,StageList,ProgressPanel,PreflightDialog,RunSummary}.tsx` | Autopilot UI. |
| `tests/e2e/autopilot_*.rs` | End-to-end wedding runs including kill/resume chaos tests. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Orchestrator contracts (frozen)**

```rust
pub struct Stage {
    pub id: StageId, pub name: &'static str,
    pub depends_on: Vec<StageId>,
    pub scope: StageScope,            // AllImages | SelectedImages | Gallery
    pub checkpoint: CheckpointKind,   // PerImage | PerBatch | PerStage
    pub optional: bool,               // failure does not fail the run
    pub est_ms_per_item: u32,
    pub resources: ResourceNeeds,     // { vram_mb, ram_mb, gpu, cpu_threads }
}

pub struct RunHandle {
    pub run_id: RunId,
    pub progress: watch::Receiver<RunProgress>,
    pub cancel: CancellationToken,
}

pub struct RunProgress {
    pub stage: StageId, pub stage_index: u32, pub stage_total: u32,
    pub items_done: u32, pub items_total: u32,
    pub eta_s: u32, pub throughput_per_s: f32,
    pub spend_usd: f32, pub warnings: Vec<String>,
    pub current_image: Option<ImageId>,
}

pub struct RunSummary {
    pub run_id: RunId, pub status: RunStatus,   // Completed | CompletedDegraded | Cancelled | Failed
    pub selected: u32, pub exported: u32, pub needs_review: u32,
    pub qc: QcReport, pub stage_timings: Vec<(StageId, u64)>,
    pub spend_usd: f32, pub output_path: PathBuf,
    pub degraded_stages: Vec<(StageId, String)>,
}
```

## 6. Algorithm, model and implementation design

### 6.1 Stage graph and checkpointing

- Stages declare dependencies, scope and resource needs; the scheduler runs independent stages concurrently only when resources allow, which is how the wall-clock budget is met.
- Checkpoint granularity is per stage's natural unit: per image for analysis, per batch for GPU-heavy stages, per stage for gallery-level solvers. Checkpoints are written transactionally with the catalog.
- Resume replays only unfinished units; a hash of stage inputs detects when upstream changes invalidate a checkpoint, forcing a clean re-run of the affected stage only.

### 6.2 Resource governance on real laptops

- VRAM budget from Phase 03's hardware plan; batch sizes shrink under pressure rather than crashing, and OOM triggers a documented fallback (smaller batch, then CPU).
- Thermal awareness: sustained high temperature reduces concurrency; the UI explains 'reducing speed to protect your machine' rather than silently slowing.
- Battery: on battery power, heavy stages pause by default with a clear prompt; disk pre-flight requires 1.6x estimated output size free.
- Quiet mode yields GPU/CPU when the user is working in another application, so an overnight run is not required.

### 6.3 Failure isolation and degraded completion

- Optional stages (cleanup, restoration, curation, cloud reasoning) can fail without failing the wedding; the run completes as `CompletedDegraded` with an explicit list of what was skipped.
- Mandatory stages retry with backoff, then fail the run with a precise diagnosis and a resumable checkpoint - never a half-written gallery.
- A driver reset or GPU loss is detected and the run continues on CPU where feasible, with an honest ETA update.

### 6.4 Honest ETA and observability

- ETA is computed from measured throughput of completed units per stage plus per-stage estimates for remaining stages, updated continuously; it must be within 20 % after 10 % of the run.
- Every stage emits structured progress and telemetry; a live log lets an advanced user see exactly what is happening, which is how support diagnoses field problems.

## 7. Cloud AI usage (bring-your-own API key)

No cloud AI call in this phase. The phase must work with the network cable unplugged; the Cloud AI Gateway from Phase 04 stays idle here.

## 8. Implementation order (execute literally, in this order)

1. Design the stage DAG with every phase owner; confirm scopes, dependencies and resource needs.
2. Implement the orchestrator core: DAG execution, checkpointing, cancellation, retry.
3. Implement stage adapters for all pipeline phases behind a uniform interface.
4. Implement the resource governor with VRAM, thermal, battery and disk policies.
5. Implement pre-flight validation with actionable error messages.
6. Implement resume with input-hash invalidation and chaos tests.
7. Implement progress, ETA, spend meter and the live log.
8. Implement degraded completion and the run summary.
9. Build the Autopilot UI with the Zero-Touch checklist and a single primary action.
10. Run the full-scale performance and reliability campaign on three reference machines.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `TLC` | Tech Lead - Imaging Core (Rust) | Own the DAG design, checkpoint semantics, resume correctness and stage interface | Architecture + review | 6 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Implement orchestrator core, adapters, retry, cancellation, summary | `aura-jobs` + tests | 11 d |
| `PERF` | Performance Engineer | Resource governor, thermal/battery policy, ETA model, full-scale benchmark campaign | Governor + report | 8 d |
| `EM` | Engineering Manager / Delivery Lead Agent | Coordinate all phase owners into the DAG; run the integration bug bash; own the release checklist | Integration plan | 6 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Autopilot screen, stage list, progress panel, live log, run summary | Autopilot UI | 7 d |
| `MFE` | Mid-Level Frontend Engineer | Pre-flight dialog, degraded-mode banners, notifications, review-queue hand-off | UI panels | 4 d |
| `QAL` | QA Lead - Automation | Chaos tests (kill, sleep, unplug, disk full, GPU reset), resume correctness, ETA accuracy | CI + manual suite | 7 d |
| `DEVOPS` | DevOps / Release Engineer | Long-run CI job on real GPU hardware nightly; artefact and log collection | Nightly pipeline | 4 d |
| `PM` | Product Manager Agent | Own `autopilot.toml` defaults, the Zero-Touch checklist wording and the intervention-rate target | Approved defaults | 3 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Run 10 real weddings end to end; measure intervention rate and log every complaint | Field report | 7 d |
| `SEC` | Security & Privacy Engineer | Verify no stage writes outside project directories; validate cancellation leaves no partial exports | Sign-off | 2 d |
| `DOC` | Technical Writer | Write the Autopilot guide, hardware recommendations and troubleshooting matrix | Docs merged | 4 d |

### 9.1 Handoff chain for this phase

```text
TLC DAG design (with all phase owners) -> SRC orchestrator -> PERF governor
                                     |
                                     v
                         SFE/MFE Autopilot UI <- PM defaults
                                     |
     QAL chaos suite + DEVOPS nightly runs + QAIQ 10 real weddings -> CTO release gate
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

- A 3,000-image wedding completes end to end within the wall-clock budget on each reference machine.
- Chaos: killing the app at 20 random points always resumes correctly with no duplicated or lost work.
- Sleep/wake, drive unplug, disk full and GPU reset each produce a clear, recoverable state.
- Optional stage failure yields `CompletedDegraded` with an accurate skipped list, never a failed wedding.
- ETA within 20 % after 10 % of the run on all reference machines.
- Cancellation leaves no partial exports and no corrupted catalog.
- Intervention rate <= 8 % of frames across the 10 field weddings.

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
| 3,000 images end to end (RTX 4070 laptop) | <= 2.5 h |
| 3,000 images end to end (M3 Pro) | <= 4 h |
| Analysis + cull portion | < 8 min (inherited gate) |
| Peak VRAM | <= 80 % of available |
| Resume overhead | <= 20 s |

Telemetry events (local-first, opt-in aggregation):

- `autopilot.started` {images, stages_enabled, hardware_plan}
- `autopilot.stage` {stage, items, ms, throughput, degraded}
- `autopilot.finished` {status, total_ms, needs_review, spend_usd}
- `autopilot.resource_event` {kind: thermal|vram|battery|disk, action}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| A two-hour run fails at 90 % | Per-stage checkpointing, resumability, stage isolation, degraded completion, and nightly long-run CI. |
| Machine becomes unusable during a run | Quiet mode, thermal and battery policies, concurrency caps, and honest UI messaging. |
| Wall-clock budget missed on weaker hardware | Published hardware tiers with expected times, adaptive quality settings, and pre-flight warnings. |
| Silent quality regressions hidden by automation | QC gate inside the pipeline plus the post-run summary and review queue. |
| Cross-phase integration debt surfaces late | EM-owned integration plan, bug bash, and the DAG being designed with every phase owner. |

## 13. Acceptance criteria

- [ ] One button processes a complete wedding from RAW import to exported gallery.
- [ ] Progress, ETA and spend are visible and accurate; completion notifies the user.
- [ ] Killing or interrupting the app never loses work.
- [ ] Optional stage failures degrade gracefully with an honest summary.
- [ ] Ten real weddings complete with intervention on 8 % of frames or fewer.
- [ ] Pre-flight catches disk, GPU, model and budget problems before the run starts.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 28 - Zero-Touch Wedding Autopilot Orchestrator.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-28-ZERO-TOUCH-AUTOPILOT.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - One button - EDIT COMPLETE WEDDING - runs ingest, analysis, culling, editing, retouching, restoration, consistency, QC and export as a single resumable, cancellable, observable job.

Rules:
  - Do not start Phase 29. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-jobs/src/{lib,dag,stage,checkpoint,resume,cancel,retry,governor,preflight,progress,summary}.rs`, `crates/aura-jobs/src/stages/*.rs`, `config/autopilot.toml`, `apps/desktop/src/routes/autopilot/{Autopilot,StageList,ProgressPanel,PreflightDialog,RunSummary}.tsx`, `tests/e2e/autopilot_*.rs`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-28-zero-touch-autopilot and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-28.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-28-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-28-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-28-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 28 of 30 - Zero-Touch Wedding Autopilot Orchestrator - part of the AURA Wedding AI master build plan.*
