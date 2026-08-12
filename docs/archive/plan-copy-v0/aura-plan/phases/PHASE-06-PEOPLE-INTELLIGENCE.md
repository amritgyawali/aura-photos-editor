# Phase 06 - Face Detection, Recognition & People Intelligence

> **Single feature shipped by this phase:** The app learns who matters at this wedding: it finds every face, groups them into identities, and automatically ranks bride, groom, immediate family and VIPs by evidence rather than by guesswork.
>
> **Mission:** Give every later decision a subject hierarchy. Sharpness on the bride's face must outrank sharpness on a stranger's elbow, and no phase can reason about that without People Intelligence.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 06 of 30 |
| Epic | E2 - Wedding Brain |
| Feature | The app learns who matters at this wedding: it finds every face, groups them into identities, and automatically ranks bride, groom, immediate family and VIPs by evidence rather than by guesswork. |
| Depends on | Phases 02, 03, 05 |
| Unlocks | Phases 09-13, 18-22, 25, 27, 29 |
| Duration | 2.5 weeks |
| Primary owners | ML Lead - Vision, Senior ML Engineer, Senior Engineer - Core Pipeline (Rust), Security & Privacy Engineer |
| Risk level | High - accuracy and privacy both matter |
| Headline KPI | face detection recall >= 0.97 at IoU 0.5 on wedding fixtures; identity clustering F1 >= 0.93; couple identification accuracy >= 0.95 |
| Competitor being beaten | Aftershoot/Imagen face grouping; Narrative Select face focus |

## 1. Why this phase exists

Wedding photography is people photography with a strict hierarchy: the couple, then immediate family, then guests. Every competitor that treats faces as equal produces galleries where a sharp photo of a stranger beats a slightly soft photo of the bride's tears - which is the wrong answer.

