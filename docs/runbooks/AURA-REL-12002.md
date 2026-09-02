# AURA-REL-12002 - An update was applied, failed its first use, and was rolled back

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

An update did not work on this machine, so AURA put the previous version back. Nothing you have done has been lost.

## What actually happened

An update installed, failed its first real use, and was automatically rolled back.

Phase 03's rule - "a model is pending until it has worked once" - applied to the whole update
channel. The previous version is back and is the one running.

## What to do

Nothing is lost. The failed version is recorded as rejected and will not be offered again. If the
same update is rejected on two machines, that is a release problem rather than a machine one.

## Where it comes from

PHASE-30. See `docs/adr/ADR-0061-delivery-learning-loop-and-release.md` and
`docs/plan/phases/PHASE-30-DELIVERY-INTEGRATIONS-LEARNING-LOOP.md`.
