# Phase 10 - Expression, Emotion & Moment Ranking AI

> **Single feature shipped by this phase:** The app finds the moments that matter: genuine smiles, laughter, tears, hugs, kisses, reactions and ritual peaks - and ranks every frame by emotional value.
>
> **Mission:** Give the product taste. Technical quality decides what is acceptable; emotion decides what is *worth delivering*, and this is where the gallery starts to feel like it was chosen by a human.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 10 of 30 |
| Epic | E2 - Wedding Brain |
| Feature | The app finds the moments that matter: genuine smiles, laughter, tears, hugs, kisses, reactions and ritual peaks - and ranks every frame by emotional value. |
| Depends on | Phases 05, 06, 07, 09 |
| Unlocks | Phases 12, 13, 27, 29 |
| Duration | 2.5 weeks |
| Primary owners | ML Lead - Vision, Senior ML Engineer, AI Agent & Prompt Engineer, ML Research Engineer |
| Risk level | High - subjective and culturally sensitive |
| Headline KPI | expression ranking agreement with photographers >= 0.80 pairwise; peak-moment detection recall >= 0.90; tear/laughter detection F1 >= 0.85 |
| Competitor being beaten | FilterPixel moment/expression scoring; Aftershoot expression preference |

## 1. Why this phase exists

Photographers do not sell sharpness, they sell feeling. Any product that automates culling without modelling emotion will consistently deliver technically perfect, emotionally flat galleries - the most common complaint about AI culling today.

Emotion also unlocks the premium features: hero selection, album storytelling and social picks are all emotion-ranking problems. Investing here pays off three more times later.

Emotion must be culturally careful: composure is the norm in many traditions, so 'no big smile' cannot mean 'no emotion'. Scene- and tradition-aware baselines are required.

## 2. Scope contract

### 2.1 In scope

