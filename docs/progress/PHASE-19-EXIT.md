# PHASE-19 exit report - Local Light Sculpting

**Branch:** `claude/phase-19-84iyxx` · **Gate:** `aura-cli verify --phase 19` exits 0 ·
**Status:** implemented **conditionally**, on the five conditions in section 8.

## 0a. What changed when this branch was merged into `main`

Everything in section 0 below describes the branch **as it was written**, on top of phase 15.
By the time it merged, `main` carried phases 16, 17 and 18, and three things about the report
moved:

* **The numbers this phase claimed are renumbered.** Phase 19 had taken migration 16, error
  codes `AURA-ML-5066` to `5071`, and ADR-0033 and ADR-0034 - all of which `main` had since
  given to tone curves, style learning and semantic masks. They are now migration **19**,
  `AURA-ML-5084` to **5089**, and **ADR-0039** and **ADR-0040**. `APP_SCHEMA_VERSION` is 19 and
  the migration's rollback note names schema 18 as the floor it returns to. Nothing about the
  behaviour changed; every reference in this report reads with the new numbers.
* **C4 is half closed.** Phases 16 and 17 exist now, so the two skipped dependencies are one.
  This phase still reads phase 15's per-scene luminance bands directly rather than phase 16's
  refined ones, and still does not read a phase 17 style profile - but both are now
  *connections that could be made* rather than phases that do not exist.
* **C1 changed its reason and not its status.** Phase 18 ships `MaskService`, but
  `AppState::local_pass` does not call `LocalPass::with_masks`, so on a merged build every
  operation is still gated and `LocalOutline::mask_covered` still reads zero. The gap is now a
  wiring task rather than a missing phase, and it is the one piece of phase 19 the merge does
  not carry. Everything C1 says about what may not be claimed still holds.

## 0. Read this first: the phase this one consumes had not shipped when this was written

This repository is at phase 15. Phase 19 depends on phases 15, 16 and 18, and **16, 17 and 18
do not exist here.**

That is not a caveat at the bottom of a report. It is the largest single fact about what this
phase can claim, and it shapes everything below:

* **Phase 18 owns masks**, and every operation in this phase is local, so every operation needs
  one. On this build every operation is gated, `LocalOutline::mask_covered` reads zero, and a
  photographer would see a Local panel full of "not available". That is condition **C1**.
* **Phase 16 owns tone curves and colour grading.** This phase reads phase 15's per-scene
  luminance bands directly, which is the number phase 16 would have refined rather than
  replaced, so the dependency degrades rather than breaks.
* **Phase 17 owns the photographer's personal style**, which this phase does not read at all.

The work was done under the contract-first handoff of the phase ritual's step 4: a lane
consumes another lane's work through the frozen interface, using a fixture until the real
implementation lands. `MaskField` is that interface, `local::fixtures` supplies it, and nothing
in `aura-brain-photo::local` can make a mask.

**A reviewer should read section 8's conditions before reading any number in section 3.**

## 1. What shipped

One frozen contract, one module of fifteen files, one migration, three shaders and a processor
reference, one IPC surface, one panel, two ADRs, six runbooks, two Python scripts, one product
document and a gate.

`aura-core::contract::local` freezes the shape. `LocalLightPlan` is section 5's struct with
five recorded spellings (ADR-0039 section 2), plus `MaskField` - the input port phase 18 fills -
`LocalOp` and its priority order, `FaceZone` and its ten named moves, thirty reason codes,
`LocalOutline`, `LocalOverride` and `LocalService`. **There is no field anywhere in it that
could hold image data**, which is what makes "all local work is stored as masks and parameters
and is fully reversible" a property of the shape rather than a promise.

