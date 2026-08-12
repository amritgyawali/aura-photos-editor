# Phase 12 - Autonomous Culling Engine, Story Coverage Guard & Gallery Sizing

> **Single feature shipped by this phase:** One decision engine turns 3,000 frames into a delivered gallery: per-moment winners, scene quotas, coverage guarantees, duplicate suppression and an automatically chosen gallery size.
>
> **Mission:** Make the single most valuable decision in the product defensibly, transparently and fast - and never lose an important part of the wedding story while doing it.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 12 of 30 |
| Epic | E2 - Wedding Brain |
| Feature | One decision engine turns 3,000 frames into a delivered gallery: per-moment winners, scene quotas, coverage guarantees, duplicate suppression and an automatically chosen gallery size. |
| Depends on | Phases 07-11 |
| Unlocks | Phases 13, 14, 27, 28, 29, 30 |
| Duration | 3 weeks |
| Primary owners | ML Lead - Vision, Senior Engineer - Core Pipeline (Rust), Product Manager Agent, ML Research Engineer |
| Risk level | Critical - this is the product |
| Headline KPI | photographer agreement on keepers >= 0.85; missed-must-have rate = 0 on fixtures; 3,000-image cull end-to-end < 8 min |
| Competitor being beaten | Aftershoot, Imagen, FilterPixel, Narrative Select - all of them, directly |

## 1. Why this phase exists

Culling is where photographers lose 6-10 hours per wedding, and it is the decision they trust least to software. Winning here wins the product; a single lost 'must-have' frame loses a customer forever.

Competitors cull frame-by-frame with global thresholds. Culling as *constrained optimisation over moments and chapters* - with coverage guarantees, quotas and diversity - is a structurally better approach and is defensible in marketing because the reasoning is inspectable.

Automatic gallery sizing removes the last configuration burden: photographers should not have to guess whether to deliver 600 or 1,100 images; the story and the shoot volume determine it.

## 2. Scope contract

### 2.1 In scope

- Unified per-image `keep_score` fusing technical integrity (P09), emotion (P10), composition (P11), subject prominence (P06) and scene weights (P07), with per-scene calibration.
- Moment-level selection: pick 1..k winners per moment where k depends on diversity, moment significance and scene profile; suppress near-identical frames.
- Chapter quotas: target counts per chapter derived from shot volume, chapter duration and importance, with min/max bands.
- Story Coverage Guard: a hard rule set of must-have moments (rings, vows, kiss, first look, entrances, cake, first dance, family formals, each close-family member, venue establishing shot, exit) that cannot be empty if any candidate exists at all.
- Coverage of people: every `family_close` identity must appear at least N times; every `guest` identity appearing more than M times should appear at least once.
- Diversity constraints: avoid 40 near-identical dance frames; enforce spread across time, framing (wide/medium/tight) and identity mix.
- Automatic gallery sizing: predicted deliverable count from a regression over shoot volume, chapter count, moment count and quality distribution, with user-adjustable target and a live slider that re-runs selection in under 2 s.
- Three autonomy modes: `Conservative` (keeps more, flags more), `Balanced`, `Aggressive` (tight gallery), plus `Zero-Touch` behaviour defined in Phase 28.
- Rejection reasons for every rejected frame and 'runner-up' pointers for every kept frame (used by QC in Phase 27 for replacements).
- Deterministic, resumable, incremental: re-running with the same inputs gives identical output; changing one weight re-selects in seconds without re-analysis.

### 2.2 Explicitly out of scope (do not build it here)

- Editing the selected frames (Phase 14+).
- QC-driven replacement (Phase 27 consumes runner-ups).
- Hero/album/social picks (Phase 29).

## 3. Architecture and data flow

