# AURA-LRN-11001 - A stored learning note came from a build with a different reason vocabulary

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

AURA found a note about learning that it does not recognise, which usually means this profile was trained by a newer version.

## What actually happened

A stored `learn_reason` row names a slug this build's `LearnCode` does not have. A catalog written
by a newer release.

## What to do

Re-run the learning summary. Nothing about the profile has changed.

## Where it comes from

PHASE-30. See `docs/adr/ADR-0061-delivery-learning-loop-and-release.md` and
`docs/plan/phases/PHASE-30-DELIVERY-INTEGRATIONS-LEARNING-LOOP.md`.
