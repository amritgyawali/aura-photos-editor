# Phase 13 - Explain My Edit, Confidence Calibration & Decision Ledger

> **Single feature shipped by this phase:** Every decision in the app can be opened up: why this frame was kept, why it was edited this way, how confident the system is, what evidence was used - and every decision is recorded in an auditable ledger.
>
> **Mission:** Convert automation into trust. Calibrated confidence is also the mechanism that makes Zero-Touch mode safe, because autonomy thresholds are only meaningful if the numbers are honest.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 13 of 30 |
| Epic | E2 - Wedding Brain |
| Feature | Every decision in the app can be opened up: why this frame was kept, why it was edited this way, how confident the system is, what evidence was used - and every decision is recorded in an auditable ledger. |
| Depends on | Phases 04, 09-12 |
| Unlocks | Phases 27, 28, 30 |
| Duration | 2 weeks |
| Primary owners | ML Lead - Vision, AI Agent & Prompt Engineer, Senior Frontend Engineer (Tauri + React), Senior Engineer - Core Pipeline (Rust) |
| Risk level | Medium - but critical for adoption |
| Headline KPI | expected calibration error <= 0.05; explanation available for 100 % of decisions; ledger replay reproduces any decision exactly |
| Competitor being beaten | FilterPixel score-and-reason explanations; nobody explains editing decisions |

## 1. Why this phase exists

Photographers will not hand over a client's wedding to a black box. An explanation that shows the actual crop of the soft eye, the reference frames used for a colour correction and the alternative that nearly won converts scepticism into confidence.

Calibration is a safety mechanism, not a nicety: the confidence bands that drive autonomy (98-100 % auto-approve, 90-98 % Zero-Touch, 75-90 % optional review, < 75 % review) are only defensible if 90 % confidence really means 90 % correct.

A decision ledger makes support and improvement possible: any complaint becomes reproducible, and the learning loop in Phase 30 needs the record of what was decided and what the user changed.

## 2. Scope contract

### 2.1 In scope

- Unified `Reason` and `Decision` model used by every phase: code, human text, weight, evidence (crop rect, reference frame ids, parameter deltas), source (local/cloud/user), confidence.
- Decision ledger: append-only table recording every automated decision with inputs hash, config versions, model versions, outputs, confidence and timing.
- Confidence calibration service: per-decision-type isotonic/temperature calibration fitted on labelled outcomes, with an ECE/Brier dashboard and a CI gate.
- Autonomy bands and the policy engine that maps calibrated confidence to `auto_apply`, `auto_apply_zero_touch`, `suggest_review`, `require_review`.
- Explain UI: a single panel with tabs (Selection, Technical, Emotion, Composition, Edit, QC) showing reasons with visual evidence, alternatives and the exact parameters applied.
- Natural-language summaries generated from structured evidence (cloud when available, deterministic templates otherwise) - never invented reasoning.
- Replay tooling: `aura replay <decision_id>` re-runs a decision from the ledger and asserts an identical outcome.
- Support bundle export: anonymised ledger slice + config + model versions, with no image pixels unless the user opts in.

### 2.2 Explicitly out of scope (do not build it here)

- The QC agent itself (Phase 27 writes to this ledger).
- Learning from user corrections (Phase 30 reads this ledger).
- Telemetry upload infrastructure (opt-in, defined in Phase 30).

## 3. Architecture and data flow

```text
every phase --> Decision { type, inputs_hash, outputs, reasons[], raw_confidence, versions }
                        |
                        v
           CalibrationService (per decision type) --> calibrated_confidence
                        |
                        v
                AutonomyPolicy --> auto | auto_zero_touch | suggest | require_review
                        |
            +-----------+------------------------------+
            v                                          v
     DecisionLedger (append-only, replayable)     Explain UI (tabs + evidence)
            |                                          |
     aura replay <id> (exact reproduction)      NL summary (cloud or template)
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-explain/src/{lib,reason,decision,ledger,calibration,policy,summary,replay,bundle}.rs` | Explainability core. |
| `crates/aura-catalog/migrations/0013_ledger.sql` | `decisions`, `decision_reasons`, `calibration_models` tables. |
| `config/autonomy_bands.toml` | Confidence thresholds per decision type, PM-owned. |
| `apps/desktop/src/components/explain/{ExplainPanel,ReasonRow,EvidenceCrop,AlternativeCompare}.tsx` | Explain UI. |
| `ml/eval/calibration_report.py` | ECE/Brier/reliability diagrams. |
| `tools/aura-cli/src/replay.rs` | Decision replay command. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Decision and reason model (frozen, used by every phase)**

