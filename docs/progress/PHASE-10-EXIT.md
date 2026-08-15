# Phase 10 exit report - Expression, Emotion & Moment Ranking AI

**Date:** 2026-08-15
**Branch:** `feat/phase-09-frame-integrity-ai` (phase 10 landed on the same branch)
**Gate:** `just phase-10-verify` exits 0
**Verdict:** the phase is implemented and **conditionally** complete. Five conditions are
open, they are listed in section 5, and **C1 is a Sev 2 trigger**.

---

## 1. What shipped

One feature: the app finds the moments that matter - genuine smiles, laughter, tears, hugs,
kisses, reactions and ritual peaks - and ranks every frame by emotional value.

| Area | What landed |
|---|---|
| Migration 10 | `image_interaction`, `face_expression`, `moment_peak`, `reaction_links`, `emotion_preferences`, and two views |
| `aura-core` | the frozen section 5 contract - `GazeTarget`, `Interaction`, `FaceExpression`, `EmotionCode`, `EmotionReason`, `PeakKind`, `MomentPeak`, `ReactionLink`, `ImageEmotion`, `Preference`, `EmotionOutline`, `EmotionService` |
| `aura-brain-wedding::emotion` | the expression head runner, geometric gaze, the interaction head with a person-prior plane, the peak curve, reaction linking, the nine features and the Bradley-Terry ranker, the weight-table loader, the store, the resumable pass and the synthetic ground truth |
| Config | `emotion_weights.toml`: 22 scenes, 5 traditions, 9 coefficients and 2 calibration tables, every row with a written rationale |
| Models | `expression_head` and `interaction_head`, signed into `models.lock` with cards |
| Cloud | `MomentSignificance`: six thumbnails, anonymised role handles, twenty banned words in a validator |
| IPC and UI | seven commands, fourteen types, the Emotion card and the moment browser |
| Gate | `aura-cli verify --phase 10`, eleven checks, exit 0 |

**Five new error codes**, each with a runbook: `AURA-ML-5038` to `AURA-ML-5042`.

**Two ADRs**: ADR-0021 (the taxonomy, the ranker and the cultural rules) and ADR-0022 (the
emotion IPC surface).

**No frozen contract was amended.** Phase 09 amended `FaceRef`; this phase needed nothing
that amendment did not already supply, which is the clearest evidence that it was the right
amendment.

---

## 2. Acceptance criteria (section 13)

| # | Criterion | Status | Evidence |
|---|---|---|---|
| 1 | Every face carries continuous expression values and gaze; every image carries interactions and an emotion score | **met, with C1** | gate: `7 of 7 frames scored`, `every frame carries a score, a confidence and at least one reason`; `the_painted_expression_survives_the_warp` recovers all eight channels within 0.05 through the real alignment |
| 2 | Each moment identifies its peak frame with a margin, matching human choice in the large majority of cases | **met, with C1** | `the_peak_is_in_the_human_top_two_across_a_range_of_shapes`: **1.000** over 72 synthetic moments against a 0.90 gate; the gate confirms a rising run peaks at frame 3 and a flat run refuses |
| 3 | Reaction frames are linked to their action frames across cameras | **met** | `reaction_recall_and_spuriousness_on_a_built_scenario`: recall **1.000** against 0.80, spurious **0.000** against 0.10; gate: `one cross-camera link found, and the same-burst frame was not linked` |
| 4 | Composed ritual frames are ranked fairly against smiling frames | **met** | `composed_frames_are_not_ranked_below_smiling_ones_in_ritual_scenes` over three rites; `composure_is_weighted_at_or_above_a_smile_in_every_ceremony_scene` over all 23 scenes; gate: `3 of 3 rites` end to end, and `4 of 4 ceremony scenes` in the table |
| 5 | The Emotion card explains the score with crops and short editorial reasons | **met** | gate: `7 reasons across the set carry a face crop`; `EmotionCard.test.tsx` asserts the crop positioning is in percentages so one rectangle fits every preview size |
| 6 | Photographer agreement study meets the gate and is published internally | **not met - C2** | `pairwise_agreement_on_the_authored_comparison_set`: **1.000** over eight comparisons against a 0.80 gate. Authored, not photographers'. There is no blind study and no five photographers |

---

## 3. Section 10.1's gates

Measured by `tests/eval/emotion_eval.rs` (38 tests) and by the phase gate.

