# AURA-ML-5066 - A tone curve was refused because its control points are not monotone

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The photograph is graded, but the Curve panel shows a straight line and the reasons carry
`curve_identity`. Contrast, highlights and shadows all applied normally.

## What actually happened

`aura_core::contract::colour::ToneCurve::new` is the only way to build a curve and it refuses
a set of control points that is not strictly increasing in `x`, not anchored at 0 and 255, or
that decreases anywhere in `y`. PHASE-16 section 6.1 makes monotonicity structural rather
than checked after the fact, because the renderer's Fritsch-Carlson interpolation is monotone
exactly when its control points are: a non-monotone point set produces a band of inverted
contrast that reads as a bug in the renderer rather than as a bad curve.

The three ways to get here:

1. **A solver bug.** `colour::curve::fit` is the only caller in the product and it clamps
   every point it emits, so this is the case the code path exists to catch during
   development. It is a bug and it should be filed with the frame's id.
2. **A stored document a newer build wrote.** The store's decoder refuses the same way and
   the row reads as an identity curve, which is why the frame is still graded.
3. **A hand-authored override.** `set_colour_override` validates a supplied curve before it
   reaches the recipe; a refusal here is the validation working.

## Operator steps

1. Read the `points` context field on the error - it says how many points the refused curve
   had. Two means the fitter degenerated; nine means something bypassed the cap.
2. Re-run `estimate_colour` for the project. The curve is recomputed from the pixels, so a
   transient refusal heals.
3. If it repeats on the same frame, attach the frame's id and its `image_colour_decision` row
   to the report. The curve itself is not stored when it is refused, so the row is the
   evidence.
