# ADR-0021 - The emotion taxonomy, the ranker, and the cultural rules

**Status:** accepted
**Date:** 2026-08-15
**Phase:** 10 - Expression, Emotion & Moment Ranking AI
**Supersedes:** nothing. **Amends:** nothing.

---

## 1. Context

Phase 09 answered *is this frame acceptable*. Phase 10 answers *is it worth delivering*,
and the two are different questions with different failure modes. Section 1 of the phase
document puts it plainly:

> Photographers do not sell sharpness, they sell feeling. Any product that automates
> culling without modelling emotion will consistently deliver technically perfect,
> emotionally flat galleries.

The risk register calls this phase **High - subjective and culturally sensitive**, and it
is the first phase in the product where the main risk is not that the model is wrong but
that it is *right about the wrong culture*. Section 12's first failure mode:

> Cultural bias toward Western expressiveness.

A model trained on "delivered versus rejected" from a Western corpus learns that a moment
is a big smile. Run over a Hindu ceremony, where the couple are composed by convention for
two hours, it delivers an empty gallery - and the photographer concludes the product does
not understand their work. That is not recoverable by an apology.

This ADR records the decisions that follow from taking that seriously, plus the seven
spellings where the shipped contract differs from section 5.

---

## 2. Eight spellings differ from the phase document

Section 5 freezes `FaceExpression` and `ImageEmotion`.
`crates/aura-core/src/contract/emotion.rs` is what shipped, and it differs in eight places.

| # | Section 5 | Shipped | Why |
|---|---|---|---|
| 1 | `ImageId` | `PhotoId`, aliased | One type. The scene, people, moment and integrity contracts already do it, and a conversion between two id types is a place they can disagree. |
| 2 | `ImageEmotion` has `reasons`, no `confidence` | both | Invariant 2 requires both of every AI decision. A ranking a photographer will disagree with has to be able to say how sure it is. |
| 3 | `FaceExpression` is unreachable from `ImageEmotion` | `ImageEmotion::faces` | Section 9's `SFE` deliverable is a card with face crops. Without this it is two round trips per thumbnail. |
| 4 | `reasons: Vec<Reason>` | `Vec<EmotionReason>` with its own `EmotionCode` | Phase 09's `Reason` carries a closed set of *technical defect* codes and shares none of this phase's vocabulary. Section 9 gives `DOC` "explain emotion scoring honestly", which is only finishable against a closed list. `CropRect` is reused rather than redeclared. |
| 5 | four gaze targets | five, with `Unknown` | Gaze is measured from phase 06's two eye landmarks. A face the detector found without landmarks has no measurement, and "we could not tell" must never read as "looking away" - `Away` is evidence the reaction linker acts on. |
| 6 | peak is two fields; reaction is one `Option` | plus `MomentPeak` and `ReactionLink` | Both section 5 fields survive unchanged. The moment-level answer needs a margin and a kind, and a link needs its bonus and the gap it crossed; neither has anywhere to live on a per-frame shape. |
| 7 | no coverage carrier | `EmotionOutline` | Phase 05's rule, inherited a sixth time. |
| 8 | no entry point | `EmotionService` | Four later phases consume emotion scores. |

Two more spellings are in the schema rather than the contract, and both are recorded here:

* **`image_interaction` is the per-image row.** Section 4 names four tables and section 5
  names two data shapes; the per-image shape has to live in one of the four. It goes in
  `image_interaction` rather than in a fifth table nobody named, so migration 10 is
  exactly section 4's list. The name is narrower than the row.
* **`emotion_preferences` is a fifth table.** Section 2.1's preference hook has to be
  stored somewhere and section 4 names no table for it. It is the one thing in this phase
  that cannot be recomputed from pixels.

---

## 3. Decision: composure is a positive class, and the weights are per tradition

**The decision.** `FaceExpression::composed` is one of the eight channels, weighted
positively, and in the four ceremony scenes it is weighted **at or above** `smile`. On top
of that, `emotion_weights.toml` carries per-tradition multipliers on `smile`, `laughter`
and `composed`, and a Hindu or Nepali or Muslim rite multiplies composure up.

**The alternatives considered.**

1. *One global weighting, and trust the training data to cover traditions.* Rejected: the
   training data does not exist yet, and when it does its composition is exactly the thing
   that will go wrong quietly. A correction that lives in a config file is one somebody can
   read, argue with and version; a correction that lives in a sampler is one nobody notices
   is missing.
2. *A per-tradition model.* Rejected: five models to train, five to validate, five to roll
   back, and a wedding whose tradition is `mixed` or `unclear` would have none.
3. *Weights per scene only, no tradition axis.* Rejected as insufficient. `ritual` already
   weights composure highest of any scene, and a Hindu ceremony is still more composed
   than the median `ritual` in a mixed corpus.

