# Phase 09 - Frame Integrity AI: Focus, Motion, Exposure, Noise & Eye State

> **Single feature shipped by this phase:** Every frame gets an honest technical verdict where it matters: is the *right subject* sharp, is motion intentional, is exposure recoverable, how noisy is it, and are the important eyes open?
>
> **Mission:** Replace global 'is this photo sharp?' with subject-aware, scene-aware, intent-aware technical judgement - including the blink detection that photographers care about most.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 09 of 30 |
| Epic | E2 - Wedding Brain |
| Feature | Every frame gets an honest technical verdict where it matters: is the *right subject* sharp, is motion intentional, is exposure recoverable, how noisy is it, and are the important eyes open? |
| Depends on | Phases 02, 05, 06, 07 |
| Unlocks | Phases 12, 13, 22, 27 |
| Duration | 2.5 weeks |
| Primary owners | ML Lead - Vision, Senior ML Engineer, Colour Scientist, Senior Engineer - Core Pipeline (Rust) |
| Risk level | High - false rejections destroy trust instantly |
| Headline KPI | subject-focus AUC >= 0.96; blink detection F1 >= 0.95 with intentional-closed-eye false-positive rate <= 2 %; analysis <= 45 ms/image |
| Competitor being beaten | Narrative Select eye assessment; Aftershoot blur detection; FilterPixel technical scoring |

## 1. Why this phase exists

Photographers abandon culling tools the moment one is thrown away that should have been kept. Technical scoring must therefore be conservative, explainable and subject-aware: a blurred background is craft, a blurred bride is a defect.

Blink handling is the classic trust test. Closed eyes during a kiss, a prayer, a first-look reaction or a tearful hug are *good* photographs. A naive eye-open detector destroys exactly the frames that sell weddings.

Exposure and noise verdicts must be recovery-aware: a 2-stop-under dance frame from a modern sensor is fine, the same from an older body is not. That requires camera-aware, RAW-aware analysis rather than JPEG heuristics.

## 2. Scope contract

### 2.1 In scope

- Region-aware sharpness: per-face and per-subject sharpness via a learned focus head plus classical measures (Laplacian variance, Tenengrad, MTF50 estimate on eye regions), calibrated per camera resolution.
- Motion analysis: distinguish camera shake (global directional smear), subject motion (local smear with sharp background) and intentional motion (panning, dance blur), using directional gradient statistics and EXIF shutter/focal length.
- Focus-miss detection: is the sharpest plane on the subject or behind/in front of it (back/front focus), using relative sharpness of face vs background bands.
- Exposure verdict: clipping percentages per channel, recoverable-highlight estimate from RAW headroom, shadow noise floor estimate, and a scene-aware 'recoverable / marginal / lost' label.
- Noise estimation: per-image sigma estimate in flat regions, ISO-aware and camera-aware, expressed as a scene-relative tolerance.
- Eye state per face: open / squint / closed / looking-down / occluded, plus intent classification (`intentional_closed` when the scene, expression and context justify it).
- Composite `technical_score` and `integrity_flags` bitfield, all scene-weighted and all with reasons.
- Camera calibration table: per-body sharpness and noise normalisation so a 61 MP body is not unfairly favoured over a 24 MP body.

### 2.2 Explicitly out of scope (do not build it here)

- Expression quality and emotional value (Phase 10).
- Composition (Phase 11).
- Any decision to keep or reject (Phase 12).
- Fixing noise or blur (Phase 22).

## 3. Architecture and data flow

