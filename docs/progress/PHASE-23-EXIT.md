# PHASE-23 exit report - Geometry Suite: lens corrections, straightening AI and smart crop

**Phase:** 23 of 30 · **Branch:** `feat/phase-23-geometry-suite` · **Date:** 2026-08-29
**Gate:** `just phase-23-verify` exits 0 · **Eval:** `tests/eval/geometry_eval.rs`, 18 passing

## 0. Read this first: what this phase can and cannot claim

This phase ships the first code in the product that decides **which pixels exist**. Every previous
phase decided what a photograph is, how it should look, or what should be repaired in it; all of
those are arguments about a frame that is still entirely present. A crop is not. Nothing on disk
moves and the recipe is reversible, so the decision is recoverable - but it is invisible in the
only artefact anybody looks at, and a gallery where six frames out of four hundred have a hand cut
off at the wrist does not read as a bug. It reads as the photographer.

What is real:

- the lens resolution chain (embedded, then the bundled database, then estimation from the frame's
  own straight edges), the corrections applied in linear light, and the chromatic aberration
  correction measured to remove a fringe without shifting a colour;
- the rotation band at both ends, and the fact that its **cost is paid before anything else
  happens**: an angle whose implied rectangle would cut somebody is reduced or abandoned, and both
  numbers - wanted and applied - are stored;
- the keystone, its measured stretch cap, and its refusal;
- the safety filter that runs *before* the score, the four-term crop objective, the per-scene
  improvement margin, the aspect variants and their stored refusals;
- migration 23, the store, the two triggers, the nine IPC commands, the Framing panel, and the
  revert that hands a photograph back **and lets automation resume on it**.

What is not:

- **phase 06's detector finds no faces**, so on a real photograph in this build
  `CropSafetyReport::considered` is zero, section 10.1's hard gate is arithmetic rather than
  evidence, and - because an unidentified subject stops the crop search - nothing is auto-cropped
  at all;
- **there are no expert crop labels**, so whether a photographer would prefer AURA's rectangle to
  their own is unmeasured;
- **the 300-crop perceptual audit did not happen**, so the phase's own headline is proven for
  safety and unproven for framing quality;
- **no lens profile in this repository is measured**; all fourteen are reference models for a class
  or a family.

Everything measured below is measured against synthetic frames whose horizon, verticals, fringes,
distortion and protected regions were painted into the pixels and read back through the real
solvers, the real filter, the real store and the real renderer. That proves the arithmetic, the
thresholds, the refusals and the schema. It says nothing about a wedding.

## 1. What shipped

| Area | Files |
|---|---|
| Frozen contract | `crates/aura-core/src/contract/geometry.rs` (30 codes, 5 aspects, 5 protected kinds, `rotation_crop`) |
| Decision engine | `crates/aura-geometry/src/{profiles,lens,straighten,keystone,safety,crop,variants,decide,store,api,errors,fixtures}.rs` |
| Renderer | `crates/aura-render/src/geometry.rs`, `shaders/geometry.wgsl` |
| Schema | `crates/aura-catalog/migrations/0023_geometry.sql` (2 tables, 2 views, 2 triggers, 1 deferred FK) |
| Config | `crates/aura-geometry/config/crop_rules.toml` (23 scenes, 10 with cropping off), `assets/lens_profiles/` (14 profiles + `ATTRIBUTION.md`) |
| Models | **none.** The third phase since 08 to ship no model |
| IPC | `crates/aura-app/src/geometry_commands.rs`, 9 commands, ADR-0048 |
| UI | `ui/src/components/develop/GeometryPanel.tsx` (+ 10 tests), `ui/src/ipc/client.ts` `geometry` block |
| Errors | `AURA-ML-5109` to `AURA-ML-5114`, one runbook each |
| Budgets | `stage.geometry_plan_frame`, `size.geometry_store_per_1000_images`, `crates/aura-perf/tests/geometry_budgets.rs` |
| Docs | `docs/geometry.md`, ADR-0047, ADR-0048 |
| Gate | `crates/aura-cli/src/phase23.rs`, `just phase-23-verify` |

## 2. Acceptance criteria (section 13)

