# ADR-0072 — Retouch comparison and draft protection

- Date: 2026-09-30
- Status: accepted

Extend the native retouch workspace with an aligned split comparison. The left
image is the existing before-native-retouch preview; the right is the current
saved or valid unsaved preview. Both images use the same photo surface, dimensions,
zoom and scroll position. A CSS clip exposes the before image. Moving the divider
does not render again, write a recipe or change exported pixels. Mismatched
preview dimensions disable comparison. Both views remain before crop/perspective,
and zoom percentages continue to refer to preview pixels.

A pointer-captured divider supports dragging, with a native range input providing
keyboard and assistive-technology access. The input reports each side's percentage.
Comparison hides selection guides and makes photo gestures pan-only, preventing
accidental paint strokes or source changes during inspection. Switching split comparison
cancels an unfinished pointer gesture. The existing full-before toggle stays
available, and unsaved previews can also be compared.

Previously, selecting another saved operation or mutating the stack/history could
clear the dirty flag or replace the draft without preserving its edits. Those
controls now disable while dirty, and the shared mutation entry point rejects
history/stack actions until Apply or Discard. Keyboard undo may still remove a
draft brush stroke; it cannot silently switch to saved history while dirty.
Changing tools, starting another operation or applying quick skin presets while
refining a dirty saved operation is also blocked. Explicit draft setting changes,
Apply, Discard, and quick presets for a new operation remain available.

Drafts remain in-memory and are not crash-recovery storage. This change protects
workspace transitions; it does not introduce autosave or a new persistence format.
No native processing algorithm, IPC contract, model or export setting changes.

Validation covers slider endpoints, pointer coordinates under zoom, read-only
comparison gestures, mismatched preview dimensions, render-call isolation and
dirty draft preservation. A desktop script reuses five real portraits to check
image alignment, keyboard/pointer controls, disposable preview comparison and
unchanged saved operations, history and original file hashes.

Verification on this Windows desktop: 546 UI tests across 57 files passed, as did
TypeScript/Vite and the native desktop build using the previously documented
local debug-information overrides. All five portraits passed the desktop script;
original hashes, saved operations and history remained unchanged. The initial
script run stopped at Playwright's default five-second assertion timeout during
native preview loading. Setting its assertion timeout to 60 seconds allowed the
complete rerun to pass. No product timeout or renderer behavior was changed.
Evidence: `.work-checks/retouch-comparison-review/results.json` and five workspace
screenshots. This validates the review workflow, not commercial retouch quality.
