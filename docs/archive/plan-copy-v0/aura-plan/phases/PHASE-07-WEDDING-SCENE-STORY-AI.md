# Phase 07 - Wedding Scene AI & Story Timeline Segmentation

> **Single feature shipped by this phase:** The app reads the wedding as a story: it labels every photo's scene and splits the day into ordered chapters (Getting Ready -> Details -> Ceremony -> Rituals -> Portraits -> Reception -> Dance -> Exit) with confidence.
>
> **Mission:** Produce the scene graph that makes every later threshold scene-aware. A dark dance frame and a formal family portrait must never be judged by the same rules again.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 07 of 30 |
| Epic | E2 - Wedding Brain |
| Feature | The app reads the wedding as a story: it labels every photo's scene and splits the day into ordered chapters (Getting Ready -> Details -> Ceremony -> Rituals -> Portraits -> Reception -> Dance -> Exit) with confidence. |
| Depends on | Phases 02, 03, 05, 06 |
| Unlocks | Phases 09-13, 15-19, 25, 27, 29 |
| Duration | 2.5 weeks |
| Primary owners | ML Lead - Vision, Senior ML Engineer, AI Agent & Prompt Engineer, Senior Engineer - Core Pipeline (Rust) |
| Risk level | High - this is the core differentiator |
| Headline KPI | scene accuracy >= 0.92 top-1; segment boundary error <= 45 s median; ritual detection F1 >= 0.85 on Hindu/Nepali/Christian/Muslim fixtures |
| Competitor being beaten | FilterPixel contextual culling; Aftershoot scene awareness |

## 1. Why this phase exists

Every other product treats a wedding as a bag of frames. Reading it as an ordered story is what lets the product make defensible decisions: which frames may be dark, where blur is acceptable, which moments cannot be dropped, and how a scene should be graded.

Cultural coverage is a genuine competitive moat. Global tools handle Western wedding structure adequately and Hindu, Nepali and Muslim ceremonies poorly. Explicit ritual taxonomies plus cloud reasoning for uncertain segments turn that gap into an advantage.

## 2. Scope contract

### 2.1 In scope

- Per-image scene classifier: 22 classes (getting_ready_bride, getting_ready_groom, details, first_look, ceremony_entrance, ceremony, ritual, vows, rings, kiss, family_portrait, group_portrait, couple_portrait, golden_hour, reception_entrance, speeches, cake, first_dance, dance_floor, candid, venue, exit) plus 14 attributes (indoor/outdoor, flash, mixed_light, backlit, night, tungsten, crowd, stage, vehicle, food, decor, kids, pets, rain).
- Ritual sub-classifier for tradition-specific events (e.g. saptapadi, jaymala/varmala, sindoor, mehendi, sangeet, nikah, signing, unity candle, tea ceremony) with an extensible taxonomy file.
- Temporal segmentation: change-point detection over embeddings + time + scene posteriors, producing ordered `segments` with start/end, dominant scene, confidence and key-frame.
- Scene smoothing with a hidden Markov model over the timeline so single-frame misclassifications cannot create fake chapters.
- Cloud reasoning for low-confidence segments (Phase 04 `SegmentNaming` task) plus a user-editable chapter timeline.
- Scene profile registry: each scene carries default thresholds (acceptable noise, acceptable blur, expected keeper rate, editing intent) consumed by later phases.
- Timeline UI: horizontal chapter strip with counts, durations, thumbnails, drag-to-adjust boundaries and rename.

### 2.2 Explicitly out of scope (do not build it here)

- Culling decisions (Phase 12) and coverage guarantees (also Phase 12).
- Editing parameters per scene (Phases 15-17 read the scene, they do not define it).
- Album sequencing (Phase 29).

## 3. Architecture and data flow

