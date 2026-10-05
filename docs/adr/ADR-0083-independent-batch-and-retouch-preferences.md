# ADR-0083: Independent batch editing and restored retouch preferences

Status: Accepted
Date: 2026-10-05

## Context

The editor already measures every image independently and stores automatic retouch options
in `studio_portrait_auto_v1.options`. The retouch panel nevertheless reopened with defaults,
so applying a small adjustment could replace a photo's previous scope and fine controls.
Batch editing offered the entire project and did not expose saved settings or targeted retry.
Stopping between steps could also omit the partially edited photo from the outcome list.

## Decision

- Restore known options from the selected photo's recipe. Fill missing settings with defaults,
  validate types, bound numerical values and create fresh objects for each photo. Remount the
  controls when the photo or saved recipe hash changes, including undo and redo.
- Keep using the existing per-photo native analysis and saved preference handling. A batch
  selection contains photo IDs, never a shared replacement recipe. An omitted selection means
  the full project; an empty selection means no work. Deduplicate paginated photo IDs.
- Search the full collection but initially render only 120 picker rows. Selecting search
  matches includes matches beyond the visible rows. Preserve selections when filtering.
- Mark success only after the saved recipe and rendered preview both return. Drain both
  requests on failure before starting another image. Report interrupted work as retryable;
  stopping does not roll back edits already saved. Retrying replaces only failed outcomes.
- Build result summaries from saved recipe values, not proposed adjustments that may have
  been blocked by manual protection.
- Plan brightness selections against the exposure that survives the recipe merge. Apply
  automatic dehaze only to measured landscapes; a bright studio backdrop is not haze.
- Include the existing refine group as its own reported and undoable history step.

## Validation and limits

UI regressions cover photo selection, pagination, cancellation, targeted retries, draining
failed reads, distinct saved settings, invalid extensions, photo switches and undo. Native
tests cover manual exposure, retouch-only exposure and bright studio/product frames alongside
the existing scene, noise, skin and manual-edit protection tests.

The picker and batch runner use the existing catalog API and require no schema or IPC change.
The picker still reads collection metadata into memory; only rendered rows are bounded.
Advanced project-wide model analysis remains a separate workflow. Stopping waits for the
current native operation; it cannot interrupt that operation midway. Scene recognition and
blemish detection remain heuristics requiring visual review, particularly on unusual lighting
or permanent facial marks. Automated tests do not establish expert-level quality on every photo.

## Import workflow follow-up

The native selection-to-export workflow now invokes `enhance_portrait` after each successful
cloud or local grade, before delivery. This is the same bundled retoucher used by the editor;
it does not depend on optional autopilot analysis being available. Running after the grade
preserves the chosen global settings and gives brightness masks the exposure that will render.
The returned recipe replaces the earlier grade in `photo-edits.json`, so the report describes
what is actually exported. A retouch failure increments failed edits, records a visible note,
and leaves the last saved recipe available for export while other photographs continue.
Cancellation is checked both before and after this native pass. The existing device-level
portrait disable switch remains honored and is recorded inside the photo's portrait report.

The offline delivery regression verifies that every frame receives a portrait assessment,
different saved scopes stay separate, manual exposure and originals remain unchanged,
no portrait operations are created for abstract frames, and report hashes match saved recipes.