```rust
pub struct Reason {
    pub code: &'static str,        // stable, documented, translatable
    pub text: String,              // human sentence, localisable
    pub weight: f32,               // contribution to the decision
    pub evidence: Evidence,        // Crop(Box2) | Frames(Vec<ImageId>) | Params(Vec<(String,f32)>) | None
}

pub struct Decision {
    pub id: DecisionId, pub kind: DecisionKind,   // Cull | Edit | Retouch | Qc | Curate | Export
    pub subject: DecisionSubject,                 // Image | Moment | Segment | Gallery
    pub inputs_hash: u64,
    pub outputs_json: String,
    pub reasons: Vec<Reason>,
    pub raw_confidence: f32, pub calibrated_confidence: f32,
    pub autonomy: Autonomy,                       // Auto | AutoZeroTouch | Suggest | RequireReview
    pub source: Source, pub model_versions: Vec<(String, u16)>,
    pub config_versions: Vec<(String, u16)>,
    pub ms: u32, pub created_at: Timestamp,
}

pub trait Explainable { fn decision(&self) -> Decision; }
```

## 6. Algorithm, model and implementation design

### 6.1 Calibration methodology

- For each decision type, collect (raw_confidence, correct?) pairs from labelled fixtures plus user overrides, then fit isotonic regression (or temperature scaling where monotonic and data-poor).
- Report ECE, Brier score and reliability diagrams; CI fails if ECE > 0.05 for any decision type with >= 500 samples.
- Cloud-sourced decisions are calibrated separately from local ones, because their error profile differs.
- Calibration models are versioned artefacts shipped with the app and refreshed with each model release.

### 6.2 Explanations that are grounded, not generated

- Reasons are always produced by the deciding code with real evidence; the language model may only *rephrase* them into prose, never add facts.
- Every reason code has a documented meaning, a severity, and a localisable sentence template - the reason-code reference is a public doc.
- The Edit tab shows literal parameter deltas ('Temperature -610 K, Exposure +0.42 EV') and which masks were applied, matching what the develop engine actually executed.
- The Selection tab shows the runner-up side by side with its score breakdown, which is the single most trust-building screen in the product.

### 6.3 Ledger and replay

- Append-only, never updated; corrections create new decisions that supersede old ones, preserving history.
- `inputs_hash` covers analysis outputs, config versions and model versions, so replay can assert determinism and detect drift after an upgrade.
- Ledger size is bounded (roughly 3-6 KB per image for a full pipeline); compaction keeps the newest decision per subject plus all user overrides.

### 6.4 Autonomy policy

- Bands from `autonomy_bands.toml`: >= 0.98 auto; 0.90-0.98 auto only in Zero-Touch; 0.75-0.90 suggest review; < 0.75 require review.
- Risk multipliers: destructive or irreversible actions (generative fill, replacement of a selected frame) require a higher band than reversible parameter edits.
- Any decision touching a must-have moment is raised one band, because the cost of being wrong is higher.

## 7. Cloud AI usage (bring-your-own API key)

**Turn structured reasons into a short, warm, accurate explanation paragraph**

| Aspect | Specification |
|---|---|
| Model class | Balanced tier text model, temperature 0 |
| Trigger | On demand when the user opens the Explain panel and requests a summary, or in batch for the delivery report |
| Input sent | The structured reason list with codes, weights, parameter deltas and scores - no images |
| Cost control | <= 40 short calls per wedding; cached per decision id |
| Offline fallback | Deterministic sentence templates assembled from reason codes (always available offline) |

System prompt contract:

```text
You explain photo-editing decisions to a professional wedding photographer.
You will receive a structured list of reasons with codes, weights and numeric parameters.
Task: write 2-4 short sentences explaining the decision in the photographer's own vocabulary.
Rules:
- Use ONLY the supplied facts and numbers. Never add a reason that is not in the input. Never invent numbers.
- Be specific: name the scene, the parameters and the alternative frame if provided.
- Neutral professional tone, no marketing language, no apologies.
- Return ONLY JSON matching the schema.
```

Required JSON response schema (validated; invalid = retry once, then fall back to local model):

```json
{
  "type": "object",
  "required": ["summary"],
  "properties": {
    "summary": { "type": "string", "maxLength": 600 },
    "headline": { "type": ["string", "null"], "maxLength": 90 }
  },
  "additionalProperties": false
}
```

## 8. Implementation order (execute literally, in this order)

