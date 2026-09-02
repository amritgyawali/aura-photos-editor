# AURA-DLV-10003 - A backup copy did not match the file it was made from

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

A backup copy came back different from the original, so the backup was stopped. Check that drive before trusting anything on it.

## What actually happened

A file copied to a backup destination was read back and its digest did not match the source.

The same shape as `AURA-RENDER-8022` and the same response: **halt**. A backup that silently
contains a different file from the original is worse than no backup, because somebody will restore
from it.

## What to do

Stop using that destination until its drive has been checked. `delivery_backup` rows record which
file diverged; the source in the export folder is the one to trust.

## Where it comes from

PHASE-30. See `docs/adr/ADR-0061-delivery-learning-loop-and-release.md` and
`docs/plan/phases/PHASE-30-DELIVERY-INTEGRATIONS-LEARNING-LOOP.md`.