| Gate | Threshold | Result | Against |
|---|---|---|---|
| Pairwise agreement | >= 0.80 | **1.000** | eight authored comparisons - **C2** |
| Peak in the human top two | >= 0.90 | **1.000** | 72 synthetic moments, six lengths x four apex positions x three steps |
| Tears F1 | >= 0.85 | **1.000** | seven painted expression profiles |
| Tears precision | >= 0.90 | **1.000** | the same seven |
| Laughter F1 | >= 0.85 | **1.000** | the same seven |
| Composure fairness | not systematically lower | **met**, 3 of 3 rites end to end, 23 of 23 scenes in the table | `fixtures::composed` against `fixtures::smiling` |
| Reaction recall | >= 0.80 | **1.000** | three genuine reactors and three distractors, one failing each condition |
| Reaction spuriousness | < 0.10 | **0.000** | the same scenario |
| Determinism | identical | **met** | two reads, gate and harness |
| A constant reader fails the agreement gate | - | **met**, 0.000 | the guard |
| An inverted ranker scores below a coin toss | - | **met** | the guard |
| A flag-everything tear detector fails on precision | - | **met**, 0.100 | `eval_emotion.py --self-test` |
| A link-everything linker fails on spuriousness | - | **met**, 0.900 | `eval_emotion.py --self-test` |

**Read the "against" column.** Every number above is a real measurement of the
*algorithms* - the two-point warp, the gaze geometry, the prominence weighting, the scene
and tradition table, the nine features, the Bradley-Terry utility, the smoothing kernel,
the margin, the three reaction conditions, the resolver and the tear certainty gate -
against ground truth painted into the pixels or known by construction.

**None of them is a measurement of the two shipped heads' weights**, which are
placeholders. That is condition C1. And the first row is measured against *authored*
preferences rather than photographers', which is condition C2 and is a second and different
kind of gap: C1 is a missing model, C2 is a missing opinion.

The last four rows are the guard phases 06 to 09 each wrote. An agreement score is
specifically vulnerable to a ranker that returns a constant, and the first version of the
inverted-ranker guard did not catch one - see `docs/progress/PHASE-10.md`, finding 2.

### What the harness, the gate and the budget found that review did not

Six real defects, in code that read correctly. Four were only reachable by measuring
something. `docs/progress/PHASE-10.md` carries all six in full; the two most consequential:

1. **The tear gate could never fire.** `reads_as_crying` required the *whole face's*
   confidence above 0.85, and that number is the mean distance of eight channels from a
   half - so a face that is emphatically crying and unremarkable otherwise scores 0.71.
   Every tear in the product would have been silently suppressed, **including phase 09's
   third intent rule**, which would have made the C4 fix inert on the day it shipped.

2. **`face_expression` referenced a column that does not exist.** `REFERENCES
   faces(face_id)` where migration 6 names it `id` - the *exact* mistake phase 09's
   storage budget found in `face_eye_state`, made again by somebody who had read the
   comment warning about it. It was caught before it reached a catalog only because
   migration 10's comment on that line quotes phase 09's finding.

---

## 4. Section 11's budgets

Measured in release on the development machine (Intel i5-10300H, 8 GB, Win 11), asserted by
`crates/aura-perf/tests/emotion_budgets.rs`.

| Row | Section 11 | This build | Status |
|---|---|---|---|
| Expression + interaction per image (GPU) | <= 40 ms | - | **waived**, no GPU backend - ADR-0021 section 9 |
| 4,000 images (RTX 4070) | <= 160 s | - | **waived**, same reason; the gate measures **124 to 156 s** on the processor path across runs |
| Peak + reaction linking for a whole wedding | <= 8 s | **13 ms** | **met** |
| Storage per image | <= 900 B | **733 B** | **met** |

### The GPU waiver

The same waiver ADR-0007 attaches to phase 03's throughput rows and ADR-0019 to phase 09's,
for the same reason: this build has no GPU backend, and a budget nothing can measure is a
wish rather than an assertion. Signed by PERF and CTO, expiring when a GPU backend lands.

What *is* asserted is the path this build runs. Two numbers, and the difference between
them is worth understanding:

* **12 ms per frame** in `emotion_budgets`, with the heads planted. That is the arithmetic
  around them - one 112 px warp per face, the 160 px four-plane resample, the gaze
  geometry, the nine features, the ranker and the reasons.
* **31 to 39 ms per frame** in the gate, with both heads through the real interpreter,
  across repeated runs on an otherwise-busy machine. The difference from the first figure
  is phase 03's pure-Rust ONNX interpreter, and a trained head of the same architecture
  costs the same arithmetic.

