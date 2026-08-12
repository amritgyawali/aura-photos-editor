# Phase 21 - Micro-Retouch Suite: Hair, Teeth, Eyes, Clothing & Glare

> **Single feature shipped by this phase:** The small fixes a retoucher makes without being asked: stray hairs tamed, teeth and eyes subtly corrected, lint and clothing distractions cleaned, glasses glare and reflections reduced.
>
> **Mission:** Close the remaining quality gap with high-end manual retouching by handling the details that photographers currently fix by hand, one image at a time.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 21 of 30 |
| Epic | E4 - Retouch & Restoration |
| Feature | The small fixes a retoucher makes without being asked: stray hairs tamed, teeth and eyes subtly corrected, lint and clothing distractions cleaned, glasses glare and reflections reduced. |
| Depends on | Phases 18, 20 |
| Unlocks | Phases 27, 28 |
| Duration | 2.5 weeks |
| Primary owners | ML Lead - Vision, Senior ML Engineer, Senior Engineer - GPU & Render (Rust / wgpu / CUDA), Colour Scientist |
| Risk level | Medium-High - subtlety and 'uncanny' risk |
| Headline KPI | flyaway reduction rated >= 4/5 with no bald patches; teeth/eye corrections judged natural >= 95 %; micro pass <= 250 ms/image full res |
| Competitor being beaten | Retouch4me's specialist modules; Evoto's portrait tools |

## 1. Why this phase exists

These are the fixes that make a delivered gallery feel finished. Photographers currently do them manually on their best 30 frames; automating them across 1,000 frames is a large, concrete time saving.

They are also where automation most easily looks creepy - whitened teeth, glowing eyes, erased hair. Doing them conservatively and identity-aware is the differentiator, not doing them harder.

## 2. Scope contract

### 2.1 In scope

- Hair intelligence: detect stray flyaways against clean backgrounds and reduce them (not erase), preserving hairline naturalness; explicitly skip complex textures and busy backgrounds.
- Teeth correction: mild luminance evening and yellow-cast reduction bounded by a natural-teeth colour locus, with a hard ceiling so no one gets fluorescent teeth.
- Eye enhancement: catchlight preservation, sclera redness reduction, iris micro-clarity, with hard caps; no eye enlargement, no colour changes.
- Clothing cleanup: lint, small stains, visible bra straps if the user enables it, stray threads, and creases only where the user opts in.
- Glare and reflection reduction on glasses: detect specular sheets over eyes, reduce or reconstruct using the other frames in the moment when available (cross-frame borrowing).
- Nostril/ear/neck micro-corrections limited to shine and shadow evening (no shape changes).
- Per-operation opt-in matrix with studio-level defaults, plus per-identity respect for protected features from Phase 20.
- Cross-frame borrowing infrastructure: use a sibling frame from the same moment as a source for reconstructing a small region (glasses glare, closed eye in a group frame is *not* included - that is deliberately excluded).

### 2.2 Explicitly out of scope (do not build it here)

- Skin texture work (Phase 20).
- Removing people or large objects (Phase 24).
- Eye or face swapping between frames - explicitly excluded as a product-ethics decision (composite portraits are not delivered without disclosure).

## 3. Architecture and data flow

```text
masks (P18) + retouch plan (P20) + moment siblings (P08)
     |
     +--> FlyawayDetector (hair-vs-background contrast) -> reduce (alpha-aware)
     +--> TeethModule (luma even + yellow reduce, locus-bounded)
     +--> EyeModule (sclera redness, iris clarity, catchlight preserve, capped)
     +--> ClothingModule (lint/stain/thread detect -> inpaint, opt-in matrix)
     +--> GlareModule (specular sheet detect -> reduce | cross-frame borrow)
                       |
              NaturalnessGuard (per-op ceilings + locus constraints)
                       |
           recipe.retouch[] micro ops + reasons + confidence
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-retouch/src/micro/{hair,teeth,eyes,clothing,glare,borrow,guard}.rs` | Micro-retouch modules. |
| `ml/models/micro/{train_flyaway.py,train_glare.py,train_lint.py,eval_micro.py}` | Detection models. |
| `config/micro_retouch.toml` | Opt-in matrix, ceilings and colour loci. |
| `apps/desktop/src/components/develop/MicroRetouchPanel.tsx` | Per-operation toggles with previews. |
| `docs/retouch-ethics.md` | What AURA will and will not do to people's appearance. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Micro-retouch operations**

