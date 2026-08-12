# Phase 25 - Gallery Intelligence Engine: Cross-Photo Colour, Skin & Scene Consistency

> **Single feature shipped by this phase:** The app edits the gallery, not the photo: reference frames anchor each scene, and every other frame is normalised toward them so an entire wedding looks like one coherent body of work.
>
> **Mission:** Deliver the capability no competitor has - gallery-level reasoning. This is the difference between 1,000 individually plausible edits and one professionally consistent wedding.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 25 of 30 |
| Epic | E5 - Gallery Brain & Autonomy |
| Feature | The app edits the gallery, not the photo: reference frames anchor each scene, and every other frame is normalised toward them so an entire wedding looks like one coherent body of work. |
| Depends on | Phases 07, 15, 16, 18, 20 |
| Unlocks | Phases 26, 27, 28, 29 |
| Duration | 3 weeks |
| Primary owners | Colour Scientist, ML Lead - Vision, Senior Engineer - Core Pipeline (Rust), Tech Lead - Imaging Core (Rust) |
| Risk level | High - the marquee differentiator |
| Headline KPI | within-scene WB spread reduced >= 60 %; per-identity skin dE00 spread <= 2.0 across the gallery; consistency pass <= 60 s for 1,000 images |
| Competitor being beaten | Nobody ships this; Imagen/Aftershoot edit per image |

## 1. Why this phase exists

Photographers spend a large part of their editing time on *sync and consistency* rather than on individual images. Solving it structurally is the highest-leverage automation in the whole product.

It is also the most visible quality signal to clients: galleries where skin tone and warmth drift from frame to frame look amateur even when every individual frame is fine.

Because it operates on the graph of scenes, identities and cameras built in Phases 06-08, it is genuinely hard for competitors to copy without the same foundation.

## 2. Scope contract

### 2.1 In scope

- Reference frame system: per segment (and per sub-scene node) select 3-5 anchors from Phase 15's candidates using WB confidence, subject exposure quality, primary-identity presence and absence of mixed light.
- Normalisation solver: adjust each frame's WB/exposure/tone toward its scene's anchor statistics, with damping so genuine lighting changes within a scene are preserved (a candle-lit vow inside a bright ceremony must not be flattened).
- Skin consistency: per identity, build a target skin appearance (luminance band + chromaticity) from their best-lit frames and correct deviations across the gallery using identity-scoped masks.
- Scene consistency: harmonise contrast, saturation, black point and grade character within each scene node so sequences read as one look.
- Change-point respect: detect intentional lighting transitions (venue change, sunset, flash on/off) and treat them as new normalisation groups rather than errors.
- Outlier detection: flag frames whose colour/exposure deviates beyond a threshold from their group, with the deviation quantified - this is exactly the Phase 27 QC input.
- Gallery timeline visualisation: strips showing WB, exposure and skin tone across the wedding so drift is visible at a glance, before and after.
- Solver stability: normalisation is idempotent (running twice changes nothing) and bounded (no frame moves more than a documented maximum).

### 2.2 Explicitly out of scope (do not build it here)

- Cross-camera profile matching (Phase 26 - a distinct problem).
- QC remediation and re-editing (Phase 27).
- Curation (Phase 29).

## 3. Architecture and data flow

```text
segments (P07) -> scene nodes (tree: Ceremony > Entrance/Ritual/Couple/Reactions)
     |
  anchor selection per node (3-5 frames: high WB conf, good subject exposure, primary identity)
     |
  node statistics: target CCT/tint band, subject luma band, contrast/saturation character
     |
  per-frame delta solve (damped, bounded, change-point aware)
     |
  identity skin targets --> identity-scoped skin corrections (P18 masks)
     |
  outlier detection (deviation > threshold) --> QC queue (P27)
     |
  recipe deltas + gallery timeline visualisation (before/after strips)
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-brain-gallery/src/{lib,tree,anchors,stats,normalise,skin_consistency,scene_consistency,changepoint,outlier}.rs` | Gallery brain. |
| `crates/aura-catalog/migrations/0025_gallery.sql` | `scene_nodes`, `anchors`, `normalisation`, `outliers` tables. |
| `config/consistency.toml` | Damping factors, bounds, thresholds, per-scene policy. |
| `apps/desktop/src/routes/gallery/{ConsistencyView,TimelineStrips,AnchorPicker,OutlierList}.tsx` | Gallery consistency UI. |
| `ml/eval/consistency_eval.py` | Spread-reduction and skin-drift metrics. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Gallery consistency contracts (frozen)**

