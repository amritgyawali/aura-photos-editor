# ADR-0041 - Geometry: the safety filter, the recipe's coefficients, and what an estimated lens is worth

**Status:** accepted · **Date:** 2026-08-26 · **Phase:** 23 · **Supersedes:** nothing
· **Amends:** [ADR-0029](ADR-0029-render-pipeline.md) section 3 (`aura_recipe::Lens`)

Phase 23 section 4 asks for no ADR by name. It needs two anyway, and this is the first: section
5's frozen shape needs an input port that phase 23 does not own, section 6.1's three routes to
a lens correction are not equally trustworthy and the difference has to be structural rather
than remembered, section 6.2's "reduced or skipped" hides a solve, and section 6.3's improvement
margin cannot apply to the thing section 6.3's last bullet asks for. The second document is
[ADR-0042](ADR-0042-geometry-ipc-surface.md), which covers the wire.

The ADR numbering in this repository is sequential across the whole project rather than aligned
to phase numbers.

## 1. Context

Twenty-two phases have decided which photographs are delivered, what they are of, whether they
worked, how they should look and how light moves inside them. Not one of them has removed
anything from a photograph. This is the phase that does, and section 1 is direct about the
asymmetry:

> Smart crop is where automation is most dangerous, so a subject-aware, conservative,
> always-reversible crop is a trust feature as much as a quality feature.

Three things make it hard, and they are different difficulties.

**A wrong crop is invisible until it is printed.** A wrong exposure looks wrong on the screen
it was decided on. A frame with somebody's hand missing from the edge looks like a frame; the
photographer finds out at the album proof, or the guest finds out at the wedding. Phase 18 made
this argument about masks - "a wrong mask is silent" - and here the silence is longer.

**Most of the work is refusal.** Section 10.1 asks that at least seventy per cent of frames keep
their original framing. A phase whose headline behaviour is *not acting* has a reporting problem
as much as a decision problem: eleven of this phase's twenty-four reason codes describe
something that did not happen, which is the highest ratio in the product, and a panel that can
only explain what it did is a panel that looks broken on most photographs.

**Its three jobs share one resample and must not each take their own.** Section 12's last
failure mode is "resampling softens images", and the mitigation is "geometry applied once in the
render graph rather than repeatedly". A lens correction, a rotation, a keystone and a crop are
four coordinate maps that compose; applying them separately is four interpolations of a
photograph that needed one.

## 2. The safety filter runs before the objective, not after it

**Decision.** Every rectangle the composition objective scores has already passed
`CropSafetyReport`. A candidate that cuts a face, cuts a primary identity's hands, falls below
the scene's resolution floor or drops phase 11's crop hint is removed from candidacy, not
penalised within it.

**The alternative, and why it loses.** Scoring safety as a heavy negative term is simpler to
write and reads as more principled: one objective, one number, one argmin. It fails for a
reason that has nothing to do with the weight. A filter applied *after* the objective invites
exactly one repair - nudge the winning rectangle until the face is back inside - and a nudged
crop is a different aspect ratio, a different resolution, or a fresh violation at the opposite
edge. Nobody writes a test for the nudge, because the nudge is the fix.

This is phase 12's rule - a guarantee outranks a preference, in that order, always - in the
phase where the preference is a score somebody tuned and the guarantee is a bride's hands.
`crop::best` cannot see a rejected rectangle, and `Objective` has nowhere to put a safety term,
so the ordering is a property of the type system rather than a test result that could drift.

**The consequence to be honest about.** A frame the filter refuses everywhere produces no crop
at all, and on a dance floor frame with a limb at every edge that is most of the search space.
`GeometryCode::REFUSALS` is a four-slot histogram on every plan for exactly that: "AURA tried
and could not, because of faces" and "AURA saw nothing worth changing" must never be the same
row. Phase 08's rule about denominators, applied to a refusal.

## 3. Faces before hands before content before resolution

**Decision.** `safety::refusal` returns the *most serious* reason a rectangle failed, and the
order is fixed: a face, then hands, then key content, then the resolution floor.

