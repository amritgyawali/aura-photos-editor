# AURA-ML-5101 - The naturalness guard made an operation gentler, or withdrew a family

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Some photographs have their small fixes applied more gently than elsewhere, and a few have one
family of them missing entirely. Those frames carry `micro_hair_energy_lost`,
`micro_catchlight_at_risk` or `micro_teeth_off_locus` in the panel, with the measured number
beside them.

## What actually happened

**This is the mechanism working.** It is registered as an error so that it is visible, not because
something is wrong.

`micro::guard::enforce` applies the plan through the real renderer and measures three quantities
on the result:

| Measurement | Held to | What it protects |
|---|---|---|
| `catchlight_ratio` | `CATCHLIGHT_FLOOR` | the highlight that makes an eye look alive |
| `hair_energy_ratio` | `HAIR_ENERGY_FLOOR` | the hairline, against a bald patch |
| `teeth_excursion` | `TEETH_EXCURSION_CEILING` | teeth staying inside the measured locus |

A family that misses its bound gives up a quarter of its strength and is measured again, up to
three times. If three re-solves do not reach it, **that family is withdrawn** and the rest of the
plan ships.

Withdrawal is **per family**, unlike phase 20's texture guard, which withdraws the whole plan. The
three measurements are over three disjoint regions and map to three disjoint operation families,
so a frame whose teeth could not be evened safely still gets its lint removed. ADR-0043 section 5.

Frames that trigger this are usually one of:

* rim light through hair, where a flyaway and the photograph's own subject are the same structure;
* a very small face, where the iris is a handful of pixels and the catchlight is one of them;
* strongly coloured light with no illuminant estimate, where the teeth locus has no origin - that
  case reports `micro_no_illuminant` instead and no colour move is attempted at all.

## What to do

1. Nothing, usually. `MicroOutline::mean_catchlight_ratio` and `mean_hair_energy_ratio` are the
   numbers to watch across a project.
2. If a whole scene triggers the hair family, check that scene's row in `micro_retouch.toml`. The
   background gate is what should be deciding, not a threshold - if it is withdrawing everywhere,
   the gate is doing its job on busy backgrounds.
3. If one specific frame matters, doing that fix by hand is the right answer, and the panel says
   what the measured number was so the choice is an informed one.
