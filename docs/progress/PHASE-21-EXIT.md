# PHASE-21 exit report - Micro-Retouch Suite: hair, teeth, eyes, clothing and glare

**Branch:** `feat/phase-21-micro-retouch-suite` · **Gate:** `aura-cli verify --phase 21` exits 0 ·
**Status:** implemented **conditionally**, on the six conditions in section 8.

## 0. Read this first: what this phase can and cannot claim

Every algorithm in this phase is real, tested and enforced. Every *number* in it is measured
against synthetic frames whose strands, sheets, marks, teeth and catchlights were painted in by
`micro/fixtures.rs` and read back through the real detectors, the real operators, the real
naturalness guard, the real renderer and the real store.

Four separate things stand between that and a claim about a wedding photograph, and they close at
four different times:

* **Phase 06's face detector is a placeholder**, so on a real photograph there are no faces and no
  landmarks. Teeth, eyes and glare all need a face; without one, none of them runs. This closes
  with phase 05's condition C10 rather than separately.
* **No region reaches this pass.** Phase 18 ships `MaskService`, but nothing fills
  `MicroPass::with_regions` from it, so every operation is skipped and every plan says
  `region_unavailable`. That is a wiring task rather than a missing phase - the same state phases
  19 and 20 are in, and for the same reason.
* **All three shipped heads are untrained and none is consulted.** What runs is the measured
  detection in `hair.rs`, `glare.rs` and `clothing.rs`. ADR-0043 section 6 records why the
  argument for shipping a measurement is not the same for all three.
* **The naturalness audit does not exist.** Section 0's headline KPI - corrections judged natural
  at or above 95 % - needs four hundred frames and a panel of retouchers, and there are neither.
  This is the phase's own headline number and it is **unmeasured**.

**No later phase may claim a micro-retouch quality result until all four close**, and in
particular nothing in this build may be described as natural-looking on the strength of these
gates.

## 1. What shipped

**The frozen contract.** `aura_core::contract::micro` freezes the five operations, the ten regions
and their total mapping onto phase 18's twenty-class vocabulary, the `MicroField` port those
regions arrive through, the colour locus, the five clothing issues, the two glare methods, three
op families and their priority order, the naturalness guard and its report, thirty-three reason
codes, the plan, the outline, the override and `MicroService`. Nothing in it can express a
displacement, a scale, a colour target or a strength above a contract bound, and
`crates/aura-core/tests/micro_contract.rs` asserts it.

**The decision.** `aura-retouch::micro` is twelve modules. `matrix` loads the opt-in table and
refuses a file that raises a ceiling. `hair` finds thin high-contrast structures in the halo
outside the hair alpha, scores them against the detail of the background immediately behind them,
and caps their total area. `teeth` evens a mouth toward its own upper quartile and removes a share
of its own chromatic excess, clamped so teeth never outshine the skin around them. `eyes` takes
redness out of the sclera as chroma only and raises iris local contrast, with specular pixels
excluded by construction. `clothing` finds small anomalies inside the garment, classifies them by
shape, and refuses patterned fabric entirely. `glare` finds specular sheets over an iris. `borrow`
searches a sibling frame for an alignment and refuses to composite anything that still carries
information. `guard` measures the result through the renderer and withdraws per family. `ops` is
one frame in and one plan out; `store` owns migration 21; `api` is the frozen service and the
resumable walk; `fixtures` is the synthetic ground truth.

**The pixels.** `aura_render::micro` is the processor reference for all five operators plus the
borrow composite, and `micro_apply.wgsl` and `micro_borrow.wgsl` are the GPU halves, held to the
reference by `shader_parity.rs` and to review by `contracts.lock`.

**The storage.** Migration 21 adds `micro_plan`, `micro_matrix` and `micro_op`, two views and two
triggers. One trigger aborts any statement that would take a borrow's source away; the other
aborts an insert of a strap or a crease into a project that has not switched it on.

**The wire.** Nine IPC commands (ADR-0044), a Micro-Retouch panel with per-operation switches, and
a disclosure that appears in four places on the surface and five in the product.

