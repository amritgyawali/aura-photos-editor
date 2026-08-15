# Phase 09 exit report - Frame Integrity AI

**Date:** 2026-08-15
**Branch:** `feat/phase-09-frame-integrity-ai`
**Gate:** `just phase-09-verify` exits 0
**Verdict:** the phase is implemented and **conditionally** complete. Six conditions are
open, they are listed in section 5, and **C1 is a Sev 2 trigger**.

---

## 1. What shipped

One feature: every frame gets an honest technical verdict where it matters - is the
*right* subject sharp, was the motion a decision, can the exposure be brought back, how
noisy is it, and are the important eyes open.

| Area | What landed |
|---|---|
| Migration 9 | `image_integrity`, `face_eye_state`, and two views |
| `aura-core` | the frozen section 5 contract - `IntegrityFlags`, `MotionKind`, `ExposureVerdict`, `EyeOpenness`, `EyeState`, `CropRect`, `ReasonCode`, `Reason`, `IntegrityResult`, `IntegrityOutline`, `IntegrityService` - plus two fields on `FaceRef` |
| `aura-brain-photo` | a new crate: the camera calibration table, subject-aware focus, motion intent from the structure tensor, recovery-aware exposure, scene-relative noise, eye state with four intent rules, the flag and reason decision, the geometric composite, the store, the resumable pass and the synthetic ground truth |
| Config | `camera_calibration.toml`: twenty bodies, each with a rationale, plus a cautious fallback |
| Models | `focus_head` and `eye_state`, signed into `models.lock` with cards |
| IPC and UI | six commands, twelve types, the Integrity card and the filter chips |
| Gate | `aura-cli verify --phase 09`, eleven checks, exit 0 |

**Five new error codes**, each with a runbook: `AURA-ML-5033` to `AURA-ML-5037`.

**Two ADRs**: ADR-0019 (the measurement design, the `FaceRef` amendment and the intent
rules) and ADR-0020 (the integrity IPC surface).

**One amendment to a frozen contract.** `FaceRef` gained `bbox` and two eye landmarks.
ADR-0019 section 3 records why it was unavoidable, why it is the smallest change that
works, and what it deliberately left out.

---

## 2. Acceptance criteria (section 13)

| # | Criterion | Status | Evidence |
|---|---|---|---|
| 1 | Every image exposes subject sharpness, motion kind, exposure verdict, noise level and per-face eye state with reasons | **met, with C1** | gate: `10 of 10 frames checked, coverage 100%, 100% subject-aware`; `every frame carries a score, a confidence and at least one reason` |
| 2 | A shallow-depth-of-field portrait with creamy bokeh is never called soft | **met** | gate: `a bokeh portrait is not soft: subject 0.73, background 0.28`; `integrity_eval.rs` asserts it in **all 23 scenes**, because the threshold is scene-conditioned |
| 3 | A kiss with closed eyes is flagged `EYES_CLOSED_OK`, not as a defect | **met, with C1** | gate: `a kiss with closed eyes is exonerated: 2 of 2 gating faces`; the eval harness measures a 0.000 false-positive rate across seven scenes and three group sizes |
| 4 | A camera-shake ceremony frame and a panned exit frame are distinguished correctly | **met** | gate: `camera shake and a panned exit are told apart, and the pan is not a defect` |
| 5 | Scores are calibrated per scene so 0.8 means the same thing everywhere | **met by construction** | `zero_point_eight_means_the_same_thing_in_every_scene`: spread under 0.02 across all 23 scenes. The isotonic layer is C5 |
| 6 | The Integrity card shows the exact crop that caused each penalty | **met** | gate: `4 penalties across the set carry an evidence rectangle`; `IntegrityCard.test.tsx` asserts the positioning is in percentages so one rectangle fits every preview size |

---

## 3. Section 10.1's gates

Measured by `tests/eval/integrity_eval.rs` (26 tests) and by the phase gate.

