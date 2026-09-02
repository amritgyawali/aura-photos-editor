# AURA-DLV-10001 - The gallery provider named is not one this build has

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

AURA does not know that client gallery. Choose one from the list in the delivery settings.

## What actually happened

A provider name reached `DeliveryService` that the registry in `aura-delivery` does not have, or a
name that is not syntactically legal - a provider name reaches both a file path and a catalog key,
and the two have different ideas about what is allowed in one.

## What to do

Choose a provider from the list. If the name came from a saved configuration written by another
build, re-save it from the delivery settings.

## Where it comes from

PHASE-30. See `docs/adr/ADR-0061-delivery-learning-loop-and-release.md` and
`docs/plan/phases/PHASE-30-DELIVERY-INTEGRATIONS-LEARNING-LOOP.md`.