**The models.** `flyaway_detector`, `glare_detector` and `lint_detector` are registered, signed and
carded. All three are untrained placeholders and none is consulted.

**The gates.** Ten in `tests/eval/micro_eval.rs`, a mechanical assembly gate in
`aura-cli verify --phase 21`, three budget rows in `crates/aura-perf/tests/micro_budgets.rs`, 57
unit tests in the micro modules, 23 contract tests, 9 renderer tests, 12 vitest cases and 17
Python self-test properties.

## 2. Acceptance criteria (section 13)

| Criterion | Status |
|---|---|
| Flyaway hair is calmed without damaging the hairline | **met on fixtures.** Gates 1 to 3: the strand is attenuated and never erased, the busy-background case is refused, and the hair region's edge energy is measured through the renderer against a floor |
| Teeth and eyes look better but unmistakably natural, with catchlights intact | **partly met.** Gates 4 and 5 measure the ceilings, the locus excursion and the catchlight peak. "Looks natural" is a human judgement and is condition C2 |
| Lint and small clothing distractions are cleaned without harming fabric texture | **met on fixtures.** Gate 6 measures recall and asserts that fabric nobody aimed at is unchanged. The 100 % zoom audit is condition C4 |
| Glasses glare is reduced, and any borrowed pixels are disclosed | **met.** Gates 7 and 8, the phase gate end to end, and a database trigger |
| Forbidden identity-changing operations are structurally impossible | **met.** There is nowhere in the contract, the schema or the wire to express one, and three tests scan for one |
| Studios can configure exactly which micro-operations they allow | **met.** Eleven switches, no strengths, and two of them default to off |

## 3. What the section 10.1 gates measured

| Gate | Asked for | Measured |
|---|---|---|
| No bald patches or hairline damage | none on any fixture | hair edge energy 0.9986 of its own baseline, against a floor of 0.94 |
| Reduce rather than remove | a strand stays a strand | the painted strand keeps 87 % of its mean contrast against the background |
| Flyaway area cap | never exceeded | the plan's flyaway area is inside `MAX_FLYAWAY_AREA`; so is the detector's own offer |
| Teeth inside the locus | luminance and chroma stay natural | excursion 0.00000 against a ceiling of 0.003; every ceiling refused when exceeded |
| Catchlights preserved | specular pixel test | peak ratio 1.0000 against a floor of 0.98 |
| No geometry change | not measurable | the iris centroid moves 0.0002 px, and the catchlight peak is unchanged to four decimals |
| Lint recall | >= 0.85 | 1.000 over twenty painted marks, with the control half of the fabric bit-identical |
| Borrowed regions align | within tolerance | 1.000 against a floor of 0.82, on a region inside `MAX_BORROW_AREA` |
| Borrows disclosed | always, in the recipe and Explain | the plan, the operation, the header, the view and the recipe; a trigger refuses the alternative |
| Forbidden operations refused | reshape and swap rejected | eight ceiling attempts refused, plus a schema scan and a contract test |
| Naturalness audit | >= 95 % judged natural | **not run** - condition C2 |
| Per-hair-type coverage | across hair types | **not measured** - condition C3 |
| 100 % zoom artefact audit | no fabric or hairline damage | **not run** - condition C4 |

`the_gates_this_build_cannot_measure_are_named` is a test, so the three unmet rows cannot quietly
become passing ones.

## 4. Performance (section 11)

Measured in release on the development machine, on a 256 px fixture frame:

| Row | Budget | This build |
|---|---|---|
| Micro pass at full resolution (GPU) | <= 250 ms | **waived** - no `wgpu` backend (ADR-0029 section 4) |
| Micro pass at proxy (2048 px) | <= 35 ms | **waived as written** - the row is about applying a plan on a device. The *decision* is 55.1 ms per frame |
| Cross-frame borrow (alignment + blend) | <= 180 ms | 1.6 ms per borrow for the alignment search; the blend runs with the plan |
| 1,000-image gallery at export | <= 5 min | 55.1 s extrapolated, on the processor path |
| Storage (not in section 11) | 2,000 B/image, self-imposed | 1,633 B/image measured over 1,000 photographs |

