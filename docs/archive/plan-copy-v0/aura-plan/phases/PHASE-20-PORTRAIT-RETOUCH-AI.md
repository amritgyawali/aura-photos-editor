# Phase 20 - Portrait Retouch AI with Natural Texture Protection

> **Single feature shipped by this phase:** Professional-grade automatic skin retouching: blemishes, temporary marks, dark circles and uneven tone corrected while pores, fine lines and real texture are preserved by design.
>
> **Mission:** Match or beat Retouch4me/Evoto/Aperty in retouch quality while being scene-aware, identity-aware and gallery-consistent - and never producing plastic skin.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 20 of 30 |
| Epic | E4 - Retouch & Restoration |
| Feature | Professional-grade automatic skin retouching: blemishes, temporary marks, dark circles and uneven tone corrected while pores, fine lines and real texture are preserved by design. |
| Depends on | Phases 14, 18, 19 |
| Unlocks | Phases 21, 25, 27 |
| Duration | 3 weeks |
| Primary owners | ML Lead - Vision, Senior ML Engineer, Colour Scientist, Senior Engineer - GPU & Render (Rust / wgpu / CUDA) |
| Risk level | High - the most scrutinised output in the product |
| Headline KPI | texture retention >= 0.9 (band-energy metric); blemish removal recall >= 0.9 / false-removal <= 2 %; retouch <= 350 ms/image at full resolution |
| Competitor being beaten | Retouch4me, Evoto, Aperty, Portraiture |

## 1. Why this phase exists

Retouching is the most emotionally sensitive part of wedding delivery: clients want to look like themselves on a good day, not like a mannequin. A retoucher that removes a pimple but keeps skin texture is worth real money.

It is also the most competitive segment, so quality must be measurable. A texture-retention metric and a false-removal metric make quality claims defensible rather than marketing copy.

Distinguishing temporary from permanent features is an ethical requirement as well as a quality one: freckles, moles, scars and birthmarks are identity, not defects.

## 2. Scope contract

### 2.1 In scope

- Blemish detection and inpainting: pimples, spots, temporary redness, small scratches - detected as *temporary* features and removed with texture-preserving patch synthesis.
- Permanent-feature preservation: freckles, moles, birthmarks, scars, tattoos and dimples explicitly detected and protected, with a user toggle per identity.
- Dark-circle and under-eye correction: luminance and chroma correction in the periorbital region with strict caps and no texture loss.
- Skin tone evening: mid-frequency unevenness (blotches, flush, makeup mismatch, neck/face mismatch) corrected by frequency-selective operations.
- Texture protection engine: frequency-band decomposition with a hard guarantee that high-frequency band energy in skin regions stays within a measured ratio of the original.
- Strength model: automatic strength per identity and per scene (a close-up portrait gets more care than a wide dance frame), plus global Light/Natural/Polished presets.
- Per-identity consistency: the same person is retouched with the same strength across the gallery, so skin does not change character between frames.
- Full-resolution execution at export with a proxy-accurate preview; every operation recorded in the recipe as a reversible retouch op.

### 2.2 Explicitly out of scope (do not build it here)

- Hair, teeth, eyes, clothing and glare (Phase 21).
- Noise reduction and sharpening (Phase 22).
- Body reshaping or slimming - explicitly excluded as a product-ethics decision.

## 3. Architecture and data flow

```text
skin/face masks (P18) + local light (P19) + identity + scene
     |
     +--> BlemishDetector -> candidates { box, type, temporary_prob, confidence }
     |          |
     |          +--> PermanentFeatureClassifier (freckle/mole/scar/tattoo) -> protect set
     |          |
     |          +--> texture-preserving inpaint (patch synthesis + frequency blend)
     |
     +--> UnderEyeCorrector (luma/chroma, capped, texture-safe)
     +--> ToneEveningEngine (mid-frequency only, high-frequency preserved)
                       |
            TextureGuard: measure high-band energy ratio -> re-solve if below floor
                       |
       recipe.retouch[] ops + per-identity strength + reasons + confidence
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-retouch/src/{lib,blemish,permanent,undereye,evening,texture_guard,strength,ops}.rs` | Retouch engine. |
| `crates/aura-render/shaders/{inpaint_patch,freq_bands,retouch_apply}.wgsl` | GPU retouch operators. |
| `ml/models/retouch/{train_blemish.py,train_permanent.py,eval_retouch.py,export.py}` | Detection models. |
| `config/retouch_presets.toml` | Light/Natural/Polished definitions and per-scene strengths. |
| `apps/desktop/src/components/develop/RetouchPanel.tsx` | Strength, presets, per-identity settings, protected features. |
| `docs/model-cards/{blemish_detector,permanent_features}.md` | Model cards with skin-tone metrics. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Retouch operation contract**

