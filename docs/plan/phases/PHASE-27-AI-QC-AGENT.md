# Phase 27 - AI Quality-Control Agent & Automatic Re-Edit Loop

> **Single feature shipped by this phase:** An autonomous inspector re-examines every edited image before export, writes a diagnosis with confidence, fixes what it can, replaces frames when a better alternative exists, and escalates the rest.
>
> **Mission:** Replicate the senior retoucher who checks the junior's work. This closes the loop that makes Zero-Touch delivery defensible rather than reckless.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 27 of 30 |
| Epic | E5 - Gallery Brain & Autonomy |
| Feature | An autonomous inspector re-examines every edited image before export, writes a diagnosis with confidence, fixes what it can, replaces frames when a better alternative exists, and escalates the rest. |
| Depends on | Phases 13, 15-26 |
| Unlocks | Phases 28, 30 |
| Duration | 3 weeks |
| Primary owners | AI Agent & Prompt Engineer, ML Lead - Vision, QA Lead - Automation, Senior Engineer - Core Pipeline (Rust) |
| Risk level | High - it is the last line of defence |
| Headline KPI | catches >= 90 % of injected defects; auto-fix success >= 85 % of accepted tickets; QC pass <= 90 s per 1,000 images |
| Competitor being beaten | Nobody ships an autonomous QC agent |

## 1. Why this phase exists

Automation without inspection is gambling with a client's wedding. A QC agent converts a pipeline of independent decisions into a system with feedback, which is what makes 'click once and deliver' a responsible promise.

It also produces the artefact photographers want most: a short, honest report of what was checked, what was fixed and what needs their eyes - turning fear of automation into an auditable workflow.

## 2. Scope contract

### 2.1 In scope

- Inspection battery over every selected, edited frame: colour consistency vs node anchors (P25), skin plausibility (P15/25), exposure/clipping regressions (P09/14), sharpness after restoration (P22), retouch artefacts and texture loss (P20/21), mask edge artefacts (P18/19), crop safety (P23), cleanup artefacts (P24), duplicate leakage (P08), coverage integrity (P12).
- Ticket model: each finding is a ticket with image, category, diagnosis text, quantified deviation, confidence, proposed remedy and expected improvement.
- Remedy engine: parameter re-solve (re-run the specific decision with constraints), strength reduction, operation revert, frame replacement from the Phase 12 runner-up, or escalation to human review.
- Bounded re-edit loop: at most 2 remediation rounds per image; each round must measurably improve the ticket's metric or the change is reverted (no thrashing).
- Replacement logic: swap a selected frame for its runner-up only when the runner-up's post-edit metrics are clearly better and coverage remains intact; always recorded with a before/after.
- Agentic reasoning (Phase 04) for triage and remediation planning on complex or multi-symptom images, with tool-calling over read-only inspection APIs and a bounded step count.
- QC report: per-wedding summary (checks run, tickets by category, auto-fixed, replaced, escalated) exportable as PDF/Markdown for studio records.
- Escalation queue in the UI with keyboard-fast review, grouped by category so a photographer can clear 40 tickets in minutes.

### 2.2 Explicitly out of scope (do not build it here)

- Making the original edits (Phases 15-26).
- The Zero-Touch orchestration itself (Phase 28 calls QC as a stage).
- Learning from resolutions (Phase 30 consumes QC outcomes).

## 3. Architecture and data flow

```text
edited gallery
     |
  INSPECTION BATTERY (10 checks, all read-only, all quantified)
     |
  tickets[] { image, category, diagnosis, deviation, confidence, remedy, expected_gain }
     |
  triage: mechanical rules  --(complex/multi-symptom)-->  agentic planner (P04, bounded)
     |
  REMEDY: re-solve param | reduce strength | revert op | replace with runner-up | escalate
     |
  re-inspect (round <= 2): improved? keep : revert
     |
  QC report + escalation queue + ledger entries (P13)
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-qc/src/{lib,checks/*,ticket,triage,remedy,replace,loop,report,queue}.rs` | QC agent. |
| `crates/aura-qc/src/checks/{consistency,skin,exposure,sharpness,retouch,mask,crop,cleanup,duplicate,coverage}.rs` | Individual inspections. |
| `config/qc_thresholds.toml` | Per-check thresholds and remedy policy. |
| `apps/desktop/src/routes/qc/{QcReport,TicketQueue,BeforeAfter,CategoryFilter}.tsx` | QC UI. |
| `tests/qc/injected_defects/` | Synthetic defect corpus for gate testing. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**QC ticket and remedy contracts (frozen)**