Thirty-nine milliseconds a frame is 156 s for four thousand images, which is inside section
11's 160 s - **on a processor, against a budget written for an RTX 4070**. That is not a
claim about the GPU row, and the margin is thin enough that it should not be read as one:
it is a note that the phase was designed cheaply enough that the waiver may turn out not to
have mattered, measured on one laptop, twice.

### The storage row, met by three decisions the budget forced

The measured split is **462 bytes for the reading row and its index, 270 for three
expressions**. Section 11's 900 bytes is what made each of the following worth doing:

| Decision | Why |
|---|---|
| The eight expression channels are **per-mille integers**, not REALs | A REAL is eight bytes whatever it holds; a small integer is one or two. Eight REALs is 141 bytes a photograph at 2.2 faces a frame - a sixth of the budget - and an expression reading has nothing like sixteen significant digits in it |
| `face_expression` carries no identity, no photo id, no timestamp and no rectangle | All four are in `faces`, indexed. Measured at 211 bytes a photograph to store a second copy of phase 06's data that would go stale the moment the face pass re-ran |
| Reasons store their **code**, not their sentence | Phase 09's decision, inherited without change. A catalog full of English sentences is a catalog that cannot be translated |

### Phase 06's clustering budget still does not reproduce

`people_budgets::clustering_a_full_skeleton_stays_inside_the_budget` fails on this machine
at about 22 s against a 12 s budget, exactly as the phase 07, 08 and 09 exit reports record.
Nothing in phase 10 touches `aura_vision::face::cluster`. Carried forward again,
unrepaired. **Owned by PERF, against phase 06, and it has now been carried four times.**

---

## 5. Open conditions

### C1 - both heads are placeholders (**Sev 2 trigger**)

`expression_head` 1.0.0 and `interaction_head` 1.0.0 have the right architectures and no
training. On a real photograph the expression head's eight sigmoids describe a random
projection of a face crop, and the interaction head's nine describe a random projection of
a frame.

Everything around them is real and measured: the shared two-point warp, the gaze geometry
and its sign, the mutual-gaze symmetry, the prominence-weighted means, the scene weight
table, the tradition multipliers, the composure inversion, the nine features, the
Bradley-Terry utility, the calibration machinery, the smoothing kernel, the margin and the
flat-moment refusal, the three reaction conditions and the resolver, the tear certainty
gate, the store's override semantics, the resumable pass, the cloud task's anonymisation,
the IPC surface and both panels.

Two mitigations are structural rather than promised:

* **A missing head is visible rather than silent.** With no expression head, every frame is
  read on interactions alone: `EmotionCode::NoFaces` is in the reasons, the confidence is
  capped at 0.55, and `EmotionOutline::face_aware` reports zero. Seven of the nine ranker
  features come from faces, so a caller about to trust an ordering can see that most of it
  was absent.
* **The heads cannot cull.** Nothing in this phase rejects, delivers or ranks into a
  gallery, so an untrained head produces a wrong *ordering* rather than a wrong deletion.
  That is a materially smaller failure than phase 09's would have been, and it is why the
  ordering could ship at all.

**No later phase may claim an emotion quality result on real photographs until this
closes.** Training needs the labelled crops and interaction labels section 9 gives DATA,
and for expression it needs consented face data - phase 06's condition C1, unchanged.

### C2 - the ranker is fitted on authored comparisons, not photographers'

Section 9 gives DATA "10k pairwise photographer comparisons across traditions" and section
10.1 gates on ">= 0.80 pairwise agreement with photographers". Neither exists.

The nine coefficients come from `ml/models/emotion/train_ranker.py --fixtures`, fitted on
eight authored preferences a working photographer would recognise. Running it prints
something the shipped table now says out loud: **four of the nine coefficients are
unidentifiable from that data**, because `interaction`, `peak`, `reaction` and
`mutual_gaze` do not vary between the two frames of any authored comparison. Those four are
set by argument.

Section 13's sixth criterion - the blind study with five photographers over 300 moments - is
**not met** and is not partially met. There is no study.

The distinction from C1 matters: C1 is a missing model and C2 is a missing opinion. A
trained head with these coefficients would rank a wedding coherently and might rank it in a
way no photographer agrees with, and only the study would find that out.

### C3 - gaze is head direction, not eye direction

`emotion::gaze` measures where the head is pointed - yaw from the eye midpoint's offset
inside the face box - and a person can look sideways without turning their head.