| Criterion | Status |
|---|---|
| Lens distortion, vignetting and fringing corrected where profiles exist | **Yes, through reference profiles.** The chain resolves embedded data first, then the database, then estimation; the corrections are applied in linear light and the CA fixture loses its fringe without a colour shift. No profile is measured. C3. |
| Tilted horizons levelled; creative tilts preserved | **Yes.** The band is 0.2° to 8° above 0.70 confidence; below it the frame is already level, above it the tilt is a decision and is left alone rather than clamped. Both ends are asserted. |
| Smart crop improves framing only when it clearly helps, and never cuts faces or hands | **The refusal half, entirely. The improvement half, structurally.** Zero of 20 painted protected regions were cut, and a candidate that would cut one is never scored. Whether the crops that pass improve framing is unmeasured (C2), and hands are never in the protected set (C4). |
| Social and album aspect variants available without duplicating files | **Yes.** Four aspects generated per frame, safe ones stored beside the delivered rectangle, refused ones stored with their reason. No file is duplicated. |
| Original framing is always one click away | **Yes.** `revert_geometry` is its own command and its own button, rendered on every plan; the gate asserts that reverting restores `Box2::FULL`, clears the rotation and the keystone, and clears `user_edited` so automation resumes. |
| Geometry applied once, at high quality, inside the render graph | **Yes, structurally.** One stage at index 21, one resample; ADR-0047 section 2 records why the lens half is not split back to index 6. The GPU path exists and no device runs it. C5. |

## 3. What the section 10.1 gates measured

| Row | Result |
|---|---|
| Straightening within 0.3° of expert on >= 90 % of labelled frames | **Pass against painted angles.** Measured against the tilt the fixture painted rather than against a person's label; there are no labelled frames. |
| Intentional tilts untouched | **Pass.** A 20° frame is left at 20°, and a frame below 0.70 confidence is left alone with `geometry_horizon_unsure`. |
| Zero auto-crops cut a detected face or a primary identity's hands | **Pass on fixtures, and the number is arithmetic on a real wedding.** 0 of 20 protected regions cut. `faces_checked` is the denominator and it is zero on any real project in this build. C1, C4. |
| Resolution floor respected on every crop | **Pass**, and the floor is on the **long edge** rather than on the area - a distinction with its own test, because 60 % of the area is 77 % of the long edge and the two differ by a third of a frame. |
| CA correction removes fringing without introducing colour shifts | **Pass.** Both halves asserted: the fringe energy drops and the frame's mean chromaticity does not move. |
| Keystone never exceeds the stretch cap, skipped when it would violate crop safety | **Pass.** No correction exceeded 1.12 at any convergence or frame shape. |
| Most frames (>= 70 %) keep their original framing | **Pass.** 20 of 24 fixture frames, 0.83. On a real wedding in this build it would be 1.00, because the crop search does not run without an identified subject. |
| Revert-to-original restores exact framing | **Pass**, end to end through the store. |
| Agreement with an expert crop | **Not measured.** No labels. C2. |
| The 300-crop perceptual audit | **Not run.** C2. |

The last two are named by `the_two_rows_this_harness_cannot_measure`, which prints them on every
run - phase 05's rule that a suite silent about its gaps reads as a suite that measured everything.

## 4. Performance (section 11)

| Row | Status |
|---|---|
| Geometry decisions per image <= 40 ms | **Measured** as `stage.geometry_plan_frame` on the processor path |
| Resampling overhead at export (45 MP) <= 120 ms | **Waived**, no `wgpu` backend (ADR-0029 section 4). The reference resampler exists and `shader_parity.rs` holds the shader to it, so what is missing is the device rather than the operator. C5. |
| 1,000 selected images <= 45 s decisions | **Extrapolated** from the per-image figure |

Storage is `size.geometry_store_per_1000_images`, budgeted at 1,400 B/image against a measured
**1,088 B**: 790 B for `geometry_plan` and its four indexes, 298 B for up to five `geometry_crop`
rows and their index. It is the second figure in the product above a kilobyte, after phase 21's
1,633 B, and the cause is the same shape - a **list** beside a fixed-width verdict. The
consequence is not: `MAX_VARIANTS` bounds this list at five in the contract and again in a CHECK,
so the widest frame in a wedding stores five rectangles rather than fifty.

## 5. What this build's numbers are and are not claims about

Every number in section 3 is measured on frames from `crates/aura-geometry/src/fixtures.rs`,
whose horizons, verticals, fringes and protected rectangles are painted into the pixels by
construction and read back through the real code.

