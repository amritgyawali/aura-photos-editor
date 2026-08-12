# Phase 29 - Curation Intelligence: B&W Selection, Hero Photos, Album Story & Social Picks

> **Single feature shipped by this phase:** After the gallery is finished, the app curates it: which frames sing in black and white, which are portfolio heroes, how the album should be sequenced, and which images to post.
>
> **Mission:** Turn a finished gallery into deliverables that make the photographer money - album drafts, portfolio picks and social sets - using the story graph the product already understands.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 29 of 30 |
| Epic | E6 - Curation & Delivery |
| Feature | After the gallery is finished, the app curates it: which frames sing in black and white, which are portfolio heroes, how the album should be sequenced, and which images to post. |
| Depends on | Phases 07-12, 25, 27 |
| Unlocks | Phase 30 |
| Duration | 2.5 weeks |
| Primary owners | ML Lead - Vision, AI Agent & Prompt Engineer, Product Manager Agent, Senior Engineer - Core Pipeline (Rust) |
| Risk level | Medium |
| Headline KPI | hero-pick agreement with photographers >= 0.75 top-20; album sequence accepted with <= 15 % reordering; B&W picks accepted >= 70 % |
| Competitor being beaten | Aftershoot/Imagen have none of this; album software has no wedding understanding |

## 1. Why this phase exists

Culling and editing save time; curation makes money. Album sales, portfolio updates and social posting are revenue activities that photographers routinely postpone for months.

Because the product already has scenes, moments, emotions, people and quality scores, curation is nearly free capability - the highest return per engineering hour in the roadmap.

## 2. Scope contract

### 2.1 In scope

- B&W suitability model: identifies frames that gain from monochrome (strong tonal separation, gesture-led, distracting colour, high emotion, grain-tolerant) and generates a tailored B&W mix per frame rather than a single preset.
- Hero photo selection: portfolio-grade picks balancing technical excellence, emotional peak, composition, uniqueness and story importance, with per-chapter diversity.
- Album Story AI: propose a sequenced album (default 60-120 images) that follows the wedding narrative, alternates wide/medium/tight rhythm, pairs facing-page images by tone and subject, guarantees coverage of must-haves and key people, and respects spread capacity.
- Spread pairing: for each spread, choose images that work together (complementary tone, matching direction of gaze/movement, no clashing colour) - the part album designers spend the most time on.
- Social selection: Instagram-ready sets (grid of 10, story set, single hero) with aspect variants from Phase 23 and caption suggestions grounded in the actual story graph.
- Client-preview set: a small teaser set (15-30 images) chosen for immediate delivery on the wedding night.
- Curation UI: drag-to-reorder album, spread view, accept/replace suggestions, export to album software formats (JSON/CSV/PSD-ready layer lists) and to social scheduling.
- Everything explained: why this image is a hero, why this spread pairs, why this frame suits B&W.

### 2.2 Explicitly out of scope (do not build it here)

- Album page layout rendering and printing (export a spec, not a printed book).
- Direct posting to social platforms (Phase 30 handles integrations).
- Client selection workflows (post-V1).

## 3. Architecture and data flow

```text
finished gallery + story graph + emotion + quality + people + consistency
     |
     +--> BwSuitability -> candidates + per-frame B&W mix
     +--> HeroSelector -> portfolio picks (diverse across chapters)
     +--> AlbumComposer -> ordered sequence -> spread pairing -> coverage guarantee
     +--> SocialSelector -> grid set / story set / hero + aspect variants + captions
     +--> TeaserSelector -> 15-30 image preview set
                       |
            explanations for every pick + export specs (JSON/CSV/PSD list)
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-curate/src/{lib,bw,hero,album,spread,social,teaser,explain,export}.rs` | Curation engine. |
| `ml/models/curate/{train_bw.py,train_hero.py,eval_curate.py}` | Learned suitability and hero ranking. |
| `config/curation.toml` | Album sizes, rhythm rules, social formats, teaser policy. |
| `apps/desktop/src/routes/curate/{AlbumBuilder,SpreadView,HeroGrid,SocialSets,BwPicks}.tsx` | Curation UI. |
| `docs/curation.md` | How curation decisions are made. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Curation outputs**

```rust
pub struct CurationResult {
    pub bw: Vec<(ImageId, BwMix, f32)>,          // suitability score
    pub heroes: Vec<(ImageId, f32, Vec<Reason>)>,
    pub album: AlbumPlan,
    pub social: SocialSets,
    pub teaser: Vec<ImageId>,
}

pub struct AlbumPlan {
    pub spreads: Vec<Spread>,                    // { left: Option<ImageId>, right: Option<ImageId>, single: bool }
    pub chapter_map: Vec<(ChapterId, Range<usize>)>,
    pub coverage: CoverageReport,
    pub rhythm_score: f32, pub pairing_score: f32,
    pub reasons: Vec<Reason>,
}

pub struct SocialSets {
    pub grid: Vec<(ImageId, AspectVariant)>,     // 10 images
    pub story: Vec<(ImageId, AspectVariant)>,
    pub hero: (ImageId, AspectVariant),
    pub captions: Vec<(ImageId, String)>,        // grounded in story graph
}
```