It looks like a cosmetic choice and is not. The code that comes back is what the photographer
is shown and what section 11's `geometry.crop_refused {reason_histogram}` counts, and section
9 gives QAIQ "audit 300 auto-crops: any cut hand, cut face or worse framing is a bug". A
rectangle that cuts a face *and* falls below the floor is a rectangle that cut a face; reporting
it as "too small" - which is what the cheap ordering does, because the resolution test is one
multiply and the face test is a loop - would make that audit under-count exactly the candidates
it exists to find.

## 4. The coefficients travel in the recipe

**Decision.** `aura_recipe::Lens` gains `coefficients: Option<LensCoefficients>`, carrying `k1`,
`k2`, `k3`, `ca_red` and `ca_blue`. The renderer looks nothing up.

This **amends a frozen contract**, which has happened twice before - phase 09's `FaceRef`, phase
16's re-lock - and the rule is the one `docs/plan/CLAUDE.md` section 8 states: an ADR, then a
re-lock, in that order. This is the ADR.

**The argument that was tried first and rejected.** The recipe says *what* to apply and the
profile table says *by how much*; the renderer resolves `lens.profile` against
`assets/lens_profiles/` at render time. It is tidier, it keeps the recipe small, and it means a
profile improvement reaches every photograph in the catalog for free.

That last property is the problem. Phase 14's rule is that **a delivered file can be re-created
from four values** - the RAW's content hash, the canonical recipe, the engine string and the
output spec - and all four are stored with every export in `export_renders`. A coefficient that
lived only in a profile table would be a fifth value, unversioned in the recipe hash, and
updating the table would silently change what an already-delivered photograph looks like. The
photographer would re-export a gallery six months later and get different pixels with an
identical hash. `AURA-RENDER-8007` exists to say which of the four moved, and it could not say
this one had.

With the numbers in the recipe, a profile update changes what AURA *would decide* for
newly-planned frames - `profile_ver` moves, `AURA-ML-5090` fires, the pass re-plans - and leaves
every delivered photograph exactly as it was. That is the correct division: the table is an
input to a decision, and the decision is what gets stored.

**The maths, unlike the numbers, is shared.** `aura_raw::colour::lens` owns `radial`,
`source_of`, `dest_of`, `valid_scale` and `keystone_source`, and both `aura-geometry` and
`aura-render` call it. Two copies of a distortion polynomial is two answers to where a face
is - one used to check that a crop does not cut it, the other used to draw the crop - and the
disagreement is silent because both frames look like photographs. `aura_raw::colour::profile`
makes the same argument about camera matrices and `aura_raw::colour::curve` about monotone
interpolation; `aura-raw` is the lowest crate both sides reach. `gate_11` asserts the agreement
rather than trusting it, because one implementation is a fact about the dependency graph today
and a comment tomorrow.

## 5. `ProtectedRegion` is an input port, and phase 23 owns no generator for it

**Decision.** Faces come from phase 06, hands from a pose estimate and key content from phase
11's `CropHint`. `aura-geometry` has no face detector, no pose model and no saliency fallback,
and `tests/no_render_calls.rs` greps for one.

Phase 18 wrote this rule from the producing side; this is the first phase to inherit it from the
consuming side, and the stakes are higher than they were for a mask. A second answer to "where
is her face" is a crop that cuts a face this product elsewhere insists it can see - and unlike a
mask, whose failure is a soft rim, this failure is a missing person.

**What that costs on this build, stated plainly.** There is no pose estimate in the product, so
`hands_checked` is **zero on every photograph**. Section 10.1's zero-face-cut gate is a claim;
the same gate for hands is currently a claim about an empty set. `CropSafetyReport` carries
`faces_checked` and `hands_checked` as *counts* rather than as booleans for exactly this reason:
a crop over a frame with no detected faces satisfies the face rule trivially, and storing that
as `faces_intact = 1` alone would make a build with no detector look like a build whose crops
are provably safe. `is_evidence()` is the predicate a panel asks before it says "safe". Phase
09's rule about denominators, applied to a guarantee.

## 6. Straightening: the rotation and its crop are solved together

**Decision.** `straighten::decide` takes the protected regions and returns a rectangle as well
as an angle. It walks down from the wanted angle in twelve steps and takes the largest one whose
implied crop is safe; if none is, it does not rotate.

