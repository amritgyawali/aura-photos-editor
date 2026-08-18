# ADR-0031 - Exposure, white balance and the skin constraint

**Status:** accepted · **Date:** 2026-08-18 · **Phase:** 15 · **Supersedes:** nothing

Phase 15 section 4 asks for no ADR by name. It needs two anyway, and this is the first of
them: section 5 freezes a contract, section 6.3 makes a fairness claim that has to be
falsifiable, and section 8 step 1 asks for a training set that does not exist in this
repository. All three are decisions, and a decision nobody wrote down is a decision the next
phase re-argues from scratch. The second document is
[ADR-0032](ADR-0032-tone-ipc-surface.md), which covers the wire.

The ADR numbering in this repository is sequential across the whole project rather than
aligned to phase numbers.

## 1. Context

Fourteen phases decided *which* photographs are delivered and *how* a decision becomes
pixels. None of them decided what a photograph should look like. Phase 15 is the first that
does, and section 0 calls it "the most visible AI decision in the product" for a reason a
photographer would recognise: exposure and white balance are what the eye reads in the first
second, before sharpness, before framing, before which frame of the burst was kept.

Three things make it hard, and they are not the same difficulty.

**The subject, not the frame.** Section 1: "the bride's face is the anchor, not the mean
luminance of a dark reception hall." Every auto-exposure that photographers complain about is
frame-referred - it computes a statistic over all the pixels and moves the exposure until
that statistic hits a target. In a dark reception that lifts the room and leaves the faces
where they were; on a bright beach it does the reverse. The fix is not a better statistic, it
is a different anchor.

**Mixed light.** A reception is tungsten uplighters plus daylight through a window plus an
LED panel the videographer brought plus a purple wash on the dance floor. There is no single
illuminant, so there is no single correct white balance, and a solver that pretends otherwise
produces a frame that is right on the faces and wrong on the wall or the reverse. Section 2.1
says what to do about it: pick the light that governs the subject, mark the frame, and leave
the local correction to phase 18.

**Skin, and the direction the error runs.** Section 6.3 is the part of this phase with
consequences outside the product. A white-balance solver has a free parameter and skin is the
most visible surface it acts on, so any target it is given becomes a target it moves skin
toward. If that target is a constant, the constant is somebody's skin, and every other
person's skin is corrected away from itself and toward that person's - which is what skin
lightening looks like when it is implemented by an engineer who thought they were correcting
a colour cast.

## 2. Decision: eight spellings differ from section 5, and here is each one

Section 5 freezes a struct. The frozen shape in `crates/aura-core/src/contract/tone.rs`
differs from it in eight places. Section 2 of the phase ritual says an interface change after
freeze needs an ADR amendment; these are made *at* freeze, before any solver existed, and are
recorded here because the phase document is the specification and the code is what shipped.

| Section 5 | Shipped | Why |
|---|---|---|
| `image_id: ImageId` | `ImageId` is an alias of `PhotoId` | Seven contracts already alias it. A second id type for the same row is a conversion nobody can remove later. |
| `alternatives: Vec<(f32, f32, f32)>` | `Vec<ToneAlternative>` | A bare triple cannot say *why* a runner-up lost. Section 13 asks that every value be overridable, and an override is a decision - a panel that offers three numbers with no argument attached is asking the photographer to guess. |
| `illuminants[].cct` | `cct_k` plus `uv`, `tint` and `chroma` | A correlated colour temperature is the projection of a chromaticity onto the Planckian locus. A fluorescent tube is nowhere near that locus and a cheap LED is further; its CCT is a number that exists but does not describe it. The estimate is the chromaticity; the CCT is what a slider shows. |
| `reasons: Vec<Reason>` | `Vec<ToneReason>` carrying a typed `ToneCode` | Phases 09 to 13's rule. A code can be translated, counted and refused by a registry; a sentence can only be printed. |
| — | `SkinLocus` added | Sections 6.2 and 6.3 both need it and neither can be implemented without it. A per-identity constraint that lives inside one solver is a constraint the next phase re-derives differently. |
| — | `ReferenceFrame` added | Section 6.4 hands 3-5 anchors per segment to phase 25. A handoff with no type is a handoff nobody can test. |
| — | `ToneOutline` added | Phase 05's rule for the ninth time: report coverage, and say what the denominator is. |
| — | `ToneService` added | Six later phases consume a tone decision. A contract with no entry point makes each of them find its own way in, and this phase's whole argument is that there must be exactly one. |

