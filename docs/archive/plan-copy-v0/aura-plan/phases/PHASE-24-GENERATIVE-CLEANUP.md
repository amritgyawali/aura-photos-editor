# Phase 24 - Generative Cleanup & Distraction Removal (safe by construction)

> **Single feature shipped by this phase:** Distracting objects, background clutter, stray limbs, signage, bins, cables and photobombing strangers are removed automatically - but only where removal is safe, small and defensible.
>
> **Mission:** Give photographers the Photoshop cleanup pass they never have time for, with hard safety rules that prevent the product from ever inventing wedding content.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 24 of 30 |
| Epic | E4 - Retouch & Restoration |
| Feature | Distracting objects, background clutter, stray limbs, signage, bins, cables and photobombing strangers are removed automatically - but only where removal is safe, small and defensible. |
| Depends on | Phases 11, 18, 22 |
| Unlocks | Phases 27, 28 |
| Duration | 3 weeks |
| Primary owners | ML Lead - Vision, Senior ML Engineer, Senior Engineer - GPU & Render (Rust / wgpu / CUDA), Security & Privacy Engineer |
| Risk level | High - generative output is the easiest way to destroy trust |
| Headline KPI | artefact-free rate >= 98 % on approved removals; zero removals touching a person's face or body of a primary identity; cleanup <= 3 s per region at full resolution |
| Competitor being beaten | Lightroom Generative Remove; Photoshop Generative Fill; Evoto background tools |

## 1. Why this phase exists

Distraction removal is one of the last manual tasks left after culling and editing. Automating the small, safe 80 % (exit signs, tape on floors, water bottles, cables, background bins) removes real hours from a wedding delivery.

Generative tools fail publicly and embarrassingly. Making safety structural - size limits, semantic denylists, identity protection, confidence gating and mandatory disclosure - is the only responsible way to ship this, and it becomes a marketing advantage over competitors who let users generate anything.

## 2. Scope contract

### 2.1 In scope