```text
embeddings (P05) + faces (P06) + EXIF (P01) + luma stats (P05)
        |
        v
  SceneClassifier (multi-head: 22 scenes + 14 attributes)  --> per-image posteriors
        |                                                        |
        v                                                        v
  RitualClassifier (tradition taxonomy)                    HMM smoothing over timeline_ts
        |                                                        |
        +---------------------> ChangePointDetector <------------+
                                       |
                            segments[] (chapter, start, end, key_frame, confidence)
                                       |
              low confidence? --> Cloud SegmentNaming (P04) --> merged label + reasons
                                       |
                        SceneProfile registry (thresholds per scene) --> Phases 09-19, 25, 27, 29
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-brain-wedding/src/scene/{classifier,attributes,ritual,taxonomy,profile}.rs` | Scene and ritual inference plus the scene-profile registry. |
| `crates/aura-brain-wedding/src/story/{segment,changepoint,hmm,keyframe,api}.rs` | Timeline segmentation and chapter construction. |
| `crates/aura-catalog/migrations/0007_scenes.sql` | `image_scenes`, `segments`, `segment_images`, `scene_profiles` tables. |
| `config/scene_profiles.toml` + `config/rituals/{hindu,nepali,christian,muslim,civil}.toml` | Editable taxonomies and per-scene thresholds. |
| `ml/models/scene/{dataset.py,train_multihead.py,train_ritual.py,eval_scene.py,export.py}` | Training and evaluation. |
| `apps/desktop/src/routes/story/{Timeline,ChapterStrip,BoundaryEditor}.tsx` | Story timeline UI. |
| `docs/model-cards/{scene_classifier,ritual_classifier}.md` | Model cards with per-tradition metrics. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Scene, segment and profile contracts (frozen)**

```rust
pub struct SceneResult {
    pub image_id: ImageId,
    pub scene: SceneId, pub scene_conf: f32,
    pub top3: [(SceneId, f32); 3],
    pub attributes: AttrFlags,          // bitflags: indoor, flash, backlit, night, ...
    pub ritual: Option<RitualId>, pub ritual_conf: f32,
    pub source: Source,                 // Local | Cloud | UserOverride
    pub model_ver: u16,
}

pub struct Segment {
    pub id: SegmentId, pub chapter: ChapterId,
    pub start_ts: Timestamp, pub end_ts: Timestamp,
    pub dominant_scene: SceneId, pub confidence: f32,
    pub key_frame: ImageId, pub image_count: u32,
    pub reasons: Vec<String>, pub user_locked: bool,
}

pub struct SceneProfile {
    pub scene: SceneId,
    pub keeper_rate: (f32, f32),        // expected min/max survival ratio
    pub max_acceptable_noise: f32,      // scene-relative tolerance
    pub max_acceptable_blur: f32,
    pub subject_focus_weight: f32,      // how much subject sharpness dominates
    pub emotion_weight: f32,
    pub composition_weight: f32,
    pub editing_intent: EditIntent,     // Airy | Neutral | Warm | Moody | Punchy
    pub must_cover: bool,               // coverage guarantee applies
}
```

## 6. Algorithm, model and implementation design

### 6.1 Multi-head classifier design

- Shared trunk = the Phase 05 embedding (frozen) plus a small trainable adapter, so scene inference costs almost nothing extra per image.
- Heads: 22-way softmax scene, 14 independent sigmoid attributes, and a tradition-conditioned ritual head that is only evaluated when the segment context suggests a ceremony.
- Context features concatenated to the embedding: hour-of-day offset from first frame, flash flag, ISO bucket, face count, dominant-identity presence, indoor/outdoor prior from luma and colour temperature.
- Class imbalance handled with focal loss; rare rituals get oversampling plus a 'don't guess' abstain threshold.

### 6.2 Timeline segmentation

- Build a 1-D signal of embedding distance between consecutive frames, normalised by time gap, then run PELT change-point detection with a penalty tuned so a 10-hour wedding yields 8-16 chapters.
- Merge micro-segments shorter than 90 s or with fewer than 8 frames into their nearest neighbour unless their scene posterior is strongly distinct.
- Time gaps larger than 20 minutes are hard boundaries (photographers travel between locations).
- HMM smoothing with a hand-authored transition matrix encoding real wedding order (getting_ready rarely follows dance_floor) - this single trick removes most absurd labels.
- Key-frame per segment = medoid embedding among frames with high `subject_focus_score` and at least one primary identity.

### 6.3 Scene profiles: where the intelligence becomes actionable

- Every scene declares tolerances and weights in `scene_profiles.toml`, versioned and shipped with the app, overridable per project.
- Examples: `dance_floor` allows noise 3x the ceremony budget, expects motion blur, weights emotion 0.5 and composition 0.15; `family_portrait` demands eyes open for all subjects, weights composition 0.35 and forbids clipped faces; `details` ignores faces entirely.
- Profiles also declare `editing_intent`, which Phases 15-17 translate into tone/colour targets - the mechanism behind 'the ceremony looks like a ceremony, the dance floor looks like a party'.

