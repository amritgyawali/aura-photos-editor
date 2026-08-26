# PHASE-23 exit report - Geometry Suite

**Branch:** `claude/phase-23-continuation-0z917n` · **Gate:** `aura-cli verify --phase 23`
exits 0 · **Status:** implemented **conditionally**, on the four conditions in section 8.

## 0. Read this first: this phase was written out of order, and it removes things

Two facts shape everything below.

**It was written on top of phase 19**, with phases 20, 21 and 22 not yet built. Section 0's
"Depends on" names phases 06, 11 and 14, and all three exist - so unlike phase 19, this phase
consumes nothing that has not shipped. What it does not have is a *pose estimate*, which no
phase in the plan owns explicitly and which section 6.3's hands rule needs. That is condition
**C3**.

**It is the first phase in the product that removes something from a photograph.** Twenty-two
phases have decided what is delivered, what it is of, whether it worked, how it should look and
how light moves inside it. Not one of them has taken anything away. The asymmetry runs through
every decision in this report: a wrong exposure looks wrong on the screen it was decided on, and
a frame with somebody's hand missing from the edge looks like a frame until it is printed.

**A reviewer should read section 8's conditions before reading any number in section 3.**

## 1. What shipped

One frozen contract, one new crate of fourteen modules, one migration, one shader, one IPC
surface, one panel, two ADRs, six runbooks, one Python script, one product document, one
amended frozen contract and a gate.

`aura-core::contract::geometry` freezes the shape. `GeometryPlan` is section 5's struct plus
the scene, three version columns and the override flag; `CropVariant`, `CropPurpose`, `Aspect`,
`Keystone`, `CropSafetyReport`, `GeometryOverride`, twenty-four reason codes and
`GeometryService`. `ProtectedRegion` is the input port phases 06 and 11 fill. **There is no
field anywhere in it that could hold image data**, which is what makes "all geometry is
recorded in the recipe and fully reversible" a property of the shape.

`aura-geometry` decides. `profiles.rs` loads the bundled lens table and refuses a row with no
attribution or a duplicate lens id; `rules.rs` loads `crop_rules.toml` and may only tighten;
`lens.rs` picks one of section 6.1's three routes, withholds fringing on an estimate, tracks
edge chains out of a proxy and fits `k1` by a filtered search; `straighten.rs` gates on
confidence and band and then *solves* the rotation against the crop it implies; `keystone.rs`
fits a vanishing point from at least three verticals and refuses past the stretch cap;
`safety.rs` is the hard filter; `crop.rs` is the objective and the bounded lattice;
`variants.rs` is the aspect crops; `plan.rs` runs section 8's seven steps in section 8's order;
`store.rs` owns migration 20; `api.rs` is the frozen service and the resumable pass.

`aura-raw::colour::lens` is new and holds the optics transform - one implementation, reachable
by the decision and by the renderer. `aura-render::geometry` applies it, and
`shaders/geometry.wgsl` is the GPU half.

Migration 20 stores the plan and its crops. `aura_recipe::Lens` gains `coefficients`
(ADR-0041 section 4). Six IPC commands (ADR-0042) feed a Geometry panel.

## 2. Acceptance criteria (section 13)

| Criterion | Status | Evidence |
|---|---|---|
| Lens distortion, vignetting and fringing corrected where profiles exist | **met, conditionally** | `gate_4c`, `gate_4d`. Every bundled profile is fabricated - **C2**. |
| Tilted horizons levelled; creative tilts preserved | **met** | `gate_1`, `gate_1b`, `gate_1c`. |
| Smart crop improves framing only when it clearly helps, and never cuts faces or hands | **met for faces, unmeasurable for hands** | `gate_2`, `gate_6`, `gate_6b`. Hands: **C3**. |
| Social and album aspect variants available without duplicating files | **met** | `variants.rs`, migration 20's `geometry_crop`. Sixty-four bytes a rectangle. |
| Original framing always one click away | **met** | `gate_7`, `gate_7b`. Derived rather than stored, so it cannot be lost. |
| Geometry applied once, at high quality, inside the render graph | **met** | `graph::ORDER`, `cpu.rs`. One resample for lens, keystone, rotation and crop. |

## 3. The gates (section 10.1)

Measured by `tests/eval/geometry_eval.rs` (23 gates) and `aura-cli verify --phase 23`.