Two of them are worth being precise about.

**The conservatism figure of 0.83 is a fixture property.** The fixture wedding carries painted
protected regions, so its frames have identified subjects and the crop search runs. A real project
on this build has no faces, so no subject, so no search - and its conservatism is 1.00 for a
reason that has nothing to do with the improvement margin working.

**The safety figure of zero is the same zero either way.** Zero regions cut over 20 checked is
evidence. Zero regions cut over zero checked is arithmetic. The wire carries both numbers, the
panel says which one it has in words rather than printing the reassuring one, and
`v_geometry_safety` puts `faces_checked` beside `faces_cut` for the same reason.

## 6. Two things the gates found, recorded because they are the useful part

**An ordering constraint and a referential constraint can contradict each other, and the database
takes the referential one.** `geometry_crop` had an immediate foreign key to `geometry_plan`, and
the store writes variants first on purpose: `geometry_primary_is_safe_insert` reads the variants to
decide whether an incoming plan's delivered index is safe, and a plan written first would be
checked against **the previous version of that photograph's rectangles** - a check that passes for
the wrong reason. The first gate run wrote zero plans and printed `FOREIGN KEY constraint failed`
twenty-four times, with the store's own comment already describing the ordering the schema forbade.
Three fixes were available and the constraint is now `DEFERRABLE INITIALLY DEFERRED`, the only
deferred foreign key in the product, checked at COMMIT by which point both rows exist. ADR-0047
section 9.

**A refusal must be raised with the code whose runbook matches it.** A crop rectangle outside the
frame raised `AURA-ML-5112`, which is run-blocking and whose user message says AURA cannot read its
own settings and will not straighten or crop anything until they are fixed. A photographer who
dragged a crop handle past the edge of their photograph has not broken their installation. It is
`AURA-ML-5111`, an item failure that asks them to try again - and the contract's own doc comment
named the wrong code as well, so the mistake was written down twice before it was written in code
once.

## 7. What was deliberately not built

**The crop search does not run without an identified subject.** Three of the objective's four
terms would still mean something, so searching anyway produces a rectangle rather than nonsense -
one optimised toward whatever is brightest, compared against the improvement margin as though the
comparison meant something, and delivered as a considered decision. Phase 19's rule and phase 22's
are the same rule from two sides, and this is the third design they have decided. The aspect
variants are still generated, because a variant is an option phase 29 may take rather than a
decision about the delivery. ADR-0047 section 5.

**Lens correction is not split back to the sensor domain.** It optically belongs beside denoise at
index 6, and moving it there would mean two resampling passes over the frame - which is section
12's fourth failure mode, whose mitigation is applying geometry once. ADR-0047 section 2.

**Nothing scales, fills, upscales or stitches.** Section 2.2 puts fill in phase 24 and panoramas
out of scope. There is no column, no field and no function; `crates/aura-geometry/tests/
boundaries.rs` is the sixth grep-as-a-test in the repository and fails the build if the words
appear.

**No sharpening control.** Phase 22's handoff: geometry resamples after `Stage::Sharpen`, and
sharpening again after a resample is the halo generator phase 22 spent four preconditions
avoiding.

## 8. Conditions

**C1 - Sev 2. The safety filter has nothing to protect, and nothing is auto-cropped.** Phase 06's
detector is a placeholder, so `projected` is empty on every real photograph: `considered` is zero,
section 10.1's hard gate is arithmetic, and the crop search does not run at all. The mechanism is
complete and is measured against painted regions. **Closes with phase 05's C10** rather than
separately, and `docs/geometry.md` says so in the product's own words.

**C2 - Sev 2. No expert crop labels and no perceptual audit.** Section 9's DATA row asks for expert
crops on 2,000 frames and QAIQ for 300 auto-crops; neither exists. So the improvement margin, the
placement targets and the headroom targets are authored numbers that have never been compared with
a photographer's judgement, and the phase's own KPI - framing that a photographer prefers - is
unmeasured. **No claim about crop quality may be made from this build.**

**C3 - Sev 3. No lens profile is measured.** All fourteen rows in `assets/lens_profiles/` are
reference models for a class or family; `ATTRIBUTION.md` says so, `geometry_plan.lens_measured` is
0 on every row, and the panel says so on any corrected photograph. The correction ships anyway,
unlike phase 22's face recovery, because its failure mode is a residual distortion of a fraction of
a percent rather than confident invention. ADR-0047 section 7.