| Gate | Threshold | Result | Against |
|---|---|---|---|
| Synthetic blur ladder monotonic | strict | **met**, seven rungs | `fixtures::blur_ladder`, known by construction |
| Back and front focus correctly signed | exact | **met** | `fixtures::focus_miss` |
| Subject-focus AUC | >= 0.96 | **1.000** | the blur ladder's keeper/reject labels |
| Shallow-DOF portraits never flagged soft | exact | **met**, all 23 scenes | `fixtures::shallow_depth_of_field` |
| Blink F1 | >= 0.95 | **1.000** | `ReferenceEyeReader` over labelled markers |
| Intentional-closed false positives | <= 2 % | **0.000** | seven scenes × three group sizes |
| Exposure recoverable vs lost | >= 0.93 | **1.000** over six cases | authored labels, not expert labels |
| Noise sigma within 15 % | exact | **met**, four ISO rungs | known sigma by construction |
| Group closed-eye ratio matches a human count | exact | **met**, 20 combinations | `fixtures::group_frame` |
| Cross-camera fairness, 24 MP vs 61 MP | <= 0.05 | **met** | `fixtures::cross_camera_pair` |
| Determinism | identical | **met** | two runs, gate and harness |
| A constant scorer fails the AUC gate | - | **met**, 0.500 | the guard |
| An inverted scorer scores below a coin toss | - | **met** | the guard |
| A flag-everything blink detector fails on precision | - | **met** | `eval_integrity.py --self-test` |

**Read the "against" column.** Every number above is a real measurement of the
*algorithms* - the region order, the calibration division, the structure tensor, the
specular exclusion, the noise estimator, the gating rules, the four intent rules, the flag
decision and the composite - against ground truth whose answer is known by construction.

**None of them is a measurement of the two shipped heads' weights**, which are
placeholders. That is condition C1.

The last three rows are the guard phases 06, 07 and 08 each wrote: a gate that cannot fail
is not a gate, and an AUC is specifically vulnerable to a scorer that returns a constant.

### What the harness, the gate and the budget found that review did not

Six real defects, in code that read correctly. Four were only reachable by measuring
something, which is the argument for having a harness, a gate *and* a budget rather than
any one of them. `docs/progress/PHASE-09.md` carries all six in full; the two most
consequential:

1. **`face_eye_state` had a foreign key onto a column that does not exist.** Migration 6
   names the faces table's key `id`; migration 9 wrote `REFERENCES faces(face_id)`. SQLite
   accepts that at `CREATE` time and raises `foreign key mismatch` on the first `INSERT` -
   so nothing failed until the storage budget planted a thousand rows. **Every eye state in
   the product would have failed to store**, silently, on a photographer's machine.

2. **A shaken frame read as a deliberate pan.** The pan test checked the background's
   coherence and the sharpness ratio, and camera shake satisfies both. The missing clause
   is the definition of a pan - the subject must not itself be smeared - and it was the
   first thing the eval harness caught. Had it shipped, every shaken ceremony frame would
   have been marked `INTENTIONAL_MOTION`: a defect reported as craft, which is the exact
   inverse of the failure section 12 warns about and arguably worse.

---

## 4. Section 11's budgets

Measured in release on the development machine (Intel i5-10300H, 8 GB, Win 11), asserted
by `crates/aura-perf/tests/integrity_budgets.rs`.

| Row | Section 11 | This build | Status |
|---|---|---|---|
| Integrity analysis per image (GPU) | <= 45 ms | - | **waived**, no GPU backend - ADR-0019 section 11 |
| Integrity analysis per image (CPU) | <= 220 ms | **128 ms** | **met** |
| 4,000 images (RTX 4070) | <= 180 s | - | **waived**, same reason; the processor path costs **512 s** and is printed beside it |
| Storage per image | <= 1 KB | **1,024 B** | **met, exactly** |

### The GPU waiver

The same waiver ADR-0007 attaches to phase 03's two throughput rows, for the same reason:
this build has no GPU backend, and a budget nothing can measure is a wish rather than an
assertion. Signed by PERF and CTO, expiring when a GPU backend lands.

What *is* asserted is the path this build runs. 128 ms per image is comfortably inside the
220 ms processor budget, and the cost is dominated by the classical measures over a
four-megapixel plane rather than by the two heads - which is why a trained head of the
same architecture will cost the same.