- Per-face expression head: smile intensity, genuineness (Duchenne cue via eye-region activation), laughter, crying/tears, surprise, tenderness, neutral-composed, discomfort/awkward - all continuous, not one-hot.
- Gaze and attention: looking at camera, at partner, at officiant, away; mutual-gaze detection between primary identities.
- Interaction detection at image level: hug, kiss, hand-hold, dance-hold, ring exchange, blessing/touch, toast, tears-being-wiped, group cheer.
- Moment peak detection within each Phase 08 moment: the frame where the action is at maximum expression (kiss apex, tear falling, bouquet released).
- Reaction linking: connect a primary action frame with reaction frames from the same instant (parents crying while the couple kisses) using the timeline and gaze direction.
- `emotion_score` per image: scene-weighted combination of subject expression, interaction significance, peak proximity and reaction value, calibrated against photographer preferences.
- Optional cloud reasoning for narrative significance of a moment when local scores are ambiguous (batched, contact-sheet based).
- Preference learning hook: pairwise comparisons collected from the user feed a lightweight ranker (used fully in Phase 30's learning loop).

### 2.2 Explicitly out of scope (do not build it here)

- Final selection (Phase 12).
- Album sequencing and hero picks (Phase 29).
- Any claim about a person's inner emotional state - the model scores *photographic expression*, not psychology.

## 3. Architecture and data flow

```text
aligned face crops (P06) --> ExpressionHead (multi-output continuous)
                                    |
 full frame (P02) --> InteractionHead (hug/kiss/hold/ritual/toast/cheer)
                                    |
 gaze estimation --> mutual gaze, attention target
                                    |
 moment (P08) --> peak curve over frames --> peak_index, peak_margin
                                    |
 reaction linker (time + gaze + identity role)
                                    |
            emotion_score (scene-weighted, calibrated) + reasons[]
                                    |
     ambiguous? --> Cloud MomentSignificance (P04) --> narrative weight + reasons
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-brain-wedding/src/emotion/{expression,gaze,interaction,peak,reaction,score}.rs` | All emotion analysis. |
| `ml/models/emotion/{train_expression.py,train_interaction.py,train_ranker.py,eval_emotion.py}` | Model training including the preference ranker. |
| `crates/aura-catalog/migrations/0010_emotion.sql` | `face_expression`, `image_interaction`, `moment_peak`, `reaction_links` tables. |
| `config/emotion_weights.toml` | Scene- and tradition-aware weighting, PM-owned. |
| `apps/desktop/src/components/explain/EmotionCard.tsx` | Emotion readout with face crops and interaction labels. |
| `docs/model-cards/{expression_head,interaction_head}.md` | Model cards with cultural-bias analysis. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Emotion contracts (frozen)**

```rust
pub struct FaceExpression {
    pub face_id: FaceId, pub identity: Option<IdentityId>,
    pub smile: f32, pub genuineness: f32, pub laughter: f32, pub tears: f32,
    pub surprise: f32, pub tenderness: f32, pub composed: f32, pub discomfort: f32,
    pub gaze: GazeTarget, pub confidence: f32,
}

pub struct ImageEmotion {
    pub image_id: ImageId,
    pub interactions: Vec<(Interaction, f32)>,   // kind + strength
    pub mutual_gaze: bool,
    pub peak_proximity: f32,                     // 1.0 at the moment's peak frame
    pub reaction_of: Option<ImageId>,            // this frame reacts to that frame
    pub emotion_score: f32,                      // scene-weighted, calibrated
    pub narrative_weight: f32,                   // raised by cloud reasoning when used
    pub reasons: Vec<Reason>,
    pub source: Source,
}
```

## 6. Algorithm, model and implementation design

### 6.1 Expression modelling that respects culture

- Continuous multi-output regression, trained on photographer-ranked wedding faces rather than on generic emotion datasets, because 'delivered vs rejected' is the label that matters.
- Genuineness uses eye-region activation alongside mouth shape so posed grins score lower than real laughter - and this is exposed as a reason, not hidden.
- Composure is a positive class: in `ritual` and `vows` scenes, `composed` with mutual gaze can outscore a smile. Weights come from `emotion_weights.toml` per scene and tradition.
- Tears detection uses eye-region specular/wet cues plus expression context, with deliberately high precision (a wrongly detected tear is embarrassing).

### 6.2 Interaction and peak detection

- Interaction head operates on the full frame with person boxes as spatial priors, predicting the interaction set with strengths.
- Within a moment, build an expression/interaction curve over frames and find the argmax with a smoothing kernel; `peak_margin` records how clearly the peak wins.
- Kiss apex, tear release, bouquet-in-air and ring-slide are trained as explicit peak types because they are the frames clients buy.

### 6.3 Reaction linking (a feature no competitor ships)

- For each high-significance action frame, search +/- 4 s across *all* cameras for frames whose subjects gaze toward the action and show strong expression.
- Link them as `reaction_of`, which lets Phase 12 guarantee that a kiss keeper is accompanied by the mother's tears, and lets Phase 29 build cause-effect album spreads.
- Reactions are scored with a bonus proportional to the action's significance and the reactor's role weight.

### 6.4 Calibration to photographer taste

- Collect pairwise preferences ('which of these two would you deliver?') from photographers on fixture moments; fit a Bradley-Terry ranker over model features.
- Final `emotion_score` is the ranker output, calibrated per scene by isotonic regression - this makes the number comparable to `technical_score` in Phase 12.
- The same mechanism is later reused for per-user personalisation in Phase 30, so the interface is designed for it now.

## 7. Cloud AI usage (bring-your-own API key)

**Narrative significance of an ambiguous moment**

| Aspect | Specification |
|---|---|
| Model class | Vision reasoning tier, temperature 0 |
| Trigger | Moment-level emotion scores within 0.05 of each other, or an unrecognised ritual peak |
| Input sent | Up to 6 thumbnails of the moment (768 px), scene/ritual labels, detected interactions, identity roles (anonymised as 'primary A/B', 'close family') |
| Cost control | <= 25 calls per wedding; batched per moment; cached |
| Offline fallback | Local ranker output only, with `narrative_weight = 0` and lower confidence |

System prompt contract:

```text
You are a wedding photo editor deciding how important a moment is to the wedding story.
Input: frames from one moment, the chapter, detected interactions and anonymised subject roles.
Task: rate narrative significance 0-1, pick the single strongest frame index, and explain in short editorial reasons.
Rules:
- Judge storytelling value: is this a milestone, a peak reaction, a unique moment, or a repeat?
- Do not comment on appearance, body, ethnicity or attractiveness. Never speculate about relationships beyond the given roles.
- Do not describe emotions as psychological facts; describe what is visible.
- Return ONLY JSON matching the schema.
```

Required JSON response schema (validated; invalid = retry once, then fall back to local model):

```json
{
  "type": "object",
  "required": ["significance", "best_index", "confidence", "reasons"],
  "properties": {
    "significance": { "type": "number", "minimum": 0, "maximum": 1 },
    "best_index": { "type": "integer", "minimum": 0 },
    "moment_type": { "type": ["string", "null"] },
    "confidence": { "type": "number", "minimum": 0, "maximum": 1 },
    "reasons": { "type": "array", "items": { "type": "string" }, "maxItems": 5 }
  },
  "additionalProperties": false
}
```

## 8. Implementation order (execute literally, in this order)

1. Define the expression/interaction taxonomy with the photographer consultant; write the cultural-sensitivity rules.
2. Collect labels: face expression regression targets and pairwise 'which would you deliver' comparisons.
3. Train the expression head; validate that composure is not penalised in ritual scenes.
4. Train the interaction head with person-box priors.
5. Implement gaze estimation and mutual-gaze detection.
6. Implement peak detection over moments and validate on kiss/toss/tears fixtures.
7. Implement reaction linking across cameras.
8. Fit the Bradley-Terry ranker and per-scene calibration; wire `emotion_score`.
9. Add the optional cloud significance task with strict anonymisation.
10. Build the Emotion card UI; run the photographer agreement study.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `MLL` | ML Lead - Vision | Own emotion taxonomy, ranker design, calibration and cultural-bias evaluation | Signed spec + bias report | 3 d |
| `SRML` | Senior ML Engineer | Train expression + interaction heads, gaze model integration, export and parity | Models registered | 7 d |
| `MLR` | ML Research Engineer | Peak detection algorithm, reaction-linking heuristics, ranker feature ablations | Research report | 5 d |
| `DATA` | Data Engineer / Dataset Curator | Expression labels, interaction boxes, 10k pairwise photographer comparisons across traditions | Preference dataset | 9 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Implement scoring, peaks, reaction links, persistence, reason generation | `emotion` module + tests | 6 d |
| `AGT` | AI Agent & Prompt Engineer | Cloud `MomentSignificance` task with anonymisation and cassettes | Cloud path live | 2 d |
| `PM` | Product Manager Agent | Own `emotion_weights.toml`, approve the cultural rules, define the 'no psychological claims' policy | Approved config + policy | 2 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Emotion card with face crops, interaction chips, peak indicator | Explain UI part 2 | 3 d |
| `MFE` | Mid-Level Frontend Engineer | Moment browser sorted by emotion, reaction pair viewer | UI panels | 3 d |
| `QAL` | QA Lead - Automation | Agreement study harness, F1 gates, cultural fixture gates, calibration tests | CI gates | 4 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Blind study: 5 photographers rank 300 moments vs the model | Agreement report | 4 d |
| `SEC` | Security & Privacy Engineer | Review that no emotion data leaves the device and that cloud payloads are anonymised | Sign-off | 1 d |
| `DOC` | Technical Writer | Explain emotion scoring honestly in user docs; avoid overclaiming | Docs merged | 2 d |

### 9.1 Handoff chain for this phase

```text
PM taxonomy + cultural rules -> DATA preference labels -> SRML models
                                              |
                                              v
                              MLR peaks/reactions -> SRC scoring -> AGT cloud
                                              |
                                SFE/MFE UI -> QAIQ agreement study -> MLL/PM gate
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

- Pairwise agreement with photographers >= 0.80 on held-out moments.
- Peak detection: chosen frame is within the human-chosen top-2 in >= 90 % of moments.
- Tears/laughter F1 >= 0.85 with precision >= 0.90 (no false tears).
- Composure fairness: in ritual/vows fixtures, composed frames are not systematically ranked below smiling frames.
- Reaction linking: >= 80 % of human-identified reaction pairs found, < 10 % spurious links.
- Determinism: identical inputs produce identical scores; cloud results cached and reproducible.

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
| Expression + interaction per image (GPU) | <= 40 ms |
| 4,000 images total (RTX 4070) | <= 160 s |
| Peak + reaction linking for a whole wedding | <= 8 s |
| Storage per image | <= 900 B |

Telemetry events (local-first, opt-in aggregation):

- `emotion.scored` {images, ms, mean_score, interaction_histogram}
- `emotion.peaks` {moments, mean_margin}
- `emotion.reactions` {links, mean_bonus}
- `emotion.cloud_used` {calls, cost_usd}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Cultural bias toward Western expressiveness | Tradition-aware weights, balanced preference data, mandatory per-tradition agreement metrics, PM-approved rules. |
| Overclaiming emotion recognition | Product language describes photographic expression, not inner states; docs and UI copy reviewed by PM. |
| Subjectivity makes the model feel wrong to some photographers | Preference ranker is personalisable in Phase 30; users can weight emotion vs technical in settings. |
| False tear/crying detection embarrasses the product | High-precision thresholds, cross-check with interaction context, and no tear-based reason text unless confidence >= 0.85. |

## 13. Acceptance criteria

- [ ] Every face carries continuous expression values and gaze; every image carries interactions and an emotion score.
- [ ] Each moment identifies its peak frame with a margin, matching human choice in the large majority of cases.
- [ ] Reaction frames are linked to their action frames across cameras.
- [ ] Composed ritual frames are ranked fairly against smiling frames.
- [ ] The Emotion card explains the score with crops and short editorial reasons.
- [ ] Photographer agreement study meets the gate and is published internally.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 10 - Expression, Emotion & Moment Ranking AI.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-10-EMOTION-MOMENT-AI.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - The app finds the moments that matter: genuine smiles, laughter, tears, hugs, kisses, reactions and ritual peaks - and ranks every frame by emotional value.

Rules:
  - Do not start Phase 11. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-brain-wedding/src/emotion/{expression,gaze,interaction,peak,reaction,score}.rs`, `ml/models/emotion/{train_expression.py,train_interaction.py,train_ranker.py,eval_emotion.py}`, `crates/aura-catalog/migrations/0010_emotion.sql`, `config/emotion_weights.toml`, `apps/desktop/src/components/explain/EmotionCard.tsx`, `docs/model-cards/{expression_head,interaction_head}.md`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-10-emotion-moment-ai and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-10.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-10-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-10-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-10-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 10 of 30 - Expression, Emotion & Moment Ranking AI - part of the AURA Wedding AI master build plan.*
