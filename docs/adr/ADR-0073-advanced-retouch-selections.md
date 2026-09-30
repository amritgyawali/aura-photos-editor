# ADR-0073 — Advanced retouch selections

- Date: 2026-09-30
- Status: accepted

Extend `studio_retouch_v1` operations with an optional `selection` object containing
shape inversion, a linear gradient and a luminance range. Old edits omit the field
and retain their previous coverage and serialization. Frozen contracts are unchanged.

A gradient replaces the ellipse and cannot coexist with a painted mask. Its
normalized endpoints are projected in physical image coordinates to preserve
direction on non-square images. Smoothstep coverage runs from zero at the start
to one at the end. Inversion complements the shape first. A luminance range then
intersects it, using linear Rec.2020 luminance expressed in stops relative to 18%
gray. Limits are -16 to +16 EV, with 0 to 4 EV smooth falloff beyond the interval.
Black and extreme HDR values clamp to the endpoint stops. The range is evaluated
after earlier operations and before the current operation. Feather continues to
control ellipses and brush strokes; gradient transition width comes from its endpoints.

The shared CPU coverage path feeds all 24 native tools, previews and export. Tool
specific color affinity or spot detection may further restrict the effect. Gradient
and inverted healing require an explicit source, including small target anchors;
the local automatic donor search was designed for small ellipses. Invalid or
conflicting selections are rejected by native recipe validation and the UI.

`native_retouch_selection_preview` returns RGB bytes encoding authored coverage
directly, without color transforms or dithering. The CPU engine clones the recipe,
retains only preceding retouch operations, and omits subsequent sharpening,
geometry and decoration. Missing replacement IDs produce an error. The app checks
collection membership, and neither the command nor the engine writes history.
The existing serial preview queue prevents overlapping draft work and discards
obsolete results, including results from the previous photo/mask display mode.

The workspace offers gradient drawing, numeric endpoints, reverse direction,
outside-shape selection, brightness presets and numeric ranges. A grayscale mask
view supports zoom/pan and read-only pointer gestures. White means selected;
black means protected. It is the authored selection, not a semantic segmentation
or a visualization of final tool strength. Selecting the entire photo clears
gradient/inversion/brush geometry but retains brightness limits. Named tool
presets still omit selection geometry and source coordinates.

Validation covers physical gradient direction, complementary inversion, EV
falloff, black/HDR support, old serialization, input rejection, all-tool pixel
isolation and preview stack position. UI tests cover zoomed drawing, cancellation,
read-only mask gestures, mode isolation, invalid limits and saved payloads.
`scripts/test-retouch-selection.py` exercises five real portraits through the
desktop, including preview isolation, exact undo/redo, protected pixels, verified
PNG exports, original hashes and actual controls. Integration evidence does not
establish commercial retouch quality or Retouch4me/SkinFiner parity.

Verification on this Windows desktop: 551 UI tests, 29 focused native tests,
TypeScript/Vite and the final native desktop build passed. The endpoint-guide
correction also passed all 21 affected UI tests. Five portraits passed coverage,
protected-pixel, history, desktop-control and original-hash checks; all five PNGs
passed read-back verification and matched the full renderer exactly. Evidence:
`.work-checks/selection-review/results.json`, with masks, before/after images and
workspace screenshots. The script supports `--resume` and verifies existing
operation IDs/pixels before continuing an interrupted isolated collection.

The desktop process exited once while rendering the fifth portrait. Its output
and the checked Windows event logs supplied no cause. After restart, completed
operations and pixels were unchanged; the fifth portrait, exports and all five
UI cases passed. The cause remains unresolved, so this is not a stability/soak
certification. Existing strict-Clippy failures remain at 67 renderer and 3 recipe
findings, matching the previous run; see the competitor audit for details.
