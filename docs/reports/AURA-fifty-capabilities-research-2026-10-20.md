# Fifty-capability continuation: source audit

Date: 2026-10-20. This is a candidate and provenance ledger, not an implementation
or Evoto-equivalence certificate. The final implementation report must enumerate
only capabilities exercised through real native rendering or durable workflows.

## Sources and existing functionality

The installed-app and native evidence is retained in the
[Evoto workflow audit](AURA-Evoto-workflows-2026-10-20.md) and
[native research report](AURA-Evoto-native-research-2026-10-20.md).
The following official desktop pages were checked again on 2026-10-20:

- [Colour controls](https://support.evoto.ai/color-adjustment-feature-modules/):
  Levels, Selective Color, point/area sampling, on-image adjustments and perspective guides.
- [Masking](https://support.evoto.ai/feature-introduction-masking/):
  spatial editing, edge refinement, local colour, luminance selection and sync.
- [Reference matching](https://support.evoto.ai/feature-introduction-ai-color-match/):
  reference management, grouped batch actions and linked regional strengths.
- [Crop](https://support.evoto.ai/crop/):
  dimensions, physical output sizes, framing constraints and person alignment.
- [Clothing](https://support.evoto.ai/clothing-accessories-adjustments-module/)
  and [hair](https://support.evoto.ai/hair/): material-specific adjustments.

AURA already contains B&W mixing, HSL, RGB curves, grading, grain, calibration,
vignetting, many skin/body settings, makeup tinting, relighting, catchlights,
42 precision-retouch tools, local masks and independent regional matching.
Its advanced clothing stage already measures and heals lint/thread/stain marks.
Those are not new additions in this continuation. A field or primitive that
exists only in native code can support a new complete workflow, but must be
described as newly exposed workflow rather than a newly invented algorithm.

## Candidate gaps

`C` refers to colour documentation, `M` masking, `R` reference matching,
`P` crop, `T` material controls. These are useful missing operations or workflows;
they are not a promise that every candidate is small enough to implement here.

| # | Candidate | Evidence | Current basis / missing work |
|---|---|---|---|
| 1 | Input/output Levels with gamma | C | No Levels renderer/schema/UI found |
| 2 | Relative/absolute CMYK Selective Color | C | No Selective Color renderer/schema/UI found |
| 3 | RGB/Lab point inspector | C | Add colour-managed numerical inspection |
| 4 | Colour-only comparison preview | C | Existing original/split compare includes other edits |
| 5 | Luminance-preserving luma curve | C | Already exists in AURA; excluded. The implementation adds luminosity Levels as an AURA extension instead |
| 6 | On-image curve targeting | C | Existing curve editor needs pixel-to-curve selection |
| 7 | On-image HSL adjustment | C | A coordinate-selected colour patch now targets an HSL band; dragging directly on the image remains unimplemented |
| 8 | Area white-balance sampling | C | Existing picker measures fixed seven-pixel neighborhood |
| 9 | Perspective correction from drawn guides | C | Existing transform sliders are not guide solving |
| 10 | Mask-local curves | M | Existing local parameters omit curves |
| 11 | Mask-local HSL | M | Existing local parameters omit HSL |
| 12 | Mask-local colour grading | M | Existing local parameters omit grading |
| 13 | Mask-local sharpening | M | Existing local texture/clarity differ from sharpening |
| 14 | Invert a composite mask | M | Inverting individual components is not complementing the result |
| 15 | Move an existing mask | M | Creation geometry exists; complete edit workflow missing |
| 16 | Resize/rotate an existing radial mask | M | Native source has geometry; UI lacks editor |
| 17 | Reposition/rotate an existing linear mask | M | Native source has endpoints; UI lacks editor |
| 18 | Smooth completed selection edges | M | No composite morphology settings |
| 19 | Expand completed selection | M | No composite dilation |
| 20 | Contract completed selection | M | No composite erosion |
| 21 | Feather a completed selection | M | Existing brush feather differs from composite feather |
| 22 | Sample luminance range from photo | M | Existing ranges are numeric/bright/dark shortcuts |
| 23 | Luminance-map inspection | M | Existing coverage overlay is a different view |
| 24 | Append/replace mask synchronization | M | Complete explicit merge-mode workflow needed |
| 25 | Individual-person selection workflow | M | Native `person` exists; picker omits individual people |
| 26 | Hover-preview a mask's coverage | M | Existing overlay follows selected mask only |
| 27 | Durable empty reference groups | R | Existing groups derive from member strings |
| 28 | Rename a reference group and its members | R | Existing metadata update changes one reference |
| 29 | Remove a group while retaining references | R | Needs explicit ungroup behavior |
| 30 | Manually reorder reference groups | R | Existing groups are alphabetically sorted |
| 31 | Move selected references together | R | Existing move is per-reference metadata |
| 32 | Remove selected references together | R | Existing remove is per-reference |
| 33 | Link regional matching strengths | R | Existing controls are independent only |
| 34 | Export/import a portable reference catalogue | AURA extension | Descriptors and metadata need validation/versioning; no proprietary interchange claim |
| 35 | Interactive free crop handles | P | Existing Studio controls expose centered ratio buttons |
| 36 | Enter a custom crop ratio | P | Existing fixed ratios only in Studio |
| 37 | Exact width/height/resolution crop workflow | P | Needs coupled geometry/output intent |
| 38 | Physical-size ID-photo workflow | P | Needs units and PPI; no official ID-compliance claim |
| 39 | Durable custom crop presets | P | Existing broad edit presets are different |
| 40 | Subject-based crop positioning | P | Existing plan primitives need direct authoring workflow |
| 41 | Face-based crop positioning | P | Needs measured face anchor and constraints |
| 42 | Head/headwear framing coverage | P | Hair/headwear support must be measured honestly |
| 43 | Joint margin/head-size/horizontal-position constraints | P | One framing tool, not three inflated feature counts |
| 44 | Eye-level crop alignment | P | Requires supported eye positions and refusal |
| 45 | Subject-axis vertical alignment | P | Requires supported geometry, not face-box guessing |
| 46 | Editable crop rules by source orientation | P | Album variants already follow orientation; expose genuinely new photographer rules |
| 47 | Independent fine/coarse fabric corrections | T | Existing fabric smoothing does not expose independent bands |
| 48 | Independent upper/lower garment correction | T | Requires valid garment localization |
| 49 | White-hair darkening with retained chroma/detail | T | Existing general recolouring is a different operation |
| 50 | Directional hair smoothing | T | Existing hair-detail controls do not establish directional smoothing |

Background banding removal and luminance moire suppression are additional
material candidates already observed in prior audit families. They must not be
counted as implemented because a chroma-only moire tool or backdrop blur exists.

## Reference-workflow count discipline

The reference source supports group creation/rename/removal/order and batch
reference move/removal. Empty groups require independent catalogue records.
Manual reference ordering, portable descriptor interchange, batch favorites,
batch default-strength changes, and reset workflows are useful AURA extensions.
Do not call all of those observed Evoto controls, count selection widgets as
separate algorithms, or split each numerical slider/channel into a unique tool.

No competitor code, shaders, model weights or image assets are product inputs.
Independent methods must keep source files unchanged, validate bounded input,
preserve saved sessions/manual edits and have observable results.

## Implemented reference-library scope

The native implementation is `color_match/library/workflows.rs`; the manager is
`ui/src/components/look/ReferenceLibraryManager.tsx`. Its twelve actions are:

| Action | Provenance |
|---|---|
| Create a durable empty reference group | Documented Evoto group management |
| Rename a group and its references together | Documented Evoto group management |
| Delete a group, retaining its references in Ungrouped | Documented group deletion; safe AURA retention policy |
| Reorder named groups, keeping Ungrouped first | Documented Evoto group management |
| Move selected references in one transaction | Documented Evoto batch management |
| Remove selected references in one transaction | Documented Evoto batch management |
| Save per-group reference order | AURA extension; manual group ordering alone does not prove Evoto reference ordering |
| Favorite/unfavorite selected references together | AURA batch-workflow extension |
| Save matching defaults on selected references together | AURA batch-workflow extension |
| Reset selected matching defaults | AURA batch-workflow extension |
| Export a portable descriptor library | AURA interchange extension |
| Import a portable descriptor library | AURA interchange extension |

Selection widgets, the two favorite states, up/down buttons and the individual
matching sliders are not counted separately. Existing single-reference metadata,
favorites, removal and default-strength operations remain existing functionality.

Changes require a matching library revision and save atomically. Named groups
and member order survive reopening and are also used by the existing reference
picker. Removing library metadata never deletes source files or saved matching
sessions. The manager supports at most 128 references and 64 named groups, with a
16 MB stored-metadata budget.

Portable documents are limited to 1 MB and bind their schema version and colour
descriptor engine. They contain measured distributions, dimensions, names,
group order and defaults, excluding image pixels, thumbnails, sampled coordinates
and source-path fields. Imported references can drive colour matching but do not
create a browsable copy of the original image. Import merges into the existing
library; by default existing metadata wins. Explicit replacement updates metadata
only when the appearance descriptors are identical. Duplicate identifiers,
conflicting descriptors, unsupported versions, invalid dimensions, non-finite or
out-of-range measurements and excessive counts are refused without partial saves.

Five manager tests passed on 2026-10-20. Five native workflow tests cover
organization/reopening, preserved matching-session metadata, stale/invalid batch
atomicity, portable round trips and malformed/oversized/non-finite/conflicting
input, plus capacity admission that still permits updating existing references.
New raster imports/captures use one atomic SQLite admission statement at capacity;
portable imports check total capacity in their transaction, including older-engine
entries. Their final execution belongs to the combined native verification run;
this ledger does not predeclare them passed.
