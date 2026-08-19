# ADR-0033 - Tone curves, HSL by content, and skin protection as a hard constraint

- **Status:** accepted
- **Date:** 2026-08-19
- **Phase:** 16 - Tone AI, Adaptive Curves, HSL AI & Skin-Tone Protection
- **Supersedes:** nothing. Extends ADR-0029 (render pipeline) and ADR-0031 (exposure,
  white balance and skin).
- **Deciders:** CTO, Colour Scientist, ML Lead - Vision, Product Manager

## Context

Phase 15 made a photograph *correct*: the exposure is on the subject's face and the white
balance is the colour of the light that was actually in the room. Phase 16 makes it *look
like a photograph somebody paid for* - contrast, highlight and shadow recovery, an adaptive
tone curve, and per-band hue, saturation and luminance adjustments driven by what is
actually in the frame.

Every product that grades globally eventually produces orange or grey skin. Phase 16
section 1 states the consequence plainly: photographers notice immediately, and a single
frame with wrong skin costs more trust than fifty frames with slightly flat contrast. So
the central question this ADR answers is not "what should the curve look like" but **where
the skin guarantee lives**.

Seven decisions follow. Each of them was a real fork.

## Decision 1 - The skin guard is a post-condition with a re-solve, not a mask the solver
consults

**Chosen:** grade first, *measure* the hue and chroma shift on the skin pixels afterwards,
and if either ceiling is exceeded, re-solve with a stronger attenuation and record the
event as a reason. The loop runs at most [`skin_guard::MAX_RESOLVES`] times and the final
attenuation is stored.

**Rejected:** attenuating inside the HSL solver by a factor derived from the mask, once,
and trusting the arithmetic.

The rejected design is what every product that has shipped orange skin was built on. Its
failure mode is not that the attenuation is wrong; it is that the attenuation is applied to
*the parameter* and the ceiling is stated about *the pixels*, and the two are separated by
a whole render. Vibrance is not linear in the saturation of the pixel it touches, two bands
overlap on a skin pixel, and a curve that lifts the shadows moves a skin pixel's chroma
without touching any HSL band at all. A post-condition measured on the pixels cannot be
fooled by any of that, because it is measured on the thing the ceiling is about.

The cost is that a graded frame is measured twice and, on the frames where the guard fires,
three or four times. Section 11 budgets 20 ms per image and the measurement is over a
sub-sampled skin mask rather than a whole frame, so the worst case is a few hundred
microseconds. It is worth every one of them.

## Decision 2 - Monotonicity is a property of the control points, not of an evaluator

**Chosen:** [`ToneCurve`] can only be constructed through a checked constructor that refuses
a set of points whose `x` is not strictly increasing or whose `y` decreases anywhere. There
is no evaluator in `aura-core`.

**Rejected:** a `ToneCurve::evaluate` in the contract that clamps its own output monotone.

Phase 14 froze the renderer and its point curve is interpolated by Fritsch-Carlson
(`aura_render::tonemap::CurveLut`), which has one relevant property: **given monotone
control points it produces a monotone curve, and given non-monotone control points it does
not**. So monotonicity of the *rendered* curve is exactly the monotonicity of the control
points, and a second evaluator in the contract would be a second answer to "what does this
curve do" - the failure ADR-0029 section 3 spent a page on.

The consequence is that the interpolation moved. Fritsch-Carlson now lives once, in
`aura_raw::colour::curve::{monotone_tangents, pchip}`, and both `aura-render`'s LUT builder
and phase 16's curve fitter call it. The arithmetic is byte-for-byte what `CurveLut::build`
did before the move; phase 14's golden suite is the regression guard and it passes
unchanged. This is the same move phase 10 made when the 112 px face warp got its second
consumer, and for the same reason: two copies of an interpolation is two curves that drift
apart while looking identical.

## Decision 3 - The eight HSL bands are named twice, and a test is what keeps them one thing

`aura_recipe::contract::recipe::HSL_BANDS` is a frozen array of eight strings and
`aura_core::contract::colour::HslBand` is a frozen enum of eight variants. `aura-core`
depends on no workspace crate, so the enum cannot read the array.

**Chosen:** define both, and assert their agreement in `aura-app`, which is the one crate
that can see both. `crates/aura-app/src/colour_commands.rs`'s
`the_eight_bands_agree_with_the_recipes` fails the build the moment somebody adds a ninth
band to one and not the other.

**Rejected:** moving `HSL_BANDS` into `aura-core`. It is phase 14's frozen contract and it
is *about the recipe document*; a band list in `aura-core` would be a second authority over
a schema `aura-core` cannot see.

## Decision 4 - Content bands are measured from chroma clusters, not from a segmentation model