### The storage row, met by four decisions the budget forced

The first measurement was **1,855 bytes per image**. Section 11's kilobyte is what made
each of the following worth doing, and all four are improvements rather than compromises:

| Decision | Saved |
|---|---|
| Reasons store their **code**, not their sentence - `ReasonCode::user_text` supplies it | ~385 B |
| Two indexes that served no query, removed after being measured | ~130 B |
| `face_eye_state` is `WITHOUT ROWID`, with no `photo_id` and no identity index | ~450 B |
| The eye rows read their geometry from `faces` rather than copying it | ~120 B |

The measured split at the end is **704 bytes for the verdict row and its one index, 320
for three eye states**. The reason decision is the one worth keeping in mind: a stored
sentence is copy that a release can change, and a catalog full of English sentences is a
catalog that cannot be translated.

### Phase 06's clustering budget still does not reproduce

`people_budgets::clustering_a_full_skeleton_stays_inside_the_budget` fails on this machine
at about 22 s against a 12 s budget, exactly as the phase 07 and phase 08 exit reports
record. Nothing in phase 09 touches `aura_vision::face::cluster`. Carried forward again,
unrepaired. **Owned by PERF, against phase 06, and it has now been carried three times.**

---

## 5. Open conditions

### C1 - both heads are placeholders (**Sev 2 trigger**)

`focus_head` 1.0.0 and `eye_state` 1.0.0 have the right architectures and no training. On
a real photograph the focus head's three-way distribution describes a random projection of
the crop, and the eye head's five-way distribution says nothing about eyelids.

Everything around them is real and measured: the three classical sharpness measures, the
region order, the camera calibration division, the structure tensor and the shake/pan/
subject distinction, the specular exclusion, the ISO- and body-normalised noise estimate,
the two-point alignment, the gating rules, three of the four intent rules, the group
closed-eye ratio, the twenty-one reason codes with their evidence crops, the geometric
composite, the store's dismissal semantics, the resumable pass, the IPC surface and both
panels.

Two mitigations are structural rather than promised, and they are why this condition is
survivable in a shipped build:

* **The focus head can only exonerate.** `focus::HEAD_OVERRULES_AT` lets it withdraw a
  softness claim and never lets it make one, so an untrained head cannot convict a
  photograph. Section 13's second acceptance criterion is additionally met by *geometry* -
  `BOKEH_RATIO` - so it does not depend on the model at all.
* **The eye head's absence is visible rather than silent.** With no head, no
  `face_eye_state` rows exist, `gating_faces` is zero, and `closedEyeRatio` of 0.0 over
  zero faces is distinguishable from 0.0 over six by construction.

**No later phase may claim a technical quality result on real photographs until this
closes.** Training needs the labelled crops section 9 gives DATA and, for the eye head,
consented face data - phase 06's condition C1, unchanged, and a consent question before it
is a machine-learning one.

### C2 - the calibration table is derived, not measured

`camera_calibration.toml`'s twenty rows come from published sensor specifications and from
the physics relating pixel pitch to read noise. Not from a slanted-edge chart, an ISO
ladder and a bracketed highlight ramp shot on each body, which is what section 8 step 1
asks for and what `AURA-ML-5037`'s runbook describes.

The distinction matters for how a wrong row behaves: a specification-derived row is
systematically slightly wrong for every copy of that body, where a measured row is right
for the copy it came from. Neither is a guess, and both are better than the fallback - but
only a measured row earns the zero confidence penalty these rows are given.

It is blocked by phase 02's first exit condition (real camera files), and the shipped file
says so at the top.

### C3 - clipping is measured on the proxy, not the RAW histogram

Section 6.3 asks for clipping "from the RAW histogram before tone mapping". This build
computes it from the 2048 px proxy, which is after phase 02's documented render.

Bounded rather than open-ended: the proxy is built from the RAW's full range, so the
*count* of clipped pixels is right; what a RAW histogram would improve is the estimate of
how far above the clip point the signal went, which is one input to the
`recoverable`/`lost` boundary. `PixelBuffer` carries its `PixelSource`, so a later build
can tell which frames were judged this way.

