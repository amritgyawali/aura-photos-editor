# Phase 10 progress - Expression, Emotion & Moment Ranking AI

One line per task group, in the order section 8 asks for them. The exit report is
`docs/progress/PHASE-10-EXIT.md`.

| Task | Files touched | Tests added | Notes |
|---|---|---|---|
| PM/CTO - the taxonomy and the cultural rules, before any code | `crates/aura-core/src/contract/emotion.rs`, `crates/aura-core/src/lib.rs`, `docs/adr/ADR-0021-*.md` | via `emotion_eval.rs` | `GazeTarget`, `Interaction`, `FaceExpression`, `EmotionCode`, `EmotionReason`, `PeakKind`, `MomentPeak`, `ReactionLink`, `ImageEmotion`, `Preference`, `EmotionOutline`, `EmotionService`. Eight spellings differ from section 5; ADR-0021 section 2 records all eight. The two that matter are `EmotionCode` as a **closed twenty-word vocabulary** - which is what makes "no psychological claims" structural rather than remembered - and `GazeTarget::Unknown`, so a missing measurement never reads as "looking away". |
| PM - the weight table and the cultural argument | `crates/aura-brain-wedding/config/emotion_weights.toml`, `crates/aura-brain-wedding/src/emotion/weights.rs` | `tests/config.rs` (+8) | 22 scene rows, 5 tradition rows, 9 ranker coefficients, 2 calibration tables, every one with a written rationale the loader refuses to do without. **In the four ceremony scenes composure is weighted at or above a smile**, and the Hindu, Nepali and Muslim multipliers raise it further. Eight refusal rules, `AURA-ML-5039`, and it is the one thing in phase 10 that halts. |
| DATA - labels and comparisons | - | - | **Not done.** Section 9's nine-day deliverable - expression targets, interaction boxes and 10k pairwise photographer comparisons across traditions - needs access to weddings and to photographers. Condition C2. The ranker is fitted on eight authored comparisons instead, and `train_ranker.py` prints which of the nine coefficients that data cannot identify. |
| SRML - the expression head | `crates/aura-infer/src/onnx/fixtures.rs`, `xtask/src/models.rs`, `models/expression_head_*`, `docs/model-cards/expression_head.md` | `cargo xtask models` | 112 px sRGB crop - the same one phases 06 and 09 read - into **eight independent sigmoids**. Not a softmax, and that is the whole design: section 2.1 requires the outputs to be continuous, and a face at a wedding can be laughing and crying at once. int8 **forbidden**, because one of the eight is read against a 0.85 threshold that decides whether the product says the word "crying". Untrained - condition C1. |
| SRML - the interaction head | `crates/aura-infer/src/onnx/fixtures.rs`, `xtask/src/models.rs`, `models/interaction_head_*`, `docs/model-cards/interaction_head.md` | `cargo xtask models` | 160 px frame in **four planes**: three of colour and one person prior painted from phase 06's face boxes. Section 6.2's "person boxes as spatial priors" as a plane rather than as coordinates, because a prior appended after the global pool arrives after every spatial decision has been made. int8 **permitted**, and the contrast with the expression head is deliberate. Untrained - condition C1. |
| MLR - gaze, measured rather than predicted | `crates/aura-brain-wedding/src/emotion/gaze.rs` | `emotion_eval.rs` (5) | All four of section 2.1's targets are relations between two positions in the frame, and phase 06 hands over both. The sign is the thing that is easy to get backwards - a head turned right moves the visible eyes *left* - and the eval harness checks it by name. Head direction rather than eye direction, which is condition C3. |
| MLR - peak detection | `crates/aura-brain-wedding/src/emotion/peak.rs` | `emotion_eval.rs` (6) | A 60/40 face-and-interaction curve, a three-tap kernel **reflected at the ends** so a bouquet toss peaking on the final frame is not pulled inwards, then an argmax and a margin. Below `MIN_MARGIN` the answer is `Flat` and `AURA-ML-5042` - because fourteen bracketed frames genuinely have no apex, and phase 29 builds album spreads around what this points at. |
| MLR - reaction linking | `crates/aura-brain-wedding/src/emotion/reaction.rs` | `emotion_eval.rs` (4) | Three conditions - a four-second window, not the same burst on the same camera, and an engaged expressive face - plus a resolver that gives each reaction exactly one action. The fourth condition section 6.3 implies, geometric direction between two cameras, is **deliberately absent**: two frames have no shared coordinate system and a claim about the direction between them would be invented. |
| SRC - the nine features, the ranker and the reasons | `crates/aura-brain-wedding/src/emotion/score.rs` | `emotion_eval.rs` (6) | A Bradley-Terry utility, linear, so the coefficients are a list a product manager can argue with and every reason names one of them. Prominence-weighted **means, not maxima**: a maximum would let one guest in row four decide a family portrait. Feature 6 duplicates a channel from feature 0 on purpose, so composure can be fixed in one coefficient rather than in twenty-three scene rows. |
| SRC - the pass, the store and the service | `crates/aura-brain-wedding/src/emotion/{analyse,store,api,fixtures}.rs`, `crates/aura-catalog/migrations/0010_emotion.sql` | the gate, `emotion_budgets.rs` (4) | One decode, two model calls, five tables, three version columns. Steps 3 to 5 of the pass - peaks, links, re-score - **open no file**, which is what makes a `weights_ver` bump the cheapest re-run in the phase. A photographer's peak choice is re-applied inside the upsert a re-analysis performs. |
| SRC - phase 09's tears rule | `crates/aura-brain-photo/src/integrity/{eyes,analyse,api}.rs` | `integrity_eval.rs` | `IntegrityPass::with_emotion` fills `IntentInput::tears` through `aura-core`'s frozen trait, so the two brain crates depend on each other in neither direction. `ANALYSIS_VER` 1 → 2, which makes every stored verdict pending. **Closes phase 09's condition C4.** |
| SRC - one warp instead of two | `crates/aura-vision/src/face/align.rs`, `crates/aura-brain-photo/src/integrity/eyes.rs` | phase 09's 37 tests, unchanged | The 112 px two-point warp moved into `aura-vision` when phase 10 became its second consumer. Phase 09's functions delegate and keep their names; its 26 eval gates and 11 calibration tests pass unchanged, which is what makes it a de-duplication rather than a change. |
| AGT - the cloud task | `crates/aura-cloud/src/moment_significance.rs`, `crates/aura-cloud/src/lib.rs` | via the schema validator | Six 768 px thumbnails, ≤ 25 calls a wedding, cached, and a local fallback that returns `significance = 0` rather than a guess. Three things are structural rather than instructed: subjects are six anonymous role handles, the output has no field a description of a person could go in, and `validate` refuses a reason containing any of twenty banned appearance and psychology words. |
| SFE/MFE - the Emotion card and the moment browser | `ui/src/components/explain/{EmotionCard,MomentBrowser}.tsx`, `ui/src/ipc/{types,client}.ts`, `crates/aura-app/src/{emotion_commands,contract/ipc,state}.rs`, `docs/adr/ADR-0022-*.md` | `EmotionCard.test.tsx` (12) | Seven commands, five reads. The browser says "An ordering, not a shortlist" in its own header and has no checkbox, star or export; a test asserts no score label contains `keep`, `reject`, `deliver` or `cull`. The card draws "not read" differently from "nothing happening", and the tear threshold is computed in Rust and sent rather than copied into TypeScript. |
| QAL/PERF - gates, budgets and the eval harnesses | `crates/aura-cli/src/phase10.rs`, `crates/aura-perf/tests/emotion_budgets.rs`, `perf/budgets.toml`, `tests/eval/emotion_eval.rs`, `ml/models/emotion/*.py`, `justfile` | `emotion_eval.rs` (38), `emotion_budgets.rs` (4) | `just phase-10-verify` runs eleven checks and exits 0, with both heads through the real inference service at 31 ms a frame. **733 bytes per image against a 900 B budget**; peaks and links for a whole wedding in **13 ms against 8 s**. The two GPU rows are waived with an expiry condition. |
| DOC - runbooks, ADRs and the reason reference | `docs/runbooks/AURA-ML-504{2}.md` and 5038-5041, `docs/adr/ADR-002{1,2}-*.md`, `docs/emotion-and-moments.md`, `docs/progress/PHASE-10*.md`, `CHANGELOG.md`, `CLAUDE.md` | - | Five new codes, five runbooks, two ADRs, and a reason reference in user language whose first section is titled "AURA describes photographs. It does not read minds." |

