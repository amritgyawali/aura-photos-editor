# ADR-0091: Protect eyes and nose detail through the complete pass and display saved coverage

Status: accepted. Date: 2026-10-06.

The acne portrait review found over-retouch around the eyes. The healing surface had an
eye exclusion, but face smoothing and the union used for texture restoration could still
select orbital skin. Protecting only the initial repair could not protect the final image.

## Decision

`protectEyeArea` and `protectNoseDetail` default to true, including for older saved options. After planning,
every newly generated automatic face operation intersects its own selection with the same
rotation-aware orbital exclusion. It covers eyelids, inner corners and tear-trough shadows,
with a smooth outward transition and a sampling margin for enlargement. Intersected mattes
disable color-guided edge expansion so skin-colored eyelids cannot re-enter at export size.
A second rotation-aware exclusion preserves the nose bridge, nostril edges, pores and
shading. Deep spot search also excludes these areas from both targets and donors. Existing authored
ellipses, brush strokes, luminance ranges and skin sampling restrictions still apply.

The guard creates separate matte IDs. It does not modify the source matte, manual operations,
body operations, or hair/clothing operations. Manual automatic-step overrides remain protected
by the existing override system. Dedicated eye controls can be used by explicitly disabling
the guard; while it is enabled the UI explains that it also limits those controls. Manual
retouch remains available. Existing recipes retain their pixels until retouch is run again.

`native_retouch_saved_selection` is a read-only, membership-checked command running off the
UI thread. The renderer prepares the normal working pixels and observes coverage in its
actual operation loop, using shared matte planes captured before the pass and the current
pixels for each brightness/sample restriction. Disabled/zero-strength operations contribute
nothing. Missing operation IDs reject the request; missing/corrupt stored masks do not broaden
coverage. The command supports the union of all saved operations or one specified operation.

The workspace adds **Show retouched areas**, a teal overlay, an operation selector, and an
overlay visibility slider. This displays selection eligibility, not a claim that every
selected pixel changed. It excludes unsaved drafts. Viewing, zooming and panning do not author
edits. Responses are keyed to the photo, recipe revision and operation; obsolete responses
cannot be drawn over another photo. Overlay strength is local display state only.

On photo or recipe reload, the workspace clears the saved preview, recipe, operation
list and history before requesting replacements. A failed reload leaves editing and
history actions disabled instead of displaying stale coverage. Reload and navigation
remain available. The recovery regression simulates a native preview failure during
undo, reloads, then redoes and checks the restored recipe hash (2026-10-08).

The **Acne only: preserve detail** preset disables broad smoothing, color and light
evening, shine changes, lines, eye/teeth and hair finishing. It uses frequency healing
and a bounded residual spot pass. Residual repairs after frequency healing use partial
strength and a wider feather to retain some original detail and soften circular patch
boundaries. It does not request a global edit.

## Limits

Five facial landmarks estimate the orbital region; this is not precise eyelid segmentation.
Conservative protection may leave acne close to the eye for a manual repair. It cannot promise
perfect arbitrary-photo retouching. Preview coverage is evaluated at preview resolution;
adaptive tool detections can differ in a full-size export. Body retouch and explicit manual
edits are independent of the face guard.

## Regression checks

- Eye centers, inner corners and lower-lid skin remain byte-identical through a strong native
  correction at two resolutions; cheeks remain editable. The guard rotates with landmarks.
- Protected mattes cannot widen an input selection, mutate its source, or silently accept
  a missing mask.
- Saved coverage follows preceding exposure edits, excludes disabled steps and matte holes,
  rejects missing IDs and leaves the recipe unchanged.
- UI tests cover read-only viewing, operation selection, stale responses after photo/history
  changes, backend errors, and tinting only covered pixels.

Desktop image inspection and run results are recorded in the session validation report.