Two fields in section 5 were kept although a reviewer asked whether they earn their storage.
`subject_luma_before` and `subject_luma_target` are the entire evidence behind the exposure
number, and a decision that stores its answer but not what it was aiming at cannot be
disagreed with.

## 3. Decision: twelve modules, not the six section 4 names

Section 4 names `{exposure, wb, illuminant, skin_locus, neutrals, solve}.rs`. Six more shipped
beside them. Each is here because it is a *different kind of thing* from the six, not a
subdivision of one:

- **`targets.rs`** loads and validates `config/exposure_targets.toml`. It is configuration
  parsing with a refusal path (`AURA-ML-5063`), and putting it inside `exposure.rs` would put
  a file reader inside a solver.
- **`stats.rs`** reads the pixels once and produces everything the other modules consume -
  histogram, per-channel sums, block chromaticities, clipping fractions. Section 8 step 2 asks
  for it explicitly and it belongs to no single consumer. Phase 14's `spatial::Stats` argument
  applies unchanged: a frame-wide statistic measured per consumer is a statistic that drifts.
- **`reference.rs`** implements section 6.4, which is not a solve. It ranks frames that have
  already been solved.
- **`analyse.rs`** is one frame in, one estimate out - the composition of the other modules,
  and the only place that knows their order.
- **`store.rs`** (with `store/codec.rs`) is migration 15's three tables.
- **`api.rs`** is the frozen `ToneService` and the resumable project walk.
- **`fixtures.rs`** is the synthetic ground truth every section 10.1 gate is measured against.

The rule this follows is the one phases 09 and 11 used: a module boundary that matches the
phase document's file list but forces a file reader, a statistics pass and a persistence layer
into a solver is a boundary that makes the document look right and the code worse.

## 4. Decision: the skin target is measured, and the schema cannot express an alternative

Section 6.3 says skin targets are "measured per identity from the wedding's own best-lit
frames, not from a fixed 'ideal' skin value". That is a promise, and a promise in a document is
worth what the next engineer in a hurry decides it is worth. This phase makes it a property of
the shapes instead.

**There is no field, column or config key anywhere in phase 15 that could hold an ideal skin
value.** Not in `aura_core::contract::tone`, not in `image_tone_estimate`, not in
`identity_skin_locus`, not in `config/exposure_targets.toml`. `identity_skin_locus` holds one
chromaticity, one radius and one luminance *per person*, keyed by `identity_id`, and has no
row that is not about a specific person photographed at this specific wedding. A schema that
cannot express a fixed target cannot drift into using one.

What the solve does with it is a **hard constraint**, not a post-hoc nudge. `solve.rs` walks
`CORRECTION_STEPS = 20` points along the segment from "leave the light alone" to "remove it
completely" and takes the first point at which every identity in frame with a usable locus
lands inside its own locus. A hypothesis that cannot satisfy the constraint at any point on
that segment loses to one that can, whatever its grey-world score. Section 6.3's "hard
constraint in the solve, not a post-hoc adjustment" is the linear scan, and the scan is linear
rather than bisected on purpose: with two people whose loci differ the satisfying set is not an
interval, and a bisection returns an arbitrary point in it while a scan returns the first.

Three guards keep a *weak* locus from being worse than none:

- **`MIN_LOCUS_SAMPLES` frames or the locus does not exist at all.** A locus fitted to two
  frames looks like evidence and is noise; the estimate emits `ToneCode::SkinLocusUnavailable`
  and says so in the panel instead.
- **Only frames that were already well solved contribute** (`MIN_CONTRIBUTING_CONF = 0.70`,
  `MIN_FACE_QUALITY = 0.45`, luminance inside `CONTRIBUTING_LUMA`). A locus accumulated from
  frames this solver got wrong is a solver agreeing with itself.
- **The radius is bounded at both ends** (`SkinLocus::MIN_RADIUS`, `MAX_RADIUS`). A person shot
  under one light all day has a measured spread near zero, and a constraint that tight rejects
  every honest answer at the reception; a person shot under six lights has a spread that
  constrains nothing.

