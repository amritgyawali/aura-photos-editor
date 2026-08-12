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
