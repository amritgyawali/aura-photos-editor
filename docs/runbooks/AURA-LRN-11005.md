# AURA-LRN-11005 - A profile could not be rolled back

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

AURA could not put the earlier version of that profile back. The version you are on is unchanged and still works.

## What actually happened

`LearnService::roll_back` could not restore the previous version of a profile.

Either there is no earlier version - a profile at version 1 has nothing behind it - or the stored
bytes did not restore to a profile that verifies.

## What to do

The version in use is unchanged and still works. `ROLLBACK_DEPTH` versions are kept; if the one
you want is older than that it is gone, and re-training from the corrections is the route back.

## Where it comes from

PHASE-30. See `docs/adr/ADR-0061-delivery-learning-loop-and-release.md` and
`docs/plan/phases/PHASE-30-DELIVERY-INTEGRATIONS-LEARNING-LOOP.md`.