Section 6.2 says "if it cannot, the rotation is reduced or skipped", which reads like a fallback
and is a solve. A straightening tool that hands a caller an angle and lets the crop be somebody
else's problem is a tool that levels a family formal by cropping the grandmother out of the left
edge - and the caller, who received an angle, has no way to know that happened.

**A linear scan rather than a bisection**, for the reason phase 15 gave about its illuminant
walk: the safe set is not an interval. Reducing the angle grows the inscribed rectangle in one
axis and shifts it in the other, so a bisection returns *an* angle that works rather than the
largest one that does.

**The inscribed rectangle keeps the frame's own aspect ratio.** The classical
`rotatedRectWithMaxArea` result is larger and was implemented first. It is the wrong answer: its
shape depends on the angle, so levelling a 3:2 frame by two degrees would deliver 1.72:1 and by
four degrees something else again. A photographer who asked for a straighten did not ask for a
reframe, and a gallery whose frames are each a slightly different shape cannot be laid out. With
the shape fixed the closed form is two inequalities and no branch, which is also how the
"does it actually fit" test became checkable.

**The confidence gate is 0.70, above phase 11's 0.60.** The two numbers answer different
questions. Phase 11's `HORIZON_ACT_AT` is where an estimate stops being worth *showing* a
photographer; this is where it becomes worth *turning their photograph by*. A const assertion
refuses a build in which this ever drops beneath phase 11's floor, because a phase acting on an
estimate phase 11 would not even report is a phase acting on nothing.

## 7. Keystone is refused past the cap, never reduced to it

**Decision.** `MAX_STRETCH` is 1.25. Above it `Keystone::new` returns an error and the frame is
left alone.

Clamping is the obvious alternative and it is worse than doing nothing. A keystone halved to fit
a cap has stopped correcting anything - the walls still lean, by half as much - and the frame
has been resampled and cropped to achieve it. Half a correction is the worst of both, and the
photographer is left with a frame that is neither as shot nor square. Phase 16 made the same
argument about clamping a curve node, and arrived at it from the other side: clamping a node
that wanted to sit above white produced a flat top, which is a posterised band and new clipping
in one move.

**Three verticals, not two.** Two lines always meet somewhere, and calling that a vanishing
point is how a keystone tool squares up a frame containing one door frame and a guest.
`verticals` is stored on the row so a correction can say what it was fitted from.

## 8. The improvement margin applies to the primary crop and to nothing else

**Decision.** `CropPurpose::must_improve()` is true for `Primary` and false for `Album`,
`Social` and `Wide`.

Section 6.3 asks for both an improvement margin - "a proposed crop must improve the composition
score by a minimum margin, otherwise the original framing wins" - and aspect variants for social
and album use. Applied to both, the second requirement cannot be met: a 1:1 crop of a wide
reception frame will essentially never score better than the frame it came out of, because it
has thrown away the context that made the composition work. Requiring an improvement from the
variants ships a product with no square variants in it at all.

The margin exists to protect the *delivered* framing. A variant is asked for by its purpose. It
still has to pass every safety rule, which is why a dance floor scene generates none, and it is
spelled as a method on the enum rather than as a branch in the solver so that the next person to
read the objective finds the reason next to the rule.

## 9. What an estimated lens correction is worth, and what it is therefore allowed to do

**Decision.** A correction fitted from the frame's own straight edges applies **distortion
only** - never chromatic aberration, never vignetting - and caps `GeometryPlan::confidence` at
0.70.

Section 6.1 gives three routes in order of preference and treats them as interchangeable once
chosen. They are not.

**Fringing is withheld because an estimate can invent it.** A CA correction fitted from the same
high-contrast edges it is meant to clean will happily produce a rim of the opposite colour, and
a photographer looking at a purple edge they did not have before has been actively harmed rather
than merely unhelped. `LensSource::is_measured` is the predicate, and it is the reason the enum
distinguishes `Profile` from `Estimated` at all.

**Vignetting is withheld because an estimate cannot tell optical falloff from a dark wall**, and
the failure mode is a brightened wall.

