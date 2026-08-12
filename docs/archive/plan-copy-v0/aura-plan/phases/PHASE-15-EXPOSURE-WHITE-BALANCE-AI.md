# Phase 15 - Exposure AI & White Balance AI (mixed lighting mastery)

> **Single feature shipped by this phase:** Per-image exposure and white balance decided by scene, subject and skin - including the hard cases: tungsten receptions, mixed daylight-plus-LED ceremonies, backlit exits and coloured stage lighting.
>
> **Mission:** Deliver the first visible 'this actually looks edited' moment. Correct exposure and believable skin colour across a whole wedding is what photographers judge an editor by in the first ten seconds.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 15 of 30 |
| Epic | E3 - Photo Brain |
| Feature | Per-image exposure and white balance decided by scene, subject and skin - including the hard cases: tungsten receptions, mixed daylight-plus-LED ceremonies, backlit exits and coloured stage lighting. |
| Depends on | Phases 06, 07, 09, 14 |
| Unlocks | Phases 16, 17, 25, 26, 27 |
| Duration | 2.5 weeks |
| Primary owners | Colour Scientist, ML Lead - Vision, Senior ML Engineer |
| Risk level | High - the most visible AI decision in the product |
| Headline KPI | exposure within +/-0.15 EV of expert on 85 % of frames; WB within 200 K / 4 tint on 85 %; skin dE00 <= 3.0 vs expert reference |
| Competitor being beaten | Imagen and Aftershoot auto-exposure/WB; Lightroom Auto |

## 1. Why this phase exists

Mixed lighting is the single hardest technical problem in wedding photography and the place where generic auto-WB fails hardest. Solving it with face- and scene-aware models is both a real user benefit and a credible technical differentiator.

Exposure must be subject-referred, not average-referred: the bride's face is the anchor, not the mean luminance of a dark reception hall. That one change fixes most of what photographers hate about auto-exposure.

## 2. Scope contract

### 2.1 In scope

- Exposure model: predicts EV offset targeting correct *subject* luminance, respecting scene intent (moody dance floors stay moody), with clipping-aware constraints from Phase 09.
- Face-anchored exposure: target face luminance bands per skin-tone group (measured, not assumed), weighted by Phase 06 prominence.
- White balance model: predicts temperature/tint from RAW statistics plus face skin patches plus known-neutral detection (white dress, tablecloths, paper), scene-conditioned.
- Mixed-light detection and handling: identify multi-illuminant frames, choose the illuminant that governs the subject, and record a `MIXED_LIGHT` note so Phase 18 can apply local WB corrections later.
- Coloured stage/LED light handling: detect saturated non-neutral illumination and deliberately *not* neutralise it (a purple dance floor should stay purple) while keeping skin plausible.
- Skin-tone-aware constraints: WB solutions are rejected if they push skin outside a plausible chromaticity locus for that person; the constraint is per-identity, learned across the wedding.
- Consistency seed: per-scene reference frames chosen here and handed to Phase 25 for gallery normalisation.
- Everything written into the recipe with confidence and reasons; every value overridable and then protected.

### 2.2 Explicitly out of scope (do not build it here)

- Tone curve, contrast and HSL (Phase 16).
- Photographer style preference (Phase 17 shifts these baselines).
- Local masks and face lighting (Phases 18-19).
- Gallery-wide normalisation (Phase 25).

## 3. Architecture and data flow

```text
RAW stats (histogram, per-channel) + faces + skin patches + neutral candidates + scene
        |
        +--> IlluminantEstimator (multi-hypothesis) --> illuminants[], mixed?
        |
        +--> SubjectLuminanceTarget (per scene, per skin group, prominence-weighted)
        |
        v
   constrained solve:  minimise (skin chroma error + neutral error)
                       subject to (no new clipping, scene intent, plausible skin locus)
        |
        v
   recipe.global { exposure, temperature, tint } + reasons + confidence + reference-frame hints
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-brain-photo/src/tone/{exposure,wb,illuminant,skin_locus,neutrals,solve}.rs` | Exposure and WB estimation. |
| `ml/models/tone/{train_exposure.py,train_wb.py,eval_tone.py,export.py}` | Model training against expert edits. |
| `config/exposure_targets.toml` | Per-scene subject luminance bands and intent rules. |
| `crates/aura-catalog/migrations/0015_tone.sql` | `image_tone_estimate` table with alternatives. |
| `apps/desktop/src/components/develop/BasicPanel.tsx` | Exposure/WB controls with AI badge and reset-to-AI. |
| `docs/model-cards/{exposure,white_balance}.md` | Model cards including skin-tone fairness analysis. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Tone estimation output**