Section 6.2 asks for "segmentation cues (greenery, sky, skin, dress, wood, decor)". There is
no segmentation model in this repository and phase 18 is where masks are built.

**Chosen:** [`content`] clusters the frame's chroma into six named content bands using hue,
saturation, luminance and *position* priors - sky is blue, desaturated-bright and above the
horizon; greenery is green-yellow and usually below it; a dress is bright, near-neutral and
near the subject; decor is whatever is left and saturated. Each band reports its area, its
mean hue and saturation, and a **confidence**, and a band below
[`content::MIN_CONFIDENCE`] is not adjusted at all.

**Rejected:** waiting for phase 18's masks. Phase 16 unlocks 17, 25 and 27; a phase that
ships nothing until a later phase exists is a phase that has not shipped.

The honest statement is in the reason vocabulary: [`ColourCode::ContentInferred`] says the
band was inferred from colour statistics rather than segmented, and it is emitted on every
frame where an HSL adjustment was made. When phase 18's masks arrive this module gets a
better input and the interface does not move.

## Decision 5 - The tone parameters are solved, and the learned head is not consulted

The shipped `tone_model` is an untrained placeholder - the seventh in the product, for the
seventh time the same reason: there is no corpus of RAW files paired with expert edits here.

**Chosen:** [`TONE_HEAD_TRAINED`] is `false` and [`tone::Solver::hint`] returns `None`, so
the head is **never run**. The five parameters come from a deterministic solver over the
histogram, the subject luminance, phase 09's noise estimate and the scene's intent row.

**Rejected:** running the head and blending it with the solver at some weight. A random
projection of a histogram blended at 30 % is a random contribution of 30 %, and it would be
indistinguishable in the panel from a learned one.

The solver is not a fallback for the head. It is the shipped behaviour, it is what section
10.1's gates measure, and the head is the thing that will one day have to beat it.

## Decision 6 - A variant is a whole parameter set, and there are exactly three

Section 6.4: "all decisions store 2-3 alternatives (flatter, punchier, warmer) so the user
or QC can switch instantly without recomputation".

**Chosen:** [`ColourVariant`] carries the five tone parameters, the vibrance, the saturation
and its own curve. It is a complete answer, not a delta.

**Rejected:** storing a delta against the primary. A delta is smaller and it is wrong: the
clipping guard and the skin guard both act on a *whole* parameter set, so a variant produced
by adding a delta to a guarded primary is a set nobody guarded. Every variant here has been
through both guards, which is what makes "switch instantly" safe rather than fast.

Three rather than "2-3": flatter, punchier and warmer are the three axes a photographer
actually reaches for, and a variable-length list makes the panel's switcher a layout
problem for no benefit. A variant that would be identical to the primary after clamping is
dropped, so a frame can carry fewer than three - and that is a real answer, not a gap.

## Decision 7 - Nothing in this phase is an edit, for the eighth time

`image_colour_decision` has no path column, no applied flag and no way to express one. The
values reach the pixels through `aura_recipe::schema::merge` and nowhere else, called from
`aura-app::colour_commands`, exactly as phase 15's three numbers do.

`aura-brain-photo` still does not depend on `aura-recipe`. It cannot write a recipe, so the
rule that a person's parameter is never overwritten is not something this phase has to
remember.

## Consequences

- **`ColourService` is the only way to ask how a photograph should be graded.** Twelfth
  service of its kind. Phase 17 shifts these values toward a photographer's own style, 18
  grades locally on top of them, 25 normalises a gallery of them and 27 checks them. Two
  answers to "how much contrast does this frame want" is an album that does not match the
  gallery.
- **The skin ceiling is measured on every frame and stored.** `SkinGuardReport` is on the
  wire and in the catalog, so "skin never shifts measurably" is a query rather than a claim.
- **A curve that could posterise cannot be constructed.** `ToneCurve::new` refuses it, the
  store's decoder refuses it, and the section 10.1 property test renders a 4,096-step ramp
  through the real renderer to prove the refusal is sufficient.
- **The interpolation moved to `aura-raw`.** `aura-render`'s `CurveLut::build` is now a
  caller. No pixel changed.

## The measurement this ADR does not license

Section 10.1 gates the skin shift "on all skin-tone buckets". The buckets live in
`tests/eval/colour_eval.rs` and are five *reflectances* rather than five people, exactly as
phase 15's are. Nothing in the product stores a skin-tone bucket, there is no column for
one, and the phase gate scans the schema on every run. `docs/skin-fairness.md` says what the
measurement is and is not, in the product's own words, and phase 16 adds a section to it
rather than starting a second document that could disagree with the first.
