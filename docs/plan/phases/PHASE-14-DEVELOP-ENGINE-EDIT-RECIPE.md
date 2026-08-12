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
