# AURA-REL-12003 - The feature-flag file was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

AURA could not read the settings that say which parts of it are switched on, so it has not started. Restore the file or reinstall.

## What actually happened

`ops/flags/flags.toml` could not be read, could not be parsed, or asked for something the code does
not permit.

The application **halts** rather than falling back on defaults. A feature-flag file is the kill
switch for every AI stage, and a build that could not read it and started anyway would be a build
running stages somebody had switched off.

## What to do

Restore the file or reinstall. The shipped copy is in the installation directory; a studio's
overrides live beside it and may only *disable* a stage, never enable one the build does not have.

## Where it comes from

PHASE-30. See `docs/adr/ADR-0061-delivery-learning-loop-and-release.md` and
`docs/plan/phases/PHASE-30-DELIVERY-INTEGRATIONS-LEARNING-LOOP.md`.
