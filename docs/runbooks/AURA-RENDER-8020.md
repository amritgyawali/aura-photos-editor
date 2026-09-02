# AURA-RENDER-8020 - A stored delivery note came from a build with a different reason vocabulary

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

AURA found a delivery note it does not recognise, which usually means this wedding was delivered by a newer version.

## What actually happened

A stored `delivery_reason` row names a slug this build's `DeliveryCode` does not have.

That is what a catalog written by a newer release looks like. It is `degraded` rather than an item
failure because the honest response is to draw the panel without that one note, not to refuse to
show a photographer what they delivered.

## What to do

Re-run the export summary, or open the wedding in the release that wrote it. Nothing about the
delivered files has changed - the note is a sentence about a file, not the file.

## Where it comes from

PHASE-30. See `docs/adr/ADR-0061-delivery-learning-loop-and-release.md` and
`docs/plan/phases/PHASE-30-DELIVERY-INTEGRATIONS-LEARNING-LOOP.md`.