The measured figure is the **decision** rather than the application, and it includes at least one
full render because the naturalness guard is a post-condition. A frame that re-solves three times
costs four renders. Phase 20's equivalent figure on the same path is 57.6 ms per frame, so five
more operators and a second guard cost nothing measurable - which is what a phase made of
measurements over small regions rather than networks over a whole face should look like.

The borrow figure is the one that would move most on real files. It is an alignment search over a
sibling that differs from the target by the sheet and nothing else, so it converges immediately;
two real frames of a burst differ by motion and noise, and condition C5 is about exactly that.

## 5. What this build's numbers are and are not claims about

They are claims about arithmetic: the background gate, the area caps, the locus distance, the
specular exclusion, the clipped-fraction rule, the alignment search, the per-family withdrawal, the
store's two triggers and the schema's refusals.

They are not claims about a photograph, for the four reasons in section 0. In particular:

* **the fixture hair is a rectangle with a halo and one strand in it.** A real hairline is
  hundreds of strands over a background that changes across the frame;
* **the fixture sheet is a painted rectangle at exactly one brightness.** A real reflection has a
  gradient, a shape and a partial transmission of what is behind it;
* **the fixture teeth are two painted rectangles.** Real teeth have gum, lip and shadow inside the
  same mask;
* **nothing in this repository has been near a camera.**

## 6. Three things the gates found, recorded because they are the useful part

**A chance-corrected agreement margin cannot be met at an extreme marginal rate.**
`ml/models/micro/eval_micro.py` asked the retouchers to agree an absolute 0.10 above what chance
predicts. At the 97 % natural rate this gate exists to certify, chance agreement among three judges
is already 0.92 - eight points of headroom for a ten-point margin - so a *perfect* panel failed.
The statistic is now the share of the available headroom the judges take up, which is Scott's pi:
coin flips score zero at any marginal rate, and a panel that is actually looking scores well above
the floor. Phase 19's halo test had the same shape of defect: **a threshold that a correct
implementation cannot meet is a bug in the threshold**.

**A storage figure written before it was measured was wrong by a factor of two.** The store
documented 612 B per image; over a thousand rows it is 1,633 B. The reason is structural rather
than sloppy, and it is the thing to carry forward: every phase from 09 to 20 stores **one
fixed-width verdict** per photograph, and this stores a **list** whose length is the number of
things that were wrong with the frame. It is the first per-image figure in the product above a
kilobyte, `perf/budgets.toml` now carries the decomposition and the argument, and the alternative -
packing five operators' magnitudes into shared columns - was rejected for the reason ADR-0044
section 5 gives about the wire.

**A refusal check that cannot fail proves nothing.** The phase gate's two trigger checks originally
read "the statement failed" as a pass, and an INSERT refused for a missing foreign key looks exactly
like one refused by the promise. Both now insert a control row first and report `Inconclusive`
rather than success when the attempt never reached the thing under test. This is the general shape
of every negative test in the product and is worth checking the next time one is written.

## 7. What was deliberately not built

**Borrowing for a closed eye, behind a flag.** Section 2.2 excludes it and ADR-0043 section 4
records why it is excluded permanently rather than deferred: a flag is a default waiting to be
changed. The rule that separates it from the glare repair is not about the mechanism - a specular
sheet has destroyed the record, and a closed eye *is* the record.

**A strength on the IPC surface.** A studio switches operations on and off; the ceilings belong to
the contract. ADR-0044 section 4.

**Crease removal by default.** It is opt-in in the contract, off in the schema default, refused by
a trigger, and absent from the lint head's class list so there is no accuracy at which it starts
happening anyway.

**A cloud call.** Section 7 is one sentence and this phase honours it: `aura-retouch` has no
`aura-cloud` dependency and `tests/no_network.rs` fails the build if one appears.

## 8. Conditions