```rust
pub enum RetouchOp {
    Blemish { box: Box2, method: InpaintMethod, strength: f32 },
    UnderEye { identity: IdentityId, luma: f32, chroma: f32 },
    ToneEvening { mask: MaskId, strength: f32, band: FreqBand },
    ShineReduce { box: Box2, strength: f32 },     // shared with P19
}

pub struct RetouchPlan {
    pub image_id: ImageId,
    pub ops: Vec<RetouchOp>,
    pub per_identity_strength: HashMap<IdentityId, f32>,
    pub protected: Vec<ProtectedFeature>,        // { box, kind, identity }
    pub texture_report: TextureReport,           // { band_ratio, floor, passed }
    pub preset: RetouchPreset,
    pub reasons: Vec<Reason>, pub confidence: f32,
}
```

## 6. Algorithm, model and implementation design

### 6.1 Temporary versus permanent - the ethical core

- Two-stage detection: find skin anomalies, then classify each as temporary (pimple, redness, scratch, makeup smudge) or permanent (mole, freckle, scar, birthmark, tattoo, beauty mark).
- Cross-frame evidence is decisive and unique to a gallery-aware product: a spot that appears on the same facial coordinate in many frames across hours is permanent; one that appears in a few frames is temporary or transient lighting.
- Permanent features are added to a per-identity protect list, visible and editable in the UI ('keep her beauty mark' as an explicit setting).
- Default behaviour is conservative: uncertain anomalies are left alone, because removing a client's mole is a far worse error than leaving a pimple.

### 6.2 Texture-preserving removal

- Inpaint by patch synthesis from nearby *skin of the same person and lighting*, matching frequency content, then blend only the low/mid bands while transplanting the original high band back.
- Small blemishes use a healing-brush-equivalent (offset patch + gradient blend); larger areas use a learned inpainting network constrained to the skin mask.
- All work happens in linear light on the full-resolution render at export; the preview uses an identical algorithm at proxy scale so what the user approves is what ships.

### 6.3 Texture guard with a measurable guarantee

- Decompose skin regions into frequency bands; measure high-band energy before and after retouching.
- Hard floor: post-retouch high-band energy >= 0.90 of the original (configurable per preset, never below 0.80 even in Polished). If violated, re-solve with lower strength and log it.
- This turns 'we don't produce plastic skin' from a claim into a tested invariant with a number in CI.

### 6.4 Strength and consistency

- Automatic strength from face size in frame, scene class, identity role and preset; a full-frame portrait of the bride gets the most careful treatment, a background guest almost none.
- Per-identity strength is fixed across the gallery so the same person's skin character does not fluctuate between images.
- Under-eye correction is capped hard (typical luma lift <= 0.25 EV inside the periorbital mask) because over-correction is the classic tell of automated retouching.

## 7. Cloud AI usage (bring-your-own API key)

No cloud AI call in this phase. The phase must work with the network cable unplugged; the Cloud AI Gateway from Phase 04 stays idle here.

## 8. Implementation order (execute literally, in this order)

1. Write the retouch ethics policy (no body reshaping, protect permanent features, conservative defaults); PM and CTO sign.
2. Label blemish and permanent-feature data across skin tones.
3. Train the anomaly detector and the temporary/permanent classifier.
4. Implement cross-frame permanence evidence using identity-aligned facial coordinates.
5. Implement patch-synthesis inpainting with frequency-band blending on GPU.
6. Implement under-eye correction and mid-frequency tone evening.
7. Implement the texture guard with measurement and re-solve.
8. Implement per-identity strength assignment and gallery consistency.
9. Build the retouch panel with presets, per-identity controls and protected-feature management.
10. Run blind expert comparison against competitor outputs; publish the results internally.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `MLL` | ML Lead - Vision | Own detection/classification design, texture metric, evaluation protocol | Signed spec + gates | 4 d |
| `SRML` | Senior ML Engineer | Train blemish detector and permanent-feature classifier; export and verify across skin tones | Models registered | 10 d |
| `COL` | Colour Scientist | Frequency-band decomposition correctness, linear-light inpainting, texture measurement | Validated engine | 7 d |
| `SRG` | Senior Engineer - GPU & Render (Rust / wgpu / CUDA) | GPU inpainting and band-blend shaders; full-resolution execution path | Shaders + tests | 7 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Cross-frame permanence, strength assignment, consistency, recipe ops, persistence | `aura-retouch` + tests | 8 d |
| `DATA` | Data Engineer / Dataset Curator | Blemish/permanent labels on 15k faces across five skin-tone buckets, with consent | Labels v1 | 12 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Retouch panel, presets, per-identity strength, protected-feature list with visual markers | Retouch UI | 5 d |
| `MFE` | Mid-Level Frontend Engineer | Before/after at 100 % zoom, batch strength adjust, per-scene overrides | UI panels | 3 d |
| `QAL` | QA Lead - Automation | Texture-retention gate, false-removal gate, cross-frame consistency test, zoom artefact test | CI gates | 5 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Blind A/B against Retouch4me/Evoto/Aperty outputs judged by retouchers | Competitive study | 5 d |
| `PM` | Product Manager Agent | Own the retouch ethics policy and preset definitions; approve default = Natural | Policy + presets | 2 d |
| `PERF` | Performance Engineer | Full-resolution retouch within budget; batch throughput at export | Benchmark | 3 d |
| `SEC` | Security & Privacy Engineer | Confirm no face crops leave the device for retouching | Sign-off | 1 d |
| `DOC` | Technical Writer | Document presets, protected features and the texture guarantee | Docs merged | 3 d |