### 6.4 Uncertainty and user control

- Segments with confidence < 0.75 are sent to the cloud `SegmentNaming` task once each (contact sheet of 12), and the merged result records both sources.
- The user can rename, split, merge and move boundaries; anything the user touches is `user_locked` and re-analysis never overwrites it.
- Unknown or genuinely novel events map to `other` with a description rather than a wrong confident label.

## 7. Cloud AI usage (bring-your-own API key)

**Name and validate uncertain chapters, identify tradition-specific rituals**

| Aspect | Specification |
|---|---|
| Model class | Vision reasoning tier, temperature 0 |
| Trigger | Segment confidence < 0.75, or ritual head abstains inside a ceremony segment |
| Input sent | Contact sheet of up to 12 key thumbnails (768 px), time range, local top-3 with scores, attribute flags, allowed vocabulary from the taxonomy files |
| Cost control | <= 16 calls per wedding; cached by content hashes |
| Offline fallback | Local argmax with confidence penalty and a 'needs review' flag in the timeline UI |

System prompt contract:

```text
You are a wedding post-production analyst reading one chapter of a wedding.
Input: a contact sheet of consecutive frames, the time range, the local classifier's top guesses, and the allowed vocabulary.
Task: choose the chapter label, the specific ritual if clearly visible, the tradition the visual evidence supports, and whether the chapter should be split.
Rules:
- Use only the allowed vocabulary; return "other" plus notes when nothing fits.
- Never infer names, gender, ethnicity or religion of individuals; describe the ceremony type only from visible ritual objects and staging.
- Prefer low confidence over a confident guess.
- Return ONLY JSON matching the schema.
```

Required JSON response schema (validated; invalid = retry once, then fall back to local model):

```json
{
  "type": "object",
  "required": ["chapter", "confidence", "reasons"],
  "properties": {
    "chapter": { "type": "string" },
    "ritual": { "type": ["string", "null"] },
    "tradition": { "type": ["string", "null"] },
    "split_at_index": { "type": ["integer", "null"] },
    "confidence": { "type": "number", "minimum": 0, "maximum": 1 },
    "reasons": { "type": "array", "items": { "type": "string" }, "maxItems": 6 },
    "notes": { "type": ["string", "null"] }
  },
  "additionalProperties": false
}
```

## 8. Implementation order (execute literally, in this order)

1. Author the scene taxonomy, attribute list and ritual taxonomies with a wedding photographer consultant; commit them as config.
2. Label the fixture weddings and an expansion set covering four traditions (DATA).
3. Train the multi-head classifier on the frozen embedding trunk; iterate until the accuracy gate is met.
4. Train the ritual head with abstention; evaluate per tradition.
5. Implement change-point detection and HMM smoothing; tune the penalty on labelled timelines.
6. Implement key-frame selection and the segment store.
7. Author `scene_profiles.toml` with the consultant and get PM sign-off - this file is a product decision, not a code detail.
8. Wire the cloud `SegmentNaming` task for low-confidence segments.
9. Build the timeline UI with boundary editing and locking.
10. Run evaluation gates, publish model cards, write the exit report.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `MLL` | ML Lead - Vision | Own taxonomy, model architecture, abstention policy, per-tradition evaluation | Signed spec + gates | 3 d |
| `SRML` | Senior ML Engineer | Train and export scene + ritual heads; parity and calibration | Models registered | 6 d |
| `MLR` | ML Research Engineer | Change-point penalty tuning, HMM transition matrix, ablation of context features | Segmentation report | 4 d |
| `DATA` | Data Engineer / Dataset Curator | Scene/ritual/timeline labels across four traditions; enforce wedding-level splits and consent records | Dataset v2 | 8 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Segment store, smoothing, key-frames, scene-profile registry, override semantics | `aura-brain-wedding` + tests | 5 d |
| `AGT` | AI Agent & Prompt Engineer | `SegmentNaming` integration, vocabulary injection, merge rules, cassette tests | Cloud path live | 2 d |
| `PM` | Product Manager Agent | Sign off `scene_profiles.toml` values with a professional wedding photographer; document rationale | Approved profiles | 3 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Chapter strip, boundary drag, split/merge, rename, review flags | Timeline UI | 4 d |
| `MFE` | Mid-Level Frontend Engineer | Scene filter chips in the grid, per-scene counts, 'needs review' queue | UI polish | 3 d |
| `QAL` | QA Lead - Automation | Accuracy/boundary/ritual gates in CI, override-persistence tests, weird-timeline fixtures | CI gates | 4 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Human review of chapters on 20 weddings across traditions; catalogue systematic errors | Audit report | 3 d |
| `DOC` | Technical Writer | Document the taxonomy, how to add a tradition, and how profiles affect results | Docs merged | 2 d |

