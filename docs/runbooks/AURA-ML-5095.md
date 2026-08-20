# AURA-ML-5095 - The texture guarantee forced a gentler retouch, or withdrew one

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Some photographs are retouched more gently than the preset asked for, and a few may not be
retouched at all. Those frames carry `texture_resolved` or `texture_floor_unreachable` in the
retouch panel, with the measured band ratio beside them.

## What actually happened

**This is the mechanism working.** It is registered as an error so that it is visible, not
because something is wrong.

`texture_guard::measure` band-decomposes the frame's skin, applies the plan through the real
renderer, decomposes the result, and divides the high-band energies. If the ratio is below the
preset's floor - 0.90 for Light and Natural, never below 0.80 for Polished - the solver gives up
a quarter of the strength and measures again, up to three times. If three re-solves do not reach
the floor, the retouch is **withdrawn entirely** and the frame ships unretouched.

A frame that could not be retouched safely is a much smaller failure than a frame that ships
with plastic skin, and a texture floor that can be exceeded "just this once" is not a floor.

Frames that trigger this are usually one of:

* very high ISO, where the high band is largely noise and the retouch removes some of it;
* a face that fills the frame, where a blemish is large in absolute terms;
* a skin region measured over very few samples - see `TextureReport::is_well_measured`.

## What to do

1. Nothing, usually. `RetouchOutline::mean_band_ratio` and `texture_resolved` are the numbers to
   watch across a project.
2. If the whole wedding triggers it, the frames are probably noisy enough that phase 22's
   denoising should run first; the retouch pass will then reach its floor comfortably.
3. If one specific frame matters, retouching it by hand is the right answer, and the panel says
   what the measured ratio was so the choice is an informed one.
