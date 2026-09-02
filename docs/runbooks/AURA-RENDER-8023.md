# AURA-RENDER-8023 - The export destination is full or cannot be written to

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

AURA cannot write where this export was going. Free up space or choose somewhere else; what has already been written is listed and unharmed.

## What actually happened

The destination is full, is read-only, does not exist, or the process is not allowed to write to it.

Distinct from `AURA-RENDER-8022` on purpose: this is a place that cannot take the files, and that is
a different conversation from a place that took them and gave back something else.

## What to do

Free space, reconnect the drive, or choose another destination. Files already written and verified
are listed in the panel and are unharmed; re-running the job writes the rest.

## Where it comes from

PHASE-30. See `docs/adr/ADR-0061-delivery-learning-loop-and-release.md` and
`docs/plan/phases/PHASE-30-DELIVERY-INTEGRATIONS-LEARNING-LOOP.md`.
