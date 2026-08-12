# Phase 08 - Smart Burst Grouping & Duplicate Detection

> **Single feature shipped by this phase:** Sequential and visually similar frames collapse into burst groups, and near-identical duplicates are identified so only the best frame from each moment competes for the gallery.
>
> **Mission:** Turn 3,000 loose files into roughly 700-1,100 'moments'. Every later decision - culling, QC, album - operates on moments, which is both faster and far more human.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 08 of 30 |
| Epic | E2 - Wedding Brain |
| Feature | Sequential and visually similar frames collapse into burst groups, and near-identical duplicates are identified so only the best frame from each moment competes for the gallery. |
| Depends on | Phases 05, 06, 07 |
| Unlocks | Phases 09-13, 27, 29 |
| Duration | 1.5 weeks |
| Primary owners | ML Lead - Vision, Senior Engineer - Core Pipeline (Rust), ML Research Engineer |
| Risk level | Medium |
| Headline KPI | burst grouping ARI >= 0.90 vs human grouping; duplicate recall >= 0.98 / precision >= 0.95; grouping of 4,000 images <= 6 s |
| Competitor being beaten | Aftershoot and Narrative Select burst grouping |

## 1. Why this phase exists

Photographers shoot in bursts: 6 frames of the kiss, 14 of the bouquet toss, 40 of the dance floor. Choosing the best of each burst is the single highest-value culling decision, and it is only possible if bursts are grouped correctly.

Grouping also protects the story: rejecting a burst is a moment lost, whereas rejecting individual frames is just tidying. Phase 12's coverage guarantees depend on moments, not files.

## 2. Scope contract

### 2.1 In scope

- Multi-signal grouping: time proximity (adaptive), embedding similarity, dHash distance, face-identity overlap, camera identity, drive-mode/sub-second EXIF evidence.
- Adaptive time window: derived from local shooting cadence per camera (median inter-frame interval in a 60 s neighbourhood), so a 10 fps burst and a slow ceremony are both handled.
- Two-tier structure: `moment` (a shot of the same subject/action) inside `segment` (a chapter), with `burst` as the tightest sub-group.
- Duplicate classes: `identical` (same file re-imported), `near_identical` (dHash <= 4 and embedding distance <= 0.03), `variant` (same moment, meaningful difference).
- Cross-camera moment merging: the same instant shot by two photographers becomes one moment with per-camera sub-groups (critical for Phase 26).
- Group-level statistics: size, duration, dominant identities, best-so-far pointer, and per-group diversity score.
- UI: moment view (stacked cells with a count badge), expand/collapse, manual split/merge with locking.

### 2.2 Explicitly out of scope (do not build it here)

- Choosing the winner of a burst (Phase 12; this phase only provides candidates and diversity).
- Quality scoring (Phase 09) and expression scoring (Phase 10).
- Deleting anything - grouping never removes files.

## 3. Architecture and data flow

```text
images ordered by timeline_ts (per camera)
     |
     v
  CadenceEstimator --> adaptive window w(t)
     |
     v
  candidate edges: |dt| < w(t)  AND  (embed_dist < t1 OR dhash_dist < t2)
     |
     +-- identity overlap boost / scene mismatch penalty / camera-aware weighting
     v
  union-find graph -> bursts -> moments -> attached to segments (P07)
     |
     +--> duplicate classifier (identical | near_identical | variant)
     |
     v
  moments table (size, duration, identities, diversity, locked)
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-brain-wedding/src/moments/{cadence,graph,burst,moment,duplicate,merge,api}.rs` | Grouping engine. |
| `crates/aura-catalog/migrations/0008_moments.sql` | `moments`, `moment_images`, `duplicates` tables. |
| `ml/eval/burst_eval.py` | ARI/NMI evaluation against human grouping labels. |
| `apps/desktop/src/components/grid/MomentStack.tsx` | Stacked moment cells, expand/collapse, split/merge. |
| `tests/fixtures/labels/bursts_*.json` | Human burst-grouping ground truth. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Moment model (frozen)**