1. Freeze the `Reason`/`Decision` model and refactor Phases 09-12 to emit it (this is why it lands now, not later).
2. Implement the ledger with append-only semantics and compaction.
3. Build the calibration harness, fit initial models, and publish the reliability report.
4. Implement the autonomy policy engine with risk multipliers.
5. Author the reason-code reference with user-facing text for every code.
6. Build the Explain panel with all tabs, evidence crops and alternative comparison.
7. Add the NL summary cloud task with template fallback.
8. Implement `aura replay` and the anonymised support bundle export.
9. Add CI gates: ECE, 100 % explanation coverage, replay determinism.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `MLL` | ML Lead - Vision | Own calibration methodology, ECE gates and per-type calibration models | Calibration report | 4 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Ledger, compaction, replay, inputs hashing, support bundle | `aura-explain` core | 6 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Refactor Phases 09-12 to emit the unified reason model | Refactor merged | 3 d |
| `AGT` | AI Agent & Prompt Engineer | NL summary task with strict grounding rules, template fallback, cassettes | Summary path live | 2 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Explain panel with tabs, evidence crops, alternative compare, parameter table | Explain UI | 6 d |
| `MFE` | Mid-Level Frontend Engineer | Confidence badges across the grid, review queues per band, 'why?' shortcut | UI integration | 4 d |
| `PM` | Product Manager Agent | Own `autonomy_bands.toml`, approve reason-code wording, define review-queue UX | Approved config + copy | 3 d |
| `QAL` | QA Lead - Automation | ECE gate, explanation-coverage gate, replay determinism test, ledger growth test | CI gates | 4 d |
| `DEVOPS` | DevOps / Release Engineer | Ship calibration artefacts with releases; wire the reliability dashboard | Pipeline update | 2 d |
| `SEC` | Security & Privacy Engineer | Ensure support bundles are anonymised and pixel-free by default | Sign-off | 1 d |
| `DOC` | Technical Writer | Publish the reason-code reference and the 'how confidence works' page | Docs merged | 3 d |

### 9.1 Handoff chain for this phase

```text
MLL calibration -> SRC ledger/policy -> refactor of P09-12 emitters
                                |
                                v
                    SFE/MFE Explain UI <- PM copy approval
                                |
                 QAL gates + DEVOPS artefacts -> CTO gate -> Phases 27/28/30
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

- Every decision produced by any phase has at least one reason and a calibrated confidence (coverage gate = 100 %).
- ECE <= 0.05 per decision type on held-out labelled outcomes; reliability diagrams archived.
- `aura replay` reproduces stored outcomes exactly on the reference fixtures.
- NL summaries contain no numbers or claims absent from the structured input (automated grounding check).
- Ledger stays within the size budget on a 4,000-image project after a full pipeline plus QC.
- Support bundle contains no pixels, no client names and no API keys.

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
| Ledger write per decision | <= 0.4 ms amortised |
| Explain panel open (with crops) | <= 250 ms |
| Ledger size per 1,000 images (full pipeline) | <= 6 MB |
| Replay of one decision | <= 1 s |

Telemetry events (local-first, opt-in aggregation):

- `decision.recorded` {kind, autonomy, calibrated_confidence, ms}
- `explain.opened` {kind, tab}
- `calibration.ece` {kind, ece, samples}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Explanations drift from actual behaviour | Reasons are emitted by the deciding code path only; a lint forbids constructing reasons outside the deciding module, and grounding tests check the NL layer. |
| Overconfident models make Zero-Touch unsafe | Hard ECE gate in CI, conservative default bands, and risk multipliers for irreversible actions. |
| Ledger bloat | Compaction policy, bounded reason counts, and a size test in CI. |
| Explanation overload in the UI | Top-3 reasons by weight shown by default with 'show all', and plain-language codes approved by PM. |

## 13. Acceptance criteria

- [ ] Opening any image shows why it was kept or rejected, with the runner-up and score breakdown.
- [ ] The Edit tab lists the exact parameters and masks that were applied.
- [ ] Confidence badges appear across the app and map to documented autonomy bands.
- [ ] Calibration report is published and ECE gates pass.
- [ ] Any decision can be replayed from the ledger with an identical result.
- [ ] A support bundle can be exported that contains no client imagery.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 13 - Explain My Edit, Confidence Calibration & Decision Ledger.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-13-EXPLAINABILITY-CONFIDENCE-LEDGER.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - Every decision in the app can be opened up: why this frame was kept, why it was edited this way, how confident the system is, what evidence was used - and every decision is recorded in an auditable ledger.

Rules:
  - Do not start Phase 14. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-explain/src/{lib,reason,decision,ledger,calibration,policy,summary,replay,bundle}.rs`, `crates/aura-catalog/migrations/0013_ledger.sql`, `config/autonomy_bands.toml`, `apps/desktop/src/components/explain/{ExplainPanel,ReasonRow,EvidenceCrop,AlternativeCompare}.tsx`, `ml/eval/calibration_report.py`, `tools/aura-cli/src/replay.rs`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-13-explainability-confidence-ledger and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-13.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-13-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-13-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-13-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 13 of 30 - Explain My Edit, Confidence Calibration & Decision Ledger - part of the AURA Wedding AI master build plan.*