```text
proxy (P02) + faces (P06) + scene (P07) + EXIF
   |
   +--> FocusHead (learned, per-region) --+
   +--> classical sharpness (Laplacian/Tenengrad/MTF50 on eyes) --+--> subject_sharpness, bg_sharpness
   +--> directional gradient stats + shutter/focal --> motion_kind (shake|subject|intentional|none)
   +--> RAW histogram + headroom --> exposure_verdict (recoverable|marginal|lost)
   +--> flat-region sigma + ISO/camera table --> noise_sigma_rel
   +--> EyeStateHead per face --> open|squint|closed|down|occluded + intent
                        |
                        v
     technical_score (scene-weighted) + integrity_flags + reasons[]
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-brain-photo/src/integrity/{focus,motion,exposure,noise,eyes,score,flags}.rs` | All technical analysis. |
| `crates/aura-brain-photo/src/integrity/calibration.rs` + `config/camera_calibration.toml` | Per-body sharpness/noise normalisation. |
| `ml/models/integrity/{train_focus.py,train_eyes.py,eval_integrity.py,export.py}` | Focus and eye-state models. |
| `crates/aura-catalog/migrations/0009_integrity.sql` | `image_integrity`, `face_eye_state` tables. |
| `apps/desktop/src/components/explain/IntegrityCard.tsx` | Per-image technical readout with crops. |
| `docs/model-cards/{focus_head,eye_state}.md` | Model cards including intentional-closed-eye analysis. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Integrity result (frozen)**

```rust
bitflags! { pub struct IntegrityFlags: u32 {
    const SUBJECT_SOFT       = 1<<0;  const CAMERA_SHAKE      = 1<<1;
    const SUBJECT_MOTION     = 1<<2;  const INTENTIONAL_MOTION= 1<<3;
    const BACK_FOCUS         = 1<<4;  const FRONT_FOCUS       = 1<<5;
    const HIGHLIGHT_LOST     = 1<<6;  const SHADOW_LOST       = 1<<7;
    const HEAVY_NOISE        = 1<<8;  const EYES_CLOSED       = 1<<9;
    const EYES_CLOSED_OK     = 1<<10; const SQUINT            = 1<<11;
    const NO_SUBJECT_DETECTED= 1<<12; const MIXED_LIGHT_RISK  = 1<<13;
} }

pub struct IntegrityResult {
    pub image_id: ImageId,
    pub subject_sharpness: f32,      // 0..1, prominence-weighted
    pub bg_sharpness: f32,
    pub focus_offset: f32,           // negative = front focus, positive = back focus
    pub motion: MotionKind, pub motion_severity: f32,
    pub exposure: ExposureVerdict, pub clip_hi: f32, pub clip_lo: f32, pub ev_offset: f32,
    pub noise_sigma_rel: f32,
    pub eyes: Vec<EyeState>,         // per face, with identity + intent
    pub technical_score: f32,        // scene-weighted 0..1
    pub flags: IntegrityFlags,
    pub reasons: Vec<Reason>,        // { code, text, weight, evidence_crop }
    pub confidence: f32,
}
```

## 6. Algorithm, model and implementation design

### 6.1 Subject-aware sharpness

- Compute sharpness on eye regions first (that is what viewers judge), then face, then body, then background bands; combine with Phase 06 prominence weights.
- Normalise by camera calibration: expected MTF50 for the body/lens/aperture combination, so 'sharp' means sharp *for this gear*.
- A learned focus head trained on labelled sharp/soft crops handles cases classical measures fail on: shallow depth of field, veil textures, backlit rim light, heavy bokeh.
- Output both absolute and *within-moment relative* sharpness, because Phase 12 mostly needs 'which of these six is sharpest'.

### 6.2 Motion intent classification

- Estimate the dominant gradient direction and anisotropy: global anisotropic smear + long shutter => camera shake; local smear with sharp background => subject motion; strong panning signature with sharp subject => intentional.
- Use EXIF: shutter vs 1/focal-length rule, stabilisation flags, and scene (`dance_floor` and `exit` expect motion; `family_portrait` does not).
- Never flag intentional motion as a defect; instead expose `INTENTIONAL_MOTION` so Phase 12 can treat it as a stylistic keeper.

### 6.3 Exposure and noise, recovery-aware

