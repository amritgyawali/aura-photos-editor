# AURA-DLV-10006 - A set has nowhere to go at this provider and was not uploaded

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

One of these sets has no place to go in that gallery, so it was left out. Map it in the delivery settings and run the upload again.

## What actually happened

A set in the delivery had no entry in the provider's set mapping, so it was not uploaded.

A `warning` rather than a failure, because a photographer who mapped the gallery and not the album
usually meant to.

## What to do

Add a mapping for the named set in the delivery settings and run the upload again. Only the
unmapped set is sent; everything already uploaded is left alone.

## Where it comes from

PHASE-30. See `docs/adr/ADR-0061-delivery-learning-loop-and-release.md` and
`docs/plan/phases/PHASE-30-DELIVERY-INTEGRATIONS-LEARNING-LOOP.md`.
