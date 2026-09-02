# AURA-LRN-11003 - An update could not be worked out from the corrections available

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

AURA could not work out an update from your corrections. Nothing about your photographs or your profile has changed.

## What actually happened

`LearnService::compute` could not produce a candidate update.

The ordinary reasons are not errors and do not reach this code - a bucket below `MIN_CORRECTIONS`
or `MIN_PROJECTS` produces `Ok(None)` with a `LearnCode` saying which. This code means the fit
itself failed: an unknown profile, a held-out split that could not be drawn, or a stored profile
whose version does not advance.

## What to do

Check that the profile named still exists in the style panel. Nothing about the current profile
has changed and the corrections are still stored, so a later attempt costs nothing.

## Where it comes from

PHASE-30. See `docs/adr/ADR-0061-delivery-learning-loop-and-release.md` and
`docs/plan/phases/PHASE-30-DELIVERY-INTEGRATIONS-LEARNING-LOOP.md`.