- Distraction detection: learned detector for common wedding distractions (exit signs, bins, cables, gaffer tape, water bottles, chairs at frame edge, phone screens, stray hands, background strangers), plus saliency-based 'unexplained attention' detection from Phase 11.
- Removal engine: two tiers - (1) classical content-aware fill / patch synthesis for small, textured regions; (2) local diffusion inpainting for larger regions, run locally when a model pack is installed or via the Phase 04 cloud path with explicit consent.
- Safety engine (the core of this phase): size caps (region <= 4 % of frame by default), semantic denylist (never inside faces, hands, dresses, rings, cake, or any primary identity's body), identity protection, structure protection (no removal that requires inventing architecture across a long span), and confidence gating.
- Cross-frame source preference: where a sibling frame in the same moment shows the same background without the distraction, borrow real pixels instead of generating (always preferred, always disclosed).
- Artefact self-check: run a detector over the result to catch typical inpainting failures (repeated texture, warped lines, ghost limbs) and revert automatically on failure.
- Human-in-the-loop by default: proposals are shown as a review queue with before/after; Zero-Touch mode may auto-apply only tier-1 (classical) removals above a high confidence.
- Disclosure: every generated or borrowed region recorded in the recipe, the ledger, the Explain panel and the delivery report.

### 2.2 Explicitly out of scope (do not build it here)

- Adding content that never existed (sky replacement, new people, new decor) - forbidden by policy.
- Face or expression swapping (forbidden, see Phase 21 ethics).
- Removing guests the client dislikes - a human decision, offered only as a manual tool with explicit confirmation.

## 3. Architecture and data flow

```text
masks (P18) + composition flags (P11) + moment siblings (P08)
     |
  DistractionDetector -> candidates { box, class, salience, removable_prob }
     |
  SAFETY ENGINE: size cap | semantic denylist | identity protect | structure check | confidence
     |            (fail -> discard candidate, record reason)
     v
  source selection:  sibling-frame borrow (preferred)  |  classical fill  |  diffusion inpaint
     |
  ArtefactSelfCheck (repeat texture / warped lines / ghost limbs) -> revert on failure
     |
  proposal queue (default) | auto-apply tier-1 only in Zero-Touch
     |
  recipe.cleanup[] + disclosure in ledger, Explain panel and delivery report
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-generative/src/{lib,detect,safety,denylist,borrow,fill,inpaint,selfcheck,queue}.rs` | Cleanup engine and safety. |
| `ml/models/generative/{train_distraction.py,train_artefact.py,eval_cleanup.py}` | Detector and artefact-checker training. |
| `config/cleanup_policy.toml` | Size caps, denylists, autonomy rules - CTO/PM/SEC co-owned. |
| `apps/desktop/src/routes/cleanup/{ProposalQueue,BeforeAfter,ManualRemove}.tsx` | Review queue and manual tool. |
| `docs/generative-policy.md` | Public statement of what AURA will and will not generate. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Cleanup proposal and safety verdict**

```rust
pub struct CleanupProposal {
    pub id: ProposalId, pub image_id: ImageId,
    pub region: Box2, pub class: DistractionClass,
    pub area_frac: f32, pub salience: f32,
    pub method: CleanupMethod,          // BorrowFrom(ImageId) | ClassicalFill | Inpaint{model}
    pub safety: SafetyVerdict,
    pub confidence: f32,
    pub preview: Option<PreviewRef>,
    pub autonomy: Autonomy,             // from Phase 13 policy, raised one band
}

pub struct SafetyVerdict {
    pub allowed: bool,
    pub checks: Vec<(SafetyCheck, bool)>,   // SizeCap, Denylist, IdentityProtect, StructureSpan, Confidence
    pub blocked_reason: Option<String>,
}
```

## 6. Algorithm, model and implementation design

### 6.1 Detection with an explicit vocabulary

- Train a detector on a labelled wedding-distraction vocabulary rather than relying on generic saliency, because 'what is distracting at a wedding' is domain knowledge (a bin is; a candle is not).
- Combine with unexplained-salience regions: high visual attention that is not a subject, not decor and not part of the story.
- Rank candidates by (salience x removability) and cap the number per image (default 3) so cleanup stays a light touch.

### 6.2 Safety engine - structural, not advisory

- Size cap: default 4 % of frame area; larger regions require explicit user action, never automation.
- Semantic denylist: intersect the region with masks for faces, skin, hands, dress, rings, cake, primary identities' bodies; any overlap above 1 % blocks the proposal.
- Structure span check: if the region crosses a long straight architectural line or a repeating pattern boundary, block automation (inpainting warps these predictably).
- Identity protection: a background stranger may be removed only if fully separated from primary subjects, small, and near the frame edge - otherwise it becomes a manual, confirmed action.
- Every blocked proposal records which check failed, which makes the system auditable and teaches users what it will not do.

### 6.3 Source preference: real pixels first

- Search sibling frames in the same moment for the same background region without the distraction; if alignment confidence is high, homography-align and blend real pixels.
- Classical fill for small textured regions (grass, carpet, wall) is preferred over diffusion because it cannot hallucinate structure.
- Diffusion inpainting is the last resort, restricted by all safety checks, and always disclosed.

### 6.4 Self-check and autonomy

- An artefact classifier trained on known-bad inpaints scores the result; failures revert automatically and the proposal is marked 'not safely removable'.
- Autonomy is raised one band relative to Phase 13 defaults: tier-1 classical/borrow removals may auto-apply at >= 0.97 calibrated confidence in Zero-Touch; diffusion always requires review unless the studio explicitly opts in.
- The delivery report lists every cleanup performed, which protects the photographer's relationship with their client.

## 7. Cloud AI usage (bring-your-own API key)

**Judge whether removing a detected object is editorially safe and appropriate for a wedding gallery**

| Aspect | Specification |
|---|---|
| Model class | Vision reasoning tier, temperature 0 |
| Trigger | Only for candidates that pass all mechanical safety checks but have removability confidence between 0.6 and 0.9 |
| Input sent | Cropped region with context (1024 px), the detected class, area fraction, scene label, and the proposed method |
| Cost control | <= 20 calls per wedding; cached; skipped when cloud is off |
| Offline fallback | Do not remove; leave the proposal in the review queue for the user |

System prompt contract:

```text
You are a cautious wedding retouching supervisor reviewing a proposed object removal.
Input: an image region with context, the detected object class, its size, and the scene.
Task: decide whether removing this object is safe and appropriate, or whether it should be left alone.
Rules:
- Say NO if the object is part of the wedding story (decor, ritual items, gifts, cake, signage naming the couple, guests interacting).
- Say NO if removal would require inventing structure, or if the object overlaps a person.
- Say YES only for genuinely extraneous clutter (bins, cables, tape, bottles, stands, unrelated signage) that is clearly not part of the event.
- When uncertain, say NO. Leaving a distraction is always better than damaging a photograph.
- Return ONLY JSON matching the schema.
```

Required JSON response schema (validated; invalid = retry once, then fall back to local model):

```json
{
  "type": "object",
  "required": ["remove", "confidence", "reasons"],
  "properties": {
    "remove": { "type": "boolean" },
    "story_relevant": { "type": "boolean" },
    "confidence": { "type": "number", "minimum": 0, "maximum": 1 },
    "reasons": { "type": "array", "items": { "type": "string" }, "maxItems": 4 }
  },
  "additionalProperties": false
}
```

## 8. Implementation order (execute literally, in this order)

1. Publish `docs/generative-policy.md`; CTO, PM and SEC co-sign before any code.
2. Label the wedding-distraction vocabulary and train the detector.
3. Implement the safety engine first, with tests, before any removal code exists.
4. Implement sibling-frame borrowing with homography alignment.
5. Implement classical content-aware fill on GPU.
6. Integrate a local diffusion inpainting model pack (optional download) and the cloud path.
7. Train the artefact classifier and wire automatic revert.
8. Implement the proposal queue, autonomy rules and disclosure records.
9. Add the cloud editorial-judgement task for mid-confidence candidates.
10. Run the adversarial safety audit: attempt to make the system damage a photograph.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `CTO` | Chief Architect / CTO Agent | Co-sign the generative policy; own the rule that AURA never adds wedding content | Signed policy | 1 d |
| `MLL` | ML Lead - Vision | Own detector and artefact-classifier design, evaluation and confidence calibration | Signed spec + gates | 4 d |
| `SRML` | Senior ML Engineer | Train distraction detector and artefact classifier; integrate inpainting model pack | Models registered | 10 d |
| `SRG` | Senior Engineer - GPU & Render (Rust / wgpu / CUDA) | GPU classical fill, homography alignment, tiled inpainting execution, VRAM safety | GPU cleanup path | 8 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Safety engine, denylist intersection, proposal queue, disclosure records, recipe ops | `aura-generative` + tests | 8 d |
| `SEC` | Security & Privacy Engineer | Adversarial safety review; verify denylist cannot be bypassed; consent flow for cloud inpainting | Security sign-off | 4 d |
| `DATA` | Data Engineer / Dataset Curator | Distraction vocabulary labels on 10k frames; known-bad inpaint set for the artefact classifier | Labels v1 | 9 d |
| `AGT` | AI Agent & Prompt Engineer | Editorial-judgement cloud task with cautious prompt and cassettes | Cloud path live | 2 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Proposal queue with before/after, accept/reject all, manual removal tool | Cleanup UI | 5 d |
| `QAL` | QA Lead - Automation | Safety-bypass tests, artefact-rate gate, denylist coverage, disclosure completeness | CI gates | 5 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Adversarial audit: 300 attempts to induce damage; every success is a release blocker | Audit report | 5 d |
| `PM` | Product Manager Agent | Own `cleanup_policy.toml` defaults and the Zero-Touch autonomy decision | Approved policy | 2 d |
| `PERF` | Performance Engineer | Keep cleanup off the interactive path; budget per region; batch scheduling | Benchmark | 3 d |
| `DOC` | Technical Writer | Publish the generative policy and the disclosure explanation for clients | Docs merged | 2 d |

### 9.1 Handoff chain for this phase

```text
CTO/PM/SEC policy -> SRC safety engine (tests first) -> DATA labels -> SRML models
                                          |
                                          v
                          SRG fill/borrow/inpaint -> AGT editorial judgement
                                          |
              SFE proposal queue -> QAIQ adversarial audit -> SEC sign-off -> release gate
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

- Safety: no proposal overlapping a face, hand, dress, ring or primary identity body is ever allowed (exhaustive fixture sweep).
- Size cap and structure-span checks cannot be bypassed by any code path (property tests).
- Artefact-free rate >= 98 % on approved removals; failures revert automatically.
- Sibling borrowing is preferred whenever available (measured on fixtures).
- Every applied cleanup appears in the recipe, the ledger and the delivery report.
- With cloud disabled and no model pack, tier-1 cleanup still works and tier-2 is cleanly unavailable.
- Adversarial audit produces zero damaged photographs.

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
| Classical fill per region (45 MP) | <= 400 ms |
| Sibling borrow per region | <= 700 ms |
| Diffusion inpaint per region (local GPU) | <= 3 s |
| Detection per image | <= 45 ms |
| Cleanup share of a 1,000-image export | <= 12 min |

Telemetry events (local-first, opt-in aggregation):

- `cleanup.proposed` {class, area_frac, method, confidence}
- `cleanup.blocked` {check, class}
- `cleanup.applied` {method, count, ms}
- `cleanup.reverted` {artefact_reason, count}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Generative artefacts in a delivered gallery | Artefact self-check with automatic revert, review-by-default, tier preference for real pixels, and a 98 % artefact-free gate. |
| Removing something meaningful to the couple | Story-relevance denylist, cautious cloud judgement, conservative size caps, and full disclosure so the photographer can check. |
| Safety bypass through a new code path | Single choke-point API, property tests, SEC adversarial review, and a lint forbidding direct calls to fill/inpaint. |
| Model pack size and licensing | Optional download, signed manifests, and a legal review of model licences before shipping. |

## 13. Acceptance criteria

- [ ] Common wedding distractions are detected and proposed for removal with previews.
- [ ] Nothing overlapping people, dresses, rings or cake can ever be auto-removed.
- [ ] Real pixels from sibling frames are preferred over generated pixels.
- [ ] Failed inpaints revert themselves before the user ever sees them.
- [ ] Every cleanup is disclosed in the recipe and the delivery report.
- [ ] An adversarial audit cannot make the system damage a photograph.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 24 - Generative Cleanup & Distraction Removal (safe by construction).

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-24-GENERATIVE-CLEANUP.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - Distracting objects, background clutter, stray limbs, signage, bins, cables and photobombing strangers are removed automatically - but only where removal is safe, small and defensible.

Rules:
  - Do not start Phase 25. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-generative/src/{lib,detect,safety,denylist,borrow,fill,inpaint,selfcheck,queue}.rs`, `ml/models/generative/{train_distraction.py,train_artefact.py,eval_cleanup.py}`, `config/cleanup_policy.toml`, `apps/desktop/src/routes/cleanup/{ProposalQueue,BeforeAfter,ManualRemove}.tsx`, `docs/generative-policy.md`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-24-generative-cleanup and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-24.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-24-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-24-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-24-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 24 of 30 - Generative Cleanup & Distraction Removal (safe by construction) - part of the AURA Wedding AI master build plan.*