**C4 - Sev 3. Two of the five protected kinds are never filled.** Phase 11's keypoint head is a
placeholder, so `Hands` and `JoinedHands` never appear; phase 08 records a key *frame* rather than
a key region, so `MomentKey` never appears either. The mitigation is structural rather than
promised: the ten scenes where hands matter most - ring exchanges, garland ceremonies, hand
details - have automatic cropping switched off entirely in `crop_rules.toml`, with the reason on
the row.

**C5 - Sev 3. Section 11's resampling row is waived**, because this build links no `wgpu` backend.
Closes with ADR-0029's own condition. Unlike phases 19 to 22 there is a second half worth stating:
the reference resampler is complete and `shader_parity.rs` holds `geometry.wgsl` to it, so the
first machine with a backend gets a measured row rather than an unwritten one.

**C6 - Sev 3. `ui/src/ipc/client.ts` covers phases 19 and 23 and not 20 to 22.** This phase adds a
typed `geometry` block; the retouch, micro-retouch and restoration surfaces are still reachable
only through `invoke`. This is the remaining half of phase 22's C5 - the other half, that phase
22's seven commands were never registered in the Tauri shell at all, is closed here.

**C7 - Sev 3. The desktop shell's Rust cannot be compiled in this environment.** `cargo check` on
`ui/src-tauri` fails in `parking_lot_core` with "error calling dlltool 'dlltool.exe': program not
found", and the copy in the toolchain's `self-contained` directory fails in turn because it has no
assembler to call. This is the same missing-MinGW limitation CLAUDE.md records for release `xtask`
builds, and it is environmental rather than a defect in this phase's code. What was verified
instead is mechanical and complete: every one of the 92 `aura_app::` calls in `main.rs` resolves to
an exported function, every one of the 114 imported IPC types is defined in `contract/ipc.rs`, and
every one of the 92 registered handler names is declared in the file. The Rust the shell calls -
`aura-app`, including all nine geometry commands - compiles, clippy-passes and is tested.

**C8 - Sev 3. CI has no lane for phases 21 and 22.** `.github/workflows/ci.yml` gained a phase 23
gate, its eval harness and its budgets in this phase; the micro-retouch and restoration gates are
runnable through `just` and are not run by CI, which has stopped at phase 20 for two phases. Not
this phase's work to fix, and recorded here because a gate nobody runs stops being a gate.

## 9. Rollback

Migration 23 creates two tables, two views and two triggers and nothing outside the file references
them; the `DROP` sequence is in the migration's own header. It is recomputable with one exception:
`geometry_plan` rows with `user_edited = 1` are not derivable from anything, and the rollback
runbook says to export those first. It matters more here than in any previous phase, because a
hand-set crop is a decision about the delivery rather than an opinion about a photograph.

The feature switch is `GeometryPassInput::enabled`. A disabled pass still writes a plan per frame -
one that does nothing - because a frame with no plan and a frame the studio switched off look
identical in a coverage report.

## 10. What phase 24 inherits

- **`GeometryService` is the only way to ask which pixels are delivered.** Nineteenth service of its
  kind. Phase 24 removes objects inside a rectangle this phase chose, phase 27 has to be able to say
  why a frame is tilted, phase 29 picks between these variants and phase 30 exports one. No phase
  may keep its own crop.
- **A safety constraint is a filter, never a term in a score.** Phase 24 is the phase most likely to
  need this rule and least likely to find it convenient: a generative fill has a quality objective
  too, and "how good does the fill look" must never be tradeable against "may this region be
  filled at all".
- **A cost that another decision will pay is computed before that decision.** Phase 24 fills a
  region and then the frame is delivered at a rectangle; a fill outside the delivered rectangle is
  work nobody sees, and a fill that assumes the whole frame is a fill that can be cropped through.
- **A photographer may be stricter than the product; nobody may be laxer.** The config file may only
  tighten. There is no field on the surface that widens a safety rule, and phase 24's ceilings
  should be shaped the same way.
- **The identity constraint is phase 22's and applies here too.** A generative fill inside a face is
  a face-recovery decision by another name, and phase 22's exit report already said so.
