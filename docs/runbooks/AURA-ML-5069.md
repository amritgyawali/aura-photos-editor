# AURA-ML-5069 - The skin protection ceilings could not be met, so the colour adjustments were withdrawn

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The photograph is graded - contrast, curve, highlights and shadows all applied - but every
HSL band is at zero, vibrance and saturation are untouched, and the reasons carry
`skin_guard_withdrew`.

## What actually happened

PHASE-16 section 6.3 sets two ceilings on what grading may do to skin: at most
`SKIN_HUE_CEILING_DEG` degrees of hue rotation and at most `SKIN_CHROMA_CEILING` of relative
chroma change, measured on the skin pixels *after* the grade rather than promised before it.
When the first answer exceeds either ceiling the guard re-solves with a stronger attenuation,
up to `skin_guard::MAX_RESOLVES` times. This code fires when even a fully attenuated colour
solve still moves skin too far, and the response is to withdraw the colour operations
entirely.

That is the guarantee working, not failing. The two situations where it happens:

1. **A person is lit by a saturated coloured light.** A face under a magenta uplighter sits
   in the same band as the uplighter, so a band adjustment aimed at the decor lands on the
   face. Phase 18's local masks are what eventually separate the two.
2. **The skin mask is very large.** A tight portrait can be forty percent skin, and at that
   area almost any global saturation change moves it measurably.

## Operator steps

1. Look at the frame. The withdrawal is usually invisible, which is the point: it is the
   difference between a frame with slightly flat decor and a frame with orange skin.
2. Check `skin_guard.mask_area` on the decision. Above about 0.3 case 2 is the explanation
   and no action is needed.
3. If a photographer wants the colour adjustment anyway, they set it by hand. A person's
   value is unbeatable and the guard does not re-open it.
4. Aggregate counts are in the `colour.skin_guard_triggered` telemetry event. A project where
   this fires on more than a few percent of frames is worth reporting with the scene
   histogram attached.