```rust
pub enum MicroOp {
    Flyaway { region: Box2, strength: f32 },
    Teeth { identity: IdentityId, luma: f32, yellow_reduce: f32 },
    Eyes { identity: IdentityId, sclera: f32, iris_clarity: f32 },
    Clothing { region: Box2, kind: ClothingIssue, strength: f32 },
    Glare { region: Box2, method: GlareMethod },   // Reduce | BorrowFrom(ImageId)
}

pub struct NaturalnessGuard {
    pub teeth_max_luma: f32,        // hard ceiling
    pub teeth_locus: ColourLocus,
    pub sclera_max: f32, pub iris_max: f32,
    pub flyaway_max_area_frac: f32,
    pub require_confidence: f32,    // below this, skip the op entirely
}
```

## 6. Algorithm, model and implementation design

### 6.1 Hair without bald patches

- Detect flyaways as thin high-contrast structures outside the hair alpha but connected to it; require a clean, low-detail background, otherwise skip.
- Reduce rather than remove: attenuate contrast against the background by up to a capped amount, preserving some strands so the hairline still reads as real hair.
- Never modify inside the hair mass; a strict area cap (fraction of frame) prevents runaway edits.

### 6.2 Teeth and eyes with hard ceilings

- Teeth: even the luminance across the teeth mask and reduce yellow toward a *natural* locus derived from real teeth measurements, with a ceiling far below cosmetic whitening; skip entirely if the mask confidence is low or the mouth is small in frame.
- Eyes: reduce sclera redness (chroma only), add small iris local contrast, and explicitly protect catchlights by excluding specular pixels; no enlargement, no colour change, no whitening of the sclera beyond a cap.
- Every ceiling is in `micro_retouch.toml` with a rationale and a fixture demonstrating the maximum allowed effect.

### 6.3 Clothing and glare

- Lint/thread/stain detection as small anomaly detection restricted to the clothing mask, with inpainting reused from Phase 20; creases and wrinkles are opt-in only, since removing them can look artificial.
- Glasses glare: detect specular sheets overlapping the eye region; if a sibling frame from the same moment has the same face without glare and closely matching geometry, borrow that region with alignment and frequency blending; otherwise reduce highlight intensity conservatively.
- Cross-frame borrowing is limited to small regions, requires high alignment confidence, and is always recorded in the recipe and the Explain panel so it is never a hidden composite.

### 6.4 Ethics as engineering

- A written policy file lists forbidden operations: body reshaping, face swapping, eye replacement, skin lightening, and anything that changes identity.
- Guard code enforces the ceilings; a CI test attempts to exceed each ceiling and asserts refusal.
- Opt-in matrix means studios choose their own standards, and the delivery report states which operations were applied.

## 7. Cloud AI usage (bring-your-own API key)

No cloud AI call in this phase. The phase must work with the network cable unplugged; the Cloud AI Gateway from Phase 04 stays idle here.

## 8. Implementation order (execute literally, in this order)

1. Publish `docs/retouch-ethics.md` and get PM/CTO sign-off before implementation.
2. Label flyaway, glare, lint and teeth/eye cases; measure natural teeth and sclera loci from real data.
3. Train the flyaway, glare and lint detectors.
4. Implement hair reduction with area caps and background gating.
5. Implement teeth and eye modules with locus constraints and ceilings.
6. Implement clothing cleanup reusing Phase 20 inpainting.
7. Implement glare reduction and cross-frame borrowing with alignment.
8. Implement the naturalness guard and the opt-in matrix.
9. Build the micro-retouch panel with per-operation previews and studio defaults.
10. Run the naturalness audit and the ceiling-refusal tests.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `MLL` | ML Lead - Vision | Own detector design, locus measurement methodology and naturalness evaluation | Signed spec | 3 d |
| `SRML` | Senior ML Engineer | Train flyaway/glare/lint detectors; export and validate across skin tones and hair types | Models registered | 8 d |
| `COL` | Colour Scientist | Measure natural teeth/sclera loci; validate chroma-only operations | Locus definitions | 4 d |
| `SRG` | Senior Engineer - GPU & Render (Rust / wgpu / CUDA) | GPU implementations, cross-frame alignment and blending | Shaders + align | 6 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Module orchestration, guard enforcement, opt-in matrix, recipe ops, delivery reporting | `micro` module + tests | 6 d |
| `DATA` | Data Engineer / Dataset Curator | Labels for flyaways, glare, lint; hair-type diversity coverage | Labels v1 | 7 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Micro-retouch panel with per-op toggles, previews and studio defaults | UI shipped | 4 d |
| `QAL` | QA Lead - Automation | Ceiling-refusal tests, bald-patch detection test, catchlight preservation test | CI gates | 4 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Naturalness audit of 400 frames; specifically hunt uncanny teeth/eyes and hair damage | Audit report | 4 d |
| `PM` | Product Manager Agent | Own the ethics policy and default opt-in matrix; approve ceilings | Policy + defaults | 2 d |
| `PERF` | Performance Engineer | Keep the micro pass under budget; share masks and bands with Phase 20 | Benchmark | 2 d |
| `DOC` | Technical Writer | Publish the ethics document and per-operation guidance | Docs merged | 2 d |

