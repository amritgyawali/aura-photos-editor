# ADR-0019 - Frame integrity: subject-aware measurement, and who decides intent

**Status:** accepted
**Date:** 2026-08-15
**Phase:** 09
**Deciders:** CTO, ML Lead - Vision, Colour Scientist, Product Manager
**Supersedes:** nothing. **Amends:** `crates/aura-core/src/contract/people.rs` (section 3).

---

## 1. Context

PHASE-09 ships one feature: every frame gets an honest technical verdict where it
matters. Five questions - is the *right* subject sharp, was the motion a decision, can the
exposure be brought back, how noisy is it, and are the important eyes open - answered from
one decode of one 2048 px proxy.

The phase document is unusually clear about the risk, and it is not a technical one.
Section 12's first failure mode is "**false rejections destroy trust instantly**" and
section 1 says photographers abandon culling tools the moment one frame is thrown away
that should have been kept. Every decision below is a decision about which direction to be
wrong in.

This ADR records eleven of them.

---

## 2. The contract, and seven spellings that differ from section 5

`crates/aura-core/src/contract/integrity.rs` is section 5's shapes as code. Seven things
differ and each is deliberate.

| Section 5 | Here | Why |
|---|---|---|
| `ImageId` | `PhotoId`, aliased | one type, as scene, people and moment already do |
| `bitflags!` | a hand-rolled `u32` | not a workspace dependency; `AttrFlags` set the precedent |
| `ExposureVerdict` with three variants | four, with `Good` | a correctly exposed frame is not "recoverable", and telling phase 15 that every frame needs a correction is worse than an extra variant |
| `Reason.code` as text | a typed `ReasonCode` | section 9 gives DOC "document every reason code in user language", which is only finishable if the set is closed |
| penalties only | a **signed** weight | the reasons that matter most say why something is *not* a defect |
| no coverage carrier | `IntegrityOutline` | phase 05's rule, inherited for the fifth time |
| no entry point | `IntegrityService` | five later phases consume verdicts |

**The fourth and fifth are the load-bearing ones.** Eight of the twenty-one reason codes
withdraw a claim rather than making one, and two of the fourteen flags describe something
*right* with a photograph. A contract that could only express penalties would make the
interface unable to show a photographer that the product understood their f/1.4 portrait -
and that is the thing section 12's first failure mode is actually about.

---

## 3. Amending `FaceRef`: a box and two eye points

**Decision.** `crates/aura-core/src/contract/people.rs` gains two fields on `FaceRef`:
`bbox: CropRect` and `eyes: [[f32; 2]; 2]`.

**Why it was necessary.** Phase 09 cannot do its job without face geometry. Section 6.1
puts eye regions first in the sharpness order; section 6.4 puts the eye head on aligned
crops; section 13's last criterion requires the card to show the crop that caused each
penalty. `FaceRef` carried none of that, and the alternatives were worse:

* **read `faces` directly** - a second reader of phase 06's table, which is the
  arrangement phase 06 built its two-crate split to prevent;
* **re-detect** - forbidden by invariant 3 and by phase 06's own rule;
* **a `PeopleService` bulk accessor** - phase 08's condition C4 proposed exactly this and
  correctly called it a contract change with its own ADR. This is that ADR, choosing the
  smaller change.

**What was deliberately not added.** The nose and the two mouth corners. `FaceRef`'s own
doc comment says it carries "not a template, not a crop, and not the five-point landmark
array a recogniser takes", and that sentence stays true: two eye points are what an
eye-state question needs and are not what makes a face identifiable. The rectangle type is
`CropRect` rather than a new one, deliberately - the evidence crop the panel shows for a
closed-eye penalty *is* this box, expanded.

**What it closes elsewhere.** Phase 08's condition C4 said its two face signals were
unwired because `PeopleService` had no way to supply them. It does now. Wiring phase 08's
`PassContext` is phase 08's code and outside this phase's scope; the blocker is removed
and the exit report says so.

---

## 4. The composite is a product, and the sub-scores are scene-relative

**Decision.** `technical_score` is the weighted **geometric** mean of five sub-scores,
each floored at 0.08, then mapped through a per-scene isotonic calibration.