## 6. Algorithm, model and implementation design

### 6.1 B&W suitability

- Score on tonal separation (histogram spread after desaturation), colour distraction (saturated non-subject regions), gesture strength (interaction detected), emotional intensity and noise character (grain reads well in mono).
- Generate a per-frame channel mix that maximises subject separation rather than applying one preset - a red-heavy mix for warm skin against green foliage, a blue-heavy mix for pale sky backgrounds.
- Present as suggestions, never applied automatically to the main gallery (B&W is a taste decision), except in a dedicated B&W set.

### 6.2 Hero selection with diversity

- Rank by a weighted blend of technical, emotion, composition, uniqueness (embedding distance from other picks) and story importance.
- Enforce diversity: at most N heroes per chapter, at most one per moment, and a spread across framing types, so the portfolio set is not eight versions of the kiss.
- Uniqueness uses the Phase 05 index, which is why heroes feel like a curated set rather than a top-scoring list.

### 6.3 Album composition as constrained sequencing

- Start from chapter order, allocate spread counts proportionally to chapter importance and duration, then fill with the highest-value images subject to coverage rules.
- Rhythm: alternate wide establishing, medium action and tight emotional frames using a target pattern per chapter; measure the rhythm score and improve by local swaps.
- Spread pairing objective: similar tonal weight, compatible colour temperature after consistency, complementary gaze/movement direction (subjects looking inward, not off the spread), and no two near-identical frames facing each other.
- Guarantee coverage: must-have moments and close-family members appear in the album, not just in the gallery - reusing the Phase 12 coverage engine.

### 6.4 Social and teaser sets

- Grid set balances one hero, two portraits, two details, two candids, two family/group and one exit-style frame, chosen for thumbnail legibility (strong subject, clear silhouette at small size).
- Captions are generated from the story graph (chapter, ritual, people roles anonymised) and are grounded - the model may not invent details about the couple.
- Teaser set is optimised for immediate emotional impact and fast delivery: hero, couple, ceremony peak, one family, one detail, one dance.

## 7. Cloud AI usage (bring-your-own API key)

**Album sequencing refinement and caption drafting**

| Aspect | Specification |
|---|---|
| Model class | Reasoning tier with vision, temperature 0 |
| Trigger | Once per album draft (and on user request), plus one batched call for captions |
| Input sent | Contact sheets per chapter (thumbnails, 512 px), chapter labels, spread capacity, rhythm targets, current draft order |
| Cost control | <= 15 calls per wedding; cached |
| Offline fallback | Deterministic rhythm-and-pairing optimiser only (fully functional offline) |

System prompt contract:

```text
You are an album designer sequencing a wedding album.
Input: chapter contact sheets, the current draft order, spread capacity and rhythm targets.
Task: propose swaps or moves that improve narrative flow and spread pairing, and draft one short caption per chapter.
Rules:
- Preserve chronological chapter order; only reorder within chapters or move an image between adjacent spreads.
- Pair images that share tonal weight and whose subjects face inward.
- Captions must be factual from the supplied chapter/ritual labels. Never invent names, vows, relationships or places.
- Keep captions under 12 words, warm but not sentimental.
- Return ONLY JSON matching the schema.
```

Required JSON response schema (validated; invalid = retry once, then fall back to local model):

```json
{
  "type": "object",
  "required": ["moves", "captions", "confidence"],
  "properties": {
    "moves": {
      "type": "array", "maxItems": 20,
      "items": {
        "type": "object",
        "required": ["from_index", "to_index", "reason"],
        "properties": {
          "from_index": { "type": "integer" },
          "to_index": { "type": "integer" },
          "reason": { "type": "string" }
        },
        "additionalProperties": false
      }
    },
    "captions": {
      "type": "array", "maxItems": 24,
      "items": {
        "type": "object",
        "required": ["chapter", "caption"],
        "properties": { "chapter": { "type": "string" }, "caption": { "type": "string", "maxLength": 90 } },
        "additionalProperties": false
      }
    },
    "confidence": { "type": "number", "minimum": 0, "maximum": 1 }
  },
  "additionalProperties": false
}
```

## 8. Implementation order (execute literally, in this order)