```rust
pub struct SceneNode {
    pub id: NodeId, pub parent: Option<NodeId>,
    pub segment_id: SegmentId, pub label: String,
    pub image_ids: Vec<ImageId>,
    pub anchors: Vec<ImageId>,
    pub target: NodeTarget,
}

pub struct NodeTarget {
    pub cct_k: f32, pub cct_tol: f32,
    pub tint: f32, pub tint_tol: f32,
    pub subject_luma: f32, pub luma_tol: f32,
    pub contrast: f32, pub saturation: f32,
    pub grade_signature: [f32; 8],     // compact colour-character descriptor
}

pub struct NormalisationDelta {
    pub image_id: ImageId, pub node_id: NodeId,
    pub d_exposure: f32, pub d_cct: f32, pub d_tint: f32,
    pub d_contrast: f32, pub d_saturation: f32,
    pub skin_correction: Option<SkinCorrection>,
    pub damping: f32, pub bounded_by: Option<Bound>,
    pub reasons: Vec<Reason>, pub confidence: f32,
}
```

## 6. Algorithm, model and implementation design

### 6.1 Anchors, not averages

- Averaging a scene's frames produces mediocrity; anchoring to the *best-judged* frames preserves quality. Anchors are the frames the system is most confident about, weighted by primary-identity presence.
- Anchor statistics are robust (trimmed means, median chromaticity) so one anchor error cannot skew a node.
- Users can pin or reject anchors in the UI; pinned anchors are authoritative, which gives professionals direct control over the look of a scene.

### 6.2 Damped, bounded, change-point-aware normalisation

- Each frame's delta = damping * (target - current), where damping (default 0.6-0.8) preserves natural variation; bounds cap movement (e.g. |d_cct| <= 450 K, |d_exposure| <= 0.35 EV).
- Change-point detection over the frame sequence splits a node when illumination genuinely changes (flash toggled, sunset, moved indoors); each side normalises to its own anchors.
- Idempotence: after normalisation, recomputing deltas yields near-zero movement - proved by a test, because a non-idempotent solver would drift the gallery on every run.

### 6.3 Per-identity skin consistency

- For each identity, collect skin chromaticity and luminance from frames with high mask confidence and good exposure; take the robust central tendency as that person's target appearance.
- Correct deviations inside identity-scoped skin masks only, capped so lighting mood is preserved (a candle-lit face may stay warm, but not magenta).
- Guarantee: the same person's skin dE00 spread across the gallery <= 2.0 after correction - a measurable claim no competitor makes.

### 6.4 Outliers as a first-class output

- Frames deviating beyond threshold after normalisation are recorded with the specific deviation ('+310 K warmer than node anchors, magenta skin cast 4.2 dE00'), which becomes a QC ticket in Phase 27.
- The timeline strips give photographers a before/after picture of gallery drift, which is a demo-friendly visual and a genuine diagnostic tool.

## 7. Cloud AI usage (bring-your-own API key)

No cloud AI call in this phase. The phase must work with the network cable unplugged; the Cloud AI Gateway from Phase 04 stays idle here.

## 8. Implementation order (execute literally, in this order)

1. Build the scene-node tree from Phase 07 segments plus sub-clustering inside long segments.
2. Implement anchor selection with robust statistics and user pinning.
3. Implement the damped bounded solver with change-point splitting.
4. Prove and test idempotence and bounds.
5. Implement per-identity skin targets and identity-scoped corrections.
6. Implement scene-level contrast/saturation/grade harmonisation.
7. Implement outlier detection with quantified deviations.
8. Build the consistency UI with timeline strips, anchor picker and outlier list.
9. Validate on multi-camera, multi-lighting fixtures; measure spread reduction and skin drift.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `COL` | Colour Scientist | Own colour statistics, grade signature, skin targets and validation methodology | Validated solver | 9 d |
| `MLL` | ML Lead - Vision | Own damping/bounds policy, change-point detection, outlier thresholds | Signed spec | 4 d |
| `TLC` | Tech Lead - Imaging Core (Rust) | Design the scene-node tree, solver architecture and idempotence guarantees | Architecture + review | 3 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Implement tree, anchors, solver, skin consistency, outliers, persistence | `aura-brain-gallery` | 9 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Consistency view, timeline strips (before/after), anchor picker, outlier list | Gallery UI | 6 d |
| `MFE` | Mid-Level Frontend Engineer | Node tree navigation, per-node overrides, batch accept | UI panels | 3 d |
| `QAL` | QA Lead - Automation | Spread-reduction gate, skin-drift gate, idempotence test, bounds test | CI gates | 5 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Full-gallery review of 5 weddings before/after; hunt for flattened mood | Audit report | 4 d |
| `PM` | Product Manager Agent | Approve `consistency.toml` damping and bounds; define the 'preserve mood' rule | Approved config | 2 d |
| `PERF` | Performance Engineer | Keep the consistency pass under 60 s for 1,000 images; incremental re-solve | Benchmark | 3 d |
| `DATA` | Data Engineer / Dataset Curator | Label intentional lighting transitions on fixture weddings for change-point validation | Labels | 3 d |
| `DOC` | Technical Writer | Explain gallery consistency, anchors and how to pin them | Docs merged | 2 d |