```rust
pub struct Moment {
    pub id: MomentId, pub segment_id: SegmentId,
    pub image_ids: Vec<ImageId>,          // ordered by timeline_ts
    pub bursts: Vec<Vec<ImageId>>,        // tighter sub-groups
    pub start_ts: Timestamp, pub end_ts: Timestamp,
    pub cameras: Vec<CameraId>,
    pub identities: Vec<IdentityId>,
    pub diversity: f32,                   // 0 = all identical, 1 = very different
    pub duplicate_sets: Vec<DuplicateSet>,
    pub user_locked: bool,
}

pub struct DuplicateSet {
    pub kind: DuplicateKind,              // Identical | NearIdentical | Variant
    pub image_ids: Vec<ImageId>,
    pub keep_hint: ImageId,               // best technical frame, not the final decision
    pub confidence: f32,
}
```

## 6. Algorithm, model and implementation design

### 6.1 Adaptive cadence estimation

- For each camera, compute the rolling median inter-frame interval over a 60 s window; the burst window is `max(0.7 s, 2.5 x median)` clamped to 8 s.
- Sub-second EXIF and drive-mode tags, where present, mark true camera bursts explicitly - use them as strong evidence rather than inferring.
- Long gaps (> 20 s) break groups regardless of similarity, because the same pose 30 seconds later is usually a new attempt at a moment, not the same moment.

### 6.2 Graph construction and grouping

- Build a sparse similarity graph with time-windowed kNN from Phase 05 (never all-pairs), then union-find over edges that pass a scene-aware threshold.
- Edge score = 0.55 * (1 - embed_dist) + 0.2 * (1 - dhash_norm) + 0.15 * identity_overlap + 0.1 * same_camera; thresholds come from the scene profile (`dance_floor` needs looser thresholds than `family_portrait`).
- Split over-large groups (> 60 frames) by re-clustering with a stricter threshold so a 5-minute dance sequence does not become one moment.
- Cross-camera merge: two moments from different cameras merge when their time ranges overlap by > 60 % and their embedding medoids are within 0.12.

### 6.3 Duplicate classification

- `identical`: same content hash (re-import) - handled at ingest but reported here for completeness.
- `near_identical`: dHash Hamming <= 4 and embedding distance <= 0.03 and face-box IoU >= 0.9 -> only one frame should ever reach the gallery.
- `variant`: same moment but a meaningful difference (expression, eyes, framing) -> all frames stay eligible and Phase 12 chooses.
- `keep_hint` is provisional: the technically strongest frame by edge energy and subject focus, replaced by the real decision in Phase 12.

### 6.4 Diversity score (feeds album and social selection later)

- Diversity = mean pairwise embedding distance within the moment, normalised; low diversity means the photographer bracketed a static subject, high diversity means the action evolved.
- High-diversity moments are allowed more than one keeper in Phase 12; low-diversity moments are capped at one.

## 7. Cloud AI usage (bring-your-own API key)

No cloud AI call in this phase. The phase must work with the network cable unplugged; the Cloud AI Gateway from Phase 04 stays idle here.

## 8. Implementation order (execute literally, in this order)