## Six things the harness, the gate and the budget found that review would not have

Recorded because every one was a real defect in code that read correctly, and because four
of the six were only reachable by measuring something.

1. **The tear gate could never fire.** `FaceExpression::reads_as_crying` required both
   `tears >= 0.85` *and* `confidence >= 0.85`, and `confidence` is the mean distance of all
   eight channels from a half. A face that is emphatically crying and unremarkable in every
   other respect scores about 0.71 on that number - so the product could only have said the
   word "crying" about a face that was also emphatically something else. Every tear in the
   product would have been silently suppressed, **including phase 09's third intent rule**,
   which would have made the C4 fix inert on the day it shipped. The eval harness caught it
   on the first run of `tears_and_laughter_f1_on_the_painted_set`.

2. **The inverted-ranker guard did not guard.** `an_inverted_ranker_scores_below_a_coin_toss`
   negated three of the nine coefficients and passed anyway, because the other six were
   still pulling the right way. A guard that cannot fail is worse than no guard, and this
   one had been written to look like phase 09's without doing its job. Negating all nine
   made it fail correctly, and then pass.

3. **`face_expression` referenced a column that does not exist.** Migration 6 names the
   faces table's key `id`; the first draft of migration 10 wrote `REFERENCES faces(face_id)`
   - the *exact* mistake phase 09's storage budget found in `face_eye_state`, made again by
   somebody who had read the comment warning about it. It was caught before it could reach a
   catalog only because migration 10's comment on that line quotes phase 09's finding, and
   writing that comment is what prompted the check. **Two phases running.**

