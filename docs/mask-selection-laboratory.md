# Selection laboratory: 25 additional workflows

Open **Masks > Selection tools > Selection laboratory**. Choose a workflow,
configure its draft, then click **Apply laboratory workflow**. Drafts do not save
or alter the photo. The existing mask overlay shows the saved result. Undo
restores the previous selection and settings in one history step.

These 25 operations extend both previous releases. They are independent AURA
implementations, primarily AURA extensions to the selection-authoring category.
They do not recover Evoto's proprietary source, model weights or exact output.
No unsupported algorithm or model parity is claimed.

| # | New workflow | Distinction and limits |
| --- | --- | --- |
| 1 | Polygon marquee | Author a closed selection from 3–128 vertices; four subsamples antialias each cell. This does not require an object detector. |
| 2 | Rounded-rectangle marquee | Author explicit bounds and rounded corners. This creates geometry rather than detecting an object within a box. |
| 3 | Angular-sector marquee | Author an elliptical angular wedge using bounds, start angle and sweep. This is a new shape primitive. |
| 4 | Sampled global colour | Select source RGB distance from one sampled cell across the full frame; soft tolerance falloff. |
| 5 | Contiguous colour wand | Flood-fill four-connected pixels within RGB tolerance of a sampled seed; disconnected matching colours are excluded. |
| 6 | Sampled luminance band | Measure mean brightness in a selected source patch and derive a soft band. Earlier luminance editors only stored manually chosen stop limits; this workflow measures a patch. |
| 7 | Saturation-range selection | Select an interval of source HSV saturation, with a soft transition outside it. This is coverage authoring rather than an HSL adjustment. |
| 8 | Texture-range selection | Select source local luminance variation, defined as twice its local standard deviation. Radius up to eight cells. |
| 9 | Image-edge selection | Select Sobel image luminance gradients. Earlier boundary-band selection measured the mask contour, not the image. |
| 10 | Image-guided mask smoothing | Bilateral coverage smoothing uses spatial and source-RGB weights. Image edges resist cross-boundary smoothing. Radius up to eight cells. |
| 11 | Edge-constrained selection growth | Grow along a bounded four-neighbor graph only where source colour differences and image gradients permit it. This differs from unrestricted dilation. |
| 12 | Interior distance falloff | Compute a four-neighbor interior distance field and use it as an inward alpha ramp. This does not blur outside the selection. |
| 13 | Enclosed negative-space selection | Extract only enclosed unselected holes, excluding original selection and frame-connected background. Earlier hole filling retained and enlarged the original selection. |
| 14 | Selection centreline | Apply connectivity-preserving Zhang–Suen thinning to a binary selection. This extracts a skeleton, not a contour band. |
| 15 | Split disconnected regions into masks | Create independent masks for at least two qualifying connected regions, with local adjustments and extended colour copied. Original mask retained; total remains bounded to 32 masks. |
| 16 | Append whole component stack | Append all source components to another mask in their original order and modes. Earlier transfer copied only one component. |
| 17 | Replace destination selection | Replace another mask's entire component stack while retaining its name, enabled state, amount and adjustments. |
| 18 | Synchronize local adjustments | Copy sliders, amount and extended colour to another mask, preserving destination selection and enabled state. Source absence clears destination extended colour. |
| 19 | Align selection centroids | Translate source coverage to another mask's measured alpha-weighted centroid. Both must be nonempty; frame clipping is explicit. |
| 20 | Fit selection to bounds | Map the measured 50% selection bounds to an explicit target rectangle. This does not crop or resize the photograph. |
| 21 | Four-corner perspective placement | Use a solved homography to map full-frame selection coordinates into four clockwise convex corners. Invalid/degenerate corners are refused. |
| 22 | Automatic coverage threshold | Otsu between-class variance chooses a binary threshold from the actual coverage histogram. This is not a fixed edge-contrast setting. |
| 23 | Sampled chroma-key exclusion | Remove sample-like chromaticity from existing coverage independently of intensity. Dark samples are unstable; review the overlay. |
| 24 | Seed partition of selection | Keep the existing coverage nearest one of multiple explicit seeds using a Voronoi partition. This does not infer separate people. |
| 25 | Filter regions by shape and area | Retain connected regions within an area interval and with sufficient 4π area/perimeter² compactness. This combines size and shape criteria, unlike removing only small islands. |

The three new marquee primitives replace the selected mask's composed coverage.
Source-based selectors also replace coverage, except chroma exclusion, guided
smoothing and edge-constrained growth, which operate on existing coverage. Stack
append, selection replacement and adjustment sync explicitly name a destination;
centroid alignment moves the current selection toward that destination's centre.
Splitting retains the original mask and adds independent masks. Enabled overlapping
adjustments can strengthen the photo; review the enabled masks after splitting.

Photo-derived operations read the **unadjusted sRGB source preview**, resampled with
a triangle filter to the selection grid. They do not sample the final exported
image or claim scene-linear/sensor measurements. Brightness uses weighted sRGB
values, not physical luminance. Colour tolerance is normalized Euclidean RGB
distance. Chroma exclusion compares normalized channel ratios. Texture and edge
thresholds use the normalized measurement domains described above.

Coordinates refer to the full uncropped photo: `(0,0)` is top left and `(1,1)`
bottom right. The coordinate diagram previews only authored points, not image
pixels. Coverage uses the existing maximum 512-cell grid and 16-level matte
encoding. Perspective and bound fitting use nearest-cell sampling. Centroid
translation reuses bilinear placement. Frame-external selection coverage is
clipped; source photos are never transformed by these workflows.

Topology uses a 50% threshold and four-connected neighbors. Centreline extraction
preserves the outermost grid boundary. Distances use the four-neighbor Manhattan
metric; they are not Euclidean physical pixel distances. Seed-partition ties use
input seed order. Automatic thresholding creates hard coverage and does not infer
an object class. No tool falls back to selecting a whole photograph when sampling
or detection fails. Pixel-derived empty results and empty growth seeds are refused;
negative-space extraction and shape filtering can intentionally yield empty coverage.

Every command reuses membership, cancellation and expected-recipe checks, plus
transactional `save_if_current`. Validation limits finite coordinates, at most
128 vertices, bounded radii/areas, valid component and matte counts. The existing
keys `studio_masks_v1`, `studio_masks_mattes_v1` and `studio_color_v1` retain
state; unrelated manual edits are preserved. No new frozen contract or command
registration is introduced. The `mask_workflow` IPC accepts defaulted optional
laboratory parameters, preserving older callers.

Native tests in `mask_workflows/tests/laboratory.rs` establish actual shape,
source-pixel and topology results, input rejection, and all 25 save/stale-write/
Undo/Redo workflows. The existing fixture verifies original bytes after the full
loop. `MaskLabControls.test.tsx` exercises all 25 distinct UI requests, drafts,
destinations, coordinate editing and validation. Synthetic tests verify mechanics;
real-photo review and paired Evoto output quality remain separate qualifications.
