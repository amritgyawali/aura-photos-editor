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