### C4 - the tears intent rule needs phase 10

Section 6.4's third intent rule - "tears detected in Phase 10" - cannot be evaluated.
`IntentInput::tears` is defined, documented, wired through and always false.

The degradation is in the safe direction: a missing exoneration makes a frame *look* more
defective, so a tearful closed-eye photograph reaches a review queue rather than a
delivery. It is a one-line change when phase 10 lands, and the eval harness prints the
condition beside the intent-rule test so nobody forgets which of the four is inert.

### C5 - the per-scene score calibration is the identity map

Section 6.5's second half - "scores are mapped through a per-scene isotonic regression
fitted on labelled keeper/reject data" - ships as the identity.

The machinery is real and tested: `Isotonic::from_knots` refuses a non-monotone map,
`SceneCalibration` installs one per scene, and
`ml/models/integrity/eval_integrity.py --fit-calibration` fits them with PAVA and prints
the eleven knots. There is no labelled keeper/reject data here to fit on.

**Section 13's fifth criterion is met without it**, by construction rather than by
fitting - every sub-score is computed against its scene's own tolerance before it reaches
the composite - and the eval harness asserts a spread under 0.02 across all 23 scenes. The
isotonic layer would refine that, not create it.

### C6 - EXIF stabilisation is not read

`MotionContext::stabilised` is always false, because phase 01's schema does not promote
the stabilisation tag to a column and this phase does not open files.

The effect is that the reciprocal rule is applied at its strict form for every frame,
which makes camera shake **harder** to claim rather than easier - the safe direction. A
stabilised frame at 1/15 s therefore looks like a frame three stops below hand-holdable,
and the EXIF term only ever *raises* an already-established smear's severity. It never
creates a verdict on its own: the pixels have to show the smear first.

---

## 6. Carried forward from earlier phases

Phase 02's three exit conditions are still open and are carried again: real camera files,
a photographed ColorChecker, and a three-OS CI run. **The first real camera file is a Sev 2
trigger that reopens phase 02's criteria whatever phase is in flight** (ADR-0006).

Phase 09 has a direct interest in the first of the three, more than any phase since 02:
condition C2 above is entirely blocked by it, and the whole camera-fairness argument is
measured against a table nobody has been able to measure.

Phase 05's condition C10 - the perceptual embedding is a placeholder - is unchanged.
**Phase 09 does not depend on it**, which is worth stating because every phase since 05
has: nothing in this crate reads an embedding. Sharpness, motion, exposure, noise and eyes
are all measured from pixels.

Phase 06's conditions are unchanged. C1 - the face models are placeholders - matters here:
a wedding whose faces were not found is a wedding judged on frame-wide sharpness, and
`IntegrityOutline::subject_aware` is the number that says so.

Phase 07's conditions are unchanged. Phase 09 degrades gracefully without it: an
unclassified frame is judged on `SceneProfile::neutral`, which is invariant 7 degraded
rather than broken, and the verdict's confidence drops by 0.10 and says why.

Phase 08's condition **C4 has its blocker removed**. It said the two face signals were
unwired because `PeopleService` had no way to supply them; the `FaceRef` amendment in
ADR-0019 section 3 supplies both. Wiring `aura-brain-wedding`'s pass context is phase 08's
code and outside this phase's allowed areas, so the condition stays open with a note that
it is now a small change rather than a contract change.

---

## 7. Rollback