### 9.1 Handoff chain for this phase

```text
PM ethics policy -> COL loci + DATA labels -> SRML detectors
                                     |
                                     v
                       SRG GPU ops + SRC guard/orchestration -> SFE UI
                                     |
                     QAL ceiling tests + QAIQ naturalness audit -> PM/CTO gate
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

- Hair: no bald patches or hairline damage on any fixture; area cap never exceeded.
- Teeth: luminance and chroma stay inside the natural locus; ceiling-exceed attempts are refused.
- Eyes: catchlights preserved (specular pixel test); no geometry change measurable.
- Clothing: lint removal recall >= 0.85 with no fabric-texture damage at 100 % zoom.
- Glare: borrowed regions align within tolerance and are always disclosed in the recipe and Explain panel.
- Forbidden operations are impossible: automated attempts to reshape or swap are rejected by guard code.
- Naturalness audit: >= 95 % of corrections judged natural by retouchers.

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
| Micro pass at full resolution | <= 250 ms |
| Micro pass at proxy | <= 35 ms |
| Cross-frame borrow (alignment + blend) | <= 180 ms |
| 1,000-image gallery | <= 5 min added at export |

Telemetry events (local-first, opt-in aggregation):

- `micro.applied` {op, count, mean_strength, ms}
- `micro.skipped` {op, reason}
- `micro.borrow` {count, alignment_score}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Uncanny results (glowing teeth, alien eyes) | Hard ceilings with locus constraints, CI refusal tests, and a naturalness audit gate. |
| Hair damage | Background gating, area caps, reduce-not-remove policy, and dedicated fixtures across hair types. |
| Hidden composites from cross-frame borrowing | Always recorded and disclosed; limited to small regions; never used for eyes or expressions. |
| Scope creep into cosmetic surgery features | Written ethics policy with CTO/PM gate on any change. |

## 13. Acceptance criteria

- [ ] Flyaway hair is calmed without damaging the hairline.
- [ ] Teeth and eyes look better but unmistakably natural, with catchlights intact.
- [ ] Lint and small clothing distractions are cleaned without harming fabric texture.
- [ ] Glasses glare is reduced, and any borrowed pixels are disclosed.
- [ ] Forbidden identity-changing operations are structurally impossible.
- [ ] Studios can configure exactly which micro-operations they allow.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 21 - Micro-Retouch Suite: Hair, Teeth, Eyes, Clothing & Glare.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-21-MICRO-RETOUCH-SUITE.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - The small fixes a retoucher makes without being asked: stray hairs tamed, teeth and eyes subtly corrected, lint and clothing distractions cleaned, glasses glare and reflections reduced.

Rules:
  - Do not start Phase 22. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-retouch/src/micro/{hair,teeth,eyes,clothing,glare,borrow,guard}.rs`, `ml/models/micro/{train_flyaway.py,train_glare.py,train_lint.py,eval_micro.py}`, `config/micro_retouch.toml`, `apps/desktop/src/components/develop/MicroRetouchPanel.tsx`, `docs/retouch-ethics.md`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-21-micro-retouch-suite and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-21.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-21-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-21-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-21-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 21 of 30 - Micro-Retouch Suite: Hair, Teeth, Eyes, Clothing & Glare - part of the AURA Wedding AI master build plan.*