**The consequence.** `tests/eval/emotion_eval.rs` gates it in **every scene in the
vocabulary**, not only in the two section 10.1 names, because an edit that broke
`ceremony_entrance` and not `ritual` would pass a two-scene test. `aura-cli verify --phase
10` checks it a second time end to end, before anything is built on top of the table.

**What was deliberately not done.** Five of the ritual head's eight traditions carry a
weight row. `sikh` has no taxonomy in `config/rituals/`, so nobody here has established
what its rites look like or how composed a couple are during them, and a multiplier
invented without that is a guess with a version number. `mixed` and `unclear` are
abstentions. All three fall back to `[default]`, which is set from the balanced fixture set
rather than from the Christian one - and `[tradition.christian]` ships with three 1.0s
precisely to say that out loud rather than by omission.

---

## 4. Decision: the weights, the ranker and the calibration are one versioned file

**The decision.** `emotion_weights.toml` holds the scene weights, the tradition
multipliers, the nine Bradley-Terry coefficients and the per-scene isotonic knots. One
`version`, written into `image_interaction.weights_ver`.

**Why.** Because all four invalidate exactly the same thing - `emotion_score` - and
nothing else. Three files would mean three version columns on one row that always move
together, which is phase 09's third inherited rule read in the direction that *removes* a
column rather than adding one:

> Three version columns, because they invalidate three different things.

Phase 10 has three: `model_ver` for the two heads, `analysis_ver` for the gaze, the peak
curve and the links, `weights_ver` for the score. A fourth for the ranker would be a
column that is never observed to differ from `weights_ver`.

**The consequence.** Re-tuning the table is the cheapest re-run in the phase: steps 3 to 5
of `EmotionPass::run_ids` open no file, so a `weights_ver` bump re-scores a wedding from
stored readings in seconds rather than re-running two heads over four thousand frames.

---

## 5. Decision: gaze is measured, not predicted

**The decision.** `GazeTarget` is computed in `emotion::gaze` from phase 06's two eye
landmarks and the face box. The expression head has no gaze slot.

**Why.** All four of section 2.1's targets are *relations between two positions in the
frame*, and phase 06 hands over both. A head that emitted a gaze slot would be guessing
with authority about something the geometry answers, and it would be one more output to
retrain when the vocabulary changes.

**What is lost, stated rather than hidden.** Eyeball direction. This measures where the
*head* is pointed, and a person can look sideways without turning it. Every threshold is
therefore set on the conservative side - a face that is not clearly turned reads as
`Camera`, the answer that claims least - and no flag, score or delivery decision turns on
gaze alone. That is condition **C3** of the exit report.

---

## 6. Decision: the store reads `moment_images` and `faces` directly

**The decision.** `EmotionStore` reads phase 08's grouping and phase 06's faces with SQL
rather than through `MomentService` and `PeopleService`.

**Why.** The same argument ADR-0019 section 6 makes for `IntegrityStore`, one phase on.
The peak pass needs to know which frames were shot together and the expression pass needs
to know where the faces are, and both are already stored. Calling `moment_of` once per
frame is four thousand round trips to answer a question one query answers.

**What keeps it honest.** This computes no cadence, builds no graph, runs no clustering
and produces no grouping of its own. It reads back, as opaque ids, a grouping
`MomentService` already made. Phase 08's rule - "`MomentService` is the only way to ask
what was shot once" - is about producing a grouping, not about reading one.

**The per-frame path is different and goes through the traits.** `EmotionPass` takes
`Arc<dyn PeopleService>` and `Arc<dyn StoryService>`, and `aura-brain-wedding` gains no
dependency on `aura-people`.

---

## 7. Decision: phase 09's tears rule is wired through `aura-core`, not through a crate

**The decision.** `IntegrityPass::with_emotion(Arc<dyn EmotionService>)` fills
`IntentInput::tears`, closing condition **C4** of the phase 09 exit report. `ANALYSIS_VER`
in `aura-brain-photo` goes from 1 to 2.

