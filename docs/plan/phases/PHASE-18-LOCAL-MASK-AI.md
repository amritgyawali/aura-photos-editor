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