Section 6.5 asks for exactly this, and the reason is worth restating: a frame that is
perfectly exposed, clean, well framed and completely out of focus on the bride has four
good sub-scores and one terrible one. A weighted sum gives it 0.75 and a place in the
gallery. A weighted product gives it 0.2.

**The floor is not a rounding detail.** A product with a true zero in it is zero, and a
frame scored zero is indistinguishable from a frame that was never analysed - which
collapses the distinction migration 9 spends a paragraph protecting.

**Section 13's fifth criterion is met by construction, not by fitting.** "Scores are
calibrated per scene so 0.8 means the same thing in a ceremony and on a dance floor" is
satisfied because every sub-score is computed against the scene's own tolerance *before*
it reaches the composite: `noise_sigma_rel` is already "sigma over what this scene
allows", and the soft threshold is already a function of `max_acceptable_blur`.
`tests/eval/integrity_eval.rs` asserts a spread under 0.02 across all 23 scenes for a
frame at a fixed relative quality.

The isotonic layer is the second half of section 6.5 and **ships as the identity map**,
because fitting one needs labelled keeper/reject pairs and there are none here. Condition
C5.

---

## 5. Four weights live in code and one lives in config

**Decision.** The focus sub-score's weight comes from
`SceneProfile::subject_focus_weight`; the eye weight is half of it; the exposure, noise
and motion weights are constants in `score.rs`.

**Rejected: adding four weight fields to `SceneProfile`.** It is a frozen contract that
ten phases read, and every one of them would recompile for numbers none of them uses. More
importantly it would be the *wrong place*: `SceneProfile` already carries
`max_acceptable_noise` and `max_acceptable_blur`, and **the scene enters the exposure and
noise sub-scores through their tolerances rather than through their weights**. A scene
that tolerates more noise should accept a noisier frame, not care less about noise.

The eye weight is half the focus weight rather than a sixth config number because the two
would always move together: `subject_focus_weight` is phase 07's statement of "how much
this scene is about the person", and eyes matter in exactly the scenes faces do.

---

## 6. Two places this crate reads the catalog rather than a service

**Decision.** `IntegrityStore` reads `moment_images` and `faces` by SQL.

Phase 08's rule is that `MomentService` is the only way to ask what was shot once, and
phase 06's is that `PeopleService` is the only way to ask who is in a photograph. Neither
is being asked here:

* `relative_sharpness` reads back the grouping `MomentService` already made, as opaque
  ids, to answer "of the frames in this group, where does this one rank". It computes no
  cadence, builds no graph, and cannot produce a grouping of its own. The alternative is
  four thousand `moment_of` calls to answer what one window function answers.
* `eyes_of` joins `faces` for the geometry and the area, which are phase 06's numbers
  read back rather than recomputed. It opens no vault and holds no template.

This is the argument `aura_brain_wedding::moments::moment` makes for reading
`faces.identity_id`, and phase 07's `PeopleStore::scene_labels` makes in the other
direction. It is a *narrow* exception and both instances are documented at the call site.

**The pass, unlike the store, goes through the frozen services.** `IntegrityPass` holds
`Arc<dyn PeopleService>` and `Arc<dyn StoryService>` and calls them once per frame -
which is affordable here and was not in phase 08, because this pass already decodes a
proxy per frame and one catalog query is lost in the noise of a 130 ms measurement.

---

## 7. Two modules section 4 does not name

Section 4 lists `{focus,motion,exposure,noise,eyes,score,flags}.rs` plus
`calibration.rs`. The crate also has `analyse.rs`, `store.rs` and `api.rs`.

`analyse.rs` is the per-frame pipeline - the thing that guarantees "one decode, every
measurement" structurally rather than by discipline. `store.rs` and `api.rs` are the two
halves phases 06, 07 and 08 all converged on, for the reason they each recorded: an
analyser that owns its SQL is an analyser nobody can test without a database.

---

## 8. The focus head may exonerate and may not convict

**Decision.** `focus::apply_head` lets the focus head **withdraw** a softness claim at
confidence 0.70 and never lets it make one. A confident `Soft` does nothing.

**Why.** Section 12's first failure mode. A placeholder model that could convict a
photograph would be the fastest possible route to a false rejection, and the asymmetry
costs nothing while the head is untrained: the classical measures still raise
`SUBJECT_SOFT` against the scene's threshold.