| Gate | Threshold | Measured | Pass |
|---|---|---|---|
| Straightening within 0.3 deg of expert | >= 90 % | 100 % of labelled frames | yes |
| Intentional tilts untouched | all | all | yes |
| Auto-crops cutting a detected face | 0 | 0, over every fixture in every one of 23 scenes | yes |
| Auto-crops cutting primary hands | 0 | 0 - **over an empty set**, see C3 | qualified |
| Crops below the resolution floor | 0 | 0 | yes |
| CA removed without a colour shift | - | withheld on an estimate; applied from a profile | qualified, C2 |
| Keystone within the stretch cap | always | 60 convergences, both orientations | yes |
| Keystone skipped when it violates crop safety | always | `gate_5c`, every scene's floor | yes |
| Frames keeping their original framing | >= 70 % | 80 % on the fixture wedding | yes |
| Revert restores exact framing | exact | `gate_7`, and the gate's store round trip | yes |

**Every number above is measured against synthetic frames.** See C1.

## 4. Performance (section 11)

| Row | Budget | This build |
|---|---|---|
| Geometry decisions per image | <= 40 ms | measured on the processor path, inside budget |
| Resampling overhead at export (45 MP) | <= 120 ms | **waived** - no GPU backend (ADR-0029 section 4) |
| 1,000 selected images | <= 45 s decisions | extrapolated from the per-image figure |
| Storage per image | none named | **839 B measured**, against an 890 B budget |

The waived row has a number beside it rather than only a sentence: the reference path's own
distortion-plus-fringing resample is measured at 1,024 px squared and the 45 MP extrapolation
is printed, which is what a future GPU row will be compared against.

The manual-lens estimator has **its own budget row** rather than being folded into the
per-frame one. It runs only on a lens with no profile at all; folding it in would make the
budget describe a case most weddings never hit, while hiding a change to `MIN_CHAIN_SPAN` that
tripled it.

## 5. Storage, and how it got there

It started at **1,474 bytes per image** against an 890 byte budget. Three decisions took it to
839, and two of them are rules this repository already had:

1. **A reason stores its code, not its sentence** - phase 09's rule, for the sixth migration
   running - with one explicit exception: four of the twenty-four reasons carry a measured
   number inside the sentence, and only those store their text. **1,474 to 999.**
2. **Ordinal zero is not stored at all.** The frame as shot is a pure function of `rotate_deg`
   and the frame's aspect. Deriving it is *stronger* than storing it: a stored row is a row
   somebody can delete, and a derived one cannot be lost, cannot drift from the rotation it
   belongs to, and cannot be edited into something that is not the frame as shot. **999 to
   839.**
3. **Refusals are four counters, not rows.** Two hundred refused rectangles a frame is 800,000
   rows on a 4,000-image wedding for information nobody queries across.

A 4,000-image wedding costs about 3.2 MB, against 4.1 MB for phase 19's plans and 48 MB for
phase 14's recipes.

## 6. What this phase got wrong first

All six were found by this phase's own gates. Three generalise beyond it.

**A crossing is not an ending.** The edge tracker died at every intersection, because the
gradient *along* one edge collapses for two or three pixels where another crosses it. An
eleven-by-eleven grid produced chains of twenty-three pixels and the span floor rejected every
one: zero chains, on a plate made of nothing but straight lines, with every unit test passing.

**A robust fit must reject the chains no coefficient can straighten, not the chains with the
largest residual.** Trimming the worst third by residual scored 0.000 against a painted 0.020 -
because on a genuinely distorted frame the largest residuals belong to the chains nearest the
*edge*, which are the only ones that see any distortion at all. Trimming by residual keeps the
optical centre and throws away the evidence.

**Re-acquiring after a gap needs a window as wide as the gap.** A tracker that only ever looks
one pixel either side re-acquires at the wrong place after a three-row crossing, holding the
chain flat at every intersection and quietly straightening the very curvature the estimator
exists to measure. It biased the recovered coefficient low by about a sixth - and **every chain
agreed with every other chain about the wrong answer**, which is what makes this class of bug
survive review.

> The general form of those three: **a measurement pipeline can be wrong in a way that is
> self-consistent.** Unit tests over synthetic inputs to the *fitter* will not find it, because
> the fitter is correct. Only a gate that runs the whole pipeline against a known answer will.

Three more, smaller:

**The max-area inscribed rectangle changed the frame's shape with the angle.** Levelling a 3:2
frame by two degrees delivered 1.72:1 and by four degrees something else again. A photographer
who asked for a straighten did not ask for a reframe.

**The straddle penalty was an order of magnitude too small**, so the objective preferred slicing
a bright window in half to leaving it whole - the single most visible mistake an automatic crop
can make.

**The forbidden-column scan matched substrings**, so `lens_profile` and `profile_ver` read as
stored paths and the gate cried wolf on the two columns this phase most needs to keep.

## 7. What this phase deliberately did not build

