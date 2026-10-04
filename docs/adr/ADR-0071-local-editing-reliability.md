# ADR-0071: Reliable local import completion and edit navigation

Status: accepted for the local photo workflow.

## Context

Testing five real portraits revealed three integration failures: one import plan tried
to insert its journal identifier for every selected file; a source identifier depended
only on its path despite sources belonging to projects; and the desktop never emitted
the completion event consumed by automatic editing. A persisted undo was also reloaded
as a new edit at the history head, making redo unavailable on the next command.

## Decision

Create one import journal row per plan, pointing to its first source. Individual file
rows keep their own source identifiers and the run accumulates counters across roots.
New source identifiers include the project ID; existing `(project_id, abs_path)` rows
are still found through the existing upsert. No catalog migration or original-file
operation is needed.

The application exposes an optional import-event callback. The shell publishes its
completion or failure through the existing `ingest` channel. Headless callers retain
the existing command signature. The terminal notification follows job cleanup.

The application records undo/redo as append-only journal rows with reserved
`$history.undo` / `$history.redo` entries in the history row's `changed` list. These
are navigation metadata, never recipe parameters, manual-protection fields or renderer
invalidation paths. The saved recipe remains the actual current recipe. The application
replays those markers when loading editable history, retaining a cursor and truncating
the redo branch only when a new edit arrives. Labels remain human-readable. Existing
rows remain readable and are treated as ordinary saved edits. If journal trimming has
removed a navigation target, the navigation row's saved recipe serves as a checkpoint.

## Consequences and verification

Visual review of the five portraits also showed that a fixed middle-gray exposure
target darkened normally exposed faces. Local auto enhancement now leaves the
display-lightness median band 0.45–0.78 at its existing exposure and caps negative
global corrections at 0.25 EV. Highlight/shadow corrections remain independent.
This is conservative histogram analysis, not face detection or an aesthetic score.

The frozen IPC DTOs and recipe schema stay unchanged. Other consumers of raw journal
rows must treat the reserved paths as navigation metadata. The existing history cap
still bounds storage. Older app versions can display the current saved recipe but do
not interpret the new navigation markers; they retain their old redo limitation.

Importer regression tests cover five selected roots, a single completed journal row,
cross-project reuse and reimport idempotence. The native portrait smoke test verifies
completion notification, repeatable enhancement, manual-field protection, successive
undo/redo commands, discarding redo after a new edit, reset, full-size export parity and
unchanged source hashes. Preview and numeric-input tests cover loading and failure states.
