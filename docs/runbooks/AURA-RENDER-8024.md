# AURA-RENDER-8024 - One photograph could not be rendered for export

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

One photograph could not be prepared, so it is not in this export. Everything else was written and checked, and the summary names it.

## What actually happened

`RenderService::render` could not produce pixels for one photograph, so nothing was written for it.

Item-level, not run-level. A wedding with one unreadable RAW in it should still deliver the other
699 frames, and the summary names the one it could not.

The usual causes are the ones phase 02 and phase 14 already have codes for: an original that is no
longer at the path the catalog remembers, a RAW this build cannot decode, or a recipe that does not
canonicalise.

## What to do

Open the named photograph in the develop panel. If it opens, the export will succeed on a re-run;
if it does not, the underlying `AURA-RAW-*` or `AURA-RENDER-*` code is the one to work.

## Where it comes from

PHASE-30. See `docs/adr/ADR-0061-delivery-learning-loop-and-release.md` and
`docs/plan/phases/PHASE-30-DELIVERY-INTEGRATIONS-LEARNING-LOOP.md`.