- Compute clipping from the RAW histogram before tone mapping, per channel, with the specular-highlight exclusion mask (a clipped candle flame is not a defect).
- Estimate recoverable headroom from `white_level` and per-camera measured headroom; label `recoverable` if a +/- correction stays within the noise budget for that ISO.
- Noise sigma is measured in flat regions (low local gradient) and normalised by ISO and camera; expressed relative to the scene profile's tolerance so a dance frame is not punished for being a dance frame.

### 6.4 Eye state and intent - the trust-critical part

- Per-face eye-state head on aligned crops with five classes plus a confidence; only faces above the Phase 06 quality gate are judged.
- Intent rules: closed eyes are acceptable when (scene in {kiss, vows, ritual, hug, first_look, speeches_emotional}) OR (mouth-smile strong and head tilted) OR (tears detected in Phase 10) OR (both partners' eyes closed simultaneously in a couple frame).
- Only the *important* identities' eyes gate a frame: a guest blinking in row four is not a defect; the bride blinking in a portrait is.
- Group photos get a special path: count how many primary/secondary subjects have closed eyes and expose `closed_eye_ratio` for Phase 12's group rules.

### 6.5 Scoring and explainability

- `technical_score = product of scene-weighted sub-scores with soft penalties`, deliberately not a linear sum, so one catastrophic factor cannot be averaged away.
- Every penalty writes a `Reason` with a code, human text, weight and an evidence crop rectangle so the UI can literally show the soft eye.
- Calibration: scores are mapped through a per-scene isotonic regression fitted on labelled keeper/reject data so 0.8 means the same thing everywhere.

## 7. Cloud AI usage (bring-your-own API key)

No cloud AI call in this phase. The phase must work with the network cable unplugged; the Cloud AI Gateway from Phase 04 stays idle here.

## 8. Implementation order (execute literally, in this order)

1. Build the camera calibration harness and populate `camera_calibration.toml` for the top 20 bodies.
2. Implement classical sharpness measures with per-region support and unit tests on synthetic blur.
3. Label sharp/soft crops and train the focus head; verify on shallow-DOF and backlit fixtures.
4. Implement motion classification with EXIF fusion; validate on panning and dance fixtures.
5. Implement RAW-based exposure verdicts with specular exclusion.
6. Implement noise estimation and ISO/camera normalisation (COL validates).
7. Label eye states including intentional-closed cases; train and calibrate the eye head.
8. Implement the intent rules and the group-photo closed-eye ratio.
9. Compose `technical_score`, fit per-scene calibration, and emit reasons with evidence crops.
10. Build the Integrity card UI showing crops and reasons; run the QA audit.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `MLL` | ML Lead - Vision | Own scoring design, calibration methodology and the conservative-rejection policy | Signed spec | 3 d |
| `SRML` | Senior ML Engineer | Train focus head and eye-state head; export, calibrate, verify parity | Models registered | 7 d |
| `MLR` | ML Research Engineer | Motion-intent research, anisotropy features, ablations vs learned alternatives | Research report | 4 d |
| `COL` | Colour Scientist | RAW headroom measurement per body, noise model validation, specular exclusion rules | Calibration table | 4 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Implement all analysers, flags, scoring, persistence, evidence crops | `integrity` module + tests | 7 d |
| `DATA` | Data Engineer / Dataset Curator | Label sharpness, focus-miss, motion kind, eye state + intent on 20k crops | Labels v1 | 8 d |
| `QAL` | QA Lead - Automation | AUC/F1 gates, synthetic blur suite, false-rejection audit harness | CI gates | 4 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | False-rejection hunt: review every frame the system calls defective on 5 weddings | Audit + bug list | 4 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Integrity card with zoomable evidence crops and reason list | Explain UI part 1 | 3 d |
| `MFE` | Mid-Level Frontend Engineer | Filter chips (soft, blinked, clipped, noisy) and a technical-review queue | UI panels | 2 d |
| `PERF` | Performance Engineer | Keep per-image analysis under budget; fuse passes to avoid re-reading pixels | Benchmark | 2 d |
| `PM` | Product Manager Agent | Approve the intent rules with a working photographer; write the trust policy | Approved rules | 2 d |
| `DOC` | Technical Writer | Document every reason code in user language | Reason-code reference | 2 d |

### 9.1 Handoff chain for this phase

```text
COL calibration + DATA labels -> SRML models -> SRC analysers
                                        |
                                        v
                        MLL calibration/scoring -> SFE/MFE explain UI
                                        |
                       QAIQ false-rejection audit -> PM trust sign-off -> gate
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

- Synthetic blur ladder: monotonic sharpness response; back/front focus correctly signed.
- Subject-focus AUC >= 0.96 on labelled keeper/reject crops; shallow-DOF portraits never flagged soft.
- Blink F1 >= 0.95; intentional-closed-eye false positives <= 2 % on the kiss/vows/hug fixture set.
- Exposure: recoverable vs lost matches expert labels >= 0.93; candle and sparkler frames not flagged.
- Noise: estimated sigma within 15 % of measured sigma on the ISO ladder fixtures.
- Group photos: closed-eye ratio matches human count exactly on 200 labelled group frames.
- Cross-camera fairness: identical scenes shot on 24 MP and 61 MP bodies score within 0.05.

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
| Integrity analysis per image (GPU) | <= 45 ms |
| Integrity analysis per image (CPU) | <= 220 ms |
| 4,000 images total (RTX 4070) | <= 180 s |
| Storage per image | <= 1 KB including per-face eye states |

Telemetry events (local-first, opt-in aggregation):

- `integrity.scored` {images, ms, mean_score, flag_histogram}
- `integrity.eyes` {faces, closed, closed_ok, squint}
- `integrity.camera_uncalibrated` {make, model}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| False rejections destroy trust | Conservative thresholds, relative-within-moment scoring, mandatory QAIQ false-rejection audit, and reasons with visual evidence on every penalty. |
| Intentional motion or closed eyes flagged as defects | Explicit intent classification, scene profiles, PM-approved rules, and dedicated fixture sets. |
| Camera bias favours high-resolution bodies | Per-body calibration table plus a cross-camera fairness test in CI. |
| Uncalibrated new camera | Fallback normalisation by sensor resolution with a telemetry event and a monthly calibration sprint. |

## 13. Acceptance criteria

- [ ] Every image exposes subject sharpness, motion kind, exposure verdict, noise level and per-face eye state with reasons.
- [ ] A shallow-depth-of-field portrait with creamy bokeh is never called soft.
- [ ] A kiss with closed eyes is flagged `EYES_CLOSED_OK`, not as a defect.
- [ ] A camera-shake ceremony frame and a panned exit frame are distinguished correctly.
- [ ] Scores are calibrated per scene so 0.8 means the same thing in a ceremony and on a dance floor.
- [ ] The Integrity card shows the exact crop that caused each penalty.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 09 - Frame Integrity AI: Focus, Motion, Exposure, Noise & Eye State.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-09-FRAME-INTEGRITY-AI.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - Every frame gets an honest technical verdict where it matters: is the *right subject* sharp, is motion intentional, is exposure recoverable, how noisy is it, and are the important eyes open?

Rules:
  - Do not start Phase 10. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-brain-photo/src/integrity/{focus,motion,exposure,noise,eyes,score,flags}.rs`, `crates/aura-brain-photo/src/integrity/calibration.rs` + `config/camera_calibration.toml`, `ml/models/integrity/{train_focus.py,train_eyes.py,eval_integrity.py,export.py}`, `crates/aura-catalog/migrations/0009_integrity.sql`, `apps/desktop/src/components/explain/IntegrityCard.tsx`, `docs/model-cards/{focus_head,eye_state}.md`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-09-frame-integrity-ai and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-09.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-09-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-09-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-09-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 09 of 30 - Frame Integrity AI: Focus, Motion, Exposure, Noise & Eye State - part of the AURA Wedding AI master build plan.*