### 9.1 Handoff chain for this phase

```text
TLC architecture -> SRC tree/anchors -> COL statistics + skin targets
                                |
                                v
                    MLL damping/change-points -> SRC solver -> SFE/MFE UI
                                |
              QAL gates + QAIQ full-gallery audit -> COL/PM gate -> Phases 26/27
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

- Within-scene WB spread reduced >= 60 % and exposure spread >= 50 % on fixture weddings.
- Per-identity skin dE00 spread <= 2.0 across the gallery.
- Idempotence: a second normalisation run moves every frame by less than a small epsilon.
- Bounds respected: no frame exceeds documented maximum movement.
- Intentional transitions (flash on/off, sunset, venue change) are not flattened - labelled fixture gate.
- Pinned anchors override automatic selection and persist through re-analysis.
- Outliers reported with quantified deviations and consumed correctly by the Phase 27 stub.

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
| Consistency pass for 1,000 images | <= 60 s |
| Incremental re-solve after one anchor change | <= 6 s |
| Timeline strips render | <= 400 ms |
| Extra storage per image | <= 500 B |

Telemetry events (local-first, opt-in aggregation):

- `gallery.normalised` {nodes, images, mean_d_cct, mean_d_ev, ms}
- `gallery.skin_corrected` {identities, mean_de00_before, mean_de00_after}
- `gallery.outliers` {count, mean_deviation}
- `gallery.anchor_pinned` {node, images}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Flattening intentional variation | Damping below 1.0, hard bounds, change-point splitting, labelled transition fixtures, and PM-owned preserve-mood policy. |
| Bad anchors propagate errors | Robust statistics, anchor confidence gating, user pinning, and outlier feedback loop. |
| Solver drift across runs | Idempotence test in CI and bounded movement. |
| Skin correction looks unnatural in coloured light | Caps that preserve mood, per-identity targets from that person's own frames, and audit gate. |

## 13. Acceptance criteria

- [ ] A whole wedding reads as one coherent gallery, verified by before/after timeline strips.
- [ ] Each scene is anchored to its best frames, and the user can pin anchors.
- [ ] The same person's skin looks consistent from morning preparation to late reception.
- [ ] Intentional lighting changes survive normalisation.
- [ ] Outliers are listed with quantified deviations, ready for QC.
- [ ] Running consistency twice changes nothing.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 25 - Gallery Intelligence Engine: Cross-Photo Colour, Skin & Scene Consistency.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-25-GALLERY-INTELLIGENCE-ENGINE.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - The app edits the gallery, not the photo: reference frames anchor each scene, and every other frame is normalised toward them so an entire wedding looks like one coherent body of work.

Rules:
  - Do not start Phase 26. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-brain-gallery/src/{lib,tree,anchors,stats,normalise,skin_consistency,scene_consistency,changepoint,outlier}.rs`, `crates/aura-catalog/migrations/0025_gallery.sql`, `config/consistency.toml`, `apps/desktop/src/routes/gallery/{ConsistencyView,TimelineStrips,AnchorPicker,OutlierList}.tsx`, `ml/eval/consistency_eval.py`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-25-gallery-intelligence-engine and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-25.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-25-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-25-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-25-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 25 of 30 - Gallery Intelligence Engine: Cross-Photo Colour, Skin & Scene Consistency - part of the AURA Wedding AI master build plan.*
