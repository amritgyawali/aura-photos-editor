# AURA-ML-5114 - A rotation or perspective correction was reduced or abandoned

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

A photograph levelled less than it should have been, or left at the angle it was shot, with the
reason in the Framing panel.

## What actually happened

**Rotation implies cropping.** Levelling a frame by two degrees means taking the largest
upright rectangle that fits inside the rotated one, and that rectangle is smaller than the frame.
When the crop it costs would break a safety rule - cutting a face, or falling below the resolution
floor - the angle is reduced until it does not, and abandoned if no angle works.

The same applies to the keystone: a perspective correction whose measured stretch would exceed
1.12 is refused rather than clamped.

Both carry **the angle that was wanted beside the angle that survived**, because a photographer
who can see that AURA wanted 3.4 degrees and applied 1.1 understands what happened; one who only
sees 1.1 thinks the horizon detector is wrong.

## What to do

1. Nothing, usually. This is the guarantee in `docs/geometry.md` being kept: nothing in this phase
   crops into somebody in order to level a horizon.
2. If a photographer wants the full rotation, they can set it by hand on that photograph.
3. If the count is high across a whole wedding, check whether the frames are genuinely tilted or
   whether phase 11's horizon confidence is low; below 0.70 nothing is rotated at all, and the
   reason code for that is `geometry_horizon_unsure` rather than this one.