| Switch | How |
|---|---|
| Feature off | Do not call `IntegrityPass::run`. Nothing else in the product requires `image_integrity`; `IntegrityOutline::coverage` reports 0.0, `IntegrityService::of_image` returns `None`, and the card draws "not checked" rather than a clean verdict. |
| Config rollback | The shipped `camera_calibration.toml` is embedded in the binary. An installation override that will not load falls back to it with a logged refusal; the *embedded* file failing to load is `AURA-ML-5036` and halts, which is the correct direction for a measurement table to fail in. |
| Re-analysis | `IntegrityPass::run` rebuilds every verdict whose versions are behind and re-applies every dismissal. It touches no pixel it did not read, opens no original, and writes nothing outside its two tables. |
| Migration reversible | Yes. The down migration is four drops, written out at the top of `0009_integrity.sql`. Everything it costs is recomputable from the pixels except the dismissals, so the runbook says to export `user_reviewed` rows first. |
| Model rollback | `models.lock` pins by digest; the registry keeps the previous version until a new one has completed one real inference (`AURA-ML-5009`). A `MODEL_VER` bump makes every verdict stale and the background pass replaces them. |
| Version rollback | Three columns - `model_ver`, `analysis_ver`, `calib_ver` - and `AURA-ML-5033` when they disagree with this build. Two vintages are never compared; the outline reports the lowest present. |
| Threshold rollback | `calib_ver` is on every verdict. A phase that acted on version 1's numbers and one that acted on version 2's are distinguishable after the fact, which is what makes a calibration change auditable rather than merely reversible. |

---

## 8. What phase 10 inherits

Five rules, and every later phase inherits them.

- **`IntegrityService` is the only way to ask whether a frame worked.** No phase may keep
  its own sharpness measure, its own blink detector or its own idea of what "recoverable"
  means. This is phase 05's rule for `SimilarityIndex`, phase 06's for `PeopleService`,
  phase 07's for `StoryService` and phase 08's for `MomentService`, a fifth time and for
  the same reason: two answers to "is this frame sharp" is two culling decisions that
  disagree.

- **A measurement is evidence; the deciding phase owns the cull.** Nothing in
  `aura-brain-photo` rejects, ranks or orders a frame, and there is no column, field or
  command that would. `technical_score` is the closest this product has yet come to
  something that *looks* like a verdict, and section 12's first failure mode is what
  happens when somebody reads it as one. Phase 05 wrote this about distances, phase 07
  about scene tolerances and phase 08 about groupings; this is the same rule about
  technical quality, and it is the hardest one to keep.

- **Three version columns, because they invalidate three different things.** `model_ver`
  invalidates the learned sharpness and every eye state, `analysis_ver` the motion kind,
  the exposure verdict, the noise figure, the flags and the score, and `calib_ver` every
  *normalised* number. `AURA-ML-5033` exists so a comparison across any of them never
  happens silently. Fifth phase, fifth version-drift code.

- **Report coverage, and say what the denominator is.** `IntegrityOutline::coverage` is
  measured against **every photograph**, unlike phase 08's - a technical verdict needs only
  pixels, so a frame with no verdict is this phase's gap. And phase 09's own refinement:
  `subject_aware` is the second number, because a wedding at 100 % coverage and 2 %
  subject-aware has been judged on frame-wide sharpness nearly everywhere, which is the
  ordinary global measure this phase exists to replace.

- **A photographer's dismissal is unbeatable, and it is re-applied rather than excluded.**
  `image_integrity.dismissed` is re-applied inside the upsert a re-analysis performs. The
  difference from phase 08's locked moments is deliberate: a locked moment *replaces* the
  machine's grouping, whereas a dismissed flag does not replace the measurement - the frame
  still has to be re-measured when the calibration table moves, and the photographer's
  disagreement is carried onto the new measurement.

And three things phase 10 should know before it starts:

**`IntentInput::tears` is waiting for you.** Section 6.4's third intent rule is wired
through, documented and always false. Filling it is one line in
`Analyser::judge_eyes` and it closes condition C4.

**Two flags are good news, and one is neither.** `INTENTIONAL_MOTION` and
`EYES_CLOSED_OK` describe something *right* with a photograph; `NO_SUBJECT_DETECTED`
withdraws a claim rather than making one. `IntegrityFlags::DEFECTS` is the mask that knows
the difference and it exists so that no later phase writes its own `!matches!`.

**A `technical_score` of 0.31 is not a rejection.** It is a scene-weighted geometric mean
of five measurements, and the frame it describes may be the only photograph of the ring
exchange. Phase 12 knows that; this crate does not, and must not.
