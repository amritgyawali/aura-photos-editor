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