```rust
pub struct ToneEstimate {
    pub image_id: ImageId,
    pub exposure_ev: f32, pub exposure_conf: f32,
    pub temperature_k: f32, pub tint: f32, pub wb_conf: f32,
    pub illuminants: Vec<Illuminant>,      // { kind, cct, weight, region }
    pub mixed_light: bool, pub dominant_on_subject: Option<usize>,
    pub subject_luma_before: f32, pub subject_luma_target: f32,
    pub skin_de00_estimate: f32,
    pub alternatives: Vec<(f32, f32, f32)>, // (ev, cct, tint) runner-up solutions
    pub reasons: Vec<Reason>,
}
```

## 6. Algorithm, model and implementation design

### 6.1 Exposure: subject-referred with intent

- Compute the prominence-weighted mean luminance of face regions in linear light; compare against the scene's target band from `exposure_targets.toml` (e.g. ceremony faces at 45-55 % linear-to-perceptual, dance floor 30-40 %).
- Solve for the EV offset that lands the subject in band, then clamp so highlight clipping does not increase beyond the scene tolerance and shadow noise stays within the Phase 09 budget.
- When no face is present (details, venue), fall back to a learned scene-level exposure model trained on expert edits of the same scene class.
- Backlit frames get a dedicated rule: expose for the subject, accept blown background above a threshold, and note it as intentional in the reasons.

### 6.2 White balance: constrained multi-illuminant solve

- Generate illuminant hypotheses from grey-world, white-patch, learned CNN prediction and known-neutral regions (dress, tablecloth, paper), each with a weight.
- Score each hypothesis by how plausible it makes the *skin* of known identities: project skin patches into a chromaticity space and measure distance from that person's estimated locus (accumulated across the wedding, so a single bad frame cannot mislead).
- Choose the illuminant governing the subject; if two strong illuminants disagree spatially, mark `mixed_light` and record both, leaving local correction to Phase 18.
- For saturated coloured lighting, detect that the illuminant is intentionally non-neutral (high chroma, stage scene, low CRI signature) and preserve mood: correct only enough to keep skin within its plausible locus.

### 6.3 Skin fairness, explicitly engineered

- Skin targets are measured per identity from the wedding's own best-lit frames, not from a fixed 'ideal' skin value - this is how the system avoids lightening or warming darker skin toward a Eurocentric target.
- Evaluation reports dE00 per skin-tone group (Monk scale buckets); the model cannot ship if any group is more than 1.0 dE00 worse than the best group.
- The skin locus constraint is a hard constraint in the solve, not a post-hoc adjustment.

### 6.4 Handing consistency forward

- For each Phase 07 segment, select 3-5 reference frames: high WB confidence, good subject exposure, primary identity present, no mixed light.
- Store them with the segment; Phase 25 normalises the rest of the segment toward these anchors, which is the mechanism behind gallery-wide consistency.

## 7. Cloud AI usage (bring-your-own API key)

No cloud AI call in this phase. The phase must work with the network cable unplugged; the Cloud AI Gateway from Phase 04 stays idle here.

## 8. Implementation order (execute literally, in this order)