### 9.1 Handoff chain for this phase

```text
PM + consultant taxonomy -> DATA labels -> SRML models -> MLR segmentation
                                                   |
                                                   v
                                       SRC store + profiles -> AGT cloud naming
                                                   |
                                    SFE/MFE timeline UI -> QAIQ audit -> MLL/PM gate
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

- Scene top-1 accuracy >= 0.92 overall and >= 0.85 for every individual class with more than 200 labelled samples.
- Boundary error median <= 45 s; no wedding produces fewer than 6 or more than 20 chapters.
- Ritual F1 >= 0.85 per tradition; abstention rather than wrong labels on unseen rituals.
- HMM sanity: no chapter sequence violates the allowed transition matrix.
- User overrides survive full re-analysis and model upgrades.
- Two-shooter interleaving does not fragment chapters (specific regression fixture).

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
| Scene + attributes for 4,000 images (reusing embeddings) | <= 35 s |
| Segmentation + smoothing | <= 2 s |
| Timeline UI open | <= 200 ms |
| Extra storage per image | <= 400 B |

Telemetry events (local-first, opt-in aggregation):

- `scene.classified` {images, ms, mean_conf, low_conf_count}
- `story.segmented` {segments, boundary_penalty, chapters}
- `story.user_edit` {action, segment, from_label, to_label}
- `scene.cloud_used` {segments, calls, cost_usd}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Cultural blind spots produce wrong chapters | Tradition taxonomies in config, cloud reasoning fallback, mandatory per-tradition metrics, and a user-editable timeline. |
| Over-segmentation makes the timeline noisy | Minimum segment size, merge pass, penalty tuning against labelled timelines, and a hard chapter-count band. |
| Scene profiles become a dumping ground of magic numbers | Every value has a written rationale, an owner (PM) and a QA fixture demonstrating its effect. |
| Model drift after an embedding upgrade | Scene heads are versioned against `embed model_ver`; a mismatch forces re-inference. |

## 13. Acceptance criteria

- [ ] A freshly analysed wedding shows a correct, ordered chapter strip with counts and durations.
- [ ] Every image carries a scene label, attributes and confidence, visible in the Explain panel.
- [ ] Hindu, Nepali, Christian and Muslim fixture weddings all produce correct ritual labels or honest abstentions.
- [ ] Editing a boundary or renaming a chapter persists through re-analysis.
- [ ] `scene_profiles.toml` drives measurable behaviour differences in later phases (proved by a fixture test).
- [ ] Low-confidence chapters are flagged for review rather than silently guessed.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 07 - Wedding Scene AI & Story Timeline Segmentation.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-07-WEDDING-SCENE-STORY-AI.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - The app reads the wedding as a story: it labels every photo's scene and splits the day into ordered chapters (Getting Ready -> Details -> Ceremony -> Rituals -> Portraits -> Reception -> Dance -> Exit) with confidence.

Rules:
  - Do not start Phase 8. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-brain-wedding/src/scene/{classifier,attributes,ritual,taxonomy,profile}.rs`, `crates/aura-brain-wedding/src/story/{segment,changepoint,hmm,keyframe,api}.rs`, `crates/aura-catalog/migrations/0007_scenes.sql`, `config/scene_profiles.toml` + `config/rituals/{hindu,nepali,christian,muslim,civil}.toml`, `ml/models/scene/{dataset.py,train_multihead.py,train_ritual.py,eval_scene.py,export.py}`, `apps/desktop/src/routes/story/{Timeline,ChapterStrip,BoundaryEditor}.tsx`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-07-wedding-scene-story-ai and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-07.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-07-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-07-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-07-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 07 of 30 - Wedding Scene AI & Story Timeline Segmentation - part of the AURA Wedding AI master build plan.*