**Where the fairness claim is falsifiable.** The evaluation harness buckets synthetic
identities by Monk-scale group and asserts both halves of section 10.1: mean dE00 at most
`SKIN_DE00_CEILING` and a spread across buckets of at most `SKIN_DE00_SPREAD = 1.0`. Two numbers
rather than one, because a solver can be uniformly mediocre or selectively good and only the
second is a fairness failure. The buckets live in `tests/eval` and never reach the catalog -
measuring a disparity needs the grouping, and shipping the grouping into a product database is
how a measurement becomes a demographic record. Phase 06's condition C5 is the same rule read
in the same direction.

**What this does not prove.** See section 8.

## 5. Decision: the two thresholds that are not round numbers

`REVIEW_WB_BELOW = 0.55` and `CLEAR_MARGIN = 0.20`. Both are cited from code and both are
derived rather than chosen, so the derivation belongs here.

The white-balance confidence is built from three terms: how far the winning hypothesis's cost
sits below the runner-up's, whether the winner was anchored by a known neutral or by
grey-world, and whether the skin constraint was available and satisfied. `CLEAR_MARGIN` is the
cost separation at which the first term stops contributing doubt. It is set at 0.20 because
that is where, on the synthetic fixture set, the hypotheses that disagree are the ones whose
answers differ by more than section 10.1's own tolerance - 200 K and 4 tint units. Below a
0.20 separation the two candidate answers are further apart than the gate the phase is measured
against, which is exactly the definition of "the solver did not decide this".

`REVIEW_WB_BELOW` follows from it. A frame reaches the queue when its confidence is low enough
that a photographer looking at it would probably change something. Setting it at 0.55 puts the
boundary just below the confidence a frame gets when the hypotheses agreed but no neutral and
no locus were available - the common, honest, grey-world-only case, which should *not* fill a
queue - and just above the confidence a frame gets when the hypotheses disagreed by more than
`CLEAR_MARGIN`. A round 0.5 puts it inside the first group and makes the queue four thousand
frames long; a round 0.6 puts it inside the second and hides the frames the queue exists for.

Both are constants in the frozen contract and in `solve.rs` rather than in
`exposure_targets.toml`, deliberately. They are properties of *how sure the arithmetic is*,
not product decisions about how a scene should look, and a threshold in a config file is a
threshold somebody tunes until the queue is empty.

## 6. Decision: three storage choices, and one crate boundary this phase does not cross

**Three JSON documents, not three child tables.** `illuminants`, `reasons` and `alternatives`
are always read together by the panel that draws them and are never queried across. Three child
tables would cost more in row overhead and index than the documents do. The documents are
positional arrays rather than objects, and `tone::store::codec` is the one place that knows what
the positions mean; the round-trip tests in that file are what keeps that true.

**The store reads `segments` directly, and that is not a second story engine.**
`ToneStore::segments_of` needs the list of chapters to select reference frames for. Phase 07's
rule is that `StoryService` is the only way to ask what a photograph is *of*; this does not ask
that. It reads back the segmentation `StoryService` already made, as opaque ids, to answer
"which chapters exist". It classifies nothing and cannot produce a segmentation of its own. The
alternative - a frozen-service round trip per chapter, inside the write path of a pass - is the
thing phases 09 and 11 already rejected for `moment_images`.

**`aura-brain-photo` does not depend on `aura-recipe`, and the tone pass does not write a
recipe.** This is the boundary that matters most in this phase. Phase 14's rule is that
`aura_recipe::schema::merge` is the only function in the workspace that writes one recipe into
another, and that it is where `user_edited_fields` is honoured. If the tone pass could write a
recipe there would be two ways to edit one, and the second would be the one that does not check
the protection. So `ToneService::set_override` records the *disagreement* - it sets
`user_edited` on the estimate row - and the caller writes the same three values through the
merge. Two writes rather than one, on purpose, and ADR-0032 section 3 is where they are
sequenced.

## 7. Decision: the error domain

Six codes, `AURA-ML-5060` to `AURA-ML-5065`, in the shape phases 09 and 11 established, plus
one that is new here.

