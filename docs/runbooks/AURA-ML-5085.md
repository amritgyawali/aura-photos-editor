# AURA-ML-5085 - A local light strength override was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The strength slider they moved snaps back and a message says nothing was changed. This is
the correct behaviour: a refused override that appeared to have been taken is worse than one
that visibly was not.

## What actually happened

`LocalService::set_override` and `LocalService::accept` refuse in four cases:

1. **The photograph has no plan.** The local pass has not reached it, or it failed with
   `AURA-ML-5086`. There is nothing to disagree with yet.
2. **The override sets nothing.** Every one of the six strengths is `None`. An empty override
   would set `user_edited` on a frame nobody edited, which would then be protected from every
   future improvement for no reason.
3. **A strength is outside `0..1`.** Almost always a caller bug rather than a UI one.
4. **The photo id does not parse.**

`LocalOverride::problem` is the single predicate all four come from, and it lives in
`aura-core` so the panel, the store and the eval harness cannot disagree about what a
readable override is.

## Operator steps

1. Check whether the frame has a plan at all: `SELECT * FROM local_light_plan WHERE photo_id
   = ?`. If not, run the local pass over the project first.
2. If it does, the message names which strength was out of range.
3. `local.gated` telemetry with a high count usually means the same frames have no usable
   mask, which is `AURA-ML-5089` and a different problem.