```rust
pub struct QcTicket {
    pub id: TicketId, pub image_id: ImageId,
    pub category: QcCategory,          // Consistency | Skin | Exposure | Sharpness | Retouch | Mask | Crop | Cleanup | Duplicate | Coverage
    pub diagnosis: String,             // "bride face 4.2 dE00 magenta vs node anchors #817/#819/#825"
    pub deviation: f32, pub threshold: f32,
    pub evidence: Evidence,
    pub remedy: Remedy,
    pub expected_gain: f32,
    pub confidence: f32, pub autonomy: Autonomy,
    pub round: u8, pub status: TicketStatus,   // Open | Fixed | Reverted | Escalated | Accepted
}

pub enum Remedy {
    ResolveParam { decision: DecisionKind, constraint: String },
    ReduceStrength { op: String, factor: f32 },
    RevertOp { op: String },
    ReplaceFrame { with: ImageId },
    Escalate { note: String },
}

pub struct QcReport {
    pub project: ProjectId,
    pub checks_run: u32, pub images: u32,
    pub by_category: Vec<(QcCategory, u32, u32, u32)>,   // found, fixed, escalated
    pub replacements: Vec<(ImageId, ImageId, String)>,
    pub duration_ms: u64, pub cloud_used: bool,
}
```

## 6. Algorithm, model and implementation design

### 6.1 Inspections must be quantified, not vibes

- Every check outputs a number and a threshold: 'skin 4.2 dE00 vs node anchors, threshold 2.5'. This makes tickets actionable, testable and explainable.
- Checks are read-only and independent so they can run in parallel across the gallery on all cores.
- Thresholds live in `qc_thresholds.toml` per scene class, because a dance floor tolerates more than a family formal.

### 6.2 Triage: mechanical first, agentic only when needed

- Single-symptom tickets with an obvious remedy are handled by deterministic rules - cheap, fast, reproducible.
- Multi-symptom or contradictory cases (soft *and* noisy *and* inconsistent) go to the bounded agentic planner, which may call read-only inspection tools, then must return a plan matching a strict schema.
- The planner never executes anything; it proposes remedies which the mechanical engine validates against policy before applying.

### 6.3 The re-edit loop, without thrashing

- Each remedy application is followed by re-inspection of that ticket's metric only; if the metric does not improve by at least the expected gain margin, the change is reverted and the ticket escalates.
- Maximum 2 rounds per image, global time budget per wedding, and a rule that no remedy may worsen another check by more than a small tolerance (checked by re-running affected checks).
- All rounds are recorded in the ledger so the history of an image's edit is fully reconstructable.

### 6.4 Replacement, the feature photographers will demo

- 'Image #382's face is below the sharpness threshold; #381 in the same moment has eyes open, higher sharpness and a better expression' - replacement uses the Phase 12 runner-up plus post-edit metrics.
- Coverage is re-validated after any replacement so a swap can never break a must-have rule.
- Replacements always require higher confidence than parameter fixes, and are shown side by side in the report.

## 7. Cloud AI usage (bring-your-own API key)

**Diagnose and plan remediation for complex, multi-symptom images**

| Aspect | Specification |
|---|---|
| Model class | Reasoning tier with vision, temperature 0, bounded to 6 tool steps |
| Trigger | Images with >= 3 open tickets, or contradictory tickets, or a failed first remediation round |
| Input sent | Ticket list with quantified deviations, the recipe summary, node anchor statistics, and up to 3 crops (subject, background, comparison anchor) |
| Cost control | <= 40 calls per wedding; batched by image; cached; skipped when cloud is off |
| Offline fallback | Mechanical priority ordering (consistency -> exposure -> skin -> retouch -> sharpness) with single-remedy-per-round and escalation on failure |

System prompt contract:

```text
You are a senior retoucher reviewing an automatically edited wedding photograph that failed several quality checks.
Input: quantified findings, the current edit recipe summary, reference-frame statistics for this scene, and image crops.
Task: produce an ordered remediation plan using ONLY the allowed remedies, or recommend escalation to a human.
Rules:
- Fix root causes before symptoms: if white balance is wrong, do not reduce retouch strength.
- Never propose a remedy that is not in the allowed list. Never invent parameter values outside the stated bounds.
- Prefer the smallest change that resolves the finding. Prefer escalation over a risky fix on a must-have moment.
- Explain each step in one short sentence referencing the specific finding.
- Return ONLY JSON matching the schema.
```

Required JSON response schema (validated; invalid = retry once, then fall back to local model):

```json
{
  "type": "object",
  "required": ["plan", "confidence"],
  "properties": {
    "plan": {
      "type": "array", "maxItems": 4,
      "items": {
        "type": "object",
        "required": ["remedy", "target", "reason"],
        "properties": {
          "remedy": { "type": "string", "enum": ["resolve_param", "reduce_strength", "revert_op", "replace_frame", "escalate"] },
          "target": { "type": "string" },
          "magnitude": { "type": ["number", "null"] },
          "reason": { "type": "string" }
        },
        "additionalProperties": false
      }
    },
    "root_cause": { "type": ["string", "null"] },
    "confidence": { "type": "number", "minimum": 0, "maximum": 1 }
  },
  "additionalProperties": false
}
```

## 8. Implementation order (execute literally, in this order)

