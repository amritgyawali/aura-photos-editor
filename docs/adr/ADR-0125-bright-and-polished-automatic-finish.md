# ADR-0125: Bright and polished automatic portrait finish

Date: 2026-10-16

Status: Accepted

## Preference

The photographer selected “Bright and polished: lighter portraits with stronger
cleanup” after the initial natural-finish qualification in ADR-0124. This changes
the default automatic finishing style for new photographs.

## Implementation

Automatic retouch now uses 125% measured strength, face fill light, 35% smoothing,
70% tone evening and 30% light evening, with 90% pore texture retained. Body skin
receives moderately stronger smoothing and tone evening. The existing adaptive
planner continues to turn settings down for small faces, noise and directional
light. Existing deep blemish search remains enabled; freckles, moles, facial hair,
eyes and nose details retain their protections. No reshaping or makeup is added.

Fill light uses the existing editable dodge operation, within segmented face skin.
It tapers off above +1.5 stops relative to 18% grey with a 0.5-stop transition, to
avoid pushing existing bright skin highlights into clipping. It does not increase
the exposure of white clothes or the background. Missing reliable skin coverage
still means this operation is skipped.

The UI offers a named “Bright and polished” preset and starts new photographs with
the same settings. Existing partial saved settings continue to decode against the
old neutral tool defaults. Explicit natural preferences and manual retouch remain
owned by the photographer. The previous automatic natural default is recognised
as an earlier default, so an uncustomised photograph can adopt the new finish on
its next automatic pass.

## Verification

The audit is `.work-checks/bright-polished-2026-10-16`. Tests cover default
migration versus explicit/manual ownership, skin-matte and highlight protection,
saved preferences, preset selection and actual callback settings. A fresh native
50-photo run exports a separate set in `F:\AURA bright and polished - 50 photos - 2026-10-16`.
The audit compares actual full-resolution JPEG face crops against the previous
natural exports, verifies all originals, export read-back, retouch pixels and
undo/redo. The result report records the measured finish differences and limits.

The 231-case natural-finish reference comparison from ADR-0124 remains evidence
for that earlier finish. It is not presented as a benchmark of this requested
brighter aesthetic.