```text
per-image scores: integrity, emotion, composition, prominence
        |
        v
  keep_score fusion (scene-weighted, calibrated)
        |
        v
  MOMENT PASS: winners per moment (k from diversity + significance)
        |
        v
  CHAPTER PASS: quotas per chapter (duration x volume x importance)
        |
        v
  COVERAGE GUARD: must-have moments + per-identity minimums (hard constraints)
        |
        v
  DIVERSITY PASS: framing/time/identity spread, near-duplicate suppression
        |
        v
  SIZING: target count reconciliation (add best runner-ups / drop weakest)
        |
        v
  selection[] + rejection reasons[] + runner_up[] + coverage report
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-cull/src/{lib,fusion,moment_pass,chapter_pass,coverage,diversity,sizing,modes,explain}.rs` | The culling engine. |
| `config/cull_weights.toml` + `config/coverage_rules.toml` | PM-owned weights and must-have rules. |
| `crates/aura-catalog/migrations/0012_selection.sql` | `selection`, `rejections`, `coverage_report` tables. |
| `apps/desktop/src/routes/cull/{CullView,SizeSlider,CoveragePanel,RejectReasons}.tsx` | Culling UI. |
| `ml/eval/cull_agreement.py` | Photographer agreement harness. |
| `tests/fixtures/labels/keepers_*.json` | Human keeper ground truth per fixture wedding. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Selection contracts (frozen)**

```rust
pub struct KeepScore {
    pub image_id: ImageId,
    pub technical: f32, pub emotion: f32, pub composition: f32, pub prominence: f32,
    pub scene_weighted: f32,          // final keep_score 0..1
    pub calibration_ver: u16,
}

pub struct SelectionResult {
    pub selected: Vec<Selected>,      // ordered by timeline
    pub rejected: Vec<Rejected>,
    pub coverage: CoverageReport,
    pub target_count: u32, pub actual_count: u32,
    pub mode: CullMode, pub deterministic_hash: u64,
}

pub struct Selected {
    pub image_id: ImageId, pub moment_id: MomentId,
    pub keep_score: f32, pub confidence: f32,
    pub reasons: Vec<Reason>,
    pub runner_up: Option<ImageId>,   // best alternative in the same moment
    pub coverage_role: Option<MustHave>,  // why it is protected, if it is
}

pub struct CoverageReport {
    pub must_haves: Vec<(MustHave, Coverage)>,   // Covered | CoveredWeak | Missing(no candidates)
    pub identity_coverage: Vec<(IdentityId, u32)>,
    pub chapter_counts: Vec<(ChapterId, u32, u32)>, // actual, target
    pub warnings: Vec<String>,
}
```

## 6. Algorithm, model and implementation design

### 6.1 Score fusion that cannot be gamed by one signal

- `keep_score = w_t*technical^a * w_e*emotion^b * w_c*composition^c` in log space with scene-specific weights, so a catastrophic technical failure cannot be rescued by emotion and vice versa.
- Hard vetoes: subject completely out of focus, primary identity's eyes closed without intent in a posed scene, or exposure `lost` - these bypass fusion and reject with a clear reason.
- Hard promotions: unique must-have moments with no alternative are protected even at lower scores, and the reason says so honestly ('only frame of the ring exchange').
- Per-scene isotonic calibration so `keep_score` thresholds mean the same thing everywhere.

### 6.2 Moment and chapter passes

- Within a moment, rank by `keep_score`; k = 1 for low-diversity moments, up to 3-5 for high-diversity, high-significance moments (dance sequences, bouquet toss, ritual peaks).
- Guarantee the moment's peak frame (Phase 10) is either selected or explicitly rejected with a reason - never silently dropped.
- Chapter targets: `target_c = round(total_target * importance_c * sqrt(volume_c) / sum(...))`, clamped by min/max bands from `coverage_rules.toml`; details/venue chapters get small fixed allocations.
- Solve the allocation with a greedy pass followed by a bounded local-search improvement (swap in a runner-up if it raises total score without breaking constraints).

### 6.3 Story Coverage Guard - the anti-catastrophe layer

- Must-have rules are declarative: `{ id: 'kiss', match: scene in [kiss] or interaction=kiss, min: 2, prefer: peak }`.
- For each rule, if the selection contains fewer than `min` frames, force-add the best available candidates even if below threshold, marking them `CoveredWeak` with a visible warning.
- If no candidate exists at all (the photographer never shot it), report `Missing` with 'no candidates found' - the product must be clear that it cannot invent coverage.
- Identity coverage: every close-family identity gets >= 3 frames, every recurring guest >= 1, resolved by adding their best frames; this is the feature that prevents 'my aunt isn't in the gallery' complaints.

### 6.4 Sizing, modes and determinism

