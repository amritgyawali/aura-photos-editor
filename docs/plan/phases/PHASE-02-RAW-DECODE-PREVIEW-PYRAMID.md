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
