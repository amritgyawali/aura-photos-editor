# AURA-RENDER-8021 - The export job was refused before anything was written

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

AURA cannot run this export as it is set up. Nothing has been written. Check the sets, sizes and file names, then try again.

## What actually happened

`ExportJob::validate` refused the job. It runs **before** a single frame is rendered, so nothing
has been written and nothing needs cleaning up.

The refusals, in the order the validator checks them:

* the job has no sets, or more than `MAX_SETS`;
* two sets share a name - their files would land on top of each other, the collision suffix would
  hide it, and the manifest would report both as delivered;
* a set has no images, or a name that is empty or longer than `MAX_SET_NAME`;
* a JPEG quality outside `MIN_JPEG_QUALITY..=MAX_JPEG_QUALITY`;
* sixteen bits per sample on a format that cannot carry them;
* a resize dimension outside `MIN_LONG_EDGE..=MAX_LONG_EDGE`;
* a naming template that is empty, too long, contains a path separator, or names a token this
  build does not have;
* more than `MAX_KEYWORDS` keywords, or a blank one.

## What to do

The message names which one. Fix it in the export dialog and run the job again. A template that
contains a `/` is the most common: a naming template names a file, and the folder structure is the
destination's own.

## Where it comes from

PHASE-30. See `docs/adr/ADR-0061-delivery-learning-loop-and-release.md` and
`docs/plan/phases/PHASE-30-DELIVERY-INTEGRATIONS-LEARNING-LOOP.md`.