Bounded rather than open-ended. Every threshold is set on the conservative side: a face
that is not clearly turned reads as `Camera`, which is the answer that claims least, and a
face with no landmarks reads as `Unknown` rather than `Away`. Gaze is one of nine ranker
features and one of three reaction conditions; no flag, score or delivery decision turns on
it alone.

Section 6.3's "frames whose subjects gaze *toward the action*" is the part this cannot do:
two cameras produce two frames with no shared coordinate system, so a claim about the
direction between them would be invented. The linker requires an *engaged* face rather than
a geometrically-aimed one, which is as far as two independent 2D frames honestly go.

### C4 - the per-scene calibration is the identity map

Section 6.4's second half - "calibrated per scene by isotonic regression - this makes the
number comparable to `technical_score` in Phase 12" - ships as the identity.

The machinery is real and tested: `Calibration::new` refuses a non-monotone knot sequence
(`a_non_monotone_calibration_is_refused`), `EmotionWeights::calibration_is_identity`
asserts what ships, and `ml/models/emotion/eval_emotion.py --fit-calibration` fits them
with PAVA and prints the eleven knots. There is no labelled keeper/reject data here to fit
on.

**What survives without it** is weaker than phase 09's equivalent claim, and that is worth
saying rather than glossing. Phase 09's C5 could point to a construction argument - every
sub-score was computed against its scene's own tolerance before the composite. Here the
scene conditioning is real (the weights) but the *output distribution* is not equalised
across scenes, so a 0.71 in a ceremony and a 0.71 on a dance floor are close but not proven
equal. Phase 12 combines this number with `technical_score`, and until this closes it
should treat the comparison as approximate.

### C5 - the four named peak kinds are derived, not trained

Section 6.2: "kiss apex, tear release, bouquet-in-air and ring-slide are trained as
explicit peak types". There is no peak-type head.

`peak::kind_of` derives them from the scene label and the detected interaction instead, and
deliberately requires **both to agree** except for a tear release, which is a face reading
above the certainty gate. So a wrong kind needs two independent things to be wrong.
`BouquetInAir` is the weakest of the four: there is no bouquet detector, so the evidence is
`Exit`-or-`Candid` plus a group cheer, which is what a toss looks like from the front and is
as far as this build honestly goes.

A moment that satisfies none of the four is `PeakKind::Expression`, which claims only that
something peaked. The vocabulary is frozen now so that a trained head later changes how the
field is filled and not who reads it.

---

## 6. Carried forward from earlier phases

Phase 02's three exit conditions are still open and are carried again: real camera files, a
photographed ColorChecker, and a three-OS CI run. **The first real camera file is a Sev 2
trigger that reopens phase 02's criteria whatever phase is in flight** (ADR-0006).

Phase 05's condition C10 - the perceptual embedding is a placeholder - is unchanged.
**Phase 10 does not depend on it**, which is worth stating because phases 07 and 08 both
did: nothing in `emotion` reads an embedding. Expressions come from crops and interactions
from frames.

Phase 06's conditions are unchanged, and C1 matters here more than in any phase since 06
itself: a wedding whose faces were not found is a wedding ranked on interactions alone, and
`EmotionOutline::face_aware` is the number that says so.

Phase 07's conditions are unchanged. Phase 10 degrades gracefully without it: an
unclassified frame is weighted by `[default]`, the confidence drops by 0.10 and
`EmotionCode::NoScene` says why. **Phase 07's condition C5 - no per-tradition accuracy
published - now has a second phase depending on it**, because the tradition multipliers are
selected by phase 07's ritual head.

Phase 08's condition C4 is unchanged and this phase did not touch it.

Phase 09's condition **C4 is closed**. `IntegrityPass::with_emotion` fills
`IntentInput::tears` through `aura-core`'s frozen `EmotionService`, `ANALYSIS_VER` went
from 1 to 2, and a tearful closed-eye photograph now carries `EYES_CLOSED_OK` instead of
`EYES_CLOSED`. Phase 09's other five conditions are unchanged.

---

## 7. Rollback