4. **The gate seeded a moment the database refused.** Migration 8's `CHECK (confidence <= 0.0
   OR length(reasons) > 2)` is invariant 2 as a constraint, and the phase 10 gate seeded a
   moment with a confidence of 0.9 and the default empty `reasons`. A seeded row is not
   exempt from a constraint that exists to stop a decision shipping without an explanation,
   and the gate now writes one.

5. **`catalog.count` refused all five new tables.** The allow-list in `aura-catalog` is
   per-phase and had not been extended, so the gate's first check reported five failures
   that were entirely about the gate. Cheap to fix and worth recording: a per-phase
   allow-list is a thing every future phase will forget once.

6. **The ranker's rationale claimed more than the fit supports.** Running
   `train_ranker.py --fixtures` showed that four of the nine coefficients -
   `interaction`, `peak`, `reaction`, `mutual_gaze` - do not vary between the two frames of
   any authored comparison, so the fit leaves them at zero. They are *unidentifiable* from
   that data rather than unimportant. The shipped rationale said "fitted"; it now says which
   five were fitted, which four were argued, and what would identify all nine. The numbers
   did not change; the claim about them did.

## What is deliberately not built

Section 2.2's three exclusions, each kept structurally rather than by discipline:

* **Final selection** is phase 12. No table, column, command or UI control in this phase
  keeps, rejects or delivers a photograph. `EmotionService::ranked` returns an ordering
  and there is no field on its return type a selection could be expressed in.
* **Album sequencing and hero picks** are phase 29. `MomentPeak` names the strongest frame
  of a moment and says nothing about sequence.
* **Any claim about a person's inner emotional state** is out of scope permanently.
  `EmotionCode` is a closed set of twenty sentences, call sites do not write sentences, and
  the one cloud task's validator refuses twenty appearance and psychology words in a reason.
