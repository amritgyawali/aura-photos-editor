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