1. Assemble the training set: RAW + expert final edits with exposure/WB parameters across traditions and lighting types (DATA).
2. Implement RAW statistics extraction, neutral detection and skin-patch sampling.
3. Implement the illuminant hypothesis generators including a learned CNN predictor.
4. Implement the per-identity skin locus accumulation.
5. Implement the constrained solve for exposure and WB with clipping and noise constraints.
6. Train and validate the scene-level exposure model for faceless frames.
7. Implement mixed-light and coloured-light handling with explicit notes.
8. Implement reference-frame selection per segment.
9. Write results into recipes with reasons and confidence; wire the develop panel badges.
10. Run fairness evaluation per skin-tone group and publish the model cards.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `COL` | Colour Scientist | Own illuminant estimation, skin locus modelling, colour validation and fairness measurement | Validated solver + report | 9 d |
| `MLL` | ML Lead - Vision | Own exposure targets, learned model design, evaluation protocol against expert edits | Signed spec + gates | 4 d |
| `SRML` | Senior ML Engineer | Train WB CNN and scene exposure model; export, calibrate, verify parity | Models registered | 7 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Implement solver plumbing, recipe writing, reference-frame selection, persistence | `tone` module + tests | 6 d |
| `DATA` | Data Engineer / Dataset Curator | Collect RAW + expert-edit pairs across 8 lighting classes and 5 skin-tone buckets | Dataset v3 | 10 d |
| `QAL` | QA Lead - Automation | EV/CCT/dE00 gates, fairness gate, mixed-light fixtures, no-face fixtures | CI gates | 5 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Expert review of 600 frames across lighting types; catalogue systematic bias | Audit report | 4 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Basic panel with AI badges, confidence, reset-to-AI, and a mixed-light indicator | Develop UI | 3 d |
| `MFE` | Mid-Level Frontend Engineer | Per-scene review queue for low-confidence WB, batch accept/adjust | UI panel | 3 d |
| `PM` | Product Manager Agent | Approve `exposure_targets.toml` and the 'preserve mood' policy with the consultant | Approved config | 2 d |
| `PERF` | Performance Engineer | Keep estimation under 25 ms per image; share statistics with Phase 09 | Benchmark | 2 d |
| `DOC` | Technical Writer | Write the mixed-lighting explainer and the skin-fairness statement | Docs merged | 2 d |

### 9.1 Handoff chain for this phase

```text
DATA expert pairs -> SRML models -> COL solver + skin locus
                                        |
                                        v
                        SRC recipe writing + reference frames -> SFE/MFE UI
                                        |
                     QAL gates + QAIQ expert audit -> COL/PM gate -> Phases 16, 25
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

- Exposure within +/-0.15 EV of expert on >= 85 % of the held-out set; never increases clipping beyond scene tolerance.
- WB within 200 K and 4 tint units on >= 85 %; tungsten reception and mixed-LED fixtures included.
- Skin dE00 <= 3.0 mean and <= 1.0 spread across skin-tone buckets (fairness gate).
- Coloured stage lighting is preserved, not neutralised (dedicated fixture set).
- Faceless frames (details, venue) are exposed sensibly by the scene model.
- Reference frames are selected for every segment with at least three candidates.
- Determinism: identical RAW + identical config produce identical parameters.

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
| Estimation per image (GPU) | <= 25 ms |
| 4,000 images total | <= 100 s |
| Extra storage per image | <= 600 B |

Telemetry events (local-first, opt-in aggregation):

- `tone.estimated` {images, ms, mean_ev, mean_cct, mixed_light_ratio}
- `tone.low_confidence` {count, scene_histogram}
- `tone.user_override` {param, delta}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Skin-tone bias | Per-identity measured targets, mandatory per-bucket fairness gate, and refusal to ship a model that regresses any bucket. |
| Neutralising creative lighting | Explicit coloured-light detection, PM-approved preserve-mood policy, and fixture tests. |
| Mixed light produces visibly split colour | Detect and defer to local WB in Phase 18; mark frames so QC can verify. |
| Auto-exposure fights the photographer's intent | Scene intent targets, style shift in Phase 17, and protected user overrides. |

## 13. Acceptance criteria

- [ ] Exposure and WB are set automatically for every selected frame with reasons and confidence.
- [ ] Faces are correctly exposed in dark receptions without flattening the mood.
- [ ] Skin colour is believable across skin tones, measured and published.
- [ ] Coloured stage lighting survives editing.
- [ ] Mixed-light frames are flagged for local correction rather than badly globally corrected.
- [ ] Every segment has reference frames ready for gallery consistency.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 15 - Exposure AI & White Balance AI (mixed lighting mastery).

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-15-EXPOSURE-WHITE-BALANCE-AI.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - Per-image exposure and white balance decided by scene, subject and skin - including the hard cases: tungsten receptions, mixed daylight-plus-LED ceremonies, backlit exits and coloured stage lighting.

Rules:
  - Do not start Phase 16. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-brain-photo/src/tone/{exposure,wb,illuminant,skin_locus,neutrals,solve}.rs`, `ml/models/tone/{train_exposure.py,train_wb.py,eval_tone.py,export.py}`, `config/exposure_targets.toml`, `crates/aura-catalog/migrations/0015_tone.sql`, `apps/desktop/src/components/develop/BasicPanel.tsx`, `docs/model-cards/{exposure,white_balance}.md`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-15-exposure-white-balance-ai and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-15.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-15-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-15-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-15-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 15 of 30 - Exposure AI & White Balance AI (mixed lighting mastery) - part of the AURA Wedding AI master build plan.*