1. Collect photographer-labelled hero picks, album sequences and B&W choices from real deliveries.
2. Train the B&W suitability model and implement per-frame mix generation.
3. Train the hero ranker and implement diversity constraints.
4. Implement album allocation, rhythm optimisation and spread pairing.
5. Reuse the coverage engine to guarantee album coverage.
6. Implement social sets, thumbnail legibility scoring and the teaser set.
7. Add the cloud sequencing/caption task with strict grounding.
8. Build the curation UI: album builder, spread view, hero grid, social sets, B&W picks.
9. Implement export specs for album software and social scheduling.
10. Run agreement studies with photographers on heroes, sequences and B&W picks.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `MLL` | ML Lead - Vision | Own B&W and hero models, diversity constraints and agreement evaluation | Signed spec + gates | 4 d |
| `SRML` | Senior ML Engineer | Train B&W suitability and hero ranker; export and calibrate | Models registered | 6 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Album composer, rhythm/pairing optimiser, social/teaser selectors, export specs | `aura-curate` + tests | 8 d |
| `AGT` | AI Agent & Prompt Engineer | Sequencing/caption cloud task with grounding rules and cassettes | Cloud path live | 3 d |
| `DATA` | Data Engineer / Dataset Curator | Collect 60 real album sequences, hero sets and B&W selections with permission | Curation dataset | 7 d |
| `PM` | Product Manager Agent | Own `curation.toml` (album sizes, rhythm, social formats) and caption tone rules | Approved config | 3 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Album builder with spread view and drag-to-reorder; hero grid; social sets | Curation UI | 7 d |
| `MFE` | Mid-Level Frontend Engineer | B&W picks panel, caption editor, export dialogs, aspect variant switcher | UI panels | 4 d |
| `QAL` | QA Lead - Automation | Agreement gates, coverage-in-album test, pairing property tests, grounding checks | CI gates | 4 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Agreement study with 5 photographers on heroes, album order and B&W picks | Study report | 5 d |
| `PERF` | Performance Engineer | Keep curation under 20 s per gallery; incremental re-composition on edits | Benchmark | 2 d |
| `DOC` | Technical Writer | Document curation logic and album export formats | Docs merged | 2 d |

### 9.1 Handoff chain for this phase

```text
DATA real albums/heroes -> SRML models -> SRC composer/optimiser
                                     |
                                     v
                          AGT sequencing + captions -> SFE/MFE curation UI
                                     |
                    QAL gates + QAIQ agreement study -> MLL/PM gate
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

- Hero agreement >= 0.75 on top-20 overlap with photographer picks.
- Album sequence accepted with <= 15 % of images reordered by photographers in the study.
- B&W picks accepted >= 70 %; generated mixes rated better than a fixed preset.
- Album coverage: every must-have moment and close-family member appears.
- Spread pairing property tests: no facing near-duplicates, no clashing tonal weight beyond threshold.
- Captions contain no invented names, places or claims (automated grounding check).
- Offline: curation works fully without cloud, using the deterministic optimiser.

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
| Full curation for a 1,000-image gallery | <= 20 s |
| Album re-composition after a swap | <= 1.5 s |
| B&W mix generation per image | <= 25 ms |

Telemetry events (local-first, opt-in aggregation):

- `curate.album` {spreads, rhythm_score, pairing_score, ms, cloud_used}
- `curate.heroes` {count, mean_score, chapters_covered}
- `curate.user_reorder` {moves, album_size}
- `curate.bw_accepted` {suggested, accepted}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Curation is taste-heavy and may feel wrong | Agreement studies, per-photographer personalisation via Phase 30's learning loop, and effortless manual reordering. |
| Captions invent facts | Strict grounding rules, automated checks, and human review before posting. |
| Album misses someone important | Coverage engine reuse with an explicit album-coverage test. |
| Scope creep into album layout design | Export a specification for album software rather than building a page designer. |

## 13. Acceptance criteria

- [ ] The app proposes hero photos, a sequenced album with paired spreads, social sets and a teaser set.
- [ ] B&W suggestions come with per-frame mixes, not a single preset.
- [ ] Album coverage of must-haves and close family is guaranteed.
- [ ] Every pick is explained; reordering is instant and remembered.
- [ ] Album and social specs export cleanly to external tools.
- [ ] Photographer agreement studies meet the gates.

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
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 29 - Curation Intelligence: B&W Selection, Hero Photos, Album Story & Social Picks.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-29-CURATION-INTELLIGENCE.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - After the gallery is finished, the app curates it: which frames sing in black and white, which are portfolio heroes, how the album should be sequenced, and which images to post.

Rules:
  - Do not start Phase 30. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-curate/src/{lib,bw,hero,album,spread,social,teaser,explain,export}.rs`, `ml/models/curate/{train_bw.py,train_hero.py,eval_curate.py}`, `config/curation.toml`, `apps/desktop/src/routes/curate/{AlbumBuilder,SpreadView,HeroGrid,SocialSets,BwPicks}.tsx`, `docs/curation.md`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-29-curation-intelligence and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-29.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-29-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-29-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-29-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 29 of 30 - Curation Intelligence: B&W Selection, Hero Photos, Album Story & Social Picks - part of the AURA Wedding AI master build plan.*