**No content-aware fill.** A keystone opens two corners and a rotation opens four; they are
cropped away. Section 2.2 puts filling in phase 24, and there is no parameter anywhere on the
IPC surface for it - so the boundary is structural rather than remembered.

**No album layout.** Which crop an album page uses is phase 29's decision.
`GeometryService::variant` is how it will ask.

**No panorama, no perspective composite.** Section 2.2.

**No face detector and no pose model.** `ProtectedRegion` is the input port. A second answer to
"where is her face" is a crop that cuts one this product elsewhere insists it can see.
`crates/aura-geometry/tests/no_render_calls.rs` greps for one.

## 8. Conditions

Four, and the first two are Sev 2 triggers.

### C1 - Sev 2. There are no wedding photographs and no expert crop labels here

Section 9 gives DATA "expert crop labels on 2k frames; architecture and tilt sets" and there are
none. Every gate in section 3 measures a geometry that was **chosen, painted into the pixels and
read back through the real pipeline**. That proves the estimator, the tracker, the caps, the
safety filter, the search and the store. It is **not** evidence that a photographer would agree
with a crop, and section 10.1's QAIQ audit of 300 auto-crops has not happened.

**No later phase may claim a crop quality result until this closes.** `ml/eval/crop_agreement.py`
is the harness the labels will run through when they exist.

### C2 - Sev 2. Every bundled lens profile is fabricated

No lens was measured. The coefficients in `assets/lens_profiles/synthetic.toml` have the right
sign and order of magnitude for their focal length and are not measurements. Every row sets
`synthetic = true`, which reaches `ProfileTable::is_synthetic`, the IPC surface and the panel -
so a photographer is never told a lens was profiled when it was invented.

Phase 14 said the same thing about camera profiles and the shape of the honesty is identical:
this is a determinism and regression gate, not a claim about optics. **The first measured lens
profile reopens this phase's acceptance criteria whatever phase is in flight**, exactly as the
first real camera file reopens phase 02's.

### C3 - There is no pose estimate, so no crop has ever been checked against a pair of hands

Section 6.3's hard constraints name "primary identities' hands and joined hands inside". The
mechanism is built, tested and enforced - `gate_2b` proves a primary pair blocks a crop and a
guest's does not - and **the set it runs over is empty on every photograph in the product**.
`CropSafetyReport::hands_checked` is zero everywhere, the panel says so in a sentence, and
section 3's hands row is marked qualified rather than passed.

Closing it needs a pose model in `aura-vision` and one line in
`aura_app::geometry_commands::build_input`. No shape frozen in this phase changes.

### C4 - The improvement margin and the objective's weights are authored, not fitted

Section 9 gives MLL "define the crop objective and improvement margin; evaluate against expert
crops". The objective is defined and its shape is argued (ADR-0041 section 8, and the geometric
mean is the house shape since phase 09). Its four weights and the 0.05 margin are **chosen
numbers**, not fitted ones, because fitting them needs C1's labels.

`crop_rules.toml` is where a product manager raises the margin per scene, every row carries a
written reason, and fourteen of the twenty-three scenes do not permit a crop at all - so the
seventy-per-cent conservatism target is set mostly by which scenes may crop rather than by the
weights. That is the right place for it to be set, and it is why C4 is not a Sev 2.

## 9. Rollback

- **Feature flag off:** `Capabilities::geometry_models = false` returns the renderer to phase
  14's behaviour - the three lens stages and the perspective correction report
  `SkipReason::GeometryAbsent` and `LensProfileAbsent`, and crop and rotate keep working as
  they did.
- **Migration reversible:** `DROP VIEW v_geometry_coverage; DROP TABLE geometry_crop; DROP TABLE
  geometry_plan;` and set `user_version = 19`. Nothing else references them, and a catalog
  rolled back renders every photograph exactly as before - the frame as shot is what
  `recipe.geometry` defaults to.
- **Profile table pinnable:** `assets/lens_profiles/` is data. Removing it makes every lens
  unknown, which is `AURA-ML-5095` per lens and no correction.
- **Rules pinnable:** `crop_rules.toml` with every `crop = false` is a build that levels and
  corrects and never crops.

## 10. What phase 24 inherits

- **`GeometryService` is the only way to ask how a photograph's frame was finished.** Sixteenth
  service of its kind.
- **The corners this phase opens are phase 24's to fill, and phase 24 must not widen the crop
  to hide them.** The rectangle here is the one the safety filter passed.
- **A crop that cannot be proven safe is not a candidate.** Any later phase that finds itself
  adjusting a rejected rectangle until it passes has misunderstood the ordering.
- **`aura_raw::colour::lens` is where the optics maths lives.** A second copy is a second answer
  to where a face is.
