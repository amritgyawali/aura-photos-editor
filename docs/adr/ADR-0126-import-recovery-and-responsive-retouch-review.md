# ADR-0126: Import recovery and responsive retouch review

Date: 2026-10-19

Status: Implemented; validation recorded in the accompanying improvement report.

## Context

The full-feature audit found a repeatable mixed-root import failure, a numeric
edit refusal that disabled the whole editor, and a timing failure in the large
retouch-history UI. These defects affect the basic import/edit/review workflow
even when individual image operators pass their tests.

The import regression reproduced `AURA-DB-3006` at the second selected file:
`UNIQUE constraint failed: photo_file.project_id, photo_file.content_hash,
photo_file.rel_path`. Every individually selected file had an empty relative
path. Byte-identical copies therefore collided across separate file roots.

## Decisions

1. Store newly selected individual files under their containing directory, with
   their filename as the relative path. Keep existing catalogue rows readable.
   Only a directory scan may mark unobserved siblings absent; an individual file
   selection is not evidence that the rest of its directory disappeared.
2. Discover all selected roots before reporting the total. Progress includes
   skipped and quarantined inputs, is cumulative across roots, and reaches the
   same event and polling surfaces. File inserts and their import counters commit
   together. Final counters, error code and terminal journal state commit together.
3. Retain terminal worker errors in the in-memory import watch, and read error
   and running state under the same lock. `ingest_progress` uses its existing
   typed error response on failure/cancellation. One-click processing therefore
   cannot proceed merely because a failed worker stopped running. Import warnings
   disable the UI's automatic handoff so partial results can be reviewed.
4. Validate whole-number controls before IPC. Keep invalid text correctable with
   an accessible field-specific error; preserve precise input in decimal controls.
   Distinguish write failures from preview failures. Reconcile authoritative
   recipe/history after a failed write and release the editor for another attempt.
5. Mount at most 24 saved retouch rows. Provide search, enabled/disabled filters,
   previous/next pages and a shortcut to the selected operation. Actions retain
   the operation ID and its original stack index; filtering does not change
   processing order. Page size is a UI control budget, not a processing limit.
6. Remove the second registration of `within_moment`.

## Consequences

No database migration, frozen wire-type change, renderer/model change or new
dependency is required. Original files remain read-only. Processing still uses
the complete saved retouch stack regardless of which page is visible.

The additional discovery pass retains file metadata for the selection in memory;
it does not decode or retain image pixels. Cancellation is checked between roots
and batches; this does not establish a 250 ms stop bound for an individual long
filesystem walk or hash batch. A filesystem failure can still stop a run, but it
must close the journal and surface a typed error.

This change addresses the audited scenarios. It does not certify all source-path
aliases, camera formats, hardware configurations or photographic quality. Exact
Evoto appearance and feature parity remain unproven. Evaluate image algorithms
with representative photographs and independent visual acceptance before making
those claims.

## Rollback

Revert the affected source changes and rebuild. No schema rollback or source-file
restoration is needed. Existing catalogue path readers support the directory plus
relative-file representation.
