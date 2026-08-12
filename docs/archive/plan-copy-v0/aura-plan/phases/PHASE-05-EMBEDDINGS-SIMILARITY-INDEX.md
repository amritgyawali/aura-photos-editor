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