**What the estimator is actually worth, measured.** Against painted grids at 512 px over barrel
and pincushion from `k1 = 0.03` to `0.06`: the sign is always right, the magnitude is within
thirty per cent, and **it never over-corrects**. The third is the one that matters and it is not
luck - a gradient tracker follows a stroke's edge, snaps to whole pixels and loses two or three
of them at every crossing, and all three flatten a curve rather than sharpen it. An
under-correction leaves a slight bow nobody sees; an over-correction turns barrel into
pincushion, which reads as a mistake because it is one. `gate_4` asserts all three.

Below about `k1 = 0.02` at that resolution the bow of a straight line is smaller than the pixel
it was tracked to, and the estimator declines rather than fitting tracking noise. That is the
correct answer and `gate_4a` asserts it: a correction nobody asked for is a resample nobody
asked for.

## 10. Three things the estimator got wrong first

All three were found by this phase's own gates, and all three generalise.

**A crossing is not an ending.** The edge tracker died at every intersection, because the
gradient *along* one edge collapses for two or three pixels where another crosses it - both
neighbours are on the other edge. An eleven-by-eleven grid produced chains of twenty-three
pixels and the span floor rejected every one of them: zero chains, on a plate made of nothing
but straight lines, with every unit test passing.

**A robust fit must reject the chains no coefficient can straighten, not the chains with the
largest residual.** The first robust attempt trimmed the worst third by residual, on the
reasonable-sounding grounds that a tracker following a photograph produces junk. It scored 0.000
against a painted 0.020 - because on a genuinely distorted frame the largest residuals belong to
the chains nearest the *edge*, which are the only ones that see any distortion at all. Trimming
by residual keeps the optical centre and throws away the evidence. What separates junk from
signal is not the size of a residual but whether any coefficient removes it: a bent straight line
straightens, a kink does not.

**Re-acquiring after a gap needs a window as wide as the gap.** A tracker that only ever looks
one pixel either side re-acquires at the wrong place after a three-row crossing, which holds the
chain flat at every intersection and quietly straightens the very curvature the estimator exists
to measure. It biased the recovered coefficient low by about a sixth, and every chain agreed
with every other chain about the wrong answer - which is what makes this class of bug survive
review.

The general form of all three: **a measurement pipeline can be wrong in a way that is
self-consistent**, and unit tests over synthetic inputs to the *fitter* will not find it because
the fitter is correct. Only a gate that runs the whole pipeline against a known answer will.

## 11. What this phase does not claim

Three conditions, carried into `docs/progress/PHASE-23-EXIT.md`.

**C1 - there are no wedding photographs and no expert crop labels here.** Section 9 gives DATA
"expert crop labels on 2k frames; architecture and tilt sets" and there are none. Every gate in
section 10.1 measures a geometry that was chosen, painted into the pixels and read back through
the real pipeline. That proves the estimator, the tracker, the caps, the safety filter, the
search and the store. It is not evidence that a photographer would agree with a crop, and
section 10.1's QAIQ audit of 300 auto-crops has not happened. Sev 2.

**C2 - every lens profile in `assets/lens_profiles/` is fabricated.** No lens was measured. The
coefficients have the right sign and order of magnitude for the focal length and are not
measurements. `ProfileTable::is_synthetic` is on the wire and in the panel, so a photographer is
never told a lens was profiled when it was invented. Phase 14 said the same thing about camera
profiles and the shape of the honesty is identical: this is a determinism and regression gate,
not a claim about optics. Sev 2.

**C3 - there is no pose estimate, so no crop in this product has ever been checked against a
pair of hands.** Section 5 above.

## 12. Consequences

- `GeometryService` is the sixteenth service of its kind and the only way to ask how a
  photograph's frame was finished. Phase 27 checks these crops, phase 29 lays albums out of the
  variants and phase 30 exports them.
- `aura_recipe::Lens` carries coefficients; `contracts.lock` is re-locked and
  `crates/aura-recipe/tests/fixtures/recipe_v1_golden.json` re-blessed.
- `aura_raw::colour::lens` is new and is where the optics maths lives for the whole product.
- `Capabilities::geometry_models` is true on the reference path for the first time, so
  `SkipReason::GeometryAbsent` stops being the answer to a perspective correction.
- Three shader entry points moved out of `colour.wgsl` and `spatial.wgsl` into `geometry.wgsl`,
  because all three gather from somewhere else in the source and every other entry point in
  those files reads index `i` and writes index `i`.
