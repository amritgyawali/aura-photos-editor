# AURA-REL-12004 - The licence could not be checked and the offline grace period is running

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

AURA could not check your licence, so it is carrying on offline. Everything works; connect within the grace period to keep it that way.

## What actually happened

The licence could not be checked and the offline grace period has started.

By design this is a `warning` and nothing stops working. A wedding photographer's edit suite that
refuses to open in a marquee with no signal is a wedding photographer's edit suite that gets
replaced.

## What to do

Connect once inside the grace period. The remaining days are on the diagnostics screen. Nothing
about your catalogs, edits or exports depends on this check.

## Where it comes from

PHASE-30. See `docs/adr/ADR-0061-delivery-learning-loop-and-release.md` and
`docs/plan/phases/PHASE-30-DELIVERY-INTEGRATIONS-LEARNING-LOOP.md`.