- Predict the deliverable count with a small regression trained on real delivered galleries (features: total frames, moments, chapters, hours, keeper-score distribution); typical output 22-38 % of shot volume.
- The size slider re-runs only the allocation passes (not analysis), so it feels instant; the coverage guard always runs last so shrinking never breaks must-haves.
- Modes shift thresholds and k-values, never the coverage rules - even `Aggressive` cannot drop a must-have.
- Determinism: stable sorts, integer seeds, and a `deterministic_hash` over inputs+config recorded in the ledger so a support case can be reproduced exactly.

## 7. Cloud AI usage (bring-your-own API key)

**Break genuine ties inside a moment when local scores are statistically indistinguishable**

| Aspect | Specification |
|---|---|
| Model class | Vision reasoning tier, temperature 0 |
| Trigger | Top-2 candidates within 0.02 `keep_score` AND the moment is significant (peak or must-have) |
| Input sent | 2-6 thumbnails (768 px) with their score breakdowns, scene, and anonymised subject roles |
| Cost control | <= 30 calls per wedding; cached; skipped entirely when cloud is off |
| Offline fallback | Deterministic tie-break: higher subject sharpness, then peak proximity, then earlier timestamp |

System prompt contract:

```text
You are a wedding photo editor choosing which single frame to deliver from a set of near-identical candidates.
Input: candidate thumbnails with technical/emotion/composition sub-scores, the chapter, and anonymised roles.
Task: choose the best index and justify it in short editorial reasons.
Rules:
- Prefer open eyes on the primary subjects unless the moment justifies closed eyes (kiss, prayer, tears).
- Prefer peak expression and cleaner framing; ignore small exposure differences (they will be corrected later).
- Never comment on appearance or attractiveness.
- If truly equivalent, say so and return the first index with low confidence.
- Return ONLY JSON matching the schema.
```

Required JSON response schema (validated; invalid = retry once, then fall back to local model):

```json
{
  "type": "object",
  "required": ["best_index", "confidence", "reasons"],
  "properties": {
    "best_index": { "type": "integer", "minimum": 0 },
    "equivalent": { "type": "boolean" },
    "confidence": { "type": "number", "minimum": 0, "maximum": 1 },
    "reasons": { "type": "array", "items": { "type": "string" }, "maxItems": 5 }
  },
  "additionalProperties": false
}
```

## 8. Implementation order (execute literally, in this order)

1. Author `cull_weights.toml` and `coverage_rules.toml` with the photographer consultant; PM signs off before code.
2. Implement score fusion with vetoes and promotions; fit per-scene calibration on labelled keepers.
3. Implement the moment pass with diversity-driven k and peak protection.
4. Implement chapter quotas and the greedy + local-search allocation.
5. Implement the Coverage Guard with declarative rules and `CoveredWeak`/`Missing` semantics.
6. Implement identity coverage and the diversity pass.
7. Train the gallery-size regression on real delivered galleries; wire the live slider.
8. Implement the three modes, determinism hashing and full rejection reasoning.
9. Add the optional cloud tie-breaker.
10. Build the culling UI: selection grid, size slider, coverage panel, rejection reasons, one-click override.
11. Run the photographer agreement study and the zero-missed-must-have gate.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `PM` | Product Manager Agent | Own must-have rules, mode definitions and gallery-size policy; run the consultant sessions | Approved rule set | 4 d |
| `MLL` | ML Lead - Vision | Own fusion maths, calibration, veto policy and the agreement evaluation design | Signed spec + gates | 4 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Implement fusion, moment/chapter passes, coverage guard, diversity, sizing, modes, determinism | `aura-cull` + tests | 10 d |
| `MLR` | ML Research Engineer | Tune k-selection, quota formula, local search, and the size regression | Tuning report | 5 d |
| `AGT` | AI Agent & Prompt Engineer | Cloud tie-breaker task, batching, caching, cassettes | Cloud path live | 2 d |
| `DATA` | Data Engineer / Dataset Curator | Human keeper labels for 8 weddings + 60 real delivered galleries for size regression | Labels + dataset | 9 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Culling view, size slider with instant re-selection, coverage panel, override actions | Cull UI | 6 d |
| `MFE` | Mid-Level Frontend Engineer | Rejection-reason drawer, runner-up compare, mode switcher, warning banners | UI panels | 4 d |
| `QAL` | QA Lead - Automation | Agreement harness, zero-missed-must-have gate, determinism test, slider performance test | CI gates | 5 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Blind study: photographers cull 4 weddings manually, compare against engine output | Study report | 6 d |
| `PERF` | Performance Engineer | Ensure full cull (analysis + selection) of 3,000 images stays under 8 minutes | Benchmark report | 3 d |
| `EM` | Engineering Manager / Delivery Lead Agent | Coordinate the cross-team integration of Phases 05-11 outputs; unblock contract mismatches | Integration log | 3 d |
| `DOC` | Technical Writer | Write 'How AURA culls', the coverage guarantee page and the mode guide | Docs merged | 3 d |

### 9.1 Handoff chain for this phase

```text
PM rules + MLL fusion spec -> SRC engine -> MLR tuning
                                  |                |
                                  v                v
                        AGT tie-breaker      DATA keeper labels
                                  |
                    SFE/MFE cull UI -> QAIQ blind study -> PERF budget -> CTO/PM release gate
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

- Photographer agreement on keepers >= 0.85 (Jaccard on moment level) across 4 blind-study weddings.
- Zero missed must-haves on all fixtures whenever candidates exist; `Missing` reported honestly when they do not.
- Every close-family identity appears >= 3 times in every fixture gallery.
- Determinism: identical inputs produce identical `deterministic_hash` and identical selection across runs and machines.
- Size slider from 500 to 1,200 re-selects in <= 2 s and never breaks coverage.
- Aggressive mode still satisfies all coverage rules.
- Every rejected frame has at least one human-readable reason; every kept frame has a runner-up where one exists.

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
| Selection passes on 4,000 analysed images | <= 1.5 s |
| Full analysis + cull, 3,000 images (RTX 4070) | < 8 min |
| Full analysis + cull, 3,000 images (M3 Pro) | < 14 min |
| Slider re-selection | <= 2 s |

Telemetry events (local-first, opt-in aggregation):

- `cull.completed` {images, selected, target, mode, ms, coverage_warnings}
- `cull.veto` {reason_code, count}
- `cull.coverage_weak` {rule, count}
- `cull.user_override` {action: keep|reject, keep_score, reason_codes}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| A missed must-have moment loses the customer | Hard coverage constraints that run last, forced weak coverage with warnings, blind-study gate, and a permanent CI test. |
| Over-culling makes galleries feel thin | Size regression trained on real deliveries, conservative default mode, and an instant slider so photographers can adjust without re-analysis. |
| Weight tuning becomes a black art | All weights in signed config with rationale, agreement harness re-run on every change, and calibration versioning. |
| Users distrust automation | Full rejection reasons, runner-up comparison, one-click override that is remembered by the learning loop (Phase 30). |
| Performance regression as phases add analysis | PERF owns a standing 8-minute budget test in CI on the reference machine. |

## 13. Acceptance criteria

- [ ] Clicking Cull on a 3,000-image wedding produces a complete gallery in under 8 minutes on the reference machine.
- [ ] The coverage panel shows every must-have as covered, weakly covered or genuinely missing.
- [ ] Every decision is explained, and every keeper offers a runner-up to compare.
- [ ] Moving the size slider instantly re-selects without breaking coverage.
- [ ] Two runs on two machines with the same inputs produce byte-identical selections.
- [ ] Blind-study agreement meets the gate and the report is archived.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 12 - Autonomous Culling Engine, Story Coverage Guard & Gallery Sizing.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-12-CULLING-ENGINE-COVERAGE.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - One decision engine turns 3,000 frames into a delivered gallery: per-moment winners, scene quotas, coverage guarantees, duplicate suppression and an automatically chosen gallery size.

Rules:
  - Do not start Phase 13. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-cull/src/{lib,fusion,moment_pass,chapter_pass,coverage,diversity,sizing,modes,explain}.rs`, `config/cull_weights.toml` + `config/coverage_rules.toml`, `crates/aura-catalog/migrations/0012_selection.sql`, `apps/desktop/src/routes/cull/{CullView,SizeSlider,CoveragePanel,RejectReasons}.tsx`, `ml/eval/cull_agreement.py`, `tests/fixtures/labels/keepers_*.json`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-12-culling-engine-coverage and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-12.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-12-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-12-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-12-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 12 of 30 - Autonomous Culling Engine, Story Coverage Guard & Gallery Sizing - part of the AURA Wedding AI master build plan.*
