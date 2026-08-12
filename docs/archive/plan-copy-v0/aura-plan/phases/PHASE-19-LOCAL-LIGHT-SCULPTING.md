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