1. Build the injected-defect corpus first - QC is tested by its ability to catch known defects.
2. Implement the ten inspections with quantified outputs and parallel execution.
3. Implement the ticket model, thresholds config and ledger integration.
4. Implement mechanical triage and the remedy engine with policy validation.
5. Implement the bounded re-edit loop with improvement verification and revert.
6. Implement replacement with coverage re-validation.
7. Implement the agentic planner with the tool registry and strict schema.
8. Implement the QC report and the escalation queue UI.
9. Run the defect-detection gate and the no-regression gate; tune thresholds.
10. Dogfood on 10 real weddings and measure how many tickets a photographer agrees with.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `QAL` | QA Lead - Automation | Own the injected-defect corpus, detection gates and no-regression methodology | Corpus + gates | 6 d |
| `MLL` | ML Lead - Vision | Own check formulations, thresholds, expected-gain margins and confidence calibration | Signed spec | 5 d |
| `AGT` | AI Agent & Prompt Engineer | Agentic planner: tool registry, bounded loop, schema, policy validation, cassettes | Planner shipped | 7 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Inspections, ticket model, remedy engine, re-edit loop, replacement, report | `aura-qc` + tests | 11 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Parallel execution and time budgets across the gallery | Scheduler | 2 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | QC report view, ticket queue with keyboard flow, before/after, category filters | QC UI | 6 d |
| `MFE` | Mid-Level Frontend Engineer | Escalation review flow, bulk accept/reject, replacement comparison | UI panels | 4 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Agreement study: do photographers agree with QC tickets and fixes on 10 weddings? | Study report | 6 d |
| `PM` | Product Manager Agent | Own `qc_thresholds.toml`, autonomy for each remedy type, and report contents | Approved policy | 3 d |
| `PERF` | Performance Engineer | Hit the 90 s per 1,000 images budget; parallelism and check cost tuning | Benchmark | 3 d |
| `EM` | Engineering Manager / Delivery Lead Agent | Coordinate the many upstream dependencies; run the integration bug bash | Integration log | 3 d |
| `DOC` | Technical Writer | Document every check, threshold and remedy; publish a sample QC report | Docs merged | 3 d |

### 9.1 Handoff chain for this phase

```text
QAL defect corpus -> MLL check specs -> SRC inspections + remedy engine
                                        |
                                        v
                             AGT agentic planner (proposals only)
                                        |
                        SFE/MFE QC UI -> QAIQ agreement study -> PM/CTO gate -> Phase 28
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

- Detection: >= 90 % of injected defects caught, with a documented false-ticket rate <= 8 %.
- Auto-fix: >= 85 % of accepted tickets resolved within 2 rounds and verified by re-inspection.
- No regression: a remedy never worsens another check beyond tolerance (checked automatically).
- Replacement never breaks coverage; every replacement is recorded with a comparison.
- Loop bounds respected: no image exceeds 2 rounds; no thrashing observed on adversarial fixtures.
- Planner output always schema-valid and always policy-validated before execution; cloud-off path fully functional.
- Photographer agreement with tickets >= 80 % in the dogfood study.

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
| QC pass for 1,000 images | <= 90 s |
| Single remediation round per image | <= 1.2 s |
| Report generation | <= 3 s |
| Cloud calls per wedding (default) | <= 40 |

Telemetry events (local-first, opt-in aggregation):

- `qc.run` {images, checks, tickets, fixed, replaced, escalated, ms, cloud_used}
- `qc.ticket` {category, deviation, confidence, remedy, outcome}
- `qc.revert` {category, reason}
- `qc.user_disagree` {category, ticket_id}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| QC introduces new problems while fixing old ones | Improvement verification with automatic revert, no-regression checks, and bounded rounds. |
| Too many false tickets exhaust the user | Calibrated thresholds per scene, false-ticket rate gate, category grouping, and bulk actions. |
| Agentic planner proposes unsafe remedies | Strict allowed-remedy enumeration, policy validation before execution, and planner-cannot-execute architecture. |
| QC becomes a performance bottleneck | Read-only parallel checks, per-wedding time budget, and PERF-owned gate. |

## 13. Acceptance criteria

- [ ] Every edited frame is inspected by ten quantified checks before export.
- [ ] Tickets read like a senior retoucher's notes, with numbers and reference frames.
- [ ] Most problems are fixed automatically and verified; the rest are escalated clearly.
- [ ] Frames are replaced by better alternatives when justified, without breaking coverage.
- [ ] A QC report can be exported for studio records.
- [ ] Photographers agree with the majority of tickets in dogfood testing.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 27 - AI Quality-Control Agent & Automatic Re-Edit Loop.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-27-AI-QC-AGENT.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - An autonomous inspector re-examines every edited image before export, writes a diagnosis with confidence, fixes what it can, replaces frames when a better alternative exists, and escalates the rest.

Rules:
  - Do not start Phase 28. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-qc/src/{lib,checks/*,ticket,triage,remedy,replace,loop,report,queue}.rs`, `crates/aura-qc/src/checks/{consistency,skin,exposure,sharpness,retouch,mask,crop,cleanup,duplicate,coverage}.rs`, `config/qc_thresholds.toml`, `apps/desktop/src/routes/qc/{QcReport,TicketQueue,BeforeAfter,CategoryFilter}.tsx`, `tests/qc/injected_defects/`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-27-ai-qc-agent and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-27.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-27-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-27-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-27-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 27 of 30 - AI Quality-Control Agent & Automatic Re-Edit Loop - part of the AURA Wedding AI master build plan.*
