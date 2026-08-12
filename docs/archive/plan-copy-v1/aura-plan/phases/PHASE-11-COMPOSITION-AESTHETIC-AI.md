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