`aura-brain-photo::local` decides. `policy.rs` loads 22 argued-over scene rows and refuses a
broken file. `measure.rs` reads the pixels once. `luminosity.rs` splits a lift so shadows move
and highlights do not. `face_light.rs` solves every face in a frame together.
`subject.rs`/`background.rs` are one decision in two halves. `freqsep.rs` separates three bands
and returns two. `dodgeburn.rs` places ten named moves and derives the map from them.
`shine.rs` finds specular sheen and reduces luminance only. `governor.rs` spends one allowance.
`guard.rs` turns the contract's predicates into this phase's errors. `plan.rs` composes them.
`store.rs` and `api.rs` own migration 19 and the frozen service. `fixtures.rs` is the synthetic
ground truth.

Migration 19 adds `local_light_plan`, `local_light_face`, `local_light_gate` and
`v_local_coverage`. **There is no mask column, no matte, no blur and nowhere to put one**, and
the gate scans for one on every run.

`aura-render` gains three shaders and the processor reference they are held to. They are the
first shader *libraries* in the product - no entry point, called by `stage_masks` - and
`shader_parity.rs` was narrowed and extended to cover that.

The IPC surface is six commands (ADR-0040). The Local panel makes an invisible edit visible: a
strength per operation, a gated operation shown as *unavailable* rather than as off, what each
face was moved by **and what stopped it**, and a group the caps could not even out reported as
exactly that.

## 2. Acceptance criteria (section 13)

| Criterion | Status |
|---|---|
| Faces in difficult light are lifted naturally without haloes or a glowing look | **met on fixtures.** The glow is prevented by the luminosity split and gated by `a_dark_face_lifts_mostly_through_the_shadows`; the halo by the monotonicity properties in `local_eval`, which **found a real one** (ADR-0039 section 7). Not measured on a photograph - C1, C3 |
| Bright or saturated backgrounds stop competing with subjects, invisibly | **met on fixtures.** Three measured triggers, and the pairing holds the frame's mean luminance to 0.028 against a 0.030 tolerance |
| Dodge and burn shapes form without touching skin texture | **met, structurally.** The finest band is never produced, so no operator can reach it; the mid band moves by at most 5 % and is gated |
| Everyone in a group photo is lit consistently | **met, under a rewritten guarantee.** The absolute reading of section 10.1 is unachievable and ADR-0039 section 6 says why. What is guaranteed: reach the threshold whenever the caps allow, and never make a group less even. Measured 0.343 apart before, 0.272 after, with nobody darkened |
| All local work is stored as masks and parameters and is fully reversible | **met.** No path, no rendered output, no applied flag anywhere in migration 19 or on the IPC surface |
| Expert reviewers rate the edits as invisible rather than obvious | **not done and cannot be** - C3 |

## 3. What the section 10.1 gates measured

`cargo test -p aura-brain-photo --test local_eval` - **38 gates, all green.**

| Gate | Threshold | Measured |
|---|---|---|
| Halo: no edit is stronger further from the subject | exact, every fixture | **met**, and the negative control confirms the gate bites |
| Face lighting reaches the band or names what stopped it | exact, every fixture | **met** |
| Noise cap binds on a high-ISO frame and is explained | exact | **met** |
| Subject/background pairing holds the mean luminance | <= 3 % | **2.81 %** worst case |
| Group spread after lighting | threshold or improved | **0.343 → 0.272**, nobody darkened |
| Mid-frequency band energy after shaping | <= 5 % | **met**, every shaped face |
| Low-confidence masks reduce strengths measurably | exact | **met**; a hopeless mask produces nothing at all |
| Determinism | byte-identical | **met**, every fixture |

`aura-cli verify --phase 19` is the assembly proof and exits 0. It prints what it does *not*
prove at the end of every run.

## 4. Performance (section 11)

| Row | Budget | This build |
|---|---|---|
| Local decisions + map generation per image | <= 80 ms | guarded on the processor path; the GPU row is waived (ADR-0029 section 4) |
| Render overhead for local application (proxy) | <= 12 ms | **waived twice over** - no `wgpu` backend and no phase 18 matte to apply through |
| 1,000 selected images total | <= 90 s | extrapolated from the per-image figure and printed by the test |
| Storage per image | *not budgeted by section 11* | **1,064 B measured**, against the 1 KB every phase since 09 has aimed at |

