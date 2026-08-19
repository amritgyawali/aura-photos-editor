# AURA-ML-5068 - One photograph's local light could not be planned

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

One frame in the Local panel says local adjustments were not made. It still carries every
global adjustment phases 15 and 16 decided, so it is a normally edited photograph rather than
a broken one.

## What actually happened

Either the proxy could not be read, or the plan the solver produced broke one of the phase's
own guarantees and was refused rather than stored. `LocalLightPlan::broken_guarantee` is the
predicate, and it refuses five things:

1. a plan with no reason - invariant 2, and a bug wherever it comes from;
2. a budget fraction outside `0..1`;
3. a paired subject/background move that shifted the frame's mean luminance by more than
   `MAX_MEAN_LUMA_DRIFT`, which is section 10.1's own acceptance criterion;
4. faces left further apart than `MAX_INTER_FACE_SPREAD` after lighting, which is the
   group-fairness criterion;
5. a shaping zone that moved in a direction its own kind forbids, or further than
   `ShapingZone::MAX_GAIN_EV`.

**A refused plan is stored as no plan rather than as a weak one.** Three of the five failures
above describe a photograph that would look visibly edited, and a visibly edited photograph
is the failure this phase exists to avoid.

## Operator steps

1. The message names the guarantee. Items 3, 4 and 5 are solver bugs and should be reported
   with the frame; items 1 and 2 are caller bugs.
2. Re-running the pass retries the frame. A frame that fails twice with the same guarantee is
   not a transient.
3. `tests/eval/local_eval.rs` asserts each of the five on a synthetic frame built to break it.