**Why it is not a crate dependency.** `aura-brain-photo` reads the frozen
`EmotionService` from `aura-core`, and `aura-brain-wedding` reads no integrity anything.
The two brain crates depend on each other in neither direction, which is what keeps phase
09's rule ("no phase may keep its own blink detector") and phase 10's ("no phase may keep
its own expression model") from becoming a cycle.

**The ordering, which is the part that needs stating.** The emotion pass has to have run
before the integrity pass for the rule to fire. It is a re-analysis rather than a race: a
verdict made before the emotion pass has `tears = false`, which is exactly the phase 09
behaviour, and the `ANALYSIS_VER` bump makes every stored verdict pending so the
background pass re-measures. Without `with_emotion` every frame is judged exactly as phase
09 shipped.

**What it changes for a photographer.** A tearful closed-eye photograph that carried
`EYES_CLOSED` now carries `EYES_CLOSED_OK`. That is the whole point of the rule and it is
the direction phase 09 said it wanted.

---

## 8. Decision: a shared two-point warp moves into `aura-vision`

**The decision.** `align_from_eyes` and `preprocess_crop` moved out of
`aura_brain_photo::integrity::eyes` into `aura_vision::face::align` as
`warp_crop_from_eyes` and `preprocess_face_crop`. Phase 09's functions delegate and keep
their names.

**Why.** Phase 10's expression head became the second consumer of exactly the same 112 px
crop of exactly the same face. Two copies of a warp is two crops that drift apart while
looking identical - and because the two brain crates deliberately do not depend on each
other, `aura-vision` is the only place both can see. Phase 09's 26 eval gates and 11
calibration tests pass unchanged after the move, which is what makes it a de-duplication
rather than a change.

---

## 9. The performance waiver

Section 11's first two rows are GPU claims: 40 ms per image, and 4,000 images in 160 s on
an RTX 4070. **Both are waived**, on ADR-0007's grounds and with ADR-0019's precedent:
this build has no GPU backend, and a budget nothing can measure is a wish rather than an
assertion. The waiver expires when a GPU backend lands.

What is asserted instead, in `perf/budgets.toml` and
`crates/aura-perf/tests/emotion_budgets.rs`:

| Row | Section 11 | This build | Status |
|---|---|---|---|
| per image, GPU | <= 40 ms | - | **waived** |
| 4,000 images, RTX 4070 | <= 160 s | - | **waived**; the gate prints 124 s on the processor path |
| peak + reaction linking, whole wedding | <= 8 s | **13 ms** | **met** |
| storage per image | <= 900 B | **733 B** | **met** |

The third row is the one this phase can most honestly assert: peaks and reaction linking
open **no file at all**, so eight seconds is a claim about SQL and arithmetic that holds on
any machine.

Signed by PERF and CTO for the first two rows only.

---

## 10. What this phase must never do, and how that is kept structural

Three boundaries, each enforced by something other than a reviewer's memory.

**No claim about anybody's inner state.** Section 2.2 puts it permanently out of scope.
`EmotionCode` is a closed vocabulary of twenty sentences written in the language of a photo
editor; a call site cannot write its own, because call sites do not write sentences. The
cloud task's `Validate` refuses a reason containing any of twenty banned appearance and
psychology words. There is no `mood` column and no free-text field on any row.

**No culling.** No table, field, command or UI control in this phase keeps, rejects,
delivers or builds a gallery. `EmotionService::ranked` returns an *ordering*, because
"ranks every frame by emotional value" is the phase's headline; the moment browser says
"An ordering, not a shortlist" in its own header, and its test asserts that no score label
contains the word `keep`, `reject`, `deliver` or `cull`.

**No inference about who somebody is.** The cloud task's subjects are six role handles
assigned locally - `primary_a`, `primary_b`, `close_family`, `guest`, `child`, `vendor` -
and the mapping from handle to identity never leaves the machine. `primary_a` rather than
`bride` for phase 06's reason: which of two people is the bride is not a photographic fact.

---

## 11. Consequences

**Good.** The cultural correction is a file a product manager owns rather than a property
of a dataset. Re-tuning it is seconds rather than hours. The ranker's coefficients are a
list somebody can argue with, and every reason the product writes names one of them.
Phase 09's condition C4 is closed. Phases 06, 09 and 10 now share one warp.

**Bad.** Nine features and a linear utility will be beaten by a learned ranker on a large
enough comparison set, and phase 30 is where that argument gets had. The four peak kinds
are derived from the scene and the interaction rather than trained, so a bouquet toss in a
scene phase 07 misread is a `PeakKind::Expression`. The gaze is a head direction.

**Ugly.** Both heads are placeholders, and every number section 10.1 asks for is measured
against an answer painted into synthetic pixels. That is condition C1 and it is a Sev 2
trigger; it closes with phase 05's C10 and phase 06's C1 rather than separately, because
all three need the same thing: a labelled wedding corpus and a GPU.

---

## 12. Related

* `docs/adr/ADR-0022-emotion-ipc-surface.md` - the seven commands and why two of them
  change anything
* `docs/adr/ADR-0019-frame-integrity-and-eye-intent.md` section 3 - the `FaceRef`
  amendment this phase's gaze and crops depend on
* `crates/aura-brain-wedding/config/emotion_weights.toml` - the cultural argument, as
  numbers
* `docs/emotion-and-moments.md` - what every reason code means to a photographer
* `docs/progress/PHASE-10-EXIT.md` - the five open conditions