The storage figure is the one worth reading. It started at **2,236 B** and the reduction is
recorded in `perf/budgets.toml`: the shaping was a child table of one row per zone, and ten
zones on each of four faces cost 1,286 B on its own. Every zone is a pure function of the face
region, the light direction and the strength, so the catalog stores those four numbers and
`dodgeburn::zones_for` reproduces the list. The panel still shows every zone by name because
they are regenerated on read.

Two further reductions were considered and rejected, and the file says why: folding the lit
faces into a document removes the index phase 20 joins on, and integer reason codes create a
second vocabulary a newer build cannot be read with.

## 5. What this build's numbers are and are not claims about

**Every mask in every gate is a fixture's.** `fixtures::mask_over` builds a field aligned
exactly with the painted region, at confidence one and edge quality one, because phase 18 has
not shipped and a fixture with an invented ragged matte would be measuring an invention. The
gates about *gating* - a weak mask produces a gentler edit, a hopeless one produces none, every
operation is gated when its mask is absent - will still be true when phase 18 arrives. The
gates about *quality* will need re-measuring against real mattes.

**The learned targets are never consulted.** `TARGET_HEAD_TRAINED` is false, so
`Analyser::learned_targets` returns `None` and phase 15's own per-scene bands are what runs. The
plan carries `TargetHeadUnavailable` so nobody mistakes one for the other.

**No number here is a claim about subtlety.** Section 10.1's seventh gate is an expert rating
of four hundred frames. `subtlety_report` in `ml/models/local/eval_local.py` reports how much
of the allowance an edit spent, which is a measurement of how much changed rather than of
whether it was right, and it refuses to return a verdict when the input carries no ratings.

## 6. Three defects the tests found, recorded because they are the useful part

**A halo made by arithmetic that looked conservative.** `apply_face_light` evaluated its
luminosity weights on the partially-edited pixel, so the highlight restraint grew quadratically
in the matte while the lift grew linearly. Past about half coverage the restraint overtook: a
mid-bright pixel's edit peaked at 0.022 at half coverage and fell to 0.014 at full, which means
**a bright pixel received more lift at the mask's edge than at its centre** - a bright rim.
Both weights now read the input pixel and the whole edit is linear in the matte, on the
processor path and in the shader. ADR-0039 section 7.

**A cap detector that could never fire.** The joint face solve reported whether a lift had been
capped by comparing against the group's converged common target - which has already absorbed
the caps, because that is what makes it reachable. Nothing was ever reported as capped. It now
compares against the scene's band.

**A joint solve that could brighten a face past the band.** One blown face in a family formal
dragged the common target above the scene's band, and everybody else was lifted past where the
scene wanted them to meet it. `reachable` now clamps every move to lie between the face and the
band, which makes the joint solve something that can only ever reduce a move rather than create
one.

## 7. What was deliberately not built

**A geometric mask fallback.** A face box and a subject box would let this phase do something
today rather than gate everything. It was rejected for two reasons, either sufficient: a
rectangle's edge does not follow a person, so an edit through it leaves a bright rim beside
them; and it would be a second answer to "where does the subject end" that would disagree with
phase 18's when it arrives, leaving a gallery with two different edits in it that nobody could
tell apart by looking. ADR-0039 section 4.

**Anything from phase 20, 24 or 25.** No blur radius, no smoothing strength, no texture
parameter, no object removal, nothing that reads a second photograph. All three boundaries are
structural: the types cannot express them and the gate scans the schema for them.

## 8. Conditions

Five, and the first three are Sev 2 triggers.

