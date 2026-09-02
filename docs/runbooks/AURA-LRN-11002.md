# AURA-LRN-11002 - A stored correction names a setting this build does not learn

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

AURA found a correction about a setting it does not learn from. Nothing is lost; it simply does not count toward your profile.

## What actually happened

A stored correction names a `Learnable` this build does not have.

The `Learnable` list is deliberately closed - fifteen members, no `Other` - because what is *not* in
it is the guarantee: no mask boundary, no retouch ceiling, no crop safety margin, no identity cap.
A correction naming something outside the list is either from a newer build or from a value that was
removed from the list on purpose.

## What to do

Nothing to do. The correction is kept and does not count toward the profile.

## Where it comes from

PHASE-30. See `docs/adr/ADR-0061-delivery-learning-loop-and-release.md` and
`docs/plan/phases/PHASE-30-DELIVERY-INTEGRATIONS-LEARNING-LOOP.md`.
