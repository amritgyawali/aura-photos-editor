# AURA Wedding AI

> Shoot the wedding. Import the RAWs. Click once. Deliver.

The autonomous AI post-production system for wedding photographers.
Import 1,000-4,000 RAW files, press one button, and receive a culled, edited, retouched,
quality-checked, consistently graded, export-ready wedding gallery.

## What this bundle is

A complete, executable plan: strategy, architecture, data model, model stack, QA strategy,
coding standards, dataset plan, business plan, risk register, and **30 phase documents of roughly
three pages each**, every one written so a coding agent can implement it without guessing.

## Read in this order

| File | Purpose |
| --- | --- |
| `CLAUDE.md` | **Start here if you are a coding agent.** Operating rules, invariants, phase ritual. |
| `00-PRODUCT-STRATEGY.md` | What we are building, for whom, why it wins, what we refuse to build. |
| `01-ARCHITECTURE.md` | Process model, crate layout, pipeline, threading, IPC, hardware strategy. |
| `02-AI-MODEL-STACK.md` | Every model, its job, size, runtime, gate, and the cloud reasoning policy. |
| `03-DATA-MODEL.md` | SQLite schema, cache layout, recipe JSON, ledger, migrations. |
| `04-ROADMAP-30-PHASES.md` | The 30 phases, epics and version milestones. |
| `AGENT-TEAM.md` | The 23-role virtual IT company and how to role-play it. |
| `05-QA-STRATEGY.md` | Test pyramid, golden images, perceptual gates, chaos tests, release gates. |
| `06-CODING-STANDARDS.md` | Rust/TypeScript/Python standards, error handling, review checklist. |
| `07-DATASET-AND-TRAINING.md` | The Wedding Intelligence Dataset - the real moat. |
| `08-BUSINESS-GTM.md` | Pricing, positioning, launch, unit economics. |
| `09-RISK-REGISTER.md` | What can kill this project and what we do about it. |
| `10-GLOSSARY.md` | Shared vocabulary. Use these exact terms in code. |
| `phases/PHASE-01..30-*.md` | The build itself. One feature per phase, fully specified. |
| `WEDDING-AI-MASTER-PLAN.md` | Everything concatenated into one file. |

## Technology stack (decided)

| Layer | Choice | Why |
| --- | --- | --- |
| Desktop shell | Tauri 2 + React 18 + TypeScript + Vite | Native performance, small binary, Rust core in the same process tree. |
| Core engine | Rust (2021, stable) | Memory safety with C-like speed; excellent concurrency for 4,000-file pipelines. |
| RAW decode | LibRaw (via Rust FFI) | Broadest camera support (CR2/CR3, NEF, ARW, RAF, DNG, ORF). |
| GPU render | wgpu (Vulkan/Metal/DX12) + WGSL | One shader codebase for Windows and macOS. |
| Inference | ONNX Runtime (TensorRT, CUDA, DirectML, CoreML, CPU) | One model artefact, every hardware target. |
| Classical CV | OpenCV (narrow, isolated usage) | Proven primitives where a model is unnecessary. |
| Catalog | SQLite (WAL) + JSON sidecars + XMP | Zero-admin, transactional, portable, interoperable. |
| Training | Python 3.11 + PyTorch 2 | Standard research stack; exports to ONNX. |
| Cloud reasoning | Bring-your-own API key, governed gateway | Your key, your budget, your data, with hard caps and offline fallback. |

## Non-negotiable promises

1. RAW files are never modified. Every edit is a recipe.
2. Client imagery never leaves the machine without explicit per-project consent.
3. Every automated decision is explainable, confidence-scored and reversible.
4. The product works fully offline. Cloud AI is an accelerator, never a dependency.
5. Nothing the product does may change what a person looks like.

---

# CLAUDE.md - Operating manual for the coding agent

You are building AURA Wedding AI, repository `aura`.
Read this file completely before writing any code. Re-read the top section at the start of every session.

## 1. Your operating mode

- You are a full engineering organisation of 23 roles (see `AGENT-TEAM.md`). Announce which role you are wearing for each task.
- Work **one phase at a time, in order**. Never start phase N+1 while phase N's Definition of Done is unmet.
- Within a phase, work the task table in order. Each task names a role, a deliverable and an estimate.
- After every task: run the tests, run the linters, update the docs, then continue.
- If a phase document and this file disagree, this file wins. If this file and an ADR disagree, the newer ADR wins.

## 2. The nine invariants (never violate)

1. **Never mutate a RAW file.** Every decision is a row in SQLite plus a JSON edit recipe. Originals are opened read-only.
2. **Every AI decision carries `confidence` (0-1) and `reasons[]`.** A decision without an explanation is a bug.
3. **Three-tier compute.** Cheap analysis on embedded previews, medium analysis on 2048 px proxies, expensive work only on survivors.
4. **Determinism.** Same inputs + same model versions + same seed = byte-identical recipe JSON. All models are pinned by hash.
5. **Resumability.** Any job can be killed at any moment and resumed without recomputing finished work.
6. **Local-first.** The product must complete a full wedding with no network. Cloud AI is an accelerator, never a dependency.
7. **Scene-conditioned everything.** No threshold is global; every threshold is a function of the detected scene and subject role.
8. **Colour discipline.** Work in linear scene-referred space, convert once, and never let a grade move skin outside its guarded region.
9. **No silent failure.** Every module emits a typed error, a fallback path and a telemetry event.

If a requested change would break an invariant, stop and write an ADR proposing the change instead of implementing it.

## 3. The phase ritual

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

## 4. Definition of Done (every phase)

- [ ] All acceptance criteria in section 13 verified by QA on the three reference weddings (indoor Hindu night ceremony, outdoor daylight Christian wedding, mixed-light Nepali reception).
- [ ] Unit, integration, golden-image, perceptual and performance suites green in CI on Windows (NVIDIA), Windows (integrated/DirectML) and macOS (Apple Silicon).
- [ ] Performance budget in section 11 met or a signed waiver from PERF + CTO recorded in the ADR.
- [ ] Telemetry events from section 11 visible in the local metrics dashboard and in the opt-in aggregate pipeline.
- [ ] Every new AI decision surface returns `confidence` + `reasons[]` and is rendered in the Explain panel.
- [ ] Docs updated: module README, model card (if a model shipped), in-app help string, CHANGELOG entry.
- [ ] Rollback path exists: feature flag off, previous model version pinnable, catalog migration reversible.
- [ ] Demo recording of the feature running on a real 3,000-image wedding attached to the phase gate.

## 5. Standard test layers (every phase)

- **Unit** - Pure functions, thresholds, scoring maths, serialisation round-trips, error taxonomy.
- **Property/fuzz** - Corrupt RAWs, truncated previews, absurd EXIF, 0-face and 60-face frames, 1-image and 6,000-image projects.
- **Golden image** - Frozen fixture set rendered and compared pixel-wise; dE2000 mean <= 0.5, max <= 2.0 unless intentionally changed and re-blessed.
- **Perceptual (human)** - QAIQ blind A/B against the previous build and against the named competitor for this feature; >= 60 % preference required.
- **Performance** - Throughput, wall clock, peak RAM, peak VRAM on the three reference machines.
- **Resume/kill** - Kill the process at 10 %, 50 %, 90 %; restart must continue without recomputation or corruption.
- **Regression** - Full previous-phase suite must stay green; no acceptance criterion from an earlier phase may regress.

## 6. Reference machines

RTX 4070 laptop (Win 11, 32 GB), M3 Pro MacBook (18 GB), Intel iGPU desktop (Win 11, 16 GB, DirectML fallback).

Every performance budget in every phase refers to these machines. Budgets are enforced as tests, not hopes.

## 7. Repository layout (create exactly this)

```
aura/
  Cargo.toml                  # workspace
  crates/
    aura-core/                # types, errors, ids, config, logging
    aura-catalog/             # SQLite catalog + migrations
    aura-ingest/              # file discovery, hashing, EXIF, journal
    aura-raw/                 # LibRaw FFI, decode, demosaic
    aura-cache/               # preview/proxy cache, pipeline_ver
    aura-preview/             # tiered preview generation
    aura-render/              # wgpu render graph + WGSL shaders
    aura-recipe/              # edit recipe schema + versioning
    aura-infer/               # ONNX Runtime abstraction, EP selection
    aura-models/              # model registry, manifests, signatures
    aura-vision/              # faces, masks, embeddings, integrity
    aura-index/               # HNSW similarity index
    aura-cloud/               # governed cloud AI gateway
    aura-people/              # identities, subject hierarchy
    aura-cull/                # selection, coverage, gallery sizing
    aura-explain/             # reasons, decisions, ledger
    aura-style/               # personal AI profiles
    aura-retouch/             # portrait + micro retouch
    aura-restore/             # denoise, sharpen, face recovery
    aura-geometry/            # lens, straighten, crop
    aura-generative/          # safe cleanup
    aura-brain-wedding/       # scenes, story, moments
    aura-brain-photo/         # technical + local light decisions
    aura-brain-gallery/       # consistency, camera matching
    aura-agents/              # agentic planners (proposals only)
    aura-qc/                  # QC agent, tickets, remedies
    aura-curate/              # album, hero, B&W, social
    aura-export/              # JPEG/TIFF/XMP export
    aura-delivery/            # backup + gallery providers
    aura-learn/               # learning loop
    aura-jobs/                # autopilot orchestrator
    aura-ipc/                 # Tauri command surface
  apps/desktop/               # Tauri + React + TypeScript
  ml/                         # PyTorch training, ONNX export, eval
  plugins/{lightroom,photoshop}/
  tools/{model-sign,aura-cli}/
  tests/{fixtures,e2e,qc}/
  docs/{adr,model-cards}/
  ops/{release,sign,update,flags,crash}/
```

## 8. Hard rules for code

- **No `unwrap()` or `expect()`** in library code. Return `Result<T, AuraError>`. Panics are reserved for provable invariants with a comment.
- **No blocking I/O on the UI thread.** All heavy work goes through `aura-jobs`.
- **All colour maths in linear light**, with explicit colour-space conversions at the boundaries. Ask COL before touching this.
- **Determinism:** identical input plus identical model versions must produce identical output. Seed everything. No time-dependent or map-iteration-order-dependent behaviour.
- **Every AI decision writes a `Reason`** into the ledger. A decision without a reason is a bug.
- **Every user override sets `user_edited`** on the affected field and is never overwritten by automation.
- **Feature-flag every AI stage** with a kill switch.
- **Frozen contracts** in phase documents are copied into code verbatim. Changing one requires an ADR.

## 9. Cloud AI rules (the user's API key)

- The key lives in the OS keychain. Never in SQLite, never in logs, never in telemetry, never in a prompt.
- Every cloud call goes through `aura-cloud`. Direct HTTP calls to model providers anywhere else are a lint error.
- Every cloud call: strict JSON schema, temperature 0, retries with backoff, cache by content hash, per-project budget cap, and a **local fallback that keeps the pipeline complete**.
- Send derivative data (thumbnails, crops, statistics), never original RAW files, and only after per-project consent.
- Record every call in `cloud_calls` with tokens, cost, latency and cache status. Show spend in the UI.
- Cloud reasoning proposes; deterministic code decides and executes. A model never executes an action directly.

## 10. When you are unsure

Write an ADR in `docs/adr/`, state the options, pick one, and explain the trade-off in three sentences.
Then implement it. A recorded decision beats a perfect decision made too late.

## 11. What we will never build

Body reshaping, skin lightening, face or eye swapping, adding people or objects that were not there,
or any operation that changes a person's identity. This is a product decision, enforced in code by guard
clauses and CI tests. Do not add these features even if asked casually; require an explicit CTO-role ADR.

---

# Product Strategy

## The opportunity

As of August 2026 the market is fragmented by capability. Aftershoot and Imagen own culling and
style-based editing. Evoto, Retouch4me and Aperty own portrait retouching. Topaz and DxO own restoration
and RAW enhancement. Lightroom and Capture One own the professional RAW workflow.

Nothing combines all of them with **wedding-story understanding, gallery-wide consistency, autonomous
quality control and genuinely zero-touch processing**. That gap is the product.

## The promise

> Shoot the wedding. Import the RAWs. Click once. Deliver.

One button. 1,000-4,000 RAW files in. A culled, edited, retouched, quality-checked, consistently graded,
export-ready gallery out. Every decision explained, scored and reversible.

## Positioning

- **We are** the autonomous AI post-production system for wedding photographers.
- **We are not** "AI skin retouching software" (crowded, commodity) and **not** "a Lightroom alternative"
  (an unwinnable category framing that invites the wrong comparison).
- Our comparison set is *a second shooter plus an editing outsourcing studio*, not a plugin.

## The four structural advantages

1. **Wedding Brain.** The product reasons about scenes, rituals, people and moments, so a dark dance frame
   is never judged by the standards of a formal family portrait.
2. **Gallery Brain.** We edit the gallery, not the photograph: reference frames anchor each scene and every
   other frame is normalised toward them. Nobody else does this.
3. **Autonomous QC.** A senior-retoucher agent inspects every edited frame, fixes what it can, replaces
   frames when a better alternative exists, and escalates the rest with numbers.
4. **The dataset.** Every consented wedding makes the models better in a way competitors cannot buy.

## Who we are for

- **Primary:** working wedding photographers delivering 15-60 weddings a year, 1,500-4,000 frames each,
  who currently spend 8-20 hours per wedding in post.
- **Secondary:** small studios with second shooters and mixed camera brands, who lose hours to matching.
- **Tertiary:** editing outsourcing studios who want throughput with an audit trail.

## What we deliberately refuse

- Body reshaping, skin lightening, identity-altering edits.
- Adding content that was never photographed.
- Requiring cloud upload of original RAW files.
- Hidden automation: everything is explained and reversible.

These refusals are a marketing asset. In a market nervous about AI, being the tool that documents exactly
what it did and refuses to fake reality is a durable trust position.

## Competitive scorecard (what "beating them" means concretely)

| Competitor | Their strength | Our specific win |
| --- | --- | --- |
| Aftershoot | End-to-end culling + editing + retouch | Deeper wedding understanding, gallery consistency, autonomous QC, offline capability |
| Imagen | Personal AI Profiles from ~2,000 edits | Scene-conditional profiles usable from 300 pairs, fully local training |
| Evoto | Batch portrait/skin/background/colour | Culling + story + gallery consistency + measured texture retention |
| Retouch4me | Layer-based professional retouch | Same depth plus RAW grading, culling and autonomous workflow in one pass |
| FilterPixel | Context-aware DeepCull with reasons | Same explainability extended through editing, retouch, QC and delivery |
| Narrative Select | High-volume culling with face/eye focus | Full automation with coverage guarantees instead of review-centred workflow |
| Lightroom | Mature RAW + AI Denoise/Masking/Remove | Decides *which* tool each photo needs, then does it across 3,000 frames |
| Capture One | RAW/colour excellence, AI masking | End-to-end wedding automation rather than tool-by-tool editing |
| Topaz Photo | Autopilot restoration | Restoration as an evidence-based pipeline decision, not a separate app |
| Aperty | Offline batch retouch | Offline retouch *plus* culling, grading, style learning and wedding intelligence |

## Success metrics for V1

| Metric | Target |
| --- | --- |
| Analysis + cull, 3,000 frames | under 8 minutes on the reference GPU |
| Full wedding end to end, 3,000 frames | under 2.5 hours on the reference GPU |
| Frames needing human intervention in Zero-Touch | 8 % or fewer |
| Keeper agreement with photographers | 0.85 or better |
| Style match to the photographer's own edits | dE00 2.5 or better |
| Trial-to-paid conversion | 25 % or better |
| Time saved per wedding (self-reported) | 8 hours or more |

---

# Architecture

## Principles

1. **Pipeline of specialists, not one giant model.** Each stage is independently improvable, testable and replaceable.
2. **Three-tier compute.** Cheap analysis on embedded previews, medium analysis on proxies, expensive work only on final selects.
3. **Local first, cloud optional.** Every stage has a complete local path. Cloud adds judgement and heavy generation.
4. **Non-destructive by construction.** The RAW file is read-only. The recipe is the truth.
5. **Explainability is data, not text generation.** Reasons are structured records emitted by the deciding code.
6. **Determinism.** Same inputs plus same model versions equals same outputs, always.

## Process and thread model

```
Tauri main process (Rust)
  |- UI (WebView: React + TypeScript)          <- IPC commands + event stream
  |- Job runtime (tokio multi-thread)
  |    |- CPU pool (rayon)      decode, hashing, EXIF, classical CV
  |    |- GPU queue (wgpu)      previews, render, masks, restoration
  |    |- Infer pool (ORT)      batched model inference, EP-aware
  |    +- Cloud queue           rate-limited, budgeted, cached
  |- Catalog (SQLite WAL, single writer, many readers)
  +- Cache (content-addressed files on disk)
```

One writer to SQLite, serialised through a channel. Long jobs never hold a transaction open.
The UI never blocks: it subscribes to progress events and reads snapshots.

## The pipeline

```
RAW files
  -> ingest (discover, hash, EXIF, timeline)
  -> previews (tier 1 embedded JPEG, tier 2 proxy 2048, tier 3 full)
  -> embeddings + similarity index
  -> faces + identities + subject hierarchy
  -> scenes + story segmentation + rituals
  -> bursts + duplicates + moments
  -> frame integrity (focus, motion, exposure, noise, eyes)
  -> emotion + moment ranking
  -> composition + aesthetics
  -> CULL (keep scores, coverage guard, gallery sizing)
  ============ selected frames only from here ============
  -> masks (semantic, identity-scoped, matted)
  -> exposure + white balance
  -> tone + curves + HSL + skin guard
  -> style profile deltas
  -> local light sculpting
  -> portrait retouch + micro retouch
  -> restoration (denoise, sharpen, face recovery)
  -> geometry (lens, straighten, crop)
  -> generative cleanup (safe, reviewed)
  -> camera + shooter matching
  -> gallery consistency (reference frames, skin, scene)
  -> QC agent (inspect, fix, replace, escalate)
  -> curation (B&W, heroes, album, social)
  -> export + delivery + learning loop
```

## Why the three-tier compute model matters

| Tier | Input | Work | Volume |
| --- | --- | --- | --- |
| 1 | Embedded RAW preview | Embeddings, faces, scene, integrity, emotion, composition, culling | All 4,000 |
| 2 | Proxy 2048 px | Masks, WB/exposure/tone/colour, style, local light, retouch decisions | ~1,000-1,800 selected |
| 3 | Full resolution | Final render, retouch pixels, restoration, cleanup, export | ~1,000 final |

Spending five seconds retouching a frame the culler will reject is the single most common way these
pipelines become slow. Tiering is not an optimisation; it is the architecture.

## Hardware strategy

At first run, `aura-infer` probes available execution providers and writes `hardware_plan.json`:
provider order (TensorRT, CUDA, DirectML, CoreML, CPU), VRAM budget, default batch sizes and probe timings.
Every later stage reads that plan instead of guessing. Under VRAM pressure, batches shrink before anything fails.

## IPC surface

Typed Tauri commands, one module per domain, mirrored by generated TypeScript types.
No command may take longer than 50 ms: anything heavier returns a job handle and streams progress events.

## Failure philosophy

- Every stage is resumable and idempotent.
- Optional stages may fail without failing the wedding; the run reports `CompletedDegraded` with specifics.
- Any stage can be disabled by a feature flag, and the pipeline must still produce a deliverable gallery.

---

# AI Model Stack

## Local models (ONNX, shipped and signed)

| Model | Job | Approx size | Runtime target | Quality gate |
| --- | --- | --- | --- | --- |
| `embed-clipish-512` | Perceptual embedding for similarity, clustering, uniqueness | 90 MB | 40 img/s GPU | dup recall 0.98 / precision 0.95 |
| `face-detect` | Face + landmark detection | 12 MB | 60 img/s | recall 0.97 at 24 px |
| `face-embed` | Identity embedding | 45 MB | 200 faces/s | identity F1 0.93 |
| `face-attr` | Eye state, gaze, expression, occlusion | 18 MB | 150 faces/s | blink F1 0.95 |
| `scene-wedding` | Scene class (17) + ritual + lighting + venue | 60 MB | 80 img/s | top-1 0.92 |
| `integrity` | Focus, motion, subject sharpness | 22 MB | 100 img/s | focus AUC 0.96 |
| `emotion-moment` | Emotion intensity, moment type, peak proximity | 40 MB | 60 img/s | pairwise agreement 0.80 |
| `aesthetic` | Composition and aesthetic scoring | 35 MB | 80 img/s | agreement 0.78 |
| `segment-14` | Semantic masks (skin, face, hair, clothing, sky, ...) | 75 MB | 20 img/s | mIoU 0.92 skin/face |
| `matting` | Alpha refinement for hair, veil, rim light | 30 MB | 25 img/s | no visible halo at 100 % |
| `blemish-detect` | Skin anomaly detection | 25 MB | 40 faces/s | recall 0.90 |
| `permanent-features` | Mole/freckle/scar/tattoo classification | 15 MB | 60 faces/s | false-removal 2 %, tattoos 0 % |
| `denoise-raw` | Noise-model-conditioned RAW denoise | 120 MB | 2.5 s per 45 MP | expert preference 80 % at ISO 6400+ |
| `face-recovery` | Gentle restoration of slightly soft faces | 80 MB | 0.6 s per face | identity distance under threshold, else skip |
| `distraction-detect` | Wedding distraction vocabulary | 35 MB | 40 img/s | precision 0.85 |
| `inpaint` (optional pack) | Local diffusion cleanup | 1.2 GB | 3 s per region | artefact-free 98 % |
| `artefact-check` | Detects failed inpainting | 20 MB | 100 regions/s | catches 95 % of known-bad |

Every model ships with a model card in `docs/model-cards/`, per-subgroup metrics (including skin-tone
buckets), a signed manifest entry in `models.lock`, and a documented fallback if it is unavailable.

## Learned-but-not-neural components

Exposure targets, white-balance priors, tone intent, colour behaviour, culling weights, coverage rules
and style deltas are **small fitted models** (regressions, robust estimators, lookup surfaces).
They train in minutes, are inspectable, deterministic, and can be shipped as config. Use a neural network
only where a fitted model genuinely cannot express the behaviour.

## Cloud reasoning (your API key)

Cloud AI is used for **judgement, not throughput**. Six governed tasks:

| Task | Phase | Trigger | Budget |
| --- | --- | --- | --- |
| Ritual and cultural scene disambiguation | 07 | Low-confidence segments only | 15 calls |
| Moment significance arbitration | 10 | Ambiguous emotional peaks | 20 calls |
| Ambiguous keep/reject arbitration | 12 | Near-threshold, must-have-coverage cases | 25 calls |
| Cleanup editorial judgement | 24 | Mid-confidence removal candidates | 20 calls |
| QC triage and remediation planning | 27 | Multi-symptom images | 40 calls |
| Album sequencing and captions | 29 | Once per album draft | 15 calls |

Global default cap: **75 calls and USD 1.50 per 3,000-image wedding**, cache hit rate 70 % or better.

**Rules, enforced in code:** strict JSON schema; temperature 0; derivative data only (thumbnails, crops,
statistics); never a RAW file; never the key in logs or prompts; per-project consent; every call recorded with
cost; and a complete local fallback so the pipeline never depends on the network. The model proposes;
deterministic code decides and executes.

## Training and deployment loop

```
PyTorch training (ml/)
  -> eval gate (per-subgroup metrics, model card)
  -> ONNX export + parity verification (max abs diff under tolerance)
  -> quantisation (int8/fp16) + re-verify
  -> sign manifest (ed25519) -> models.lock
  -> staged rollout with per-model rollback
```

A model that cannot demonstrate parity after export and quantisation does not ship. A model without a
card does not ship. A model whose subgroup metrics diverge by more than 10 % does not ship.

---

# Data Model

## Storage layout

```
<project>.aura/
  catalog.sqlite           # WAL mode, the single source of truth
  catalog.sqlite-wal
  recipes/<image_id>.json   # optional export form; canonical copy lives in SQLite
  cache/<hh>/<hash>.jpg     # tier-1 preview
  cache/<hh>/<hash>.proxy   # tier-2 proxy (2048 px)
  cache/<hh>/<hash>.meta    # decode metadata + pipeline_ver
  masks/<hh>/<hash>.masks   # compressed mask payloads
  models.lock               # pinned model versions + signatures
  hardware_plan.json        # probed execution providers and budgets
  ledger/                   # decision ledger segments
  exports/                  # delivery manifests
```

The cache is content-addressed by `content_hash` (xxhash3-128) plus `pipeline_ver`, so changing a decode
or preview algorithm invalidates exactly what it should and nothing more.

## Core tables

| Table | Purpose | Key columns |
| --- | --- | --- |
| `projects` | One wedding | id, name, created_at, engine_ver, consent_flags |
| `cameras` | Bodies and profiles seen in the project | id, make, model, serial, clock_offset_ms |
| `images` | One row per file | id, path, content_hash, camera_id, exif_ts, timeline_ts, width, height, iso, aperture, shutter, focal, flash, orientation |
| `ingest_journal` | Crash-safe ingest progress | id, image_id, stage, status, ts |
| `embeddings` | Perceptual vectors | image_id, vec(512 fp16), dhash, hsv_hist, luma_stats |
| `faces` | Detections | id, image_id, box, landmarks, quality, eye_state, expression, embedding |
| `identities` | Recurring people | id, label, role, face_count, prominence, user_locked |
| `scenes` | Story segmentation | segment_id, image_id, scene_class, ritual, lighting, confidence |
| `moments` | Burst and moment grouping | moment_id, image_id, duplicate_set, relation |
| `integrity` | Technical quality | image_id, focus, motion, clipping, noise_sigma_rel, flags |
| `emotion` | Emotional evidence | image_id, intensity, moment_type, peak_proximity, reaction_of |
| `composition` | Aesthetic evidence | image_id, score, horizon_deg, headroom, balance, crop_hint |
| `selection` | Culling result | image_id, keep_score, selected, rank_in_moment, runner_up_of |
| `recipes` | Edit recipes | image_id, version, json, user_edited_fields, engine_ver |
| `masks` | Mask payloads | id, image_id, kind, identity_id, payload, feather, confidence, user_edited |
| `profiles` | Style profiles | id, name, version, tree_json, diagnostics_json |
| `camera_transforms` | Matching results | camera_id, flash, reference, transform_json, evidence_pairs |
| `scene_nodes` | Gallery tree | id, parent_id, segment_id, anchors_json, target_json |
| `qc_tickets` | QC findings | id, image_id, category, diagnosis, deviation, remedy, status, round |
| `decisions` | The ledger | id, image_id, kind, value_json, reasons_json, confidence, autonomy, actor, ts |
| `corrections` | Learning loop input | id, decision_id, before_json, after_json, magnitude, ts |
| `cloud_calls` | Cloud audit | id, task, tokens_in, tokens_out, cost_usd, latency_ms, cache_status |
| `cloud_budget` | Caps and spend | project_id, cap_usd, spent_usd, cap_calls, used_calls |

## Edit recipe (schema v1, the heart of the product)

```json
{
  "schema": 1,
  "image_id": "...",
  "global": {
    "exposure": 0.31, "temperature": 4930, "tint": 8, "contrast": 11,
    "highlights": -31, "shadows": 22, "whites": 7, "blacks": -9,
    "vibrance": 6, "saturation": 0, "curve": [[0,0],[64,60],[128,132],[255,255]],
    "hsl": { "orange": { "h": -2, "s": -4, "l": 3 } }
  },
  "lens": { "distortion": true, "vignette": 0.8, "ca": true, "profile": "sony-fe-35-1.4" },
  "geometry": { "rotate": -0.6, "crop": [0.02,0.0,0.98,1.0], "aspect": "original" },
  "masks": [ { "id": "m1", "kind": "face", "identity": "bride", "exposure": 0.22, "shadows": 12 } ],
  "retouch": { "preset": "natural", "ops": [ { "op": "blemish", "box": [0.41,0.28,0.43,0.30] } ] },
  "restoration": { "denoise": "standard", "sharpen": 0.35, "face_recovery": 0.2 },
  "bw": null,
  "provenance": {
    "scene": "indoor_ceremony", "confidence": 0.982, "engine_ver": "1.0.0",
    "models": { "scene-wedding": "1.2.0" },
    "user_edited_fields": ["global.exposure"],
    "cleanup_disclosures": []
  }
}
```

`user_edited_fields` is sacred: automation must never overwrite a field listed there.

## Migrations

Forward-only, numbered, one file per phase that changes the schema (`0001_init.sql` ... `0030_delivery.sql`).
Every migration has an up-test on a populated fixture database. A release may never require a manual database step.

## Ledger

Append-only decision records: what was decided, the value, structured reasons, confidence, autonomy band,
actor (model, rule, cloud, user) and timestamp. Budget: 6 MB per 1,000 images. The ledger powers
"Explain My Edit", QC diagnosis, the learning loop and support bundles.

---

# Roadmap: 30 Phases

Each phase delivers exactly one powerful feature, end to end: schema, contracts, algorithms, UI, tests,
performance budgets, telemetry and acceptance criteria. Phases are sequential by dependency, not by convenience.

| # | Phase | Epic | Duration | Primary owners | Risk |
| --- | --- | --- | --- | --- | --- |
| 01 | [Project Foundation, Catalog & Wedding Project Ingest](phases/PHASE-01-FOUNDATION-CATALOG-INGEST.md) | E1 - Foundation | 2 weeks | CTO, TLC, SRC, SFE, DEVOPS | Medium - foundational mistakes are expensive later |
| 02 | [RAW Decode Engine & Three-Tier Preview Pyramid](phases/PHASE-02-RAW-DECODE-PREVIEW-PYRAMID.md) | E1 - Foundation | 2.5 weeks | TLC, SRC, COL, PERF | High - this is the throughput backbone of the product |
| 03 | [Inference Runtime Layer & Signed Model Package Manager](phases/PHASE-03-INFERENCE-RUNTIME-MODEL-REGISTRY.md) | E1 - Foundation | 2 weeks | MLL, MLOPS, SRG, SEC | High - hardware diversity is where desktop AI products die |
| 04 | [Cloud AI Gateway & Agentic Reasoning Runtime (bring-your-own key)](phases/PHASE-04-CLOUD-AI-GATEWAY.md) | E1 - Foundation | 2 weeks | AGT, CTO, SEC, MBE | High - cost, privacy and non-determinism all live here |
| 05 | [Perceptual Embeddings & Wedding Similarity Index](phases/PHASE-05-EMBEDDINGS-SIMILARITY-INDEX.md) | E2 - Wedding Brain | 1.5 weeks | MLL, SRML, SRC | Medium |
| 06 | [Face Detection, Recognition & People Intelligence](phases/PHASE-06-PEOPLE-INTELLIGENCE.md) | E2 - Wedding Brain | 2.5 weeks | MLL, SRML, SRC, SEC | High - accuracy and privacy both matter |
| 07 | [Wedding Scene AI & Story Timeline Segmentation](phases/PHASE-07-WEDDING-SCENE-STORY-AI.md) | E2 - Wedding Brain | 2.5 weeks | MLL, SRML, AGT, SRC | High - this is the core differentiator |
| 08 | [Smart Burst Grouping & Duplicate Detection](phases/PHASE-08-BURST-GROUPING-DUPLICATES.md) | E2 - Wedding Brain | 1.5 weeks | MLL, SRC, MLR | Medium |
| 09 | [Frame Integrity AI: Focus, Motion, Exposure, Noise & Eye State](phases/PHASE-09-FRAME-INTEGRITY-AI.md) | E2 - Wedding Brain | 2.5 weeks | MLL, SRML, COL, SRC | High - false rejections destroy trust instantly |
| 10 | [Expression, Emotion & Moment Ranking AI](phases/PHASE-10-EMOTION-MOMENT-AI.md) | E2 - Wedding Brain | 2.5 weeks | MLL, SRML, AGT, MLR | High - subjective and culturally sensitive |
| 11 | [Composition & Aesthetic AI](phases/PHASE-11-COMPOSITION-AESTHETIC-AI.md) | E2 - Wedding Brain | 2 weeks | MLL, SRML, MLR | Medium-High - aesthetics are subjective |
| 12 | [Autonomous Culling Engine, Story Coverage Guard & Gallery Sizing](phases/PHASE-12-CULLING-ENGINE-COVERAGE.md) | E2 - Wedding Brain | 3 weeks | MLL, SRC, PM, MLR | Critical - this is the product |
| 13 | [Explain My Edit, Confidence Calibration & Decision Ledger](phases/PHASE-13-EXPLAINABILITY-CONFIDENCE-LEDGER.md) | E2 - Wedding Brain | 2 weeks | MLL, AGT, SFE, SRC | Medium - but critical for adoption |
| 14 | [Non-Destructive Edit Recipe & GPU Develop Engine](phases/PHASE-14-DEVELOP-ENGINE-EDIT-RECIPE.md) | E3 - Photo Brain | 3 weeks | COL, SRG, TLC, PERF | High - correctness here is load-bearing for everything visual |
| 15 | [Exposure AI & White Balance AI (mixed lighting mastery)](phases/PHASE-15-EXPOSURE-WHITE-BALANCE-AI.md) | E3 - Photo Brain | 2.5 weeks | COL, MLL, SRML | High - the most visible AI decision in the product |
| 16 | [Tone AI, Adaptive Curves, HSL AI & Skin-Tone Protection](phases/PHASE-16-TONE-CURVES-COLOUR-AI.md) | E3 - Photo Brain | 2 weeks | COL, MLL, SRML | Medium-High |
| 17 | [Style Learning: Scene-Conditional Personal AI Profiles ("Teach My AI")](phases/PHASE-17-STYLE-LEARNING-PERSONAL-AI.md) | E3 - Photo Brain | 3 weeks | MLL, SRML, MLOPS, COL | High - the strongest retention feature in the product |
| 18 | [Local Mask AI: Automatic Semantic Masking](phases/PHASE-18-LOCAL-MASK-AI.md) | E3 - Photo Brain | 2.5 weeks | MLL, SRML, SRG | High - mask quality is visible in every retouch |
| 19 | [Local Light Sculpting: Face Lighting, Subject Enhancement, Background Balancing & Dodge/Burn AI](phases/PHASE-19-LOCAL-LIGHT-SCULPTING.md) | E3 - Photo Brain | 2 weeks | COL, MLL, SRG | Medium-High - subtlety is the whole point |
| 20 | [Portrait Retouch AI with Natural Texture Protection](phases/PHASE-20-PORTRAIT-RETOUCH-AI.md) | E4 - Retouch & Restoration | 3 weeks | MLL, SRML, COL, SRG | High - the most scrutinised output in the product |
| 21 | [Micro-Retouch Suite: Hair, Teeth, Eyes, Clothing & Glare](phases/PHASE-21-MICRO-RETOUCH-SUITE.md) | E4 - Retouch & Restoration | 2.5 weeks | MLL, SRML, SRG, COL | Medium-High - subtlety and 'uncanny' risk |
| 22 | [Restoration Stack: Scene-Aware Denoise, Selective Sharpen & Face Recovery](phases/PHASE-22-RESTORATION-STACK.md) | E4 - Retouch & Restoration | 3 weeks | COL, MLL, SRML, PERF | High - heavy compute and easy to overdo |
| 23 | [Geometry Suite: Lens Corrections, Straightening AI & Smart Crop](phases/PHASE-23-GEOMETRY-SUITE.md) | E4 - Retouch & Restoration | 1.5 weeks | COL, SRC, MLL | Medium |
| 24 | [Generative Cleanup & Distraction Removal (safe by construction)](phases/PHASE-24-GENERATIVE-CLEANUP.md) | E4 - Retouch & Restoration | 3 weeks | MLL, SRML, SRG, SEC | High - generative output is the easiest way to destroy trust |
| 25 | [Gallery Intelligence Engine: Cross-Photo Colour, Skin & Scene Consistency](phases/PHASE-25-GALLERY-INTELLIGENCE-ENGINE.md) | E5 - Gallery Brain & Autonomy | 3 weeks | COL, MLL, SRC, TLC | High - the marquee differentiator |
| 26 | [Multi-Camera & Second-Shooter Matching](phases/PHASE-26-MULTI-CAMERA-SHOOTER-MATCHING.md) | E5 - Gallery Brain & Autonomy | 2 weeks | COL, MLL, SRC | Medium-High |
| 27 | [AI Quality-Control Agent & Automatic Re-Edit Loop](phases/PHASE-27-AI-QC-AGENT.md) | E5 - Gallery Brain & Autonomy | 3 weeks | AGT, MLL, QAL, SRC | High - it is the last line of defence |
| 28 | [Zero-Touch Wedding Autopilot Orchestrator](phases/PHASE-28-ZERO-TOUCH-AUTOPILOT.md) | E5 - Gallery Brain & Autonomy | 3 weeks | TLC, EM, PERF, SRC | Critical - it is the product's headline |
| 29 | [Curation Intelligence: B&W Selection, Hero Photos, Album Story & Social Picks](phases/PHASE-29-CURATION-INTELLIGENCE.md) | E6 - Curation & Delivery | 2.5 weeks | MLL, AGT, PM, SRC | Medium |
| 30 | [Delivery, Integrations, Learning Loop & Release Engineering](phases/PHASE-30-DELIVERY-INTEGRATIONS-LEARNING-LOOP.md) | E6 - Curation & Delivery | 4 weeks | DEVOPS, TLC, MLOPS, PM, SEC | High - launch quality and data governance |

## Epics

| Epic | Phases | Outcome |
| --- | --- | --- |
| E1 Foundation | 01-04 | A desktop app that ingests 4,000 RAWs fast, renders previews on the GPU, runs local models on any hardware, and can call a cloud model safely. |
| E2 Wedding Brain | 05-13 | The app understands the wedding: people, scenes, story, moments, technical quality, and culls autonomously with explanations. |
| E3 Photo Brain / Develop | 14-19 | A non-destructive develop engine that gets exposure, white balance, tone, colour and local light right, in the photographer's own style. |
| E4 Retouch & Restoration | 20-24 | Portrait retouching, micro-retouching, denoise/sharpen/face recovery, geometry and safe generative cleanup. |
| E5 Gallery Brain & Autonomy | 25-28 | Gallery-wide consistency, multi-camera matching, an autonomous QC agent, and one-button Zero-Touch delivery. |
| E6 Curation & Delivery | 29-30 | Album, hero, B&W and social curation, then export, integrations, the learning loop and release engineering. |

## Version milestones

- **V1 Wedding Autopilot Core** - Phases 01-17 plus 28 and 30 (culling, grading, style, autopilot, export). This is the sellable product.
- **V2 Portrait Intelligence** - Phases 18-23 (masks, local light, retouch, micro-retouch, restoration, geometry).
- **V3 Gallery Intelligence** - Phases 25-27 (consistency, camera matching, QC agent).
- **V4 Advanced AI** - Phases 24 and 29 plus cloud scale-out and studio/team workflow.

Ship V1 before starting V2. A shipped culling-and-grading autopilot beats an unfinished everything-machine.

---

# The AURA AI Studio - Your Virtual IT Company (23 roles)

You are not hiring a department. You are instructing one coding agent to *think as* 23 specialists, one at a time,
with the discipline of a real studio. Every task in every phase is assigned to a role code below.
When Claude Code works a task, it must adopt that role's mandate, deliverable style and quality bar.

## How to use the roster

1. Open the phase file. Find the task table.
2. For each task, say: `Act as {ROLE} ({Title}). Mandate: {mandate}. Task: {task}. Deliverable: {deliverable}.`
3. Do not let one role review its own work. QA roles (QAL, QAIQ), SEC and PERF must review as separate passes.
4. Architecture-affecting decisions require the CTO or TLC role to write an ADR before code is written.
5. The EM role runs the phase ritual and refuses to close a phase whose Definition of Done is unmet.

## Roster

| Code | Title | Mandate |
| --- | --- | --- |
| `CTO` | Chief Architect / CTO Agent | Owns system architecture, ADRs, cross-phase invariants, tech-debt budget and the final technical sign-off on every phase gate. |
| `PM` | Product Manager Agent | Owns the feature definition, user stories, competitor parity checks and the user-visible acceptance criteria. |
| `EM` | Engineering Manager / Delivery Lead Agent | Owns task breakdown, sequencing, WIP limits, daily status, blocker escalation and the phase exit report. |
| `TLC` | Tech Lead - Imaging Core (Rust) | Owns crate boundaries, public API review, error taxonomy and the correctness of the core pipeline. |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Owns catalog, job graph, RAW/IO, concurrency, FFI safety and cancellation/resume semantics. |
| `SRG` | Senior Engineer - GPU & Render (Rust / wgpu / CUDA) | Owns the develop engine, shaders, tiling, GPU memory and render throughput. |
| `MLL` | ML Lead - Vision | Owns the model portfolio, training strategy, evaluation gates and model cards. |
| `SRML` | Senior ML Engineer | Owns model implementation, training runs, quantisation and ONNX export correctness. |
| `MLR` | ML Research Engineer | Owns literature review, ablations, loss design and label-schema experiments. |
| `MLOPS` | MLOps / Model Packaging Engineer | Owns the model registry, signing, delta updates, execution-provider benchmarking and model CI. |
| `AGT` | AI Agent & Prompt Engineer | Owns cloud LLM/VLM orchestration, tool schemas, prompt contracts, JSON validation and the cost governor. |
| `COL` | Colour Scientist | Owns colour spaces, camera profiles, skin-tone science and perceptual metrics (dE2000, CIECAM). |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Owns app shell, virtualised grid, state machines, typed IPC and UI performance. |
| `MFE` | Mid-Level Frontend Engineer | Owns panels, settings, review queues, i18n and component tests. |
| `MBE` | Mid-Level Backend / Cloud Engineer | Owns optional cloud services, licensing, uploads and delivery integrations. |
| `DATA` | Data Engineer / Dataset Curator | Owns dataset ingest, labelling pipeline, splits, dataset versioning and leakage prevention. |
| `QAL` | QA Lead - Automation | Owns test strategy, harness, fixtures, CI gates and the regression suite. |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Owns golden-image diffing, blind A/B panels, skin and colour audits, artefact hunting. |
| `PERF` | Performance Engineer | Owns benchmarks, profiling, memory ceilings, thermal behaviour and throughput budgets. |
| `DEVOPS` | DevOps / Release Engineer | Owns CI/CD, signed installers, crash reporting, telemetry pipeline and auto-update. |
| `SEC` | Security & Privacy Engineer | Owns key storage, biometric/PII handling, consent, sandboxing and the threat model. |
| `UX` | UX / UI Designer | Owns flows, wireframes, review affordances, explainability surfaces and accessibility. |
| `DOC` | Technical Writer | Owns documentation, in-app help, release notes, model cards and runbooks. |

## Escalation and decision rights

- **CTO** owns the invariants, the release gate and any decision that changes what the product will or will not do to a photograph.
- **TLC** owns cross-crate architecture, module boundaries and frozen contracts. Contracts change only by ADR.
- **MLL** owns model quality gates. A model ships only when its gate is met and its model card is published.
- **COL** owns colour correctness and has veto power over any change that harms colour fidelity or skin rendering.
- **QAL** owns the CI gate list. A red gate blocks a merge; nobody may bypass it.
- **PERF** owns every performance budget. Budgets are tests, not aspirations.
- **SEC** owns privacy, key handling, signing and the rule that client imagery never leaves the machine without consent.
- **PM** owns defaults, autonomy policy and anything a photographer will see or feel.
- **EM** owns sequencing, integration and the phase ritual.

## The two-hat rule

Whenever the agent writes code, it wears exactly one hat and states which one.
Whenever the agent reviews code, it wears a *different* hat and tries to break the work.
This single habit is what turns a code generator into an engineering organisation.

---

# QA Strategy

An AI application cannot be tested by asserting exact pixel values, and it cannot be shipped on vibes.
We test three different things with three different instruments: **correctness**, **quality** and **behaviour under stress**.

## The pyramid

- **Unit** - Pure functions, thresholds, scoring maths, serialisation round-trips, error taxonomy.
- **Property/fuzz** - Corrupt RAWs, truncated previews, absurd EXIF, 0-face and 60-face frames, 1-image and 6,000-image projects.
- **Golden image** - Frozen fixture set rendered and compared pixel-wise; dE2000 mean <= 0.5, max <= 2.0 unless intentionally changed and re-blessed.
- **Perceptual (human)** - QAIQ blind A/B against the previous build and against the named competitor for this feature; >= 60 % preference required.
- **Performance** - Throughput, wall clock, peak RAM, peak VRAM on the three reference machines.
- **Resume/kill** - Kill the process at 10 %, 50 %, 90 %; restart must continue without recomputation or corruption.
- **Regression** - Full previous-phase suite must stay green; no acceptance criterion from an earlier phase may regress.

## Fixture weddings (the backbone)

Three complete, consented, permanently versioned weddings live in `tests/fixtures/weddings/`:

| Fixture | Character | Why it exists |
| --- | --- | --- |
| `hindu_night` | Mixed tungsten and LED, heavy rituals, 3,200 frames, two bodies | Hardest white balance and ritual understanding |
| `daylight_church` | Bright windows, dark interior, formal groups, 2,400 frames | Extreme dynamic range and group logic |
| `nepali_reception` | Flash plus ambient, dance floor, high ISO, 2,800 frames | Restoration, motion, emotional peaks |

Every phase adds its own labels to these fixtures. Every quality gate is measured on them.

## Quality gates (examples, all enforced in CI)

| Gate | Threshold |
| --- | --- |
| Duplicate detection | recall 0.98, precision 0.95 |
| Face recall / identity F1 | 0.97 / 0.93 |
| Scene top-1 / ritual F1 | 0.92 / 0.85 |
| Blink F1 (intentional-closed false positives) | 0.95 (2 % or fewer) |
| Keeper agreement / missed must-haves | 0.85 / zero |
| Confidence calibration (ECE) | 0.05 or better |
| Exposure within 0.15 EV / WB within 200 K | 85 % of frames |
| Skin dE00 after grading | 3.0 or better, spread 1.0 or less across skin-tone buckets |
| Texture retention after retouch | 0.90 band-energy ratio |
| Permanent-feature false removal / tattoos | 2 % / 0 % |
| Identity drift after face recovery | below threshold, else operation skipped |
| Gallery WB spread reduction | 60 % or better |
| Per-identity skin dE00 spread across gallery | 2.0 or better |
| QC defect detection / auto-fix success | 90 % / 85 % |
| Cleanup artefact-free rate | 98 % |
| Crash-free session rate | 99.5 % |

## Perceptual and human review

- **Golden images:** reference renders with a perceptual difference threshold, not exact-pixel equality.
  Intentional changes require an explicit golden update in the pull request with a visual diff.
- **Blind studies:** for style match, retouch naturalness, restoration preference, curation agreement and
  camera matching, the QAIQ role runs blind comparisons with real photographers. These are release gates.
- **Zoom audits:** masks, retouch and cleanup are reviewed at 100 % zoom. Halos, bald patches and warped
  lines are bugs even when metrics pass.

## Chaos and endurance

Kill the process at 20 random points during a full wedding run. Sleep the machine. Unplug the drive.
Fill the disk. Reset the GPU driver. Revoke the API key mid-run. Every case must leave a resumable,
uncorrupted state with an honest message. Nightly CI runs a full 3,000-image wedding on real GPU hardware.

## Release gate

A release ships only when: all CI gates are green, the nightly long-run passed on all three reference
machines, no open blocker bugs, the perceptual golden set is clean, model cards are current, the privacy
document matches the code, and the CTO role has signed the checklist.

---

# Coding Standards

## Rust

- Edition 2021, stable toolchain pinned in `rust-toolchain.toml`. `#![deny(warnings)]` in CI.
- `clippy::pedantic` on, with documented per-crate allowances. `rustfmt` enforced.
- **Errors:** one `AuraError` enum per crate boundary using `thiserror`; `anyhow` only in binaries.
  Never `unwrap()`/`expect()` in library code. Every error carries actionable context.
- **No panics across FFI.** LibRaw and ONNX Runtime boundaries catch and convert.
- **Newtype every id:** `ImageId`, `FaceId`, `MaskId`, `DecisionId`. No bare `u64` or `String` ids.
- **Units in names:** `exposure_ev`, `temperature_k`, `ms`, `bytes`, `deg`. Ambiguous names are review failures.
- **Public API documented** with `///` including at least one example for non-trivial functions.
- **Concurrency:** no `Mutex` held across `await`. Prefer channels and message passing. One SQLite writer.
- **Determinism:** no `HashMap` iteration in output paths (use `BTreeMap` or sort), seed all randomness,
  never branch on wall-clock time in decision code.

## TypeScript / React

- `strict: true`, no `any` (use `unknown` and narrow). ESLint plus Prettier enforced.
- IPC types are generated from Rust; hand-written duplicates are a review failure.
- State: TanStack Query for server state, Zustand for view state. No global mutable singletons.
- Virtualise every list of images. The grid must stay at 60 fps with 4,000 items.
- Accessibility: keyboard-first workflows (culling and QC review are keyboard-driven), visible focus, ARIA labels.

## Python (training only)

- Ruff plus Black plus mypy. Every training run writes a config snapshot, seed, dataset hash and metrics to
  an experiment record. No notebook is a deliverable; notebooks may explore, scripts must reproduce.
- Every model export runs the parity verifier and refuses on failure.

## Commits, branches, reviews

- Branch: `phase-NN/<slug>`. Conventional commits: `feat(phase-07): ritual disambiguation`.
- Pull requests use `templates/PR.md` and must state the phase, the invariants touched, the gates run and the benchmark deltas.
- **Review checklist:** correctness, error handling, determinism, performance budget, privacy, explainability
  (does every decision emit a reason?), user-override protection, test coverage, docs, telemetry.
- Two-hat rule: the reviewing role must differ from the implementing role.

## Performance discipline

- Every budget in a phase document becomes a benchmark test. A budget regression fails CI.
- Measure before optimising; commit the measurement. `tracing` spans on every pipeline stage.
- Allocation discipline in hot loops: reuse buffers, avoid per-image heap churn, prefer slices over vectors.

## Security and privacy in code

- Secrets only in the OS keychain, accessed through one module.
- No image content, file paths or personal data in telemetry or crash reports.
- All file writes inside project directories, validated against path traversal.
- Dependency audit in CI (`cargo audit`, `npm audit`), pinned versions, reproducible builds.

---

# The Wedding Intelligence Dataset

The user interface can be copied in six months. The dataset cannot. This document is the moat plan.

## What one labelled wedding contains

```
RAW originals
+ photographer's final edits
+ edit parameters (XMP or fitted recipe)
+ scene and ritual classification per frame
+ selected / rejected status with reasons
+ burst and duplicate groupings
+ technical quality labels (focus, motion, eye state)
+ emotional and moment labels with pairwise rankings
+ identity labels with roles (bride, groom, family, VIP, guest)
+ retouch labels (temporary versus permanent features)
+ album sequence and hero picks
+ consent record and licence terms
```

## Growth plan

| Stage | Weddings | Source | Purpose |
| --- | --- | --- | --- |
| Seed | 30 | Paid licensing from 6-10 photographers across traditions | Bootstrap every model to gate level |
| Beta | 200 | Closed beta, opt-in contribution with revenue share or free licence | Generalisation across gear and lighting |
| V1 | 1,000 | Opt-in from paying customers plus continued licensing | Scene-conditional depth |
| Scale | 10,000 (30 M frames) | Opt-in at scale | Cultural and regional coverage no competitor can match |

Deliberate diversity targets: Hindu, Nepali, Muslim, Christian, Sikh and civil ceremonies; day and night;
indoor and outdoor; flash and natural light; five skin-tone buckets with balanced representation;
Sony, Canon, Nikon, Fujifilm bodies; single and multi-shooter teams.

## Consent and ethics (non-negotiable)

- Contribution is **opt-in per project**, never per account, never by default.
- Photographers must warrant that they hold the rights and that couples have consented.
- Data is stored encrypted with access logging; identity labels are pseudonymous; withdrawal deletes
  contributed data and is honoured within 30 days.
- Models trained on contributed data are documented; contributors get free licence terms or revenue share.
- No face recognition data ever leaves a customer machine as part of telemetry.

## Labelling operations

- Two independent labellers per subjective task (emotion, aesthetics, keep/reject) plus adjudication;
  publish inter-annotator agreement with every dataset version.
- Pairwise comparisons rather than absolute scores for taste-based labels - people are consistent about
  "A is better than B" and inconsistent about "this is a 7".
- Active learning: prioritise labelling frames where the model is least confident or where users override most.
- Every dataset version is hashed, immutable and referenced in the model card of anything trained on it.

## Training discipline

- Split by **wedding**, never by frame, or the model will memorise weddings and report fantasy metrics.
- Always report per-subgroup metrics (skin tone, tradition, lighting, camera brand). A 10 % subgroup gap blocks release.
- Every experiment records seed, config, dataset hash, hardware and metrics. Unreproducible results do not count.
- The learning loop (Phase 30) is the long-term engine: every correction a photographer makes is a free,
  perfectly targeted label. Capture it, verify it, and adopt it only with the user's approval.

---

# Business and Go-To-Market

## Value proposition in one sentence

AURA gives a wedding photographer their evenings back: 8-20 hours of post-production per wedding becomes
one button, one review pass, and a delivered gallery.

## Pricing (recommended)

| Plan | Price | Includes |
| --- | --- | --- |
| Trial | Free, 14 days | Two full weddings, watermark-free, all features except cloud reasoning |
| Solo | USD 29 / month or 290 / year | Unlimited weddings, local AI, one style profile set, Lightroom/Photoshop integration |
| Studio | USD 79 / month or 790 / year | Three seats, second-shooter matching, shared profiles, team delivery, priority support |
| Enterprise | Quote | Outsourcing studios: volume seats, audit reports, custom profiles, SLA |

Cloud reasoning uses the customer's own API key, so inference cost is never our margin risk.
This is a deliberate structural advantage over cloud-only competitors whose gross margin shrinks with usage.

## Unit economics (illustrative)

| Line | Solo |
| --- | --- |
| Revenue per year | USD 290 |
| Cloud cost to us | ~0 (customer key) |
| Model distribution and updates | ~USD 6 |
| Support (0.4 tickets/month at USD 3) | ~USD 15 |
| Payment and platform fees | ~USD 12 |
| Gross margin | ~USD 257 (89 %) |

High gross margin funds the dataset, which is the moat. Protect it: avoid features that require us to pay
for per-image cloud inference.

## Launch sequence

1. **Alpha (private, 10 photographers).** Phases 01-13. Culling only, positioned as "the most explainable
   wedding culler that exists". Collect licensed weddings.
2. **Beta (closed, 20 photographers).** Phases 01-17 plus 28 and 30. Culling plus grading plus style plus autopilot plus export.
   Publish honest benchmarks against competitor culling times.
3. **V1 public launch.** Zero-Touch Autopilot. Message: *shoot the wedding, import the RAWs, click once, deliver.*
4. **V2/V3 within 9 months.** Retouch depth and gallery intelligence, marketed as "the only tool that edits
   the gallery, not the photo".

## Distribution

- Wedding photography communities and Facebook groups (where this audience actually lives).
- Educators and workshop leaders: free studio licences in exchange for honest teaching.
- YouTube long-form: full 3,000-frame wedding processed in real time, unedited. This category buys on proof.
- Comparison content: side-by-side against the tools they already pay for, with our failures shown too.
  Credibility converts better than polish in this market.

## Retention levers

1. Style profiles get better the longer they use it (switching cost rises honestly).
2. The learning loop makes their corrections compound.
3. Delivery integrations put us in the middle of their workflow.
4. QC reports become part of their studio quality process.

## Key business risks

| Risk | Response |
| --- | --- |
| Adobe ships wedding-aware automation | Depth of wedding domain plus offline plus dataset; be the specialist they cannot justify being |
| Aftershoot/Imagen add gallery consistency | Ship it first and make it measurable; publish the metrics |
| Photographer backlash against AI editing | Radical transparency: explainability, disclosure, refusal list, no identity alteration |
| Support burden of GPU diversity | Hardware tiers, pre-flight checks, honest performance expectations, DirectML/CPU fallbacks |

---

# Risk Register

Severity: 1 (annoying) to 5 (project-ending). Owner is the role accountable for the mitigation.

| # | Risk | Sev | Owner | Mitigation | Trigger to act |
| --- | --- | --- | --- | --- | --- |
| R1 | Culling rejects a must-have moment | 5 | MLL | Coverage guard with hard rules, zero-missed gate, runner-up retention, QC replacement | Any fixture miss |
| R2 | Retouch produces plastic skin or removes permanent features | 5 | COL/MLL | Texture-retention floor in CI, permanent-feature classifier, conservative defaults | Gate breach or one field report |
| R3 | Generative cleanup damages a delivered photograph | 5 | SEC | Safety engine, size caps, denylists, artefact self-check, review-by-default | Any adversarial audit success |
| R4 | Two-hour autopilot run fails near the end | 4 | TLC | Per-stage checkpoints, resume, stage isolation, degraded completion, nightly long-run CI | Any resume failure |
| R5 | Performance budgets missed on real laptops | 4 | PERF | Tiered compute, hardware plan, adaptive batches, published hardware tiers | 20 % over budget |
| R6 | Face recovery or restoration changes identity | 5 | MLL | Embedding-distance constraint with skip-on-failure, 100 % CI gate | Any drift beyond threshold |
| R7 | Skin-tone bias in detection, retouch or grading | 5 | MLL/COL | Balanced labelling, per-bucket metrics, 10 % parity ship gate | Any bucket gap over 10 % |
| R8 | Cloud API key leak or cost blowout | 4 | SEC | Keychain-only storage, single gateway, hard budget caps, no key in logs or prompts | Any leak or cap breach |
| R9 | Privacy incident with client imagery | 5 | SEC | Local-first, derivative-only cloud data, per-project consent, no imagery in telemetry | Any unconsented transmission |
| R10 | Dataset acquisition stalls, models plateau | 5 | DATA/PM | Paid licensing early, opt-in with revenue share, learning loop, active labelling | Under 30 weddings by beta |
| R11 | Adobe or an incumbent ships the same automation | 4 | CTO/PM | Specialist depth, offline capability, dataset moat, faster iteration | Any credible announcement |
| R12 | Style learning bakes in a photographer's mistakes | 3 | MLL | Residual-on-baseline design, robust fitting, A/B before adoption | Complaint pattern |
| R13 | Gallery consistency flattens intentional mood | 4 | COL/PM | Damping under 1.0, hard bounds, change-point detection, labelled transition fixtures | Audit finding |
| R14 | QC agent creates new problems while fixing old ones | 4 | QAL | Improvement verification, automatic revert, no-regression checks, bounded rounds | Any regression in fixtures |
| R15 | Model artefacts bloat the installer | 2 | DEVOPS | Optional packs, delta updates, quantisation | Installer over 900 MB |
| R16 | Lightroom/Photoshop plugin breakage | 3 | MBE | Version detection, XMP-only degradation, compatibility matrix in CI | Any host update |
| R17 | Scope creep delays V1 indefinitely | 4 | PM/EM | Version milestones, ship V1 at Phase 17 plus 28 plus 30, phase gate discipline | Two phases over estimate |
| R18 | Determinism loss makes support impossible | 3 | TLC | Seeded everything, sorted iteration, golden tests, support bundles | Any non-reproducible report |
| R19 | Photographer trust collapse from a public AI failure | 5 | PM/CTO | Explainability, disclosure, refusal list, conservative autonomy, fast rollback | Any viral complaint |
| R20 | Key-person dependency in colour science | 3 | EM | Document COL methodology, pair reviews, recorded measurement procedures | Single-owner area |

## Standing rules that retire whole classes of risk

1. RAW files are read-only. Data loss risk becomes cache-corruption risk, which is recoverable.
2. Cloud is optional everywhere. Vendor outage becomes a slower run, not a broken product.
3. Every AI stage has a kill switch. A bad model becomes a config change, not an emergency release.
4. Every decision is in the ledger. "We cannot reproduce it" stops being a support outcome.

---

# Glossary

Use these exact terms in code, UI and documentation. Consistent vocabulary is how a 30-phase build stays coherent.

| Term | Meaning |
| --- | --- |
| **Autonomy band** | Confidence range that determines whether a decision auto-applies, applies in Zero-Touch, is suggested, or requires review (0.98+, 0.90-0.98, 0.75-0.90, under 0.75). |
| **Anchor** | A reference frame within a scene node whose colour and exposure the other frames are normalised toward. |
| **Chapter** | A user-facing grouping of segments (Getting Ready, Ceremony, Reception). |
| **Content hash** | xxhash3-128 of file bytes; identity of an image for caching and deduplication. |
| **Coverage rule** | A hard requirement that certain moments or people appear in the final selection. |
| **Decision** | A ledger record: what was chosen, why, with what confidence, by which actor. |
| **Develop engine** | The GPU pipeline that turns a RAW plus a recipe into pixels. |
| **Duplicate set** | A group of near-identical frames classified Identical, NearIdentical or Variant. |
| **Explain My Edit** | The UI surface that renders ledger reasons for culling and editing decisions. |
| **Frame integrity** | Technical quality evidence: focus, motion, clipping, noise, eye state. |
| **Gallery Brain** | Cross-photo reasoning: consistency, anchors, skin and scene harmonisation. |
| **Hardware plan** | Probed execution-provider order and resource budgets written at first run. |
| **Keep score** | Composite culling score combining integrity, emotion, composition, story value and uniqueness. |
| **Ledger** | Append-only store of all decisions, reasons and confidences. |
| **Mask kind** | Semantic class of a mask (skin, face, hair, clothing, subject, sky, ...). |
| **Moment** | A short burst of frames capturing the same event instant. |
| **Photo Brain** | Per-image technical and aesthetic decision-making. |
| **Pipeline version** | Version stamp that invalidates caches when decode or preview algorithms change. |
| **Proxy** | Tier-2 render at 2048 px used for analysis and interactive editing. |
| **Recipe** | The non-destructive JSON description of every edit applied to an image. |
| **Reason** | Structured explanation record attached to a decision. |
| **Ritual** | A culturally specific ceremony element (varmala, saptapadi, sindoor, vows, ring exchange, nikah). |
| **Runner-up** | The best non-selected frame in a moment, retained for QC replacement. |
| **Scene node** | A node in the wedding tree that groups images for consistency solving. |
| **Segment** | A contiguous time range assigned a scene class by the story model. |
| **Skin guard** | Constraint that prevents grading from pushing skin outside a plausible locus. |
| **Story graph** | The full wedding structure: chapters, segments, nodes, moments, people. |
| **Style tree** | Scene-conditional personal profile: global, group and bucket-level deltas. |
| **Subject hierarchy** | Ranking of people by role and prominence (couple, close family, VIP, guest). |
| **Texture retention** | Measured ratio of high-frequency energy in skin after retouching versus before. |
| **Ticket** | A QC finding with diagnosis, quantified deviation, remedy and status. |
| **Wedding Brain** | Scene, story, people and moment understanding. |
| **Zero-Touch** | The mode in which every stage runs autonomously within its autonomy bands. |

---

# Phase 01 - Project Foundation, Catalog & Wedding Project Ingest

> **Single feature shipped by this phase:** Create a wedding project, point it at 1-6 folders of RAWs from multiple cameras, and get a fully indexed, deduplicated, timeline-ordered catalog with a scrollable grid.
>
> **Mission:** Build the skeleton that every other phase bolts onto: monorepo, typed IPC, SQLite catalog, ingest walker, EXIF normalisation, multi-camera clock alignment and a virtualised grid that stays at 60 fps with 4,000 images.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 01 of 30 |
| Epic | E1 - Foundation |
| Feature | Create a wedding project, point it at 1-6 folders of RAWs from multiple cameras, and get a fully indexed, deduplicated, timeline-ordered catalog with a scrollable grid. |
| Depends on | Nothing |
| Unlocks | All phases |
| Duration | 2 weeks |
| Primary owners | Chief Architect / CTO Agent, Tech Lead - Imaging Core (Rust), Senior Engineer - Core Pipeline (Rust), Senior Frontend Engineer (Tauri + React), DevOps / Release Engineer |
| Risk level | Medium - foundational mistakes are expensive later |
| Headline KPI | 4,000 files indexed in < 90 s from a fast SSD; grid scroll >= 60 fps; catalog open < 400 ms |
| Competitor being beaten | Lightroom import speed and Narrative Select project setup |

## 1. Why this phase exists

Every competitor's real bottleneck is the boring part: getting thousands of files from several cards, several cameras and two photographers into one coherent, queryable timeline. If ingest is slow or lossy, no amount of AI later can save the product.

A wedding is a *story ordered in time*. That ordering is created here, not later: cameras with drifting clocks must be aligned, second-shooter files must be interleaved correctly, and the capture sequence must survive filename chaos (`IMG_0001` from two bodies).

This phase also fixes the shape of the whole codebase: crate boundaries, the typed Rust<->TypeScript bridge, migration strategy, error taxonomy, logging, CI and the fixture weddings that every later phase tests against.

## 2. Scope contract

### 2.1 In scope

- Monorepo scaffold: Cargo workspace of crates + Tauri desktop app + React/TypeScript UI + Python `ml/` tree + `docs/`.
- `aura-catalog`: SQLite (WAL) schema, versioned migrations, prepared-statement layer, transactional batch insert.
- Ingest walker: recursive scan, extension allow-list, symlink safety, network-volume detection, per-file xxhash3 content hash, resumable ingest journal.
- EXIF/maker-note extraction: camera body + serial, lens, focal length, ISO, shutter, aperture, flash fired, orientation, capture timestamp with sub-second field, GPS.
- Multi-camera clock alignment: cluster by body serial, estimate per-body offset from overlapping bursts, expose a manual nudge, write `timeline_ts` separate from `exif_ts`.
- Sidecar and duplicate-file handling: RAW+JPEG pairs, `.xmp` discovery, already-imported detection by hash, missing-file relink.
- Virtualised grid UI: windowed rendering, keyboard navigation, filmstrip, project switcher, empty/loading/error states.
- Typed IPC layer: one Rust command registry, generated TypeScript types, cancellable long-running commands with progress events.
- Reference fixtures: three anonymised mini-weddings (150 images each) committed via Git LFS for CI.

### 2.2 Explicitly out of scope (do not build it here)

- RAW decoding and preview generation (Phase 02).
- Any AI model, embedding or score (Phases 05+).
- Editing, recipes or rendering (Phase 14+).
- Cloud sync, licensing, telemetry upload (Phase 30).
- Pretty visual design polish - functional, accessible UI only; UX system lands progressively.

## 3. Architecture and data flow

```text
  Card / folder(s)              aura-core (domain types)
        |                              ^
        v                              |
  IngestWalker  --file events-->  IngestPipeline  --batch-->  aura-catalog (SQLite WAL)
        |                              |                             |
   xxhash3 + stat              ExifExtractor (exiv2 FFI)        migrations/*.sql
        |                              |                             |
        +-----> IngestJournal <--------+                             v
                (resume, dedupe)                            ClockAligner -> timeline_ts
                                                                     |
                                                                     v
  React UI  <--typed IPC events (progress, rows)--  aura-ipc  <--  query API
```

- Ingest is a bounded producer/consumer: one walker thread, N hashing/EXIF workers (N = cores-2), one writer thread owning the SQLite connection. Never write to SQLite from multiple threads.
- Every batch insert is one transaction of <= 500 rows so the UI sees rows appear continuously and a crash loses at most one batch.
- `timeline_ts` is the single source of truth for ordering everywhere else in the product; `exif_ts` is preserved untouched for forensics.

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `Cargo.toml` (workspace) | Declares all crates, shared dependency versions, release profile with LTO. |
| `crates/aura-core/src/{lib,ids,image,project,error,progress}.rs` | Domain types: `ImageId`, `ProjectId`, `CaptureMeta`, `AuraError`, progress/cancel primitives. |
| `crates/aura-catalog/src/{lib,schema,migrate,images,projects,query,tx}.rs` | SQLite access layer, migrations, typed row structs, batch writer. |
| `crates/aura-catalog/migrations/0001_init.sql` | Initial schema: projects, images, cameras, ingest_journal, settings. |
| `crates/aura-ingest/src/{lib,walker,hash,exif,clock,journal,pipeline}.rs` | Scan, hash, EXIF, clock alignment, resumable journal, orchestration. |
| `crates/aura-ipc/src/{lib,commands,events,types}.rs` | Tauri command registry, event bus, `ts-rs` type export. |
| `apps/desktop/src-tauri/src/main.rs` | App bootstrap, state, single-instance guard, panic hook, logging. |
| `apps/desktop/src/{app,routes,state}/*` | React shell, router, Zustand store, IPC client with generated types. |
| `apps/desktop/src/components/grid/{VirtualGrid,Cell,Filmstrip}.tsx` | Windowed grid, selection model, keyboard navigation. |
| `tests/fixtures/weddings/{hindu_night,daylight_church,nepali_reception}/` | Reference mini-weddings + expected-catalog JSON snapshots. |
| `justfile` | `just dev`, `just test`, `just bench`, `just phase-01-verify`. |
| `.github/workflows/ci.yml` | Matrix CI: Windows/macOS/Linux, fmt, clippy, tests, fixture ingest benchmark. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Catalog core schema (migration 0001)**

```sql
CREATE TABLE projects (
  id           TEXT PRIMARY KEY,
  name         TEXT NOT NULL,
  couple_names TEXT,
  event_date   TEXT,
  created_at   TEXT NOT NULL,
  schema_ver   INTEGER NOT NULL,
  settings_json TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE cameras (
  id            TEXT PRIMARY KEY,
  project_id    TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  make          TEXT, model TEXT, body_serial TEXT,
  shooter_label TEXT,              -- 'primary' | 'second' | free text
  clock_offset_ms INTEGER NOT NULL DEFAULT 0,
  UNIQUE(project_id, body_serial)
);

CREATE TABLE images (
  id           TEXT PRIMARY KEY,
  project_id   TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  camera_id    TEXT REFERENCES cameras(id),
  abs_path     TEXT NOT NULL,
  rel_path     TEXT NOT NULL,
  file_name    TEXT NOT NULL,
  ext          TEXT NOT NULL,
  bytes        INTEGER NOT NULL,
  content_hash TEXT NOT NULL,          -- xxhash3-128 hex
  exif_ts      TEXT,                   -- as recorded
  timeline_ts  TEXT,                   -- clock-aligned, authoritative order
  sub_sec      INTEGER,
  iso          INTEGER, shutter REAL, aperture REAL, focal_len REAL,
  lens         TEXT, flash_fired INTEGER, orientation INTEGER,
  width INTEGER, height INTEGER,
  gps_lat REAL, gps_lon REAL,
  status       TEXT NOT NULL DEFAULT 'indexed', -- indexed|missing|error
  error_text   TEXT,
  created_at   TEXT NOT NULL,
  UNIQUE(project_id, content_hash)
);
CREATE INDEX idx_images_timeline ON images(project_id, timeline_ts);
CREATE INDEX idx_images_camera   ON images(project_id, camera_id, timeline_ts);

CREATE TABLE ingest_journal (
  project_id TEXT NOT NULL, abs_path TEXT NOT NULL,
  phase TEXT NOT NULL,          -- discovered|hashed|exif|inserted|failed
  updated_at TEXT NOT NULL,
  PRIMARY KEY(project_id, abs_path)
);
```

**Ingest API (frozen)**

```rust
pub struct IngestOptions {
    pub roots: Vec<PathBuf>,
    pub include_jpeg_pairs: bool,
    pub follow_symlinks: bool,
    pub worker_threads: Option<usize>,
}

#[derive(Clone, serde::Serialize)]
pub enum IngestEvent {
    Discovered { total_hint: u64 },
    Progress { done: u64, total: u64, current: String },
    Batch { rows: Vec<ImageRowLite> },
    Warning { path: String, message: String },
    Finished { inserted: u64, skipped: u64, failed: u64, elapsed_ms: u64 },
}

pub trait Ingestor: Send + Sync {
    fn ingest(
        &self,
        project: ProjectId,
        opts: IngestOptions,
        cancel: CancelToken,
        sink: &dyn Fn(IngestEvent),
    ) -> Result<IngestSummary, AuraError>;
}
```

**Typed IPC surface generated for the UI**

```typescript
export type ImageRowLite = {
  id: string; fileName: string; timelineTs: string | null;
  cameraId: string | null; width: number; height: number; status: 'indexed' | 'missing' | 'error';
};

export const api = {
  createProject: (input: { name: string; coupleNames?: string; eventDate?: string }) => invoke<{ id: string }>('create_project', input),
  startIngest:   (input: { projectId: string; roots: string[] }) => invoke<{ jobId: string }>('start_ingest', input),
  cancelJob:     (input: { jobId: string }) => invoke<void>('cancel_job', input),
  listImages:    (input: { projectId: string; offset: number; limit: number; orderBy?: 'timeline' | 'filename' }) => invoke<ImageRowLite[]>('list_images', input),
  setCameraLabel:(input: { cameraId: string; shooterLabel: string; clockOffsetMs: number }) => invoke<void>('set_camera_label', input),
};
```

## 6. Algorithm, model and implementation design

### 6.1 Ingest pipeline mechanics

- Walk with `jwalk` (parallel directory iteration) and stream paths into a bounded channel of capacity 1,024 to keep memory flat.
- Hash with xxhash3-128 over the first 1 MiB + last 1 MiB + file size for speed; upgrade to full-file hash lazily when a collision is detected.
- Extract EXIF using an `exiv2`/`rexiv2` FFI wrapper; never decode the image here. Target < 8 ms per file.
- Insert in transactions of 500 rows; emit a `Batch` event so the grid grows visibly during import.
- Journal every path transition so a crash or cancel resumes exactly where it stopped.

### 6.2 Multi-camera clock alignment (a real competitive detail)

- Group images by `body_serial`; the body with the most frames becomes the reference clock.
- For every other body, build a histogram of nearest-neighbour timestamp deltas against the reference within +/- 30 minutes; the mode of that histogram is the coarse offset.
- Refine with least-squares on matched high-activity windows (ceremony/dance) where both bodies fire densely; store `clock_offset_ms` on the camera row.
- Expose the offset in the UI with a two-frame comparison so the photographer can nudge it; recomputing `timeline_ts` is a single UPDATE and must take < 1 s for 4,000 rows.
- Guard: if confidence in the estimated offset is < 0.6, keep offset 0, flag the camera, and tell the user which two frames to compare.

### 6.3 Grid performance strategy

- Virtualise on both axes; render only visible cells plus two rows of overscan.
- Cells subscribe to a thumbnail store keyed by `ImageId`; in this phase the store returns placeholders, Phase 02 fills real pixels without any UI change.
- Selection is a roaring-bitmap-style index, not an array, so select-all on 4,000 images is instant.
- All IPC list calls are paginated and cancelled on scroll direction change.

### 6.4 Error taxonomy and safety

- `AuraError` variants: `Io`, `UnsupportedFormat`, `CorruptMetadata`, `Catalog`, `Cancelled`, `Internal` - each with a user-facing message and a machine code.
- Files that fail EXIF still get indexed with `status='error'` and the reason: never silently drop a photographer's file.
- Read-only file opens everywhere; a unit test asserts no write handle is ever requested on a fixture RAW.

## 7. Cloud AI usage (bring-your-own API key)

No cloud AI call in this phase. The phase must work with the network cable unplugged; the Cloud AI Gateway from Phase 04 stays idle here.

## 8. Implementation order (execute literally, in this order)

1. Create the workspace, crates, Tauri app, `justfile`, CI, and a `docs/adr/ADR-0001-architecture.md` recording the stack choice.
2. Write `aura-core` domain types and the error taxonomy; no logic yet.
3. Write migration `0001_init.sql` plus the migration runner and a round-trip test on a temp DB.
4. Implement the walker and hashing with a benchmark on 4,000 synthetic files.
5. Implement EXIF extraction; snapshot-test against fixture files from Canon CR3, Sony ARW, Nikon NEF, Fuji RAF and DNG.
6. Implement the batch writer and the ingest journal; add kill/resume tests.
7. Implement clock alignment with unit tests using synthetic offsets of -90 s, +12 min and +7 h.
8. Expose IPC commands, generate TypeScript types, wire the React shell and virtual grid.
9. Add the three reference fixture weddings and the expected-catalog snapshots.
10. Run `just phase-01-verify`, record the benchmark table, write the exit report.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `CTO` | Chief Architect / CTO Agent | Write ADR-0001 (Tauri + Rust core + ONNX Runtime + SQLite) and freeze crate boundaries | ADR + crate map | 6 h |
| `PM` | Product Manager Agent | Write the ingest user stories and the 'import a two-shooter wedding' acceptance script | Story set + QA script | 5 h |
| `EM` | Engineering Manager / Delivery Lead Agent | Break this phase into tracker tasks, set WIP limits, schedule the design review | Sprint board | 4 h |
| `TLC` | Tech Lead - Imaging Core (Rust) | Design `aura-core` types, `AuraError`, progress/cancel primitives; review every PR in this phase | Frozen core API | 2 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Implement walker, hashing, EXIF FFI, journal, batch writer, clock aligner | `aura-ingest` + tests | 6 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Implement catalog schema, migrations and query API | `aura-catalog` + tests | 3 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Build the Tauri shell, typed IPC client, virtual grid, filmstrip and project switcher | Working app shell at 60 fps | 5 d |
| `MFE` | Mid-Level Frontend Engineer | Build project-create wizard, folder picker, camera/shooter labelling panel, progress UI | UI panels + component tests | 3 d |
| `UX` | UX / UI Designer | Flow for 'new wedding -> pick folders -> label shooters -> grid', plus loading/error states | Wireframes + states | 2 d |
| `DATA` | Data Engineer / Dataset Curator | Assemble and anonymise the three reference fixture weddings, document licence/consent | Fixture set in Git LFS | 3 d |
| `QAL` | QA Lead - Automation | Build the test harness, fixture runner, catalog snapshot diffing, kill/resume tests | CI suite green | 4 d |
| `PERF` | Performance Engineer | Benchmark ingest throughput and grid frame time; publish the budget dashboard | Benchmark report | 2 d |
| `DEVOPS` | DevOps / Release Engineer | CI matrix, Git LFS, caching, artefact upload, crash-log capture | `ci.yml` green on 3 OSes | 3 d |
| `SEC` | Security & Privacy Engineer | Threat model v1; enforce read-only opens; decide where the DB and caches live per OS | Threat model doc | 2 d |
| `DOC` | Technical Writer | Write `docs/runbooks/ingest.md` and the module READMEs | Docs merged | 1 d |

### 9.1 Handoff chain for this phase

```text
CTO/ADR -> TLC (core types frozen)
            |
            +-> SRC (ingest + catalog) --------+
            +-> SFE/MFE (shell + grid, stubs)  +--> QAL (fixtures, snapshots) -> PERF (benchmarks)
            +-> DATA (fixture weddings) -------+                                   |
                                                                                   v
                                                                       EM/PM/CTO phase gate
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

- Ingest 4,000 mixed-format files: exact row count, zero duplicates, zero silent drops.
- Re-run ingest on the same folders: 100 % skipped by hash, < 5 s.
- Kill the process mid-ingest at 10/50/90 %: resume completes with an identical final catalog snapshot.
- Clock alignment: synthetic offsets recovered within +/- 500 ms; low-confidence case correctly refuses to guess.
- Corrupt/truncated RAW, zero-byte file, unicode and emoji filenames, 300-character paths, files on a slow network share.
- Grid: scroll 4,000 items with frame time <= 16.6 ms p95; select-all < 50 ms.
- Migration test: 0001 applied to an empty DB and to a v0 DB, then rolled back.

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
| Index 4,000 RAWs (NVMe) | <= 90 s wall clock |
| Per-file EXIF + hash | <= 20 ms average |
| Catalog open (existing 4,000-image project) | <= 400 ms |
| Grid frame time p95 | <= 16.6 ms |
| Peak RAM during ingest | <= 800 MB |
| SQLite file size for 4,000 images | <= 12 MB |

Telemetry events (local-first, opt-in aggregation):

- `ingest.started` {file_count_hint, root_count, volume_type}
- `ingest.finished` {inserted, skipped, failed, elapsed_ms, files_per_sec}
- `ingest.file_failed` {reason_code, ext}
- `clock.aligned` {camera_count, offsets_ms, confidence}
- `ui.grid_fps` {p50, p95, item_count}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| EXIF library gaps on new camera bodies | Vendor-neutral maker-note fallback + a `unknown_camera` telemetry event and monthly camera-support sprint. |
| SQLite write contention stalls the UI | Single writer thread, WAL mode, 500-row transactions, all reads on pooled read-only connections. |
| Network volumes murder throughput | Detect volume type, reduce worker count, warn the user, offer 'copy locally first'. |
| Clock alignment guesses wrong and scrambles the story | Confidence gate + manual nudge + `exif_ts` preserved so the operation is always reversible. |
| Foundation churn slows later phases | Interfaces frozen in section 5 and changed only via ADR amendment. |

## 13. Acceptance criteria

- [ ] A photographer can create a wedding project, add three folders from two bodies, and see a correctly time-ordered grid.
- [ ] 4,000 files index in <= 90 s on the reference NVMe machine with peak RAM <= 800 MB.
- [ ] Re-import is a no-op; moved files relink by hash without duplicating rows.
- [ ] Second-shooter frames interleave correctly after automatic clock alignment; the offset is visible and editable.
- [ ] Killing the app at any point never corrupts the catalog and always resumes.
- [ ] Every failed file is visible in a 'Problems' list with a plain-language reason.
- [ ] Generated TypeScript types match the Rust commands; a type mismatch fails CI.
- [ ] `just phase-01-verify` runs ingest on all three fixture weddings and diffs catalog snapshots to zero.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 01 - Project Foundation, Catalog & Wedding Project Ingest.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-01-FOUNDATION-CATALOG-INGEST.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - Create a wedding project, point it at 1-6 folders of RAWs from multiple cameras, and get a fully indexed, deduplicated, timeline-ordered catalog with a scrollable grid.

Rules:
  - Do not start Phase 2. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `Cargo.toml` (workspace), `crates/aura-core/src/{lib,ids,image,project,error,progress}.rs`, `crates/aura-catalog/src/{lib,schema,migrate,images,projects,query,tx}.rs`, `crates/aura-catalog/migrations/0001_init.sql`, `crates/aura-ingest/src/{lib,walker,hash,exif,clock,journal,pipeline}.rs`, `crates/aura-ipc/src/{lib,commands,events,types}.rs`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-01-foundation-catalog-ingest and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-01.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-01-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-01-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-01-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 01 of 30 - Project Foundation, Catalog & Wedding Project Ingest - part of the AURA Wedding AI master build plan.*

---

# Phase 02 - RAW Decode Engine & Three-Tier Preview Pyramid

> **Single feature shipped by this phase:** Instant, colour-correct previews for every RAW: embedded JPEG for triage, 2048 px proxy for AI, and on-demand full-resolution decode for final render.
>
> **Mission:** Make 4,000 RAWs feel like 4,000 JPEGs. Extract embedded previews at thousands of files per minute, generate cached proxies for AI, and provide a demosaic path that is colour-managed and reproducible.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 02 of 30 |
| Epic | E1 - Foundation |
| Feature | Instant, colour-correct previews for every RAW: embedded JPEG for triage, 2048 px proxy for AI, and on-demand full-resolution decode for final render. |
| Depends on | Phase 01 |
| Unlocks | Phases 05-30 (everything needs pixels) |
| Duration | 2.5 weeks |
| Primary owners | Tech Lead - Imaging Core (Rust), Senior Engineer - Core Pipeline (Rust), Colour Scientist, Performance Engineer |
| Risk level | High - this is the throughput backbone of the product |
| Headline KPI | embedded previews for 4,000 files in <= 3 min; 2048 px proxy <= 120 ms/image GPU-assisted; cache hit read <= 8 ms |
| Competitor being beaten | Narrative Select's fast preview loading; DxO PureRAW batch parallelisation |

## 1. Why this phase exists

The three-tier pipeline is the economic core of the product. Deep analysis on 4,000 full-resolution RAWs is unaffordable; extracting the camera's embedded JPEG is nearly free and is good enough for scene, face, expression and duplicate analysis.

Preview quality decides AI quality. If the proxy is contrast-crushed or wrongly white-balanced, every downstream model sees a lie. So proxies must come from a documented, linear, colour-managed path - not from whatever the camera baked.

Caching is a first-class feature: a photographer will reopen the project many times, and reopening must never re-decode.

## 2. Scope contract

### 2.1 In scope

- LibRaw FFI wrapper (`aura-raw`) with safe Rust bindings, panic isolation and per-file timeouts.
- Tier 1: embedded preview extraction (JPEG/thumb) with orientation applied, plus a synthetic fallback when a file has no embedded preview.
- Tier 2: proxy generation at 2048 px long edge - half-size demosaic, camera matrix to linear Rec.2020, neutral tone curve, sRGB output for models, plus a linear 16-bit variant for colour work.
- Tier 3: full-resolution decode API (AHD/DCB or LibRaw's dcraw-compatible path) used only on final render, streamed in tiles.
- Content-addressed on-disk cache: `cache/<hash[0:2]>/<hash>.{jpg,proxy,meta}` with size accounting, LRU eviction and a user-visible cache budget.
- Colour management: camera profile lookup (DNG matrices/DCP where available), working space definition, ICC tagging on export previews.
- Decode worker pool with backpressure, per-file timeout, poison-file quarantine, and NUMA/thread affinity tuning.
- Thumbnail store integration so the Phase 01 grid becomes real pixels with zero UI changes.

### 2.2 Explicitly out of scope (do not build it here)

- Editing operations or recipes (Phase 14).
- Denoise/sharpen models (Phase 22).
- Lens corrections (Phase 23).
- Any AI inference (Phase 03 provides the runtime, Phase 05 the first model).

## 3. Architecture and data flow

```text
                        +--------------------- PreviewService ---------------------+
  images table ---------> | tier1: embedded JPEG  (libraw_unpack_thumb)  ~6 ms/file |
                          | tier2: proxy 2048 px  (half demosaic + colour) ~120 ms  |
                          | tier3: full decode    (tiled, on demand)      ~1-3 s    |
                          +---------------------------+-----------------------------+
                                                      |
                                        content-addressed cache (LRU, budgeted)
                                                      |
              +---------------------------+-----------+-----------------------+
              v                           v                                   v
        Grid thumbnails            AI proxy consumers                Full-res render (Phase 14)
                                   (Phases 05-13, 18-22)
```

- All three tiers are behind one trait so later phases request `Pixels::at_least(level)` and never care where the data came from.
- Proxy pixels are always stored twice: 8-bit sRGB JPEG (for models and UI) and 16-bit linear (for colour maths), because re-linearising an 8-bit JPEG loses the highlights that WB and exposure models need.
- LibRaw runs in a worker with a watchdog; a hanging or crashing decode quarantines the file rather than killing the app.

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-raw/src/{lib,ffi,libraw_sys,thumb,proxy,full,orientation,timeout}.rs` | LibRaw bindings and the three decode tiers. |
| `crates/aura-raw/src/colour/{matrix,profile,working_space,curve}.rs` | Camera matrices, DCP handling, linear working space, neutral preview curve. |
| `crates/aura-cache/src/{lib,store,lru,budget,paths}.rs` | Content-addressed cache with eviction and accounting. |
| `crates/aura-preview/src/{lib,service,pool,request,priority}.rs` | Priority queue: visible cells first, then AI batch, then background. |
| `crates/aura-preview/benches/decode.rs` | Criterion benchmarks per tier and per camera format. |
| `apps/desktop/src/stores/thumbnailStore.ts` | Subscribes to preview events, LRU in-memory bitmap cache, cancels off-screen work. |
| `tests/golden/proxy/*.png` + `tests/golden/manifest.json` | Blessed proxy outputs per camera model with dE2000 tolerances. |
| `docs/adr/ADR-0002-colour-pipeline.md` | Working space, transfer function and profile decisions. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Preview service trait (frozen)**

```rust
pub enum PixelLevel { Thumb(u32), Proxy2048, Full }

pub struct PixelBuffer {
    pub width: u32, pub height: u32,
    pub data: PixelData,             // Srgb8 | Linear16 | Tiled(Full)
    pub colour_space: ColourSpace,
    pub source: PixelSource,         // Embedded | Demosaiced
    pub decode_ms: u32,
}

pub trait PreviewService: Send + Sync {
    fn get(&self, id: ImageId, level: PixelLevel, prio: Priority) -> Result<Arc<PixelBuffer>, AuraError>;
    fn prefetch(&self, ids: &[ImageId], level: PixelLevel);
    fn cache_stats(&self) -> CacheStats;
    fn quarantine(&self) -> Vec<(ImageId, String)>;
}
```

**Per-image preview metadata sidecar in cache**

```json
{
  "image_id": "img_01H...",
  "content_hash": "9f2c...",
  "tier1": { "w": 1616, "h": 1080, "source": "embedded", "bytes": 412331, "decode_ms": 6 },
  "tier2": { "w": 2048, "h": 1365, "source": "demosaic_half", "linear16": true, "decode_ms": 118 },
  "camera_profile": { "make": "Sony", "model": "ILCE-7M4", "matrix": "adobe_dng", "illuminant": "D65" },
  "as_shot_neutral": [0.4523, 1.0, 0.6041],
  "black_level": 512, "white_level": 16383,
  "engine": { "libraw": "0.22.0", "pipeline_ver": 3 }
}
```

## 6. Algorithm, model and implementation design

### 6.1 Tier 1 - embedded preview (the speed weapon)

- `libraw_open_file` -> `libraw_unpack_thumb` -> memcpy the JPEG bytes; do not demosaic anything.
- Apply EXIF orientation losslessly; keep the original JPEG for the grid and derive a 512 px thumbnail with `zune-jpeg`/`libjpeg-turbo` scaled decode.
- Files with no embedded preview (rare, some medium format and DNG) fall back to a fast quarter-size demosaic and are tagged so QA can see the difference.
- Batch across cores with work stealing; expect 15-40 files/s/core, so 4,000 files land in 2-3 minutes on 8 cores.

### 6.2 Tier 2 - the AI proxy (quality matters more than speed)

- Half-size demosaic (`half_size=1`) is enough for 2048 px and is 3-5x faster than full AHD.
- Pipeline: black/white level -> as-shot neutral -> camera matrix -> linear Rec.2020 -> optional highlight reconstruction -> neutral filmic-lite curve -> sRGB 8-bit; the 16-bit linear buffer is written before the curve.
- Disable all camera-baked looks (no camera picture profile, no LibRaw auto brightness) so the AI sees a consistent, documented rendering across every brand. This is exactly what makes cross-camera matching possible in Phase 26.
- Record `as_shot_neutral`, black/white levels and clipping statistics in the sidecar; Phases 15-16 need them.

### 6.3 Tier 3 - full decode

- Full demosaic (AHD by default, DCB for high-detail scenes) with tiled output (512 px tiles + 32 px halo) so 60 MP files never blow the memory ceiling.
- Full decodes are never speculative: only the culled survivors that reach final render get one.
- Deterministic: the tile decomposition must not change results; a test compares tiled vs whole-image output.

### 6.4 Cache and scheduling

- Key = `content_hash` + `pipeline_ver`; bumping the pipeline version invalidates cleanly without deleting anything by hand.
- Priority classes: `Visible` (user is looking at it) > `Interactive` > `AiBatch` > `Background`; visible requests preempt batch work within 50 ms.
- Budgeted cache with default 40 GB, LRU eviction, and a settings UI showing 'previews use X GB of Y'.
- Cache lives per project with a global shared store option for photographers on a single fast SSD.

### 6.5 Colour correctness

- Prefer embedded DNG colour matrices; otherwise use the bundled camera-profile table; otherwise fall back to Adobe-style matrices with a `profile=generic` flag surfaced in QA.
- Golden tests use a photographed ColorChecker per camera body; mean dE2000 <= 2.0 for the 24 patches on the proxy path.
- COL signs off every camera profile addition; unsigned profiles cannot ship.

## 7. Cloud AI usage (bring-your-own API key)

No cloud AI call in this phase. The phase must work with the network cable unplugged; the Cloud AI Gateway from Phase 04 stays idle here.

## 8. Implementation order (execute literally, in this order)

1. Vendor and build LibRaw for the three platforms; write the FFI layer with `unsafe` isolated into one module and a fuzz test.
2. Implement Tier 1 with orientation, thumbnail derivation and quarantine handling; benchmark it.
3. Write ADR-0002 for the colour pipeline; get COL sign-off before implementing Tier 2.
4. Implement Tier 2 with both 8-bit sRGB and 16-bit linear outputs and the sidecar metadata.
5. Implement the content-addressed cache with LRU, budget and stats.
6. Implement the priority-based preview service and wire it into the Phase 01 grid.
7. Implement Tier 3 tiled full decode with a tiled-vs-whole equivalence test.
8. Add ColorChecker golden tests for at least 8 camera bodies.
9. Profile and tune worker counts, thread affinity and IO readahead; publish the benchmark table.
10. Write the exit report and the 'preview troubleshooting' runbook.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `TLC` | Tech Lead - Imaging Core (Rust) | Design the `PreviewService` trait, pixel buffer types and priority model; review all FFI code | Frozen preview API | 3 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | LibRaw FFI, Tier 1 extraction, watchdog, quarantine, orientation handling | `aura-raw` tier 1 + fuzz tests | 5 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Tier 3 tiled full decode with equivalence tests | Full decode API | 3 d |
| `SRG` | Senior Engineer - GPU & Render (Rust / wgpu / CUDA) | GPU-assisted resize/curve for proxy generation; fall back to SIMD CPU path | Proxy fast path | 4 d |
| `COL` | Colour Scientist | Define working space, transfer function, camera matrices, ColorChecker methodology; sign ADR-0002 | ADR-0002 + profile table | 4 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Content-addressed cache, LRU, budget accounting, stats API | `aura-cache` + tests | 3 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Thumbnail store, progressive tier upgrade in the grid, cancel-on-scroll, cache settings UI | Real pixels in the grid | 3 d |
| `PERF` | Performance Engineer | Benchmark all tiers across formats and machines; tune worker pool and IO | Benchmark report + tuned defaults | 4 d |
| `QAL` | QA Lead - Automation | Golden proxy suite, tiled/whole equivalence, poison-file corpus, cache eviction tests | CI gates | 4 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Visual audit of proxies vs camera JPEG and vs Lightroom neutral for 8 bodies | Audit report | 3 d |
| `MLL` | ML Lead - Vision | Confirm the proxy rendering is what models will be trained on; freeze `pipeline_ver` for datasets | Signed training-input spec | 1 d |
| `DEVOPS` | DevOps / Release Engineer | Cross-platform LibRaw build in CI, artefact caching, licence compliance report | Reproducible builds | 3 d |
| `SEC` | Security & Privacy Engineer | Fuzz the decode path, sandbox the decoder, cap memory per decode | Fuzz corpus + limits | 2 d |
| `DOC` | Technical Writer | `docs/runbooks/previews.md`, camera-support matrix page | Docs merged | 1 d |

### 9.1 Handoff chain for this phase

```text
COL (ADR-0002 colour) --> SRC/SRG (tier 1/2/3) --> SRC (cache) --> SFE (grid pixels)
                                    |                            |
                                    v                            v
                            PERF (tuning)                 QAL/QAIQ (golden + visual)
                                    \____________ MLL signs training-input spec ____________/
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

- Tier 1 on 4,000 files: <= 3 min, zero crashes, quarantine list correct for deliberately corrupted files.
- Proxy dE2000 vs blessed golden <= 0.5 mean; ColorChecker dE2000 <= 2.0 mean per body.
- Tiled full decode == whole-image decode bit-for-bit.
- Cache: budget respected, LRU evicts oldest, `pipeline_ver` bump invalidates, corrupted cache entry self-heals.
- Priority: a visible request during a 4,000-image AI prefetch is served in <= 50 ms.
- Memory: peak RSS <= 2.5 GB while proxying 4,000 files; no leak across 20,000 decodes (valgrind/heaptrack).
- Formats: CR2, CR3, ARW, NEF, NRW, RAF, ORF, RW2, DNG, HEIF and JPEG all decode or fail loudly.

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
| Tier 1, 4,000 files, 8 cores | <= 180 s |
| Tier 2 proxy per image | <= 120 ms (GPU-assisted) / <= 250 ms (CPU only) |
| Tier 3 full decode, 45 MP | <= 1.8 s |
| Cached preview read | <= 8 ms |
| Peak RSS during batch proxying | <= 2.5 GB |
| Cache size per 1,000 images | <= 3.5 GB at default settings |

Telemetry events (local-first, opt-in aggregation):

- `preview.decoded` {tier, ms, source, camera_model, ok}
- `preview.quarantined` {reason, camera_model}
- `cache.stats` {bytes_used, hit_rate, evictions}
- `colour.profile_missing` {make, model}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| LibRaw hangs or crashes on an exotic file | Watchdog + per-file timeout + quarantine + fuzz corpus in CI. |
| Embedded previews differ from the neutral render and confuse models | Train models on Tier 2 proxies only; Tier 1 is used for triage and UI, and the tier is recorded with every score. |
| Cache explodes and fills the disk | Hard budget, LRU eviction, low-disk warning, one-click purge. |
| New camera launches with unsupported RAW | Monthly LibRaw bump job, `unknown_camera` telemetry, generic-matrix fallback marked in the UI. |
| Colour drift after a pipeline change silently invalidates trained models | `pipeline_ver` is part of the dataset key; changing it requires MLL sign-off and a model re-validation run. |

## 13. Acceptance criteria

- [ ] Opening a freshly ingested 4,000-image wedding shows real thumbnails filling in within 3 minutes, scrolling never blocks.
- [ ] Every image can produce a 2048 px proxy with both 8-bit sRGB and 16-bit linear buffers.
- [ ] Colour: ColorChecker mean dE2000 <= 2.0 on all 8 tested bodies; profiles are documented and signed by COL.
- [ ] Full-resolution tiled decode matches whole-image decode exactly and stays within the memory ceiling on a 60 MP file.
- [ ] Cache respects its budget, survives corruption, and reports hit rate in settings.
- [ ] No decode failure can crash or hang the app; failures land in the Problems list.
- [ ] `just phase-02-verify` passes golden proxy diffs on all fixture weddings.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 02 - RAW Decode Engine & Three-Tier Preview Pyramid.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-02-RAW-DECODE-PREVIEW-PYRAMID.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - Instant, colour-correct previews for every RAW: embedded JPEG for triage, 2048 px proxy for AI, and on-demand full-resolution decode for final render.

Rules:
  - Do not start Phase 3. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-raw/src/{lib,ffi,libraw_sys,thumb,proxy,full,orientation,timeout}.rs`, `crates/aura-raw/src/colour/{matrix,profile,working_space,curve}.rs`, `crates/aura-cache/src/{lib,store,lru,budget,paths}.rs`, `crates/aura-preview/src/{lib,service,pool,request,priority}.rs`, `crates/aura-preview/benches/decode.rs`, `apps/desktop/src/stores/thumbnailStore.ts`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-02-raw-decode-preview-pyramid and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-02.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-02-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-02-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-02-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 02 of 30 - RAW Decode Engine & Three-Tier Preview Pyramid - part of the AURA Wedding AI master build plan.*

---

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

---

# Phase 04 - Cloud AI Gateway & Agentic Reasoning Runtime (bring-your-own key)

> **Single feature shipped by this phase:** Paste one AI API key and the app gains a governed reasoning layer: VLM/LLM calls with tool-calling, strict JSON contracts, caching, budget caps, redaction and a full audit trail.
>
> **Mission:** Turn the user's API key into an auditable, offline-tolerant reasoning capability that every later agent (Culling, QC, Album Story, Explain, Learning Loop) calls through one door - and that never becomes a hard dependency.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 04 of 30 |
| Epic | E1 - Foundation |
| Feature | Paste one AI API key and the app gains a governed reasoning layer: VLM/LLM calls with tool-calling, strict JSON contracts, caching, budget caps, redaction and a full audit trail. |
| Depends on | Phases 01-03 |
| Unlocks | Phases 07, 10, 12, 13, 24, 27, 28, 29, 30 |
| Duration | 2 weeks |
| Primary owners | AI Agent & Prompt Engineer, Chief Architect / CTO Agent, Security & Privacy Engineer, Mid-Level Backend / Cloud Engineer |
| Risk level | High - cost, privacy and non-determinism all live here |
| Headline KPI | 100 % of responses schema-valid after one retry; cache hit rate >= 70 % on a re-run; cost per 3,000-image wedding <= USD 1.50 at default settings |
| Competitor being beaten | No competitor exposes a user-owned reasoning layer; this is a category-defining differentiator |

## 1. Why this phase exists

The user supplies an AI API key, so the product should extract maximum value from it - but a photo pipeline cannot be at the mercy of a rate limit or a flaky network. The gateway makes cloud reasoning a *bonus tier*: it upgrades decisions when available and degrades silently to local models when not.

Reasoning is only useful if it is structured. Free-text answers cannot drive a pipeline, so every call is a typed contract: JSON schema in, JSON schema out, validated, versioned, cached and logged. A malformed response is a handled error, never a corrupted gallery.

Weddings are private. The gateway is the single place where redaction, consent, per-project opt-in, region pinning and 'never upload originals' are enforced, so no later phase can accidentally leak a client's faces.

## 2. Scope contract

### 2.1 In scope

- `aura-cloud`: provider-agnostic client (Anthropic / OpenAI / Google / OpenAI-compatible endpoints) with model aliasing so prompts do not hard-code vendors.
- Secure key storage in the OS keychain (DPAPI / Keychain / libsecret), never in the catalog or logs; key validation and quota probe on entry.
- Typed task registry: every cloud task is a versioned `CloudTask` with input schema, output schema, prompt template, max tokens, temperature 0 default and a local fallback function.
- Strict JSON enforcement: schema validation, one repair retry with the validator error appended, then deterministic local fallback.
- Multimodal payload builder: downscaled proxy crops (default 768 px long edge), tiled context sheets (contact sheets of up to 12 thumbnails), EXIF summaries, never RAW files.
- Cost governor: per-project and per-month caps, live spend meter, per-task cost accounting, automatic downgrade to cheaper model tiers, hard stop with resumable state.
- Response cache keyed by (task, task_version, prompt_hash, image_content_hashes, model) with an on-disk SQLite store so re-running a wedding is nearly free.
- Audit trail: every call stored with prompt hash, tokens, latency, cost, model, decision id, so any AI decision in the product can be traced to its evidence.
- Privacy controls: project-level 'cloud AI off', face-blur-before-upload option, region/endpoint pinning, retention statement surfaced in the UI.
- Agent loop primitives: tool registry, bounded step count, deterministic tool ordering, structured scratchpad, timeout and cancellation.

### 2.2 Explicitly out of scope (do not build it here)

- Any specific reasoning feature (those live in Phases 07, 12, 13, 27, 29).
- Cloud GPU rendering or generative inpainting infrastructure (Phase 24 and 30).
- Fine-tuning or training against the user's key (never; training happens in `ml/`).
- Telemetry upload of image content (forbidden by the threat model).

## 3. Architecture and data flow

```text
caller (any later phase)
        |  CloudTask::<Name>{ typed input }
        v
  +-------------------- CloudAiGateway ---------------------+
  | 1 policy check (project opt-in, budget, privacy mode)   |
  | 2 cache lookup (task+version+content hashes+model)      |
  | 3 payload build (proxy crops <=768px, contact sheets)   |
  | 4 redaction (optional face blur, strip GPS/names)       |
  | 5 provider call (retry, backoff, timeout, cancel)       |
  | 6 JSON schema validate -> repair retry -> fallback      |
  | 7 cost accounting + audit row + cache write             |
  +------------------------+--------------------------------+
                           v
         typed output  OR  local fallback result (flagged source='local')
```

- Every result carries `source: 'cloud' | 'cache' | 'local_fallback'` plus `confidence`, so the QC agent and the Explain panel can always say where a decision came from.
- Temperature is 0 and prompts are hash-pinned, which makes cloud decisions reproducible enough for golden tests; the cache makes them fully reproducible in CI.
- The gateway is the only crate allowed to make outbound network calls; a CI lint enforces this.

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-cloud/src/{lib,gateway,provider,anthropic,openai,google,compat}.rs | Provider abstraction and HTTP clients. |
| `crates/aura-cloud/src/{tasks,schema,validate,repair,fallback}.rs` | Task registry, JSON schema validation and repair loop. |
| `crates/aura-cloud/src/{payload,redact,budget,cache,audit,keys}.rs` | Payload building, redaction, cost governor, cache, audit log, keychain. |
| `crates/aura-cloud/src/agent/{loop,tools,scratchpad,limits}.rs` | Bounded agent loop primitives reused by the QC and Album agents. |
| `crates/aura-catalog/migrations/0004_cloud_audit.sql` | `cloud_calls`, `cloud_cache`, `cloud_budget` tables. |
| `apps/desktop/src/routes/settings/AiKeys.tsx` | Key entry, provider choice, budget caps, privacy switches, live spend meter. |
| `docs/adr/ADR-0004-cloud-ai-policy.md` | Privacy, budget, determinism and fallback policy. |
| `tests/cloud/cassettes/*.json` | Recorded provider responses so CI never touches the network. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Cloud task contract (frozen)**

```rust
pub trait CloudTask: Send + Sync {
    const NAME: &'static str;
    const VERSION: u16;
    type Input: Serialize + Hash;
    type Output: DeserializeOwned + Validate;
    fn prompt(&self, input: &Self::Input) -> PromptSpec;      // system + user + images
    fn output_schema(&self) -> &'static str;                  // JSON Schema
    fn local_fallback(&self, input: &Self::Input) -> Result<Self::Output, AuraError>;
    fn max_cost_usd(&self) -> f32 { 0.02 }
}

pub struct CloudResult<T> {
    pub value: T,
    pub source: Source,          // Cloud | Cache | LocalFallback
    pub confidence: f32,
    pub model: String,
    pub tokens_in: u32, pub tokens_out: u32, pub cost_usd: f32,
    pub call_id: Uuid,
}
```

**Audit and cache tables**

```sql
CREATE TABLE cloud_calls (
  id TEXT PRIMARY KEY, project_id TEXT, task TEXT NOT NULL, task_version INTEGER NOT NULL,
  model TEXT NOT NULL, prompt_hash TEXT NOT NULL, image_hashes TEXT,
  tokens_in INTEGER, tokens_out INTEGER, cost_usd REAL,
  latency_ms INTEGER, status TEXT, retry_count INTEGER,
  decision_ref TEXT, created_at TEXT NOT NULL
);
CREATE TABLE cloud_cache (
  key TEXT PRIMARY KEY, task TEXT, response_json TEXT NOT NULL,
  created_at TEXT NOT NULL, hits INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE cloud_budget (
  project_id TEXT PRIMARY KEY, cap_usd REAL, spent_usd REAL NOT NULL DEFAULT 0,
  month TEXT, hard_stop INTEGER NOT NULL DEFAULT 1
);
```

## 6. Algorithm, model and implementation design

### 6.1 Where cloud reasoning is actually worth the money

- Scene naming and ritual identification for culturally specific weddings (Hindu, Nepali, Muslim, Christian) where a local classifier is uncertain - one contact-sheet call per timeline segment, not per image.
- Tie-breaking inside a burst when local scores are within noise (Phase 12): send 3-6 thumbnails and ask for a ranked choice with reasons.
- Natural-language explanations (Phase 13) generated from structured local evidence, so the reasoning is grounded, not invented.
- QC triage narrative and remediation planning (Phase 27) and album sequencing/captioning (Phase 29).
- Rule: never send more than ~1 call per 40 images by default; the pipeline must be able to run with `cloud=off` and lose no capability, only polish.

### 6.2 Determinism, validation and repair

- Temperature 0, top_p 1, fixed system prompt, sorted JSON keys, and a prompt hash committed in the audit row.
- Validate with a JSON Schema validator; on failure send exactly one repair message containing the validator error and the original response, then fall back locally.
- Any field the schema does not define is dropped; unknown enum values map to `unknown` and lower the confidence by 0.2.
- Cloud confidence is calibrated in Phase 13 against outcomes, so it is comparable to local model confidence.

### 6.3 Cost governor mechanics

- Estimate cost before calling from token/image counts; refuse or downgrade if the estimate exceeds the remaining budget.
- Three model tiers per provider (`reasoning`, `balanced`, `cheap`); tasks declare a minimum tier and the governor picks the cheapest acceptable one.
- Spend meter in the UI: 'this wedding has used $0.42 of your $5 cap'; on hard stop, the pipeline continues with local fallbacks and records which decisions were downgraded.
- Batching: contact sheets and multi-item questions collapse dozens of decisions into one call.

### 6.4 Privacy and security

- Keys live only in the OS keychain; logs redact anything key-shaped; a unit test greps artefacts for key patterns.
- Uploads are downscaled derivatives only - never the RAW, never the full-resolution export.
- Optional pre-upload face blur for extra-sensitive clients; GPS and client names stripped from payloads by default.
- Per-project switch plus a global 'offline studio mode' that disables the crate entirely; SEC signs off the payload builder.

## 7. Cloud AI usage (bring-your-own API key)

**Reference task implemented in this phase: `SegmentNaming` (proves the whole contract end to end)**

| Aspect | Specification |
|---|---|
| Model class | Vision-capable reasoning tier (e.g. Claude/GPT/Gemini class VLM), temperature 0 |
| Trigger | One call per timeline segment whose local scene confidence < 0.75 |
| Input sent | Contact sheet of up to 12 thumbnails (768 px each), segment start/end times, local top-3 scene guesses with scores, camera/flash summary |
| Cost control | Max 1 call per 40 images; cached by content hashes; downgrade to `balanced` tier when budget < 30 % remains |
| Offline fallback | Local scene classifier argmax from Phase 07 with `source='local_fallback'` |

System prompt contract:

```text
You are a wedding post-production analyst. You will receive a contact sheet of consecutive photographs from one wedding, a time range, and a local classifier's top guesses.
Task: name the wedding scene, name the specific ritual or activity if present, and state which cultural tradition the visual evidence supports.
Rules:
- Judge only from visible evidence. If evidence is weak, say so with low confidence rather than guessing.
- Use the controlled vocabulary supplied in `allowed_scenes` and `allowed_rituals`. If nothing fits, return "other" and describe it in `notes`.
- Never infer names, ethnicity or religion of individuals; describe the ceremony type only.
- Return ONLY JSON matching the provided schema. No prose, no markdown.
```

Required JSON response schema (validated; invalid = retry once, then fall back to local model):

```json
{
  "type": "object",
  "required": ["scene", "confidence", "reasons"],
  "properties": {
    "scene": { "type": "string" },
    "ritual": { "type": ["string", "null"] },
    "tradition": { "type": ["string", "null"] },
    "confidence": { "type": "number", "minimum": 0, "maximum": 1 },
    "reasons": { "type": "array", "items": { "type": "string" }, "maxItems": 5 },
    "boundary_hint": { "type": ["string", "null"], "description": "index where the scene appears to change" },
    "notes": { "type": ["string", "null"] }
  },
  "additionalProperties": false
}
```

## 8. Implementation order (execute literally, in this order)

1. Write ADR-0004 covering privacy, budget, determinism and the 'cloud is never required' rule; SEC and CTO co-sign.
2. Implement keychain storage, key validation and the provider abstraction with one provider first.
3. Implement the `CloudTask` trait, schema validation, repair retry and local-fallback dispatch.
4. Implement the payload builder (crops, contact sheets, EXIF summary) with hard size limits and redaction.
5. Implement cache + audit tables and wire the cost governor with pre-call estimation.
6. Implement the bounded agent loop primitives (tools, scratchpad, step cap, timeout, cancel).
7. Ship `SegmentNaming` as the reference task with cassette-based tests.
8. Build the Settings > AI Keys UI: provider, key, caps, privacy switches, spend meter, audit viewer.
9. Add the CI lint that forbids network calls outside `aura-cloud` and the key-leak grep test.
10. Add the second and third provider behind the same aliases and verify identical schema behaviour.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `CTO` | Chief Architect / CTO Agent | Co-sign ADR-0004; enforce 'gateway is the only network door' in architecture lints | ADR + lint | 1 d |
| `AGT` | AI Agent & Prompt Engineer | Design the task registry, prompt-template system, repair loop, confidence mapping and the agent loop | `tasks`/`agent` modules | 6 d |
| `AGT` | AI Agent & Prompt Engineer | Implement `SegmentNaming` reference task with prompt versioning and cassettes | Reference task + tests | 2 d |
| `MBE` | Mid-Level Backend / Cloud Engineer | Provider clients, retry/backoff, streaming off, timeouts, rate-limit handling, model aliasing | `provider` layer | 5 d |
| `SEC` | Security & Privacy Engineer | Keychain integration, redaction rules, payload review, key-leak tests, threat-model update | Security sign-off | 4 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Cache + audit persistence, budget tables, migration 0004, cancellation plumbing | Storage layer | 3 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Settings > AI Keys, spend meter, audit viewer, privacy switches, offline-studio mode | UI shipped | 3 d |
| `MFE` | Mid-Level Frontend Engineer | Per-project cloud toggle, budget dialogs, downgrade notices, error toasts with plain language | UI polish | 2 d |
| `QAL` | QA Lead - Automation | Cassette harness, schema-violation fixtures, budget-exhaustion tests, offline tests, retry tests | CI gates | 4 d |
| `PM` | Product Manager Agent | Define which decisions may use cloud, default caps, and the user-facing privacy copy | Policy doc + copy | 2 d |
| `MLL` | ML Lead - Vision | Define how cloud confidence merges with local model confidence; guard against cloud overriding strong local evidence | Fusion rule spec | 1 d |
| `DOC` | Technical Writer | Write 'Using your own AI key', privacy FAQ, cost guide and the audit-trail explainer | Docs merged | 2 d |

### 9.1 Handoff chain for this phase

```text
PM policy + SEC threat model -> ADR-0004 (CTO)
            |
            v
  AGT (tasks, prompts, agent loop) <---> MBE (providers)
            |                                |
            v                                v
  SRC (cache/audit/budget)            SFE/MFE (settings, meter)
            \________ QAL (cassettes, offline, budget tests) ________/ -> CTO/PM gate
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

- Offline: with the network disabled every cloud task returns a local fallback and the pipeline completes.
- Schema violation: malformed, truncated and extra-field responses trigger exactly one repair, then fallback; never a panic.
- Budget: a project at its cap stops calling, records downgrades, and still produces a complete gallery.
- Cache: re-running the same wedding produces >= 70 % cache hits and identical decisions.
- Privacy: payload inspector test asserts no RAW bytes, no GPS, no filenames with client names, and blur applied when enabled.
- Key safety: logs, crash dumps and telemetry artefacts contain no key-shaped strings.
- Provider swap: the same task on three providers yields schema-valid output and comparable decisions on the fixture set.

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
| Gateway overhead excluding provider latency | <= 15 ms per call |
| Cloud calls per 3,000-image wedding (default) | <= 75 |
| Cost per 3,000-image wedding (default settings) | <= USD 1.50 |
| Cache hit rate on re-run | >= 70 % |
| Failure impact on total pipeline time | <= 3 % when all cloud calls fail |

Telemetry events (local-first, opt-in aggregation):

- `cloud.call` {task, task_version, model, tokens_in, tokens_out, cost_usd, latency_ms, status, retries}
- `cloud.fallback` {task, reason}
- `cloud.budget_stop` {project, cap_usd, spent_usd}
- `cloud.cache` {hit_rate, entries, bytes}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Provider outage or rate limits stall a wedding | Bounded retries, circuit breaker, immediate local fallback, and no pipeline stage may block on cloud results. |
| Runaway cost | Pre-call estimation, hard caps, batching, tier downgrade, and per-task cost ceilings. |
| Prompt drift changes decisions between releases | Prompt hashing, task versioning, cassette golden tests, and a changelog entry required for any prompt edit. |
| Privacy incident | Derivative-only uploads, optional face blur, region pinning, audit log, SEC sign-off on the payload builder, and offline-studio mode. |
| Cloud reasoning overrides better local evidence | Fusion rule: cloud may not override a local decision with confidence >= 0.9 unless it supplies contradicting visual evidence, and the conflict is logged. |

## 13. Acceptance criteria

- [ ] A user pastes a key, sees it validated, sets a budget cap, and can immediately see spend after running a wedding.
- [ ] Every cloud task has a schema, a version, a prompt hash, a cost ceiling and a working local fallback.
- [ ] With the network unplugged, a full wedding completes end to end with decisions marked `local_fallback`.
- [ ] The audit viewer can trace any AI decision to its call, model, tokens, cost and evidence.
- [ ] No RAW or full-resolution pixels ever leave the machine; verified by an automated payload test.
- [ ] Turning on 'offline studio mode' makes the crate inert and the UI honest about what is disabled.
- [ ] CI contains no network access and still fully tests the gateway via cassettes.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 04 - Cloud AI Gateway & Agentic Reasoning Runtime (bring-your-own key).

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-04-CLOUD-AI-GATEWAY.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - Paste one AI API key and the app gains a governed reasoning layer: VLM/LLM calls with tool-calling, strict JSON contracts, caching, budget caps, redaction and a full audit trail.

Rules:
  - Do not start Phase 5. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-cloud/src/{lib,gateway,provider,anthropic,openai,google,compat}.rs, `crates/aura-cloud/src/{tasks,schema,validate,repair,fallback}.rs`, `crates/aura-cloud/src/{payload,redact,budget,cache,audit,keys}.rs`, `crates/aura-cloud/src/agent/{loop,tools,scratchpad,limits}.rs`, `crates/aura-catalog/migrations/0004_cloud_audit.sql`, `apps/desktop/src/routes/settings/AiKeys.tsx`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-04-cloud-ai-gateway and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-04.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-04-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-04-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-04-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 04 of 30 - Cloud AI Gateway & Agentic Reasoning Runtime (bring-your-own key) - part of the AURA Wedding AI master build plan.*

---

# Phase 05 - Perceptual Embeddings & Wedding Similarity Index

> **Single feature shipped by this phase:** Every image gets a compact perceptual embedding plus a fast similarity index, so the app can answer 'what looks like this?' across 4,000 photos in milliseconds.
>
> **Mission:** Create the shared vector substrate that scene clustering, burst grouping, duplicate detection, people grouping, reference-frame selection and consistency checks all reuse - computed once, in one pass.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 05 of 30 |
| Epic | E2 - Wedding Brain |
| Feature | Every image gets a compact perceptual embedding plus a fast similarity index, so the app can answer 'what looks like this?' across 4,000 photos in milliseconds. |
| Depends on | Phases 01-03 |
| Unlocks | Phases 06, 07, 08, 12, 25, 26, 29 |
| Duration | 1.5 weeks |
| Primary owners | ML Lead - Vision, Senior ML Engineer, Senior Engineer - Core Pipeline (Rust) |
| Risk level | Medium |
| Headline KPI | 4,000 embeddings in <= 150 s on RTX 4070; kNN query <= 5 ms; duplicate recall >= 0.98 at precision >= 0.95 |
| Competitor being beaten | Narrative Select scene grouping; Aftershoot burst grouping |

## 1. Why this phase exists

Almost every wedding-intelligence question is a similarity question. Computing one good embedding per image and reusing it everywhere is the difference between an 8-minute pipeline and an hour-long one.

Generic CLIP-style embeddings are decent but not wedding-aware. A light domain adaptation head trained on wedding scenes makes clustering dramatically cleaner, especially for dark dance floors and ritual close-ups that generic models confuse.

## 2. Scope contract

### 2.1 In scope

- Global embedding model (ViT-B/16-class backbone, 512-d output, fp16) run on Tier-1/Tier-2 previews at 384 px.
- Wedding domain-adaptation head trained with supervised contrastive loss on scene + ritual labels; exported as one fused ONNX graph.
- Auxiliary cheap descriptors computed in the same pass: 64-bit dHash, 8x8x8 HSV colour histogram, edge-energy map summary, mean/percentile luminance, dominant-colour palette.
- Vector store in SQLite (`embeddings` table, fp16 blobs) plus an in-memory HNSW index built at project open with a persisted graph snapshot.
- Query API: kNN, radius search, time-windowed kNN (only frames within +/- N seconds), camera-filtered kNN, and centroid/medoid computation for any set.
- Incremental updates: newly imported images are embedded and inserted without a full rebuild.
- Evaluation harness: retrieval mAP, duplicate detection PR curve, cluster purity/NMI against labelled fixture weddings.

### 2.2 Explicitly out of scope (do not build it here)

- Face embeddings (Phase 06 - a different model and a different index).
- Scene naming and timeline segmentation (Phase 07).
- Burst logic and duplicate policy decisions (Phase 08 consumes this index).
- Aesthetic or quality scoring (Phases 09-11).

## 3. Architecture and data flow

```text
Tier1/Tier2 preview --> resize 384 --> [ backbone ViT ] --> 768-d --> [ wedding head ] --> 512-d fp16
                              |                                                        |
                              +--> dHash64, HSV hist, edge energy, luma stats ---------+
                                                        |
                                                        v
                                        embeddings table (SQLite, fp16 blob)
                                                        |
                          HNSW index (M=32, ef=200) built at open, snapshot cached
                                                        |
     +--------------------+----------------------+------+-----------------+
     v                    v                      v                        v
  bursts (P08)      scene clusters (P07)   duplicate pairs (P08)   reference frames (P25/26)
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-vision/src/embed/{lib,model,batch,descriptors,hash}.rs` | Embedding runner, batching, cheap descriptors, dHash. |
| `crates/aura-index/src/{lib,hnsw,store,snapshot,query,metrics}.rs` | Vector store, HNSW index, persisted snapshot, query API. |
| `crates/aura-catalog/migrations/0005_embeddings.sql` | `embeddings`, `descriptors` tables + indices. |
| `ml/models/embed/{dataset.py,train_contrastive.py,eval_retrieval.py,export.py}` | Training and evaluation of the wedding head. |
| `docs/model-cards/wedding_embedding.md` | Data, metrics, failure modes, bias notes. |
| `tests/eval/embedding_eval.rs` | Cluster purity, duplicate PR, retrieval mAP gates. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Embedding and index API (frozen)**

```rust
pub struct ImageEmbedding {
    pub image_id: ImageId,
    pub vec: [f16; 512],
    pub dhash: u64,
    pub hsv_hist: [u8; 512],
    pub luma: LumaStats,        // mean, p1, p50, p99, clip_lo, clip_hi
    pub model_ver: u16,
}

pub trait SimilarityIndex: Send + Sync {
    fn knn(&self, q: &ImageEmbedding, k: usize, filter: IndexFilter) -> Vec<(ImageId, f32)>;
    fn radius(&self, q: &ImageEmbedding, max_dist: f32, filter: IndexFilter) -> Vec<(ImageId, f32)>;
    fn medoid(&self, ids: &[ImageId]) -> Option<ImageId>;
    fn insert(&self, e: &ImageEmbedding);
    fn stats(&self) -> IndexStats;
}

pub struct IndexFilter {
    pub time_window_s: Option<u32>,   // only frames within +/- N seconds
    pub camera_id: Option<CameraId>,
    pub scene: Option<SceneId>,
    pub exclude: Option<&'static [ImageId]>,
}
```

## 6. Algorithm, model and implementation design

### 6.1 Model and training

- Backbone: an open ViT-B/16 image encoder, frozen for the first two epochs, then the last four blocks unfrozen at 1/10 learning rate.
- Head: two-layer MLP 768 -> 1024 -> 512 with L2 normalisation, trained with supervised contrastive loss where positives are same-scene same-wedding frames within 20 s and hard negatives are different-scene frames from the same wedding.
- Augmentations must be photographic, not destructive: exposure +/- 1.5 EV, WB +/- 800 K, mild noise, JPEG artefacts - never flips of ritual scenes (handedness matters) and never heavy crops.
- Export fused (resize + normalise + backbone + head) so preprocessing can never drift between training and inference.

### 6.2 Cheap descriptors earn their keep

- dHash catches exact and near-exact duplicates in O(1) with Hamming distance, so the expensive index is never asked trivial questions.
- HSV histograms and luma percentiles are what Phases 25-26 use for gallery colour consistency and camera matching, so they are computed here once.
- Edge-energy summary gives a free global sharpness prior that Phase 09 refines with a proper model.

### 6.3 Index engineering

- HNSW with M=32, ef_construction=200, ef_search=64; cosine distance on L2-normalised fp16 vectors.
- Build in parallel at project open (4,000 vectors in < 400 ms), then persist a snapshot so subsequent opens are instant.
- Time-windowed queries are implemented as a pre-filter over a sorted timeline array, not as post-filtering, so burst queries stay under 1 ms.
- Deterministic tie-breaking by `timeline_ts` then `image_id` so clustering is reproducible.

### 6.4 Evaluation gates

- Cluster purity >= 0.85 and NMI >= 0.80 against human scene labels on the fixture weddings.
- Duplicate detection recall >= 0.98 at precision >= 0.95 using the labelled duplicate sets.
- Retrieval mAP@10 must beat the raw backbone by >= 8 points, otherwise the head is not worth shipping.

## 7. Cloud AI usage (bring-your-own API key)

No cloud AI call in this phase. The phase must work with the network cable unplugged; the Cloud AI Gateway from Phase 04 stays idle here.

## 8. Implementation order (execute literally, in this order)

1. Define the embedding contract and migration; land the table and the HNSW wrapper with synthetic vectors.
2. Wire the ONNX backbone through `InferService` and benchmark batch sizes 8/16/32/64.
3. Implement cheap descriptors in the same pass and store them.
4. Label scene/ritual data on the fixture weddings (DATA) and train the wedding head.
5. Run the evaluation harness; only ship the head if it clears the gates in 6.4.
6. Export the fused ONNX, verify parity across EPs, register it in `models.lock`.
7. Implement incremental insert and the persisted snapshot; add kill/resume tests.
8. Expose a debug UI: 'find similar' on any photo, with distances - invaluable for later phases.
9. Publish the model card and the benchmark table.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `MLL` | ML Lead - Vision | Own the embedding spec, loss design, evaluation gates and the model card | Signed spec + gates | 2 d |
| `MLR` | ML Research Engineer | Ablate backbone choices, positive/negative mining strategies and augmentation policy | Ablation report | 4 d |
| `SRML` | Senior ML Engineer | Train the head, export fused ONNX, verify EP parity, quantise the CPU variant | Model + parity report | 5 d |
| `DATA` | Data Engineer / Dataset Curator | Scene/ritual labels and duplicate ground truth on fixture weddings; enforce wedding-level splits | Labelled dataset v1 | 5 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Embedding runner, descriptor computation, storage, incremental insert, snapshot | `aura-vision::embed` + tests | 4 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | HNSW index, filters, medoid, deterministic ordering | `aura-index` + tests | 3 d |
| `PERF` | Performance Engineer | Throughput tuning, batch sizing, memory ceiling, index build benchmarks | Benchmark report | 2 d |
| `QAL` | QA Lead - Automation | Evaluation harness in CI, purity/NMI/PR gates, incremental-insert tests | CI gates | 3 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Debug 'find similar' panel with distance readout and cluster preview | Debug UI | 2 d |
| `MLOPS` | MLOps / Model Packaging Engineer | Register the model, dataset versioning, training reproducibility (seed, env lock) | Reproducible run | 2 d |
| `SEC` | Security & Privacy Engineer | Confirm embeddings are not reversible to recognisable images; document biometric implications | Privacy note | 1 d |

### 9.1 Handoff chain for this phase

```text
DATA labels -> MLR ablations -> SRML training -> MLOPS registry
                                        |
                                        v
                        SRC (runner + index) -> SFE debug UI
                                        |
                     QAL eval gates + PERF benchmarks -> MLL sign-off
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

- Embed 4,000 images: throughput, determinism (same input -> identical vector), and no drift after app restart.
- Duplicate detection PR curve meets the gate on labelled duplicate sets, including near-duplicates one stop apart.
- Cluster purity/NMI gates on all three fixture weddings.
- Time-windowed kNN returns only in-window frames and stays under 1 ms.
- Incremental insert after a second card import keeps recall within 1 % of a full rebuild.
- Dark dance-floor and backlit-ceremony subsets are not collapsed into one cluster (specific regression fixtures).

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
| 4,000 embeddings (RTX 4070, batch 32) | <= 150 s |
| 4,000 embeddings (M3 Pro) | <= 300 s |
| HNSW build for 4,000 vectors | <= 400 ms |
| kNN query (k=32) | <= 5 ms |
| Storage per image (vector + descriptors) | <= 1.6 KB |

Telemetry events (local-first, opt-in aggregation):

- `embed.batch` {count, ms, ep, batch_size}
- `index.build` {vectors, ms, snapshot_used}
- `index.query` {k, ms, filter_kind}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Domain head overfits to fixture weddings | Wedding-level train/val/test splits, cross-tradition holdout, and a gate that the head must also beat the backbone on unseen traditions. |
| Embedding version drift invalidates stored vectors | `model_ver` stored per row; a version bump triggers a background re-embed with progress UI, never a silent mismatch. |
| HNSW memory growth on huge projects | fp16 vectors, optional on-disk index for > 20,000 images, and a documented ceiling. |
| Dark scenes cluster badly | Luma-aware sampling in training plus dedicated dark-scene regression fixtures. |

## 13. Acceptance criteria

- [ ] Every image in a project has an embedding, dHash, colour histogram and luma stats after analysis.
- [ ] 'Find similar' returns visually correct neighbours in under 5 ms on a 4,000-image project.
- [ ] Duplicate and cluster evaluation gates pass in CI on all fixture weddings.
- [ ] Re-opening a project rebuilds or loads the index in under 400 ms.
- [ ] Importing another card embeds only the new files.
- [ ] The model card is published with metrics, failure modes and bias notes.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 05 - Perceptual Embeddings & Wedding Similarity Index.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-05-EMBEDDINGS-SIMILARITY-INDEX.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - Every image gets a compact perceptual embedding plus a fast similarity index, so the app can answer 'what looks like this?' across 4,000 photos in milliseconds.

Rules:
  - Do not start Phase 6. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-vision/src/embed/{lib,model,batch,descriptors,hash}.rs`, `crates/aura-index/src/{lib,hnsw,store,snapshot,query,metrics}.rs`, `crates/aura-catalog/migrations/0005_embeddings.sql`, `ml/models/embed/{dataset.py,train_contrastive.py,eval_retrieval.py,export.py}`, `docs/model-cards/wedding_embedding.md`, `tests/eval/embedding_eval.rs`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-05-embeddings-similarity-index and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-05.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-05-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-05-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-05-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 05 of 30 - Perceptual Embeddings & Wedding Similarity Index - part of the AURA Wedding AI master build plan.*

---

# Phase 06 - Face Detection, Recognition & People Intelligence

> **Single feature shipped by this phase:** The app learns who matters at this wedding: it finds every face, groups them into identities, and automatically ranks bride, groom, immediate family and VIPs by evidence rather than by guesswork.
>
> **Mission:** Give every later decision a subject hierarchy. Sharpness on the bride's face must outrank sharpness on a stranger's elbow, and no phase can reason about that without People Intelligence.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 06 of 30 |
| Epic | E2 - Wedding Brain |
| Feature | The app learns who matters at this wedding: it finds every face, groups them into identities, and automatically ranks bride, groom, immediate family and VIPs by evidence rather than by guesswork. |
| Depends on | Phases 02, 03, 05 |
| Unlocks | Phases 09-13, 18-22, 25, 27, 29 |
| Duration | 2.5 weeks |
| Primary owners | ML Lead - Vision, Senior ML Engineer, Senior Engineer - Core Pipeline (Rust), Security & Privacy Engineer |
| Risk level | High - accuracy and privacy both matter |
| Headline KPI | face detection recall >= 0.97 at IoU 0.5 on wedding fixtures; identity clustering F1 >= 0.93; couple identification accuracy >= 0.95 |
| Competitor being beaten | Aftershoot/Imagen face grouping; Narrative Select face focus |

## 1. Why this phase exists

Wedding photography is people photography with a strict hierarchy: the couple, then immediate family, then guests. Every competitor that treats faces as equal produces galleries where a sharp photo of a stranger beats a slightly soft photo of the bride's tears - which is the wrong answer.

Identity grouping also unlocks coverage guarantees ('every close family member appears at least three times'), consistency ('the bride's skin must render the same all night') and curation ('the hero shots must feature the couple').

Because this is biometric data, it must be strictly local, project-scoped, deletable and never uploaded - a privacy stance that is also a marketing advantage.

## 2. Scope contract

### 2.1 In scope

- Face detection with landmarks (5-point + 68-point optional) on Tier-1 previews, tuned for small faces in wide ceremony frames.
- Face quality gate: pose (yaw/pitch/roll), blur, occlusion, resolution - low-quality faces are detected but excluded from identity voting.
- Face recognition embeddings (512-d ArcFace-class), project-scoped index, and agglomerative clustering with a tuned threshold plus rank-order verification.
- Identity roles: automatic detection of `bride`, `groom`, `couple`, `family_close`, `family_extended`, `vip`, `guest`, `vendor`, `child`, `unknown` with confidence and evidence.
- Prominence scoring per image: face area, centrality, sharpness, gaze, occlusion, count of frames the identity appears in, and scene weighting.
- Body/person detection to associate faces with bodies for later masking, and to count people when faces are hidden.
- Person timeline: for each identity, when they appear, in which scenes, with which other identities (co-occurrence graph).
- UI: People panel with merge/split/rename, 'this is the bride' one-click assignment, and per-identity importance slider.
- Privacy: face data in a separate encrypted-at-rest store, per-project deletion, export/erase controls, never sent to the cloud unless the user explicitly enables blur-free cloud reasoning.

### 2.2 Explicitly out of scope (do not build it here)

- Eye-open/blink analysis (Phase 09) and expression scoring (Phase 10) - they consume these crops.
- Skin masks and retouching (Phases 18, 20-21).
- Cross-wedding identity persistence (explicitly never - identities are project-scoped by policy).

## 3. Architecture and data flow

```text
Tier1 preview --> FaceDetector (SCRFD-class) --> boxes + 5pt landmarks + quality
                                 |
                                 +--> align 112x112 --> FaceEmbedder (ArcFace-class) --> 512-d
                                 |                                          |
                                 +--> PersonDetector (body boxes) --> face<->body association
                                                                            v
                                                        project face index -> clustering -> identities
                                                                            |
                       role inference (evidence: co-occurrence, scene, attire, frequency, cloud hint)
                                                                            |
                       per-image prominence: area x centrality x sharpness x role weight
                                                                            |
                                        People panel (merge/split/rename/mark bride)
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-vision/src/face/{detect,align,embed,quality,cluster,roles,prominence,person}.rs` | Detection through role inference. |
| `crates/aura-catalog/migrations/0006_people.sql` | `faces`, `identities`, `identity_links`, `person_boxes`, `cooccurrence` tables. |
| `crates/aura-people/src/{lib,timeline,graph,importance,api}.rs` | Identity timeline, co-occurrence graph, importance model. |
| `ml/models/face/{train_quality.py,tune_cluster.py,eval_identity.py}` | Quality head training and clustering threshold tuning. |
| `apps/desktop/src/routes/people/{PeoplePanel,IdentityCard,MergeDialog}.tsx` | People management UI. |
| `docs/model-cards/{face_detect,face_embed,face_quality}.md` | Model cards including demographic performance analysis. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**People schema**

```sql
CREATE TABLE faces (
  id TEXT PRIMARY KEY, image_id TEXT NOT NULL, identity_id TEXT,
  x REAL, y REAL, w REAL, h REAL,               -- normalised
  yaw REAL, pitch REAL, roll REAL,
  det_score REAL, quality REAL, blur REAL, occlusion REAL,
  landmarks_json TEXT, embed BLOB,               -- 512-d fp16
  area_frac REAL, centrality REAL, sharpness REAL,
  model_ver INTEGER NOT NULL
);
CREATE INDEX idx_faces_image ON faces(image_id);
CREATE INDEX idx_faces_identity ON faces(identity_id);

CREATE TABLE identities (
  id TEXT PRIMARY KEY, project_id TEXT NOT NULL,
  label TEXT,                                    -- user-assigned name
  role TEXT NOT NULL DEFAULT 'unknown',          -- bride|groom|family_close|...
  role_confidence REAL, role_reasons TEXT,
  user_locked INTEGER NOT NULL DEFAULT 0,        -- user decisions are never overwritten
  importance REAL NOT NULL DEFAULT 0.5,
  face_count INTEGER, first_seen TEXT, last_seen TEXT,
  centroid BLOB
);
```

**Prominence and role API**

```rust
pub struct SubjectHierarchy {
    pub primary: Vec<IdentityId>,     // couple
    pub secondary: Vec<IdentityId>,   // close family, VIPs
    pub weights: HashMap<IdentityId, f32>,
}

pub struct ImageSubjects {
    pub faces: Vec<FaceRef>,
    pub dominant: Option<IdentityId>,
    pub prominence: HashMap<IdentityId, f32>,  // 0..1 within this frame
    pub subject_focus_score: f32,              // weighted sharpness of who matters
}

pub trait PeopleService {
    fn hierarchy(&self, project: ProjectId) -> SubjectHierarchy;
    fn subjects(&self, image: ImageId) -> ImageSubjects;
    fn merge(&self, a: IdentityId, b: IdentityId) -> Result<IdentityId, AuraError>;
    fn split(&self, id: IdentityId, faces: &[FaceId]) -> Result<IdentityId, AuraError>;
    fn set_role(&self, id: IdentityId, role: Role) -> Result<(), AuraError>; // sets user_locked
}
```

## 6. Algorithm, model and implementation design

### 6.1 Detection tuned for real weddings

- Multi-scale inference: full-frame pass at 640 px plus a 2x2 tiled pass at 640 px when the frame is wide-angle and faces are small, then NMS across passes - this is what recovers guests in wide ceremony shots.
- Quality head predicts usability (pose, blur, occlusion, pixel height); faces under 48 px or quality < 0.4 are stored but excluded from identity voting.
- Person/body detection covers back-of-head and hair-covered cases so later masks and people counts still work.

### 6.2 Identity clustering that survives 12 hours of changing light

- Cluster with average-linkage agglomerative clustering on cosine distance, threshold tuned per project by a small validation sweep using high-quality faces only.
- Two-pass strategy: build a skeleton from high-quality faces, then assign medium-quality faces by nearest centroid with a stricter margin, leaving ambiguous faces unassigned rather than wrong.
- Rank-order verification (mutual nearest neighbour) prevents chain-merging two similar-looking relatives.
- Outfit-change robustness: no clothing features in the identity signal; hairstyle changes handled by using multiple centroids per identity (sub-clusters) when internal variance is high.

### 6.3 Role inference from evidence, not stereotypes

- Bride/groom candidates: identities with the highest frequency across `getting_ready`, `ceremony`, `couple_portrait` scenes, highest average face area, and highest mutual co-occurrence with each other.
- The couple is the identity pair maximising (co-occurrence in `couple_portrait` + ceremony centrality + total frequency); labels `bride`/`groom` are only suggestions until the user confirms, and the app is explicit that it may be a same-sex couple - the pair is what matters, not gender.
- Close family: high co-occurrence with the couple in `family` scenes plus presence in `getting_ready`.
- Vendors: appear across scenes but rarely as a photographic subject (low face area, low centrality, often at frame edges).
- Optional cloud hint (Phase 04) may raise confidence but can never override a `user_locked` identity.

### 6.4 Prominence scoring

- `prominence = w1*sqrt(area_frac) + w2*centrality + w3*sharpness + w4*gaze_to_camera + w5*role_weight`, with scene-specific weights (in `dance`, area matters less; in `family_portrait`, centrality matters more).
- `subject_focus_score` for an image is the prominence-weighted sharpness of the faces present - the single number Phases 09 and 12 use instead of naive global sharpness.
- All weights live in a versioned config file so QA can tune without recompiling, and every score records the config version.

### 6.5 Privacy engineering

- Face embeddings and crops live in a per-project store encrypted with a key in the OS keychain; deleting a project shreds them.
- No cross-project identity linking, ever - enforced by schema (identities are project-scoped) and by a test.
- 'Erase biometric data' button removes faces/identities while keeping culling and edit decisions intact.

## 7. Cloud AI usage (bring-your-own API key)

**Optional couple/role hint when local evidence is ambiguous**

| Aspect | Specification |
|---|---|
| Model class | Vision reasoning tier, temperature 0 |
| Trigger | Only when top-2 couple candidates are within 0.05 score and the user has enabled cloud reasoning for people |
| Input sent | Up to 8 blurred-background thumbnails (faces visible only if the user allows), scene labels, co-occurrence statistics as text |
| Cost control | Max 2 calls per project; cached |
| Offline fallback | Local evidence argmax, flagged low confidence, user prompted to confirm in the People panel |

System prompt contract:

```text
You are helping identify which two people are the wedding couple from statistical evidence and sample frames.
Rules:
- Base the answer only on photographic role evidence: who appears in getting-ready, who is central during the ceremony, who is photographed together in portraits.
- Do not infer gender, ethnicity, religion or relationships beyond 'couple' / 'close family' / 'guest'.
- If the evidence is ambiguous, return low confidence and explain what is missing.
- Return ONLY JSON matching the schema.
```

Required JSON response schema (validated; invalid = retry once, then fall back to local model):

```json
{
  "type": "object",
  "required": ["couple", "confidence", "reasons"],
  "properties": {
    "couple": { "type": "array", "items": { "type": "string" }, "minItems": 0, "maxItems": 2 },
    "close_family": { "type": "array", "items": { "type": "string" }, "maxItems": 10 },
    "confidence": { "type": "number", "minimum": 0, "maximum": 1 },
    "reasons": { "type": "array", "items": { "type": "string" }, "maxItems": 6 }
  },
  "additionalProperties": false
}
```

## 8. Implementation order (execute literally, in this order)

1. Land the schema and the face store with encryption; write the privacy tests first.
2. Integrate the detector with multi-scale/tiled inference; measure recall on small faces.
3. Add alignment plus the recognition embedder; verify embedding quality on labelled identities.
4. Train/calibrate the face-quality head and set the voting gate.
5. Implement clustering with the two-pass strategy and tune the threshold on fixtures.
6. Implement person detection and face-body association.
7. Implement role inference, co-occurrence graph and identity timelines.
8. Implement prominence scoring with the versioned weight config.
9. Build the People panel with merge/split/rename/mark-couple and `user_locked` semantics.
10. Add the optional cloud hint, evaluation gates, model cards and the erase-biometrics control.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `MLL` | ML Lead - Vision | Own detector/embedder selection, quality head design, clustering evaluation and demographic fairness analysis | Signed spec + fairness report | 3 d |
| `SRML` | Senior ML Engineer | Integrate and calibrate detection + recognition + quality models; export and verify parity | Models in registry | 6 d |
| `MLR` | ML Research Engineer | Tune clustering strategy, threshold sweeps, chain-merge prevention, sub-centroid handling | Clustering report | 4 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Face store, encryption, batching, face-body association, timelines and co-occurrence graph | `aura-people` + tests | 6 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Prominence scoring + versioned weight config + `subject_focus_score` API | Scoring module | 2 d |
| `AGT` | AI Agent & Prompt Engineer | Implement the optional couple-hint cloud task with strict non-inference rules | Cloud task + cassettes | 2 d |
| `SEC` | Security & Privacy Engineer | Biometric threat model, at-rest encryption, erase flow, no-cross-project proof, DPIA-style note | Security sign-off | 4 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | People panel, identity cards, merge/split, mark-couple, importance slider | UI shipped | 4 d |
| `MFE` | Mid-Level Frontend Engineer | Face-quality filters, person timeline view, erase-biometrics dialog, empty states | UI polish | 3 d |
| `DATA` | Data Engineer / Dataset Curator | Identity ground truth on fixtures (with consent records), small-face and dark-scene subsets | Labelled identities | 5 d |
| `QAL` | QA Lead - Automation | Detection recall, clustering F1, merge/split correctness, privacy tests in CI | CI gates | 4 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Manual audit: does the app pick the right couple on 20 real weddings across traditions? | Audit report | 3 d |
| `PERF` | Performance Engineer | Face pipeline throughput and memory; tile-pass cost/benefit tuning | Benchmark report | 2 d |
| `DOC` | Technical Writer | Privacy documentation, People panel help, model cards | Docs merged | 2 d |

### 9.1 Handoff chain for this phase

```text
SEC privacy design -> SRC face store -> SRML models -> MLR clustering
                                   |                        |
                                   v                        v
                        SRC prominence + graph        AGT cloud hint (optional)
                                   |
                        SFE/MFE People panel -> QAIQ audit -> MLL/CTO gate
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

- Detection recall >= 0.97 overall and >= 0.90 on the small-face subset; false positives on bokeh highlights < 1 %.
- Identity clustering F1 >= 0.93; no cluster contains two labelled people; siblings are not merged.
- Couple identification correct on >= 19 of 20 audit weddings; ambiguous cases correctly ask the user.
- Merge/split/rename are undoable and never overwritten by a re-analysis (`user_locked` respected).
- Privacy: face store unreadable without the keychain entry; erase removes all faces; no cross-project query is possible.
- Prominence: in labelled test frames, the identified dominant subject matches human judgement >= 90 % of the time.

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
| Face pipeline, 4,000 images (RTX 4070) | <= 240 s including tiled passes |
| Face pipeline, 4,000 images (M3 Pro) | <= 480 s |
| Clustering 25,000 faces | <= 12 s |
| People panel open | <= 300 ms |
| Face store size per 1,000 images | <= 25 MB |

Telemetry events (local-first, opt-in aggregation):

- `face.detect` {images, faces, small_faces, ms, tiled_pass_used}
- `identity.cluster` {faces_used, identities, threshold, ms}
- `identity.role_inferred` {role, confidence, evidence_kinds}
- `people.user_edit` {action: merge|split|rename|role, identities}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Wrong couple identification poisons every later decision | Confidence gate + explicit user confirmation prompt on first Autopilot run + `user_locked` overrides that survive re-analysis. |
| Recognition accuracy varies across skin tones | Mandatory demographic evaluation in the model card, balanced fixture sets, and a stricter margin rather than a wrong assignment. |
| Guests merged into one identity | Mutual-NN verification, high-quality-only skeleton pass, and a QA fixture with lookalike relatives. |
| Biometric regulation exposure | Local-only, project-scoped, encrypted, deletable, documented, never uploaded by default; SEC owns the compliance note. |
| Tiled detection doubles cost | Trigger tiling only for wide-angle frames with many small detections; measured cost/benefit in the benchmark report. |

## 13. Acceptance criteria

- [ ] After analysis, the People panel shows clean identities with face counts and a suggested couple.
- [ ] Marking someone as the bride updates every downstream weight and survives a full re-analysis.
- [ ] Wide ceremony frames detect guests reliably; back-of-head cases still register a person.
- [ ] Every image exposes `subject_focus_score` and per-identity prominence.
- [ ] Biometric data is encrypted, project-scoped and erasable, and never leaves the machine by default.
- [ ] Fairness analysis is published in the model cards with per-group metrics.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 06 - Face Detection, Recognition & People Intelligence.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-06-PEOPLE-INTELLIGENCE.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - The app learns who matters at this wedding: it finds every face, groups them into identities, and automatically ranks bride, groom, immediate family and VIPs by evidence rather than by guesswork.

Rules:
  - Do not start Phase 7. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-vision/src/face/{detect,align,embed,quality,cluster,roles,prominence,person}.rs`, `crates/aura-catalog/migrations/0006_people.sql`, `crates/aura-people/src/{lib,timeline,graph,importance,api}.rs`, `ml/models/face/{train_quality.py,tune_cluster.py,eval_identity.py}`, `apps/desktop/src/routes/people/{PeoplePanel,IdentityCard,MergeDialog}.tsx`, `docs/model-cards/{face_detect,face_embed,face_quality}.md`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-06-people-intelligence and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-06.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-06-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-06-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-06-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 06 of 30 - Face Detection, Recognition & People Intelligence - part of the AURA Wedding AI master build plan.*

---

# Phase 07 - Wedding Scene AI & Story Timeline Segmentation

> **Single feature shipped by this phase:** The app reads the wedding as a story: it labels every photo's scene and splits the day into ordered chapters (Getting Ready -> Details -> Ceremony -> Rituals -> Portraits -> Reception -> Dance -> Exit) with confidence.
>
> **Mission:** Produce the scene graph that makes every later threshold scene-aware. A dark dance frame and a formal family portrait must never be judged by the same rules again.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 07 of 30 |
| Epic | E2 - Wedding Brain |
| Feature | The app reads the wedding as a story: it labels every photo's scene and splits the day into ordered chapters (Getting Ready -> Details -> Ceremony -> Rituals -> Portraits -> Reception -> Dance -> Exit) with confidence. |
| Depends on | Phases 02, 03, 05, 06 |
| Unlocks | Phases 09-13, 15-19, 25, 27, 29 |
| Duration | 2.5 weeks |
| Primary owners | ML Lead - Vision, Senior ML Engineer, AI Agent & Prompt Engineer, Senior Engineer - Core Pipeline (Rust) |
| Risk level | High - this is the core differentiator |
| Headline KPI | scene accuracy >= 0.92 top-1; segment boundary error <= 45 s median; ritual detection F1 >= 0.85 on Hindu/Nepali/Christian/Muslim fixtures |
| Competitor being beaten | FilterPixel contextual culling; Aftershoot scene awareness |

## 1. Why this phase exists

Every other product treats a wedding as a bag of frames. Reading it as an ordered story is what lets the product make defensible decisions: which frames may be dark, where blur is acceptable, which moments cannot be dropped, and how a scene should be graded.

Cultural coverage is a genuine competitive moat. Global tools handle Western wedding structure adequately and Hindu, Nepali and Muslim ceremonies poorly. Explicit ritual taxonomies plus cloud reasoning for uncertain segments turn that gap into an advantage.

## 2. Scope contract

### 2.1 In scope

- Per-image scene classifier: 22 classes (getting_ready_bride, getting_ready_groom, details, first_look, ceremony_entrance, ceremony, ritual, vows, rings, kiss, family_portrait, group_portrait, couple_portrait, golden_hour, reception_entrance, speeches, cake, first_dance, dance_floor, candid, venue, exit) plus 14 attributes (indoor/outdoor, flash, mixed_light, backlit, night, tungsten, crowd, stage, vehicle, food, decor, kids, pets, rain).
- Ritual sub-classifier for tradition-specific events (e.g. saptapadi, jaymala/varmala, sindoor, mehendi, sangeet, nikah, signing, unity candle, tea ceremony) with an extensible taxonomy file.
- Temporal segmentation: change-point detection over embeddings + time + scene posteriors, producing ordered `segments` with start/end, dominant scene, confidence and key-frame.
- Scene smoothing with a hidden Markov model over the timeline so single-frame misclassifications cannot create fake chapters.
- Cloud reasoning for low-confidence segments (Phase 04 `SegmentNaming` task) plus a user-editable chapter timeline.
- Scene profile registry: each scene carries default thresholds (acceptable noise, acceptable blur, expected keeper rate, editing intent) consumed by later phases.
- Timeline UI: horizontal chapter strip with counts, durations, thumbnails, drag-to-adjust boundaries and rename.

### 2.2 Explicitly out of scope (do not build it here)

- Culling decisions (Phase 12) and coverage guarantees (also Phase 12).
- Editing parameters per scene (Phases 15-17 read the scene, they do not define it).
- Album sequencing (Phase 29).

## 3. Architecture and data flow

```text
embeddings (P05) + faces (P06) + EXIF (P01) + luma stats (P05)
        |
        v
  SceneClassifier (multi-head: 22 scenes + 14 attributes)  --> per-image posteriors
        |                                                        |
        v                                                        v
  RitualClassifier (tradition taxonomy)                    HMM smoothing over timeline_ts
        |                                                        |
        +---------------------> ChangePointDetector <------------+
                                       |
                            segments[] (chapter, start, end, key_frame, confidence)
                                       |
              low confidence? --> Cloud SegmentNaming (P04) --> merged label + reasons
                                       |
                        SceneProfile registry (thresholds per scene) --> Phases 09-19, 25, 27, 29
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-brain-wedding/src/scene/{classifier,attributes,ritual,taxonomy,profile}.rs` | Scene and ritual inference plus the scene-profile registry. |
| `crates/aura-brain-wedding/src/story/{segment,changepoint,hmm,keyframe,api}.rs` | Timeline segmentation and chapter construction. |
| `crates/aura-catalog/migrations/0007_scenes.sql` | `image_scenes`, `segments`, `segment_images`, `scene_profiles` tables. |
| `config/scene_profiles.toml` + `config/rituals/{hindu,nepali,christian,muslim,civil}.toml` | Editable taxonomies and per-scene thresholds. |
| `ml/models/scene/{dataset.py,train_multihead.py,train_ritual.py,eval_scene.py,export.py}` | Training and evaluation. |
| `apps/desktop/src/routes/story/{Timeline,ChapterStrip,BoundaryEditor}.tsx` | Story timeline UI. |
| `docs/model-cards/{scene_classifier,ritual_classifier}.md` | Model cards with per-tradition metrics. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Scene, segment and profile contracts (frozen)**

```rust
pub struct SceneResult {
    pub image_id: ImageId,
    pub scene: SceneId, pub scene_conf: f32,
    pub top3: [(SceneId, f32); 3],
    pub attributes: AttrFlags,          // bitflags: indoor, flash, backlit, night, ...
    pub ritual: Option<RitualId>, pub ritual_conf: f32,
    pub source: Source,                 // Local | Cloud | UserOverride
    pub model_ver: u16,
}

pub struct Segment {
    pub id: SegmentId, pub chapter: ChapterId,
    pub start_ts: Timestamp, pub end_ts: Timestamp,
    pub dominant_scene: SceneId, pub confidence: f32,
    pub key_frame: ImageId, pub image_count: u32,
    pub reasons: Vec<String>, pub user_locked: bool,
}

pub struct SceneProfile {
    pub scene: SceneId,
    pub keeper_rate: (f32, f32),        // expected min/max survival ratio
    pub max_acceptable_noise: f32,      // scene-relative tolerance
    pub max_acceptable_blur: f32,
    pub subject_focus_weight: f32,      // how much subject sharpness dominates
    pub emotion_weight: f32,
    pub composition_weight: f32,
    pub editing_intent: EditIntent,     // Airy | Neutral | Warm | Moody | Punchy
    pub must_cover: bool,               // coverage guarantee applies
}
```

## 6. Algorithm, model and implementation design

### 6.1 Multi-head classifier design

- Shared trunk = the Phase 05 embedding (frozen) plus a small trainable adapter, so scene inference costs almost nothing extra per image.
- Heads: 22-way softmax scene, 14 independent sigmoid attributes, and a tradition-conditioned ritual head that is only evaluated when the segment context suggests a ceremony.
- Context features concatenated to the embedding: hour-of-day offset from first frame, flash flag, ISO bucket, face count, dominant-identity presence, indoor/outdoor prior from luma and colour temperature.
- Class imbalance handled with focal loss; rare rituals get oversampling plus a 'don't guess' abstain threshold.

### 6.2 Timeline segmentation

- Build a 1-D signal of embedding distance between consecutive frames, normalised by time gap, then run PELT change-point detection with a penalty tuned so a 10-hour wedding yields 8-16 chapters.
- Merge micro-segments shorter than 90 s or with fewer than 8 frames into their nearest neighbour unless their scene posterior is strongly distinct.
- Time gaps larger than 20 minutes are hard boundaries (photographers travel between locations).
- HMM smoothing with a hand-authored transition matrix encoding real wedding order (getting_ready rarely follows dance_floor) - this single trick removes most absurd labels.
- Key-frame per segment = medoid embedding among frames with high `subject_focus_score` and at least one primary identity.

### 6.3 Scene profiles: where the intelligence becomes actionable

- Every scene declares tolerances and weights in `scene_profiles.toml`, versioned and shipped with the app, overridable per project.
- Examples: `dance_floor` allows noise 3x the ceremony budget, expects motion blur, weights emotion 0.5 and composition 0.15; `family_portrait` demands eyes open for all subjects, weights composition 0.35 and forbids clipped faces; `details` ignores faces entirely.
- Profiles also declare `editing_intent`, which Phases 15-17 translate into tone/colour targets - the mechanism behind 'the ceremony looks like a ceremony, the dance floor looks like a party'.

### 6.4 Uncertainty and user control

- Segments with confidence < 0.75 are sent to the cloud `SegmentNaming` task once each (contact sheet of 12), and the merged result records both sources.
- The user can rename, split, merge and move boundaries; anything the user touches is `user_locked` and re-analysis never overwrites it.
- Unknown or genuinely novel events map to `other` with a description rather than a wrong confident label.

## 7. Cloud AI usage (bring-your-own API key)

**Name and validate uncertain chapters, identify tradition-specific rituals**

| Aspect | Specification |
|---|---|
| Model class | Vision reasoning tier, temperature 0 |
| Trigger | Segment confidence < 0.75, or ritual head abstains inside a ceremony segment |
| Input sent | Contact sheet of up to 12 key thumbnails (768 px), time range, local top-3 with scores, attribute flags, allowed vocabulary from the taxonomy files |
| Cost control | <= 16 calls per wedding; cached by content hashes |
| Offline fallback | Local argmax with confidence penalty and a 'needs review' flag in the timeline UI |

System prompt contract:

```text
You are a wedding post-production analyst reading one chapter of a wedding.
Input: a contact sheet of consecutive frames, the time range, the local classifier's top guesses, and the allowed vocabulary.
Task: choose the chapter label, the specific ritual if clearly visible, the tradition the visual evidence supports, and whether the chapter should be split.
Rules:
- Use only the allowed vocabulary; return "other" plus notes when nothing fits.
- Never infer names, gender, ethnicity or religion of individuals; describe the ceremony type only from visible ritual objects and staging.
- Prefer low confidence over a confident guess.
- Return ONLY JSON matching the schema.
```

Required JSON response schema (validated; invalid = retry once, then fall back to local model):

```json
{
  "type": "object",
  "required": ["chapter", "confidence", "reasons"],
  "properties": {
    "chapter": { "type": "string" },
    "ritual": { "type": ["string", "null"] },
    "tradition": { "type": ["string", "null"] },
    "split_at_index": { "type": ["integer", "null"] },
    "confidence": { "type": "number", "minimum": 0, "maximum": 1 },
    "reasons": { "type": "array", "items": { "type": "string" }, "maxItems": 6 },
    "notes": { "type": ["string", "null"] }
  },
  "additionalProperties": false
}
```

## 8. Implementation order (execute literally, in this order)

1. Author the scene taxonomy, attribute list and ritual taxonomies with a wedding photographer consultant; commit them as config.
2. Label the fixture weddings and an expansion set covering four traditions (DATA).
3. Train the multi-head classifier on the frozen embedding trunk; iterate until the accuracy gate is met.
4. Train the ritual head with abstention; evaluate per tradition.
5. Implement change-point detection and HMM smoothing; tune the penalty on labelled timelines.
6. Implement key-frame selection and the segment store.
7. Author `scene_profiles.toml` with the consultant and get PM sign-off - this file is a product decision, not a code detail.
8. Wire the cloud `SegmentNaming` task for low-confidence segments.
9. Build the timeline UI with boundary editing and locking.
10. Run evaluation gates, publish model cards, write the exit report.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `MLL` | ML Lead - Vision | Own taxonomy, model architecture, abstention policy, per-tradition evaluation | Signed spec + gates | 3 d |
| `SRML` | Senior ML Engineer | Train and export scene + ritual heads; parity and calibration | Models registered | 6 d |
| `MLR` | ML Research Engineer | Change-point penalty tuning, HMM transition matrix, ablation of context features | Segmentation report | 4 d |
| `DATA` | Data Engineer / Dataset Curator | Scene/ritual/timeline labels across four traditions; enforce wedding-level splits and consent records | Dataset v2 | 8 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Segment store, smoothing, key-frames, scene-profile registry, override semantics | `aura-brain-wedding` + tests | 5 d |
| `AGT` | AI Agent & Prompt Engineer | `SegmentNaming` integration, vocabulary injection, merge rules, cassette tests | Cloud path live | 2 d |
| `PM` | Product Manager Agent | Sign off `scene_profiles.toml` values with a professional wedding photographer; document rationale | Approved profiles | 3 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Chapter strip, boundary drag, split/merge, rename, review flags | Timeline UI | 4 d |
| `MFE` | Mid-Level Frontend Engineer | Scene filter chips in the grid, per-scene counts, 'needs review' queue | UI polish | 3 d |
| `QAL` | QA Lead - Automation | Accuracy/boundary/ritual gates in CI, override-persistence tests, weird-timeline fixtures | CI gates | 4 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Human review of chapters on 20 weddings across traditions; catalogue systematic errors | Audit report | 3 d |
| `DOC` | Technical Writer | Document the taxonomy, how to add a tradition, and how profiles affect results | Docs merged | 2 d |

### 9.1 Handoff chain for this phase

```text
PM + consultant taxonomy -> DATA labels -> SRML models -> MLR segmentation
                                                   |
                                                   v
                                       SRC store + profiles -> AGT cloud naming
                                                   |
                                    SFE/MFE timeline UI -> QAIQ audit -> MLL/PM gate
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

- Scene top-1 accuracy >= 0.92 overall and >= 0.85 for every individual class with more than 200 labelled samples.
- Boundary error median <= 45 s; no wedding produces fewer than 6 or more than 20 chapters.
- Ritual F1 >= 0.85 per tradition; abstention rather than wrong labels on unseen rituals.
- HMM sanity: no chapter sequence violates the allowed transition matrix.
- User overrides survive full re-analysis and model upgrades.
- Two-shooter interleaving does not fragment chapters (specific regression fixture).

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
| Scene + attributes for 4,000 images (reusing embeddings) | <= 35 s |
| Segmentation + smoothing | <= 2 s |
| Timeline UI open | <= 200 ms |
| Extra storage per image | <= 400 B |

Telemetry events (local-first, opt-in aggregation):

- `scene.classified` {images, ms, mean_conf, low_conf_count}
- `story.segmented` {segments, boundary_penalty, chapters}
- `story.user_edit` {action, segment, from_label, to_label}
- `scene.cloud_used` {segments, calls, cost_usd}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Cultural blind spots produce wrong chapters | Tradition taxonomies in config, cloud reasoning fallback, mandatory per-tradition metrics, and a user-editable timeline. |
| Over-segmentation makes the timeline noisy | Minimum segment size, merge pass, penalty tuning against labelled timelines, and a hard chapter-count band. |
| Scene profiles become a dumping ground of magic numbers | Every value has a written rationale, an owner (PM) and a QA fixture demonstrating its effect. |
| Model drift after an embedding upgrade | Scene heads are versioned against `embed model_ver`; a mismatch forces re-inference. |

## 13. Acceptance criteria

- [ ] A freshly analysed wedding shows a correct, ordered chapter strip with counts and durations.
- [ ] Every image carries a scene label, attributes and confidence, visible in the Explain panel.
- [ ] Hindu, Nepali, Christian and Muslim fixture weddings all produce correct ritual labels or honest abstentions.
- [ ] Editing a boundary or renaming a chapter persists through re-analysis.
- [ ] `scene_profiles.toml` drives measurable behaviour differences in later phases (proved by a fixture test).
- [ ] Low-confidence chapters are flagged for review rather than silently guessed.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 07 - Wedding Scene AI & Story Timeline Segmentation.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-07-WEDDING-SCENE-STORY-AI.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - The app reads the wedding as a story: it labels every photo's scene and splits the day into ordered chapters (Getting Ready -> Details -> Ceremony -> Rituals -> Portraits -> Reception -> Dance -> Exit) with confidence.

Rules:
  - Do not start Phase 8. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-brain-wedding/src/scene/{classifier,attributes,ritual,taxonomy,profile}.rs`, `crates/aura-brain-wedding/src/story/{segment,changepoint,hmm,keyframe,api}.rs`, `crates/aura-catalog/migrations/0007_scenes.sql`, `config/scene_profiles.toml` + `config/rituals/{hindu,nepali,christian,muslim,civil}.toml`, `ml/models/scene/{dataset.py,train_multihead.py,train_ritual.py,eval_scene.py,export.py}`, `apps/desktop/src/routes/story/{Timeline,ChapterStrip,BoundaryEditor}.tsx`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-07-wedding-scene-story-ai and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-07.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-07-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-07-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-07-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 07 of 30 - Wedding Scene AI & Story Timeline Segmentation - part of the AURA Wedding AI master build plan.*

---

# Phase 08 - Smart Burst Grouping & Duplicate Detection

> **Single feature shipped by this phase:** Sequential and visually similar frames collapse into burst groups, and near-identical duplicates are identified so only the best frame from each moment competes for the gallery.
>
> **Mission:** Turn 3,000 loose files into roughly 700-1,100 'moments'. Every later decision - culling, QC, album - operates on moments, which is both faster and far more human.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 08 of 30 |
| Epic | E2 - Wedding Brain |
| Feature | Sequential and visually similar frames collapse into burst groups, and near-identical duplicates are identified so only the best frame from each moment competes for the gallery. |
| Depends on | Phases 05, 06, 07 |
| Unlocks | Phases 09-13, 27, 29 |
| Duration | 1.5 weeks |
| Primary owners | ML Lead - Vision, Senior Engineer - Core Pipeline (Rust), ML Research Engineer |
| Risk level | Medium |
| Headline KPI | burst grouping ARI >= 0.90 vs human grouping; duplicate recall >= 0.98 / precision >= 0.95; grouping of 4,000 images <= 6 s |
| Competitor being beaten | Aftershoot and Narrative Select burst grouping |

## 1. Why this phase exists

Photographers shoot in bursts: 6 frames of the kiss, 14 of the bouquet toss, 40 of the dance floor. Choosing the best of each burst is the single highest-value culling decision, and it is only possible if bursts are grouped correctly.

Grouping also protects the story: rejecting a burst is a moment lost, whereas rejecting individual frames is just tidying. Phase 12's coverage guarantees depend on moments, not files.

## 2. Scope contract

### 2.1 In scope

- Multi-signal grouping: time proximity (adaptive), embedding similarity, dHash distance, face-identity overlap, camera identity, drive-mode/sub-second EXIF evidence.
- Adaptive time window: derived from local shooting cadence per camera (median inter-frame interval in a 60 s neighbourhood), so a 10 fps burst and a slow ceremony are both handled.
- Two-tier structure: `moment` (a shot of the same subject/action) inside `segment` (a chapter), with `burst` as the tightest sub-group.
- Duplicate classes: `identical` (same file re-imported), `near_identical` (dHash <= 4 and embedding distance <= 0.03), `variant` (same moment, meaningful difference).
- Cross-camera moment merging: the same instant shot by two photographers becomes one moment with per-camera sub-groups (critical for Phase 26).
- Group-level statistics: size, duration, dominant identities, best-so-far pointer, and per-group diversity score.
- UI: moment view (stacked cells with a count badge), expand/collapse, manual split/merge with locking.

### 2.2 Explicitly out of scope (do not build it here)

- Choosing the winner of a burst (Phase 12; this phase only provides candidates and diversity).
- Quality scoring (Phase 09) and expression scoring (Phase 10).
- Deleting anything - grouping never removes files.

## 3. Architecture and data flow

```text
images ordered by timeline_ts (per camera)
     |
     v
  CadenceEstimator --> adaptive window w(t)
     |
     v
  candidate edges: |dt| < w(t)  AND  (embed_dist < t1 OR dhash_dist < t2)
     |
     +-- identity overlap boost / scene mismatch penalty / camera-aware weighting
     v
  union-find graph -> bursts -> moments -> attached to segments (P07)
     |
     +--> duplicate classifier (identical | near_identical | variant)
     |
     v
  moments table (size, duration, identities, diversity, locked)
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-brain-wedding/src/moments/{cadence,graph,burst,moment,duplicate,merge,api}.rs` | Grouping engine. |
| `crates/aura-catalog/migrations/0008_moments.sql` | `moments`, `moment_images`, `duplicates` tables. |
| `ml/eval/burst_eval.py` | ARI/NMI evaluation against human grouping labels. |
| `apps/desktop/src/components/grid/MomentStack.tsx` | Stacked moment cells, expand/collapse, split/merge. |
| `tests/fixtures/labels/bursts_*.json` | Human burst-grouping ground truth. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Moment model (frozen)**

```rust
pub struct Moment {
    pub id: MomentId, pub segment_id: SegmentId,
    pub image_ids: Vec<ImageId>,          // ordered by timeline_ts
    pub bursts: Vec<Vec<ImageId>>,        // tighter sub-groups
    pub start_ts: Timestamp, pub end_ts: Timestamp,
    pub cameras: Vec<CameraId>,
    pub identities: Vec<IdentityId>,
    pub diversity: f32,                   // 0 = all identical, 1 = very different
    pub duplicate_sets: Vec<DuplicateSet>,
    pub user_locked: bool,
}

pub struct DuplicateSet {
    pub kind: DuplicateKind,              // Identical | NearIdentical | Variant
    pub image_ids: Vec<ImageId>,
    pub keep_hint: ImageId,               // best technical frame, not the final decision
    pub confidence: f32,
}
```

## 6. Algorithm, model and implementation design

### 6.1 Adaptive cadence estimation

- For each camera, compute the rolling median inter-frame interval over a 60 s window; the burst window is `max(0.7 s, 2.5 x median)` clamped to 8 s.
- Sub-second EXIF and drive-mode tags, where present, mark true camera bursts explicitly - use them as strong evidence rather than inferring.
- Long gaps (> 20 s) break groups regardless of similarity, because the same pose 30 seconds later is usually a new attempt at a moment, not the same moment.

### 6.2 Graph construction and grouping

- Build a sparse similarity graph with time-windowed kNN from Phase 05 (never all-pairs), then union-find over edges that pass a scene-aware threshold.
- Edge score = 0.55 * (1 - embed_dist) + 0.2 * (1 - dhash_norm) + 0.15 * identity_overlap + 0.1 * same_camera; thresholds come from the scene profile (`dance_floor` needs looser thresholds than `family_portrait`).
- Split over-large groups (> 60 frames) by re-clustering with a stricter threshold so a 5-minute dance sequence does not become one moment.
- Cross-camera merge: two moments from different cameras merge when their time ranges overlap by > 60 % and their embedding medoids are within 0.12.

### 6.3 Duplicate classification

- `identical`: same content hash (re-import) - handled at ingest but reported here for completeness.
- `near_identical`: dHash Hamming <= 4 and embedding distance <= 0.03 and face-box IoU >= 0.9 -> only one frame should ever reach the gallery.
- `variant`: same moment but a meaningful difference (expression, eyes, framing) -> all frames stay eligible and Phase 12 chooses.
- `keep_hint` is provisional: the technically strongest frame by edge energy and subject focus, replaced by the real decision in Phase 12.

### 6.4 Diversity score (feeds album and social selection later)

- Diversity = mean pairwise embedding distance within the moment, normalised; low diversity means the photographer bracketed a static subject, high diversity means the action evolved.
- High-diversity moments are allowed more than one keeper in Phase 12; low-diversity moments are capped at one.

## 7. Cloud AI usage (bring-your-own API key)

No cloud AI call in this phase. The phase must work with the network cable unplugged; the Cloud AI Gateway from Phase 04 stays idle here.

## 8. Implementation order (execute literally, in this order)

1. Implement cadence estimation with tests on real burst fixtures.
2. Implement the time-windowed similarity graph and union-find grouping.
3. Add scene-aware thresholds from Phase 07 profiles and the over-large-group split pass.
4. Implement duplicate classification with the three classes and confidence.
5. Implement cross-camera merging and per-camera sub-groups.
6. Compute diversity and group statistics; persist moments.
7. Build the moment stack UI with expand/collapse and manual split/merge.
8. Evaluate against human grouping labels; tune until the ARI gate passes.
9. Add regression fixtures: 10 fps bouquet toss, slow ceremony, dance floor, two-shooter overlap.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `MLL` | ML Lead - Vision | Define grouping objective, evaluation metric and acceptance gates | Spec + gates | 1 d |
| `MLR` | ML Research Engineer | Tune edge weights and thresholds per scene; ablate each signal's contribution | Tuning report | 3 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Implement cadence, graph, union-find, splitting, duplicates, cross-camera merge, persistence | `moments` module + tests | 6 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Manual split/merge with locking and undo | Editing API | 2 d |
| `DATA` | Data Engineer / Dataset Curator | Human burst-grouping ground truth on fixtures plus adversarial cases | Labels v1 | 3 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Moment stack cells, count badges, expand/collapse, keyboard flow | Moment UI | 3 d |
| `MFE` | Mid-Level Frontend Engineer | Duplicate review panel with side-by-side comparison and 'keep this one' action | UI panel | 2 d |
| `QAL` | QA Lead - Automation | ARI/precision/recall gates, adversarial fixtures, performance test, lock persistence | CI gates | 3 d |
| `PERF` | Performance Engineer | Ensure grouping stays linear-ish; profile graph construction on 6,000 images | Benchmark | 1 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Eyeball 500 groups: any group mixing two different moments is a bug | Audit report | 2 d |
| `DOC` | Technical Writer | Explain moments vs bursts vs duplicates in the help centre | Docs merged | 1 d |

### 9.1 Handoff chain for this phase

```text
DATA labels -> MLR tuning -> SRC implementation -> SFE/MFE UI
                                     |
                        QAL gates + QAIQ audit + PERF profile -> MLL gate
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

- ARI >= 0.90 against human grouping on all fixtures; no group mixes two labelled moments.
- Near-identical detection: recall >= 0.98 at precision >= 0.95, including one-stop-apart exposures.
- 10 fps burst stays one moment; a 5-minute dance sequence becomes multiple moments.
- Two cameras shooting the same kiss produce one moment with two sub-groups.
- Manual split/merge persists through re-analysis; undo restores exactly.
- 4,000 images grouped in <= 6 s with < 200 MB extra memory.

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
| Grouping 4,000 images | <= 6 s |
| Grouping 12,000 images | <= 25 s |
| Extra storage per image | <= 200 B |
| Moment UI expand/collapse | <= 60 ms |

Telemetry events (local-first, opt-in aggregation):

- `moments.built` {images, moments, bursts, mean_size, ms}
- `duplicates.found` {identical, near_identical, variant}
- `moments.user_edit` {action, moment_size}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Merging two different moments hides a keeper | Conservative thresholds, over-large-group splitting, QA audit gate, and one-click split in the UI. |
| Dance floor produces giant groups | Scene-aware thresholds plus a hard size cap with re-clustering. |
| Cross-camera merge misfires on similar scenes | Require strong temporal overlap and medoid proximity; keep sub-groups separable so a bad merge is recoverable. |
| Duplicate deletion anxiety | Nothing is deleted; duplicates are marked and reviewable, and the user can always see all frames. |

## 13. Acceptance criteria

- [ ] The grid can switch between 'all frames' and 'moments' views, with correct counts.
- [ ] Bursts are grouped the way a photographer would group them on the audit set.
- [ ] Near-identical frames are marked with a confidence and reviewable side by side.
- [ ] The same instant captured by two shooters appears as one moment.
- [ ] Manual grouping edits are permanent and undoable.
- [ ] Grouping 4,000 images takes seconds, not minutes.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 08 - Smart Burst Grouping & Duplicate Detection.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-08-BURST-GROUPING-DUPLICATES.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - Sequential and visually similar frames collapse into burst groups, and near-identical duplicates are identified so only the best frame from each moment competes for the gallery.

Rules:
  - Do not start Phase 9. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-brain-wedding/src/moments/{cadence,graph,burst,moment,duplicate,merge,api}.rs`, `crates/aura-catalog/migrations/0008_moments.sql`, `ml/eval/burst_eval.py`, `apps/desktop/src/components/grid/MomentStack.tsx`, `tests/fixtures/labels/bursts_*.json`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-08-burst-grouping-duplicates and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-08.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-08-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-08-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-08-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 08 of 30 - Smart Burst Grouping & Duplicate Detection - part of the AURA Wedding AI master build plan.*

---

# Phase 09 - Frame Integrity AI: Focus, Motion, Exposure, Noise & Eye State

> **Single feature shipped by this phase:** Every frame gets an honest technical verdict where it matters: is the *right subject* sharp, is motion intentional, is exposure recoverable, how noisy is it, and are the important eyes open?
>
> **Mission:** Replace global 'is this photo sharp?' with subject-aware, scene-aware, intent-aware technical judgement - including the blink detection that photographers care about most.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 09 of 30 |
| Epic | E2 - Wedding Brain |
| Feature | Every frame gets an honest technical verdict where it matters: is the *right subject* sharp, is motion intentional, is exposure recoverable, how noisy is it, and are the important eyes open? |
| Depends on | Phases 02, 05, 06, 07 |
| Unlocks | Phases 12, 13, 22, 27 |
| Duration | 2.5 weeks |
| Primary owners | ML Lead - Vision, Senior ML Engineer, Colour Scientist, Senior Engineer - Core Pipeline (Rust) |
| Risk level | High - false rejections destroy trust instantly |
| Headline KPI | subject-focus AUC >= 0.96; blink detection F1 >= 0.95 with intentional-closed-eye false-positive rate <= 2 %; analysis <= 45 ms/image |
| Competitor being beaten | Narrative Select eye assessment; Aftershoot blur detection; FilterPixel technical scoring |

## 1. Why this phase exists

Photographers abandon culling tools the moment one is thrown away that should have been kept. Technical scoring must therefore be conservative, explainable and subject-aware: a blurred background is craft, a blurred bride is a defect.

Blink handling is the classic trust test. Closed eyes during a kiss, a prayer, a first-look reaction or a tearful hug are *good* photographs. A naive eye-open detector destroys exactly the frames that sell weddings.

Exposure and noise verdicts must be recovery-aware: a 2-stop-under dance frame from a modern sensor is fine, the same from an older body is not. That requires camera-aware, RAW-aware analysis rather than JPEG heuristics.

## 2. Scope contract

### 2.1 In scope

- Region-aware sharpness: per-face and per-subject sharpness via a learned focus head plus classical measures (Laplacian variance, Tenengrad, MTF50 estimate on eye regions), calibrated per camera resolution.
- Motion analysis: distinguish camera shake (global directional smear), subject motion (local smear with sharp background) and intentional motion (panning, dance blur), using directional gradient statistics and EXIF shutter/focal length.
- Focus-miss detection: is the sharpest plane on the subject or behind/in front of it (back/front focus), using relative sharpness of face vs background bands.
- Exposure verdict: clipping percentages per channel, recoverable-highlight estimate from RAW headroom, shadow noise floor estimate, and a scene-aware 'recoverable / marginal / lost' label.
- Noise estimation: per-image sigma estimate in flat regions, ISO-aware and camera-aware, expressed as a scene-relative tolerance.
- Eye state per face: open / squint / closed / looking-down / occluded, plus intent classification (`intentional_closed` when the scene, expression and context justify it).
- Composite `technical_score` and `integrity_flags` bitfield, all scene-weighted and all with reasons.
- Camera calibration table: per-body sharpness and noise normalisation so a 61 MP body is not unfairly favoured over a 24 MP body.

### 2.2 Explicitly out of scope (do not build it here)

- Expression quality and emotional value (Phase 10).
- Composition (Phase 11).
- Any decision to keep or reject (Phase 12).
- Fixing noise or blur (Phase 22).

## 3. Architecture and data flow

```text
proxy (P02) + faces (P06) + scene (P07) + EXIF
   |
   +--> FocusHead (learned, per-region) --+
   +--> classical sharpness (Laplacian/Tenengrad/MTF50 on eyes) --+--> subject_sharpness, bg_sharpness
   +--> directional gradient stats + shutter/focal --> motion_kind (shake|subject|intentional|none)
   +--> RAW histogram + headroom --> exposure_verdict (recoverable|marginal|lost)
   +--> flat-region sigma + ISO/camera table --> noise_sigma_rel
   +--> EyeStateHead per face --> open|squint|closed|down|occluded + intent
                        |
                        v
     technical_score (scene-weighted) + integrity_flags + reasons[]
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-brain-photo/src/integrity/{focus,motion,exposure,noise,eyes,score,flags}.rs` | All technical analysis. |
| `crates/aura-brain-photo/src/integrity/calibration.rs` + `config/camera_calibration.toml` | Per-body sharpness/noise normalisation. |
| `ml/models/integrity/{train_focus.py,train_eyes.py,eval_integrity.py,export.py}` | Focus and eye-state models. |
| `crates/aura-catalog/migrations/0009_integrity.sql` | `image_integrity`, `face_eye_state` tables. |
| `apps/desktop/src/components/explain/IntegrityCard.tsx` | Per-image technical readout with crops. |
| `docs/model-cards/{focus_head,eye_state}.md` | Model cards including intentional-closed-eye analysis. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Integrity result (frozen)**

```rust
bitflags! { pub struct IntegrityFlags: u32 {
    const SUBJECT_SOFT       = 1<<0;  const CAMERA_SHAKE      = 1<<1;
    const SUBJECT_MOTION     = 1<<2;  const INTENTIONAL_MOTION= 1<<3;
    const BACK_FOCUS         = 1<<4;  const FRONT_FOCUS       = 1<<5;
    const HIGHLIGHT_LOST     = 1<<6;  const SHADOW_LOST       = 1<<7;
    const HEAVY_NOISE        = 1<<8;  const EYES_CLOSED       = 1<<9;
    const EYES_CLOSED_OK     = 1<<10; const SQUINT            = 1<<11;
    const NO_SUBJECT_DETECTED= 1<<12; const MIXED_LIGHT_RISK  = 1<<13;
} }

pub struct IntegrityResult {
    pub image_id: ImageId,
    pub subject_sharpness: f32,      // 0..1, prominence-weighted
    pub bg_sharpness: f32,
    pub focus_offset: f32,           // negative = front focus, positive = back focus
    pub motion: MotionKind, pub motion_severity: f32,
    pub exposure: ExposureVerdict, pub clip_hi: f32, pub clip_lo: f32, pub ev_offset: f32,
    pub noise_sigma_rel: f32,
    pub eyes: Vec<EyeState>,         // per face, with identity + intent
    pub technical_score: f32,        // scene-weighted 0..1
    pub flags: IntegrityFlags,
    pub reasons: Vec<Reason>,        // { code, text, weight, evidence_crop }
    pub confidence: f32,
}
```

## 6. Algorithm, model and implementation design

### 6.1 Subject-aware sharpness

- Compute sharpness on eye regions first (that is what viewers judge), then face, then body, then background bands; combine with Phase 06 prominence weights.
- Normalise by camera calibration: expected MTF50 for the body/lens/aperture combination, so 'sharp' means sharp *for this gear*.
- A learned focus head trained on labelled sharp/soft crops handles cases classical measures fail on: shallow depth of field, veil textures, backlit rim light, heavy bokeh.
- Output both absolute and *within-moment relative* sharpness, because Phase 12 mostly needs 'which of these six is sharpest'.

### 6.2 Motion intent classification

- Estimate the dominant gradient direction and anisotropy: global anisotropic smear + long shutter => camera shake; local smear with sharp background => subject motion; strong panning signature with sharp subject => intentional.
- Use EXIF: shutter vs 1/focal-length rule, stabilisation flags, and scene (`dance_floor` and `exit` expect motion; `family_portrait` does not).
- Never flag intentional motion as a defect; instead expose `INTENTIONAL_MOTION` so Phase 12 can treat it as a stylistic keeper.

### 6.3 Exposure and noise, recovery-aware

- Compute clipping from the RAW histogram before tone mapping, per channel, with the specular-highlight exclusion mask (a clipped candle flame is not a defect).
- Estimate recoverable headroom from `white_level` and per-camera measured headroom; label `recoverable` if a +/- correction stays within the noise budget for that ISO.
- Noise sigma is measured in flat regions (low local gradient) and normalised by ISO and camera; expressed relative to the scene profile's tolerance so a dance frame is not punished for being a dance frame.

### 6.4 Eye state and intent - the trust-critical part

- Per-face eye-state head on aligned crops with five classes plus a confidence; only faces above the Phase 06 quality gate are judged.
- Intent rules: closed eyes are acceptable when (scene in {kiss, vows, ritual, hug, first_look, speeches_emotional}) OR (mouth-smile strong and head tilted) OR (tears detected in Phase 10) OR (both partners' eyes closed simultaneously in a couple frame).
- Only the *important* identities' eyes gate a frame: a guest blinking in row four is not a defect; the bride blinking in a portrait is.
- Group photos get a special path: count how many primary/secondary subjects have closed eyes and expose `closed_eye_ratio` for Phase 12's group rules.

### 6.5 Scoring and explainability

- `technical_score = product of scene-weighted sub-scores with soft penalties`, deliberately not a linear sum, so one catastrophic factor cannot be averaged away.
- Every penalty writes a `Reason` with a code, human text, weight and an evidence crop rectangle so the UI can literally show the soft eye.
- Calibration: scores are mapped through a per-scene isotonic regression fitted on labelled keeper/reject data so 0.8 means the same thing everywhere.

## 7. Cloud AI usage (bring-your-own API key)

No cloud AI call in this phase. The phase must work with the network cable unplugged; the Cloud AI Gateway from Phase 04 stays idle here.

## 8. Implementation order (execute literally, in this order)

1. Build the camera calibration harness and populate `camera_calibration.toml` for the top 20 bodies.
2. Implement classical sharpness measures with per-region support and unit tests on synthetic blur.
3. Label sharp/soft crops and train the focus head; verify on shallow-DOF and backlit fixtures.
4. Implement motion classification with EXIF fusion; validate on panning and dance fixtures.
5. Implement RAW-based exposure verdicts with specular exclusion.
6. Implement noise estimation and ISO/camera normalisation (COL validates).
7. Label eye states including intentional-closed cases; train and calibrate the eye head.
8. Implement the intent rules and the group-photo closed-eye ratio.
9. Compose `technical_score`, fit per-scene calibration, and emit reasons with evidence crops.
10. Build the Integrity card UI showing crops and reasons; run the QA audit.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `MLL` | ML Lead - Vision | Own scoring design, calibration methodology and the conservative-rejection policy | Signed spec | 3 d |
| `SRML` | Senior ML Engineer | Train focus head and eye-state head; export, calibrate, verify parity | Models registered | 7 d |
| `MLR` | ML Research Engineer | Motion-intent research, anisotropy features, ablations vs learned alternatives | Research report | 4 d |
| `COL` | Colour Scientist | RAW headroom measurement per body, noise model validation, specular exclusion rules | Calibration table | 4 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Implement all analysers, flags, scoring, persistence, evidence crops | `integrity` module + tests | 7 d |
| `DATA` | Data Engineer / Dataset Curator | Label sharpness, focus-miss, motion kind, eye state + intent on 20k crops | Labels v1 | 8 d |
| `QAL` | QA Lead - Automation | AUC/F1 gates, synthetic blur suite, false-rejection audit harness | CI gates | 4 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | False-rejection hunt: review every frame the system calls defective on 5 weddings | Audit + bug list | 4 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Integrity card with zoomable evidence crops and reason list | Explain UI part 1 | 3 d |
| `MFE` | Mid-Level Frontend Engineer | Filter chips (soft, blinked, clipped, noisy) and a technical-review queue | UI panels | 2 d |
| `PERF` | Performance Engineer | Keep per-image analysis under budget; fuse passes to avoid re-reading pixels | Benchmark | 2 d |
| `PM` | Product Manager Agent | Approve the intent rules with a working photographer; write the trust policy | Approved rules | 2 d |
| `DOC` | Technical Writer | Document every reason code in user language | Reason-code reference | 2 d |

### 9.1 Handoff chain for this phase

```text
COL calibration + DATA labels -> SRML models -> SRC analysers
                                        |
                                        v
                        MLL calibration/scoring -> SFE/MFE explain UI
                                        |
                       QAIQ false-rejection audit -> PM trust sign-off -> gate
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

- Synthetic blur ladder: monotonic sharpness response; back/front focus correctly signed.
- Subject-focus AUC >= 0.96 on labelled keeper/reject crops; shallow-DOF portraits never flagged soft.
- Blink F1 >= 0.95; intentional-closed-eye false positives <= 2 % on the kiss/vows/hug fixture set.
- Exposure: recoverable vs lost matches expert labels >= 0.93; candle and sparkler frames not flagged.
- Noise: estimated sigma within 15 % of measured sigma on the ISO ladder fixtures.
- Group photos: closed-eye ratio matches human count exactly on 200 labelled group frames.
- Cross-camera fairness: identical scenes shot on 24 MP and 61 MP bodies score within 0.05.

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
| Integrity analysis per image (GPU) | <= 45 ms |
| Integrity analysis per image (CPU) | <= 220 ms |
| 4,000 images total (RTX 4070) | <= 180 s |
| Storage per image | <= 1 KB including per-face eye states |

Telemetry events (local-first, opt-in aggregation):

- `integrity.scored` {images, ms, mean_score, flag_histogram}
- `integrity.eyes` {faces, closed, closed_ok, squint}
- `integrity.camera_uncalibrated` {make, model}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| False rejections destroy trust | Conservative thresholds, relative-within-moment scoring, mandatory QAIQ false-rejection audit, and reasons with visual evidence on every penalty. |
| Intentional motion or closed eyes flagged as defects | Explicit intent classification, scene profiles, PM-approved rules, and dedicated fixture sets. |
| Camera bias favours high-resolution bodies | Per-body calibration table plus a cross-camera fairness test in CI. |
| Uncalibrated new camera | Fallback normalisation by sensor resolution with a telemetry event and a monthly calibration sprint. |

## 13. Acceptance criteria

- [ ] Every image exposes subject sharpness, motion kind, exposure verdict, noise level and per-face eye state with reasons.
- [ ] A shallow-depth-of-field portrait with creamy bokeh is never called soft.
- [ ] A kiss with closed eyes is flagged `EYES_CLOSED_OK`, not as a defect.
- [ ] A camera-shake ceremony frame and a panned exit frame are distinguished correctly.
- [ ] Scores are calibrated per scene so 0.8 means the same thing in a ceremony and on a dance floor.
- [ ] The Integrity card shows the exact crop that caused each penalty.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 09 - Frame Integrity AI: Focus, Motion, Exposure, Noise & Eye State.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-09-FRAME-INTEGRITY-AI.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - Every frame gets an honest technical verdict where it matters: is the *right subject* sharp, is motion intentional, is exposure recoverable, how noisy is it, and are the important eyes open?

Rules:
  - Do not start Phase 10. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-brain-photo/src/integrity/{focus,motion,exposure,noise,eyes,score,flags}.rs`, `crates/aura-brain-photo/src/integrity/calibration.rs` + `config/camera_calibration.toml`, `ml/models/integrity/{train_focus.py,train_eyes.py,eval_integrity.py,export.py}`, `crates/aura-catalog/migrations/0009_integrity.sql`, `apps/desktop/src/components/explain/IntegrityCard.tsx`, `docs/model-cards/{focus_head,eye_state}.md`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-09-frame-integrity-ai and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-09.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-09-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-09-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-09-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 09 of 30 - Frame Integrity AI: Focus, Motion, Exposure, Noise & Eye State - part of the AURA Wedding AI master build plan.*

---

# Phase 10 - Expression, Emotion & Moment Ranking AI

> **Single feature shipped by this phase:** The app finds the moments that matter: genuine smiles, laughter, tears, hugs, kisses, reactions and ritual peaks - and ranks every frame by emotional value.
>
> **Mission:** Give the product taste. Technical quality decides what is acceptable; emotion decides what is *worth delivering*, and this is where the gallery starts to feel like it was chosen by a human.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 10 of 30 |
| Epic | E2 - Wedding Brain |
| Feature | The app finds the moments that matter: genuine smiles, laughter, tears, hugs, kisses, reactions and ritual peaks - and ranks every frame by emotional value. |
| Depends on | Phases 05, 06, 07, 09 |
| Unlocks | Phases 12, 13, 27, 29 |
| Duration | 2.5 weeks |
| Primary owners | ML Lead - Vision, Senior ML Engineer, AI Agent & Prompt Engineer, ML Research Engineer |
| Risk level | High - subjective and culturally sensitive |
| Headline KPI | expression ranking agreement with photographers >= 0.80 pairwise; peak-moment detection recall >= 0.90; tear/laughter detection F1 >= 0.85 |
| Competitor being beaten | FilterPixel moment/expression scoring; Aftershoot expression preference |

## 1. Why this phase exists

Photographers do not sell sharpness, they sell feeling. Any product that automates culling without modelling emotion will consistently deliver technically perfect, emotionally flat galleries - the most common complaint about AI culling today.

Emotion also unlocks the premium features: hero selection, album storytelling and social picks are all emotion-ranking problems. Investing here pays off three more times later.

Emotion must be culturally careful: composure is the norm in many traditions, so 'no big smile' cannot mean 'no emotion'. Scene- and tradition-aware baselines are required.

## 2. Scope contract

### 2.1 In scope

- Per-face expression head: smile intensity, genuineness (Duchenne cue via eye-region activation), laughter, crying/tears, surprise, tenderness, neutral-composed, discomfort/awkward - all continuous, not one-hot.
- Gaze and attention: looking at camera, at partner, at officiant, away; mutual-gaze detection between primary identities.
- Interaction detection at image level: hug, kiss, hand-hold, dance-hold, ring exchange, blessing/touch, toast, tears-being-wiped, group cheer.
- Moment peak detection within each Phase 08 moment: the frame where the action is at maximum expression (kiss apex, tear falling, bouquet released).
- Reaction linking: connect a primary action frame with reaction frames from the same instant (parents crying while the couple kisses) using the timeline and gaze direction.
- `emotion_score` per image: scene-weighted combination of subject expression, interaction significance, peak proximity and reaction value, calibrated against photographer preferences.
- Optional cloud reasoning for narrative significance of a moment when local scores are ambiguous (batched, contact-sheet based).
- Preference learning hook: pairwise comparisons collected from the user feed a lightweight ranker (used fully in Phase 30's learning loop).

### 2.2 Explicitly out of scope (do not build it here)

- Final selection (Phase 12).
- Album sequencing and hero picks (Phase 29).
- Any claim about a person's inner emotional state - the model scores *photographic expression*, not psychology.

## 3. Architecture and data flow

```text
aligned face crops (P06) --> ExpressionHead (multi-output continuous)
                                    |
 full frame (P02) --> InteractionHead (hug/kiss/hold/ritual/toast/cheer)
                                    |
 gaze estimation --> mutual gaze, attention target
                                    |
 moment (P08) --> peak curve over frames --> peak_index, peak_margin
                                    |
 reaction linker (time + gaze + identity role)
                                    |
            emotion_score (scene-weighted, calibrated) + reasons[]
                                    |
     ambiguous? --> Cloud MomentSignificance (P04) --> narrative weight + reasons
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-brain-wedding/src/emotion/{expression,gaze,interaction,peak,reaction,score}.rs` | All emotion analysis. |
| `ml/models/emotion/{train_expression.py,train_interaction.py,train_ranker.py,eval_emotion.py}` | Model training including the preference ranker. |
| `crates/aura-catalog/migrations/0010_emotion.sql` | `face_expression`, `image_interaction`, `moment_peak`, `reaction_links` tables. |
| `config/emotion_weights.toml` | Scene- and tradition-aware weighting, PM-owned. |
| `apps/desktop/src/components/explain/EmotionCard.tsx` | Emotion readout with face crops and interaction labels. |
| `docs/model-cards/{expression_head,interaction_head}.md` | Model cards with cultural-bias analysis. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Emotion contracts (frozen)**

```rust
pub struct FaceExpression {
    pub face_id: FaceId, pub identity: Option<IdentityId>,
    pub smile: f32, pub genuineness: f32, pub laughter: f32, pub tears: f32,
    pub surprise: f32, pub tenderness: f32, pub composed: f32, pub discomfort: f32,
    pub gaze: GazeTarget, pub confidence: f32,
}

pub struct ImageEmotion {
    pub image_id: ImageId,
    pub interactions: Vec<(Interaction, f32)>,   // kind + strength
    pub mutual_gaze: bool,
    pub peak_proximity: f32,                     // 1.0 at the moment's peak frame
    pub reaction_of: Option<ImageId>,            // this frame reacts to that frame
    pub emotion_score: f32,                      // scene-weighted, calibrated
    pub narrative_weight: f32,                   // raised by cloud reasoning when used
    pub reasons: Vec<Reason>,
    pub source: Source,
}
```

## 6. Algorithm, model and implementation design

### 6.1 Expression modelling that respects culture

- Continuous multi-output regression, trained on photographer-ranked wedding faces rather than on generic emotion datasets, because 'delivered vs rejected' is the label that matters.
- Genuineness uses eye-region activation alongside mouth shape so posed grins score lower than real laughter - and this is exposed as a reason, not hidden.
- Composure is a positive class: in `ritual` and `vows` scenes, `composed` with mutual gaze can outscore a smile. Weights come from `emotion_weights.toml` per scene and tradition.
- Tears detection uses eye-region specular/wet cues plus expression context, with deliberately high precision (a wrongly detected tear is embarrassing).

### 6.2 Interaction and peak detection

- Interaction head operates on the full frame with person boxes as spatial priors, predicting the interaction set with strengths.
- Within a moment, build an expression/interaction curve over frames and find the argmax with a smoothing kernel; `peak_margin` records how clearly the peak wins.
- Kiss apex, tear release, bouquet-in-air and ring-slide are trained as explicit peak types because they are the frames clients buy.

### 6.3 Reaction linking (a feature no competitor ships)

- For each high-significance action frame, search +/- 4 s across *all* cameras for frames whose subjects gaze toward the action and show strong expression.
- Link them as `reaction_of`, which lets Phase 12 guarantee that a kiss keeper is accompanied by the mother's tears, and lets Phase 29 build cause-effect album spreads.
- Reactions are scored with a bonus proportional to the action's significance and the reactor's role weight.

### 6.4 Calibration to photographer taste

- Collect pairwise preferences ('which of these two would you deliver?') from photographers on fixture moments; fit a Bradley-Terry ranker over model features.
- Final `emotion_score` is the ranker output, calibrated per scene by isotonic regression - this makes the number comparable to `technical_score` in Phase 12.
- The same mechanism is later reused for per-user personalisation in Phase 30, so the interface is designed for it now.

## 7. Cloud AI usage (bring-your-own API key)

**Narrative significance of an ambiguous moment**

| Aspect | Specification |
|---|---|
| Model class | Vision reasoning tier, temperature 0 |
| Trigger | Moment-level emotion scores within 0.05 of each other, or an unrecognised ritual peak |
| Input sent | Up to 6 thumbnails of the moment (768 px), scene/ritual labels, detected interactions, identity roles (anonymised as 'primary A/B', 'close family') |
| Cost control | <= 25 calls per wedding; batched per moment; cached |
| Offline fallback | Local ranker output only, with `narrative_weight = 0` and lower confidence |

System prompt contract:

```text
You are a wedding photo editor deciding how important a moment is to the wedding story.
Input: frames from one moment, the chapter, detected interactions and anonymised subject roles.
Task: rate narrative significance 0-1, pick the single strongest frame index, and explain in short editorial reasons.
Rules:
- Judge storytelling value: is this a milestone, a peak reaction, a unique moment, or a repeat?
- Do not comment on appearance, body, ethnicity or attractiveness. Never speculate about relationships beyond the given roles.
- Do not describe emotions as psychological facts; describe what is visible.
- Return ONLY JSON matching the schema.
```

Required JSON response schema (validated; invalid = retry once, then fall back to local model):

```json
{
  "type": "object",
  "required": ["significance", "best_index", "confidence", "reasons"],
  "properties": {
    "significance": { "type": "number", "minimum": 0, "maximum": 1 },
    "best_index": { "type": "integer", "minimum": 0 },
    "moment_type": { "type": ["string", "null"] },
    "confidence": { "type": "number", "minimum": 0, "maximum": 1 },
    "reasons": { "type": "array", "items": { "type": "string" }, "maxItems": 5 }
  },
  "additionalProperties": false
}
```

## 8. Implementation order (execute literally, in this order)

1. Define the expression/interaction taxonomy with the photographer consultant; write the cultural-sensitivity rules.
2. Collect labels: face expression regression targets and pairwise 'which would you deliver' comparisons.
3. Train the expression head; validate that composure is not penalised in ritual scenes.
4. Train the interaction head with person-box priors.
5. Implement gaze estimation and mutual-gaze detection.
6. Implement peak detection over moments and validate on kiss/toss/tears fixtures.
7. Implement reaction linking across cameras.
8. Fit the Bradley-Terry ranker and per-scene calibration; wire `emotion_score`.
9. Add the optional cloud significance task with strict anonymisation.
10. Build the Emotion card UI; run the photographer agreement study.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `MLL` | ML Lead - Vision | Own emotion taxonomy, ranker design, calibration and cultural-bias evaluation | Signed spec + bias report | 3 d |
| `SRML` | Senior ML Engineer | Train expression + interaction heads, gaze model integration, export and parity | Models registered | 7 d |
| `MLR` | ML Research Engineer | Peak detection algorithm, reaction-linking heuristics, ranker feature ablations | Research report | 5 d |
| `DATA` | Data Engineer / Dataset Curator | Expression labels, interaction boxes, 10k pairwise photographer comparisons across traditions | Preference dataset | 9 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Implement scoring, peaks, reaction links, persistence, reason generation | `emotion` module + tests | 6 d |
| `AGT` | AI Agent & Prompt Engineer | Cloud `MomentSignificance` task with anonymisation and cassettes | Cloud path live | 2 d |
| `PM` | Product Manager Agent | Own `emotion_weights.toml`, approve the cultural rules, define the 'no psychological claims' policy | Approved config + policy | 2 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Emotion card with face crops, interaction chips, peak indicator | Explain UI part 2 | 3 d |
| `MFE` | Mid-Level Frontend Engineer | Moment browser sorted by emotion, reaction pair viewer | UI panels | 3 d |
| `QAL` | QA Lead - Automation | Agreement study harness, F1 gates, cultural fixture gates, calibration tests | CI gates | 4 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Blind study: 5 photographers rank 300 moments vs the model | Agreement report | 4 d |
| `SEC` | Security & Privacy Engineer | Review that no emotion data leaves the device and that cloud payloads are anonymised | Sign-off | 1 d |
| `DOC` | Technical Writer | Explain emotion scoring honestly in user docs; avoid overclaiming | Docs merged | 2 d |

### 9.1 Handoff chain for this phase

```text
PM taxonomy + cultural rules -> DATA preference labels -> SRML models
                                              |
                                              v
                              MLR peaks/reactions -> SRC scoring -> AGT cloud
                                              |
                                SFE/MFE UI -> QAIQ agreement study -> MLL/PM gate
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

- Pairwise agreement with photographers >= 0.80 on held-out moments.
- Peak detection: chosen frame is within the human-chosen top-2 in >= 90 % of moments.
- Tears/laughter F1 >= 0.85 with precision >= 0.90 (no false tears).
- Composure fairness: in ritual/vows fixtures, composed frames are not systematically ranked below smiling frames.
- Reaction linking: >= 80 % of human-identified reaction pairs found, < 10 % spurious links.
- Determinism: identical inputs produce identical scores; cloud results cached and reproducible.

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
| Expression + interaction per image (GPU) | <= 40 ms |
| 4,000 images total (RTX 4070) | <= 160 s |
| Peak + reaction linking for a whole wedding | <= 8 s |
| Storage per image | <= 900 B |

Telemetry events (local-first, opt-in aggregation):

- `emotion.scored` {images, ms, mean_score, interaction_histogram}
- `emotion.peaks` {moments, mean_margin}
- `emotion.reactions` {links, mean_bonus}
- `emotion.cloud_used` {calls, cost_usd}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Cultural bias toward Western expressiveness | Tradition-aware weights, balanced preference data, mandatory per-tradition agreement metrics, PM-approved rules. |
| Overclaiming emotion recognition | Product language describes photographic expression, not inner states; docs and UI copy reviewed by PM. |
| Subjectivity makes the model feel wrong to some photographers | Preference ranker is personalisable in Phase 30; users can weight emotion vs technical in settings. |
| False tear/crying detection embarrasses the product | High-precision thresholds, cross-check with interaction context, and no tear-based reason text unless confidence >= 0.85. |

## 13. Acceptance criteria

- [ ] Every face carries continuous expression values and gaze; every image carries interactions and an emotion score.
- [ ] Each moment identifies its peak frame with a margin, matching human choice in the large majority of cases.
- [ ] Reaction frames are linked to their action frames across cameras.
- [ ] Composed ritual frames are ranked fairly against smiling frames.
- [ ] The Emotion card explains the score with crops and short editorial reasons.
- [ ] Photographer agreement study meets the gate and is published internally.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 10 - Expression, Emotion & Moment Ranking AI.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-10-EMOTION-MOMENT-AI.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - The app finds the moments that matter: genuine smiles, laughter, tears, hugs, kisses, reactions and ritual peaks - and ranks every frame by emotional value.

Rules:
  - Do not start Phase 11. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-brain-wedding/src/emotion/{expression,gaze,interaction,peak,reaction,score}.rs`, `ml/models/emotion/{train_expression.py,train_interaction.py,train_ranker.py,eval_emotion.py}`, `crates/aura-catalog/migrations/0010_emotion.sql`, `config/emotion_weights.toml`, `apps/desktop/src/components/explain/EmotionCard.tsx`, `docs/model-cards/{expression_head,interaction_head}.md`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-10-emotion-moment-ai and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-10.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-10-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-10-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-10-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 10 of 30 - Expression, Emotion & Moment Ranking AI - part of the AURA Wedding AI master build plan.*

---

# Phase 11 - Composition & Aesthetic AI

> **Single feature shipped by this phase:** Framing intelligence: headroom, horizon tilt, limb and head cropping, subject placement, balance, background clutter, distraction detection and an overall aesthetic score.
>
> **Mission:** Judge photographs the way a picture editor does, so that among six technically equal frames the best-composed one wins - and so Phase 23's smart crop has a target to optimise toward.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 11 of 30 |
| Epic | E2 - Wedding Brain |
| Feature | Framing intelligence: headroom, horizon tilt, limb and head cropping, subject placement, balance, background clutter, distraction detection and an overall aesthetic score. |
| Depends on | Phases 05, 06, 07, 09 |
| Unlocks | Phases 12, 13, 23, 29 |
| Duration | 2 weeks |
| Primary owners | ML Lead - Vision, Senior ML Engineer, ML Research Engineer |
| Risk level | Medium-High - aesthetics are subjective |
| Headline KPI | aesthetic pairwise agreement >= 0.78; limb-crop detection F1 >= 0.90; horizon angle error <= 0.4 deg |
| Competitor being beaten | No competitor exposes composition reasoning; Lightroom only offers manual crop tools |

## 1. Why this phase exists

Composition is the difference between a snapshot and a photograph, and it is fully computable: headroom, tilt, crop violations and balance are geometric facts, while aesthetic preference can be learned from photographer choices.

It also feeds two later features directly - Smart Crop (Phase 23) needs a differentiable-ish objective, and Hero/Album selection (Phase 29) needs an aesthetic ranking that is not just emotion.

## 2. Scope contract

### 2.1 In scope

- Geometric analysis: horizon detection (vanishing-line and gradient-orientation based), vertical-line convergence, headroom ratio, subject placement vs thirds/centre, negative-space balance, tilt classification (intentional dutch angle vs accidental).
- Crop violation detection using body keypoints: cut at joints (wrist, elbow, knee, ankle), top-of-head crop, half-limb crop, third-person edge intrusion.
- Background analysis: clutter density, distracting bright blobs, exit signs/rubbish-bin-class distractions, merging heads with background poles, colour competition with the subject.
- Frame edge audit: objects entering the frame, cropped guest heads at edges, mirror/reflection artefacts.
- Learned aesthetic head trained on photographer pairwise choices, conditioned on scene (a `details` flat-lay is judged differently from a `couple_portrait`).
- `composition_score` plus structured `composition_flags` and per-flag evidence boxes; `crop_suggestion_hint` (region of interest and safe margins) handed to Phase 23.

### 2.2 Explicitly out of scope (do not build it here)

- Actually cropping or straightening (Phase 23).
- Removing distractions (Phase 24).
- Emotional value (Phase 10).

## 3. Architecture and data flow

```text
proxy + person keypoints + faces + scene
   |
   +--> HorizonEstimator -> tilt_deg, intentional?
   +--> KeypointCropAudit -> joint_cuts[], head_crop, edge_intrusions[]
   +--> PlacementAnalyser -> headroom, thirds_offset, balance, negative_space
   +--> BackgroundAnalyser -> clutter, bright_blobs[], head_merge, colour_competition
   +--> AestheticHead (scene-conditioned, learned from photographer pairs)
                       |
                       v
   composition_score + composition_flags + evidence boxes + crop_suggestion_hint
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-brain-photo/src/composition/{horizon,keypoints,crop_audit,placement,background,aesthetic,score}.rs` | All composition analysis. |
| `ml/models/composition/{train_aesthetic.py,eval_composition.py,export.py}` | Aesthetic head training. |
| `crates/aura-catalog/migrations/0011_composition.sql` | `image_composition` table with flags and evidence. |
| `config/composition_rules.toml` | Scene-specific headroom bands, tilt tolerance, crop-violation severity. |
| `apps/desktop/src/components/explain/CompositionCard.tsx` | Overlay showing thirds, horizon, violations. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Composition result (frozen)**

```rust
pub struct CompositionResult {
    pub image_id: ImageId,
    pub tilt_deg: f32, pub tilt_intentional: bool, pub horizon_conf: f32,
    pub headroom: f32,                  // subject top to frame top, fraction of height
    pub thirds_offset: f32,             // distance of main subject from nearest power point
    pub balance: f32,                   // 0 = lopsided, 1 = balanced
    pub joint_cuts: Vec<JointCut>,      // { joint, severity, box }
    pub head_crop: bool, pub edge_intrusions: Vec<Box2>,
    pub clutter: f32, pub bright_blobs: Vec<Box2>, pub head_merge: bool,
    pub aesthetic: f32,                 // learned, scene-conditioned
    pub composition_score: f32,         // fused, calibrated
    pub crop_suggestion_hint: Option<CropHint>,
    pub reasons: Vec<Reason>,
}
```

## 6. Algorithm, model and implementation design

### 6.1 Geometry first, learning second

- Horizon: estimate dominant line orientation with a weighted Hough transform on long edges, cross-checked against gravity metadata when the camera records it; report confidence and never straighten below 0.6 confidence.
- Intentional tilt: a large tilt (> 6 deg) combined with a centred subject, no strong horizon and a `candid`/`dance_floor` scene is treated as style, not error.
- Crop violations use body keypoints: cutting at a joint scores worse than cutting mid-limb, and top-of-head crops are only violations outside `couple_portrait` close-ups where they can be deliberate.
- Headroom is compared to scene-specific bands from `composition_rules.toml` (a `family_portrait` wants more headroom than a `dance_floor` frame).

### 6.2 Background and distraction analysis

- Segment background (Phase 18's masks are not available yet, so use a light saliency model here) and measure edge density, colour variance and the count of high-luminance blobs above the subject's luminance.
- Head-merge detection: check for vertical structures within a small radius of head centroids - the classic 'pole growing out of the head' error.
- Colour competition: compare background chroma clusters against subject skin/clothing chroma; a saturated red exit sign behind a white dress is flagged.

### 6.3 Learned aesthetics, honestly scoped

- Train a scene-conditioned pairwise ranker on photographer choices from Phase 10's preference collection (reused labels, extra composition-focused pairs).
- Feature inputs include the geometric measures, so the model learns *how much* each violation matters rather than re-deriving geometry.
- Aesthetic score is capped in influence: Phase 12 weights it below technical integrity and emotion, because taste should break ties, not override substance.

## 7. Cloud AI usage (bring-your-own API key)

No cloud AI call in this phase. The phase must work with the network cable unplugged; the Cloud AI Gateway from Phase 04 stays idle here.

## 8. Implementation order (execute literally, in this order)

1. Author `composition_rules.toml` with the photographer consultant.
2. Implement horizon estimation and validate against a labelled tilt set.
3. Integrate a body-keypoint model and implement the crop audit.
4. Implement placement and balance measures with scene bands.
5. Implement background clutter, bright blobs, head-merge and colour competition.
6. Collect composition-focused pairwise labels and train the aesthetic head.
7. Fuse and calibrate `composition_score`; emit reasons with evidence boxes.
8. Build the composition overlay UI (thirds grid, horizon line, violation boxes).
9. Produce `crop_suggestion_hint` for Phase 23 and validate on portrait fixtures.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `MLL` | ML Lead - Vision | Own the composition metric set, scoring fusion and calibration | Signed spec | 2 d |
| `MLR` | ML Research Engineer | Horizon estimator research, intentional-tilt heuristics, aesthetic feature ablations | Research report | 4 d |
| `SRML` | Senior ML Engineer | Keypoint model integration, aesthetic head training, export and parity | Models registered | 5 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Implement all analysers, flags, evidence boxes, persistence, crop hints | `composition` module | 6 d |
| `DATA` | Data Engineer / Dataset Curator | Tilt labels, crop-violation labels, 4k composition pairwise comparisons | Labels v1 | 6 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Composition overlay with thirds, horizon and violation boxes | Explain UI part 3 | 3 d |
| `QAL` | QA Lead - Automation | Angle error, F1 and agreement gates; intentional-tilt regression fixtures | CI gates | 3 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Audit 300 frames: are flagged violations real, and are unflagged frames clean? | Audit report | 3 d |
| `PM` | Product Manager Agent | Approve rule bands and decide how much aesthetics may influence culling | Approved weights | 1 d |
| `PERF` | Performance Engineer | Keep composition analysis under 30 ms per image; share keypoint inference with Phase 06 | Benchmark | 2 d |
| `DOC` | Technical Writer | Document composition reason codes with example images | Docs merged | 2 d |

### 9.1 Handoff chain for this phase

```text
PM rules -> MLR geometry research -> SRC analysers -> SRML aesthetic head
                                   |
                        SFE overlay UI -> QAIQ audit -> MLL gate -> Phase 12/23 consumers
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

- Horizon angle error <= 0.4 deg on labelled architecture and seascape-style fixtures.
- Intentional dutch angles are not flagged as tilt errors (dedicated fixture set).
- Limb/joint crop detection F1 >= 0.90; top-of-head crop correctly context-dependent.
- Head-merge detection finds >= 85 % of labelled cases with < 10 % false positives.
- Aesthetic pairwise agreement >= 0.78 on held-out photographer comparisons.
- Composition score never dominates: a beautifully composed out-of-focus frame still loses in Phase 12 (integration test).

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
| Composition analysis per image (GPU) | <= 30 ms |
| 4,000 images total | <= 120 s |
| Storage per image | <= 800 B |

Telemetry events (local-first, opt-in aggregation):

- `composition.scored` {images, ms, mean_score, flag_histogram}
- `composition.tilt` {mean_abs_deg, intentional_ratio}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Rigid rules punish creative work | Intentional-style detection, scene-specific bands, capped influence on culling, and a global 'composition strictness' setting. |
| Aesthetic model encodes one photographer's taste | Multi-photographer labels, per-scene conditioning, and personalisation in Phase 30. |
| Saliency-based background analysis is weak | Re-validate after Phase 18 ships real masks and upgrade the background analyser then (tracked as a follow-up task). |

## 13. Acceptance criteria

- [ ] Every image carries tilt, headroom, placement, balance, clutter and violation flags with evidence boxes.
- [ ] The overlay shows exactly why a frame was marked badly composed.
- [ ] Creative tilts and deliberate tight crops are respected.
- [ ] Crop hints are available for Phase 23 on all portrait-class frames.
- [ ] Agreement and geometric accuracy gates pass in CI.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 11 - Composition & Aesthetic AI.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-11-COMPOSITION-AESTHETIC-AI.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - Framing intelligence: headroom, horizon tilt, limb and head cropping, subject placement, balance, background clutter, distraction detection and an overall aesthetic score.

Rules:
  - Do not start Phase 12. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-brain-photo/src/composition/{horizon,keypoints,crop_audit,placement,background,aesthetic,score}.rs`, `ml/models/composition/{train_aesthetic.py,eval_composition.py,export.py}`, `crates/aura-catalog/migrations/0011_composition.sql`, `config/composition_rules.toml`, `apps/desktop/src/components/explain/CompositionCard.tsx`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-11-composition-aesthetic-ai and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-11.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-11-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-11-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-11-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 11 of 30 - Composition & Aesthetic AI - part of the AURA Wedding AI master build plan.*

---

# Phase 12 - Autonomous Culling Engine, Story Coverage Guard & Gallery Sizing

> **Single feature shipped by this phase:** One decision engine turns 3,000 frames into a delivered gallery: per-moment winners, scene quotas, coverage guarantees, duplicate suppression and an automatically chosen gallery size.
>
> **Mission:** Make the single most valuable decision in the product defensibly, transparently and fast - and never lose an important part of the wedding story while doing it.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 12 of 30 |
| Epic | E2 - Wedding Brain |
| Feature | One decision engine turns 3,000 frames into a delivered gallery: per-moment winners, scene quotas, coverage guarantees, duplicate suppression and an automatically chosen gallery size. |
| Depends on | Phases 07-11 |
| Unlocks | Phases 13, 14, 27, 28, 29, 30 |
| Duration | 3 weeks |
| Primary owners | ML Lead - Vision, Senior Engineer - Core Pipeline (Rust), Product Manager Agent, ML Research Engineer |
| Risk level | Critical - this is the product |
| Headline KPI | photographer agreement on keepers >= 0.85; missed-must-have rate = 0 on fixtures; 3,000-image cull end-to-end < 8 min |
| Competitor being beaten | Aftershoot, Imagen, FilterPixel, Narrative Select - all of them, directly |

## 1. Why this phase exists

Culling is where photographers lose 6-10 hours per wedding, and it is the decision they trust least to software. Winning here wins the product; a single lost 'must-have' frame loses a customer forever.

Competitors cull frame-by-frame with global thresholds. Culling as *constrained optimisation over moments and chapters* - with coverage guarantees, quotas and diversity - is a structurally better approach and is defensible in marketing because the reasoning is inspectable.

Automatic gallery sizing removes the last configuration burden: photographers should not have to guess whether to deliver 600 or 1,100 images; the story and the shoot volume determine it.

## 2. Scope contract

### 2.1 In scope

- Unified per-image `keep_score` fusing technical integrity (P09), emotion (P10), composition (P11), subject prominence (P06) and scene weights (P07), with per-scene calibration.
- Moment-level selection: pick 1..k winners per moment where k depends on diversity, moment significance and scene profile; suppress near-identical frames.
- Chapter quotas: target counts per chapter derived from shot volume, chapter duration and importance, with min/max bands.
- Story Coverage Guard: a hard rule set of must-have moments (rings, vows, kiss, first look, entrances, cake, first dance, family formals, each close-family member, venue establishing shot, exit) that cannot be empty if any candidate exists at all.
- Coverage of people: every `family_close` identity must appear at least N times; every `guest` identity appearing more than M times should appear at least once.
- Diversity constraints: avoid 40 near-identical dance frames; enforce spread across time, framing (wide/medium/tight) and identity mix.
- Automatic gallery sizing: predicted deliverable count from a regression over shoot volume, chapter count, moment count and quality distribution, with user-adjustable target and a live slider that re-runs selection in under 2 s.
- Three autonomy modes: `Conservative` (keeps more, flags more), `Balanced`, `Aggressive` (tight gallery), plus `Zero-Touch` behaviour defined in Phase 28.
- Rejection reasons for every rejected frame and 'runner-up' pointers for every kept frame (used by QC in Phase 27 for replacements).
- Deterministic, resumable, incremental: re-running with the same inputs gives identical output; changing one weight re-selects in seconds without re-analysis.

### 2.2 Explicitly out of scope (do not build it here)

- Editing the selected frames (Phase 14+).
- QC-driven replacement (Phase 27 consumes runner-ups).
- Hero/album/social picks (Phase 29).

## 3. Architecture and data flow

```text
per-image scores: integrity, emotion, composition, prominence
        |
        v
  keep_score fusion (scene-weighted, calibrated)
        |
        v
  MOMENT PASS: winners per moment (k from diversity + significance)
        |
        v
  CHAPTER PASS: quotas per chapter (duration x volume x importance)
        |
        v
  COVERAGE GUARD: must-have moments + per-identity minimums (hard constraints)
        |
        v
  DIVERSITY PASS: framing/time/identity spread, near-duplicate suppression
        |
        v
  SIZING: target count reconciliation (add best runner-ups / drop weakest)
        |
        v
  selection[] + rejection reasons[] + runner_up[] + coverage report
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-cull/src/{lib,fusion,moment_pass,chapter_pass,coverage,diversity,sizing,modes,explain}.rs` | The culling engine. |
| `config/cull_weights.toml` + `config/coverage_rules.toml` | PM-owned weights and must-have rules. |
| `crates/aura-catalog/migrations/0012_selection.sql` | `selection`, `rejections`, `coverage_report` tables. |
| `apps/desktop/src/routes/cull/{CullView,SizeSlider,CoveragePanel,RejectReasons}.tsx` | Culling UI. |
| `ml/eval/cull_agreement.py` | Photographer agreement harness. |
| `tests/fixtures/labels/keepers_*.json` | Human keeper ground truth per fixture wedding. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Selection contracts (frozen)**

```rust
pub struct KeepScore {
    pub image_id: ImageId,
    pub technical: f32, pub emotion: f32, pub composition: f32, pub prominence: f32,
    pub scene_weighted: f32,          // final keep_score 0..1
    pub calibration_ver: u16,
}

pub struct SelectionResult {
    pub selected: Vec<Selected>,      // ordered by timeline
    pub rejected: Vec<Rejected>,
    pub coverage: CoverageReport,
    pub target_count: u32, pub actual_count: u32,
    pub mode: CullMode, pub deterministic_hash: u64,
}

pub struct Selected {
    pub image_id: ImageId, pub moment_id: MomentId,
    pub keep_score: f32, pub confidence: f32,
    pub reasons: Vec<Reason>,
    pub runner_up: Option<ImageId>,   // best alternative in the same moment
    pub coverage_role: Option<MustHave>,  // why it is protected, if it is
}

pub struct CoverageReport {
    pub must_haves: Vec<(MustHave, Coverage)>,   // Covered | CoveredWeak | Missing(no candidates)
    pub identity_coverage: Vec<(IdentityId, u32)>,
    pub chapter_counts: Vec<(ChapterId, u32, u32)>, // actual, target
    pub warnings: Vec<String>,
}
```

## 6. Algorithm, model and implementation design

### 6.1 Score fusion that cannot be gamed by one signal

- `keep_score = w_t*technical^a * w_e*emotion^b * w_c*composition^c` in log space with scene-specific weights, so a catastrophic technical failure cannot be rescued by emotion and vice versa.
- Hard vetoes: subject completely out of focus, primary identity's eyes closed without intent in a posed scene, or exposure `lost` - these bypass fusion and reject with a clear reason.
- Hard promotions: unique must-have moments with no alternative are protected even at lower scores, and the reason says so honestly ('only frame of the ring exchange').
- Per-scene isotonic calibration so `keep_score` thresholds mean the same thing everywhere.

### 6.2 Moment and chapter passes

- Within a moment, rank by `keep_score`; k = 1 for low-diversity moments, up to 3-5 for high-diversity, high-significance moments (dance sequences, bouquet toss, ritual peaks).
- Guarantee the moment's peak frame (Phase 10) is either selected or explicitly rejected with a reason - never silently dropped.
- Chapter targets: `target_c = round(total_target * importance_c * sqrt(volume_c) / sum(...))`, clamped by min/max bands from `coverage_rules.toml`; details/venue chapters get small fixed allocations.
- Solve the allocation with a greedy pass followed by a bounded local-search improvement (swap in a runner-up if it raises total score without breaking constraints).

### 6.3 Story Coverage Guard - the anti-catastrophe layer

- Must-have rules are declarative: `{ id: 'kiss', match: scene in [kiss] or interaction=kiss, min: 2, prefer: peak }`.
- For each rule, if the selection contains fewer than `min` frames, force-add the best available candidates even if below threshold, marking them `CoveredWeak` with a visible warning.
- If no candidate exists at all (the photographer never shot it), report `Missing` with 'no candidates found' - the product must be clear that it cannot invent coverage.
- Identity coverage: every close-family identity gets >= 3 frames, every recurring guest >= 1, resolved by adding their best frames; this is the feature that prevents 'my aunt isn't in the gallery' complaints.

### 6.4 Sizing, modes and determinism

- Predict the deliverable count with a small regression trained on real delivered galleries (features: total frames, moments, chapters, hours, keeper-score distribution); typical output 22-38 % of shot volume.
- The size slider re-runs only the allocation passes (not analysis), so it feels instant; the coverage guard always runs last so shrinking never breaks must-haves.
- Modes shift thresholds and k-values, never the coverage rules - even `Aggressive` cannot drop a must-have.
- Determinism: stable sorts, integer seeds, and a `deterministic_hash` over inputs+config recorded in the ledger so a support case can be reproduced exactly.

## 7. Cloud AI usage (bring-your-own API key)

**Break genuine ties inside a moment when local scores are statistically indistinguishable**

| Aspect | Specification |
|---|---|
| Model class | Vision reasoning tier, temperature 0 |
| Trigger | Top-2 candidates within 0.02 `keep_score` AND the moment is significant (peak or must-have) |
| Input sent | 2-6 thumbnails (768 px) with their score breakdowns, scene, and anonymised subject roles |
| Cost control | <= 30 calls per wedding; cached; skipped entirely when cloud is off |
| Offline fallback | Deterministic tie-break: higher subject sharpness, then peak proximity, then earlier timestamp |

System prompt contract:

```text
You are a wedding photo editor choosing which single frame to deliver from a set of near-identical candidates.
Input: candidate thumbnails with technical/emotion/composition sub-scores, the chapter, and anonymised roles.
Task: choose the best index and justify it in short editorial reasons.
Rules:
- Prefer open eyes on the primary subjects unless the moment justifies closed eyes (kiss, prayer, tears).
- Prefer peak expression and cleaner framing; ignore small exposure differences (they will be corrected later).
- Never comment on appearance or attractiveness.
- If truly equivalent, say so and return the first index with low confidence.
- Return ONLY JSON matching the schema.
```

Required JSON response schema (validated; invalid = retry once, then fall back to local model):

```json
{
  "type": "object",
  "required": ["best_index", "confidence", "reasons"],
  "properties": {
    "best_index": { "type": "integer", "minimum": 0 },
    "equivalent": { "type": "boolean" },
    "confidence": { "type": "number", "minimum": 0, "maximum": 1 },
    "reasons": { "type": "array", "items": { "type": "string" }, "maxItems": 5 }
  },
  "additionalProperties": false
}
```

## 8. Implementation order (execute literally, in this order)

1. Author `cull_weights.toml` and `coverage_rules.toml` with the photographer consultant; PM signs off before code.
2. Implement score fusion with vetoes and promotions; fit per-scene calibration on labelled keepers.
3. Implement the moment pass with diversity-driven k and peak protection.
4. Implement chapter quotas and the greedy + local-search allocation.
5. Implement the Coverage Guard with declarative rules and `CoveredWeak`/`Missing` semantics.
6. Implement identity coverage and the diversity pass.
7. Train the gallery-size regression on real delivered galleries; wire the live slider.
8. Implement the three modes, determinism hashing and full rejection reasoning.
9. Add the optional cloud tie-breaker.
10. Build the culling UI: selection grid, size slider, coverage panel, rejection reasons, one-click override.
11. Run the photographer agreement study and the zero-missed-must-have gate.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `PM` | Product Manager Agent | Own must-have rules, mode definitions and gallery-size policy; run the consultant sessions | Approved rule set | 4 d |
| `MLL` | ML Lead - Vision | Own fusion maths, calibration, veto policy and the agreement evaluation design | Signed spec + gates | 4 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Implement fusion, moment/chapter passes, coverage guard, diversity, sizing, modes, determinism | `aura-cull` + tests | 10 d |
| `MLR` | ML Research Engineer | Tune k-selection, quota formula, local search, and the size regression | Tuning report | 5 d |
| `AGT` | AI Agent & Prompt Engineer | Cloud tie-breaker task, batching, caching, cassettes | Cloud path live | 2 d |
| `DATA` | Data Engineer / Dataset Curator | Human keeper labels for 8 weddings + 60 real delivered galleries for size regression | Labels + dataset | 9 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Culling view, size slider with instant re-selection, coverage panel, override actions | Cull UI | 6 d |
| `MFE` | Mid-Level Frontend Engineer | Rejection-reason drawer, runner-up compare, mode switcher, warning banners | UI panels | 4 d |
| `QAL` | QA Lead - Automation | Agreement harness, zero-missed-must-have gate, determinism test, slider performance test | CI gates | 5 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Blind study: photographers cull 4 weddings manually, compare against engine output | Study report | 6 d |
| `PERF` | Performance Engineer | Ensure full cull (analysis + selection) of 3,000 images stays under 8 minutes | Benchmark report | 3 d |
| `EM` | Engineering Manager / Delivery Lead Agent | Coordinate the cross-team integration of Phases 05-11 outputs; unblock contract mismatches | Integration log | 3 d |
| `DOC` | Technical Writer | Write 'How AURA culls', the coverage guarantee page and the mode guide | Docs merged | 3 d |

### 9.1 Handoff chain for this phase

```text
PM rules + MLL fusion spec -> SRC engine -> MLR tuning
                                  |                |
                                  v                v
                        AGT tie-breaker      DATA keeper labels
                                  |
                    SFE/MFE cull UI -> QAIQ blind study -> PERF budget -> CTO/PM release gate
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

- Photographer agreement on keepers >= 0.85 (Jaccard on moment level) across 4 blind-study weddings.
- Zero missed must-haves on all fixtures whenever candidates exist; `Missing` reported honestly when they do not.
- Every close-family identity appears >= 3 times in every fixture gallery.
- Determinism: identical inputs produce identical `deterministic_hash` and identical selection across runs and machines.
- Size slider from 500 to 1,200 re-selects in <= 2 s and never breaks coverage.
- Aggressive mode still satisfies all coverage rules.
- Every rejected frame has at least one human-readable reason; every kept frame has a runner-up where one exists.

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
| Selection passes on 4,000 analysed images | <= 1.5 s |
| Full analysis + cull, 3,000 images (RTX 4070) | < 8 min |
| Full analysis + cull, 3,000 images (M3 Pro) | < 14 min |
| Slider re-selection | <= 2 s |

Telemetry events (local-first, opt-in aggregation):

- `cull.completed` {images, selected, target, mode, ms, coverage_warnings}
- `cull.veto` {reason_code, count}
- `cull.coverage_weak` {rule, count}
- `cull.user_override` {action: keep|reject, keep_score, reason_codes}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| A missed must-have moment loses the customer | Hard coverage constraints that run last, forced weak coverage with warnings, blind-study gate, and a permanent CI test. |
| Over-culling makes galleries feel thin | Size regression trained on real deliveries, conservative default mode, and an instant slider so photographers can adjust without re-analysis. |
| Weight tuning becomes a black art | All weights in signed config with rationale, agreement harness re-run on every change, and calibration versioning. |
| Users distrust automation | Full rejection reasons, runner-up comparison, one-click override that is remembered by the learning loop (Phase 30). |
| Performance regression as phases add analysis | PERF owns a standing 8-minute budget test in CI on the reference machine. |

## 13. Acceptance criteria

- [ ] Clicking Cull on a 3,000-image wedding produces a complete gallery in under 8 minutes on the reference machine.
- [ ] The coverage panel shows every must-have as covered, weakly covered or genuinely missing.
- [ ] Every decision is explained, and every keeper offers a runner-up to compare.
- [ ] Moving the size slider instantly re-selects without breaking coverage.
- [ ] Two runs on two machines with the same inputs produce byte-identical selections.
- [ ] Blind-study agreement meets the gate and the report is archived.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 12 - Autonomous Culling Engine, Story Coverage Guard & Gallery Sizing.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-12-CULLING-ENGINE-COVERAGE.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - One decision engine turns 3,000 frames into a delivered gallery: per-moment winners, scene quotas, coverage guarantees, duplicate suppression and an automatically chosen gallery size.

Rules:
  - Do not start Phase 13. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-cull/src/{lib,fusion,moment_pass,chapter_pass,coverage,diversity,sizing,modes,explain}.rs`, `config/cull_weights.toml` + `config/coverage_rules.toml`, `crates/aura-catalog/migrations/0012_selection.sql`, `apps/desktop/src/routes/cull/{CullView,SizeSlider,CoveragePanel,RejectReasons}.tsx`, `ml/eval/cull_agreement.py`, `tests/fixtures/labels/keepers_*.json`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-12-culling-engine-coverage and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-12.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-12-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-12-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-12-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 12 of 30 - Autonomous Culling Engine, Story Coverage Guard & Gallery Sizing - part of the AURA Wedding AI master build plan.*

---

# Phase 13 - Explain My Edit, Confidence Calibration & Decision Ledger

> **Single feature shipped by this phase:** Every decision in the app can be opened up: why this frame was kept, why it was edited this way, how confident the system is, what evidence was used - and every decision is recorded in an auditable ledger.
>
> **Mission:** Convert automation into trust. Calibrated confidence is also the mechanism that makes Zero-Touch mode safe, because autonomy thresholds are only meaningful if the numbers are honest.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 13 of 30 |
| Epic | E2 - Wedding Brain |
| Feature | Every decision in the app can be opened up: why this frame was kept, why it was edited this way, how confident the system is, what evidence was used - and every decision is recorded in an auditable ledger. |
| Depends on | Phases 04, 09-12 |
| Unlocks | Phases 27, 28, 30 |
| Duration | 2 weeks |
| Primary owners | ML Lead - Vision, AI Agent & Prompt Engineer, Senior Frontend Engineer (Tauri + React), Senior Engineer - Core Pipeline (Rust) |
| Risk level | Medium - but critical for adoption |
| Headline KPI | expected calibration error <= 0.05; explanation available for 100 % of decisions; ledger replay reproduces any decision exactly |
| Competitor being beaten | FilterPixel score-and-reason explanations; nobody explains editing decisions |

## 1. Why this phase exists

Photographers will not hand over a client's wedding to a black box. An explanation that shows the actual crop of the soft eye, the reference frames used for a colour correction and the alternative that nearly won converts scepticism into confidence.

Calibration is a safety mechanism, not a nicety: the confidence bands that drive autonomy (98-100 % auto-approve, 90-98 % Zero-Touch, 75-90 % optional review, < 75 % review) are only defensible if 90 % confidence really means 90 % correct.

A decision ledger makes support and improvement possible: any complaint becomes reproducible, and the learning loop in Phase 30 needs the record of what was decided and what the user changed.

## 2. Scope contract

### 2.1 In scope

- Unified `Reason` and `Decision` model used by every phase: code, human text, weight, evidence (crop rect, reference frame ids, parameter deltas), source (local/cloud/user), confidence.
- Decision ledger: append-only table recording every automated decision with inputs hash, config versions, model versions, outputs, confidence and timing.
- Confidence calibration service: per-decision-type isotonic/temperature calibration fitted on labelled outcomes, with an ECE/Brier dashboard and a CI gate.
- Autonomy bands and the policy engine that maps calibrated confidence to `auto_apply`, `auto_apply_zero_touch`, `suggest_review`, `require_review`.
- Explain UI: a single panel with tabs (Selection, Technical, Emotion, Composition, Edit, QC) showing reasons with visual evidence, alternatives and the exact parameters applied.
- Natural-language summaries generated from structured evidence (cloud when available, deterministic templates otherwise) - never invented reasoning.
- Replay tooling: `aura replay <decision_id>` re-runs a decision from the ledger and asserts an identical outcome.
- Support bundle export: anonymised ledger slice + config + model versions, with no image pixels unless the user opts in.

### 2.2 Explicitly out of scope (do not build it here)

- The QC agent itself (Phase 27 writes to this ledger).
- Learning from user corrections (Phase 30 reads this ledger).
- Telemetry upload infrastructure (opt-in, defined in Phase 30).

## 3. Architecture and data flow

```text
every phase --> Decision { type, inputs_hash, outputs, reasons[], raw_confidence, versions }
                        |
                        v
           CalibrationService (per decision type) --> calibrated_confidence
                        |
                        v
                AutonomyPolicy --> auto | auto_zero_touch | suggest | require_review
                        |
            +-----------+------------------------------+
            v                                          v
     DecisionLedger (append-only, replayable)     Explain UI (tabs + evidence)
            |                                          |
     aura replay <id> (exact reproduction)      NL summary (cloud or template)
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-explain/src/{lib,reason,decision,ledger,calibration,policy,summary,replay,bundle}.rs` | Explainability core. |
| `crates/aura-catalog/migrations/0013_ledger.sql` | `decisions`, `decision_reasons`, `calibration_models` tables. |
| `config/autonomy_bands.toml` | Confidence thresholds per decision type, PM-owned. |
| `apps/desktop/src/components/explain/{ExplainPanel,ReasonRow,EvidenceCrop,AlternativeCompare}.tsx` | Explain UI. |
| `ml/eval/calibration_report.py` | ECE/Brier/reliability diagrams. |
| `tools/aura-cli/src/replay.rs` | Decision replay command. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Decision and reason model (frozen, used by every phase)**

```rust
pub struct Reason {
    pub code: &'static str,        // stable, documented, translatable
    pub text: String,              // human sentence, localisable
    pub weight: f32,               // contribution to the decision
    pub evidence: Evidence,        // Crop(Box2) | Frames(Vec<ImageId>) | Params(Vec<(String,f32)>) | None
}

pub struct Decision {
    pub id: DecisionId, pub kind: DecisionKind,   // Cull | Edit | Retouch | Qc | Curate | Export
    pub subject: DecisionSubject,                 // Image | Moment | Segment | Gallery
    pub inputs_hash: u64,
    pub outputs_json: String,
    pub reasons: Vec<Reason>,
    pub raw_confidence: f32, pub calibrated_confidence: f32,
    pub autonomy: Autonomy,                       // Auto | AutoZeroTouch | Suggest | RequireReview
    pub source: Source, pub model_versions: Vec<(String, u16)>,
    pub config_versions: Vec<(String, u16)>,
    pub ms: u32, pub created_at: Timestamp,
}

pub trait Explainable { fn decision(&self) -> Decision; }
```

## 6. Algorithm, model and implementation design

### 6.1 Calibration methodology

- For each decision type, collect (raw_confidence, correct?) pairs from labelled fixtures plus user overrides, then fit isotonic regression (or temperature scaling where monotonic and data-poor).
- Report ECE, Brier score and reliability diagrams; CI fails if ECE > 0.05 for any decision type with >= 500 samples.
- Cloud-sourced decisions are calibrated separately from local ones, because their error profile differs.
- Calibration models are versioned artefacts shipped with the app and refreshed with each model release.

### 6.2 Explanations that are grounded, not generated

- Reasons are always produced by the deciding code with real evidence; the language model may only *rephrase* them into prose, never add facts.
- Every reason code has a documented meaning, a severity, and a localisable sentence template - the reason-code reference is a public doc.
- The Edit tab shows literal parameter deltas ('Temperature -610 K, Exposure +0.42 EV') and which masks were applied, matching what the develop engine actually executed.
- The Selection tab shows the runner-up side by side with its score breakdown, which is the single most trust-building screen in the product.

### 6.3 Ledger and replay

- Append-only, never updated; corrections create new decisions that supersede old ones, preserving history.
- `inputs_hash` covers analysis outputs, config versions and model versions, so replay can assert determinism and detect drift after an upgrade.
- Ledger size is bounded (roughly 3-6 KB per image for a full pipeline); compaction keeps the newest decision per subject plus all user overrides.

### 6.4 Autonomy policy

- Bands from `autonomy_bands.toml`: >= 0.98 auto; 0.90-0.98 auto only in Zero-Touch; 0.75-0.90 suggest review; < 0.75 require review.
- Risk multipliers: destructive or irreversible actions (generative fill, replacement of a selected frame) require a higher band than reversible parameter edits.
- Any decision touching a must-have moment is raised one band, because the cost of being wrong is higher.

## 7. Cloud AI usage (bring-your-own API key)

**Turn structured reasons into a short, warm, accurate explanation paragraph**

| Aspect | Specification |
|---|---|
| Model class | Balanced tier text model, temperature 0 |
| Trigger | On demand when the user opens the Explain panel and requests a summary, or in batch for the delivery report |
| Input sent | The structured reason list with codes, weights, parameter deltas and scores - no images |
| Cost control | <= 40 short calls per wedding; cached per decision id |
| Offline fallback | Deterministic sentence templates assembled from reason codes (always available offline) |

System prompt contract:

```text
You explain photo-editing decisions to a professional wedding photographer.
You will receive a structured list of reasons with codes, weights and numeric parameters.
Task: write 2-4 short sentences explaining the decision in the photographer's own vocabulary.
Rules:
- Use ONLY the supplied facts and numbers. Never add a reason that is not in the input. Never invent numbers.
- Be specific: name the scene, the parameters and the alternative frame if provided.
- Neutral professional tone, no marketing language, no apologies.
- Return ONLY JSON matching the schema.
```

Required JSON response schema (validated; invalid = retry once, then fall back to local model):

```json
{
  "type": "object",
  "required": ["summary"],
  "properties": {
    "summary": { "type": "string", "maxLength": 600 },
    "headline": { "type": ["string", "null"], "maxLength": 90 }
  },
  "additionalProperties": false
}
```

## 8. Implementation order (execute literally, in this order)

1. Freeze the `Reason`/`Decision` model and refactor Phases 09-12 to emit it (this is why it lands now, not later).
2. Implement the ledger with append-only semantics and compaction.
3. Build the calibration harness, fit initial models, and publish the reliability report.
4. Implement the autonomy policy engine with risk multipliers.
5. Author the reason-code reference with user-facing text for every code.
6. Build the Explain panel with all tabs, evidence crops and alternative comparison.
7. Add the NL summary cloud task with template fallback.
8. Implement `aura replay` and the anonymised support bundle export.
9. Add CI gates: ECE, 100 % explanation coverage, replay determinism.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `MLL` | ML Lead - Vision | Own calibration methodology, ECE gates and per-type calibration models | Calibration report | 4 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Ledger, compaction, replay, inputs hashing, support bundle | `aura-explain` core | 6 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Refactor Phases 09-12 to emit the unified reason model | Refactor merged | 3 d |
| `AGT` | AI Agent & Prompt Engineer | NL summary task with strict grounding rules, template fallback, cassettes | Summary path live | 2 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Explain panel with tabs, evidence crops, alternative compare, parameter table | Explain UI | 6 d |
| `MFE` | Mid-Level Frontend Engineer | Confidence badges across the grid, review queues per band, 'why?' shortcut | UI integration | 4 d |
| `PM` | Product Manager Agent | Own `autonomy_bands.toml`, approve reason-code wording, define review-queue UX | Approved config + copy | 3 d |
| `QAL` | QA Lead - Automation | ECE gate, explanation-coverage gate, replay determinism test, ledger growth test | CI gates | 4 d |
| `DEVOPS` | DevOps / Release Engineer | Ship calibration artefacts with releases; wire the reliability dashboard | Pipeline update | 2 d |
| `SEC` | Security & Privacy Engineer | Ensure support bundles are anonymised and pixel-free by default | Sign-off | 1 d |
| `DOC` | Technical Writer | Publish the reason-code reference and the 'how confidence works' page | Docs merged | 3 d |

### 9.1 Handoff chain for this phase

```text
MLL calibration -> SRC ledger/policy -> refactor of P09-12 emitters
                                |
                                v
                    SFE/MFE Explain UI <- PM copy approval
                                |
                 QAL gates + DEVOPS artefacts -> CTO gate -> Phases 27/28/30
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

- Every decision produced by any phase has at least one reason and a calibrated confidence (coverage gate = 100 %).
- ECE <= 0.05 per decision type on held-out labelled outcomes; reliability diagrams archived.
- `aura replay` reproduces stored outcomes exactly on the reference fixtures.
- NL summaries contain no numbers or claims absent from the structured input (automated grounding check).
- Ledger stays within the size budget on a 4,000-image project after a full pipeline plus QC.
- Support bundle contains no pixels, no client names and no API keys.

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
| Ledger write per decision | <= 0.4 ms amortised |
| Explain panel open (with crops) | <= 250 ms |
| Ledger size per 1,000 images (full pipeline) | <= 6 MB |
| Replay of one decision | <= 1 s |

Telemetry events (local-first, opt-in aggregation):

- `decision.recorded` {kind, autonomy, calibrated_confidence, ms}
- `explain.opened` {kind, tab}
- `calibration.ece` {kind, ece, samples}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Explanations drift from actual behaviour | Reasons are emitted by the deciding code path only; a lint forbids constructing reasons outside the deciding module, and grounding tests check the NL layer. |
| Overconfident models make Zero-Touch unsafe | Hard ECE gate in CI, conservative default bands, and risk multipliers for irreversible actions. |
| Ledger bloat | Compaction policy, bounded reason counts, and a size test in CI. |
| Explanation overload in the UI | Top-3 reasons by weight shown by default with 'show all', and plain-language codes approved by PM. |

## 13. Acceptance criteria

- [ ] Opening any image shows why it was kept or rejected, with the runner-up and score breakdown.
- [ ] The Edit tab lists the exact parameters and masks that were applied.
- [ ] Confidence badges appear across the app and map to documented autonomy bands.
- [ ] Calibration report is published and ECE gates pass.
- [ ] Any decision can be replayed from the ledger with an identical result.
- [ ] A support bundle can be exported that contains no client imagery.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 13 - Explain My Edit, Confidence Calibration & Decision Ledger.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-13-EXPLAINABILITY-CONFIDENCE-LEDGER.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - Every decision in the app can be opened up: why this frame was kept, why it was edited this way, how confident the system is, what evidence was used - and every decision is recorded in an auditable ledger.

Rules:
  - Do not start Phase 14. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-explain/src/{lib,reason,decision,ledger,calibration,policy,summary,replay,bundle}.rs`, `crates/aura-catalog/migrations/0013_ledger.sql`, `config/autonomy_bands.toml`, `apps/desktop/src/components/explain/{ExplainPanel,ReasonRow,EvidenceCrop,AlternativeCompare}.tsx`, `ml/eval/calibration_report.py`, `tools/aura-cli/src/replay.rs`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-13-explainability-confidence-ledger and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-13.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-13-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-13-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-13-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 13 of 30 - Explain My Edit, Confidence Calibration & Decision Ledger - part of the AURA Wedding AI master build plan.*

---

# Phase 14 - Non-Destructive Edit Recipe & GPU Develop Engine

> **Single feature shipped by this phase:** A colour-managed, GPU-accelerated RAW development engine driven by a versioned JSON edit recipe - the deterministic renderer that every AI editing decision writes into.
>
> **Mission:** Build the pixel engine the whole editing half of the product depends on: fast, reproducible, colour-correct, fully non-destructive, and interchangeable with Lightroom via XMP.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 14 of 30 |
| Epic | E3 - Photo Brain |
| Feature | A colour-managed, GPU-accelerated RAW development engine driven by a versioned JSON edit recipe - the deterministic renderer that every AI editing decision writes into. |
| Depends on | Phases 01, 02, 03 |
| Unlocks | Phases 15-26, 30 |
| Duration | 3 weeks |
| Primary owners | Colour Scientist, Senior Engineer - GPU & Render (Rust / wgpu / CUDA), Tech Lead - Imaging Core (Rust), Performance Engineer |
| Risk level | High - correctness here is load-bearing for everything visual |
| Headline KPI | proxy render <= 60 ms at 2048 px; full 45 MP render <= 900 ms; recipe round-trip bit-identical |
| Competitor being beaten | Lightroom and Capture One rendering quality; DxO pipeline discipline |

## 1. Why this phase exists

Every AI decision from Phase 15 onward is meaningless unless a real, high-quality renderer executes it identically every time. Getting demosaic, colour management and tone mapping right is the difference between 'AI-edited' and 'professionally edited'.

Non-destructive recipes are also the product's honesty guarantee: the photographer can change any AI decision, and the RAW is never modified. This is what makes an autonomous editor acceptable to professionals.

The recipe format is the interchange currency: XMP export, Lightroom hand-off, style learning targets, QC re-edits and the learning loop all read and write it.

## 2. Scope contract

### 2.1 In scope

- Edit recipe schema v1 (versioned, forward-compatible): exposure, contrast, temperature/tint, highlights/shadows/whites/blacks, tone curve, HSL, clarity/texture/dehaze, vibrance/saturation, sharpening, noise reduction, lens corrections, geometry (crop/rotate/perspective), local masks with per-mask parameters, retouch operations, B&W mix, film-style grade, plus provenance (`source`, `confidence`, `decision_id`).
- GPU develop pipeline (wgpu compute shaders, with a CPU reference implementation): black/white level, linearisation, demosaic (AHD-class + a high-quality option), highlight recovery, camera-profile matrix / DCP-style handling, working space conversion (linear ProPhoto-like), white balance, exposure/tone, curve, HSL, local masks, output transform, sRGB/AdobeRGB/Display-P3 output.
- Colour management: ICC-aware output, camera profile handling, per-camera calibration matrices, and monitor profile awareness in the viewer.
- Deterministic render contract: same recipe + same RAW + same engine version = bit-identical output, verified by hashes across GPU and CPU paths within a documented tolerance.
- Render tiers: interactive proxy (2048 px, <= 60 ms), review (screen resolution), export (full resolution, 16-bit).
- Edit history: parameter-level undo/redo, snapshots, and 'reset to AI suggestion' / 'reset to original'.
- XMP read/write for the parameter subset Lightroom understands, plus a lossless AURA sidecar for the rest.
- Recipe migration framework so v1 recipes still render correctly under v2+ engines.

### 2.2 Explicitly out of scope (do not build it here)

- Deciding parameter values (Phases 15-17 do that).
- Retouch pixel operations (Phases 20-21 add operators to this engine).
- Generative fill (Phase 24).

## 3. Architecture and data flow

```text
RAW (LibRaw) --> black/white level --> linearise --> demosaic --> highlight recovery
      |                                                                    |
      v                                                                    v
 camera profile matrix ------------------------------> working space (linear, wide gamut)
                                                                           |
   recipe.wb --> temperature/tint  ->  exposure  ->  tone (H/S/W/B) -> curve -> HSL
                                                                           |
                                            local masks (P18) applied per-parameter
                                                                           |
                                     retouch operators (P20-21) / restoration (P22)
                                                                           |
                                     geometry (P23) -> output transform -> ICC out
                                                                           |
                                proxy (2048) | review (screen) | export (full, 16-bit)
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-recipe/src/{lib,schema,migrate,xmp,sidecar,history,hash}.rs` | Recipe model, versioning, XMP/sidecar IO, undo history. |
| `crates/aura-render/src/{lib,graph,gpu,cpu,tiles,colour,profiles,tonemap,output}.rs` | Render graph and backends. |
| `crates/aura-render/shaders/*.wgsl` | Compute shaders for each stage. |
| `crates/aura-render/tests/golden/` | Golden renders per camera and per stage. |
| `docs/adr/ADR-0014-render-pipeline.md` | Colour architecture and determinism policy. |
| `docs/recipe-schema-v1.md` | Public recipe documentation. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Edit recipe v1 (frozen; every AI phase writes this)**

```json
{
  "schema": 1,
  "engine": "aura-render/1.0.0",
  "image": { "content_hash": "a3f2...", "camera": "ILCE-7M4", "profile": "adobe_standard" },
  "global": {
    "exposure": 0.31, "contrast": 11,
    "temperature": 4930, "tint": 8,
    "highlights": -31, "shadows": 22, "whites": 7, "blacks": -9,
    "clarity": 6, "texture": 4, "dehaze": 0,
    "vibrance": 8, "saturation": 0,
    "curve": { "points": [[0,0],[64,58],[128,132],[255,255]] },
    "hsl": { "orange": { "h": -2, "s": -6, "l": 4 } },
    "sharpen": { "amount": 40, "radius": 0.8, "detail": 25, "masking": 30 },
    "noise": { "luminance": 22, "colour": 30, "detail": 50, "model": "scene_aware_v1" }
  },
  "lens": { "distortion": true, "vignette": 60, "ca": true, "profile": "FE 35mm F1.4 GM" },
  "geometry": { "rotate": -0.6, "crop": [0.02,0.01,0.97,0.98], "perspective": null },
  "masks": [
    { "id": "m1", "kind": "face", "target": "identity:bride", "feather": 0.35,
      "params": { "exposure": 0.18, "shadows": 8, "clarity": -4, "temperature": -60 } },
    { "id": "m2", "kind": "background", "invert_of": "subject", "feather": 0.5,
      "params": { "exposure": -0.22, "saturation": -6 } }
  ],
  "retouch": [ { "op": "skin_smooth", "strength": 0.35, "protect_texture": 0.8, "mask": "skin" } ],
  "restoration": { "denoise": "auto", "face_recovery": 0.2, "deblur": 0 },
  "bw": null,
  "provenance": {
    "scene": "indoor_ceremony", "style_profile": "amrit_v3",
    "confidence": 0.982, "decision_id": "d_8f21", "source": "ai",
    "user_edited_fields": []
  }
}
```

**Render API**

```rust
pub struct RenderRequest {
    pub image_id: ImageId, pub recipe: Recipe,
    pub level: RenderLevel,          // Proxy2048 | Screen(u32,u32) | Full
    pub output: OutputSpec,          // { colour_space, bit_depth, icc }
    pub purpose: RenderPurpose,      // Interactive | Analysis | Export
}

pub trait RenderService: Send + Sync {
    fn render(&self, req: RenderRequest) -> Result<RenderedImage, AuraError>;
    fn render_hash(&self, req: &RenderRequest) -> u64;   // determinism check
    fn capabilities(&self) -> RenderCaps;                // gpu backend, max texture, precision
}
```

## 6. Algorithm, model and implementation design

### 6.1 Colour correctness first

- Work in a linear wide-gamut space with 16-bit half floats on GPU; all creative operations happen there, and the output transform is the only place tone is baked.
- Camera profiles: use LibRaw's colour matrices as the baseline and support DCP-style profiles; store the profile choice in the recipe so renders are reproducible.
- Highlight recovery reconstructs clipped channels from unclipped ones before white balance, which is what saves blown veils and window light.
- Match a documented reference: golden renders are compared against a calibrated reference build, and deviations require COL sign-off.

### 6.2 Determinism and the CPU reference

- Every stage has a CPU reference implementation; CI compares GPU vs CPU within a per-stage tolerance (typically <= 1/1024 in linear light) and fails on drift.
- No non-deterministic reductions: fixed tile order, no atomics in colour math, fixed random seeds for dither.
- `render_hash` over (raw content hash, recipe canonical JSON, engine version, output spec) is stored with exports so a delivered file can always be re-created.

### 6.3 Performance architecture

- Tiled execution with a render graph that fuses stages into as few compute passes as possible; the interactive path skips restoration and heavy retouch until the user zooms in.
- Proxy renders reuse the Phase 02 cache when only output-space parameters change; parameter changes invalidate only downstream stages.
- Full-resolution export streams tiles so a 100 MP file never needs to fit in VRAM; CPU fallback path is correct but slower and clearly reported.
- Batch export uses a worker pool sized by VRAM budget from the Phase 03 hardware plan.

### 6.4 Recipe discipline

- Canonical JSON serialisation (sorted keys, fixed float formatting) so hashes are stable across platforms.
- `user_edited_fields` records which parameters a human touched; AI passes and QC must never overwrite those - enforced in the merge function, with a test.
- Migration functions per schema version, plus a golden test that v1 recipes render identically under later engines within tolerance.

## 7. Cloud AI usage (bring-your-own API key)

No cloud AI call in this phase. The phase must work with the network cable unplugged; the Cloud AI Gateway from Phase 04 stays idle here.

## 8. Implementation order (execute literally, in this order)

1. Write ADR-0014 for colour architecture, precision and determinism; COL and CTO sign.
2. Define and freeze recipe schema v1 with canonical serialisation and hashing.
3. Implement the CPU reference pipeline stage by stage with unit tests per stage.
4. Implement the GPU pipeline in wgpu with the same stage decomposition; add the parity harness.
5. Implement colour management, camera profiles and output transforms; validate against reference targets.
6. Implement tiling, the render graph, caching and invalidation.
7. Implement edit history, undo/redo, snapshots and `user_edited_fields` protection.
8. Implement XMP read/write plus the AURA sidecar, with round-trip tests against Lightroom.
9. Build golden renders for 12 camera bodies and wire the CI gate.
10. Benchmark and tune to hit the interactive and export budgets.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `COL` | Colour Scientist | Own colour architecture, camera profiles, highlight recovery, reference validation | ADR + validation report | 8 d |
| `SRG` | Senior Engineer - GPU & Render (Rust / wgpu / CUDA) | Implement GPU stages in wgsl, render graph, tiling, VRAM management | GPU pipeline | 12 d |
| `TLC` | Tech Lead - Imaging Core (Rust) | Recipe schema, canonical serialisation, migration framework, merge semantics | `aura-recipe` | 5 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | CPU reference implementation and parity harness | Reference path + tests | 7 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Edit history, undo/redo, snapshots, XMP/sidecar IO | History + IO | 5 d |
| `PERF` | Performance Engineer | Profile and tune to budgets; batch export worker pool; VRAM safety | Benchmark report | 5 d |
| `QAL` | QA Lead - Automation | Golden render suite, GPU/CPU parity gate, round-trip and determinism tests | CI gates | 6 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Develop panel scaffolding, before/after, zoom/pan at 60 fps, history UI | Editor UI | 6 d |
| `DEVOPS` | DevOps / Release Engineer | GPU CI runners (NVIDIA + Apple + DirectML), artefact storage for golden images | CI capability | 3 d |
| `MFE` | Mid-Level Frontend Engineer | Parameter controls bound to the recipe with instant proxy preview | UI controls | 4 d |
| `DOC` | Technical Writer | Publish recipe schema v1 docs and the colour-management explainer | Docs merged | 3 d |

### 9.1 Handoff chain for this phase

```text
COL colour spec -> SRC CPU reference -> SRG GPU implementation (parity-gated)
                        |                              |
                        v                              v
              TLC recipe schema ------------> SFE/MFE editor UI
                        |
        QAL golden + parity gates + PERF budgets -> CTO gate -> Phases 15+
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

- Golden renders match the reference within tolerance for 12 camera bodies across 6 scene types.
- GPU vs CPU parity within 1/1024 in linear light per stage; failure blocks the build.
- Recipe round-trip: serialise -> parse -> render produces an identical hash; XMP round-trip preserves the Lightroom-compatible subset.
- `user_edited_fields` are never overwritten by an AI or QC pass (dedicated test).
- Full-resolution export of a 100 MP file completes without exceeding the VRAM budget.
- Interactive budget: proxy re-render after a slider change <= 60 ms on the reference GPU.
- v1 recipes render correctly after a simulated schema v2 migration.

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
| Proxy render at 2048 px (RTX 4070) | <= 60 ms |
| Proxy render at 2048 px (M3 Pro) | <= 110 ms |
| Full 45 MP render + export (GPU) | <= 900 ms |
| Full 45 MP render (CPU fallback) | <= 6 s |
| Batch export throughput (RTX 4070) | >= 1.2 images/s at 45 MP JPEG |

Telemetry events (local-first, opt-in aggregation):

- `render.request` {level, ms, backend, cache_hit, stages_run}
- `render.parity_fail` {stage, max_delta} (dev builds only)
- `export.batch` {images, ms, throughput, backend}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Colour rendering does not match professional expectations | COL-owned reference validation against calibrated targets, golden tests per camera, and a public colour explainer so behaviour is predictable. |
| GPU driver differences break determinism | CPU reference as ground truth, per-EP parity gates in CI on three GPU families, and documented tolerances. |
| Recipe schema churn breaks old projects | Versioned schema, migration functions, golden migration tests, and a rule that fields are deprecated but never removed. |
| Interactive performance regresses as later phases add stages | Stage-level budgets, fused passes, PERF gate in CI, and the interactive path deliberately skipping heavy stages until zoom. |

## 13. Acceptance criteria

- [ ] A recipe fully describes an edit, renders identically on any machine, and never modifies the RAW.
- [ ] Sliders in the develop panel respond within the interactive budget at 2048 px.
- [ ] Exported JPEG/TIFF files are colour-managed and match the on-screen render.
- [ ] XMP export opens in Lightroom with the compatible parameters intact.
- [ ] Parameters a user has touched survive every subsequent AI pass.
- [ ] Golden and parity gates run on NVIDIA, Apple and DirectML CI runners.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 14 - Non-Destructive Edit Recipe & GPU Develop Engine.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-14-DEVELOP-ENGINE-EDIT-RECIPE.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - A colour-managed, GPU-accelerated RAW development engine driven by a versioned JSON edit recipe - the deterministic renderer that every AI editing decision writes into.

Rules:
  - Do not start Phase 15. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-recipe/src/{lib,schema,migrate,xmp,sidecar,history,hash}.rs`, `crates/aura-render/src/{lib,graph,gpu,cpu,tiles,colour,profiles,tonemap,output}.rs`, `crates/aura-render/shaders/*.wgsl`, `crates/aura-render/tests/golden/`, `docs/adr/ADR-0014-render-pipeline.md`, `docs/recipe-schema-v1.md`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-14-develop-engine-edit-recipe and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-14.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-14-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-14-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-14-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 14 of 30 - Non-Destructive Edit Recipe & GPU Develop Engine - part of the AURA Wedding AI master build plan.*

---

# Phase 15 - Exposure AI & White Balance AI (mixed lighting mastery)

> **Single feature shipped by this phase:** Per-image exposure and white balance decided by scene, subject and skin - including the hard cases: tungsten receptions, mixed daylight-plus-LED ceremonies, backlit exits and coloured stage lighting.
>
> **Mission:** Deliver the first visible 'this actually looks edited' moment. Correct exposure and believable skin colour across a whole wedding is what photographers judge an editor by in the first ten seconds.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 15 of 30 |
| Epic | E3 - Photo Brain |
| Feature | Per-image exposure and white balance decided by scene, subject and skin - including the hard cases: tungsten receptions, mixed daylight-plus-LED ceremonies, backlit exits and coloured stage lighting. |
| Depends on | Phases 06, 07, 09, 14 |
| Unlocks | Phases 16, 17, 25, 26, 27 |
| Duration | 2.5 weeks |
| Primary owners | Colour Scientist, ML Lead - Vision, Senior ML Engineer |
| Risk level | High - the most visible AI decision in the product |
| Headline KPI | exposure within +/-0.15 EV of expert on 85 % of frames; WB within 200 K / 4 tint on 85 %; skin dE00 <= 3.0 vs expert reference |
| Competitor being beaten | Imagen and Aftershoot auto-exposure/WB; Lightroom Auto |

## 1. Why this phase exists

Mixed lighting is the single hardest technical problem in wedding photography and the place where generic auto-WB fails hardest. Solving it with face- and scene-aware models is both a real user benefit and a credible technical differentiator.

Exposure must be subject-referred, not average-referred: the bride's face is the anchor, not the mean luminance of a dark reception hall. That one change fixes most of what photographers hate about auto-exposure.

## 2. Scope contract

### 2.1 In scope

- Exposure model: predicts EV offset targeting correct *subject* luminance, respecting scene intent (moody dance floors stay moody), with clipping-aware constraints from Phase 09.
- Face-anchored exposure: target face luminance bands per skin-tone group (measured, not assumed), weighted by Phase 06 prominence.
- White balance model: predicts temperature/tint from RAW statistics plus face skin patches plus known-neutral detection (white dress, tablecloths, paper), scene-conditioned.
- Mixed-light detection and handling: identify multi-illuminant frames, choose the illuminant that governs the subject, and record a `MIXED_LIGHT` note so Phase 18 can apply local WB corrections later.
- Coloured stage/LED light handling: detect saturated non-neutral illumination and deliberately *not* neutralise it (a purple dance floor should stay purple) while keeping skin plausible.
- Skin-tone-aware constraints: WB solutions are rejected if they push skin outside a plausible chromaticity locus for that person; the constraint is per-identity, learned across the wedding.
- Consistency seed: per-scene reference frames chosen here and handed to Phase 25 for gallery normalisation.
- Everything written into the recipe with confidence and reasons; every value overridable and then protected.

### 2.2 Explicitly out of scope (do not build it here)

- Tone curve, contrast and HSL (Phase 16).
- Photographer style preference (Phase 17 shifts these baselines).
- Local masks and face lighting (Phases 18-19).
- Gallery-wide normalisation (Phase 25).

## 3. Architecture and data flow

```text
RAW stats (histogram, per-channel) + faces + skin patches + neutral candidates + scene
        |
        +--> IlluminantEstimator (multi-hypothesis) --> illuminants[], mixed?
        |
        +--> SubjectLuminanceTarget (per scene, per skin group, prominence-weighted)
        |
        v
   constrained solve:  minimise (skin chroma error + neutral error)
                       subject to (no new clipping, scene intent, plausible skin locus)
        |
        v
   recipe.global { exposure, temperature, tint } + reasons + confidence + reference-frame hints
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-brain-photo/src/tone/{exposure,wb,illuminant,skin_locus,neutrals,solve}.rs` | Exposure and WB estimation. |
| `ml/models/tone/{train_exposure.py,train_wb.py,eval_tone.py,export.py}` | Model training against expert edits. |
| `config/exposure_targets.toml` | Per-scene subject luminance bands and intent rules. |
| `crates/aura-catalog/migrations/0015_tone.sql` | `image_tone_estimate` table with alternatives. |
| `apps/desktop/src/components/develop/BasicPanel.tsx` | Exposure/WB controls with AI badge and reset-to-AI. |
| `docs/model-cards/{exposure,white_balance}.md` | Model cards including skin-tone fairness analysis. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Tone estimation output**

```rust
pub struct ToneEstimate {
    pub image_id: ImageId,
    pub exposure_ev: f32, pub exposure_conf: f32,
    pub temperature_k: f32, pub tint: f32, pub wb_conf: f32,
    pub illuminants: Vec<Illuminant>,      // { kind, cct, weight, region }
    pub mixed_light: bool, pub dominant_on_subject: Option<usize>,
    pub subject_luma_before: f32, pub subject_luma_target: f32,
    pub skin_de00_estimate: f32,
    pub alternatives: Vec<(f32, f32, f32)>, // (ev, cct, tint) runner-up solutions
    pub reasons: Vec<Reason>,
}
```

## 6. Algorithm, model and implementation design

### 6.1 Exposure: subject-referred with intent

- Compute the prominence-weighted mean luminance of face regions in linear light; compare against the scene's target band from `exposure_targets.toml` (e.g. ceremony faces at 45-55 % linear-to-perceptual, dance floor 30-40 %).
- Solve for the EV offset that lands the subject in band, then clamp so highlight clipping does not increase beyond the scene tolerance and shadow noise stays within the Phase 09 budget.
- When no face is present (details, venue), fall back to a learned scene-level exposure model trained on expert edits of the same scene class.
- Backlit frames get a dedicated rule: expose for the subject, accept blown background above a threshold, and note it as intentional in the reasons.

### 6.2 White balance: constrained multi-illuminant solve

- Generate illuminant hypotheses from grey-world, white-patch, learned CNN prediction and known-neutral regions (dress, tablecloth, paper), each with a weight.
- Score each hypothesis by how plausible it makes the *skin* of known identities: project skin patches into a chromaticity space and measure distance from that person's estimated locus (accumulated across the wedding, so a single bad frame cannot mislead).
- Choose the illuminant governing the subject; if two strong illuminants disagree spatially, mark `mixed_light` and record both, leaving local correction to Phase 18.
- For saturated coloured lighting, detect that the illuminant is intentionally non-neutral (high chroma, stage scene, low CRI signature) and preserve mood: correct only enough to keep skin within its plausible locus.

### 6.3 Skin fairness, explicitly engineered

- Skin targets are measured per identity from the wedding's own best-lit frames, not from a fixed 'ideal' skin value - this is how the system avoids lightening or warming darker skin toward a Eurocentric target.
- Evaluation reports dE00 per skin-tone group (Monk scale buckets); the model cannot ship if any group is more than 1.0 dE00 worse than the best group.
- The skin locus constraint is a hard constraint in the solve, not a post-hoc adjustment.

### 6.4 Handing consistency forward

- For each Phase 07 segment, select 3-5 reference frames: high WB confidence, good subject exposure, primary identity present, no mixed light.
- Store them with the segment; Phase 25 normalises the rest of the segment toward these anchors, which is the mechanism behind gallery-wide consistency.

## 7. Cloud AI usage (bring-your-own API key)

No cloud AI call in this phase. The phase must work with the network cable unplugged; the Cloud AI Gateway from Phase 04 stays idle here.

## 8. Implementation order (execute literally, in this order)

1. Assemble the training set: RAW + expert final edits with exposure/WB parameters across traditions and lighting types (DATA).
2. Implement RAW statistics extraction, neutral detection and skin-patch sampling.
3. Implement the illuminant hypothesis generators including a learned CNN predictor.
4. Implement the per-identity skin locus accumulation.
5. Implement the constrained solve for exposure and WB with clipping and noise constraints.
6. Train and validate the scene-level exposure model for faceless frames.
7. Implement mixed-light and coloured-light handling with explicit notes.
8. Implement reference-frame selection per segment.
9. Write results into recipes with reasons and confidence; wire the develop panel badges.
10. Run fairness evaluation per skin-tone group and publish the model cards.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `COL` | Colour Scientist | Own illuminant estimation, skin locus modelling, colour validation and fairness measurement | Validated solver + report | 9 d |
| `MLL` | ML Lead - Vision | Own exposure targets, learned model design, evaluation protocol against expert edits | Signed spec + gates | 4 d |
| `SRML` | Senior ML Engineer | Train WB CNN and scene exposure model; export, calibrate, verify parity | Models registered | 7 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Implement solver plumbing, recipe writing, reference-frame selection, persistence | `tone` module + tests | 6 d |
| `DATA` | Data Engineer / Dataset Curator | Collect RAW + expert-edit pairs across 8 lighting classes and 5 skin-tone buckets | Dataset v3 | 10 d |
| `QAL` | QA Lead - Automation | EV/CCT/dE00 gates, fairness gate, mixed-light fixtures, no-face fixtures | CI gates | 5 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Expert review of 600 frames across lighting types; catalogue systematic bias | Audit report | 4 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Basic panel with AI badges, confidence, reset-to-AI, and a mixed-light indicator | Develop UI | 3 d |
| `MFE` | Mid-Level Frontend Engineer | Per-scene review queue for low-confidence WB, batch accept/adjust | UI panel | 3 d |
| `PM` | Product Manager Agent | Approve `exposure_targets.toml` and the 'preserve mood' policy with the consultant | Approved config | 2 d |
| `PERF` | Performance Engineer | Keep estimation under 25 ms per image; share statistics with Phase 09 | Benchmark | 2 d |
| `DOC` | Technical Writer | Write the mixed-lighting explainer and the skin-fairness statement | Docs merged | 2 d |

### 9.1 Handoff chain for this phase

```text
DATA expert pairs -> SRML models -> COL solver + skin locus
                                        |
                                        v
                        SRC recipe writing + reference frames -> SFE/MFE UI
                                        |
                     QAL gates + QAIQ expert audit -> COL/PM gate -> Phases 16, 25
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

- Exposure within +/-0.15 EV of expert on >= 85 % of the held-out set; never increases clipping beyond scene tolerance.
- WB within 200 K and 4 tint units on >= 85 %; tungsten reception and mixed-LED fixtures included.
- Skin dE00 <= 3.0 mean and <= 1.0 spread across skin-tone buckets (fairness gate).
- Coloured stage lighting is preserved, not neutralised (dedicated fixture set).
- Faceless frames (details, venue) are exposed sensibly by the scene model.
- Reference frames are selected for every segment with at least three candidates.
- Determinism: identical RAW + identical config produce identical parameters.

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
| Estimation per image (GPU) | <= 25 ms |
| 4,000 images total | <= 100 s |
| Extra storage per image | <= 600 B |

Telemetry events (local-first, opt-in aggregation):

- `tone.estimated` {images, ms, mean_ev, mean_cct, mixed_light_ratio}
- `tone.low_confidence` {count, scene_histogram}
- `tone.user_override` {param, delta}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Skin-tone bias | Per-identity measured targets, mandatory per-bucket fairness gate, and refusal to ship a model that regresses any bucket. |
| Neutralising creative lighting | Explicit coloured-light detection, PM-approved preserve-mood policy, and fixture tests. |
| Mixed light produces visibly split colour | Detect and defer to local WB in Phase 18; mark frames so QC can verify. |
| Auto-exposure fights the photographer's intent | Scene intent targets, style shift in Phase 17, and protected user overrides. |

## 13. Acceptance criteria

- [ ] Exposure and WB are set automatically for every selected frame with reasons and confidence.
- [ ] Faces are correctly exposed in dark receptions without flattening the mood.
- [ ] Skin colour is believable across skin tones, measured and published.
- [ ] Coloured stage lighting survives editing.
- [ ] Mixed-light frames are flagged for local correction rather than badly globally corrected.
- [ ] Every segment has reference frames ready for gallery consistency.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 15 - Exposure AI & White Balance AI (mixed lighting mastery).

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-15-EXPOSURE-WHITE-BALANCE-AI.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - Per-image exposure and white balance decided by scene, subject and skin - including the hard cases: tungsten receptions, mixed daylight-plus-LED ceremonies, backlit exits and coloured stage lighting.

Rules:
  - Do not start Phase 16. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-brain-photo/src/tone/{exposure,wb,illuminant,skin_locus,neutrals,solve}.rs`, `ml/models/tone/{train_exposure.py,train_wb.py,eval_tone.py,export.py}`, `config/exposure_targets.toml`, `crates/aura-catalog/migrations/0015_tone.sql`, `apps/desktop/src/components/develop/BasicPanel.tsx`, `docs/model-cards/{exposure,white_balance}.md`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-15-exposure-white-balance-ai and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-15.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-15-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-15-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-15-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 15 of 30 - Exposure AI & White Balance AI (mixed lighting mastery) - part of the AURA Wedding AI master build plan.*

---

# Phase 16 - Tone AI, Adaptive Curves, HSL AI & Skin-Tone Protection

> **Single feature shipped by this phase:** Scene-specific contrast, highlight/shadow recovery, adaptive tone curves and intelligent HSL - with a hard guarantee that colour grading never turns skin unnatural.
>
> **Mission:** Turn technically correct frames into finished-looking photographs, and make skin protection a structural property of the grading engine rather than an afterthought.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 16 of 30 |
| Epic | E3 - Photo Brain |
| Feature | Scene-specific contrast, highlight/shadow recovery, adaptive tone curves and intelligent HSL - with a hard guarantee that colour grading never turns skin unnatural. |
| Depends on | Phases 14, 15 |
| Unlocks | Phases 17, 25, 27 |
| Duration | 2 weeks |
| Primary owners | Colour Scientist, ML Lead - Vision, Senior ML Engineer |
| Risk level | Medium-High |
| Headline KPI | tone parameter MAE within expert tolerance on 85 % of frames; skin hue shift <= 2 deg after grading; no clipping introduced on 99.5 % of frames |
| Competitor being beaten | Imagen style rendering; Lightroom Auto tone; Capture One colour editor |

## 1. Why this phase exists

Exposure and WB make a frame correct; tone and colour make it *look like a photograph someone paid for*. This phase is where the output starts to feel professional.

Skin protection has to be built into the grading maths. Every product that grades globally eventually produces orange or grey skin, and photographers notice immediately. Making skin a protected region inside the colour pipeline is a structural advantage.

## 2. Scope contract

### 2.1 In scope

- Tone model: contrast, highlights, shadows, whites, blacks predicted per frame, conditioned on scene, histogram shape and subject luminance from Phase 15.
- Adaptive tone curve generation: a smooth monotonic curve (4-8 control points) fitted to achieve target subject contrast and shadow lift without crushing or clipping, with a guaranteed-monotonic spline.
- HSL intelligence: per-band hue/saturation/luminance adjustments driven by detected content (greenery, sky, wood, skin, dress white, saturated decor), aimed at colour harmony rather than fixed presets.
- Skin-tone protection: a chromaticity-space skin mask derived from Phase 06 faces and skin colour clustering, excluded or attenuated in HSL/saturation/vibrance operations, with a measured hue-shift ceiling.
- Colour harmony analysis: detect competing saturated hues and reduce the distracting ones (a fluorescent green exit sign) rather than desaturating the whole frame.
- Clipping guard: any parameter set that introduces new clipping above the scene tolerance is re-solved.
- Everything written to the recipe with reasons; alternatives stored so QC and the user can switch quickly.

### 2.2 Explicitly out of scope (do not build it here)

- Photographer-specific style (Phase 17 shifts all of this).
- Local adjustments (Phases 18-19).
- Gallery consistency (Phase 25).
- B&W conversion selection (Phase 29).

## 3. Architecture and data flow

```text
exposure/WB-corrected linear image + scene + subject luma + content segmentation
     |
     +--> ToneModel -> contrast, highlights, shadows, whites, blacks
     |
     +--> CurveFitter -> monotonic spline hitting subject-contrast target
     |
     +--> ContentColourAnalyser -> greenery/sky/wood/decor/dress clusters
     |            |
     |            v
     |     HSL solver (harmony objective)  <---- SKIN LOCUS CONSTRAINT (hard)
     |
     +--> ClippingGuard (re-solve if new clipping)
                       |
                       v
   recipe.global { contrast, H/S/W/B, curve, hsl, vibrance } + reasons + alternatives
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-brain-photo/src/colour/{tone,curve,hsl,harmony,skin_guard,clip_guard}.rs` | Tone and colour decisions. |
| `ml/models/colour/{train_tone.py,eval_colour.py,export.py}` | Tone model training against expert edits. |
| `config/tone_intent.toml` | Per-scene contrast and shadow-lift intents. |
| `apps/desktop/src/components/develop/{TonePanel,CurveEditor,HslPanel}.tsx` | Tone/curve/HSL UI with AI badges. |
| `docs/model-cards/tone_model.md` | Model card with per-scene and per-skin-tone metrics. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Tone and colour decision output**

```rust
pub struct ColourDecision {
    pub image_id: ImageId,
    pub contrast: f32, pub highlights: f32, pub shadows: f32, pub whites: f32, pub blacks: f32,
    pub curve: ToneCurve,               // monotonic, 4..8 points, invertible
    pub hsl: HslAdjustments,            // 8 bands
    pub vibrance: f32, pub saturation: f32,
    pub skin_guard: SkinGuardReport,    // { mask_area, max_hue_shift_deg, attenuation }
    pub clipping_after: (f32, f32),     // highlight %, shadow %
    pub alternatives: Vec<ColourVariant>,
    pub reasons: Vec<Reason>, pub confidence: f32,
}
```

## 6. Algorithm, model and implementation design

### 6.1 Tone prediction and curve fitting

- Predict the five tone parameters with a small MLP over features: histogram percentiles, subject luminance, scene class, dynamic range, flash flag, noise level.
- Fit the curve as a monotonic cubic spline (PCHIP) through control points solved so that (a) subject mid-tone contrast hits the scene intent, (b) shadow lift stays within the noise budget, (c) highlight roll-off preserves the dress texture.
- Monotonicity is enforced structurally, so no AI decision can ever produce a posterised or inverted curve.
- Highlight recovery interacts with Phase 14: the curve is fitted *after* recovery so recovered detail is not re-clipped.

### 6.2 HSL by content, not by preset

- Cluster image chroma into content bands using segmentation cues (greenery, sky, skin, dress, wood, decor) and measure each band's saturation and hue relative to pleasing targets from expert edits.
- Solve small per-band adjustments with a harmony objective: reduce hue conflicts with the subject, tame the most saturated distractor, and keep total adjustment magnitude small (professional edits are subtle).
- Greenery is the classic wedding problem: outdoor foliage is usually too yellow-green and too saturated; the solver learns per-photographer preference in Phase 17.

### 6.3 Skin protection as a hard constraint

- Build a soft skin mask in chromaticity space seeded by actual detected skin patches for the identities in the frame, not a generic skin range - this handles all skin tones correctly.
- All HSL, vibrance and saturation operations are attenuated inside the skin mask by a factor that guarantees measured hue shift <= 2 deg and chroma change <= 6 %.
- After grading, measure actual skin hue/chroma shift; if the ceiling is exceeded, re-solve with stronger attenuation and record the event in the reasons.

### 6.4 Guardrails

- Clipping guard re-solves any parameter set that introduces new clipping above the scene tolerance; the reason states which parameter was reduced.
- Noise guard limits shadow lift based on the Phase 09 sigma estimate so the system never trades a dark frame for a noisy one.
- All decisions store 2-3 alternatives (flatter, punchier, warmer) so the user or QC can switch instantly without recomputation.

## 7. Cloud AI usage (bring-your-own API key)

No cloud AI call in this phase. The phase must work with the network cable unplugged; the Cloud AI Gateway from Phase 04 stays idle here.

## 8. Implementation order (execute literally, in this order)

1. Extract tone/HSL parameters from the expert-edit dataset and align them to scenes.
2. Train and validate the tone parameter model.
3. Implement the monotonic curve fitter with the three constraints and unit tests.
4. Implement content colour clustering and the HSL harmony solver.
5. Implement the chromaticity skin mask and the attenuation guarantee, with measurement.
6. Implement clipping and noise guards with re-solve logic.
7. Generate alternatives and write everything into the recipe with reasons.
8. Build the tone/curve/HSL UI with AI badges, alternatives and reset-to-AI.
9. Run expert evaluation and the skin-shift fairness measurement; publish the model card.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `COL` | Colour Scientist | Own curve mathematics, colour harmony targets, skin guard measurement and validation | Validated solver | 7 d |
| `MLL` | ML Lead - Vision | Own tone model design and evaluation protocol; define the subtlety metric | Signed spec | 3 d |
| `SRML` | Senior ML Engineer | Train and export the tone model; calibrate confidence | Model registered | 5 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Implement solvers, guards, alternatives, recipe writing, persistence | `colour` module + tests | 6 d |
| `DATA` | Data Engineer / Dataset Curator | Extract tone/HSL parameters from expert edits; label greenery/decor cases | Dataset v3.1 | 5 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Tone panel, curve editor with AI curve overlay, HSL panel with protected-skin indicator | Develop UI | 5 d |
| `MFE` | Mid-Level Frontend Engineer | Alternatives switcher, before/after slider, per-scene batch adjust | UI panels | 3 d |
| `QAL` | QA Lead - Automation | Parameter MAE gates, monotonicity property tests, skin-shift gate, clipping gate | CI gates | 4 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Expert review of 400 graded frames: subtlety, skin, greenery, dress texture | Audit report | 3 d |
| `PM` | Product Manager Agent | Approve `tone_intent.toml` per scene with the consultant | Approved config | 2 d |
| `PERF` | Performance Engineer | Keep colour decisions under 20 ms per image | Benchmark | 1 d |
| `DOC` | Technical Writer | Document the skin-protection guarantee and how curves are generated | Docs merged | 2 d |

### 9.1 Handoff chain for this phase

```text
DATA expert parameters -> SRML tone model -> COL curve/HSL solvers
                                        |
                                        v
                          SRC guards + alternatives -> SFE/MFE UI
                                        |
                       QAL gates + QAIQ expert audit -> COL/PM gate
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

- Tone parameters within expert tolerance on >= 85 % of held-out frames; subtlety metric within expert distribution.
- Property test: every generated curve is monotonic and produces no posterisation on a gradient ramp.
- Skin hue shift <= 2 deg and chroma change <= 6 % on all skin-tone buckets after full grading.
- No new clipping introduced above scene tolerance on 99.5 % of frames.
- Shadow lift never exceeds the noise budget on high-ISO fixtures.
- Greenery fixtures show measurable improvement in expert scoring versus no HSL.

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
| Colour decisions per image | <= 20 ms |
| 4,000 images total | <= 80 s |
| Alternatives generation overhead | <= 15 % |

Telemetry events (local-first, opt-in aggregation):

- `colour.decided` {images, ms, mean_contrast, mean_shadow_lift}
- `colour.skin_guard_triggered` {count, mean_attenuation}
- `colour.clip_guard_resolve` {count, param_histogram}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Over-graded, HDR-looking results | Subtlety metric trained on professional edits, magnitude penalties in the solver, and expert audit gate. |
| Unnatural skin after grading | Hard skin-locus attenuation with measured ceilings and automatic re-solve. |
| Noisy shadows from aggressive lifting | Noise-budget constraint tied to Phase 09 sigma estimates. |
| HSL fights the photographer's taste | Small magnitudes by default, alternatives always available, and full personalisation in Phase 17. |

## 13. Acceptance criteria

- [ ] Selected frames receive scene-appropriate contrast, curves and HSL automatically.
- [ ] Skin never shifts measurably, on any skin tone, after grading.
- [ ] Generated curves are always monotonic and never posterise.
- [ ] No frame gains new clipping beyond its scene tolerance.
- [ ] The curve editor shows the AI curve and allows instant switching to alternatives.
- [ ] Expert audit confirms results look professionally subtle rather than filtered.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 16 - Tone AI, Adaptive Curves, HSL AI & Skin-Tone Protection.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-16-TONE-CURVES-COLOUR-AI.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - Scene-specific contrast, highlight/shadow recovery, adaptive tone curves and intelligent HSL - with a hard guarantee that colour grading never turns skin unnatural.

Rules:
  - Do not start Phase 17. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-brain-photo/src/colour/{tone,curve,hsl,harmony,skin_guard,clip_guard}.rs`, `ml/models/colour/{train_tone.py,eval_colour.py,export.py}`, `config/tone_intent.toml`, `apps/desktop/src/components/develop/{TonePanel,CurveEditor,HslPanel}.tsx`, `docs/model-cards/tone_model.md`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-16-tone-curves-colour-ai and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-16.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-16-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-16-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-16-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 16 of 30 - Tone AI, Adaptive Curves, HSL AI & Skin-Tone Protection - part of the AURA Wedding AI master build plan.*

---

# Phase 17 - Style Learning: Scene-Conditional Personal AI Profiles ("Teach My AI")

> **Single feature shipped by this phase:** Import previously edited weddings and the app learns the photographer's own look - not as one style, but as a scene-conditional style tree (outdoor portrait, indoor ceremony, golden hour, flash, dance floor, details, night).
>
> **Mission:** Beat Imagen's Personal AI Profile by learning *per scene and per lighting condition*, from as few as 300 pairs, with an honest report of what was learned and where the profile is weak.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 17 of 30 |
| Epic | E3 - Photo Brain |
| Feature | Import previously edited weddings and the app learns the photographer's own look - not as one style, but as a scene-conditional style tree (outdoor portrait, indoor ceremony, golden hour, flash, dance floor, details, night). |
| Depends on | Phases 14, 15, 16 |
| Unlocks | Phases 25, 27, 28, 30 |
| Duration | 3 weeks |
| Primary owners | ML Lead - Vision, Senior ML Engineer, MLOps / Model Packaging Engineer, Colour Scientist |
| Risk level | High - the strongest retention feature in the product |
| Headline KPI | style match dE00 <= 2.5 vs the photographer's own edit; usable profile from 300 pairs; training of 2,000 pairs <= 25 min locally |
| Competitor being beaten | Imagen Personal AI Profiles (needs ~2,000 images, one global style) |

## 1. Why this phase exists

A photographer's style is their brand. A tool that renders 'good' edits that are not *their* edits will always be a temporary tool. Style learning is what converts a trial into a subscription.

Scene conditioning is the differentiator: real photographers do not edit a candlelit ceremony the way they edit a golden-hour portrait. One global style profile is the reason existing tools feel almost-right-but-wrong.

Learning locally, from the user's own files, with no upload requirement, is both a privacy advantage and a speed advantage over cloud-only competitors.

## 2. Scope contract

### 2.1 In scope

- Pair ingestion: match RAW originals to exported JPEG/TIFF finals (by content hash, filename stem, capture time, and perceptual matching for renamed files), or read XMP/Lightroom catalogues directly when available.
- Parameter extraction: when XMP exists, read parameters directly; when only a JPEG final exists, *infer* the recipe by fitting the develop engine's parameters to reproduce the final (differentiable-ish optimisation over the Phase 14 pipeline).
- Scene bucketing: every pair is assigned a scene + lighting bucket from Phases 07/15, producing a style tree rather than a single model.
- Style model per bucket: a residual predictor that outputs *deltas* on top of the Phase 15/16 baseline decisions, with shrinkage toward the global profile when a bucket has few samples.
- Hierarchical fallback: bucket -> parent scene group -> global profile -> factory default, so sparse buckets degrade gracefully instead of behaving randomly.
- Profile diagnostics: per-bucket sample counts, confidence, measured match error, and an honest 'weak buckets' report telling the photographer exactly which weddings to add.
- Multi-profile support: several named profiles (personal, light-and-airy client set, dark-and-moody, second shooter), switchable per project and per chapter.
- Local training with progress UI, cancellable, resumable, GPU-accelerated where available; no cloud requirement.
- Profile versioning, export/import (signed `.auraprofile` bundle), and A/B comparison against the previous version before adoption.

### 2.2 Explicitly out of scope (do not build it here)

- Learning from in-app corrections (Phase 30's learning loop, which updates these same profiles).
- Retouch style (Phase 20 has its own strength learning).
- Gallery consistency (Phase 25).

## 3. Architecture and data flow

```text
RAW originals + finals (JPEG/XMP/catalogue)
        |
   PairMatcher (hash / stem / time / perceptual)
        |
   ParameterExtractor:  XMP present? read : fit recipe to reproduce final
        |
   SceneBucketer (P07 scene x P15 lighting) --> buckets[]
        |
   per bucket: ResidualStyleModel (delta on baseline decisions) + shrinkage
        |
   StyleTree { global, groups[], buckets[] } + diagnostics
        |
   inference: baseline (P15/P16) + style delta (bucket -> group -> global) -> recipe
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-style/src/{lib,pairs,extract,fit,bucket,tree,infer,profile,diagnostics}.rs` | Style learning engine. |
| `crates/aura-style/src/fit/optimise.rs` | Recipe fitting against a target final image. |
| `ml/models/style/{train_residual.py,eval_style.py,export.py}` | Residual style model training. |
| `crates/aura-catalog/migrations/0017_style.sql` | `profiles`, `profile_buckets`, `style_pairs` tables. |
| `apps/desktop/src/routes/style/{TeachMyAi,ProfileReport,BucketMatrix,AbCompare}.tsx` | Teach My AI UI. |
| `docs/style-profiles.md` | How style learning works, data requirements, privacy. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Style profile contracts (frozen)**

```rust
pub struct StyleProfile {
    pub id: ProfileId, pub name: String, pub version: u16,
    pub global: StyleDelta,
    pub groups: HashMap<SceneGroup, StyleDelta>,
    pub buckets: HashMap<StyleBucket, BucketModel>,   // (scene, lighting) -> model
    pub diagnostics: ProfileDiagnostics,
    pub trained_pairs: u32, pub trained_at: Timestamp,
    pub engine_ver: String,
}

pub struct StyleDelta {                 // additive deltas on baseline decisions
    pub exposure: f32, pub temperature_k: f32, pub tint: f32,
    pub contrast: f32, pub highlights: f32, pub shadows: f32,
    pub whites: f32, pub blacks: f32,
    pub curve_shift: CurveShift,        // control-point offsets
    pub hsl: HslAdjustments,
    pub vibrance: f32, pub saturation: f32,
    pub skin_bias: SkinBias,            // warm/neutral preference, chroma preference
    pub confidence: f32, pub samples: u32,
}

pub struct ProfileDiagnostics {
    pub per_bucket: Vec<(StyleBucket, u32, f32)>,   // samples, match_de00
    pub weak_buckets: Vec<StyleBucket>,
    pub overall_de00: f32,
    pub recommendation: String,                     // "add 1 outdoor daylight wedding"
}
```

## 6. Algorithm, model and implementation design

### 6.1 Getting ground truth from photographers who only kept JPEGs

- Preferred path: read XMP/Lightroom catalogue parameters directly - exact and free.
- Fallback path: fit the Phase 14 recipe to reproduce the final JPEG. Optimise a small parameter vector (exposure, WB, tone, curve, HSL) by coordinate descent on a perceptual loss (dE00 on a downsampled grid plus histogram matching), typically 60-120 render iterations at 512 px, about 1-2 s per pair on GPU.
- Reject pairs whose fit residual is too high (heavy local retouching, composites, crops) so the style model learns global look, not unmodelled work.
- Report how many pairs were used versus rejected - honesty here prevents 'why doesn't it look like me' support tickets.

### 6.2 Residual learning with shrinkage

- The model predicts *deltas from the baseline*, so it inherits Phase 15/16's correctness and only learns taste. This is why 300 pairs are enough.
- Per bucket, fit a small ridge-regularised model on features (subject luma, CCT, dynamic range, ISO, flash, scene, skin group) predicting each delta.
- James-Stein style shrinkage toward the parent group and global delta, weighted by sample count, so a bucket with 8 samples barely moves and a bucket with 400 dominates.
- Robust fitting (Huber loss) so one wildly different wedding cannot skew the profile.

### 6.3 Honest diagnostics and adoption safety

- After training, re-render a held-out set of the photographer's own pairs and report dE00 per bucket; this is the number shown in the UI, not a vague 'profile ready'.
- A/B compare: side-by-side of old profile, new profile and the photographer's own edit before adoption; adoption is an explicit action.
- `weak_buckets` drives a concrete recommendation ('add one indoor flash reception to improve dance-floor accuracy').

### 6.4 Multi-profile and per-chapter application

- Profiles are selectable per project and per chapter, which supports real studio practice (a moody reception with an airy ceremony) and second-shooter normalisation in Phase 26.
- Profile bundles are signed and portable so a studio can distribute one look to a team.

## 7. Cloud AI usage (bring-your-own API key)

No cloud AI call in this phase. The phase must work with the network cable unplugged; the Cloud AI Gateway from Phase 04 stays idle here.

## 8. Implementation order (execute literally, in this order)

1. Implement pair matching including perceptual matching for renamed exports.
2. Implement XMP/catalogue parameter reading.
3. Implement recipe fitting against final JPEGs with residual rejection.
4. Implement scene/lighting bucketing and the style tree structure.
5. Implement per-bucket robust regression with shrinkage and hierarchical fallback.
6. Implement diagnostics, held-out evaluation and the recommendation engine.
7. Implement profile versioning, signed export/import and A/B comparison.
8. Build the Teach My AI flow: folder pick, progress, report, bucket matrix, adopt.
9. Validate end to end with 5 real photographers' archives; measure dE00 per bucket.
10. Wire style deltas into the Phase 15/16 decision path with provenance in the recipe.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `MLL` | ML Lead - Vision | Own the residual formulation, shrinkage maths, evaluation protocol and adoption gates | Signed spec | 5 d |
| `SRML` | Senior ML Engineer | Implement recipe fitting optimiser and per-bucket model training; GPU acceleration | Training pipeline | 9 d |
| `COL` | Colour Scientist | Define the perceptual loss, validate fitted parameters against known XMP ground truth | Validation report | 4 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Pair matching, XMP reading, profile storage, versioning, signed bundles, inference path | `aura-style` + tests | 8 d |
| `MLOPS` | MLOps / Model Packaging Engineer | Local training orchestration: progress, cancel, resume, artefact management, reproducibility | Training runtime | 4 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Teach My AI wizard, progress UI, profile report, bucket matrix heat-map, A/B compare | Style UI | 6 d |
| `MFE` | Mid-Level Frontend Engineer | Per-project and per-chapter profile selection, multi-profile management | UI panels | 3 d |
| `DATA` | Data Engineer / Dataset Curator | Collect consented archives from 5 photographers across traditions for validation | Validation sets | 6 d |
| `QAL` | QA Lead - Automation | dE00 gates, sparse-bucket fallback tests, pair-matching edge cases, determinism | CI gates | 5 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Blind test: can each photographer distinguish AURA's output from their own edit? | Blind study | 4 d |
| `SEC` | Security & Privacy Engineer | Ensure archives are never uploaded; sign profile bundles; validate import safety | Sign-off | 2 d |
| `PERF` | Performance Engineer | Hit the 25-minute training budget for 2,000 pairs; tune fit iterations | Benchmark | 3 d |
| `DOC` | Technical Writer | Write 'Teach My AI' guide, data requirements and troubleshooting | Docs merged | 3 d |

### 9.1 Handoff chain for this phase

```text
SRC pair matching -> COL loss definition -> SRML fitting + training
                                  |
                                  v
                    MLL shrinkage/eval -> MLOPS local training runtime
                                  |
                       SFE/MFE Teach My AI UI -> QAIQ blind study -> MLL/PM gate
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

- Fitted parameters match known XMP ground truth within tolerance on 200 pairs (validates the JPEG-only path).
- Style match dE00 <= 2.5 on held-out pairs per photographer; <= 3.5 in weak buckets.
- Usable profile from 300 pairs: measurable improvement over factory baseline in every populated bucket.
- Sparse bucket (< 10 samples) falls back to group/global without erratic output.
- One outlier wedding cannot shift the global profile beyond a bounded amount (robustness test).
- Profile export/import round-trips with signature verification; tampered bundles are rejected.
- Training 2,000 pairs completes within budget and is cancellable and resumable.

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
| Recipe fitting per pair (GPU, 512 px) | <= 1.5 s |
| Training 2,000 pairs end to end (RTX 4070) | <= 25 min |
| Training 2,000 pairs (M3 Pro) | <= 45 min |
| Style inference overhead per image | <= 2 ms |
| Profile size | <= 3 MB |

Telemetry events (local-first, opt-in aggregation):

- `style.training` {pairs, accepted, rejected, buckets, ms}
- `style.profile_adopted` {profile, version, overall_de00}
- `style.bucket_fallback` {bucket, fallback_level}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Photographer's archive is messy (renames, crops, composites) | Multiple matching strategies, residual-based rejection, and an honest accepted/rejected report. |
| Learned style bakes in past mistakes | Residual-on-baseline design keeps corrections intact; outlier-robust fitting; A/B comparison before adoption. |
| Sparse buckets behave unpredictably | Shrinkage plus hierarchical fallback plus explicit weak-bucket reporting. |
| Users expect one-click magic from 20 photos | Clear minimum-data guidance in the UI and a profile-strength meter rather than a false 'ready' state. |

## 13. Acceptance criteria

- [ ] Pointing the app at past weddings produces a scene-conditional profile with a per-bucket accuracy report.
- [ ] Edits made with the profile are measurably closer to the photographer's own edits than the factory baseline.
- [ ] Weak buckets are named with a concrete recommendation for what to add.
- [ ] Profiles can be versioned, compared, adopted, exported and shared safely.
- [ ] Training runs locally with progress, cancel and resume, and never uploads imagery.
- [ ] At least three of five validation photographers cannot reliably distinguish AURA's output from their own.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 17 - Style Learning: Scene-Conditional Personal AI Profiles ("Teach My AI").

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-17-STYLE-LEARNING-PERSONAL-AI.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - Import previously edited weddings and the app learns the photographer's own look - not as one style, but as a scene-conditional style tree (outdoor portrait, indoor ceremony, golden hour, flash, dance floor, details, night).

Rules:
  - Do not start Phase 18. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-style/src/{lib,pairs,extract,fit,bucket,tree,infer,profile,diagnostics}.rs`, `crates/aura-style/src/fit/optimise.rs`, `ml/models/style/{train_residual.py,eval_style.py,export.py}`, `crates/aura-catalog/migrations/0017_style.sql`, `apps/desktop/src/routes/style/{TeachMyAi,ProfileReport,BucketMatrix,AbCompare}.tsx`, `docs/style-profiles.md`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-17-style-learning-personal-ai and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-17.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-17-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-17-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-17-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 17 of 30 - Style Learning: Scene-Conditional Personal AI Profiles ("Teach My AI") - part of the AURA Wedding AI master build plan.*

---

# Phase 18 - Local Mask AI: Automatic Semantic Masking

> **Single feature shipped by this phase:** Every selected frame is automatically segmented into the regions that matter: face, skin, eyes, teeth, hair, clothing, dress, subject, background, sky, greenery, skin-tone-safe zones - as feathered, editable masks.
>
> **Mission:** Provide the spatial vocabulary for every local decision in the rest of the product. Without trustworthy masks, face lighting, retouching, background balancing and generative cleanup are all impossible.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 18 of 30 |
| Epic | E3 - Photo Brain |
| Feature | Every selected frame is automatically segmented into the regions that matter: face, skin, eyes, teeth, hair, clothing, dress, subject, background, sky, greenery, skin-tone-safe zones - as feathered, editable masks. |
| Depends on | Phases 06, 07, 14 |
| Unlocks | Phases 19-24, 27 |
| Duration | 2.5 weeks |
| Primary owners | ML Lead - Vision, Senior ML Engineer, Senior Engineer - GPU & Render (Rust / wgpu / CUDA) |
| Risk level | High - mask quality is visible in every retouch |
| Headline KPI | mask mIoU >= 0.92 on faces/skin, >= 0.88 on hair, >= 0.90 on subject; mask generation <= 120 ms/image; edge halo invisible at 100 % zoom |
| Competitor being beaten | Lightroom AI Masking; Capture One AI masks; Evoto segmentation |

## 1. Why this phase exists

Local, region-specific editing is the difference between a preset and a professional edit. It is also where competitors are strongest, so mask quality must be at least as good as Lightroom's Select Subject and Select Sky.

Masks must be *semantic and identity-aware*: 'the bride's skin' is a more useful mask than 'skin', and it enables per-person consistency across the gallery in Phase 25.

Mask edges are where amateur software reveals itself. Hair, veils and backlit rim light demand matting quality, not just segmentation.

## 2. Scope contract

### 2.1 In scope

- Semantic segmentation model producing 14 classes: skin, face, eyes, sclera, iris, teeth, lips, eyebrows, hair, facial_hair, clothing, dress, background, sky.
- Instance-aware masks tied to Phase 06 identities: `identity:bride/skin`, `identity:groom/face`, etc.
- Subject/background separation with alpha matting refinement (guided filter + trimap-based matting network) for hair, veil and rim-light edges.
- Additional environment masks: sky, greenery, water, floor/stage, window/light source, skin-tone-safe zone (used by Phase 16).
- Mask representation: compressed run-length + low-resolution alpha with GPU upsampling, so 4,000 masks do not consume gigabytes.
- Mask algebra: union, intersect, subtract, feather, expand/contract, invert, and 'mask minus skin' style compositions used by later phases.
- Per-mask parameter attachment in the recipe (Phase 14 already supports it) and full user editability: brush add/remove, feather slider, refine-edge.
- Quality self-assessment: each mask reports a confidence and an edge-quality estimate; low-quality masks disable aggressive downstream operations.

### 2.2 Explicitly out of scope (do not build it here)

- Using masks to make edits (Phases 19-24).
- Retouch pixel operations (Phases 20-21).
- Generative fill (Phase 24).

## 3. Architecture and data flow

```text
proxy + faces + person boxes + identities
     |
     +--> SegmentationNet (14 classes, 768px) --> class logits
     |                                              |
     +--> SubjectNet (salient person) --> coarse alpha
                                                    |
                              trimap --> MattingNet --> refined alpha (hair/veil/rim)
                                                    |
              instance assignment via face/person boxes --> identity-scoped masks
                                                    |
                    compress (RLE + low-res alpha) --> mask store
                                                    |
        mask algebra API (union/intersect/subtract/feather) --> Phases 19-24 + user brushes
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-vision/src/mask/{segment,subject,matting,trimap,instance,algebra,store,quality}.rs` | Masking engine. |
| `crates/aura-render/shaders/mask_*.wgsl` | GPU mask upsampling, feathering and compositing. |
| `ml/models/mask/{train_seg.py,train_matting.py,eval_mask.py,export.py}` | Model training. |
| `crates/aura-catalog/migrations/0018_masks.sql` | `masks` table with compressed payloads. |
| `apps/desktop/src/components/develop/MaskPanel.tsx` | Mask list, visibility, brush tools, refine edge. |
| `docs/model-cards/{segmentation,matting}.md` | Model cards with per-class and per-skin-tone metrics. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Mask API (frozen)**

```rust
pub struct Mask {
    pub id: MaskId, pub image_id: ImageId,
    pub kind: MaskKind,                    // Skin | Face | Hair | Clothing | Subject | Sky | ...
    pub identity: Option<IdentityId>,      // instance scoping
    pub payload: MaskPayload,              // Rle(Vec<u8>) | Alpha8 { w, h, data }
    pub feather: f32, pub confidence: f32, pub edge_quality: f32,
    pub user_edited: bool, pub model_ver: u16,
}

pub trait MaskService {
    fn masks(&self, image: ImageId) -> Vec<Mask>;
    fn ensure(&self, image: ImageId, kinds: &[MaskKind]) -> Result<Vec<Mask>, AuraError>;
    fn compose(&self, ops: &[MaskOp]) -> Mask;      // union/intersect/subtract/feather/grow
    fn upload_gpu(&self, mask: &Mask, level: RenderLevel) -> GpuMask;
}
```

## 6. Algorithm, model and implementation design

### 6.1 Segmentation and matting

- Single multi-class segmentation network at 768 px with a lightweight decoder; classes chosen for editing utility, not academic completeness.
- Subject alpha is refined with a matting stage: build a trimap by eroding/dilating the coarse mask, then run a matting network only in the uncertain band - this is what makes veils and flyaway hair look correct.
- Guided-filter upsampling to full resolution at render time, so masks are stored small but composited precisely.
- Skin masks are seeded by detected faces and extended by colour-space growth constrained to connected regions, which handles arms, shoulders and hands reliably.

### 6.2 Instance scoping

- Assign each connected component of face/skin/hair/clothing to the nearest Phase 06 face/person box with an overlap test; unassigned components become `identity: None`.
- Identity-scoped masks make per-person operations possible: brighten the bride's face without touching the guest beside her, keep her skin consistent all night (Phase 25).

### 6.3 Storage and performance

- Binary-ish classes stored as RLE; alpha classes stored as 8-bit at 1/4 resolution; total mask payload target <= 180 KB per image for all classes.
- Masks are generated lazily for selected frames only (post-cull), because rejected frames never need them - a large part of why the pipeline meets its time budget.
- GPU path uploads masks as textures once per render session and reuses them across parameter changes.

### 6.4 Quality gating

- Edge quality estimated from matting uncertainty and gradient agreement; low edge quality (backlit veil, motion) reduces the strength allowed for downstream retouching.
- Mask confidence below threshold disables aggressive operations (skin smoothing, generative cleanup) and records the reason, so a bad mask can never cause a visible artefact.
- User-edited masks are locked and never regenerated (`user_edited` flag), consistent with the recipe protection rule.

## 7. Cloud AI usage (bring-your-own API key)

No cloud AI call in this phase. The phase must work with the network cable unplugged; the Cloud AI Gateway from Phase 04 stays idle here.

## 8. Implementation order (execute literally, in this order)

1. Define the class list with the retouching consultant; align with what Phases 19-24 actually need.
2. Label segmentation data on wedding imagery (including veils, sarees, sherwanis, dark suits, varied skin tones).
3. Train and export the segmentation model; evaluate per class and per skin tone.
4. Implement trimap generation and train/integrate the matting model.
5. Implement instance assignment to identities.
6. Implement compression, storage and GPU upsampling.
7. Implement mask algebra and the lazy generation policy.
8. Implement quality estimation and the downstream gating contract.
9. Build the mask panel with brushes, feather and refine-edge.
10. Run zoom-level artefact audits and publish model cards.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `MLL` | ML Lead - Vision | Own class taxonomy, matting strategy, evaluation per class and per skin tone | Signed spec + gates | 3 d |
| `SRML` | Senior ML Engineer | Train segmentation + matting models, export, quantise, verify parity | Models registered | 9 d |
| `SRG` | Senior Engineer - GPU & Render (Rust / wgpu / CUDA) | GPU mask upsampling, feathering, compositing shaders, texture management | GPU mask path | 5 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Mask store, compression, algebra, instance assignment, lazy generation, quality gating | `mask` module + tests | 7 d |
| `DATA` | Data Engineer / Dataset Curator | Segmentation labels on 12k wedding frames incl. veils, ethnic attire, varied skin tones | Labels v1 | 12 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Mask panel, overlay visualisation, brush add/remove, feather, refine edge | Mask UI | 5 d |
| `QAL` | QA Lead - Automation | mIoU gates per class, edge-artefact tests at 100 % zoom, storage budget test | CI gates | 4 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Zoom audit of 300 masks: halos, veil edges, dark-suit boundaries, hair detail | Audit report | 3 d |
| `PERF` | Performance Engineer | Hit 120 ms/image; tune resolution and lazy policy; measure storage | Benchmark | 3 d |
| `COL` | Colour Scientist | Verify mask compositing happens in linear light without gamma errors | Sign-off | 1 d |
| `DOC` | Technical Writer | Document mask kinds and how to edit masks manually | Docs merged | 2 d |

### 9.1 Handoff chain for this phase

```text
MLL taxonomy -> DATA labels -> SRML models -> SRG GPU path
                                     |
                                     v
                          SRC store + algebra + gating -> SFE mask UI
                                     |
                     QAL/QAIQ artefact audits + COL linear-light check -> gate
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

- mIoU gates per class met on held-out data, including a dark-skin subset and an ethnic-attire subset.
- Matting quality: veil and flyaway-hair fixtures show no visible halo at 100 % zoom (human-verified plus SSIM-band metric).
- Instance scoping: in group photos, per-identity skin masks do not bleed between adjacent people.
- Storage: all masks for a 1,000-image gallery stay within budget.
- User-edited masks survive re-analysis and model upgrades.
- Low-confidence masks demonstrably block aggressive downstream operations (integration test with Phase 20 stub).

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
| All masks per image (GPU) | <= 120 ms |
| 1,000 selected images total | <= 120 s |
| Mask payload per image (all classes) | <= 180 KB |
| GPU mask upload + composite per render | <= 4 ms |

Telemetry events (local-first, opt-in aggregation):

- `mask.generated` {image_count, classes, ms, mean_confidence}
- `mask.low_quality` {kind, count}
- `mask.user_edit` {kind, action}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Mask errors cause visible retouch artefacts | Confidence and edge-quality gating that reduces downstream strength, plus 100 % zoom audits in QA. |
| Ethnic attire and dark fabrics segment poorly | Deliberate labelling coverage, per-subset metrics, and gates that block shipping on regression. |
| Mask storage explodes | RLE plus quarter-resolution alpha, lazy generation for selected frames only, and a CI budget test. |
| Model upgrade invalidates stored masks | `model_ver` per mask, background regeneration with progress, and locked user edits preserved. |

## 13. Acceptance criteria

- [ ] Every selected frame has semantic, identity-scoped masks with confidence and edge quality.
- [ ] Hair, veil and rim-light edges look clean at 100 % zoom.
- [ ] Masks can be inspected, brushed and feathered by the user, and those edits are permanent.
- [ ] Downstream phases receive a stable mask algebra API.
- [ ] Mask generation and storage stay within their budgets on a 1,000-image gallery.
- [ ] Model cards report per-class and per-skin-tone performance.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 18 - Local Mask AI: Automatic Semantic Masking.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-18-LOCAL-MASK-AI.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - Every selected frame is automatically segmented into the regions that matter: face, skin, eyes, teeth, hair, clothing, dress, subject, background, sky, greenery, skin-tone-safe zones - as feathered, editable masks.

Rules:
  - Do not start Phase 19. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-vision/src/mask/{segment,subject,matting,trimap,instance,algebra,store,quality}.rs`, `crates/aura-render/shaders/mask_*.wgsl`, `ml/models/mask/{train_seg.py,train_matting.py,eval_mask.py,export.py}`, `crates/aura-catalog/migrations/0018_masks.sql`, `apps/desktop/src/components/develop/MaskPanel.tsx`, `docs/model-cards/{segmentation,matting}.md`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-18-local-mask-ai and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-18.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-18-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-18-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-18-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 18 of 30 - Local Mask AI: Automatic Semantic Masking - part of the AURA Wedding AI master build plan.*

---

# Phase 19 - Local Light Sculpting: Face Lighting, Subject Enhancement, Background Balancing & Dodge/Burn AI

> **Single feature shipped by this phase:** The app shapes light like a retoucher: lifts faces naturally, makes the subject visually dominant, calms distracting backgrounds, and applies frequency-aware dodge and burn.
>
> **Mission:** Deliver the 'why does this look so much better and I can't tell what changed' effect - invisible local light shaping that separates professional editing from slider presets.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 19 of 30 |
| Epic | E3 - Photo Brain |
| Feature | The app shapes light like a retoucher: lifts faces naturally, makes the subject visually dominant, calms distracting backgrounds, and applies frequency-aware dodge and burn. |
| Depends on | Phases 15, 16, 18 |
| Unlocks | Phases 20, 25, 27 |
| Duration | 2 weeks |
| Primary owners | Colour Scientist, ML Lead - Vision, Senior Engineer - GPU & Render (Rust / wgpu / CUDA) |
| Risk level | Medium-High - subtlety is the whole point |
| Headline KPI | expert 'invisible edit' rating >= 4.2/5; no halo artefacts on 99 % of frames; local pass <= 80 ms/image |
| Competitor being beaten | Lightroom masking workflows; Evoto light adjustments; manual retoucher dodge and burn |

## 1. Why this phase exists

Global adjustments cannot fix the most common wedding lighting problems: a face in shadow under a mandap, a bright window behind the couple, a hot spot on a forehead. Local light shaping is where perceived quality jumps.

Done badly it looks obvious - haloes, glowing faces, muddy backgrounds. Doing it *subtly and automatically* is a genuine engineering achievement and hard for competitors to copy.

## 2. Scope contract

### 2.1 In scope

- Face lighting: per-identity exposure/shadow lift inside the face mask, targeted at the Phase 15 luminance band, with luminosity-masked falloff so the transition is invisible.
- Subject enhancement: a small contrast/clarity/micro-contrast lift on the subject combined with a matching background reduction, calibrated so the total change stays under a perceptual threshold.
- Background balancing: reduce background luminance and chroma competition when it exceeds the subject (bright windows, sunlit doorways, saturated decor), with edge-aware falloff.
- Dodge and burn AI: frequency-separated local tonal correction - low-frequency shaping (cheekbones, jawline, shadow depth) and blemish-free mid-frequency evening, driven by a face-geometry-aware light model.
- Hot-spot and shine control: detect specular sheen on foreheads/noses (common with makeup and flash) and reduce it as a luminance operation, not a texture-destroying blur.
- Group-photo fairness: everyone in a family formal gets consistent face lighting rather than only the primary subject.
- Strength governance: total local adjustment budget per image, with per-operation caps, all conditioned on mask confidence from Phase 18.
- Everything expressed as recipe masks + parameters so it is fully reversible and inspectable.

### 2.2 Explicitly out of scope (do not build it here)

- Skin texture retouching (Phase 20).
- Removing objects (Phase 24).
- Gallery-wide consistency (Phase 25).

## 3. Architecture and data flow

```text
masks (P18) + tone decisions (P15/16) + scene + identities
     |
     +--> FaceLighting: per-identity target band -> exposure/shadow delta inside face mask
     |            (luminosity-masked falloff, blend in linear light)
     +--> SubjectEnhance: subject clarity/micro-contrast + background attenuation (paired)
     +--> BackgroundBalance: luma/chroma reduction where background competes
     +--> DodgeBurn: frequency separation -> low-freq shaping map + mid-freq evening map
     +--> ShineControl: specular detection -> luminance-only reduction
                       |
              StrengthGovernor (per-image budget, mask-confidence scaling)
                       |
         recipe.masks[] with params + reasons + confidence
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-brain-photo/src/local/{face_light,subject,background,dodgeburn,shine,governor}.rs` | Local light decisions. |
| `crates/aura-render/shaders/{luminosity_mask,freq_sep,local_apply}.wgsl` | GPU implementations. |
| `config/local_light.toml` | Targets, caps and per-scene strength policy. |
| `ml/models/local/{train_light_targets.py,eval_local.py}` | Learned targets from expert edits. |
| `apps/desktop/src/components/develop/LocalPanel.tsx` | Local adjustments with per-operation strength. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Local light plan**

```rust
pub struct LocalLightPlan {
    pub image_id: ImageId,
    pub face_light: Vec<(IdentityId, FaceLightDelta)>,
    pub subject: SubjectEnhanceDelta,
    pub background: BackgroundBalanceDelta,
    pub dodge_burn: Option<DodgeBurnMaps>,     // low-freq + mid-freq maps, quarter res
    pub shine: Option<ShineReduction>,
    pub total_budget_used: f32,                // 0..1 of allowed perceptual change
    pub gated_by_mask_quality: Vec<MaskKind>,  // operations reduced or skipped
    pub reasons: Vec<Reason>, pub confidence: f32,
}
```

## 6. Algorithm, model and implementation design

### 6.1 Invisible face lighting

- Work in linear light and modulate by a luminosity mask derived from the face's own tonal distribution, so shadows lift more than mid-tones and highlights barely move - this is what prevents the flat 'glowing face' look.
- Cap the lift at the scene's noise budget: a face lifted 1.2 EV in a high-ISO reception would reveal noise, so the cap is dynamic and the reason explains it.
- Feather by face size (a small guest face needs a wider relative feather) and blend across the hair/neck boundary using the Phase 18 alpha.
- Group fairness: solve all faces jointly toward the same target band with a maximum inter-face difference, so nobody looks pasted in.

### 6.2 Paired subject/background operations

- Never brighten the subject alone; always pair with a proportional background reduction so overall image luminance stays roughly constant - the eye reads the *relationship*, not absolute values.
- Measure competition explicitly: background/subject luminance ratio, background chroma energy, and count of high-luminance blobs (from Phase 11) - operations trigger only when a measured threshold is crossed.
- Edge-aware falloff using the subject alpha plus a guided filter so the background reduction does not trace a visible outline.

### 6.3 Dodge and burn with frequency separation

- Separate the face region into low-frequency (form) and mid-frequency (texture) bands with a bilateral/Gaussian pair; shape only the low-frequency band so pores are untouched.
- Derive a shaping map from face geometry (landmarks) and the existing light direction, deepening natural shadow zones slightly and lifting under-eye and jaw shadows - the classic retoucher moves, applied conservatively.
- Mid-frequency evening reduces blotchy tonal patches without smoothing; this is the honest, non-plastic alternative to skin blur (Phase 20 goes further under strict texture protection).
- Shine control detects specular pixels (high luma, low chroma, small area, near-highlight) and reduces luminance only, preserving underlying texture.

### 6.4 Strength governance

- A per-image perceptual budget (measured as mean absolute change in a perceptual space) prevents accumulation across operations; when the budget is exhausted, operations are scaled down in priority order (face lighting first, dodge/burn last).
- All strengths are scaled by mask confidence and edge quality, so a poor mask produces a gentle edit instead of an artefact.
- Scene policy from `local_light.toml`: `dance_floor` gets minimal shaping (motion, mood), `family_portrait` gets the full treatment.

## 7. Cloud AI usage (bring-your-own API key)

No cloud AI call in this phase. The phase must work with the network cable unplugged; the Cloud AI Gateway from Phase 04 stays idle here.

## 8. Implementation order (execute literally, in this order)

1. Extract local-adjustment behaviour from expert edits (difference maps between baseline-graded and final images) to learn realistic targets.
2. Implement luminosity-masked face lighting with dynamic caps.
3. Implement paired subject/background operations with measured triggers.
4. Implement frequency separation and the dodge/burn shaping model.
5. Implement shine detection and luminance-only reduction.
6. Implement the strength governor and mask-quality scaling.
7. Implement group-photo joint solving.
8. Write masks/parameters into the recipe with reasons; build the local panel UI.
9. Run expert subtlety scoring and halo-artefact audits; tune caps.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `COL` | Colour Scientist | Own linear-light blending, luminosity masks, frequency separation correctness | Validated implementation | 6 d |
| `MLL` | ML Lead - Vision | Learn targets from expert difference maps; define the perceptual budget metric | Target model + metric | 4 d |
| `SRG` | Senior Engineer - GPU & Render (Rust / wgpu / CUDA) | GPU shaders for luminosity masks, frequency separation and local application | Shaders + tests | 6 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Decision logic, governor, group solving, recipe writing, persistence | `local` module + tests | 6 d |
| `DATA` | Data Engineer / Dataset Curator | Difference-map dataset from expert edits; label shine and halo cases | Dataset v3.2 | 5 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Local panel with per-operation strength, overlay of applied maps, before/after | Local UI | 4 d |
| `QAL` | QA Lead - Automation | Halo detection test, budget-cap tests, group-fairness test, determinism | CI gates | 4 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Expert subtlety scoring of 400 frames; specifically hunt for haloes and glowing faces | Audit report | 4 d |
| `PM` | Product Manager Agent | Approve `local_light.toml` per-scene policy and default strengths | Approved config | 2 d |
| `PERF` | Performance Engineer | Keep the local pass under 80 ms/image; fuse into the render graph | Benchmark | 2 d |
| `DOC` | Technical Writer | Explain local light shaping and how to tune strength | Docs merged | 2 d |

### 9.1 Handoff chain for this phase

```text
DATA difference maps -> MLL targets/metric -> COL blending design
                                     |
                                     v
                        SRG shaders + SRC decisions -> SFE local UI
                                     |
                       QAL halo gates + QAIQ subtlety audit -> COL/PM gate
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

- Halo detection: automated edge-gradient test finds no artefact on 99 % of fixtures; failures reviewed manually.
- Face lighting hits the target band without exceeding the noise budget on high-ISO fixtures.
- Subject/background pairing keeps global mean luminance within 3 % of the pre-local value.
- Group photos: inter-face luminance spread after lighting <= a documented threshold.
- Dodge/burn preserves mid-frequency texture (measured band energy unchanged within tolerance).
- Low-confidence masks reduce strengths measurably (integration test).
- Expert subtlety rating >= 4.2/5 with no 'obviously edited' flags on the audit set.

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
| Local decisions + map generation per image | <= 80 ms |
| Render overhead for local application (proxy) | <= 12 ms |
| 1,000 selected images total | <= 90 s |

Telemetry events (local-first, opt-in aggregation):

- `local.applied` {images, ops_histogram, mean_budget_used, ms}
- `local.gated` {mask_kind, count}
- `local.shine_reduced` {count, mean_strength}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Visible haloes destroy credibility | Linear-light blending, guided-filter falloff, automated halo tests, and mask-quality gating. |
| Faces look artificially lit | Luminosity masking, dynamic caps, group joint solving, and expert subtlety gate. |
| Accumulated local edits become heavy-handed | Per-image perceptual budget with priority-ordered scaling. |
| Noise revealed by shadow lifting | Noise-budget-aware caps tied to Phase 09 estimates and Phase 22 denoising order. |

## 13. Acceptance criteria

- [ ] Faces in difficult light are lifted naturally without haloes or a glowing look.
- [ ] Bright or saturated backgrounds stop competing with subjects, invisibly.
- [ ] Dodge and burn shapes form without touching skin texture.
- [ ] Everyone in a group photo is lit consistently.
- [ ] All local work is stored as masks and parameters and is fully reversible.
- [ ] Expert reviewers rate the edits as invisible rather than obvious.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 19 - Local Light Sculpting: Face Lighting, Subject Enhancement, Background Balancing & Dodge/Burn AI.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-19-LOCAL-LIGHT-SCULPTING.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - The app shapes light like a retoucher: lifts faces naturally, makes the subject visually dominant, calms distracting backgrounds, and applies frequency-aware dodge and burn.

Rules:
  - Do not start Phase 20. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-brain-photo/src/local/{face_light,subject,background,dodgeburn,shine,governor}.rs`, `crates/aura-render/shaders/{luminosity_mask,freq_sep,local_apply}.wgsl`, `config/local_light.toml`, `ml/models/local/{train_light_targets.py,eval_local.py}`, `apps/desktop/src/components/develop/LocalPanel.tsx`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-19-local-light-sculpting and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-19.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-19-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-19-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-19-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 19 of 30 - Local Light Sculpting: Face Lighting, Subject Enhancement, Background Balancing & Dodge/Burn AI - part of the AURA Wedding AI master build plan.*

---

# Phase 20 - Portrait Retouch AI with Natural Texture Protection

> **Single feature shipped by this phase:** Professional-grade automatic skin retouching: blemishes, temporary marks, dark circles and uneven tone corrected while pores, fine lines and real texture are preserved by design.
>
> **Mission:** Match or beat Retouch4me/Evoto/Aperty in retouch quality while being scene-aware, identity-aware and gallery-consistent - and never producing plastic skin.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 20 of 30 |
| Epic | E4 - Retouch & Restoration |
| Feature | Professional-grade automatic skin retouching: blemishes, temporary marks, dark circles and uneven tone corrected while pores, fine lines and real texture are preserved by design. |
| Depends on | Phases 14, 18, 19 |
| Unlocks | Phases 21, 25, 27 |
| Duration | 3 weeks |
| Primary owners | ML Lead - Vision, Senior ML Engineer, Colour Scientist, Senior Engineer - GPU & Render (Rust / wgpu / CUDA) |
| Risk level | High - the most scrutinised output in the product |
| Headline KPI | texture retention >= 0.9 (band-energy metric); blemish removal recall >= 0.9 / false-removal <= 2 %; retouch <= 350 ms/image at full resolution |
| Competitor being beaten | Retouch4me, Evoto, Aperty, Portraiture |

## 1. Why this phase exists

Retouching is the most emotionally sensitive part of wedding delivery: clients want to look like themselves on a good day, not like a mannequin. A retoucher that removes a pimple but keeps skin texture is worth real money.

It is also the most competitive segment, so quality must be measurable. A texture-retention metric and a false-removal metric make quality claims defensible rather than marketing copy.

Distinguishing temporary from permanent features is an ethical requirement as well as a quality one: freckles, moles, scars and birthmarks are identity, not defects.

## 2. Scope contract

### 2.1 In scope

- Blemish detection and inpainting: pimples, spots, temporary redness, small scratches - detected as *temporary* features and removed with texture-preserving patch synthesis.
- Permanent-feature preservation: freckles, moles, birthmarks, scars, tattoos and dimples explicitly detected and protected, with a user toggle per identity.
- Dark-circle and under-eye correction: luminance and chroma correction in the periorbital region with strict caps and no texture loss.
- Skin tone evening: mid-frequency unevenness (blotches, flush, makeup mismatch, neck/face mismatch) corrected by frequency-selective operations.
- Texture protection engine: frequency-band decomposition with a hard guarantee that high-frequency band energy in skin regions stays within a measured ratio of the original.
- Strength model: automatic strength per identity and per scene (a close-up portrait gets more care than a wide dance frame), plus global Light/Natural/Polished presets.
- Per-identity consistency: the same person is retouched with the same strength across the gallery, so skin does not change character between frames.
- Full-resolution execution at export with a proxy-accurate preview; every operation recorded in the recipe as a reversible retouch op.

### 2.2 Explicitly out of scope (do not build it here)

- Hair, teeth, eyes, clothing and glare (Phase 21).
- Noise reduction and sharpening (Phase 22).
- Body reshaping or slimming - explicitly excluded as a product-ethics decision.

## 3. Architecture and data flow

```text
skin/face masks (P18) + local light (P19) + identity + scene
     |
     +--> BlemishDetector -> candidates { box, type, temporary_prob, confidence }
     |          |
     |          +--> PermanentFeatureClassifier (freckle/mole/scar/tattoo) -> protect set
     |          |
     |          +--> texture-preserving inpaint (patch synthesis + frequency blend)
     |
     +--> UnderEyeCorrector (luma/chroma, capped, texture-safe)
     +--> ToneEveningEngine (mid-frequency only, high-frequency preserved)
                       |
            TextureGuard: measure high-band energy ratio -> re-solve if below floor
                       |
       recipe.retouch[] ops + per-identity strength + reasons + confidence
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-retouch/src/{lib,blemish,permanent,undereye,evening,texture_guard,strength,ops}.rs` | Retouch engine. |
| `crates/aura-render/shaders/{inpaint_patch,freq_bands,retouch_apply}.wgsl` | GPU retouch operators. |
| `ml/models/retouch/{train_blemish.py,train_permanent.py,eval_retouch.py,export.py}` | Detection models. |
| `config/retouch_presets.toml` | Light/Natural/Polished definitions and per-scene strengths. |
| `apps/desktop/src/components/develop/RetouchPanel.tsx` | Strength, presets, per-identity settings, protected features. |
| `docs/model-cards/{blemish_detector,permanent_features}.md` | Model cards with skin-tone metrics. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Retouch operation contract**

```rust
pub enum RetouchOp {
    Blemish { box: Box2, method: InpaintMethod, strength: f32 },
    UnderEye { identity: IdentityId, luma: f32, chroma: f32 },
    ToneEvening { mask: MaskId, strength: f32, band: FreqBand },
    ShineReduce { box: Box2, strength: f32 },     // shared with P19
}

pub struct RetouchPlan {
    pub image_id: ImageId,
    pub ops: Vec<RetouchOp>,
    pub per_identity_strength: HashMap<IdentityId, f32>,
    pub protected: Vec<ProtectedFeature>,        // { box, kind, identity }
    pub texture_report: TextureReport,           // { band_ratio, floor, passed }
    pub preset: RetouchPreset,
    pub reasons: Vec<Reason>, pub confidence: f32,
}
```

## 6. Algorithm, model and implementation design

### 6.1 Temporary versus permanent - the ethical core

- Two-stage detection: find skin anomalies, then classify each as temporary (pimple, redness, scratch, makeup smudge) or permanent (mole, freckle, scar, birthmark, tattoo, beauty mark).
- Cross-frame evidence is decisive and unique to a gallery-aware product: a spot that appears on the same facial coordinate in many frames across hours is permanent; one that appears in a few frames is temporary or transient lighting.
- Permanent features are added to a per-identity protect list, visible and editable in the UI ('keep her beauty mark' as an explicit setting).
- Default behaviour is conservative: uncertain anomalies are left alone, because removing a client's mole is a far worse error than leaving a pimple.

### 6.2 Texture-preserving removal

- Inpaint by patch synthesis from nearby *skin of the same person and lighting*, matching frequency content, then blend only the low/mid bands while transplanting the original high band back.
- Small blemishes use a healing-brush-equivalent (offset patch + gradient blend); larger areas use a learned inpainting network constrained to the skin mask.
- All work happens in linear light on the full-resolution render at export; the preview uses an identical algorithm at proxy scale so what the user approves is what ships.

### 6.3 Texture guard with a measurable guarantee

- Decompose skin regions into frequency bands; measure high-band energy before and after retouching.
- Hard floor: post-retouch high-band energy >= 0.90 of the original (configurable per preset, never below 0.80 even in Polished). If violated, re-solve with lower strength and log it.
- This turns 'we don't produce plastic skin' from a claim into a tested invariant with a number in CI.

### 6.4 Strength and consistency

- Automatic strength from face size in frame, scene class, identity role and preset; a full-frame portrait of the bride gets the most careful treatment, a background guest almost none.
- Per-identity strength is fixed across the gallery so the same person's skin character does not fluctuate between images.
- Under-eye correction is capped hard (typical luma lift <= 0.25 EV inside the periorbital mask) because over-correction is the classic tell of automated retouching.

## 7. Cloud AI usage (bring-your-own API key)

No cloud AI call in this phase. The phase must work with the network cable unplugged; the Cloud AI Gateway from Phase 04 stays idle here.

## 8. Implementation order (execute literally, in this order)

1. Write the retouch ethics policy (no body reshaping, protect permanent features, conservative defaults); PM and CTO sign.
2. Label blemish and permanent-feature data across skin tones.
3. Train the anomaly detector and the temporary/permanent classifier.
4. Implement cross-frame permanence evidence using identity-aligned facial coordinates.
5. Implement patch-synthesis inpainting with frequency-band blending on GPU.
6. Implement under-eye correction and mid-frequency tone evening.
7. Implement the texture guard with measurement and re-solve.
8. Implement per-identity strength assignment and gallery consistency.
9. Build the retouch panel with presets, per-identity controls and protected-feature management.
10. Run blind expert comparison against competitor outputs; publish the results internally.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `MLL` | ML Lead - Vision | Own detection/classification design, texture metric, evaluation protocol | Signed spec + gates | 4 d |
| `SRML` | Senior ML Engineer | Train blemish detector and permanent-feature classifier; export and verify across skin tones | Models registered | 10 d |
| `COL` | Colour Scientist | Frequency-band decomposition correctness, linear-light inpainting, texture measurement | Validated engine | 7 d |
| `SRG` | Senior Engineer - GPU & Render (Rust / wgpu / CUDA) | GPU inpainting and band-blend shaders; full-resolution execution path | Shaders + tests | 7 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Cross-frame permanence, strength assignment, consistency, recipe ops, persistence | `aura-retouch` + tests | 8 d |
| `DATA` | Data Engineer / Dataset Curator | Blemish/permanent labels on 15k faces across five skin-tone buckets, with consent | Labels v1 | 12 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Retouch panel, presets, per-identity strength, protected-feature list with visual markers | Retouch UI | 5 d |
| `MFE` | Mid-Level Frontend Engineer | Before/after at 100 % zoom, batch strength adjust, per-scene overrides | UI panels | 3 d |
| `QAL` | QA Lead - Automation | Texture-retention gate, false-removal gate, cross-frame consistency test, zoom artefact test | CI gates | 5 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Blind A/B against Retouch4me/Evoto/Aperty outputs judged by retouchers | Competitive study | 5 d |
| `PM` | Product Manager Agent | Own the retouch ethics policy and preset definitions; approve default = Natural | Policy + presets | 2 d |
| `PERF` | Performance Engineer | Full-resolution retouch within budget; batch throughput at export | Benchmark | 3 d |
| `SEC` | Security & Privacy Engineer | Confirm no face crops leave the device for retouching | Sign-off | 1 d |
| `DOC` | Technical Writer | Document presets, protected features and the texture guarantee | Docs merged | 3 d |

### 9.1 Handoff chain for this phase

```text
PM ethics policy -> DATA labels -> SRML models -> COL/SRG engine
                                            |
                                            v
                            SRC strength + consistency -> SFE/MFE UI
                                            |
                QAL texture gates + QAIQ competitive blind study -> MLL/PM/CTO gate
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

- Texture retention >= 0.90 band-energy ratio on all presets; Polished never below 0.80.
- Blemish recall >= 0.90 with false-removal of permanent features <= 2 % (and 0 % for tattoos).
- Cross-frame consistency: the same identity's retouch strength varies by <= 5 % across a gallery.
- Under-eye correction never exceeds its cap; no visible light patches at 100 % zoom.
- Per-skin-tone metrics show no bucket more than 10 % worse than the best bucket.
- Proxy preview matches full-resolution output within a perceptual tolerance.
- Blind study: retouchers rate AURA >= parity with the best competitor on natural appearance.

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
| Retouch at full resolution (45 MP, GPU) | <= 350 ms |
| Retouch at proxy (2048 px) | <= 45 ms |
| 1,000-image gallery retouch at export | <= 7 min |
| CPU fallback (45 MP) | <= 4 s |

Telemetry events (local-first, opt-in aggregation):

- `retouch.applied` {images, ops, preset, mean_strength, ms}
- `retouch.texture_guard` {triggered, mean_band_ratio}
- `retouch.protected` {kind, count}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Plastic skin | Hard texture-retention floor enforced in CI with automatic strength re-solve. |
| Removing permanent features (mole, tattoo, scar) | Dedicated classifier, cross-frame permanence evidence, conservative defaults, protected-feature UI, and a 0 % tattoo-removal gate. |
| Skin-tone bias in detection | Balanced labelling, per-bucket metrics, and a ship gate on parity. |
| Preview/export mismatch | Identical algorithm at both scales plus a perceptual comparison test. |
| Ethical creep toward body reshaping | Explicit written policy excluding it; any such feature requires CTO and PM approval and a separate consent design. |

## 13. Acceptance criteria

- [ ] Temporary blemishes disappear while pores, fine lines and permanent features remain.
- [ ] Freckles, moles, scars and tattoos are preserved by default and listed as protected.
- [ ] Under-eye and uneven tone corrections are visible as improvement but not as retouching.
- [ ] The same person looks like the same person across the whole gallery.
- [ ] Texture retention is measured, gated in CI and reported in the UI.
- [ ] Blind expert study shows parity or better versus leading retouch tools.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 20 - Portrait Retouch AI with Natural Texture Protection.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-20-PORTRAIT-RETOUCH-AI.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - Professional-grade automatic skin retouching: blemishes, temporary marks, dark circles and uneven tone corrected while pores, fine lines and real texture are preserved by design.

Rules:
  - Do not start Phase 21. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-retouch/src/{lib,blemish,permanent,undereye,evening,texture_guard,strength,ops}.rs`, `crates/aura-render/shaders/{inpaint_patch,freq_bands,retouch_apply}.wgsl`, `ml/models/retouch/{train_blemish.py,train_permanent.py,eval_retouch.py,export.py}`, `config/retouch_presets.toml`, `apps/desktop/src/components/develop/RetouchPanel.tsx`, `docs/model-cards/{blemish_detector,permanent_features}.md`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-20-portrait-retouch-ai and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-20.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-20-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-20-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-20-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 20 of 30 - Portrait Retouch AI with Natural Texture Protection - part of the AURA Wedding AI master build plan.*

---

# Phase 21 - Micro-Retouch Suite: Hair, Teeth, Eyes, Clothing & Glare

> **Single feature shipped by this phase:** The small fixes a retoucher makes without being asked: stray hairs tamed, teeth and eyes subtly corrected, lint and clothing distractions cleaned, glasses glare and reflections reduced.
>
> **Mission:** Close the remaining quality gap with high-end manual retouching by handling the details that photographers currently fix by hand, one image at a time.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 21 of 30 |
| Epic | E4 - Retouch & Restoration |
| Feature | The small fixes a retoucher makes without being asked: stray hairs tamed, teeth and eyes subtly corrected, lint and clothing distractions cleaned, glasses glare and reflections reduced. |
| Depends on | Phases 18, 20 |
| Unlocks | Phases 27, 28 |
| Duration | 2.5 weeks |
| Primary owners | ML Lead - Vision, Senior ML Engineer, Senior Engineer - GPU & Render (Rust / wgpu / CUDA), Colour Scientist |
| Risk level | Medium-High - subtlety and 'uncanny' risk |
| Headline KPI | flyaway reduction rated >= 4/5 with no bald patches; teeth/eye corrections judged natural >= 95 %; micro pass <= 250 ms/image full res |
| Competitor being beaten | Retouch4me's specialist modules; Evoto's portrait tools |

## 1. Why this phase exists

These are the fixes that make a delivered gallery feel finished. Photographers currently do them manually on their best 30 frames; automating them across 1,000 frames is a large, concrete time saving.

They are also where automation most easily looks creepy - whitened teeth, glowing eyes, erased hair. Doing them conservatively and identity-aware is the differentiator, not doing them harder.

## 2. Scope contract

### 2.1 In scope

- Hair intelligence: detect stray flyaways against clean backgrounds and reduce them (not erase), preserving hairline naturalness; explicitly skip complex textures and busy backgrounds.
- Teeth correction: mild luminance evening and yellow-cast reduction bounded by a natural-teeth colour locus, with a hard ceiling so no one gets fluorescent teeth.
- Eye enhancement: catchlight preservation, sclera redness reduction, iris micro-clarity, with hard caps; no eye enlargement, no colour changes.
- Clothing cleanup: lint, small stains, visible bra straps if the user enables it, stray threads, and creases only where the user opts in.
- Glare and reflection reduction on glasses: detect specular sheets over eyes, reduce or reconstruct using the other frames in the moment when available (cross-frame borrowing).
- Nostril/ear/neck micro-corrections limited to shine and shadow evening (no shape changes).
- Per-operation opt-in matrix with studio-level defaults, plus per-identity respect for protected features from Phase 20.
- Cross-frame borrowing infrastructure: use a sibling frame from the same moment as a source for reconstructing a small region (glasses glare, closed eye in a group frame is *not* included - that is deliberately excluded).

### 2.2 Explicitly out of scope (do not build it here)

- Skin texture work (Phase 20).
- Removing people or large objects (Phase 24).
- Eye or face swapping between frames - explicitly excluded as a product-ethics decision (composite portraits are not delivered without disclosure).

## 3. Architecture and data flow

```text
masks (P18) + retouch plan (P20) + moment siblings (P08)
     |
     +--> FlyawayDetector (hair-vs-background contrast) -> reduce (alpha-aware)
     +--> TeethModule (luma even + yellow reduce, locus-bounded)
     +--> EyeModule (sclera redness, iris clarity, catchlight preserve, capped)
     +--> ClothingModule (lint/stain/thread detect -> inpaint, opt-in matrix)
     +--> GlareModule (specular sheet detect -> reduce | cross-frame borrow)
                       |
              NaturalnessGuard (per-op ceilings + locus constraints)
                       |
           recipe.retouch[] micro ops + reasons + confidence
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-retouch/src/micro/{hair,teeth,eyes,clothing,glare,borrow,guard}.rs` | Micro-retouch modules. |
| `ml/models/micro/{train_flyaway.py,train_glare.py,train_lint.py,eval_micro.py}` | Detection models. |
| `config/micro_retouch.toml` | Opt-in matrix, ceilings and colour loci. |
| `apps/desktop/src/components/develop/MicroRetouchPanel.tsx` | Per-operation toggles with previews. |
| `docs/retouch-ethics.md` | What AURA will and will not do to people's appearance. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Micro-retouch operations**

```rust
pub enum MicroOp {
    Flyaway { region: Box2, strength: f32 },
    Teeth { identity: IdentityId, luma: f32, yellow_reduce: f32 },
    Eyes { identity: IdentityId, sclera: f32, iris_clarity: f32 },
    Clothing { region: Box2, kind: ClothingIssue, strength: f32 },
    Glare { region: Box2, method: GlareMethod },   // Reduce | BorrowFrom(ImageId)
}

pub struct NaturalnessGuard {
    pub teeth_max_luma: f32,        // hard ceiling
    pub teeth_locus: ColourLocus,
    pub sclera_max: f32, pub iris_max: f32,
    pub flyaway_max_area_frac: f32,
    pub require_confidence: f32,    // below this, skip the op entirely
}
```

## 6. Algorithm, model and implementation design

### 6.1 Hair without bald patches

- Detect flyaways as thin high-contrast structures outside the hair alpha but connected to it; require a clean, low-detail background, otherwise skip.
- Reduce rather than remove: attenuate contrast against the background by up to a capped amount, preserving some strands so the hairline still reads as real hair.
- Never modify inside the hair mass; a strict area cap (fraction of frame) prevents runaway edits.

### 6.2 Teeth and eyes with hard ceilings

- Teeth: even the luminance across the teeth mask and reduce yellow toward a *natural* locus derived from real teeth measurements, with a ceiling far below cosmetic whitening; skip entirely if the mask confidence is low or the mouth is small in frame.
- Eyes: reduce sclera redness (chroma only), add small iris local contrast, and explicitly protect catchlights by excluding specular pixels; no enlargement, no colour change, no whitening of the sclera beyond a cap.
- Every ceiling is in `micro_retouch.toml` with a rationale and a fixture demonstrating the maximum allowed effect.

### 6.3 Clothing and glare

- Lint/thread/stain detection as small anomaly detection restricted to the clothing mask, with inpainting reused from Phase 20; creases and wrinkles are opt-in only, since removing them can look artificial.
- Glasses glare: detect specular sheets overlapping the eye region; if a sibling frame from the same moment has the same face without glare and closely matching geometry, borrow that region with alignment and frequency blending; otherwise reduce highlight intensity conservatively.
- Cross-frame borrowing is limited to small regions, requires high alignment confidence, and is always recorded in the recipe and the Explain panel so it is never a hidden composite.

### 6.4 Ethics as engineering

- A written policy file lists forbidden operations: body reshaping, face swapping, eye replacement, skin lightening, and anything that changes identity.
- Guard code enforces the ceilings; a CI test attempts to exceed each ceiling and asserts refusal.
- Opt-in matrix means studios choose their own standards, and the delivery report states which operations were applied.

## 7. Cloud AI usage (bring-your-own API key)

No cloud AI call in this phase. The phase must work with the network cable unplugged; the Cloud AI Gateway from Phase 04 stays idle here.

## 8. Implementation order (execute literally, in this order)

1. Publish `docs/retouch-ethics.md` and get PM/CTO sign-off before implementation.
2. Label flyaway, glare, lint and teeth/eye cases; measure natural teeth and sclera loci from real data.
3. Train the flyaway, glare and lint detectors.
4. Implement hair reduction with area caps and background gating.
5. Implement teeth and eye modules with locus constraints and ceilings.
6. Implement clothing cleanup reusing Phase 20 inpainting.
7. Implement glare reduction and cross-frame borrowing with alignment.
8. Implement the naturalness guard and the opt-in matrix.
9. Build the micro-retouch panel with per-operation previews and studio defaults.
10. Run the naturalness audit and the ceiling-refusal tests.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `MLL` | ML Lead - Vision | Own detector design, locus measurement methodology and naturalness evaluation | Signed spec | 3 d |
| `SRML` | Senior ML Engineer | Train flyaway/glare/lint detectors; export and validate across skin tones and hair types | Models registered | 8 d |
| `COL` | Colour Scientist | Measure natural teeth/sclera loci; validate chroma-only operations | Locus definitions | 4 d |
| `SRG` | Senior Engineer - GPU & Render (Rust / wgpu / CUDA) | GPU implementations, cross-frame alignment and blending | Shaders + align | 6 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Module orchestration, guard enforcement, opt-in matrix, recipe ops, delivery reporting | `micro` module + tests | 6 d |
| `DATA` | Data Engineer / Dataset Curator | Labels for flyaways, glare, lint; hair-type diversity coverage | Labels v1 | 7 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Micro-retouch panel with per-op toggles, previews and studio defaults | UI shipped | 4 d |
| `QAL` | QA Lead - Automation | Ceiling-refusal tests, bald-patch detection test, catchlight preservation test | CI gates | 4 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Naturalness audit of 400 frames; specifically hunt uncanny teeth/eyes and hair damage | Audit report | 4 d |
| `PM` | Product Manager Agent | Own the ethics policy and default opt-in matrix; approve ceilings | Policy + defaults | 2 d |
| `PERF` | Performance Engineer | Keep the micro pass under budget; share masks and bands with Phase 20 | Benchmark | 2 d |
| `DOC` | Technical Writer | Publish the ethics document and per-operation guidance | Docs merged | 2 d |

### 9.1 Handoff chain for this phase

```text
PM ethics policy -> COL loci + DATA labels -> SRML detectors
                                     |
                                     v
                       SRG GPU ops + SRC guard/orchestration -> SFE UI
                                     |
                     QAL ceiling tests + QAIQ naturalness audit -> PM/CTO gate
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

- Hair: no bald patches or hairline damage on any fixture; area cap never exceeded.
- Teeth: luminance and chroma stay inside the natural locus; ceiling-exceed attempts are refused.
- Eyes: catchlights preserved (specular pixel test); no geometry change measurable.
- Clothing: lint removal recall >= 0.85 with no fabric-texture damage at 100 % zoom.
- Glare: borrowed regions align within tolerance and are always disclosed in the recipe and Explain panel.
- Forbidden operations are impossible: automated attempts to reshape or swap are rejected by guard code.
- Naturalness audit: >= 95 % of corrections judged natural by retouchers.

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
| Micro pass at full resolution | <= 250 ms |
| Micro pass at proxy | <= 35 ms |
| Cross-frame borrow (alignment + blend) | <= 180 ms |
| 1,000-image gallery | <= 5 min added at export |

Telemetry events (local-first, opt-in aggregation):

- `micro.applied` {op, count, mean_strength, ms}
- `micro.skipped` {op, reason}
- `micro.borrow` {count, alignment_score}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Uncanny results (glowing teeth, alien eyes) | Hard ceilings with locus constraints, CI refusal tests, and a naturalness audit gate. |
| Hair damage | Background gating, area caps, reduce-not-remove policy, and dedicated fixtures across hair types. |
| Hidden composites from cross-frame borrowing | Always recorded and disclosed; limited to small regions; never used for eyes or expressions. |
| Scope creep into cosmetic surgery features | Written ethics policy with CTO/PM gate on any change. |

## 13. Acceptance criteria

- [ ] Flyaway hair is calmed without damaging the hairline.
- [ ] Teeth and eyes look better but unmistakably natural, with catchlights intact.
- [ ] Lint and small clothing distractions are cleaned without harming fabric texture.
- [ ] Glasses glare is reduced, and any borrowed pixels are disclosed.
- [ ] Forbidden identity-changing operations are structurally impossible.
- [ ] Studios can configure exactly which micro-operations they allow.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 21 - Micro-Retouch Suite: Hair, Teeth, Eyes, Clothing & Glare.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-21-MICRO-RETOUCH-SUITE.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - The small fixes a retoucher makes without being asked: stray hairs tamed, teeth and eyes subtly corrected, lint and clothing distractions cleaned, glasses glare and reflections reduced.

Rules:
  - Do not start Phase 22. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-retouch/src/micro/{hair,teeth,eyes,clothing,glare,borrow,guard}.rs`, `ml/models/micro/{train_flyaway.py,train_glare.py,train_lint.py,eval_micro.py}`, `config/micro_retouch.toml`, `apps/desktop/src/components/develop/MicroRetouchPanel.tsx`, `docs/retouch-ethics.md`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-21-micro-retouch-suite and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-21.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-21-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-21-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-21-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 21 of 30 - Micro-Retouch Suite: Hair, Teeth, Eyes, Clothing & Glare - part of the AURA Wedding AI master build plan.*

---

# Phase 22 - Restoration Stack: Scene-Aware Denoise, Selective Sharpen & Face Recovery

> **Single feature shipped by this phase:** RAW-level denoising, deconvolution sharpening and gentle face recovery applied only where they help - the Topaz/DxO capability integrated into the wedding workflow instead of bolted on.
>
> **Mission:** Rescue the frames weddings actually produce - ISO 12800 dance floors, candlelit vows, slightly soft ceremony moments - without the smeared, over-sharpened, AI-looking output that restoration tools are notorious for.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 22 of 30 |
| Epic | E4 - Retouch & Restoration |
| Feature | RAW-level denoising, deconvolution sharpening and gentle face recovery applied only where they help - the Topaz/DxO capability integrated into the wedding workflow instead of bolted on. |
| Depends on | Phases 09, 14, 18 |
| Unlocks | Phases 27, 28, 30 |
| Duration | 3 weeks |
| Primary owners | Colour Scientist, ML Lead - Vision, Senior ML Engineer, Performance Engineer |
| Risk level | High - heavy compute and easy to overdo |
| Headline KPI | denoise PSNR/SSIM beats bilinear baseline decisively; expert preference >= 80 % vs no-denoise at ISO >= 6400; no plastic-face artefacts on 99 % of frames; denoise <= 2.5 s per 45 MP on GPU |
| Competitor being beaten | DxO DeepPRIME, Topaz Photo AI, Lightroom AI Denoise |

## 1. Why this phase exists

Wedding receptions are dark. Being able to deliver clean ISO 12800 dance-floor frames materially increases how many photographs a photographer can sell, which is a direct revenue argument for the product.

Restoration tools are usually separate applications with their own import/export cycle. Making restoration a decision inside the pipeline - applied only where the analysis says it helps - is both faster and better.

Restraint is the differentiator: applying denoise and sharpening everywhere is what makes AI-processed images look synthetic. Frame-level decisions based on Phase 09 evidence avoid that.

## 2. Scope contract

### 2.1 In scope

- RAW-domain denoising: a learned denoiser operating on demosaiced linear data with the camera's noise model (ISO, sensor, black level) as conditioning; three strength tiers plus off.
- Decision logic: apply denoise only when Phase 09's noise estimate exceeds the scene tolerance, and choose strength from noise level, subject prominence and output size.
- Selective sharpening: deconvolution-based sharpening driven by estimated blur kernel and masked to detail regions; explicitly attenuated on skin, sky and bokeh.
- Face recovery: gentle restoration of slightly soft faces using a face-prior model with a hard identity-preservation constraint and strict strength caps.
- Order-of-operations correctness: denoise before local retouch and sharpening; sharpening as the last pixel operation before output transform; documented and enforced in the render graph.
- Cloud offload option for heavy restoration when the local GPU is weak (via Phase 04 governance) with local fallback and explicit user consent per project.
- Artefact detection self-check: measure texture smearing, ringing and identity drift; reduce strength automatically if thresholds are exceeded.
- Batch scheduling that keeps restoration off the interactive path: it runs at export or in a background pass, never blocking the editor.

### 2.2 Explicitly out of scope (do not build it here)

- Upscaling beyond native resolution (explicitly out of scope for V1 - weddings do not need it and it invites artefacts).
- Generative reconstruction of missing content (Phase 24).
- Motion-blur removal on heavily blurred frames (rejected in Phase 12 instead of rescued).

## 3. Architecture and data flow

```text
integrity (P09): noise_sigma_rel, motion, focus | scene tolerance | masks (P18)
     |
  decision: denoise? strength? sharpen? face recovery?
     |
 RAW linear --> [ Denoiser (noise-model conditioned) ] --> local retouch (P20/21)
                                     |
                    [ Deconvolution Sharpen (kernel-estimated, masked) ]
                                     |
                    [ FaceRecovery (prior + identity constraint, capped) ]
                                     |
                  ArtefactSelfCheck (smear / ringing / identity drift)
                                     |
                       reduce strength & retry  |  accept -> output transform
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-restore/src/{lib,denoise,sharpen,kernel,face_recovery,decide,selfcheck,schedule}.rs` | Restoration engine. |
| `ml/models/restore/{train_denoise.py,train_face_recovery.py,eval_restore.py,export.py}` | Model training with paired noisy/clean data. |
| `config/noise_models/<camera>.toml` | Per-camera noise models measured by COL. |
| `crates/aura-render/shaders/{deconv,denoise_tile}.wgsl` | GPU implementations. |
| `apps/desktop/src/components/develop/RestorePanel.tsx` | Strength tiers, per-image override, preview. |
| `docs/model-cards/{denoise,face_recovery}.md` | Model cards including identity-preservation metrics. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Restoration plan**

```rust
pub struct RestorePlan {
    pub image_id: ImageId,
    pub denoise: DenoiseTier,           // Off | Light | Standard | Strong
    pub denoise_reason: Vec<Reason>,
    pub sharpen: Option<SharpenSpec>,   // { kernel_sigma, amount, mask, skin_attenuation }
    pub face_recovery: Option<f32>,     // strength 0..0.4, capped
    pub run_where: RunWhere,            // LocalGpu | LocalCpu | Cloud
    pub selfcheck: Option<ArtefactReport>,
    pub confidence: f32,
}
```

## 6. Algorithm, model and implementation design

### 6.1 Denoising that respects the sensor

- Condition the denoiser on the measured per-camera noise model (read noise, shot noise slope per ISO) so it removes the right amount rather than a learned average.
- Train on real paired data where possible (bracketed low-ISO reference versus high-ISO capture on the same scene) plus synthetic noise matched to the measured model.
- Preserve chroma detail separately from luminance detail; wedding fabrics and skin suffer most from chroma smearing.
- Strength selection is evidence-based: the tier is chosen from the measured sigma relative to the scene tolerance, not from a global preference.

### 6.2 Sharpening only where it helps

- Estimate the blur kernel width from edge profiles; if blur is dominated by motion or gross defocus, do not sharpen (Phase 12 already rejected the worst cases).
- Use Richardson-Lucy-style deconvolution with a small iteration count and edge-aware damping to avoid ringing; mask out skin, sky and bokeh regions from Phase 18.
- Cap the amount by the noise level after denoising, because sharpening amplifies residual noise.

### 6.3 Face recovery with an identity constraint

- Apply a face-prior restoration model only when the face is slightly soft (a narrow band of measured sharpness), never on heavily blurred faces where the model would hallucinate.
- Hard identity constraint: compute the Phase 06 face embedding before and after; if cosine distance exceeds a small threshold, reduce strength and retry, and if it still fails, skip and record the reason. This is the guarantee that the product never changes what someone looks like.
- Cap strength at 0.4 and blend with the original at high frequencies to keep skin realistic.

### 6.4 Scheduling and self-check

- Restoration never runs on the interactive path; it runs during export or as an explicit background enhancement pass with progress and cancellation.
- Self-check measures band-energy smearing, ringing near edges and identity drift; violations reduce strength automatically and log a reason.
- Cloud offload only with explicit consent, only for derivative-scale data where possible, governed by Phase 04's budget and privacy rules.

## 7. Cloud AI usage (bring-your-own API key)

No cloud AI call in this phase. The phase must work with the network cable unplugged; the Cloud AI Gateway from Phase 04 stays idle here.

## 8. Implementation order (execute literally, in this order)

1. Measure noise models for the top 20 camera bodies (COL) and commit the config files.
2. Collect and/or synthesise paired training data matched to those noise models.
3. Train and export the denoiser; validate per camera and per ISO.
4. Implement kernel estimation and masked deconvolution sharpening.
5. Train the face-recovery model and implement the identity-preservation constraint.
6. Implement the decision logic tied to Phase 09 evidence and scene tolerances.
7. Implement the artefact self-check with automatic strength reduction.
8. Implement scheduling (export-time and background pass) with cancellation.
9. Wire the optional cloud offload path with consent and fallback.
10. Build the restore panel; run expert preference studies at ISO 3200/6400/12800/25600.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `COL` | Colour Scientist | Measure per-camera noise models; validate denoise fidelity and colour integrity | Noise models + report | 9 d |
| `MLL` | ML Lead - Vision | Own denoiser/face-recovery design, identity constraint, evaluation protocol | Signed spec + gates | 4 d |
| `SRML` | Senior ML Engineer | Train and export denoise and face-recovery models; tile-based inference; quantisation | Models registered | 11 d |
| `SRG` | Senior Engineer - GPU & Render (Rust / wgpu / CUDA) | GPU tiled denoise/deconvolution, VRAM-safe scheduling, CPU fallback | GPU path | 7 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Decision logic, self-check, scheduling, recipe integration, cloud offload plumbing | `aura-restore` + tests | 7 d |
| `DATA` | Data Engineer / Dataset Curator | Paired noisy/clean captures across 20 bodies and 6 ISO steps; soft-face labelled set | Dataset v4 | 12 d |
| `PERF` | Performance Engineer | Hit the 2.5 s/45 MP budget; tile sizing; batch export throughput; thermal behaviour | Benchmark report | 5 d |
| `QAL` | QA Lead - Automation | PSNR/SSIM gates, identity-drift gate, ringing test, order-of-operations test | CI gates | 5 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Expert preference study at four ISO levels vs DxO/Topaz/Lightroom outputs | Competitive study | 5 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Restore panel with tiers, per-image override, 100 % zoom preview | UI shipped | 3 d |
| `AGT` | AI Agent & Prompt Engineer | Cloud offload task definition, consent flow, budget integration | Cloud path | 2 d |
| `DEVOPS` | DevOps / Release Engineer | Ship large model artefacts efficiently; delta updates; download resumption | Distribution | 3 d |
| `DOC` | Technical Writer | Document tiers, when restoration applies, and the identity guarantee | Docs merged | 2 d |

### 9.1 Handoff chain for this phase

```text
COL noise models -> DATA paired captures -> SRML models -> SRG GPU tiling
                                        |
                                        v
                        SRC decisions + self-check -> AGT cloud offload (optional)
                                        |
              PERF budgets + QAL gates + QAIQ competitive study -> COL/MLL gate
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

- Denoise: expert preference >= 80 % versus no-denoise at ISO >= 6400; chroma detail preserved on fabric fixtures.
- Identity preservation: face embedding distance after face recovery below threshold on 100 % of fixtures, or the operation is skipped.
- Sharpening: no ringing above threshold; skin and bokeh measurably unaffected.
- Order of operations enforced (denoise before retouch, sharpen last) - render-graph test.
- Self-check reduces strength automatically on adversarial smear fixtures.
- Performance budget met on the reference GPU; VRAM never exceeded on 100 MP files.
- Cloud offload declines gracefully when disabled, with identical decisions run locally.

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
| Denoise 45 MP (RTX 4070) | <= 2.5 s |
| Denoise 45 MP (M3 Pro) | <= 5 s |
| Denoise 45 MP (CPU int8) | <= 40 s |
| Sharpen + face recovery 45 MP | <= 1.2 s |
| Restoration share of a 1,000-image export | <= 45 min on the reference GPU |

Telemetry events (local-first, opt-in aggregation):

- `restore.applied` {tier, images, ms, backend}
- `restore.selfcheck` {reduced_count, reason_histogram}
- `restore.identity_guard` {skipped_count, mean_distance}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Over-processed, synthetic-looking output | Evidence-based decisions, conservative tiers, self-check with automatic reduction, and expert preference gates. |
| Face recovery changes identity | Hard embedding-distance constraint with skip-on-failure and a 100 % CI gate. |
| Restoration blows the time budget | Applied only where needed, scheduled off the interactive path, tiled GPU execution, and a PERF-owned budget. |
| Model size bloats the installer | Optional model packs downloaded on demand with signed manifests and delta updates. |

## 13. Acceptance criteria

- [ ] High-ISO reception frames become deliverable, with fabric and skin detail intact.
- [ ] Sharpening appears only where it helps and never on skin or bokeh.
- [ ] Slightly soft faces improve without anyone's identity changing.
- [ ] Restoration decisions are explained and overridable per image.
- [ ] Export budgets are met on the reference machines.
- [ ] Competitive study shows parity or better versus dedicated restoration tools on wedding content.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 22 - Restoration Stack: Scene-Aware Denoise, Selective Sharpen & Face Recovery.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-22-RESTORATION-STACK.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - RAW-level denoising, deconvolution sharpening and gentle face recovery applied only where they help - the Topaz/DxO capability integrated into the wedding workflow instead of bolted on.

Rules:
  - Do not start Phase 23. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-restore/src/{lib,denoise,sharpen,kernel,face_recovery,decide,selfcheck,schedule}.rs`, `ml/models/restore/{train_denoise.py,train_face_recovery.py,eval_restore.py,export.py}`, `config/noise_models/<camera>.toml`, `crates/aura-render/shaders/{deconv,denoise_tile}.wgsl`, `apps/desktop/src/components/develop/RestorePanel.tsx`, `docs/model-cards/{denoise,face_recovery}.md`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-22-restoration-stack and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-22.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-22-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-22-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-22-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 22 of 30 - Restoration Stack: Scene-Aware Denoise, Selective Sharpen & Face Recovery - part of the AURA Wedding AI master build plan.*

---

# Phase 23 - Geometry Suite: Lens Corrections, Straightening AI & Smart Crop

> **Single feature shipped by this phase:** Automatic lens profile corrections, horizon and architecture levelling, and subject-aware smart cropping that improves composition without ever cutting off what matters.
>
> **Mission:** Finish the frame. Correct optics, level the world, and crop with judgement - the last step before a gallery looks deliberately composed rather than merely captured.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 23 of 30 |
| Epic | E4 - Retouch & Restoration |
| Feature | Automatic lens profile corrections, horizon and architecture levelling, and subject-aware smart cropping that improves composition without ever cutting off what matters. |
| Depends on | Phases 06, 11, 14 |
| Unlocks | Phases 27, 29, 30 |
| Duration | 1.5 weeks |
| Primary owners | Colour Scientist, Senior Engineer - Core Pipeline (Rust), ML Lead - Vision |
| Risk level | Medium |
| Headline KPI | straightening within 0.3 deg of expert on 90 % of frames; zero crops that cut a primary subject's face; geometry pass <= 40 ms/image |
| Competitor being beaten | Lightroom lens profiles and auto-straighten; Capture One keystone tools |

## 1. Why this phase exists

Lens corrections are table stakes for a serious RAW editor, and getting them right requires real profile data - it is a credibility feature.

Tilted horizons are one of the most common complaints about delivered wedding galleries; automatic levelling with intent detection fixes it at scale.

Smart crop is where automation is most dangerous, so a subject-aware, conservative, always-reversible crop is a trust feature as much as a quality feature.

## 2. Scope contract

### 2.1 In scope

- Lens profile corrections: distortion, vignetting and chromatic aberration using embedded profiles plus a bundled profile database, with per-lens overrides.
- Manual-lens fallback: estimate distortion from long straight edges when no profile exists.
- Straightening: apply Phase 11's horizon estimate with confidence gating, plus architectural vertical correction (keystone) where it improves the frame without excessive stretching.
- Perspective correction limits: never exceed a documented stretch factor; refuse when correction would crop into a primary subject.
- Smart crop: propose crops that improve composition using Phase 11's hints, respecting aspect-ratio options (original, 4:5, 5:4, 1:1, 16:9 for social), with hard protection of primary subjects and all detected faces.
- Crop safety rules: never cut a primary identity's face or hands, never crop below a resolution floor, never crop out a must-have subject, always keep the moment's key content.
- Multi-aspect delivery: generate additional crop variants for social/album use without duplicating files (crop variants live in the recipe).
- All geometry recorded in the recipe, fully reversible, with the original framing always recoverable.

### 2.2 Explicitly out of scope (do not build it here)

- Content-aware fill after rotation (Phase 24 can fill corners when enabled).
- Choosing which crops are used for album layouts (Phase 29).
- Perspective composites or panoramas.

## 3. Architecture and data flow

```text
EXIF lens + profile DB --> distortion/vignette/CA correction
     |
     +--> horizon (P11) + confidence gate --> rotate
     +--> vertical lines --> keystone (limited, stretch-capped)
     |
     v
  SmartCrop proposer: composition hints (P11) + faces (P06) + moment content
     |
  safety filter: primary faces intact? resolution floor? must-have content kept?
     |
  crop variants { original, primary, 4:5, 16:9 } written to recipe.geometry
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-geometry/src/{lib,lens,profiles,straighten,keystone,crop,safety,variants}.rs` | Geometry engine. |
| `assets/lens_profiles/` | Bundled lens profile database with attribution. |
| `crates/aura-render/shaders/geometry.wgsl` | GPU resampling with high-quality filtering. |
| `config/crop_rules.toml` | Aspect options, safety margins, resolution floors. |
| `apps/desktop/src/components/develop/GeometryPanel.tsx` | Crop/straighten UI with AI proposal and revert. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Geometry decisions**

```rust
pub struct GeometryPlan {
    pub image_id: ImageId,
    pub lens: LensCorrection,            // { distortion, vignette, ca, profile_id, source }
    pub rotate_deg: f32, pub rotate_conf: f32,
    pub keystone: Option<Keystone>,      // stretch-capped
    pub crops: Vec<CropVariant>,         // { aspect, rect, purpose, score, safe }
    pub primary_crop: usize,
    pub safety: CropSafetyReport,        // { faces_intact, resolution_ok, content_kept }
    pub reasons: Vec<Reason>, pub confidence: f32,
}
```

## 6. Algorithm, model and implementation design

### 6.1 Lens corrections

- Prefer embedded correction data, then the bundled profile database keyed by lens id and focal length, then geometric estimation from straight edges.
- Apply corrections in linear light before creative operations so vignette correction does not fight exposure decisions.
- Chromatic aberration correction is per-channel radial scaling with sub-pixel accuracy, validated on high-contrast edge fixtures.

### 6.2 Straightening with restraint

- Rotate only when Phase 11 horizon confidence >= 0.7 and the correction is between 0.2 and 8 degrees; larger tilts are treated as intentional and left alone.
- Keystone correction is limited to frames with strong architectural verticals and capped so no axis is stretched beyond a documented factor.
- Rotation implies cropping; the crop is computed to stay inside the safety rules, and if it cannot, the rotation is reduced or skipped.

### 6.3 Smart crop with hard safety rules

- Generate candidate crops by optimising a composition objective (subject placement, balance, edge cleanliness, headroom) over translation/scale within a bounded search.
- Hard constraints: every detected face fully inside, primary identities' hands and joined hands inside, resolution >= 60 % of the original long edge, and the moment's key content preserved.
- Score improvement gate: a proposed crop must improve the composition score by a minimum margin, otherwise the original framing wins - most frames should keep their original crop.
- Aspect variants for social and album use are generated as additional entries so the delivered gallery keeps native framing while Phase 29 can use the variants.

## 7. Cloud AI usage (bring-your-own API key)

No cloud AI call in this phase. The phase must work with the network cable unplugged; the Cloud AI Gateway from Phase 04 stays idle here.

## 8. Implementation order (execute literally, in this order)

1. Integrate the lens profile database and implement corrections with linear-light ordering.
2. Implement the manual-lens distortion estimator.
3. Implement rotation with confidence gating and rotation-induced crop computation.
4. Implement keystone with stretch caps and refusal conditions.
5. Implement crop candidate generation and the composition objective.
6. Implement the safety filter and the improvement-margin gate.
7. Implement aspect variants and recipe integration.
8. Build the geometry UI with AI proposals, manual override and revert-to-original.
9. Validate on architecture, portrait and group fixtures; run the safety audit.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `COL` | Colour Scientist | Lens correction correctness, CA sub-pixel accuracy, resampling quality validation | Validated corrections | 5 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Straightening, keystone, crop search, safety filter, variants, recipe integration | `aura-geometry` + tests | 7 d |
| `MLL` | ML Lead - Vision | Define the crop objective and improvement margin; evaluate against expert crops | Objective spec | 3 d |
| `SRG` | Senior Engineer - GPU & Render (Rust / wgpu / CUDA) | GPU resampling with high-quality filters; verify no aliasing at export | Shader + tests | 3 d |
| `DATA` | Data Engineer / Dataset Curator | Expert crop labels on 2k frames; architecture and tilt sets | Labels v1 | 4 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Geometry panel with proposal preview, aspect switcher, revert, manual crop | UI shipped | 4 d |
| `QAL` | QA Lead - Automation | Angle gates, zero-face-cut gate, resolution-floor test, aliasing test | CI gates | 3 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Audit 300 auto-crops: any cut hand, cut face or worse framing is a bug | Audit report | 3 d |
| `PM` | Product Manager Agent | Approve `crop_rules.toml`, aspect list and the conservative default (keep original framing) | Approved config | 1 d |
| `PERF` | Performance Engineer | Keep geometry under 40 ms; fuse resampling with the render graph | Benchmark | 1 d |
| `DOC` | Technical Writer | Document lens profile coverage and crop safety rules | Docs merged | 2 d |

### 9.1 Handoff chain for this phase

```text
COL lens corrections -> SRC straighten/crop -> MLL objective tuning
                                  |
                                  v
                        SRG resampling -> SFE geometry UI
                                  |
                    QAL safety gates + QAIQ crop audit -> COL/PM gate
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

- Straightening within 0.3 deg of expert on >= 90 % of labelled frames; intentional tilts untouched.
- Zero auto-crops cut a detected face or a primary identity's hands (hard gate).
- Resolution floor respected on every crop; no crop below the documented threshold.
- CA correction removes fringing on high-contrast fixtures without introducing colour shifts.
- Keystone never exceeds the stretch cap and is skipped when it would violate crop safety.
- Most frames (>= 70 %) keep their original framing - the system is conservative by design.
- Revert-to-original restores exact framing.

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
| Geometry decisions per image | <= 40 ms |
| Resampling overhead at export (45 MP) | <= 120 ms |
| 1,000 selected images total | <= 45 s decisions |

Telemetry events (local-first, opt-in aggregation):

- `geometry.applied` {rotate_count, mean_deg, keystone_count, crop_count}
- `geometry.crop_refused` {reason_histogram}
- `geometry.lens_profile_missing` {lens_id}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Auto-crop cuts something important | Hard safety constraints with a zero-tolerance CI gate, improvement-margin requirement, and conservative defaults. |
| Straightening ruins intentional tilts | Confidence gating plus Phase 11 intent detection plus an angle band. |
| Missing lens profiles | Estimation fallback, telemetry on missing lenses, and a monthly profile expansion task. |
| Resampling softens images | High-quality filtering validated by COL, and geometry applied once in the render graph rather than repeatedly. |

## 13. Acceptance criteria

- [ ] Lens distortion, vignetting and fringing are corrected automatically where profiles exist.
- [ ] Tilted horizons are levelled; creative tilts are preserved.
- [ ] Smart crop improves framing only when it clearly helps, and never cuts faces or hands.
- [ ] Social and album aspect variants are available without duplicating files.
- [ ] Original framing is always one click away.
- [ ] Geometry is applied once, at high quality, inside the render graph.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 23 - Geometry Suite: Lens Corrections, Straightening AI & Smart Crop.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-23-GEOMETRY-SUITE.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - Automatic lens profile corrections, horizon and architecture levelling, and subject-aware smart cropping that improves composition without ever cutting off what matters.

Rules:
  - Do not start Phase 24. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-geometry/src/{lib,lens,profiles,straighten,keystone,crop,safety,variants}.rs`, `assets/lens_profiles/`, `crates/aura-render/shaders/geometry.wgsl`, `config/crop_rules.toml`, `apps/desktop/src/components/develop/GeometryPanel.tsx`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-23-geometry-suite and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-23.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-23-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-23-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-23-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 23 of 30 - Geometry Suite: Lens Corrections, Straightening AI & Smart Crop - part of the AURA Wedding AI master build plan.*

---

# Phase 24 - Generative Cleanup & Distraction Removal (safe by construction)

> **Single feature shipped by this phase:** Distracting objects, background clutter, stray limbs, signage, bins, cables and photobombing strangers are removed automatically - but only where removal is safe, small and defensible.
>
> **Mission:** Give photographers the Photoshop cleanup pass they never have time for, with hard safety rules that prevent the product from ever inventing wedding content.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 24 of 30 |
| Epic | E4 - Retouch & Restoration |
| Feature | Distracting objects, background clutter, stray limbs, signage, bins, cables and photobombing strangers are removed automatically - but only where removal is safe, small and defensible. |
| Depends on | Phases 11, 18, 22 |
| Unlocks | Phases 27, 28 |
| Duration | 3 weeks |
| Primary owners | ML Lead - Vision, Senior ML Engineer, Senior Engineer - GPU & Render (Rust / wgpu / CUDA), Security & Privacy Engineer |
| Risk level | High - generative output is the easiest way to destroy trust |
| Headline KPI | artefact-free rate >= 98 % on approved removals; zero removals touching a person's face or body of a primary identity; cleanup <= 3 s per region at full resolution |
| Competitor being beaten | Lightroom Generative Remove; Photoshop Generative Fill; Evoto background tools |

## 1. Why this phase exists

Distraction removal is one of the last manual tasks left after culling and editing. Automating the small, safe 80 % (exit signs, tape on floors, water bottles, cables, background bins) removes real hours from a wedding delivery.

Generative tools fail publicly and embarrassingly. Making safety structural - size limits, semantic denylists, identity protection, confidence gating and mandatory disclosure - is the only responsible way to ship this, and it becomes a marketing advantage over competitors who let users generate anything.

## 2. Scope contract

### 2.1 In scope

- Distraction detection: learned detector for common wedding distractions (exit signs, bins, cables, gaffer tape, water bottles, chairs at frame edge, phone screens, stray hands, background strangers), plus saliency-based 'unexplained attention' detection from Phase 11.
- Removal engine: two tiers - (1) classical content-aware fill / patch synthesis for small, textured regions; (2) local diffusion inpainting for larger regions, run locally when a model pack is installed or via the Phase 04 cloud path with explicit consent.
- Safety engine (the core of this phase): size caps (region <= 4 % of frame by default), semantic denylist (never inside faces, hands, dresses, rings, cake, or any primary identity's body), identity protection, structure protection (no removal that requires inventing architecture across a long span), and confidence gating.
- Cross-frame source preference: where a sibling frame in the same moment shows the same background without the distraction, borrow real pixels instead of generating (always preferred, always disclosed).
- Artefact self-check: run a detector over the result to catch typical inpainting failures (repeated texture, warped lines, ghost limbs) and revert automatically on failure.
- Human-in-the-loop by default: proposals are shown as a review queue with before/after; Zero-Touch mode may auto-apply only tier-1 (classical) removals above a high confidence.
- Disclosure: every generated or borrowed region recorded in the recipe, the ledger, the Explain panel and the delivery report.

### 2.2 Explicitly out of scope (do not build it here)

- Adding content that never existed (sky replacement, new people, new decor) - forbidden by policy.
- Face or expression swapping (forbidden, see Phase 21 ethics).
- Removing guests the client dislikes - a human decision, offered only as a manual tool with explicit confirmation.

## 3. Architecture and data flow

```text
masks (P18) + composition flags (P11) + moment siblings (P08)
     |
  DistractionDetector -> candidates { box, class, salience, removable_prob }
     |
  SAFETY ENGINE: size cap | semantic denylist | identity protect | structure check | confidence
     |            (fail -> discard candidate, record reason)
     v
  source selection:  sibling-frame borrow (preferred)  |  classical fill  |  diffusion inpaint
     |
  ArtefactSelfCheck (repeat texture / warped lines / ghost limbs) -> revert on failure
     |
  proposal queue (default) | auto-apply tier-1 only in Zero-Touch
     |
  recipe.cleanup[] + disclosure in ledger, Explain panel and delivery report
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-generative/src/{lib,detect,safety,denylist,borrow,fill,inpaint,selfcheck,queue}.rs` | Cleanup engine and safety. |
| `ml/models/generative/{train_distraction.py,train_artefact.py,eval_cleanup.py}` | Detector and artefact-checker training. |
| `config/cleanup_policy.toml` | Size caps, denylists, autonomy rules - CTO/PM/SEC co-owned. |
| `apps/desktop/src/routes/cleanup/{ProposalQueue,BeforeAfter,ManualRemove}.tsx` | Review queue and manual tool. |
| `docs/generative-policy.md` | Public statement of what AURA will and will not generate. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Cleanup proposal and safety verdict**

```rust
pub struct CleanupProposal {
    pub id: ProposalId, pub image_id: ImageId,
    pub region: Box2, pub class: DistractionClass,
    pub area_frac: f32, pub salience: f32,
    pub method: CleanupMethod,          // BorrowFrom(ImageId) | ClassicalFill | Inpaint{model}
    pub safety: SafetyVerdict,
    pub confidence: f32,
    pub preview: Option<PreviewRef>,
    pub autonomy: Autonomy,             // from Phase 13 policy, raised one band
}

pub struct SafetyVerdict {
    pub allowed: bool,
    pub checks: Vec<(SafetyCheck, bool)>,   // SizeCap, Denylist, IdentityProtect, StructureSpan, Confidence
    pub blocked_reason: Option<String>,
}
```

## 6. Algorithm, model and implementation design

### 6.1 Detection with an explicit vocabulary

- Train a detector on a labelled wedding-distraction vocabulary rather than relying on generic saliency, because 'what is distracting at a wedding' is domain knowledge (a bin is; a candle is not).
- Combine with unexplained-salience regions: high visual attention that is not a subject, not decor and not part of the story.
- Rank candidates by (salience x removability) and cap the number per image (default 3) so cleanup stays a light touch.

### 6.2 Safety engine - structural, not advisory

- Size cap: default 4 % of frame area; larger regions require explicit user action, never automation.
- Semantic denylist: intersect the region with masks for faces, skin, hands, dress, rings, cake, primary identities' bodies; any overlap above 1 % blocks the proposal.
- Structure span check: if the region crosses a long straight architectural line or a repeating pattern boundary, block automation (inpainting warps these predictably).
- Identity protection: a background stranger may be removed only if fully separated from primary subjects, small, and near the frame edge - otherwise it becomes a manual, confirmed action.
- Every blocked proposal records which check failed, which makes the system auditable and teaches users what it will not do.

### 6.3 Source preference: real pixels first

- Search sibling frames in the same moment for the same background region without the distraction; if alignment confidence is high, homography-align and blend real pixels.
- Classical fill for small textured regions (grass, carpet, wall) is preferred over diffusion because it cannot hallucinate structure.
- Diffusion inpainting is the last resort, restricted by all safety checks, and always disclosed.

### 6.4 Self-check and autonomy

- An artefact classifier trained on known-bad inpaints scores the result; failures revert automatically and the proposal is marked 'not safely removable'.
- Autonomy is raised one band relative to Phase 13 defaults: tier-1 classical/borrow removals may auto-apply at >= 0.97 calibrated confidence in Zero-Touch; diffusion always requires review unless the studio explicitly opts in.
- The delivery report lists every cleanup performed, which protects the photographer's relationship with their client.

## 7. Cloud AI usage (bring-your-own API key)

**Judge whether removing a detected object is editorially safe and appropriate for a wedding gallery**

| Aspect | Specification |
|---|---|
| Model class | Vision reasoning tier, temperature 0 |
| Trigger | Only for candidates that pass all mechanical safety checks but have removability confidence between 0.6 and 0.9 |
| Input sent | Cropped region with context (1024 px), the detected class, area fraction, scene label, and the proposed method |
| Cost control | <= 20 calls per wedding; cached; skipped when cloud is off |
| Offline fallback | Do not remove; leave the proposal in the review queue for the user |

System prompt contract:

```text
You are a cautious wedding retouching supervisor reviewing a proposed object removal.
Input: an image region with context, the detected object class, its size, and the scene.
Task: decide whether removing this object is safe and appropriate, or whether it should be left alone.
Rules:
- Say NO if the object is part of the wedding story (decor, ritual items, gifts, cake, signage naming the couple, guests interacting).
- Say NO if removal would require inventing structure, or if the object overlaps a person.
- Say YES only for genuinely extraneous clutter (bins, cables, tape, bottles, stands, unrelated signage) that is clearly not part of the event.
- When uncertain, say NO. Leaving a distraction is always better than damaging a photograph.
- Return ONLY JSON matching the schema.
```

Required JSON response schema (validated; invalid = retry once, then fall back to local model):

```json
{
  "type": "object",
  "required": ["remove", "confidence", "reasons"],
  "properties": {
    "remove": { "type": "boolean" },
    "story_relevant": { "type": "boolean" },
    "confidence": { "type": "number", "minimum": 0, "maximum": 1 },
    "reasons": { "type": "array", "items": { "type": "string" }, "maxItems": 4 }
  },
  "additionalProperties": false
}
```

## 8. Implementation order (execute literally, in this order)

1. Publish `docs/generative-policy.md`; CTO, PM and SEC co-sign before any code.
2. Label the wedding-distraction vocabulary and train the detector.
3. Implement the safety engine first, with tests, before any removal code exists.
4. Implement sibling-frame borrowing with homography alignment.
5. Implement classical content-aware fill on GPU.
6. Integrate a local diffusion inpainting model pack (optional download) and the cloud path.
7. Train the artefact classifier and wire automatic revert.
8. Implement the proposal queue, autonomy rules and disclosure records.
9. Add the cloud editorial-judgement task for mid-confidence candidates.
10. Run the adversarial safety audit: attempt to make the system damage a photograph.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `CTO` | Chief Architect / CTO Agent | Co-sign the generative policy; own the rule that AURA never adds wedding content | Signed policy | 1 d |
| `MLL` | ML Lead - Vision | Own detector and artefact-classifier design, evaluation and confidence calibration | Signed spec + gates | 4 d |
| `SRML` | Senior ML Engineer | Train distraction detector and artefact classifier; integrate inpainting model pack | Models registered | 10 d |
| `SRG` | Senior Engineer - GPU & Render (Rust / wgpu / CUDA) | GPU classical fill, homography alignment, tiled inpainting execution, VRAM safety | GPU cleanup path | 8 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Safety engine, denylist intersection, proposal queue, disclosure records, recipe ops | `aura-generative` + tests | 8 d |
| `SEC` | Security & Privacy Engineer | Adversarial safety review; verify denylist cannot be bypassed; consent flow for cloud inpainting | Security sign-off | 4 d |
| `DATA` | Data Engineer / Dataset Curator | Distraction vocabulary labels on 10k frames; known-bad inpaint set for the artefact classifier | Labels v1 | 9 d |
| `AGT` | AI Agent & Prompt Engineer | Editorial-judgement cloud task with cautious prompt and cassettes | Cloud path live | 2 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Proposal queue with before/after, accept/reject all, manual removal tool | Cleanup UI | 5 d |
| `QAL` | QA Lead - Automation | Safety-bypass tests, artefact-rate gate, denylist coverage, disclosure completeness | CI gates | 5 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Adversarial audit: 300 attempts to induce damage; every success is a release blocker | Audit report | 5 d |
| `PM` | Product Manager Agent | Own `cleanup_policy.toml` defaults and the Zero-Touch autonomy decision | Approved policy | 2 d |
| `PERF` | Performance Engineer | Keep cleanup off the interactive path; budget per region; batch scheduling | Benchmark | 3 d |
| `DOC` | Technical Writer | Publish the generative policy and the disclosure explanation for clients | Docs merged | 2 d |

### 9.1 Handoff chain for this phase

```text
CTO/PM/SEC policy -> SRC safety engine (tests first) -> DATA labels -> SRML models
                                          |
                                          v
                          SRG fill/borrow/inpaint -> AGT editorial judgement
                                          |
              SFE proposal queue -> QAIQ adversarial audit -> SEC sign-off -> release gate
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

- Safety: no proposal overlapping a face, hand, dress, ring or primary identity body is ever allowed (exhaustive fixture sweep).
- Size cap and structure-span checks cannot be bypassed by any code path (property tests).
- Artefact-free rate >= 98 % on approved removals; failures revert automatically.
- Sibling borrowing is preferred whenever available (measured on fixtures).
- Every applied cleanup appears in the recipe, the ledger and the delivery report.
- With cloud disabled and no model pack, tier-1 cleanup still works and tier-2 is cleanly unavailable.
- Adversarial audit produces zero damaged photographs.

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
| Classical fill per region (45 MP) | <= 400 ms |
| Sibling borrow per region | <= 700 ms |
| Diffusion inpaint per region (local GPU) | <= 3 s |
| Detection per image | <= 45 ms |
| Cleanup share of a 1,000-image export | <= 12 min |

Telemetry events (local-first, opt-in aggregation):

- `cleanup.proposed` {class, area_frac, method, confidence}
- `cleanup.blocked` {check, class}
- `cleanup.applied` {method, count, ms}
- `cleanup.reverted` {artefact_reason, count}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Generative artefacts in a delivered gallery | Artefact self-check with automatic revert, review-by-default, tier preference for real pixels, and a 98 % artefact-free gate. |
| Removing something meaningful to the couple | Story-relevance denylist, cautious cloud judgement, conservative size caps, and full disclosure so the photographer can check. |
| Safety bypass through a new code path | Single choke-point API, property tests, SEC adversarial review, and a lint forbidding direct calls to fill/inpaint. |
| Model pack size and licensing | Optional download, signed manifests, and a legal review of model licences before shipping. |

## 13. Acceptance criteria

- [ ] Common wedding distractions are detected and proposed for removal with previews.
- [ ] Nothing overlapping people, dresses, rings or cake can ever be auto-removed.
- [ ] Real pixels from sibling frames are preferred over generated pixels.
- [ ] Failed inpaints revert themselves before the user ever sees them.
- [ ] Every cleanup is disclosed in the recipe and the delivery report.
- [ ] An adversarial audit cannot make the system damage a photograph.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 24 - Generative Cleanup & Distraction Removal (safe by construction).

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-24-GENERATIVE-CLEANUP.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - Distracting objects, background clutter, stray limbs, signage, bins, cables and photobombing strangers are removed automatically - but only where removal is safe, small and defensible.

Rules:
  - Do not start Phase 25. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-generative/src/{lib,detect,safety,denylist,borrow,fill,inpaint,selfcheck,queue}.rs`, `ml/models/generative/{train_distraction.py,train_artefact.py,eval_cleanup.py}`, `config/cleanup_policy.toml`, `apps/desktop/src/routes/cleanup/{ProposalQueue,BeforeAfter,ManualRemove}.tsx`, `docs/generative-policy.md`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-24-generative-cleanup and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-24.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-24-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-24-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-24-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 24 of 30 - Generative Cleanup & Distraction Removal (safe by construction) - part of the AURA Wedding AI master build plan.*

---

# Phase 25 - Gallery Intelligence Engine: Cross-Photo Colour, Skin & Scene Consistency

> **Single feature shipped by this phase:** The app edits the gallery, not the photo: reference frames anchor each scene, and every other frame is normalised toward them so an entire wedding looks like one coherent body of work.
>
> **Mission:** Deliver the capability no competitor has - gallery-level reasoning. This is the difference between 1,000 individually plausible edits and one professionally consistent wedding.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 25 of 30 |
| Epic | E5 - Gallery Brain & Autonomy |
| Feature | The app edits the gallery, not the photo: reference frames anchor each scene, and every other frame is normalised toward them so an entire wedding looks like one coherent body of work. |
| Depends on | Phases 07, 15, 16, 18, 20 |
| Unlocks | Phases 26, 27, 28, 29 |
| Duration | 3 weeks |
| Primary owners | Colour Scientist, ML Lead - Vision, Senior Engineer - Core Pipeline (Rust), Tech Lead - Imaging Core (Rust) |
| Risk level | High - the marquee differentiator |
| Headline KPI | within-scene WB spread reduced >= 60 %; per-identity skin dE00 spread <= 2.0 across the gallery; consistency pass <= 60 s for 1,000 images |
| Competitor being beaten | Nobody ships this; Imagen/Aftershoot edit per image |

## 1. Why this phase exists

Photographers spend a large part of their editing time on *sync and consistency* rather than on individual images. Solving it structurally is the highest-leverage automation in the whole product.

It is also the most visible quality signal to clients: galleries where skin tone and warmth drift from frame to frame look amateur even when every individual frame is fine.

Because it operates on the graph of scenes, identities and cameras built in Phases 06-08, it is genuinely hard for competitors to copy without the same foundation.

## 2. Scope contract

### 2.1 In scope

- Reference frame system: per segment (and per sub-scene node) select 3-5 anchors from Phase 15's candidates using WB confidence, subject exposure quality, primary-identity presence and absence of mixed light.
- Normalisation solver: adjust each frame's WB/exposure/tone toward its scene's anchor statistics, with damping so genuine lighting changes within a scene are preserved (a candle-lit vow inside a bright ceremony must not be flattened).
- Skin consistency: per identity, build a target skin appearance (luminance band + chromaticity) from their best-lit frames and correct deviations across the gallery using identity-scoped masks.
- Scene consistency: harmonise contrast, saturation, black point and grade character within each scene node so sequences read as one look.
- Change-point respect: detect intentional lighting transitions (venue change, sunset, flash on/off) and treat them as new normalisation groups rather than errors.
- Outlier detection: flag frames whose colour/exposure deviates beyond a threshold from their group, with the deviation quantified - this is exactly the Phase 27 QC input.
- Gallery timeline visualisation: strips showing WB, exposure and skin tone across the wedding so drift is visible at a glance, before and after.
- Solver stability: normalisation is idempotent (running twice changes nothing) and bounded (no frame moves more than a documented maximum).

### 2.2 Explicitly out of scope (do not build it here)

- Cross-camera profile matching (Phase 26 - a distinct problem).
- QC remediation and re-editing (Phase 27).
- Curation (Phase 29).

## 3. Architecture and data flow

```text
segments (P07) -> scene nodes (tree: Ceremony > Entrance/Ritual/Couple/Reactions)
     |
  anchor selection per node (3-5 frames: high WB conf, good subject exposure, primary identity)
     |
  node statistics: target CCT/tint band, subject luma band, contrast/saturation character
     |
  per-frame delta solve (damped, bounded, change-point aware)
     |
  identity skin targets --> identity-scoped skin corrections (P18 masks)
     |
  outlier detection (deviation > threshold) --> QC queue (P27)
     |
  recipe deltas + gallery timeline visualisation (before/after strips)
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-brain-gallery/src/{lib,tree,anchors,stats,normalise,skin_consistency,scene_consistency,changepoint,outlier}.rs` | Gallery brain. |
| `crates/aura-catalog/migrations/0025_gallery.sql` | `scene_nodes`, `anchors`, `normalisation`, `outliers` tables. |
| `config/consistency.toml` | Damping factors, bounds, thresholds, per-scene policy. |
| `apps/desktop/src/routes/gallery/{ConsistencyView,TimelineStrips,AnchorPicker,OutlierList}.tsx` | Gallery consistency UI. |
| `ml/eval/consistency_eval.py` | Spread-reduction and skin-drift metrics. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Gallery consistency contracts (frozen)**

```rust
pub struct SceneNode {
    pub id: NodeId, pub parent: Option<NodeId>,
    pub segment_id: SegmentId, pub label: String,
    pub image_ids: Vec<ImageId>,
    pub anchors: Vec<ImageId>,
    pub target: NodeTarget,
}

pub struct NodeTarget {
    pub cct_k: f32, pub cct_tol: f32,
    pub tint: f32, pub tint_tol: f32,
    pub subject_luma: f32, pub luma_tol: f32,
    pub contrast: f32, pub saturation: f32,
    pub grade_signature: [f32; 8],     // compact colour-character descriptor
}

pub struct NormalisationDelta {
    pub image_id: ImageId, pub node_id: NodeId,
    pub d_exposure: f32, pub d_cct: f32, pub d_tint: f32,
    pub d_contrast: f32, pub d_saturation: f32,
    pub skin_correction: Option<SkinCorrection>,
    pub damping: f32, pub bounded_by: Option<Bound>,
    pub reasons: Vec<Reason>, pub confidence: f32,
}
```

## 6. Algorithm, model and implementation design

### 6.1 Anchors, not averages

- Averaging a scene's frames produces mediocrity; anchoring to the *best-judged* frames preserves quality. Anchors are the frames the system is most confident about, weighted by primary-identity presence.
- Anchor statistics are robust (trimmed means, median chromaticity) so one anchor error cannot skew a node.
- Users can pin or reject anchors in the UI; pinned anchors are authoritative, which gives professionals direct control over the look of a scene.

### 6.2 Damped, bounded, change-point-aware normalisation

- Each frame's delta = damping * (target - current), where damping (default 0.6-0.8) preserves natural variation; bounds cap movement (e.g. |d_cct| <= 450 K, |d_exposure| <= 0.35 EV).
- Change-point detection over the frame sequence splits a node when illumination genuinely changes (flash toggled, sunset, moved indoors); each side normalises to its own anchors.
- Idempotence: after normalisation, recomputing deltas yields near-zero movement - proved by a test, because a non-idempotent solver would drift the gallery on every run.

### 6.3 Per-identity skin consistency

- For each identity, collect skin chromaticity and luminance from frames with high mask confidence and good exposure; take the robust central tendency as that person's target appearance.
- Correct deviations inside identity-scoped skin masks only, capped so lighting mood is preserved (a candle-lit face may stay warm, but not magenta).
- Guarantee: the same person's skin dE00 spread across the gallery <= 2.0 after correction - a measurable claim no competitor makes.

### 6.4 Outliers as a first-class output

- Frames deviating beyond threshold after normalisation are recorded with the specific deviation ('+310 K warmer than node anchors, magenta skin cast 4.2 dE00'), which becomes a QC ticket in Phase 27.
- The timeline strips give photographers a before/after picture of gallery drift, which is a demo-friendly visual and a genuine diagnostic tool.

## 7. Cloud AI usage (bring-your-own API key)

No cloud AI call in this phase. The phase must work with the network cable unplugged; the Cloud AI Gateway from Phase 04 stays idle here.

## 8. Implementation order (execute literally, in this order)

1. Build the scene-node tree from Phase 07 segments plus sub-clustering inside long segments.
2. Implement anchor selection with robust statistics and user pinning.
3. Implement the damped bounded solver with change-point splitting.
4. Prove and test idempotence and bounds.
5. Implement per-identity skin targets and identity-scoped corrections.
6. Implement scene-level contrast/saturation/grade harmonisation.
7. Implement outlier detection with quantified deviations.
8. Build the consistency UI with timeline strips, anchor picker and outlier list.
9. Validate on multi-camera, multi-lighting fixtures; measure spread reduction and skin drift.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `COL` | Colour Scientist | Own colour statistics, grade signature, skin targets and validation methodology | Validated solver | 9 d |
| `MLL` | ML Lead - Vision | Own damping/bounds policy, change-point detection, outlier thresholds | Signed spec | 4 d |
| `TLC` | Tech Lead - Imaging Core (Rust) | Design the scene-node tree, solver architecture and idempotence guarantees | Architecture + review | 3 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Implement tree, anchors, solver, skin consistency, outliers, persistence | `aura-brain-gallery` | 9 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Consistency view, timeline strips (before/after), anchor picker, outlier list | Gallery UI | 6 d |
| `MFE` | Mid-Level Frontend Engineer | Node tree navigation, per-node overrides, batch accept | UI panels | 3 d |
| `QAL` | QA Lead - Automation | Spread-reduction gate, skin-drift gate, idempotence test, bounds test | CI gates | 5 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Full-gallery review of 5 weddings before/after; hunt for flattened mood | Audit report | 4 d |
| `PM` | Product Manager Agent | Approve `consistency.toml` damping and bounds; define the 'preserve mood' rule | Approved config | 2 d |
| `PERF` | Performance Engineer | Keep the consistency pass under 60 s for 1,000 images; incremental re-solve | Benchmark | 3 d |
| `DATA` | Data Engineer / Dataset Curator | Label intentional lighting transitions on fixture weddings for change-point validation | Labels | 3 d |
| `DOC` | Technical Writer | Explain gallery consistency, anchors and how to pin them | Docs merged | 2 d |

### 9.1 Handoff chain for this phase

```text
TLC architecture -> SRC tree/anchors -> COL statistics + skin targets
                                |
                                v
                    MLL damping/change-points -> SRC solver -> SFE/MFE UI
                                |
              QAL gates + QAIQ full-gallery audit -> COL/PM gate -> Phases 26/27
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

- Within-scene WB spread reduced >= 60 % and exposure spread >= 50 % on fixture weddings.
- Per-identity skin dE00 spread <= 2.0 across the gallery.
- Idempotence: a second normalisation run moves every frame by less than a small epsilon.
- Bounds respected: no frame exceeds documented maximum movement.
- Intentional transitions (flash on/off, sunset, venue change) are not flattened - labelled fixture gate.
- Pinned anchors override automatic selection and persist through re-analysis.
- Outliers reported with quantified deviations and consumed correctly by the Phase 27 stub.

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
| Consistency pass for 1,000 images | <= 60 s |
| Incremental re-solve after one anchor change | <= 6 s |
| Timeline strips render | <= 400 ms |
| Extra storage per image | <= 500 B |

Telemetry events (local-first, opt-in aggregation):

- `gallery.normalised` {nodes, images, mean_d_cct, mean_d_ev, ms}
- `gallery.skin_corrected` {identities, mean_de00_before, mean_de00_after}
- `gallery.outliers` {count, mean_deviation}
- `gallery.anchor_pinned` {node, images}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Flattening intentional variation | Damping below 1.0, hard bounds, change-point splitting, labelled transition fixtures, and PM-owned preserve-mood policy. |
| Bad anchors propagate errors | Robust statistics, anchor confidence gating, user pinning, and outlier feedback loop. |
| Solver drift across runs | Idempotence test in CI and bounded movement. |
| Skin correction looks unnatural in coloured light | Caps that preserve mood, per-identity targets from that person's own frames, and audit gate. |

## 13. Acceptance criteria

- [ ] A whole wedding reads as one coherent gallery, verified by before/after timeline strips.
- [ ] Each scene is anchored to its best frames, and the user can pin anchors.
- [ ] The same person's skin looks consistent from morning preparation to late reception.
- [ ] Intentional lighting changes survive normalisation.
- [ ] Outliers are listed with quantified deviations, ready for QC.
- [ ] Running consistency twice changes nothing.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 25 - Gallery Intelligence Engine: Cross-Photo Colour, Skin & Scene Consistency.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-25-GALLERY-INTELLIGENCE-ENGINE.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - The app edits the gallery, not the photo: reference frames anchor each scene, and every other frame is normalised toward them so an entire wedding looks like one coherent body of work.

Rules:
  - Do not start Phase 26. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-brain-gallery/src/{lib,tree,anchors,stats,normalise,skin_consistency,scene_consistency,changepoint,outlier}.rs`, `crates/aura-catalog/migrations/0025_gallery.sql`, `config/consistency.toml`, `apps/desktop/src/routes/gallery/{ConsistencyView,TimelineStrips,AnchorPicker,OutlierList}.tsx`, `ml/eval/consistency_eval.py`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-25-gallery-intelligence-engine and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-25.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-25-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-25-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-25-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 25 of 30 - Gallery Intelligence Engine: Cross-Photo Colour, Skin & Scene Consistency - part of the AURA Wedding AI master build plan.*

---

# Phase 26 - Multi-Camera & Second-Shooter Matching

> **Single feature shipped by this phase:** Sony, Canon, Nikon and Fujifilm bodies - and two photographers with different habits - are matched to one visual result, not to identical slider values.
>
> **Mission:** Solve the problem every wedding team has and no tool addresses: making mixed gear and mixed shooters look like one studio, by matching appearance rather than parameters.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 26 of 30 |
| Epic | E5 - Gallery Brain & Autonomy |
| Feature | Sony, Canon, Nikon and Fujifilm bodies - and two photographers with different habits - are matched to one visual result, not to identical slider values. |
| Depends on | Phases 05, 15, 16, 25 |
| Unlocks | Phases 27, 28 |
| Duration | 2 weeks |
| Primary owners | Colour Scientist, ML Lead - Vision, Senior Engineer - Core Pipeline (Rust) |
| Risk level | Medium-High |
| Headline KPI | cross-camera skin dE00 <= 2.0 in matched scenes; cross-shooter grade signature distance reduced >= 65 %; matching pass <= 25 s per wedding |
| Competitor being beaten | No competitor offers appearance-based camera matching |

## 1. Why this phase exists

Most weddings are shot by two or three photographers on different brands. Colour science differs by brand, so identical settings produce different results - which is precisely why manual matching eats hours.

Matching *appearance* is the correct formulation: the goal is that skin, whites and blacks agree, not that the sliders agree. Framing it this way turns an unsolved workflow problem into a measurable optimisation.

It is also a standalone selling point: 'your second shooter's files will finally match yours' is an immediately understood benefit.

## 2. Scope contract

### 2.1 In scope

- Camera fingerprinting: per body/profile colour response measured from the wedding's own frames (skin chromaticity, white-point behaviour, saturation response, contrast character, highlight roll-off).
- Cross-camera pairing: find matched scene pairs (same node, overlapping time, similar subjects) where two cameras photographed the same conditions, and use them as calibration evidence.
- Appearance-matching transform: solve a small per-camera correction (WB offset, exposure offset, per-channel gain, saturation and contrast shaping, skin-specific correction) that minimises appearance distance to the reference camera.
- Reference camera choice: primary shooter's body by default, user-selectable, or the body with the most frames in the gallery.
- Second-shooter style normalisation: correct systematic differences in exposure habits and framing brightness, applied per shooter and per scene, without erasing legitimate stylistic variety.
- Flash/ambient handling: separate transforms for flash-lit and ambient frames from the same body, because their colour behaviour differs materially.
- Fallback path when no matched pairs exist: use bundled per-brand baseline transforms plus skin-target matching only.
- Transforms recorded in the recipe with provenance, reversible, and visible in a per-camera report.

### 2.2 Explicitly out of scope (do not build it here)

- Within-scene normalisation (Phase 25 does that; this phase supplies camera-level corrections it consumes).
- Lens-specific geometry (Phase 23).
- Style learning (Phase 17).

## 3. Architecture and data flow

```text
images grouped by camera_id + flash_state
     |
  matched-pair finder: same scene node, overlapping time, similar subjects
     |
  per-camera fingerprint: skin chromaticity, white point, sat/contrast response, roll-off
     |
  choose reference camera (primary shooter / most frames / user)
     |
  solve per-camera transform: minimise appearance distance on matched pairs
         subject to: bounded movement, skin locus validity, no mood destruction
     |
  apply as camera-level recipe deltas -> then Phase 25 normalises within scenes
     |
  per-camera report + cross-shooter exposure-habit correction
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-brain-gallery/src/camera/{fingerprint,pairs,solve,transform,shooter,report}.rs` | Matching engine. |
| `assets/camera_baselines/<brand>.toml` | Bundled fallback transforms measured in the lab. |
| `crates/aura-catalog/migrations/0026_camera_match.sql` | `camera_fingerprints`, `camera_transforms`, `matched_pairs` tables. |
| `apps/desktop/src/routes/gallery/CameraMatchPanel.tsx` | Reference camera choice, per-camera report, before/after pairs. |
| `ml/eval/camera_match_eval.py` | Cross-camera dE00 and grade-distance metrics. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Camera matching contracts**

```rust
pub struct CameraFingerprint {
    pub camera_id: CameraId, pub flash: FlashState,
    pub skin_chroma: [f32; 2], pub white_point: [f32; 2],
    pub sat_response: [f32; 4], pub contrast_response: [f32; 4],
    pub highlight_rolloff: f32, pub samples: u32, pub confidence: f32,
}

pub struct CameraTransform {
    pub camera_id: CameraId, pub flash: FlashState,
    pub reference: CameraId,
    pub d_cct: f32, pub d_tint: f32, pub d_exposure: f32,
    pub channel_gain: [f32; 3],
    pub d_saturation: f32, pub contrast_shape: [f32; 3],
    pub skin_correction: SkinCorrection,
    pub evidence_pairs: u32, pub source: TransformSource,   // MatchedPairs | BrandBaseline
    pub confidence: f32, pub reasons: Vec<Reason>,
}
```

## 6. Algorithm, model and implementation design

### 6.1 Finding real evidence inside the wedding

- Matched pairs are the gold standard: two cameras shooting the same ceremony minutes apart under the same light. Find them by scene node, time overlap and embedding similarity, then verify by comparing background statistics rather than subjects.
- Require a minimum number of pairs (default 12) before trusting a solved transform; below that, blend with the brand baseline proportionally to evidence.
- Separate flash and ambient populations, since brand differences are amplified under flash.

### 6.2 Solving for appearance, not parameters

- Objective: minimise a weighted appearance distance = 3*skin_dE00 + 1.5*white_point_distance + 1.0*grade_signature_distance + 0.5*contrast_distance on matched pairs.
- Solve with bounded least squares over the small transform vector; bounds prevent the solver from making a Canon file look broken to satisfy a metric.
- Skin locus validity is a hard constraint (reused from Phase 15), so matching never pushes skin somewhere implausible.
- Verify on held-out pairs: if the transform does not improve appearance distance on held-out evidence, fall back to the brand baseline and say so.

### 6.3 Second-shooter habits

- Estimate systematic per-shooter exposure bias (median subject luminance offset per scene class) and correct it as part of the camera transform, since gear and habit are entangled in practice.
- Cap the correction so a deliberately moodier second shooter is harmonised, not erased; the report tells the photographer what was corrected.
- Per-chapter profile assignment from Phase 17 can additionally be used when a second shooter has their own learned profile.

### 6.4 Order of operations

- Camera transforms are applied *before* Phase 25's within-scene normalisation, so the gallery brain works on already-comparable frames; this ordering is enforced in the pipeline and tested.
- Everything is stored as recipe deltas with provenance, so a photographer can inspect and disable matching per camera.

## 7. Cloud AI usage (bring-your-own API key)

No cloud AI call in this phase. The phase must work with the network cable unplugged; the Cloud AI Gateway from Phase 04 stays idle here.

## 8. Implementation order (execute literally, in this order)

1. Measure bundled brand baselines in controlled conditions (COL) for the top brands and profiles.
2. Implement camera fingerprinting from wedding frames.
3. Implement matched-pair discovery with background-based verification.
4. Implement the bounded appearance-matching solver with skin-locus constraints.
5. Implement evidence blending with brand baselines and held-out verification.
6. Implement per-shooter exposure-habit correction with caps.
7. Enforce and test ordering against Phase 25.
8. Build the camera match panel with reference selection and before/after pairs.
9. Validate on real two- and three-camera weddings across brands.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `COL` | Colour Scientist | Measure brand baselines; own the appearance distance metric and validation | Baselines + metric | 8 d |
| `MLL` | ML Lead - Vision | Own solver formulation, bounds, evidence thresholds and held-out verification | Signed spec | 4 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Fingerprinting, pair discovery, solver, shooter correction, ordering, persistence | `camera` module + tests | 8 d |
| `DATA` | Data Engineer / Dataset Curator | Collect multi-camera wedding fixtures (Sony+Canon, Canon+Nikon, +Fuji) with matched scenes | Fixtures v1 | 6 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Camera match panel, reference chooser, per-camera report, matched-pair viewer | UI shipped | 4 d |
| `QAL` | QA Lead - Automation | Cross-camera dE00 gate, held-out verification test, ordering test, fallback test | CI gates | 4 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Blind review: can a photographer tell which camera shot which frame after matching? | Blind study | 3 d |
| `PM` | Product Manager Agent | Decide default reference-camera policy and how much shooter style may be normalised | Policy | 1 d |
| `PERF` | Performance Engineer | Keep matching under 25 s per wedding; cache fingerprints | Benchmark | 2 d |
| `DOC` | Technical Writer | Document camera matching, when it needs matched pairs, and how to disable it | Docs merged | 2 d |

### 9.1 Handoff chain for this phase

```text
COL baselines + metric -> SRC fingerprints/pairs -> MLL solver spec
                                    |
                                    v
                        SRC transforms (ordered before P25) -> SFE panel
                                    |
                   QAL gates + QAIQ blind study -> COL/PM gate
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

- Cross-camera skin dE00 <= 2.0 in matched scenes after transform; white points converge.
- Grade-signature distance between cameras reduced >= 65 %.
- Held-out verification: transforms improve appearance distance on unseen pairs, or the baseline is used.
- Flash and ambient populations receive distinct transforms.
- Ordering enforced: camera transforms always precede within-scene normalisation.
- With no matched pairs, brand baselines are used and the report says so honestly.
- Blind study: photographers cannot reliably identify the second camera after matching.

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
| Fingerprinting + pair discovery | <= 18 s per wedding |
| Solve per camera | <= 1 s |
| Total matching pass | <= 25 s |

Telemetry events (local-first, opt-in aggregation):

- `camera.fingerprinted` {cameras, flash_states, samples}
- `camera.matched` {pairs, reference, mean_de00_before, mean_de00_after, source}
- `camera.baseline_fallback` {camera, reason}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Not enough matched pairs | Evidence thresholds with proportional blending to brand baselines and explicit reporting. |
| Erasing a second shooter's legitimate style | Capped corrections, PM policy, per-chapter profile option, and a transparent report. |
| Solver overfits a few pairs | Bounded solve, held-out verification, and fallback on failure. |
| Unknown camera profiles | Fingerprint from the wedding's own data plus telemetry to prioritise new baselines. |

## 13. Acceptance criteria

- [ ] Frames from different brands in the same scene look like they came from one camera.
- [ ] Flash and ambient frames are matched separately and correctly.
- [ ] The per-camera report explains what was corrected and on what evidence.
- [ ] Matching runs before gallery normalisation and can be disabled per camera.
- [ ] With no matched evidence, the system falls back gracefully and says so.
- [ ] Photographers cannot pick out the second camera in a blind review.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 26 - Multi-Camera & Second-Shooter Matching.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-26-MULTI-CAMERA-SHOOTER-MATCHING.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - Sony, Canon, Nikon and Fujifilm bodies - and two photographers with different habits - are matched to one visual result, not to identical slider values.

Rules:
  - Do not start Phase 27. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-brain-gallery/src/camera/{fingerprint,pairs,solve,transform,shooter,report}.rs`, `assets/camera_baselines/<brand>.toml`, `crates/aura-catalog/migrations/0026_camera_match.sql`, `apps/desktop/src/routes/gallery/CameraMatchPanel.tsx`, `ml/eval/camera_match_eval.py`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-26-multi-camera-shooter-matching and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-26.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-26-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-26-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-26-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 26 of 30 - Multi-Camera & Second-Shooter Matching - part of the AURA Wedding AI master build plan.*

---

# Phase 27 - AI Quality-Control Agent & Automatic Re-Edit Loop

> **Single feature shipped by this phase:** An autonomous inspector re-examines every edited image before export, writes a diagnosis with confidence, fixes what it can, replaces frames when a better alternative exists, and escalates the rest.
>
> **Mission:** Replicate the senior retoucher who checks the junior's work. This closes the loop that makes Zero-Touch delivery defensible rather than reckless.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 27 of 30 |
| Epic | E5 - Gallery Brain & Autonomy |
| Feature | An autonomous inspector re-examines every edited image before export, writes a diagnosis with confidence, fixes what it can, replaces frames when a better alternative exists, and escalates the rest. |
| Depends on | Phases 13, 15-26 |
| Unlocks | Phases 28, 30 |
| Duration | 3 weeks |
| Primary owners | AI Agent & Prompt Engineer, ML Lead - Vision, QA Lead - Automation, Senior Engineer - Core Pipeline (Rust) |
| Risk level | High - it is the last line of defence |
| Headline KPI | catches >= 90 % of injected defects; auto-fix success >= 85 % of accepted tickets; QC pass <= 90 s per 1,000 images |
| Competitor being beaten | Nobody ships an autonomous QC agent |

## 1. Why this phase exists

Automation without inspection is gambling with a client's wedding. A QC agent converts a pipeline of independent decisions into a system with feedback, which is what makes 'click once and deliver' a responsible promise.

It also produces the artefact photographers want most: a short, honest report of what was checked, what was fixed and what needs their eyes - turning fear of automation into an auditable workflow.

## 2. Scope contract

### 2.1 In scope

- Inspection battery over every selected, edited frame: colour consistency vs node anchors (P25), skin plausibility (P15/25), exposure/clipping regressions (P09/14), sharpness after restoration (P22), retouch artefacts and texture loss (P20/21), mask edge artefacts (P18/19), crop safety (P23), cleanup artefacts (P24), duplicate leakage (P08), coverage integrity (P12).
- Ticket model: each finding is a ticket with image, category, diagnosis text, quantified deviation, confidence, proposed remedy and expected improvement.
- Remedy engine: parameter re-solve (re-run the specific decision with constraints), strength reduction, operation revert, frame replacement from the Phase 12 runner-up, or escalation to human review.
- Bounded re-edit loop: at most 2 remediation rounds per image; each round must measurably improve the ticket's metric or the change is reverted (no thrashing).
- Replacement logic: swap a selected frame for its runner-up only when the runner-up's post-edit metrics are clearly better and coverage remains intact; always recorded with a before/after.
- Agentic reasoning (Phase 04) for triage and remediation planning on complex or multi-symptom images, with tool-calling over read-only inspection APIs and a bounded step count.
- QC report: per-wedding summary (checks run, tickets by category, auto-fixed, replaced, escalated) exportable as PDF/Markdown for studio records.
- Escalation queue in the UI with keyboard-fast review, grouped by category so a photographer can clear 40 tickets in minutes.

### 2.2 Explicitly out of scope (do not build it here)

- Making the original edits (Phases 15-26).
- The Zero-Touch orchestration itself (Phase 28 calls QC as a stage).
- Learning from resolutions (Phase 30 consumes QC outcomes).

## 3. Architecture and data flow

```text
edited gallery
     |
  INSPECTION BATTERY (10 checks, all read-only, all quantified)
     |
  tickets[] { image, category, diagnosis, deviation, confidence, remedy, expected_gain }
     |
  triage: mechanical rules  --(complex/multi-symptom)-->  agentic planner (P04, bounded)
     |
  REMEDY: re-solve param | reduce strength | revert op | replace with runner-up | escalate
     |
  re-inspect (round <= 2): improved? keep : revert
     |
  QC report + escalation queue + ledger entries (P13)
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-qc/src/{lib,checks/*,ticket,triage,remedy,replace,loop,report,queue}.rs` | QC agent. |
| `crates/aura-qc/src/checks/{consistency,skin,exposure,sharpness,retouch,mask,crop,cleanup,duplicate,coverage}.rs` | Individual inspections. |
| `config/qc_thresholds.toml` | Per-check thresholds and remedy policy. |
| `apps/desktop/src/routes/qc/{QcReport,TicketQueue,BeforeAfter,CategoryFilter}.tsx` | QC UI. |
| `tests/qc/injected_defects/` | Synthetic defect corpus for gate testing. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**QC ticket and remedy contracts (frozen)**

```rust
pub struct QcTicket {
    pub id: TicketId, pub image_id: ImageId,
    pub category: QcCategory,          // Consistency | Skin | Exposure | Sharpness | Retouch | Mask | Crop | Cleanup | Duplicate | Coverage
    pub diagnosis: String,             // "bride face 4.2 dE00 magenta vs node anchors #817/#819/#825"
    pub deviation: f32, pub threshold: f32,
    pub evidence: Evidence,
    pub remedy: Remedy,
    pub expected_gain: f32,
    pub confidence: f32, pub autonomy: Autonomy,
    pub round: u8, pub status: TicketStatus,   // Open | Fixed | Reverted | Escalated | Accepted
}

pub enum Remedy {
    ResolveParam { decision: DecisionKind, constraint: String },
    ReduceStrength { op: String, factor: f32 },
    RevertOp { op: String },
    ReplaceFrame { with: ImageId },
    Escalate { note: String },
}

pub struct QcReport {
    pub project: ProjectId,
    pub checks_run: u32, pub images: u32,
    pub by_category: Vec<(QcCategory, u32, u32, u32)>,   // found, fixed, escalated
    pub replacements: Vec<(ImageId, ImageId, String)>,
    pub duration_ms: u64, pub cloud_used: bool,
}
```

## 6. Algorithm, model and implementation design

### 6.1 Inspections must be quantified, not vibes

- Every check outputs a number and a threshold: 'skin 4.2 dE00 vs node anchors, threshold 2.5'. This makes tickets actionable, testable and explainable.
- Checks are read-only and independent so they can run in parallel across the gallery on all cores.
- Thresholds live in `qc_thresholds.toml` per scene class, because a dance floor tolerates more than a family formal.

### 6.2 Triage: mechanical first, agentic only when needed

- Single-symptom tickets with an obvious remedy are handled by deterministic rules - cheap, fast, reproducible.
- Multi-symptom or contradictory cases (soft *and* noisy *and* inconsistent) go to the bounded agentic planner, which may call read-only inspection tools, then must return a plan matching a strict schema.
- The planner never executes anything; it proposes remedies which the mechanical engine validates against policy before applying.

### 6.3 The re-edit loop, without thrashing

- Each remedy application is followed by re-inspection of that ticket's metric only; if the metric does not improve by at least the expected gain margin, the change is reverted and the ticket escalates.
- Maximum 2 rounds per image, global time budget per wedding, and a rule that no remedy may worsen another check by more than a small tolerance (checked by re-running affected checks).
- All rounds are recorded in the ledger so the history of an image's edit is fully reconstructable.

### 6.4 Replacement, the feature photographers will demo

- 'Image #382's face is below the sharpness threshold; #381 in the same moment has eyes open, higher sharpness and a better expression' - replacement uses the Phase 12 runner-up plus post-edit metrics.
- Coverage is re-validated after any replacement so a swap can never break a must-have rule.
- Replacements always require higher confidence than parameter fixes, and are shown side by side in the report.

## 7. Cloud AI usage (bring-your-own API key)

**Diagnose and plan remediation for complex, multi-symptom images**

| Aspect | Specification |
|---|---|
| Model class | Reasoning tier with vision, temperature 0, bounded to 6 tool steps |
| Trigger | Images with >= 3 open tickets, or contradictory tickets, or a failed first remediation round |
| Input sent | Ticket list with quantified deviations, the recipe summary, node anchor statistics, and up to 3 crops (subject, background, comparison anchor) |
| Cost control | <= 40 calls per wedding; batched by image; cached; skipped when cloud is off |
| Offline fallback | Mechanical priority ordering (consistency -> exposure -> skin -> retouch -> sharpness) with single-remedy-per-round and escalation on failure |

System prompt contract:

```text
You are a senior retoucher reviewing an automatically edited wedding photograph that failed several quality checks.
Input: quantified findings, the current edit recipe summary, reference-frame statistics for this scene, and image crops.
Task: produce an ordered remediation plan using ONLY the allowed remedies, or recommend escalation to a human.
Rules:
- Fix root causes before symptoms: if white balance is wrong, do not reduce retouch strength.
- Never propose a remedy that is not in the allowed list. Never invent parameter values outside the stated bounds.
- Prefer the smallest change that resolves the finding. Prefer escalation over a risky fix on a must-have moment.
- Explain each step in one short sentence referencing the specific finding.
- Return ONLY JSON matching the schema.
```

Required JSON response schema (validated; invalid = retry once, then fall back to local model):

```json
{
  "type": "object",
  "required": ["plan", "confidence"],
  "properties": {
    "plan": {
      "type": "array", "maxItems": 4,
      "items": {
        "type": "object",
        "required": ["remedy", "target", "reason"],
        "properties": {
          "remedy": { "type": "string", "enum": ["resolve_param", "reduce_strength", "revert_op", "replace_frame", "escalate"] },
          "target": { "type": "string" },
          "magnitude": { "type": ["number", "null"] },
          "reason": { "type": "string" }
        },
        "additionalProperties": false
      }
    },
    "root_cause": { "type": ["string", "null"] },
    "confidence": { "type": "number", "minimum": 0, "maximum": 1 }
  },
  "additionalProperties": false
}
```

## 8. Implementation order (execute literally, in this order)

1. Build the injected-defect corpus first - QC is tested by its ability to catch known defects.
2. Implement the ten inspections with quantified outputs and parallel execution.
3. Implement the ticket model, thresholds config and ledger integration.
4. Implement mechanical triage and the remedy engine with policy validation.
5. Implement the bounded re-edit loop with improvement verification and revert.
6. Implement replacement with coverage re-validation.
7. Implement the agentic planner with the tool registry and strict schema.
8. Implement the QC report and the escalation queue UI.
9. Run the defect-detection gate and the no-regression gate; tune thresholds.
10. Dogfood on 10 real weddings and measure how many tickets a photographer agrees with.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `QAL` | QA Lead - Automation | Own the injected-defect corpus, detection gates and no-regression methodology | Corpus + gates | 6 d |
| `MLL` | ML Lead - Vision | Own check formulations, thresholds, expected-gain margins and confidence calibration | Signed spec | 5 d |
| `AGT` | AI Agent & Prompt Engineer | Agentic planner: tool registry, bounded loop, schema, policy validation, cassettes | Planner shipped | 7 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Inspections, ticket model, remedy engine, re-edit loop, replacement, report | `aura-qc` + tests | 11 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Parallel execution and time budgets across the gallery | Scheduler | 2 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | QC report view, ticket queue with keyboard flow, before/after, category filters | QC UI | 6 d |
| `MFE` | Mid-Level Frontend Engineer | Escalation review flow, bulk accept/reject, replacement comparison | UI panels | 4 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Agreement study: do photographers agree with QC tickets and fixes on 10 weddings? | Study report | 6 d |
| `PM` | Product Manager Agent | Own `qc_thresholds.toml`, autonomy for each remedy type, and report contents | Approved policy | 3 d |
| `PERF` | Performance Engineer | Hit the 90 s per 1,000 images budget; parallelism and check cost tuning | Benchmark | 3 d |
| `EM` | Engineering Manager / Delivery Lead Agent | Coordinate the many upstream dependencies; run the integration bug bash | Integration log | 3 d |
| `DOC` | Technical Writer | Document every check, threshold and remedy; publish a sample QC report | Docs merged | 3 d |

### 9.1 Handoff chain for this phase

```text
QAL defect corpus -> MLL check specs -> SRC inspections + remedy engine
                                        |
                                        v
                             AGT agentic planner (proposals only)
                                        |
                        SFE/MFE QC UI -> QAIQ agreement study -> PM/CTO gate -> Phase 28
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

- Detection: >= 90 % of injected defects caught, with a documented false-ticket rate <= 8 %.
- Auto-fix: >= 85 % of accepted tickets resolved within 2 rounds and verified by re-inspection.
- No regression: a remedy never worsens another check beyond tolerance (checked automatically).
- Replacement never breaks coverage; every replacement is recorded with a comparison.
- Loop bounds respected: no image exceeds 2 rounds; no thrashing observed on adversarial fixtures.
- Planner output always schema-valid and always policy-validated before execution; cloud-off path fully functional.
- Photographer agreement with tickets >= 80 % in the dogfood study.

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
| QC pass for 1,000 images | <= 90 s |
| Single remediation round per image | <= 1.2 s |
| Report generation | <= 3 s |
| Cloud calls per wedding (default) | <= 40 |

Telemetry events (local-first, opt-in aggregation):

- `qc.run` {images, checks, tickets, fixed, replaced, escalated, ms, cloud_used}
- `qc.ticket` {category, deviation, confidence, remedy, outcome}
- `qc.revert` {category, reason}
- `qc.user_disagree` {category, ticket_id}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| QC introduces new problems while fixing old ones | Improvement verification with automatic revert, no-regression checks, and bounded rounds. |
| Too many false tickets exhaust the user | Calibrated thresholds per scene, false-ticket rate gate, category grouping, and bulk actions. |
| Agentic planner proposes unsafe remedies | Strict allowed-remedy enumeration, policy validation before execution, and planner-cannot-execute architecture. |
| QC becomes a performance bottleneck | Read-only parallel checks, per-wedding time budget, and PERF-owned gate. |

## 13. Acceptance criteria

- [ ] Every edited frame is inspected by ten quantified checks before export.
- [ ] Tickets read like a senior retoucher's notes, with numbers and reference frames.
- [ ] Most problems are fixed automatically and verified; the rest are escalated clearly.
- [ ] Frames are replaced by better alternatives when justified, without breaking coverage.
- [ ] A QC report can be exported for studio records.
- [ ] Photographers agree with the majority of tickets in dogfood testing.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 27 - AI Quality-Control Agent & Automatic Re-Edit Loop.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-27-AI-QC-AGENT.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - An autonomous inspector re-examines every edited image before export, writes a diagnosis with confidence, fixes what it can, replaces frames when a better alternative exists, and escalates the rest.

Rules:
  - Do not start Phase 28. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-qc/src/{lib,checks/*,ticket,triage,remedy,replace,loop,report,queue}.rs`, `crates/aura-qc/src/checks/{consistency,skin,exposure,sharpness,retouch,mask,crop,cleanup,duplicate,coverage}.rs`, `config/qc_thresholds.toml`, `apps/desktop/src/routes/qc/{QcReport,TicketQueue,BeforeAfter,CategoryFilter}.tsx`, `tests/qc/injected_defects/`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-27-ai-qc-agent and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-27.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-27-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-27-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-27-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 27 of 30 - AI Quality-Control Agent & Automatic Re-Edit Loop - part of the AURA Wedding AI master build plan.*

---

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

---

# Phase 29 - Curation Intelligence: B&W Selection, Hero Photos, Album Story & Social Picks

> **Single feature shipped by this phase:** After the gallery is finished, the app curates it: which frames sing in black and white, which are portfolio heroes, how the album should be sequenced, and which images to post.
>
> **Mission:** Turn a finished gallery into deliverables that make the photographer money - album drafts, portfolio picks and social sets - using the story graph the product already understands.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 29 of 30 |
| Epic | E6 - Curation & Delivery |
| Feature | After the gallery is finished, the app curates it: which frames sing in black and white, which are portfolio heroes, how the album should be sequenced, and which images to post. |
| Depends on | Phases 07-12, 25, 27 |
| Unlocks | Phase 30 |
| Duration | 2.5 weeks |
| Primary owners | ML Lead - Vision, AI Agent & Prompt Engineer, Product Manager Agent, Senior Engineer - Core Pipeline (Rust) |
| Risk level | Medium |
| Headline KPI | hero-pick agreement with photographers >= 0.75 top-20; album sequence accepted with <= 15 % reordering; B&W picks accepted >= 70 % |
| Competitor being beaten | Aftershoot/Imagen have none of this; album software has no wedding understanding |

## 1. Why this phase exists

Culling and editing save time; curation makes money. Album sales, portfolio updates and social posting are revenue activities that photographers routinely postpone for months.

Because the product already has scenes, moments, emotions, people and quality scores, curation is nearly free capability - the highest return per engineering hour in the roadmap.

## 2. Scope contract

### 2.1 In scope

- B&W suitability model: identifies frames that gain from monochrome (strong tonal separation, gesture-led, distracting colour, high emotion, grain-tolerant) and generates a tailored B&W mix per frame rather than a single preset.
- Hero photo selection: portfolio-grade picks balancing technical excellence, emotional peak, composition, uniqueness and story importance, with per-chapter diversity.
- Album Story AI: propose a sequenced album (default 60-120 images) that follows the wedding narrative, alternates wide/medium/tight rhythm, pairs facing-page images by tone and subject, guarantees coverage of must-haves and key people, and respects spread capacity.
- Spread pairing: for each spread, choose images that work together (complementary tone, matching direction of gaze/movement, no clashing colour) - the part album designers spend the most time on.
- Social selection: Instagram-ready sets (grid of 10, story set, single hero) with aspect variants from Phase 23 and caption suggestions grounded in the actual story graph.
- Client-preview set: a small teaser set (15-30 images) chosen for immediate delivery on the wedding night.
- Curation UI: drag-to-reorder album, spread view, accept/replace suggestions, export to album software formats (JSON/CSV/PSD-ready layer lists) and to social scheduling.
- Everything explained: why this image is a hero, why this spread pairs, why this frame suits B&W.

### 2.2 Explicitly out of scope (do not build it here)

- Album page layout rendering and printing (export a spec, not a printed book).
- Direct posting to social platforms (Phase 30 handles integrations).
- Client selection workflows (post-V1).

## 3. Architecture and data flow

```text
finished gallery + story graph + emotion + quality + people + consistency
     |
     +--> BwSuitability -> candidates + per-frame B&W mix
     +--> HeroSelector -> portfolio picks (diverse across chapters)
     +--> AlbumComposer -> ordered sequence -> spread pairing -> coverage guarantee
     +--> SocialSelector -> grid set / story set / hero + aspect variants + captions
     +--> TeaserSelector -> 15-30 image preview set
                       |
            explanations for every pick + export specs (JSON/CSV/PSD list)
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-curate/src/{lib,bw,hero,album,spread,social,teaser,explain,export}.rs` | Curation engine. |
| `ml/models/curate/{train_bw.py,train_hero.py,eval_curate.py}` | Learned suitability and hero ranking. |
| `config/curation.toml` | Album sizes, rhythm rules, social formats, teaser policy. |
| `apps/desktop/src/routes/curate/{AlbumBuilder,SpreadView,HeroGrid,SocialSets,BwPicks}.tsx` | Curation UI. |
| `docs/curation.md` | How curation decisions are made. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Curation outputs**

```rust
pub struct CurationResult {
    pub bw: Vec<(ImageId, BwMix, f32)>,          // suitability score
    pub heroes: Vec<(ImageId, f32, Vec<Reason>)>,
    pub album: AlbumPlan,
    pub social: SocialSets,
    pub teaser: Vec<ImageId>,
}

pub struct AlbumPlan {
    pub spreads: Vec<Spread>,                    // { left: Option<ImageId>, right: Option<ImageId>, single: bool }
    pub chapter_map: Vec<(ChapterId, Range<usize>)>,
    pub coverage: CoverageReport,
    pub rhythm_score: f32, pub pairing_score: f32,
    pub reasons: Vec<Reason>,
}

pub struct SocialSets {
    pub grid: Vec<(ImageId, AspectVariant)>,     // 10 images
    pub story: Vec<(ImageId, AspectVariant)>,
    pub hero: (ImageId, AspectVariant),
    pub captions: Vec<(ImageId, String)>,        // grounded in story graph
}
```

## 6. Algorithm, model and implementation design

### 6.1 B&W suitability

- Score on tonal separation (histogram spread after desaturation), colour distraction (saturated non-subject regions), gesture strength (interaction detected), emotional intensity and noise character (grain reads well in mono).
- Generate a per-frame channel mix that maximises subject separation rather than applying one preset - a red-heavy mix for warm skin against green foliage, a blue-heavy mix for pale sky backgrounds.
- Present as suggestions, never applied automatically to the main gallery (B&W is a taste decision), except in a dedicated B&W set.

### 6.2 Hero selection with diversity

- Rank by a weighted blend of technical, emotion, composition, uniqueness (embedding distance from other picks) and story importance.
- Enforce diversity: at most N heroes per chapter, at most one per moment, and a spread across framing types, so the portfolio set is not eight versions of the kiss.
- Uniqueness uses the Phase 05 index, which is why heroes feel like a curated set rather than a top-scoring list.

### 6.3 Album composition as constrained sequencing

- Start from chapter order, allocate spread counts proportionally to chapter importance and duration, then fill with the highest-value images subject to coverage rules.
- Rhythm: alternate wide establishing, medium action and tight emotional frames using a target pattern per chapter; measure the rhythm score and improve by local swaps.
- Spread pairing objective: similar tonal weight, compatible colour temperature after consistency, complementary gaze/movement direction (subjects looking inward, not off the spread), and no two near-identical frames facing each other.
- Guarantee coverage: must-have moments and close-family members appear in the album, not just in the gallery - reusing the Phase 12 coverage engine.

### 6.4 Social and teaser sets

- Grid set balances one hero, two portraits, two details, two candids, two family/group and one exit-style frame, chosen for thumbnail legibility (strong subject, clear silhouette at small size).
- Captions are generated from the story graph (chapter, ritual, people roles anonymised) and are grounded - the model may not invent details about the couple.
- Teaser set is optimised for immediate emotional impact and fast delivery: hero, couple, ceremony peak, one family, one detail, one dance.

## 7. Cloud AI usage (bring-your-own API key)

**Album sequencing refinement and caption drafting**

| Aspect | Specification |
|---|---|
| Model class | Reasoning tier with vision, temperature 0 |
| Trigger | Once per album draft (and on user request), plus one batched call for captions |
| Input sent | Contact sheets per chapter (thumbnails, 512 px), chapter labels, spread capacity, rhythm targets, current draft order |
| Cost control | <= 15 calls per wedding; cached |
| Offline fallback | Deterministic rhythm-and-pairing optimiser only (fully functional offline) |

System prompt contract:

```text
You are an album designer sequencing a wedding album.
Input: chapter contact sheets, the current draft order, spread capacity and rhythm targets.
Task: propose swaps or moves that improve narrative flow and spread pairing, and draft one short caption per chapter.
Rules:
- Preserve chronological chapter order; only reorder within chapters or move an image between adjacent spreads.
- Pair images that share tonal weight and whose subjects face inward.
- Captions must be factual from the supplied chapter/ritual labels. Never invent names, vows, relationships or places.
- Keep captions under 12 words, warm but not sentimental.
- Return ONLY JSON matching the schema.
```

Required JSON response schema (validated; invalid = retry once, then fall back to local model):

```json
{
  "type": "object",
  "required": ["moves", "captions", "confidence"],
  "properties": {
    "moves": {
      "type": "array", "maxItems": 20,
      "items": {
        "type": "object",
        "required": ["from_index", "to_index", "reason"],
        "properties": {
          "from_index": { "type": "integer" },
          "to_index": { "type": "integer" },
          "reason": { "type": "string" }
        },
        "additionalProperties": false
      }
    },
    "captions": {
      "type": "array", "maxItems": 24,
      "items": {
        "type": "object",
        "required": ["chapter", "caption"],
        "properties": { "chapter": { "type": "string" }, "caption": { "type": "string", "maxLength": 90 } },
        "additionalProperties": false
      }
    },
    "confidence": { "type": "number", "minimum": 0, "maximum": 1 }
  },
  "additionalProperties": false
}
```

## 8. Implementation order (execute literally, in this order)

1. Collect photographer-labelled hero picks, album sequences and B&W choices from real deliveries.
2. Train the B&W suitability model and implement per-frame mix generation.
3. Train the hero ranker and implement diversity constraints.
4. Implement album allocation, rhythm optimisation and spread pairing.
5. Reuse the coverage engine to guarantee album coverage.
6. Implement social sets, thumbnail legibility scoring and the teaser set.
7. Add the cloud sequencing/caption task with strict grounding.
8. Build the curation UI: album builder, spread view, hero grid, social sets, B&W picks.
9. Implement export specs for album software and social scheduling.
10. Run agreement studies with photographers on heroes, sequences and B&W picks.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `MLL` | ML Lead - Vision | Own B&W and hero models, diversity constraints and agreement evaluation | Signed spec + gates | 4 d |
| `SRML` | Senior ML Engineer | Train B&W suitability and hero ranker; export and calibrate | Models registered | 6 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Album composer, rhythm/pairing optimiser, social/teaser selectors, export specs | `aura-curate` + tests | 8 d |
| `AGT` | AI Agent & Prompt Engineer | Sequencing/caption cloud task with grounding rules and cassettes | Cloud path live | 3 d |
| `DATA` | Data Engineer / Dataset Curator | Collect 60 real album sequences, hero sets and B&W selections with permission | Curation dataset | 7 d |
| `PM` | Product Manager Agent | Own `curation.toml` (album sizes, rhythm, social formats) and caption tone rules | Approved config | 3 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Album builder with spread view and drag-to-reorder; hero grid; social sets | Curation UI | 7 d |
| `MFE` | Mid-Level Frontend Engineer | B&W picks panel, caption editor, export dialogs, aspect variant switcher | UI panels | 4 d |
| `QAL` | QA Lead - Automation | Agreement gates, coverage-in-album test, pairing property tests, grounding checks | CI gates | 4 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Agreement study with 5 photographers on heroes, album order and B&W picks | Study report | 5 d |
| `PERF` | Performance Engineer | Keep curation under 20 s per gallery; incremental re-composition on edits | Benchmark | 2 d |
| `DOC` | Technical Writer | Document curation logic and album export formats | Docs merged | 2 d |

### 9.1 Handoff chain for this phase

```text
DATA real albums/heroes -> SRML models -> SRC composer/optimiser
                                     |
                                     v
                          AGT sequencing + captions -> SFE/MFE curation UI
                                     |
                    QAL gates + QAIQ agreement study -> MLL/PM gate
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

- Hero agreement >= 0.75 on top-20 overlap with photographer picks.
- Album sequence accepted with <= 15 % of images reordered by photographers in the study.
- B&W picks accepted >= 70 %; generated mixes rated better than a fixed preset.
- Album coverage: every must-have moment and close-family member appears.
- Spread pairing property tests: no facing near-duplicates, no clashing tonal weight beyond threshold.
- Captions contain no invented names, places or claims (automated grounding check).
- Offline: curation works fully without cloud, using the deterministic optimiser.

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
| Full curation for a 1,000-image gallery | <= 20 s |
| Album re-composition after a swap | <= 1.5 s |
| B&W mix generation per image | <= 25 ms |

Telemetry events (local-first, opt-in aggregation):

- `curate.album` {spreads, rhythm_score, pairing_score, ms, cloud_used}
- `curate.heroes` {count, mean_score, chapters_covered}
- `curate.user_reorder` {moves, album_size}
- `curate.bw_accepted` {suggested, accepted}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Curation is taste-heavy and may feel wrong | Agreement studies, per-photographer personalisation via Phase 30's learning loop, and effortless manual reordering. |
| Captions invent facts | Strict grounding rules, automated checks, and human review before posting. |
| Album misses someone important | Coverage engine reuse with an explicit album-coverage test. |
| Scope creep into album layout design | Export a specification for album software rather than building a page designer. |

## 13. Acceptance criteria

- [ ] The app proposes hero photos, a sequenced album with paired spreads, social sets and a teaser set.
- [ ] B&W suggestions come with per-frame mixes, not a single preset.
- [ ] Album coverage of must-haves and close family is guaranteed.
- [ ] Every pick is explained; reordering is instant and remembered.
- [ ] Album and social specs export cleanly to external tools.
- [ ] Photographer agreement studies meet the gates.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 29 - Curation Intelligence: B&W Selection, Hero Photos, Album Story & Social Picks.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-29-CURATION-INTELLIGENCE.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - After the gallery is finished, the app curates it: which frames sing in black and white, which are portfolio heroes, how the album should be sequenced, and which images to post.

Rules:
  - Do not start Phase 30. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-curate/src/{lib,bw,hero,album,spread,social,teaser,explain,export}.rs`, `ml/models/curate/{train_bw.py,train_hero.py,eval_curate.py}`, `config/curation.toml`, `apps/desktop/src/routes/curate/{AlbumBuilder,SpreadView,HeroGrid,SocialSets,BwPicks}.tsx`, `docs/curation.md`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-29-curation-intelligence and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-29.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-29-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-29-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-29-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 29 of 30 - Curation Intelligence: B&W Selection, Hero Photos, Album Story & Social Picks - part of the AURA Wedding AI master build plan.*

---

# Phase 30 - Delivery, Integrations, Learning Loop & Release Engineering

> **Single feature shipped by this phase:** Export and delivery (JPEG/TIFF/XMP, backup, client galleries), Lightroom and Photoshop integration, the learning loop that improves from every correction, and the release machinery that ships it all safely.
>
> **Mission:** Close the product: get finished work out of the app and into the client's hands, learn from every human correction, and make shipping updates a routine, reversible, well-tested event.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 30 of 30 |
| Epic | E6 - Curation & Delivery |
| Feature | Export and delivery (JPEG/TIFF/XMP, backup, client galleries), Lightroom and Photoshop integration, the learning loop that improves from every correction, and the release machinery that ships it all safely. |
| Depends on | Phases 01-29 |
| Unlocks | V1 launch and continuous improvement |
| Duration | 4 weeks |
| Primary owners | DevOps / Release Engineer, Tech Lead - Imaging Core (Rust), MLOps / Model Packaging Engineer, Product Manager Agent, Security & Privacy Engineer |
| Risk level | High - launch quality and data governance |
| Headline KPI | export 1,000 images (45 MP JPEG) <= 12 min on reference GPU; learning loop improves style match by >= 15 % after 3 corrected weddings; crash-free session rate >= 99.5 % |
| Competitor being beaten | Imagen cloud delivery; Aftershoot Lightroom integration; Pic-Time/ShootProof galleries |

## 1. Why this phase exists

A photographer's job ends when the client has the gallery, not when the pixels are finished. Export quality, naming, backup and gallery upload are the last mile that determines whether the product is actually used.

The learning loop is the compounding advantage: every correction a photographer makes should make their next wedding better, which turns usage into a moat that competitors cannot buy.

Release engineering is what keeps a complex AI application trustworthy over time: signed models, staged rollouts, crash reporting and instant rollback.

## 2. Scope contract

### 2.1 In scope

- Export engine: JPEG/TIFF/PNG with quality/resize/sharpen-for-output options, ICC embedding, metadata and copyright, file naming templates, folder structures, per-set exports (gallery, album, social, teaser, B&W).
- XMP/sidecar export for Lightroom hand-off, plus a Lightroom Classic plugin (import selection and recipes, round-trip) and a Photoshop plugin (open with masks and retouch layers where feasible).
- Backup: local/NAS/external destinations with verification hashes, plus optional cloud object storage; delivery bundle manifest with checksums.
- Client gallery integrations: Pic-Time / ShootProof / SmugMug / Google Drive / Dropbox style connectors via a pluggable provider interface, with upload resumption and per-set mapping.
- Learning loop: capture every user override (culling, parameters, masks, retouch strength, curation reorder), attribute it to the decision in the Phase 13 ledger, aggregate into preference updates, and retrain/adjust style profiles and ranker weights incrementally - locally, with an explicit review before adoption.
- Model and profile update channel: signed model packs, delta downloads, staged rollout, and one-click rollback.
- Release engineering: code signing and notarisation for Windows/macOS, installer, auto-update, crash reporting with opt-in, structured telemetry with consent, feature flags and kill switches.
- Licensing and entitlement: offline-tolerant licence checks, seat management, trial mode with clear limits.
- Support tooling: anonymised support bundles (Phase 13), diagnostics screen, and a reproducible-issue workflow.

### 2.2 Explicitly out of scope (do not build it here)

- Building a proprietary client gallery product (integrate, do not compete).
- Cross-machine distributed rendering (post-V1).
- Marketplace for third-party profiles (post-V1).

## 3. Architecture and data flow

```text
finished gallery + curation sets
     |
  EXPORT: naming templates, ICC, metadata, per-set outputs, verification hashes
     |
     +--> local/NAS/cloud backup (manifest + checksums)
     +--> XMP sidecars -> Lightroom plugin round-trip -> Photoshop hand-off
     +--> client gallery providers (resumable upload, per-set mapping)
     |
  USER CORRECTIONS (culling, params, masks, retouch, curation)
     |
  attribute to decisions (P13 ledger) -> preference aggregation -> profile/ranker updates
     |
  review & adopt (A/B vs current) -> signed profile/model update -> staged rollout | rollback
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-export/src/{lib,jpeg,tiff,naming,metadata,sets,verify,manifest}.rs` | Export engine. |
| `crates/aura-delivery/src/{lib,backup,providers/*,resume,mapping}.rs` | Backup and gallery providers. |
| `crates/aura-learn/src/{lib,capture,attribute,aggregate,update,review,rollback}.rs` | Learning loop. |
| `plugins/lightroom/` and `plugins/photoshop/` | Integration plugins. |
| `ops/{release,sign,notarise,update,flags,crash}/` | Release engineering. |
| `docs/{delivery,learning-loop,release-process,privacy}.md` | Operational documentation. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Delivery and learning contracts (frozen)**

```rust
pub struct ExportJob {
    pub sets: Vec<ExportSet>,          // { name, images, format, quality, resize, sharpen, naming }
    pub destination: Destination,      // Folder | Nas | CloudBucket | Provider(ProviderId)
    pub metadata: MetadataPolicy,      // copyright, contact, keywords, strip_gps
    pub verify: bool,                  // hash-verify every written file
}

pub struct DeliveryManifest {
    pub project: ProjectId, pub created_at: Timestamp,
    pub files: Vec<(PathBuf, u64, String)>,   // path, bytes, hash
    pub sets: Vec<(String, u32)>,
    pub qc_report_path: Option<PathBuf>,
    pub cleanup_disclosures: Vec<(ImageId, String)>,
    pub engine_versions: Vec<(String, String)>,
}

pub struct Correction {
    pub decision_id: DecisionId, pub kind: DecisionKind,
    pub before_json: String, pub after_json: String,
    pub scene: SceneId, pub identity: Option<IdentityId>,
    pub magnitude: f32, pub created_at: Timestamp,
}

pub struct LearningUpdate {
    pub profile_id: ProfileId, pub from_version: u16, pub to_version: u16,
    pub corrections_used: u32,
    pub expected_improvement: f32,     // measured on held-out corrections
    pub diff_summary: Vec<String>,
    pub adopted: bool,
}
```

## 6. Algorithm, model and implementation design

### 6.1 Export that a professional can trust

- Verification is mandatory by default: every written file is re-read and hashed, and the manifest records it - photographers have lost galleries to silent write failures.
- Naming templates cover the real conventions (date, couple, chapter, sequence, camera, original name) with collision-safe suffixes.
- Output sharpening is resolution-aware and applied after resize; metadata policy can strip GPS while preserving copyright.
- Per-set exports mean gallery, album, social, teaser and B&W sets come out of one job with correct sizes and aspects.

### 6.2 Integrations without lock-in

- XMP sidecars are the universal path: any photographer can take AURA's culling and grading into Lightroom, which lowers adoption risk enormously.
- The Lightroom plugin imports selections, flags, colour labels and recipes, and can round-trip corrections back into AURA as learning-loop input.
- Gallery providers sit behind one trait with resumable uploads, per-set mapping and clear error surfaces; adding a provider must not touch core code.

### 6.3 The learning loop, done safely

- Capture: every override is written as a `Correction` attributed to the originating decision, with scene and identity context.
- Aggregate: group corrections by (decision kind, scene bucket, identity role) and compute robust central tendencies; require a minimum count before acting, and discard outliers.
- Update: adjust the Phase 17 style deltas, Phase 10/11 ranker weights and Phase 12 threshold offsets incrementally - never a full retrain in the background without consent.
- Verify before adopting: measure expected improvement on held-out corrections and show an A/B comparison; the user adopts explicitly, and one click rolls back.
- All learning is local by default; contributing anonymised data to the Wedding Intelligence Dataset is strictly opt-in per project with a clear consent record.

### 6.4 Release engineering

- Signed and notarised installers, staged rollout by percentage, feature flags with kill switches for every AI stage, and a documented rollback within one release cycle.
- Model packs are versioned, signed and delta-updated; a model rollback must be possible without downgrading the app.
- Crash reporting and structured telemetry are opt-in, contain no image content, and are documented in the privacy page.
- Nightly long-run CI on real GPU hardware plus the full golden/eval suite gates every release; the release checklist is owned by EM and signed by CTO.

## 7. Cloud AI usage (bring-your-own API key)

No cloud AI call in this phase. The phase must work with the network cable unplugged; the Cloud AI Gateway from Phase 04 stays idle here.

## 8. Implementation order (execute literally, in this order)

1. Implement the export engine with naming, metadata, ICC, per-set outputs and verification.
2. Implement backup destinations with manifests and checksum verification.
3. Implement the provider interface and two gallery providers with resumable upload.
4. Ship XMP sidecar export and the Lightroom plugin with round-trip.
5. Ship the Photoshop hand-off (masks and retouch layers where feasible).
6. Implement correction capture and attribution to ledger decisions.
7. Implement aggregation, incremental updates, held-out verification and A/B review.
8. Implement signed model/profile update channel with staged rollout and rollback.
9. Implement licensing, crash reporting, telemetry consent and feature flags.
10. Build the release pipeline: signing, notarisation, installers, auto-update, nightly long-run CI.
11. Run a closed beta with 20 photographers; triage, fix, and gate V1 on the exit criteria.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `DEVOPS` | DevOps / Release Engineer | Release pipeline, signing, notarisation, installers, auto-update, staged rollout, rollback | Release machinery | 12 d |
| `TLC` | Tech Lead - Imaging Core (Rust) | Export engine architecture, provider trait design, plugin boundaries | Architecture + review | 5 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Export implementation, naming, metadata, verification, manifests | `aura-export` | 8 d |
| `MBE` | Mid-Level Backend / Cloud Engineer | Backup destinations, provider implementations, resumable upload, error surfaces | `aura-delivery` | 8 d |
| `MLOPS` | MLOps / Model Packaging Engineer | Learning loop: capture, aggregation, incremental updates, A/B verification, rollback | `aura-learn` | 9 d |
| `MLL` | ML Lead - Vision | Define which parameters may be learned, robustness rules and improvement metrics | Learning spec | 4 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Export dialog, delivery screen, provider setup, learning-review UI, diagnostics screen | UI shipped | 8 d |
| `MFE` | Mid-Level Frontend Engineer | Naming template editor, per-set configuration, upload progress, rollback dialog | UI panels | 5 d |
| `SEC` | Security & Privacy Engineer | Licence security, telemetry/consent review, provider credential storage, privacy page sign-off | Security sign-off | 5 d |
| `PM` | Product Manager Agent | Own V1 exit criteria, pricing/licensing model, beta programme and launch messaging | Launch plan | 6 d |
| `QAL` | QA Lead - Automation | Export fidelity tests, verification tests, provider mocks, learning-loop regression gates | CI gates | 7 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Closed beta triage with 20 photographers; own the launch bug bar | Beta report | 10 d |
| `EM` | Engineering Manager / Delivery Lead Agent | Own the release checklist, beta logistics, cross-team burndown to launch | Launch readiness | 8 d |
| `PERF` | Performance Engineer | Export throughput tuning; upload concurrency; long-run stability | Benchmark | 4 d |
| `DOC` | Technical Writer | Delivery guide, plugin docs, learning-loop explainer, privacy page, release notes process | Docs merged | 6 d |
| `CTO` | Chief Architect / CTO Agent | Sign the V1 release gate; approve rollback criteria and the post-launch on-call plan | Release sign-off | 2 d |

### 9.1 Handoff chain for this phase

```text
TLC architecture -> SRC export engine + MBE delivery/providers -> LR/PS plugins
                                |
                                v
          MLL learning spec -> MLOPS learning loop -> SFE/MFE delivery + review UI
                                |
     DEVOPS release machinery + SEC privacy/licence review -> EM/QAIQ closed beta -> CTO V1 gate
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

- Export fidelity: rendered JPEG/TIFF match the reference render within a perceptual tolerance; ICC and metadata correct.
- Verification catches a deliberately corrupted write and fails the job with a clear error.
- Naming templates produce collision-free names across 4,000 files, including duplicate original names from two cameras.
- XMP round-trip: Lightroom shows AURA's selections and grading; corrections made there return as learning-loop input.
- Provider uploads resume correctly after a network drop; per-set mapping is respected.
- Learning loop improves style match by >= 15 % after 3 corrected weddings on held-out corrections, and rollback restores the previous profile exactly.
- No learning update is adopted without explicit user action; opt-in dataset contribution is off by default and recorded with consent.
- Signed model packs verify; tampered packs are rejected; model rollback works without downgrading the app.
- Installers are signed and notarised; auto-update applies and can be rolled back; kill switches disable each AI stage.
- Crash-free session rate >= 99.5 % across the closed beta.

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
| Export 1,000 images (45 MP JPEG, GPU) | <= 12 min |
| Export throughput | >= 1.4 images/s sustained |
| Hash verification overhead | <= 8 % of export time |
| Upload 1,000 images (100 Mbps) | <= 35 min with resumption |
| Learning update computation | <= 90 s per wedding of corrections |

Telemetry events (local-first, opt-in aggregation):

- `export.job` {sets, images, format, ms, verified, destination_kind}
- `delivery.upload` {provider, images, bytes, ms, resumes}
- `learn.corrections` {kind, count, mean_magnitude}
- `learn.update` {profile, expected_improvement, adopted}
- `release.update` {from_version, to_version, channel, rolled_back}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Silent export corruption | Mandatory hash verification, delivery manifest, and a failing job rather than a bad gallery. |
| Learning loop degrades quality over time | Held-out verification, explicit adoption, A/B comparison, versioned profiles, and one-click rollback. |
| Privacy backlash over telemetry or data collection | Opt-in only, no image content in telemetry, per-project consent for dataset contribution, and a plain-language privacy page. |
| Plugin breakage on Lightroom/Photoshop updates | Version detection, graceful degradation to XMP-only, and a compatibility matrix in CI. |
| Launch quality risk from 30 phases of integration | Closed beta with 20 photographers, published exit criteria, nightly long-run CI, feature flags and staged rollout. |
| Provider API changes | Single provider trait, contract tests against mocks and live sandboxes, and clear error surfaces. |

## 13. Acceptance criteria

- [ ] Finished galleries export in every required format and set, verified by checksums, with a delivery manifest.
- [ ] Lightroom and Photoshop users can adopt AURA without abandoning their workflow.
- [ ] Client galleries and backups upload automatically with resumption.
- [ ] Every correction a photographer makes measurably improves their next wedding, only after they approve the update.
- [ ] Releases are signed, staged, flag-controlled and reversible; models can roll back independently.
- [ ] V1 exit criteria are met and signed off by the CTO after the closed beta.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 30 - Delivery, Integrations, Learning Loop & Release Engineering.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-30-DELIVERY-INTEGRATIONS-LEARNING-LOOP.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - Export and delivery (JPEG/TIFF/XMP, backup, client galleries), Lightroom and Photoshop integration, the learning loop that improves from every correction, and the release machinery that ships it all safely.

Rules:
  - Do not start Phase 31. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-export/src/{lib,jpeg,tiff,naming,metadata,sets,verify,manifest}.rs`, `crates/aura-delivery/src/{lib,backup,providers/*,resume,mapping}.rs`, `crates/aura-learn/src/{lib,capture,attribute,aggregate,update,review,rollback}.rs`, `plugins/lightroom/` and `plugins/photoshop/`, `ops/{release,sign,notarise,update,flags,crash}/`, `docs/{delivery,learning-loop,release-process,privacy}.md`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-30-delivery-integrations-learning-loop and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-30.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-30-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-30-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-30-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 30 of 30 - Delivery, Integrations, Learning Loop & Release Engineering - part of the AURA Wedding AI master build plan.*

---

# Pull Request

## Phase

Phase: `NN - <title>`
Tasks completed: `<role codes and task names>`

## What changed

<Two or three sentences. What a reviewer needs to know before reading the diff.>

## Invariants

- [ ] RAW files untouched; all edits expressed as recipe changes
- [ ] `user_edited` fields respected and never overwritten
- [ ] Every new AI decision emits structured reasons into the ledger
- [ ] Determinism preserved (seeded, sorted iteration, no wall-clock branching)
- [ ] Local path complete without cloud; cloud path budgeted and cached
- [ ] No secrets, image content or personal data in logs or telemetry
- [ ] Feature flag and kill switch present for any new AI stage
- [ ] No identity-altering operation introduced

## Gates

| Gate | Before | After | Budget | Pass |
| --- | --- | --- | --- | --- |
|  |  |  |  |  |

## Tests

- Unit / property:
- Golden / perceptual:
- Integration on fixture wedding:
- Benchmark deltas:

## Screenshots or renders

<Before/after at 100 % zoom for anything that changes pixels.>

## Docs and ADRs

- [ ] Phase document updated if reality diverged from the plan
- [ ] ADR added for any architectural or contract change
- [ ] Model card updated if a model changed
- [ ] User-facing docs updated

## Reviewer hat

Reviewing role (must differ from implementing role): `____`
What I tried to break:
