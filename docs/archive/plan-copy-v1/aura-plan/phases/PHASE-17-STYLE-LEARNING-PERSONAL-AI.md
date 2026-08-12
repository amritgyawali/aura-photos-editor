# Phase 17 - Style Learning: Scene-Conditional Personal AI Profiles ("Teach My AI")

> **Single feature shipped by this phase:** Import previously edited weddings and the app learns the photographer's own look - not as one style, but as a scene-conditional style tree (outdoor portrait, indoor ceremony, golden hour, flash, dance floor, details, night).
>
> **Mission:** Beat Imagen's Personal AI Profile by learning *per scene and per lighting condition*, from as few as 300 pairs, with an honest report of what was learned and where the profile is weak.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 17 of 30 |
| Epic | E3 - Photo Brain |
| Feature | Import previously edited weddings and the app learns the photographer's own look - not as one style, but as a scene-conditional style tree (outdoor portrait, indoor ceremony, golden hour, flash, dance floor, details, night). |
| Depends on | Phases 14, 15, 16 |
| Unlocks | Phases 25, 27, 28, 30 |
| Duration | 3 weeks |
| Primary owners | ML Lead - Vision, Senior ML Engineer, MLOps / Model Packaging Engineer, Colour Scientist |
| Risk level | High - the strongest retention feature in the product |
| Headline KPI | style match dE00 <= 2.5 vs the photographer's own edit; usable profile from 300 pairs; training of 2,000 pairs <= 25 min locally |
| Competitor being beaten | Imagen Personal AI Profiles (needs ~2,000 images, one global style) |

## 1. Why this phase exists

A photographer's style is their brand. A tool that renders 'good' edits that are not *their* edits will always be a temporary tool. Style learning is what converts a trial into a subscription.

Scene conditioning is the differentiator: real photographers do not edit a candlelit ceremony the way they edit a golden-hour portrait. One global style profile is the reason existing tools feel almost-right-but-wrong.

Learning locally, from the user's own files, with no upload requirement, is both a privacy advantage and a speed advantage over cloud-only competitors.

## 2. Scope contract

### 2.1 In scope

- Pair ingestion: match RAW originals to exported JPEG/TIFF finals (by content hash, filename stem, capture time, and perceptual matching for renamed files), or read XMP/Lightroom catalogues directly when available.
- Parameter extraction: when XMP exists, read parameters directly; when only a JPEG final exists, *infer* the recipe by fitting the develop engine's parameters to reproduce the final (differentiable-ish optimisation over the Phase 14 pipeline).
- Scene bucketing: every pair is assigned a scene + lighting bucket from Phases 07/15, producing a style tree rather than a single model.
- Style model per bucket: a residual predictor that outputs *deltas* on top of the Phase 15/16 baseline decisions, with shrinkage toward the global profile when a bucket has few samples.
- Hierarchical fallback: bucket -> parent scene group -> global profile -> factory default, so sparse buckets degrade gracefully instead of behaving randomly.
- Profile diagnostics: per-bucket sample counts, confidence, measured match error, and an honest 'weak buckets' report telling the photographer exactly which weddings to add.
- Multi-profile support: several named profiles (personal, light-and-airy client set, dark-and-moody, second shooter), switchable per project and per chapter.
- Local training with progress UI, cancellable, resumable, GPU-accelerated where available; no cloud requirement.
- Profile versioning, export/import (signed `.auraprofile` bundle), and A/B comparison against the previous version before adoption.

### 2.2 Explicitly out of scope (do not build it here)

- Learning from in-app corrections (Phase 30's learning loop, which updates these same profiles).
- Retouch style (Phase 20 has its own strength learning).
- Gallery consistency (Phase 25).

## 3. Architecture and data flow

```text
RAW originals + finals (JPEG/XMP/catalogue)
        |
   PairMatcher (hash / stem / time / perceptual)
        |
   ParameterExtractor:  XMP present? read : fit recipe to reproduce final
        |
   SceneBucketer (P07 scene x P15 lighting) --> buckets[]
        |
   per bucket: ResidualStyleModel (delta on baseline decisions) + shrinkage
        |
   StyleTree { global, groups[], buckets[] } + diagnostics
        |
   inference: baseline (P15/P16) + style delta (bucket -> group -> global) -> recipe
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-style/src/{lib,pairs,extract,fit,bucket,tree,infer,profile,diagnostics}.rs` | Style learning engine. |
| `crates/aura-style/src/fit/optimise.rs` | Recipe fitting against a target final image. |
| `ml/models/style/{train_residual.py,eval_style.py,export.py}` | Residual style model training. |
| `crates/aura-catalog/migrations/0017_style.sql` | `profiles`, `profile_buckets`, `style_pairs` tables. |
| `apps/desktop/src/routes/style/{TeachMyAi,ProfileReport,BucketMatrix,AbCompare}.tsx` | Teach My AI UI. |
| `docs/style-profiles.md` | How style learning works, data requirements, privacy. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Style profile contracts (frozen)**

```rust
pub struct StyleProfile {
    pub id: ProfileId, pub name: String, pub version: u16,
    pub global: StyleDelta,
    pub groups: HashMap<SceneGroup, StyleDelta>,
    pub buckets: HashMap<StyleBucket, BucketModel>,   // (scene, lighting) -> model
    pub diagnostics: ProfileDiagnostics,
    pub trained_pairs: u32, pub trained_at: Timestamp,
    pub engine_ver: String,
}

pub struct StyleDelta {                 // additive deltas on baseline decisions
    pub exposure: f32, pub temperature_k: f32, pub tint: f32,
    pub contrast: f32, pub highlights: f32, pub shadows: f32,
    pub whites: f32, pub blacks: f32,
    pub curve_shift: CurveShift,        // control-point offsets
    pub hsl: HslAdjustments,
    pub vibrance: f32, pub saturation: f32,
    pub skin_bias: SkinBias,            // warm/neutral preference, chroma preference
    pub confidence: f32, pub samples: u32,
}

pub struct ProfileDiagnostics {
    pub per_bucket: Vec<(StyleBucket, u32, f32)>,   // samples, match_de00
    pub weak_buckets: Vec<StyleBucket>,
    pub overall_de00: f32,
    pub recommendation: String,                     // "add 1 outdoor daylight wedding"
}
```

## 6. Algorithm, model and implementation design

### 6.1 Getting ground truth from photographers who only kept JPEGs

- Preferred path: read XMP/Lightroom catalogue parameters directly - exact and free.
- Fallback path: fit the Phase 14 recipe to reproduce the final JPEG. Optimise a small parameter vector (exposure, WB, tone, curve, HSL) by coordinate descent on a perceptual loss (dE00 on a downsampled grid plus histogram matching), typically 60-120 render iterations at 512 px, about 1-2 s per pair on GPU.
- Reject pairs whose fit residual is too high (heavy local retouching, composites, crops) so the style model learns global look, not unmodelled work.
- Report how many pairs were used versus rejected - honesty here prevents 'why doesn't it look like me' support tickets.

### 6.2 Residual learning with shrinkage

- The model predicts *deltas from the baseline*, so it inherits Phase 15/16's correctness and only learns taste. This is why 300 pairs are enough.
- Per bucket, fit a small ridge-regularised model on features (subject luma, CCT, dynamic range, ISO, flash, scene, skin group) predicting each delta.
- James-Stein style shrinkage toward the parent group and global delta, weighted by sample count, so a bucket with 8 samples barely moves and a bucket with 400 dominates.
- Robust fitting (Huber loss) so one wildly different wedding cannot skew the profile.

### 6.3 Honest diagnostics and adoption safety

- After training, re-render a held-out set of the photographer's own pairs and report dE00 per bucket; this is the number shown in the UI, not a vague 'profile ready'.
- A/B compare: side-by-side of old profile, new profile and the photographer's own edit before adoption; adoption is an explicit action.
- `weak_buckets` drives a concrete recommendation ('add one indoor flash reception to improve dance-floor accuracy').

### 6.4 Multi-profile and per-chapter application

- Profiles are selectable per project and per chapter, which supports real studio practice (a moody reception with an airy ceremony) and second-shooter normalisation in Phase 26.
- Profile bundles are signed and portable so a studio can distribute one look to a team.

## 7. Cloud AI usage (bring-your-own API key)

No cloud AI call in this phase. The phase must work with the network cable unplugged; the Cloud AI Gateway from Phase 04 stays idle here.

## 8. Implementation order (execute literally, in this order)

1. Implement pair matching including perceptual matching for renamed exports.
2. Implement XMP/catalogue parameter reading.
3. Implement recipe fitting against final JPEGs with residual rejection.
4. Implement scene/lighting bucketing and the style tree structure.
5. Implement per-bucket robust regression with shrinkage and hierarchical fallback.
6. Implement diagnostics, held-out evaluation and the recommendation engine.
7. Implement profile versioning, signed export/import and A/B comparison.
8. Build the Teach My AI flow: folder pick, progress, report, bucket matrix, adopt.
9. Validate end to end with 5 real photographers' archives; measure dE00 per bucket.
10. Wire style deltas into the Phase 15/16 decision path with provenance in the recipe.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `MLL` | ML Lead - Vision | Own the residual formulation, shrinkage maths, evaluation protocol and adoption gates | Signed spec | 5 d |
| `SRML` | Senior ML Engineer | Implement recipe fitting optimiser and per-bucket model training; GPU acceleration | Training pipeline | 9 d |
| `COL` | Colour Scientist | Define the perceptual loss, validate fitted parameters against known XMP ground truth | Validation report | 4 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Pair matching, XMP reading, profile storage, versioning, signed bundles, inference path | `aura-style` + tests | 8 d |
| `MLOPS` | MLOps / Model Packaging Engineer | Local training orchestration: progress, cancel, resume, artefact management, reproducibility | Training runtime | 4 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Teach My AI wizard, progress UI, profile report, bucket matrix heat-map, A/B compare | Style UI | 6 d |
| `MFE` | Mid-Level Frontend Engineer | Per-project and per-chapter profile selection, multi-profile management | UI panels | 3 d |
| `DATA` | Data Engineer / Dataset Curator | Collect consented archives from 5 photographers across traditions for validation | Validation sets | 6 d |
| `QAL` | QA Lead - Automation | dE00 gates, sparse-bucket fallback tests, pair-matching edge cases, determinism | CI gates | 5 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Blind test: can each photographer distinguish AURA's output from their own edit? | Blind study | 4 d |
| `SEC` | Security & Privacy Engineer | Ensure archives are never uploaded; sign profile bundles; validate import safety | Sign-off | 2 d |
| `PERF` | Performance Engineer | Hit the 25-minute training budget for 2,000 pairs; tune fit iterations | Benchmark | 3 d |
| `DOC` | Technical Writer | Write 'Teach My AI' guide, data requirements and troubleshooting | Docs merged | 3 d |

### 9.1 Handoff chain for this phase

```text
SRC pair matching -> COL loss definition -> SRML fitting + training
                                  |
                                  v
                    MLL shrinkage/eval -> MLOPS local training runtime
                                  |
                       SFE/MFE Teach My AI UI -> QAIQ blind study -> MLL/PM gate
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

- Fitted parameters match known XMP ground truth within tolerance on 200 pairs (validates the JPEG-only path).
- Style match dE00 <= 2.5 on held-out pairs per photographer; <= 3.5 in weak buckets.
- Usable profile from 300 pairs: measurable improvement over factory baseline in every populated bucket.
- Sparse bucket (< 10 samples) falls back to group/global without erratic output.
- One outlier wedding cannot shift the global profile beyond a bounded amount (robustness test).
- Profile export/import round-trips with signature verification; tampered bundles are rejected.
- Training 2,000 pairs completes within budget and is cancellable and resumable.

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
| Recipe fitting per pair (GPU, 512 px) | <= 1.5 s |
| Training 2,000 pairs end to end (RTX 4070) | <= 25 min |
| Training 2,000 pairs (M3 Pro) | <= 45 min |
| Style inference overhead per image | <= 2 ms |
| Profile size | <= 3 MB |

Telemetry events (local-first, opt-in aggregation):

- `style.training` {pairs, accepted, rejected, buckets, ms}
- `style.profile_adopted` {profile, version, overall_de00}
- `style.bucket_fallback` {bucket, fallback_level}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Photographer's archive is messy (renames, crops, composites) | Multiple matching strategies, residual-based rejection, and an honest accepted/rejected report. |
| Learned style bakes in past mistakes | Residual-on-baseline design keeps corrections intact; outlier-robust fitting; A/B comparison before adoption. |
| Sparse buckets behave unpredictably | Shrinkage plus hierarchical fallback plus explicit weak-bucket reporting. |
| Users expect one-click magic from 20 photos | Clear minimum-data guidance in the UI and a profile-strength meter rather than a false 'ready' state. |

## 13. Acceptance criteria

- [ ] Pointing the app at past weddings produces a scene-conditional profile with a per-bucket accuracy report.
- [ ] Edits made with the profile are measurably closer to the photographer's own edits than the factory baseline.
- [ ] Weak buckets are named with a concrete recommendation for what to add.
- [ ] Profiles can be versioned, compared, adopted, exported and shared safely.
- [ ] Training runs locally with progress, cancel and resume, and never uploads imagery.
- [ ] At least three of five validation photographers cannot reliably distinguish AURA's output from their own.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 17 - Style Learning: Scene-Conditional Personal AI Profiles ("Teach My AI").

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-17-STYLE-LEARNING-PERSONAL-AI.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - Import previously edited weddings and the app learns the photographer's own look - not as one style, but as a scene-conditional style tree (outdoor portrait, indoor ceremony, golden hour, flash, dance floor, details, night).

Rules:
  - Do not start Phase 18. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-style/src/{lib,pairs,extract,fit,bucket,tree,infer,profile,diagnostics}.rs`, `crates/aura-style/src/fit/optimise.rs`, `ml/models/style/{train_residual.py,eval_style.py,export.py}`, `crates/aura-catalog/migrations/0017_style.sql`, `apps/desktop/src/routes/style/{TeachMyAi,ProfileReport,BucketMatrix,AbCompare}.tsx`, `docs/style-profiles.md`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-17-style-learning-personal-ai and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-17.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-17-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-17-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-17-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 17 of 30 - Style Learning: Scene-Conditional Personal AI Profiles ("Teach My AI") - part of the AURA Wedding AI master build plan.*
