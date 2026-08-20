# PHASE-16 exit report - Tone AI, Adaptive Curves, HSL AI & Skin-Tone Protection

**Branch:** `feat/phase-16-tone-curves-colour-ai` · **Gate:** `aura-cli verify --phase 16`
exits 0 · **Status:** implemented **conditionally**, on the six conditions in section 8.

## 1. What shipped

One frozen contract, one module of thirteen files, one migration, one IPC surface, three
panels, two ADRs, one signed model, one product document and a gate.

`aura-core::contract::colour` freezes the shape. `ColourDecision` is section 5's struct with
seven additions (ADR-0033), plus `ToneCurve`, `HslAdjustments`, `HslBand`, `SkinGuardReport`,
`ColourVariant`, `ContentBand`, `BandReading`, 29 reason codes, `ColourOutline`,
`ColourOverride` and `ColourService`. **There is no field anywhere in it for an ideal skin
colour**, which is the same central design decision phase 15 made and the same structural
defence.

`aura-brain-photo::colour` decides. `intent.rs` loads 22 argued-over scene rows and refuses a
broken file. `tone.rs` solves the five parameters from the histogram, the subject's own spread
and phase 09's noise headroom. `curve.rs` fits a monotone curve under section 6.1's three
constraints. `content.rs` reads what is in the frame. `harmony.rs` decides what should change
about its colour. `hsl.rs` says what the recipe's eight bands should hold. `clip_guard.rs`
bounds the grade and `skin_guard.rs` measures what it did to the people in it. `analyse.rs`
composes them, `store.rs` owns migration 16, `codec.rs` owns the six documents it stores,
`api.rs` is the frozen service and the resumable walk, `fixtures.rs` is the synthetic ground
truth.

Migration 16 adds `image_colour_decision` and `v_colour_coverage`. **There is no skin-target
column, no skin-tone column and nowhere to put one**, and the gate scans both the schema and
the config file for one on every run.

The IPC surface is seven commands (ADR-0034); the Tone panel reports the guarantee as a
measurement, the curve editor draws AURA's curve over the identity, and the HSL panel shows
the protected-skin indicator and what the content pass actually read.

## 2. Acceptance criteria (section 13)

| Criterion | Status |
|---|---|
| Selected frames receive scene-appropriate contrast, curves and HSL automatically | **met** - every frame, up to six typed reasons, 22 scene intents |
| Skin never shifts measurably, on any skin tone, after grading | **measured on synthetic reflectances** - worst 0.06 deg of 2.0, worst 2.4 % of 6 %, on all five. **Not measured on photographs of people** - C2 |
| Generated curves are always monotonic and never posterise | **met** - 4,096-step ramp through the renderer's own interpolation, on every fixture; longest flat run 0 |
| No frame gains new clipping beyond its scene tolerance | **met** - 22 of 22, against each scene's own tolerance |
| The curve editor shows the AI curve and allows instant switching to alternatives | **met** - drawn with the renderer's interpolation, three complete guarded alternatives |
| Expert audit confirms results look professionally subtle rather than filtered | **not done** - there is no expert here. The subtlety metric is the enforceable half and it holds on every fixture - C3 |

## 3. What the section 10.1 gates measured

`cargo test -p aura-brain-photo --test colour_eval` - 27 gates, all green.

| Gate | Threshold | Measured |
|---|---|---|
| Subject contrast within tolerance of the scene intent | >= 85 % | **met** (20/20 frames with faces) |
| Every generated curve monotone, no posterisation | exact, every frame | **met**, longest flat run 0 of 4,096 |
| Skin hue shift, every skin tone | <= 2.0 deg | **0.06 deg** |
| Skin chroma change, every skin tone | <= 6 % | **2.4 %** |
| Skin guard behaves the same across the five tones | spread <= 1.0 deg | **met** |
| No new clipping beyond the scene tolerance | >= 99.5 % | **100 %** (22/22) |
| Shadow lift within phase 09's noise budget | exact, five headroom values | **met** |
| Greenery found where painted and pulled toward the target | every greenery frame | **met** - found at 44 %, pulled +12 deg and -12 saturation |
| Every grade inside its scene's subtlety cap | exact, every frame | **met** |
| Determinism | byte-identical | **met** |

Six of the twenty-seven gates exist to prove the harness can fail: a do-nothing tone solver, a
curve that goes backwards, a grade that turns one skin-tone bucket orange, a frame past the
hue ceiling, a grade that clips, and a filtered-looking grade are each asserted to be
*rejected*. `ml/models/colour/eval_colour.py --self-test` makes the same eight assertions on
the Python side, and a Rust test asserts the two agree about every threshold.

**Every one of these numbers is about synthetic frames.** The foliage hue, the dress
luminance, the subject contrast and the distractor saturation were chosen, painted into the
pixels and read back through the real pipeline. That proves the arithmetic. It is not evidence
about a photograph.

## 4. Benchmarks

| Row | Section 11 | This build |
|---|---|---|
| Colour decisions per image | <= 20 ms | **waived** - no GPU backend (ADR-0007); the debug processor path measures ~180 ms per 384x256 fixture, dominated by the content pass and the guards' histogram replays |
| 4,000 images | <= 80 s | **waived** - extrapolated from a debug build on fixtures rather than proxies |
| Alternatives generation overhead | <= 15 % | **not met - about 3x.** See section 8, C4 |
| Extra storage per image | (no section 11 row) | **1,581 B against this phase's own 1,600 B budget** |

## 5. Telemetry (section 11)