The same reasoning decides the two precision policies, and the contrast is the clearest
statement of the principle in the model set: **int8 is permitted on the focus head and
forbidden on the eye head.** The focus head's quantisation noise sits around a threshold
that can only exonerate; the eye head's sits at `ACT_ON_CLOSED = 0.55`, where a wrong
answer marks a photograph of a kiss as a fault.

A trained head lifting the asymmetry is an ADR and a re-validation, not a constant change.

---

## 9. The head does not decide intent; four rules do

**Decision.** `eye_state` emits five states and no "intentional" slot. Section 6.4's four
intent rules are implemented in `eyes::intent`, in the order the phase document writes
them, and the PM signs them off as policy.

**Why.** Every one of the four depends on something outside the crop - the scene, the
partner's eyes, phase 10's tears, the expression. A model given the crop alone and asked
"was this deliberate" would be guessing with authority, and section 9 puts these rules in
front of a working photographer precisely because they are a product decision.

**Three of the four are implemented.** The tears rule is phase 10's; `IntentInput::tears`
is wired through and always false. The degradation is in the safe direction - a missing
exoneration makes a frame *look* more defective, which surfaces in a review queue rather
than in a delivery. Condition C4.

**Two scenes were added to section 6.4's list**, `rings` and `first_dance`, and one was
widened: `speeches_emotional` has no separate scene in phase 07's vocabulary, so the whole
of `Speeches` is admitted. The failure of admitting too much is a missed blink; the
failure of admitting too little is a deleted photograph of somebody crying at a toast.

---

## 10. Only important subjects' eyes gate a frame, by two independent tests

**Decision.** A face gates when its prominence is at least 0.18 **or** its area is at
least 1.5 % of the frame.

Section 6.4 asks for the first. The second exists because prominence needs an identity
assignment behind it, and a wedding whose face clustering has not run would otherwise have
no gating faces at all - reporting every frame as eye-clean, which is the "reports nothing
wrong because it looked at nothing" failure `AURA-ML-5035` exists to make visible
elsewhere.

An uncertain `closed` reading - below `ACT_ON_CLOSED` - is stored with its confidence and
does not gate. One-directional on purpose: an uncertain `open` costs nothing.

---

## 11. Section 11's GPU rows are waived

**Decision.** Section 11's 45 ms-per-image GPU budget and its 180 s-for-4,000-images
RTX 4070 row are **waived**, with the same expiry condition ADR-0007 attaches to phase
03's throughput rows: this build has no GPU backend, and a budget nothing can measure is a
wish rather than an assertion.

What is asserted instead is the processor path, which is what this build runs: 220 ms per
image, met at **128 ms** on the development machine, and the four-thousand-image figure is
printed beside it so a reader can see what a full wedding costs.

**Signed:** PERF, CTO. **Expires** when a GPU backend lands.

The storage row is *not* waived. Section 11's kilobyte per image is met at exactly
1,024 bytes, after four schema decisions the budget forced: reasons store their code
rather than their sentence, the eye table is `WITHOUT ROWID` with no `photo_id`, the two
indexes that served no query were removed, and the face geometry is read from `faces`
rather than copied.

---

## 12. Consequences

**Good.**

* A blurred background is craft and a blurred bride is a defect, and the product can tell
  them apart - which is the whole feature.
* Two of the fourteen flags are good news, and the interface shows them as prominently as
  the bad news.
* Nothing in the phase can reject a photograph, structurally: no column, no field, no
  command.
* Phase 08's condition C4 has its blocker removed.

**Bad, and recorded as conditions.**

* Both heads ship untrained (C1). Every gate that needs them is measured against ground
  truth known by construction, which proves the algorithms and says nothing about the
  weights.
* The twenty calibration rows are derived from published specifications rather than
  measured from bodies (C2), because phase 02's first exit condition is still open.
* Clipping is measured on the proxy rather than on the RAW histogram (C3).
* The tears intent rule needs phase 10 (C4).
* The per-scene isotonic calibration ships as the identity (C5).

**Ugly.** The `FaceRef` amendment is a change to a frozen contract, and frozen contracts
are meant to stay frozen. The mitigation is that it was argued in this document before it
was written, it is the smallest change that unblocks the phase, and it makes a *previous*
phase's recorded condition solvable rather than only this one's.
