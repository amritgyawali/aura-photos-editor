# Phase 07 progress - Wedding Scene AI & Story Timeline Segmentation

One line per task group, in the order section 8 asks for them. The exit report is
`docs/progress/PHASE-07-EXIT.md`.

| Task | Files touched | Tests added | Notes |
|---|---|---|---|
| PM/MLL - the taxonomies and the profile table, authored first | `crates/aura-brain-wedding/config/scene_profiles.toml`, `crates/aura-brain-wedding/config/rituals/{hindu,nepali,christian,muslim,civil}.toml` | `crates/aura-brain-wedding/tests/config.rs` (27) | Section 8 step 1 and step 7. Forty-eight rites across five traditions; twenty-two profiles, every one with a rationale a photographer would agree with. The loader refuses a profile without one - `AURA-ML-5024` - which is intentional friction, not validation theatre. |
| CTO - freeze section 5 before any code | `crates/aura-core/src/contract/scene.rs`, `crates/aura-core/src/contract/ids.rs`, `crates/aura-core/src/lib.rs`, `docs/adr/ADR-0015-*.md` | in `config.rs` | `SceneId`, `AttrFlags`, `RitualId`, `ChapterId`, `SceneResult`, `Segment`, `SceneProfile`, `StoryOutline`, `StoryService`, plus `SegmentId`. In `aura-core` for ADR-0013's reason applied to a second vocabulary: ten later phases need the word `ceremony` and none of them needs the classifier. Five spellings differ from the phase document; ADR-0015 section 2 records all five. |
| SRC - migration 7 and the store | `crates/aura-catalog/migrations/0007_scenes.sql`, `crates/aura-catalog/src/{migrate,lib}.rs`, `crates/aura-brain-wedding/src/story/segment.rs` | in the gate | Four tables, two views, four version columns. `source <> 'user'` and `user_locked` are checked *inside* the statements that would overwrite them, exactly as `identities.user_locked` is in migration 6 - a read-then-write leaves a window in which a photographer loses a race with a background pass. |
| SRML - the multi-head classifier | `crates/aura-infer/src/onnx/fixtures.rs`, `crates/aura-brain-wedding/src/scene/{classifier,attributes}.rs`, `xtask/src/models.rs`, `docs/model-cards/scene_classifier.md` | in `tests/eval/scene_eval.rs` | An adapter on the frozen phase 05 trunk, so it takes a feature vector and not pixels - which is why `xtask` gained a non-image `InputSpec`. The abstention is a decoder rule, not a softmax slot, and the margin is what actually rejects. Four attributes are decided from EXIF and luminance rather than predicted. |
| SRML - the ritual head with abstention | `crates/aura-brain-wedding/src/scene/{ritual,taxonomy}.rs`, `docs/model-cards/ritual_classifier.md`, `docs/adding-a-tradition.md` | in `config.rs`, `scene_eval.rs` | The 160 output slots **are** the ids authored in the taxonomy files, which is why a duplicate id is a refusal. Masking by tradition renormalises the surviving distribution - without that, establishing the tradition made the head *less* likely to name a rite, which the eval harness caught. |
| MLR - HMM smoothing | `crates/aura-brain-wedding/src/story/hmm.rs` | in `scene_eval.rs` | Viterbi over nine chapters, not twenty-two scenes: 484 authored numbers is a matrix nobody would review. No entry is zero except leaving `Other`, because a hard zero is a claim that a wedding cannot do something. |
| MLR - change-point detection | `crates/aura-brain-wedding/src/story/changepoint.rs` | in `scene_eval.rs` | PELT over a three-term fused signal, with the penalty **searched in log space**. Linear bisection of `0.0005..40` never reaches the bottom two decades, so a wedding whose answer was 0.008 fell back for no reason but arithmetic - found by the eval harness, fixed in the search. |
| SRC - key frames, the segment store and the service | `crates/aura-brain-wedding/src/story/{keyframe,api}.rs`, `crates/aura-brain-wedding/src/scene/pass.rs` | in `scene_eval.rs`, the gate | Medoid among eligible frames, with the filter relaxing in three recorded steps so a Details chapter can say its cover is arbitrary. `Story::segment` runs six steps and smooths before it segments; ADR-0015 section 6 settles the order the phase document draws ambiguously. |
| SRC - scene labels reach the people graph | `crates/aura-people/src/{store,api}.rs` | in the gate | `PeopleStore::scene_labels` plus `SceneId::role_label`. Half of phase 06's condition C3 closes: `RoleOutcome::scene_starved` is false on a classified wedding and `SCENELESS_CONFIDENCE_CEILING` stops capping the couple decision at 0.62. |
| AGT - the cloud naming policy | `crates/aura-brain-wedding/src/story/naming.rs` | in the gate | `SegmentNaming` is phase 04's and is unchanged; ADR-0015 section 5 records why `chapter`/`split_at_index` are not renamed. What is new is the policy: sixteen calls per wedding, least-confident first, locked chapters never priced, and phase 04's 0.90 authority rule enforced with the conflict logged. |
| SFE/MFE - the story timeline | `ui/src/components/story/{Timeline,ChapterStrip,BoundaryEditor}.tsx`, `ui/src/ipc/{types,client}.ts`, `crates/aura-app/src/{story_commands,contract/ipc,state}.rs`, `docs/adr/ADR-0016-*.md` | `ui/src/components/story/Timeline.test.tsx` (23) | Nine commands, thirteen types. Chapter cards are sized by **duration**, not by frame count - a ninety-minute dinner and a six-minute cake with forty frames each are not the same shape of event. A boundary edit locks both chapters, because a boundary is shared. |
| QAL - gates, budgets and the eval harness | `crates/aura-cli/src/phase07.rs`, `crates/aura-perf/tests/scene_budgets.rs`, `perf/budgets.toml`, `tests/eval/scene_eval.rs`, `justfile` | `scene_eval.rs` (18), `scene_budgets.rs` (6) | `just phase-07-verify` runs thirteen checks and exits 0. All four of section 11's budget rows are asserted and none is waived - the first phase since 02 where that is true, because nothing here needs a GPU. The store budget was 410 B against a 400 B budget until the top-3 encoding was compacted. |
| DOC - cards, runbooks and the exit report | `docs/model-cards/{scene_classifier,ritual_classifier}.md`, `docs/runbooks/AURA-ML-502{2..7}.md`, `docs/adding-a-tradition.md`, `docs/adr/ADR-001{5,6}-*.md`, `docs/progress/PHASE-07*.md`, `CHANGELOG.md`, `CLAUDE.md`, `ml/models/scene/*.py` | `dataset.py` and `eval_scene.py` self-tests | Six new codes, six runbooks, two model cards with an honest fairness section that names *cultural* rather than demographic disparity, two ADRs, and a procedure a consultant can follow to add a tradition without a compiler. |