**C1 - the pipeline underneath is placeholder.** Phase 06's detector finds no faces and phase 18's
regions do not reach this pass, so on a real photograph nothing is micro-retouched. **Sev 2.**
Closes with phase 05's C10 and with the `with_regions` wiring; the second is a small change that
touches no frozen shape.

**C2 - the naturalness audit did not happen.** Section 0's headline KPI and section 10.1's last
row: four hundred frames judged natural by retouchers at or above 95 %. There is no such audit and
no such panel. **This phase's own headline number is unmeasured**, the scoring arithmetic ships and
self-tests, and nothing in this build may be described as natural-looking. **Sev 2.**

**C3 - the three heads are untrained, and there are no labels.** `FLYAWAY_HEAD_TRAINED`,
`GLARE_HEAD_TRAINED` and `LINT_HEAD_TRAINED` are false and every plan carries `head_untrained`.
The per-hair-type coverage report has no corpus to run on, and no per-bucket number is published or
should be inferred. Closes when a corpus and a GPU exist.

**C4 - no 100 % zoom artefact audit.** Section 10.1 asks that lint removal leave no fabric-texture
damage at 100 % zoom, and that is a person at a monitor. Gate 6 asserts that fabric nobody aimed at
is unchanged, which is a narrower claim: it says nothing about how the cleaned patch itself reads
at print size. Phase 18's condition C2 is the same gap one phase earlier.

**C5 - the borrow is measured on a synthetic burst.** The sibling frame in the fixture differs from
the target by the sheet and nothing else, so the alignment search scores 1.000. Two real frames of
one burst differ by subject motion, camera motion, noise and a little exposure drift, and the
alignment floor has never been tested against that. This is the condition most likely to change a
number when real files arrive.

**C6 - the panel is not reachable from the running application.** The nine commands are
registered in `ui/src-tauri/src/main.rs` and the shell builds, but `ui/src/ipc/client.ts` has no
wrappers for them and `ui/src/App.tsx` mounts no develop panel at all. That is a repository-wide
gap rather than this phase's: `client.ts` stops at phase 19, and every panel from phase 12 onward
exists with tests and is imported nowhere. `MicroRetouchPanel` is props-driven and fully tested,
so wiring it is a caller rather than a component.

## 9. Rollback

The stage is off with one field: `MicroPassInput::enabled = false`, which produces a plan carrying
`disabled` rather than no plan, so a project can be re-planned later without losing the fact that
it was skipped. `micro_matrix` switches every operator off independently, and switching borrowing
off leaves glare reduction working.

The models roll back through `models.lock` plus `cargo xtask models`; the stored `model_ver` moves
with them, `AURA-ML-5096` is raised, and the affected frames are re-planned in the background.
`ANALYSIS_VER` and `matrix_ver` do the same for the arithmetic and the table.

Migration 21 is additive: three tables, two views, two triggers and no change to any earlier
object. Dropping them leaves every phase up to 20 intact.

## 10. What phase 22 inherits

- **`MicroService` is the only way to ask what was done to somebody's hair, teeth, eyes or
  clothes**, and `RetouchService` is still the only way to ask about their skin. Phase 22 restores
  and sharpens; it must not re-smooth what phase 20 smoothed or re-sharpen what this phase evened,
  and `idx_micro_guarded` plus `v_micro_coverage` are the queries that say what happened.
- **The shared per-image perceptual allowance now has twelve operations spending against it**, and
  the five this phase adds sit below everything phases 19 and 20 do. A thirteenth operation
  inherits the allowance rather than getting its own.
- **A borrow is disclosed in five places and one of them is the recipe**, so an export path that
  rebuilt a recipe without `borrowed_from` would produce an undisclosed composite. Phase 30's
  delivery report reads `v_micro_composites`.
- **The desktop shell builds again**, with 75 commands registered. Phase 20's condition C5 is
  closed, and what is left of that gap is C6: the TypeScript client and `App.tsx`, which have
  lagged the engine since phase 12 and are one task rather than twenty.
