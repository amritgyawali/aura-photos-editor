# Fifteen additional mask authoring and topology workflows

Date: 2026-10-20. These extend the earlier fifty-capability release. Open
**Masks > Selection tools**; authored geometry lives in **Authored components**.

These are independently implemented AURA operations, inspired by the documented
mask-authoring category. The additional topology operators are AURA extensions.
They are not recovered Evoto algorithms, pretrained models, or evidence of
identical Evoto output. No proprietary weights or tuning data are included.

| # | Workflow | Actual behavior |
| --- | --- | --- |
| 1 | Edit an existing linear gradient | Change its two stored control points without redrawing or replacing its combination. |
| 2 | Edit an existing radial gradient | Change centre, radii, angle and feather as one explicit geometry edit. |
| 3 | Edit authored brush strokes | Select a stored stroke and point; edit position, pressure, radius, opacity, eraser status and component feather. Other strokes stay intact. |
| 4 | Edit luminance range | Change low/high stop limits and transition softness; inverted limits are rejected. |
| 5 | Edit component combination | Switch an existing component between add, subtract and intersect. |
| 6 | Reorder component composition | Place a component at an explicit existing position; order matters for subtraction and intersection. |
| 7 | Extract an independent component mask | Make an independent mask from a component, retaining its inversion and adjustment settings. The source component stays present; the extracted component starts with Add. Extended local colour settings are copied. |
| 8 | Transfer a component to another mask | Append a copy to an explicit destination mask, preserving its combination and leaving both masks' adjustments independent. |
| 9 | Place a composed selection | Inverse-map translation, anisotropic scale and clockwise rotation with bilinear coverage sampling around the frame centre. |
| 10 | Select a boundary band | Compute the difference between dilated and eroded coverage using the selected cell radius. |
| 11 | Keep the largest connected region | Retain the largest four-connected region at 50% coverage, retaining that region's original alpha. Equal-size ties use frame scan order. |
| 12 | Isolate a seed-connected region | Enter normalized photo coordinates; keep only the four-connected selected region under that point. Unselected seeds are refused. |
| 13 | Median speckle cleanup | Apply a local median filter, independently suppressing isolated bright and dark outliers. Radius is limited to four cells. |
| 14 | Symmetric difference of saved selections | Absolute alpha difference preserves areas selected by one mask and removes their shared full coverage. Soft coverage follows this explicit alpha-difference rule. |
| 15 | Convex-hull selection | Build a monotone-chain convex hull of selected cell centres, filling its polygon. Fewer than three non-collinear selected cells leave the selection unchanged. |

The four source editors retain vector control points and brush pressure. Existing
AI mattes and measured portrait components can be reordered, copied, extracted,
or assigned a combination, but are not silently turned into editable gradients.

The seven composed-coverage operators use the existing full uncropped frame,
maximum 512-cell grid, and 16-level stored matte encoding. Coordinates use the
uncropped photo: `(0,0)` is top left and `(1,1)` bottom right. Rotation is defined
in normalized frame coordinates, so its apparent shape depends on the frame's
aspect ratio. Transformed coverage outside the frame is clipped. Connectivity
uses four neighbors, excludes coverage below 50%, and does not claim semantic
person identification. Convex hulls replace soft contours with a hard polygon.

Every command checks collection membership, cancellation and the expected recipe
hash before saving with `save_if_current`. It uses one history edit; Undo restores
the exact previous recipe. Mask sliders and unrelated manual fields are preserved.
Source geometry, component, brush-point, matte and mask-count bounds reuse the
existing recipe validator and keys (`studio_masks_v1`, `studio_masks_mattes_v1`).
Unreferenced mattes are pruned by the existing writer. No new frozen contract or
IPC command is introduced; `mask_workflow` accepts the additional actions and
optional authoring inputs.

Verification is in `mask_workflows/tests/mod.rs` and
`MaskAuthoringControls.test.tsx`. The native integration fixture runs all fifteen
through saving, stale-write rejection, exact single-step Undo and Redo, and checks
local settings and source bytes. Synthetic coverage tests establish actual
translation, boundary-band, median, connectivity, symmetric difference and hull
behavior. Frontend tests exercise all fifteen requests, actual draft edits,
pressure preservation, apply-only saving and disabled state. These tests are
mechanical validation; they do not establish Evoto visual parity or native
portrait cohort quality.