**C1 - every mask is a fixture's.** *(Sev 2.)* Every operation in this phase is gated on a real
build, `mask_covered` reads zero, and every quality gate was measured against a perfect
synthetic matte. When this was written the reason was that phase 18 did not exist; after the
merge (section 0a) phase 18 ships `MaskService` and nothing calls `LocalPass::with_masks`, so
the reason is a missing connection and the consequence is identical. **No later phase may claim
a local-light quality result until the pass reads real mattes and the gates are re-measured
against them.**

**C2 - the learned targets are untrained and are never consulted.** *(Sev 2.)* Section 8 step 1
asks for targets extracted from expert difference maps and there is no corpus of expert edits
in this repository. `ml/models/local/train_light_targets.py` is the extraction, written and
self-tested and unable to run on anything real here. While `TARGET_HEAD_TRAINED` is false, the
head is never called and phase 15's own bands run instead. Closes when a corpus exists.

**C3 - the expert subtlety study and the halo audit do not exist.** *(Sev 2.)* Section 10.1's
seventh gate is "expert subtlety rating >= 4.2/5 with no 'obviously edited' flags"; section 9
gives QAIQ four hundred frames to hunt haloes in. Neither exists here and no arithmetic
substitutes for either. **The headline KPI of this phase is unmeasured.** Closes when a QAIQ
audit set and a reviewer panel exist.

**C4 - phases 16 and 17 are not read.** The phase document lists 15, 16 and 18 as dependencies.
Phase 16's tone curves would have refined the per-scene luminance bands this phase lifts faces
toward, and phase 17's personal style would have shifted them per photographer. When this was
written neither phase existed; after the merge (section 0a) both do, and neither is read - so
the condition has stopped being about missing phases and started being about an unmade
connection. The dependency still degrades rather than breaks, but **a photographer's own style
still does not reach this phase's decisions**, and it should. Closes when this phase's band
source is re-pointed at phase 16 and its solved parameters are passed through phase 17's lean.

**C5 - the group-fairness guarantee is weaker than section 10.1's words.** Section 10.1 asks
for an absolute spread threshold; what is guaranteed is that the threshold is reached whenever
the caps allow and that a group is never made less even. ADR-0039 section 6 has the argument
and `docs/local-light.md` says the same thing in the product's own voice. This is a **recorded
divergence rather than a gap** - it does not close, it stands unless somebody overturns the
argument.

## 9. Rollback

Feature flag: `LocalPass::enabled(false)`. A disabled pass still writes a plan per frame - one
that does nothing and says `local_disabled` - because a frame with no plan and a frame the
photographer switched off look identical in a coverage report.

Migration: reversible and recomputable.

```sql
DROP VIEW  IF EXISTS v_local_coverage;
DROP TABLE IF EXISTS local_light_gate;
DROP TABLE IF EXISTS local_light_face;
DROP TABLE IF EXISTS local_light_plan;
DELETE FROM schema_version WHERE version = 16;
```

Every row is derived from pixels, phase 06's faces, phase 07's scenes, phase 09's noise and
phase 15's bands, so a re-run reproduces it exactly - with the usual exception. The six
per-operation strengths in `user_strengths` are not derivable from anything; export that column
first. They also reach `edit_recipes` and the sidecars beside the RAWs, which is the second copy
that makes the loss survivable.

Model: `TARGET_HEAD_TRAINED` is false and no head is consulted, so there is no model version to
pin back.

## 10. What phase 20 inherits

**`LocalService` is the only way to ask how light was shaped inside a photograph.** Thirteenth
service of its kind. Phase 20 retouches skin this phase has already evened and must not do it
twice; `idx_local_evened` is the index that query uses.

**`shaping_ver` is load-bearing and easy to forget.** The shaping zones and their grids are both
derived from four stored numbers, so a change to `zones_for` or to `grid` moves delivered pixels
without moving one stored value. A build that edits either without bumping `shaping_ver` will
pass every test in this repository.

**A budget is a stored number with a schema check on it.** Six individually defensible
adjustments are how a gallery quietly starts looking processed, and the per-image allowance is
what stops it. Phase 20 adds a seventh operation and inherits the allowance rather than getting
its own.