### 9.1 Handoff chain for this phase

```text
PM ethics policy -> DATA labels -> SRML models -> COL/SRG engine
                                            |
                                            v
                            SRC strength + consistency -> SFE/MFE UI
                                            |
                QAL texture gates + QAIQ competitive blind study -> MLL/PM/CTO gate
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

- Texture retention >= 0.90 band-energy ratio on all presets; Polished never below 0.80.
- Blemish recall >= 0.90 with false-removal of permanent features <= 2 % (and 0 % for tattoos).
- Cross-frame consistency: the same identity's retouch strength varies by <= 5 % across a gallery.
- Under-eye correction never exceeds its cap; no visible light patches at 100 % zoom.
- Per-skin-tone metrics show no bucket more than 10 % worse than the best bucket.
- Proxy preview matches full-resolution output within a perceptual tolerance.
- Blind study: retouchers rate AURA >= parity with the best competitor on natural appearance.

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
| Retouch at full resolution (45 MP, GPU) | <= 350 ms |
| Retouch at proxy (2048 px) | <= 45 ms |
| 1,000-image gallery retouch at export | <= 7 min |
| CPU fallback (45 MP) | <= 4 s |

Telemetry events (local-first, opt-in aggregation):

- `retouch.applied` {images, ops, preset, mean_strength, ms}
- `retouch.texture_guard` {triggered, mean_band_ratio}
- `retouch.protected` {kind, count}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Plastic skin | Hard texture-retention floor enforced in CI with automatic strength re-solve. |
| Removing permanent features (mole, tattoo, scar) | Dedicated classifier, cross-frame permanence evidence, conservative defaults, protected-feature UI, and a 0 % tattoo-removal gate. |
| Skin-tone bias in detection | Balanced labelling, per-bucket metrics, and a ship gate on parity. |
| Preview/export mismatch | Identical algorithm at both scales plus a perceptual comparison test. |
| Ethical creep toward body reshaping | Explicit written policy excluding it; any such feature requires CTO and PM approval and a separate consent design. |

## 13. Acceptance criteria

- [ ] Temporary blemishes disappear while pores, fine lines and permanent features remain.
- [ ] Freckles, moles, scars and tattoos are preserved by default and listed as protected.
- [ ] Under-eye and uneven tone corrections are visible as improvement but not as retouching.
- [ ] The same person looks like the same person across the whole gallery.
- [ ] Texture retention is measured, gated in CI and reported in the UI.
- [ ] Blind expert study shows parity or better versus leading retouch tools.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 20 - Portrait Retouch AI with Natural Texture Protection.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-20-PORTRAIT-RETOUCH-AI.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - Professional-grade automatic skin retouching: blemishes, temporary marks, dark circles and uneven tone corrected while pores, fine lines and real texture are preserved by design.

Rules:
  - Do not start Phase 21. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-retouch/src/{lib,blemish,permanent,undereye,evening,texture_guard,strength,ops}.rs`, `crates/aura-render/shaders/{inpaint_patch,freq_bands,retouch_apply}.wgsl`, `ml/models/retouch/{train_blemish.py,train_permanent.py,eval_retouch.py,export.py}`, `config/retouch_presets.toml`, `apps/desktop/src/components/develop/RetouchPanel.tsx`, `docs/model-cards/{blemish_detector,permanent_features}.md`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-20-portrait-retouch-ai and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-20.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-20-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-20-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-20-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 20 of 30 - Portrait Retouch AI with Natural Texture Protection - part of the AURA Wedding AI master build plan.*