## Three things the harness found that review would not have

Recorded because each one was a real bug in code that read correctly.

1. **The penalty search never reached its own range.** Linear bisection of `0.0005..40`
   spends its first ten steps between 40 and 0.04. The Nepali fixture's answer is 0.008,
   so it fell back to gap-only segmentation and produced three chapters against a
   six-chapter floor. The search is logarithmic now.
2. **Masking made the ritual head abstain more, not less.** Zeroing another tradition's
   slots without renormalising left the surviving distribution summing to less than one,
   so the confidence floor rejected an answer that establishing the tradition should have
   made *easier*. Exactly backwards, and invisible without a test that asserted the
   direction.
3. **The storage estimate was 25 % low.** The migration's comment claimed 330 bytes per
   image; the catalog said 410 against a 400-byte budget. The 48 bytes that closed it
   came from writing the top-3 as pairs rather than as objects - the words "scene" and
   "score", repeated three times per photograph, were a fifth of the whole budget.

## One thing this phase did not fix

`people_budgets::clustering_a_full_skeleton_stays_inside_the_budget` fails on this
machine at 21.7 s against a 12 s budget, where `docs/progress/PHASE-06-EXIT.md` records
2.1 s. It was ruled out as a phase 07 effect by measurement - the same 21.7 s with
`aura-brain-wedding` removed from `aura-perf`'s dev-dependencies entirely - and it is
recorded in section 4 of the phase 07 exit report rather than repaired here. Changing a
phase 06 budget from inside phase 07 is not this phase's call.