| Switch | How |
|---|---|
| Feature off | Do not call `EmotionPass::run`. Nothing else in the product requires `image_interaction`; `EmotionOutline::coverage` reports 0.0, `EmotionService::of_image` returns `None`, and the card draws "not read" rather than a flat reading. Phase 09 reverts to `tears = false`, which is exactly what it shipped with |
| Config rollback | The shipped `emotion_weights.toml` is embedded in the binary. An installation override that will not load falls back to it with a logged refusal; the *embedded* file failing to load is `AURA-ML-5039` and halts, which is the correct direction for a table that decides cultural weighting to fail in |
| Re-scoring | `EmotionPass::run` rebuilds every reading whose versions are behind. Steps 3 to 5 - peaks, links, re-score - open no file, so a `weights_ver` bump is arithmetic over stored readings rather than two model calls per frame |
| Migration reversible | Yes. The down migration is seven drops, written out at the top of `0010_emotion.sql`. Everything it costs is recomputable from the pixels except two things - `moment_peak.user_chosen` and every row of `emotion_preferences`, which is a photographer's taste and cannot be recomputed at all - so the runbook says to export both first |
| Model rollback | `models.lock` pins by digest; the registry keeps the previous version until a new one has completed one real inference (`AURA-ML-5009`). The two heads share one `MODEL_VER`, so rolling one back rolls both back - deliberately: a catalog holding faces from one vintage and interactions from another is a state nothing could interpret |
| Version rollback | Three columns - `model_ver`, `analysis_ver`, `weights_ver` - and `AURA-ML-5038` when they disagree with this build. Two vintages are never compared; the outline reports the lowest present |
| Cloud rollback | `MomentSignificance` is optional by construction. With no key, no consent or no network, the local fallback returns `significance = 0` and says so in its reasons; `narrative_weight` is carried beside `emotion_score` rather than folded into it, so removing every cloud answer changes no local score |

---

## 8. What phase 11 inherits

Five rules, and every later phase inherits them.

- **`EmotionService` is the only way to ask what a photograph is worth.** No phase may
  keep its own expression model, its own idea of a peak, or its own reaction linking. This
  is phase 05's rule for `SimilarityIndex`, phase 06's for `PeopleService`, phase 07's for
  `StoryService`, phase 08's for `MomentService` and phase 09's for `IntegrityService`, a
  sixth time and for the same reason: two answers to "which of these six frames is the one"
  is two galleries that disagree.

- **A score is evidence; the deciding phase owns the cull - and an *ordering* is still
  evidence.** Nothing in `emotion` rejects, delivers or ranks into a gallery, and no
  column, field or command would let it. This is the hardest version of the rule so far:
  phase 09 produced a number that looked like a verdict, and this phase produces a *sorted
  list*, which is one button away from a shortlist. `MomentBrowser` says so in its own
  header and a test asserts no label in it says `keep`, `reject`, `deliver` or `cull`.

- **Three version columns, and a fourth was deliberately not added.** `model_ver`
  invalidates every expression and interaction reading, `analysis_ver` the gaze, the peaks
  and the links, `weights_ver` the score. The ranker's coefficients ship *inside* the
  weight table so that one number invalidates the score - phase 09's rule read in the
  direction that removes a column. `AURA-ML-5038` is the sixth version-drift code.

- **Report coverage, and say what the denominator is.** `EmotionOutline::coverage` is
  measured against **every photograph**, as phase 09's is, because a reading needs only
  pixels. `face_aware` is the second number and it is the one that matters when it is low:
  seven of the nine ranker features come from faces, so a wedding at 3 % face-aware has
  been ranked on very nearly nothing.

- **A weight table is a product decision, and it needs a written reason per row.** Third
  config file in this crate to enforce it and the one where it matters most:
  `emotion_weights.toml` is where the product decides that a composed Hindu ceremony is not
  an empty gallery, and a threshold nobody can explain there is a cultural failure waiting
  to be shipped.

And three things phase 11 should know before it starts:

**The composure inversion is load-bearing and it is easy to break.** Four ceremony scenes
weight composure at or above a smile, and three tradition multipliers raise it further. Two
tests and one gate check it, in every scene rather than in the two the phase document
names. A phase that adds a scene must add its row, and a phase that adds a tradition must
either add its multipliers or explain why it did not.

**`emotion_score` and `technical_score` are meant to be combined and are not yet exactly
comparable.** Both are `0..1` and both are scene-conditioned; phase 09's is calibrated by
construction and phase 10's isotonic layer ships as the identity - condition C4. Phase 12
should combine them and should treat the comparison as approximate until that closes.

**The two brain crates depend on each other in neither direction, and that is deliberate
rather than accidental.** Phase 09 reads `EmotionService` from `aura-core`; phase 10 reads
no integrity anything. The shared 112 px warp lives in `aura-vision` for the same reason.
Any phase that needs both should read both traits, not link both crates.