| Code | Severity | Raised when |
|---|---|---|
| `AURA-ML-5060` | degraded | Stored estimates came from a different head, a different arithmetic or a different target table. Ninth version-drift code. |
| `AURA-ML-5061` | item_failed | An exposure or white-balance override was refused: no estimate, an empty override, or a value outside its documented range. |
| `AURA-ML-5062` | item_failed | One photograph could not be estimated. The frame keeps the camera's own values and the pass continues. |
| `AURA-ML-5063` | run_blocking | `config/exposure_targets.toml` was refused. The pass does not start, because a solver with no bands is a solver inventing them. |
| `AURA-ML-5064` | warning | A scene has no target row. The neutral band is used and `ToneOutline::untargeted_scenes` names the scene. |
| `AURA-ML-5065` | warning | The skin constraint could not be applied because no identity in frame has a usable locus. |

`AURA-ML-5065` is the one with no analogue in an earlier phase, and it is here because section
6.3's constraint failing *open* is invisible. A frame white-balanced with no skin constraint
looks exactly like a frame white-balanced with one; the only difference is that nothing stopped
the answer from putting somebody's skin somewhere it has never been. A warning plus
`ToneOutline::skin_constrained` makes the gap countable.

## 8. Decision: what this build's numbers are and are not claims about

Section 8 step 1 asks for "RAW + expert final edits with exposure/WB parameters across
traditions and lighting types". There is no such dataset in this repository, there are no camera
files (phase 02's condition, still open), and there is no photographed ColorChecker (phase 14's
condition C2, still open). Section 9 budgets ten days of a data engineer's time for it.

So this phase ships two heads that are **placeholders**: `white_balance` predicts a chromaticity
that is a deterministic projection of its input, and `exposure_scene` predicts an offset that is
a deterministic projection of sixteen scene features. Neither has seen a wedding.

What that leaves is worth being exact about, because the temptation is to describe it either
better or worse than it is.

**Real, measured, and passing:** the statistics pass, the neutral detector, the four hypothesis
generators, the chromaticity arithmetic, the mixed-light detector, the coloured-light policy, the
skin locus accumulator, the constrained solve, the exposure clamp, the reference-frame selector,
the store, the resumable walk and every refusal path. The section 10.1 gates are measured against
synthetic frames in `tone::fixtures` whose illuminant, subject luminance and skin chromaticity are
**painted into the pixels** and read back through the real pipeline, phase 10's method. Those
gates prove the arithmetic.

**Not proven by anything in this build:** that the white-balance head predicts the illuminant of a
real reception, that the scene exposure head predicts what an expert would have done to a real
faceless frame, that the skin dE00 figures correspond to a chart, or that any of it is fair across
real skin tones - the fairness gate is measured on synthetic identities whose loci were authored,
which proves the *mechanism* is per-identity and says nothing about a photograph.

This is condition C1 in the phase 15 exit report and it is a Sev 2 trigger. It closes with phase
05's condition C10 and phase 02's camera files rather than separately.

**One mitigation is structural rather than promised.** The learned heads can only ever be *one of
four* white-balance hypotheses, scored against the same skin constraint as the other three, and the
faceless exposure head's output is clamped by the same scene band and clipping tolerance a
face-anchored exposure is. An untrained head therefore produces a hypothesis that loses, not a
number that is applied. The second mitigation is the same shape: `face_anchored` is a stored bit
and a reported fraction, so a wedding where the scene head decided everything is a wedding that
says so in its own outline.

## 9. Consequences

- **`ToneService` is the eleventh frozen service and the only way to ask what colour the light
  was.** Phase 16 grades on top of these values, 17 shifts them, 18 corrects locally against them,
  25 normalises a gallery toward them, 26 matches two cameras with them and 27 checks them. None of
  them keeps its own illuminant estimator.
- **Phase 18 has its input.** `mixed_light` plus `Illuminant::region` on two entries is the note
  section 2.1 asks for, `idx_tone_mixed` is the query, and nothing in this phase attempts a local
  correction.
- **Phase 25 has its anchors.** `segment_reference_frames`, 3 to 5 per segment, with the
  temperature and tint to normalise toward. They are anchors and not corrections: no column here can
  say what another frame should become.
- **The 600 B storage budget in section 11 is not met and the measured figure is recorded instead.**
  See `perf/budgets.toml`; the decomposition and the argument are written there, and the waiver is in
  the exit report.
- **Nothing in this phase can cull, crop, grade or grade back.** There is no curve, contrast,
  saturation or mask anywhere in `image_tone_estimate`, which is section 2.2's boundary made
  structural.