Identity grouping also unlocks coverage guarantees ('every close family member appears at least three times'), consistency ('the bride's skin must render the same all night') and curation ('the hero shots must feature the couple').

Because this is biometric data, it must be strictly local, project-scoped, deletable and never uploaded - a privacy stance that is also a marketing advantage.

## 2. Scope contract

### 2.1 In scope

- Face detection with landmarks (5-point + 68-point optional) on Tier-1 previews, tuned for small faces in wide ceremony frames.
- Face quality gate: pose (yaw/pitch/roll), blur, occlusion, resolution - low-quality faces are detected but excluded from identity voting.
- Face recognition embeddings (512-d ArcFace-class), project-scoped index, and agglomerative clustering with a tuned threshold plus rank-order verification.
- Identity roles: automatic detection of `bride`, `groom`, `couple`, `family_close`, `family_extended`, `vip`, `guest`, `vendor`, `child`, `unknown` with confidence and evidence.
- Prominence scoring per image: face area, centrality, sharpness, gaze, occlusion, count of frames the identity appears in, and scene weighting.
- Body/person detection to associate faces with bodies for later masking, and to count people when faces are hidden.
- Person timeline: for each identity, when they appear, in which scenes, with which other identities (co-occurrence graph).
- UI: People panel with merge/split/rename, 'this is the bride' one-click assignment, and per-identity importance slider.
- Privacy: face data in a separate encrypted-at-rest store, per-project deletion, export/erase controls, never sent to the cloud unless the user explicitly enables blur-free cloud reasoning.

### 2.2 Explicitly out of scope (do not build it here)

- Eye-open/blink analysis (Phase 09) and expression scoring (Phase 10) - they consume these crops.
- Skin masks and retouching (Phases 18, 20-21).
- Cross-wedding identity persistence (explicitly never - identities are project-scoped by policy).

## 3. Architecture and data flow

```text
Tier1 preview --> FaceDetector (SCRFD-class) --> boxes + 5pt landmarks + quality
                                 |
                                 +--> align 112x112 --> FaceEmbedder (ArcFace-class) --> 512-d
                                 |                                          |
                                 +--> PersonDetector (body boxes) --> face<->body association
                                                                            v
                                                        project face index -> clustering -> identities
                                                                            |
                       role inference (evidence: co-occurrence, scene, attire, frequency, cloud hint)
                                                                            |
                       per-image prominence: area x centrality x sharpness x role weight
                                                                            |
                                        People panel (merge/split/rename/mark bride)
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-vision/src/face/{detect,align,embed,quality,cluster,roles,prominence,person}.rs` | Detection through role inference. |
| `crates/aura-catalog/migrations/0006_people.sql` | `faces`, `identities`, `identity_links`, `person_boxes`, `cooccurrence` tables. |
| `crates/aura-people/src/{lib,timeline,graph,importance,api}.rs` | Identity timeline, co-occurrence graph, importance model. |
| `ml/models/face/{train_quality.py,tune_cluster.py,eval_identity.py}` | Quality head training and clustering threshold tuning. |
| `apps/desktop/src/routes/people/{PeoplePanel,IdentityCard,MergeDialog}.tsx` | People management UI. |
| `docs/model-cards/{face_detect,face_embed,face_quality}.md` | Model cards including demographic performance analysis. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**People schema**

```sql
CREATE TABLE faces (
  id TEXT PRIMARY KEY, image_id TEXT NOT NULL, identity_id TEXT,
  x REAL, y REAL, w REAL, h REAL,               -- normalised
  yaw REAL, pitch REAL, roll REAL,
  det_score REAL, quality REAL, blur REAL, occlusion REAL,
  landmarks_json TEXT, embed BLOB,               -- 512-d fp16
  area_frac REAL, centrality REAL, sharpness REAL,
  model_ver INTEGER NOT NULL
);
CREATE INDEX idx_faces_image ON faces(image_id);
CREATE INDEX idx_faces_identity ON faces(identity_id);

CREATE TABLE identities (
  id TEXT PRIMARY KEY, project_id TEXT NOT NULL,
  label TEXT,                                    -- user-assigned name
  role TEXT NOT NULL DEFAULT 'unknown',          -- bride|groom|family_close|...
  role_confidence REAL, role_reasons TEXT,
  user_locked INTEGER NOT NULL DEFAULT 0,        -- user decisions are never overwritten
  importance REAL NOT NULL DEFAULT 0.5,
  face_count INTEGER, first_seen TEXT, last_seen TEXT,
  centroid BLOB
);
```

**Prominence and role API**

```rust
pub struct SubjectHierarchy {
    pub primary: Vec<IdentityId>,     // couple
    pub secondary: Vec<IdentityId>,   // close family, VIPs
    pub weights: HashMap<IdentityId, f32>,
}

pub struct ImageSubjects {
    pub faces: Vec<FaceRef>,
    pub dominant: Option<IdentityId>,
    pub prominence: HashMap<IdentityId, f32>,  // 0..1 within this frame
    pub subject_focus_score: f32,              // weighted sharpness of who matters
}

pub trait PeopleService {
    fn hierarchy(&self, project: ProjectId) -> SubjectHierarchy;
    fn subjects(&self, image: ImageId) -> ImageSubjects;
    fn merge(&self, a: IdentityId, b: IdentityId) -> Result<IdentityId, AuraError>;
    fn split(&self, id: IdentityId, faces: &[FaceId]) -> Result<IdentityId, AuraError>;
    fn set_role(&self, id: IdentityId, role: Role) -> Result<(), AuraError>; // sets user_locked
}
```

## 6. Algorithm, model and implementation design

### 6.1 Detection tuned for real weddings

- Multi-scale inference: full-frame pass at 640 px plus a 2x2 tiled pass at 640 px when the frame is wide-angle and faces are small, then NMS across passes - this is what recovers guests in wide ceremony shots.
- Quality head predicts usability (pose, blur, occlusion, pixel height); faces under 48 px or quality < 0.4 are stored but excluded from identity voting.
- Person/body detection covers back-of-head and hair-covered cases so later masks and people counts still work.

### 6.2 Identity clustering that survives 12 hours of changing light

- Cluster with average-linkage agglomerative clustering on cosine distance, threshold tuned per project by a small validation sweep using high-quality faces only.
- Two-pass strategy: build a skeleton from high-quality faces, then assign medium-quality faces by nearest centroid with a stricter margin, leaving ambiguous faces unassigned rather than wrong.
- Rank-order verification (mutual nearest neighbour) prevents chain-merging two similar-looking relatives.
- Outfit-change robustness: no clothing features in the identity signal; hairstyle changes handled by using multiple centroids per identity (sub-clusters) when internal variance is high.

### 6.3 Role inference from evidence, not stereotypes

- Bride/groom candidates: identities with the highest frequency across `getting_ready`, `ceremony`, `couple_portrait` scenes, highest average face area, and highest mutual co-occurrence with each other.
- The couple is the identity pair maximising (co-occurrence in `couple_portrait` + ceremony centrality + total frequency); labels `bride`/`groom` are only suggestions until the user confirms, and the app is explicit that it may be a same-sex couple - the pair is what matters, not gender.
- Close family: high co-occurrence with the couple in `family` scenes plus presence in `getting_ready`.
- Vendors: appear across scenes but rarely as a photographic subject (low face area, low centrality, often at frame edges).
- Optional cloud hint (Phase 04) may raise confidence but can never override a `user_locked` identity.

### 6.4 Prominence scoring

- `prominence = w1*sqrt(area_frac) + w2*centrality + w3*sharpness + w4*gaze_to_camera + w5*role_weight`, with scene-specific weights (in `dance`, area matters less; in `family_portrait`, centrality matters more).
- `subject_focus_score` for an image is the prominence-weighted sharpness of the faces present - the single number Phases 09 and 12 use instead of naive global sharpness.
- All weights live in a versioned config file so QA can tune without recompiling, and every score records the config version.

### 6.5 Privacy engineering

- Face embeddings and crops live in a per-project store encrypted with a key in the OS keychain; deleting a project shreds them.
- No cross-project identity linking, ever - enforced by schema (identities are project-scoped) and by a test.
- 'Erase biometric data' button removes faces/identities while keeping culling and edit decisions intact.

## 7. Cloud AI usage (bring-your-own API key)

**Optional couple/role hint when local evidence is ambiguous**

| Aspect | Specification |
|---|---|
| Model class | Vision reasoning tier, temperature 0 |
| Trigger | Only when top-2 couple candidates are within 0.05 score and the user has enabled cloud reasoning for people |
| Input sent | Up to 8 blurred-background thumbnails (faces visible only if the user allows), scene labels, co-occurrence statistics as text |
| Cost control | Max 2 calls per project; cached |
| Offline fallback | Local evidence argmax, flagged low confidence, user prompted to confirm in the People panel |

System prompt contract:

```text
You are helping identify which two people are the wedding couple from statistical evidence and sample frames.
Rules:
- Base the answer only on photographic role evidence: who appears in getting-ready, who is central during the ceremony, who is photographed together in portraits.
- Do not infer gender, ethnicity, religion or relationships beyond 'couple' / 'close family' / 'guest'.
- If the evidence is ambiguous, return low confidence and explain what is missing.
- Return ONLY JSON matching the schema.
```

Required JSON response schema (validated; invalid = retry once, then fall back to local model):

```json
{
  "type": "object",
  "required": ["couple", "confidence", "reasons"],
  "properties": {
    "couple": { "type": "array", "items": { "type": "string" }, "minItems": 0, "maxItems": 2 },
    "close_family": { "type": "array", "items": { "type": "string" }, "maxItems": 10 },
    "confidence": { "type": "number", "minimum": 0, "maximum": 1 },
    "reasons": { "type": "array", "items": { "type": "string" }, "maxItems": 6 }
  },
  "additionalProperties": false
}
```

## 8. Implementation order (execute literally, in this order)

1. Land the schema and the face store with encryption; write the privacy tests first.
2. Integrate the detector with multi-scale/tiled inference; measure recall on small faces.
3. Add alignment plus the recognition embedder; verify embedding quality on labelled identities.
4. Train/calibrate the face-quality head and set the voting gate.
5. Implement clustering with the two-pass strategy and tune the threshold on fixtures.
6. Implement person detection and face-body association.
7. Implement role inference, co-occurrence graph and identity timelines.
8. Implement prominence scoring with the versioned weight config.
9. Build the People panel with merge/split/rename/mark-couple and `user_locked` semantics.
10. Add the optional cloud hint, evaluation gates, model cards and the erase-biometrics control.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `MLL` | ML Lead - Vision | Own detector/embedder selection, quality head design, clustering evaluation and demographic fairness analysis | Signed spec + fairness report | 3 d |
| `SRML` | Senior ML Engineer | Integrate and calibrate detection + recognition + quality models; export and verify parity | Models in registry | 6 d |
| `MLR` | ML Research Engineer | Tune clustering strategy, threshold sweeps, chain-merge prevention, sub-centroid handling | Clustering report | 4 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Face store, encryption, batching, face-body association, timelines and co-occurrence graph | `aura-people` + tests | 6 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Prominence scoring + versioned weight config + `subject_focus_score` API | Scoring module | 2 d |
| `AGT` | AI Agent & Prompt Engineer | Implement the optional couple-hint cloud task with strict non-inference rules | Cloud task + cassettes | 2 d |
| `SEC` | Security & Privacy Engineer | Biometric threat model, at-rest encryption, erase flow, no-cross-project proof, DPIA-style note | Security sign-off | 4 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | People panel, identity cards, merge/split, mark-couple, importance slider | UI shipped | 4 d |
| `MFE` | Mid-Level Frontend Engineer | Face-quality filters, person timeline view, erase-biometrics dialog, empty states | UI polish | 3 d |
| `DATA` | Data Engineer / Dataset Curator | Identity ground truth on fixtures (with consent records), small-face and dark-scene subsets | Labelled identities | 5 d |
| `QAL` | QA Lead - Automation | Detection recall, clustering F1, merge/split correctness, privacy tests in CI | CI gates | 4 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Manual audit: does the app pick the right couple on 20 real weddings across traditions? | Audit report | 3 d |
| `PERF` | Performance Engineer | Face pipeline throughput and memory; tile-pass cost/benefit tuning | Benchmark report | 2 d |
| `DOC` | Technical Writer | Privacy documentation, People panel help, model cards | Docs merged | 2 d |

### 9.1 Handoff chain for this phase

```text
SEC privacy design -> SRC face store -> SRML models -> MLR clustering
                                   |                        |
                                   v                        v
                        SRC prominence + graph        AGT cloud hint (optional)
                                   |
                        SFE/MFE People panel -> QAIQ audit -> MLL/CTO gate
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

- Detection recall >= 0.97 overall and >= 0.90 on the small-face subset; false positives on bokeh highlights < 1 %.
- Identity clustering F1 >= 0.93; no cluster contains two labelled people; siblings are not merged.
- Couple identification correct on >= 19 of 20 audit weddings; ambiguous cases correctly ask the user.
- Merge/split/rename are undoable and never overwritten by a re-analysis (`user_locked` respected).
- Privacy: face store unreadable without the keychain entry; erase removes all faces; no cross-project query is possible.
- Prominence: in labelled test frames, the identified dominant subject matches human judgement >= 90 % of the time.

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
| Face pipeline, 4,000 images (RTX 4070) | <= 240 s including tiled passes |
| Face pipeline, 4,000 images (M3 Pro) | <= 480 s |
| Clustering 25,000 faces | <= 12 s |
| People panel open | <= 300 ms |
| Face store size per 1,000 images | <= 25 MB |

Telemetry events (local-first, opt-in aggregation):

- `face.detect` {images, faces, small_faces, ms, tiled_pass_used}
- `identity.cluster` {faces_used, identities, threshold, ms}
- `identity.role_inferred` {role, confidence, evidence_kinds}
- `people.user_edit` {action: merge|split|rename|role, identities}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Wrong couple identification poisons every later decision | Confidence gate + explicit user confirmation prompt on first Autopilot run + `user_locked` overrides that survive re-analysis. |
| Recognition accuracy varies across skin tones | Mandatory demographic evaluation in the model card, balanced fixture sets, and a stricter margin rather than a wrong assignment. |
| Guests merged into one identity | Mutual-NN verification, high-quality-only skeleton pass, and a QA fixture with lookalike relatives. |
| Biometric regulation exposure | Local-only, project-scoped, encrypted, deletable, documented, never uploaded by default; SEC owns the compliance note. |
| Tiled detection doubles cost | Trigger tiling only for wide-angle frames with many small detections; measured cost/benefit in the benchmark report. |

## 13. Acceptance criteria

- [ ] After analysis, the People panel shows clean identities with face counts and a suggested couple.
- [ ] Marking someone as the bride updates every downstream weight and survives a full re-analysis.
- [ ] Wide ceremony frames detect guests reliably; back-of-head cases still register a person.
- [ ] Every image exposes `subject_focus_score` and per-identity prominence.
- [ ] Biometric data is encrypted, project-scoped and erasable, and never leaves the machine by default.
- [ ] Fairness analysis is published in the model cards with per-group metrics.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 06 - Face Detection, Recognition & People Intelligence.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-06-PEOPLE-INTELLIGENCE.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - The app learns who matters at this wedding: it finds every face, groups them into identities, and automatically ranks bride, groom, immediate family and VIPs by evidence rather than by guesswork.

Rules:
  - Do not start Phase 7. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-vision/src/face/{detect,align,embed,quality,cluster,roles,prominence,person}.rs`, `crates/aura-catalog/migrations/0006_people.sql`, `crates/aura-people/src/{lib,timeline,graph,importance,api}.rs`, `ml/models/face/{train_quality.py,tune_cluster.py,eval_identity.py}`, `apps/desktop/src/routes/people/{PeoplePanel,IdentityCard,MergeDialog}.tsx`, `docs/model-cards/{face_detect,face_embed,face_quality}.md`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-06-people-intelligence and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-06.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-06-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-06-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-06-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 06 of 30 - Face Detection, Recognition & People Intelligence - part of the AURA Wedding AI master build plan.*