1. Implement cadence estimation with tests on real burst fixtures.
2. Implement the time-windowed similarity graph and union-find grouping.
3. Add scene-aware thresholds from Phase 07 profiles and the over-large-group split pass.
4. Implement duplicate classification with the three classes and confidence.
5. Implement cross-camera merging and per-camera sub-groups.
6. Compute diversity and group statistics; persist moments.
7. Build the moment stack UI with expand/collapse and manual split/merge.
8. Evaluate against human grouping labels; tune until the ARI gate passes.
9. Add regression fixtures: 10 fps bouquet toss, slow ceremony, dance floor, two-shooter overlap.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `MLL` | ML Lead - Vision | Define grouping objective, evaluation metric and acceptance gates | Spec + gates | 1 d |
| `MLR` | ML Research Engineer | Tune edge weights and thresholds per scene; ablate each signal's contribution | Tuning report | 3 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Implement cadence, graph, union-find, splitting, duplicates, cross-camera merge, persistence | `moments` module + tests | 6 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Manual split/merge with locking and undo | Editing API | 2 d |
| `DATA` | Data Engineer / Dataset Curator | Human burst-grouping ground truth on fixtures plus adversarial cases | Labels v1 | 3 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Moment stack cells, count badges, expand/collapse, keyboard flow | Moment UI | 3 d |
| `MFE` | Mid-Level Frontend Engineer | Duplicate review panel with side-by-side comparison and 'keep this one' action | UI panel | 2 d |
| `QAL` | QA Lead - Automation | ARI/precision/recall gates, adversarial fixtures, performance test, lock persistence | CI gates | 3 d |
| `PERF` | Performance Engineer | Ensure grouping stays linear-ish; profile graph construction on 6,000 images | Benchmark | 1 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Eyeball 500 groups: any group mixing two different moments is a bug | Audit report | 2 d |
| `DOC` | Technical Writer | Explain moments vs bursts vs duplicates in the help centre | Docs merged | 1 d |

### 9.1 Handoff chain for this phase

```text
DATA labels -> MLR tuning -> SRC implementation -> SFE/MFE UI
                                     |
                        QAL gates + QAIQ audit + PERF profile -> MLL gate
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

- ARI >= 0.90 against human grouping on all fixtures; no group mixes two labelled moments.
- Near-identical detection: recall >= 0.98 at precision >= 0.95, including one-stop-apart exposures.
- 10 fps burst stays one moment; a 5-minute dance sequence becomes multiple moments.
- Two cameras shooting the same kiss produce one moment with two sub-groups.
- Manual split/merge persists through re-analysis; undo restores exactly.
- 4,000 images grouped in <= 6 s with < 200 MB extra memory.

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
| Grouping 4,000 images | <= 6 s |
| Grouping 12,000 images | <= 25 s |
| Extra storage per image | <= 200 B |
| Moment UI expand/collapse | <= 60 ms |

Telemetry events (local-first, opt-in aggregation):

- `moments.built` {images, moments, bursts, mean_size, ms}
- `duplicates.found` {identical, near_identical, variant}
- `moments.user_edit` {action, moment_size}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Merging two different moments hides a keeper | Conservative thresholds, over-large-group splitting, QA audit gate, and one-click split in the UI. |
| Dance floor produces giant groups | Scene-aware thresholds plus a hard size cap with re-clustering. |
| Cross-camera merge misfires on similar scenes | Require strong temporal overlap and medoid proximity; keep sub-groups separable so a bad merge is recoverable. |
| Duplicate deletion anxiety | Nothing is deleted; duplicates are marked and reviewable, and the user can always see all frames. |

## 13. Acceptance criteria

- [ ] The grid can switch between 'all frames' and 'moments' views, with correct counts.
- [ ] Bursts are grouped the way a photographer would group them on the audit set.
- [ ] Near-identical frames are marked with a confidence and reviewable side by side.
- [ ] The same instant captured by two shooters appears as one moment.
- [ ] Manual grouping edits are permanent and undoable.
- [ ] Grouping 4,000 images takes seconds, not minutes.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 08 - Smart Burst Grouping & Duplicate Detection.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-08-BURST-GROUPING-DUPLICATES.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - Sequential and visually similar frames collapse into burst groups, and near-identical duplicates are identified so only the best frame from each moment competes for the gallery.

Rules:
  - Do not start Phase 9. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-brain-wedding/src/moments/{cadence,graph,burst,moment,duplicate,merge,api}.rs`, `crates/aura-catalog/migrations/0008_moments.sql`, `ml/eval/burst_eval.py`, `apps/desktop/src/components/grid/MomentStack.tsx`, `tests/fixtures/labels/bursts_*.json`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-08-burst-grouping-duplicates and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-08.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-08-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-08-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-08-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 08 of 30 - Smart Burst Grouping & Duplicate Detection - part of the AURA Wedding AI master build plan.*