`colour.decided` (images, ms, mean_contrast, mean_shadow_lift, mean_subtlety,
skin_measured_ratio, worst_skin_hue_shift), `colour.skin_guard_triggered` (count, withdrew,
mean_attenuation) and `colour.clip_guard_resolve` (count, param_histogram) are emitted by
`ColourPass::emit`. `colour.untargeted` is a fourth, for a scene with no intent row.

## 6. Invariants

1. **Never mutate a RAW.** No path column, no file operation anywhere in this phase.
2. **Confidence and reasons.** One confidence, up to six reasons, and migration 16's
   `reason_count` CHECK refuses a row with none.
3. **Three-tier compute.** Tier 2, the 2048 px proxy - a cache hit, because phases 06, 09, 11
   and 15 already read it.
4. **Determinism.** Asserted by the harness and by the gate on the assembled path.
5. **Resumability.** `ColourStore::pending` is keyed on the three version columns.
6. **Local-first.** No cloud call in this phase, as section 7 requires.
7. **Scene-conditioned everything.** 22 scene rows; a scene with no row is recorded and
   reported.
8. **Colour discipline.** The content classification runs in linear light; the grade is
   measured through `aura_render::tonemap` rather than through a copy of it.
9. **No silent failure.** Six codes, `AURA-ML-5066` to `5071`, each with a runbook.

## 7. Rollback

Migration 16 is reversible and recomputable: one `DROP VIEW`, one `DROP TABLE` and one
`DELETE` return the catalog to schema 15, and every row is derived from pixels, phase 06's
faces, phase 07's scenes and phase 09's calibration. The one exception is the usual one -
`user_values` is not derivable from anything, and the runbook says to export it first. It also
lives in `edit_recipes` and in the sidecars, which is the second copy that makes the loss
survivable.

Feature flag: the pass is only reached through `estimate_colour`. The model is pinned by
digest and rolls back on a failed first use - and because `TONE_HEAD_TRAINED` is false,
rolling it back changes nothing a photographer can see.

## 8. Conditions carried out of this phase

**C1 - The tone head is untrained, and no number here is a claim about a photograph.** `Sev 2.`
Section 8 steps 1 and 2 ask for tone and HSL parameters extracted from an expert-edit dataset;
there is no such dataset here, there are no camera files and there are no expert edits.
`TONE_HEAD_TRAINED` is false, so the head is **never consulted** and no frame in this build is
graded by a random projection. Everything measured above is the *solver*. This closes with
phase 05's C10 and phase 02's camera files rather than separately. **No later phase may claim
a tone or colour result that depends on these weights until it closes.**

**C2 - The fairness gate is five reflectances, not five people.** `Sev 2.` The guarantee in
section 6.3 is measured across five skin reflectances spanning light to dark, and it holds on
every one of them with a spread well inside the limit. They are five points on a line through
the region human skin occupies. Until this has been measured on photographs of real people
with their consent, the honest statement is that **the mechanism is per-frame and
self-referential, and that says nothing yet about a photograph of a real person**.
`docs/skin-fairness.md` says so in the product's own words. Nothing in the product stores a
skin-tone bucket; the five live only in `tests/eval` and in `eval_colour.py`.

**C3 - Section 13's expert audit did not happen.** Section 9 gives QAIQ "expert review of 400
graded frames: subtlety, skin, greenery, dress texture", and section 10.2 asks for a blind A/B
against the named competitor at >= 60 % preference. Neither exists here: there are no experts
and no real frames. The enforceable half - the subtlety metric, capped per scene and asserted
on every fixture - is what ships in its place, and it is a bound rather than a judgement.

**C4 - The alternatives cost about 3x, against a 15 % budget.** Section 11 allows 15 % for
generating alternatives; three complete variants each go through the clipping guard and the
skin guard, which is three more histogram replays and three more skin measurements per frame.
ADR-0033 decision 6 is why they are complete rather than deltas - a delta would be cheap and
would be a parameter set nobody guarded - so the fix is caching rather than a change of shape:
the tone half of a variant does not change, so its clipping measurement could be shared. Not
done here because section 11's other rows are waived anyway and optimising against an
unmeasured budget is how a phase acquires a benchmark it cannot reproduce.

**C5 - The content bands are inferred, not segmented.** Section 6.2 asks for "segmentation
cues"; there is no segmentation model in this build and phase 18 is where masks are made. Every
adjusted frame carries `ColourCode::ContentInferred`, every band carries a confidence, a band
below the floor is not adjusted, and the panel says so. ADR-0033 decision 4 records the
argument for shipping this rather than waiting. **It closes with phase 18**, and the interface
does not move when it does.

**C6 - Section 11's per-image budget is unmeasured on a reference machine.** The same waiver
phases 14 and 15 carry, for the same reason: no GPU backend is linked (ADR-0007) and no
reference machine has run this build. The processor path is not the path the budget describes.

## 9. What phase 17 inherits

- **`ColourService` is the only way to ask how a photograph should be graded.** Phase 17
  shifts these values toward one photographer's own style; it does not re-derive them, and it
  does not keep a second tone solver.
- **The nine recipe paths this phase owns are the nine it may change.** Phase 17's job is to
  move the same nine numbers, and the boundary against phase 15's three is checked by a test
  rather than remembered.
- **The skin guard runs last, and it runs after phase 17's shift too.** A personal style that
  moved somebody's skin would be a personal style the guard withdraws, and that ordering is
  not negotiable by any later phase.
- **The intent table is what a photographer's style replaces.** `tone_intent.toml` is the
  consensus; phase 17's profiles are the deviation from it, which is why the file records a
  written reason per row rather than a tuned number.
