# Photo editor implementation progress

This tracks implementation after the [100-feature comparison](photo-editor-feature-roadmap.md).
The comparison describes baseline `a69b236`; it is not a claim that AURA implements all
100 feature families or matches the ten products in quality.

## Advanced retouch selections

All 24 native tools now accept linear gradients, inverted shapes and luminance
ranges with adjustable falloff. A disposable grayscale preview evaluates the
authored selection before the selected operation, preserving the saved recipe.
The UI supplies numeric endpoints, reversal and tonal presets as well as drawing.
Preview and export share the same coverage implementation. These are manual
selection refinements, not automatic face segmentation. See [ADR-0073](adr/ADR-0073-advanced-retouch-selections.md)
and the [user guide](native-retouch.md).

Verification: 551 UI tests, 29 focused native tests, TypeScript/Vite and the final
native desktop build passed. Five real portraits passed mask preview isolation,
protected pixels, exact undo/redo and desktop controls; five verified PNGs matched
the full renderer and original download hashes remained unchanged. Evidence is
in `.work-checks/selection-review/`. One unexplained desktop exit interrupted the
first run; restart verification preserved completed operation IDs and pixels,
and the resumed workflow passed. The existing strict-Clippy baseline remains red
(67 renderer and 3 recipe findings). This is a completed selection upgrade, not
completion of the entire 100-feature roadmap.

## Sampled skin processing

Three additional native tools provide sample-guided skin smoothing, color
uniformity and dodge/burn. They share a color-range selection intersected with
the photographer's ellipse or brush mask. The smoothing filter protects edges
and retains the fine residual by default; uniformity preserves luminance; the
dodge/burn correction is bounded and preserves RGB proportions. Named presets
include tolerance and edge protection without copying source coordinates.

This is not automatic face segmentation or exact Retouch4me/SkinFiner parity.
See the [behavior audit](retouch-competitor-audit.md), [user guide](native-retouch.md)
and [ADR-0070](adr/ADR-0070-sampled-skin-processing.md).

Verification: 540 UI tests across 57 files, 18 native tests, TypeScript and the
production UI build passed. The native desktop build passed with the local
debug-information workaround recorded in the audit. Five real portraits passed
all three tool previews, non-persistent draft checks, exact undo/redo, original
hash checks, and pixel-identical full-render/PNG export comparison. Missing
samples were rejected without changing history. The actual desktop sample
controls, full-photo targeting, unsaved preview, comparison and return to Develop
passed. Results and screenshots are in `.work-checks/sampled-skin-review/`;
`scripts/test-sampled-skin.py` reproduces the workflow and supports `--resume`.

## Precision retouch batch

The native workspace now shares painted masks, erasing, pressure-sensitive brush
radius, zoom/pan and keyboard controls across all 20 tools. Brush coverage uses the
same Rust rasterizer in previews and exports. Erasing protects the removed area,
and sparse pointer input produces continuous strokes.

Unsaved previews render temporary recipes without writing history. The UI
debounces requests, runs them sequentially, and ignores obsolete results. Saved
operations can be duplicated and reordered through normal undoable history.
Named local presets reuse settings without carrying masks or donor coordinates
between photographs. Unsaved drafts block collection navigation until applied
or discarded.

The UI suite passed 538 tests across 57 files; eleven native rendering tests passed,
including mask continuity, pressure, erasure across all tools, low-opacity
automatic spot cleanup and old-recipe compatibility. Production UI and native
desktop builds passed. Five real portraits passed draft-history isolation,
painted-region and erased-pixel checks, and exact undo/redo. All five PNG exports
passed read-back verification and matched the full renderer pixel for pixel.
Source SHA-256 hashes still match the original downloads. Four completed photos
retained identical operation IDs and pixels after restarting the desktop to
resume an interrupted run. Operation duplication/reordering and actual pointer
drawing at zoom with an unsaved preview also passed.

Evidence is in `.work-checks/precision-retouch-review/results.json` and its
adjacent screenshot and before/after images. See [ADR-0069](adr/ADR-0069-precision-retouch-authoring.md), the
[tool guide](native-retouch.md) and `scripts/test-precision-retouch.py`.

## Native retouch foundation

**Photo Studio → Develop → Retouch** now provides 20 local workflows: sampled
healing/cloning, selected-region spot detection, frequency tone/texture controls,
micro dodge and burn, dodge/burn, skin color and sampled color matching, shine
reduction, under-eye lifting, fine-line softening, teeth and eye adjustments,
fabric/backdrop smoothing, glare attenuation and cosmetic tinting.

Each operation has a feathered elliptical target and saved parameters. The stack
supports update, disable, remove, clear, durable undo/redo, before/after and two
three-operation skin presets. The CPU preview and export pipelines render the
same recipe extension before crop/perspective. Collection editing blocks retouch
writes; external recipe revisions refresh the workspace.

These are original deterministic AURA algorithms, with shared primitives across
several named workflows. They do not reproduce Retouch4me's proprietary models
or establish commercial quality parity. Automatic face/skin targeting, layered frequency editing and reconstruction
of obscured detail remain
unimplemented in this workspace. See the [tool guide](native-retouch.md) and
[ADR-0068](adr/ADR-0068-native-retouch-workspace.md).

Verification includes six focused native tests, the production UI/desktop build,
529 UI tests across 54 files, and five real portrait history/export checks. Five PNG
exports exactly matched the full renderer; original hashes were unchanged.
After restarting the final desktop build, all five photographs retained the same
operation IDs and rendered pixels. The native interface passed tool selection,
before/after comparison and return-to-Develop checks.
The repeatable workflow is `scripts/test-native-retouch.py`; this run's evidence
is under `.work-checks/native-retouch-review/`.

## Studio authoring batch

| Roadmap feature | Implemented behavior | Remaining scope |
| --- | --- | --- |
| 12 — White balance | Click an original-image neutral patch or enter normalized coordinates; fit temperature/tint with the real renderer; one protected manual history entry | Gray-card/color-chart calibration and camera-specific illuminant fitting |
| 19 — Clipping overlay | Independent highlight/shadow modes, spatial hatch patterns, RGB channel clipping, comparison support | Sensor-level RAW clipping analysis |
| 82 — Selective sync | Nine settings groups, selected photos or collection, crop opt-in, target deduplication, collection membership checks, per-target failure reporting | Clipboard copy/paste and transactional all-or-nothing batches |
| 85 — Named snapshots | Save named versions, reject duplicate names, restore through durable undoable history | Independent virtual-copy library entries |
| 94 — Watermarks | Text or PNG graphic, color, opacity, relative size, margin, five anchors; native linear-light alpha blending after output resize/sharpening; 8-/16-bit and sRGB/Adobe RGB/P3 support | Saved watermark presets and live photo placement preview |

Find the first four tools in **Photo Studio → Develop**; clipping warnings are beneath
the photograph. Find watermarks in the export panel. All processing is local.

Settings sync copies optical correction switches but retains target camera/lens profiles.
White-balance coordinates refer to the oriented original, before crop/geometry edits.
Watermark text is rasterized using installed system fonts; the actual bitmap and settings
participate in the delivered render hash. Originals and saved recipes are unchanged by export.
The exact watermark bitmap/settings are archived beside the output and referenced by name
and verified BLAKE3 in the manifest, so they remain available after closing the editor.

## Studio authoring verification

- UI suite: 522 tests across 52 files passed.
- Final focused UI run: 10 tests passed, including two additional watermark tests.
- Production TypeScript/Vite build passed.
- Native desktop compile check passed.
- Six focused Rust tests passed against the desktop's exact compiled dependency graph:
  white-balance recovery/rejection and watermark blending, depth, color spaces, validation,
  archive reuse and refusal to overwrite a mismatched existing archive.
- Five real portraits passed native enhancement/history, picker undo/redo, snapshot restore,
  selective-sync isolation, target deduplication and collection membership validation.
- Five plain PNG exports matched the full renderer exactly. Five watermarked PNG exports
  changed only the watermark region. All ten passed export read-back verification.
- After a native application restart, named snapshots remained available. Five text-watermarked
  JPEGs were exported through the interface, verified and recorded in a versioned manifest.
- Original file hashes were unchanged. The final UI check passed after narrowing a test's
  Calibration selector to distinguish the section heading from the new sync checkbox.

The reproducible native workflow is `scripts/test-portrait-studio.py`. It imports five
real JPEG portraits into a separate collection, verifies enhancement/history, exercises
the new commands, compares full-resolution exports, checks watermark bounds, and hashes
the original files before and after. Generated evidence stays under `.work-checks/`.
This run's machine-readable results are in `.work-checks/studio-tools-review/results.json`;
plain outputs, watermark proofs and interface screenshots are alongside them.

## Texture-aware patch repair

The native Retouch workspace now includes a separate texture-aware patch heal
operation. Small ellipses can search nearby texture automatically; painted or
larger repairs use a photographer-selected source. Approximate harmonic tone
blending matches surrounding light while transferring donor detail. Old Heal
operations retain their rendering behavior.

The [native retouch guide](native-retouch.md) describes the controls and limits;
[ADR-0071](adr/ADR-0071-texture-aware-patch-heal.md) records the independent
algorithm. The five-portrait verification script accepts `--workflow patch-heal`.
This feature does not provide semantic blemish detection or demonstrate exact
Retouch4me/SkinFiner quality parity.

## Work still required for the full request

All 100 features are **not implemented**. This batch expands the existing editor without
pretending that missing models or document architecture exist. The following remain
separate substantial implementations:

- Complete brush/gradient authoring and mask edge refinement; validate subject/face/sky models.
- Layers, adjustment layers, blend modes, editable text/vector objects, linked assets and
  layered-document interchange with versioned persistence and rendering.
- HDR/panorama/focus-stack alignment, merging, deghosting and failure handling.
- Model-backed denoising, super-resolution, replacement and generative operations with
  explicit model availability, versioning and measurable quality acceptance.
- Broader camera/RAW/ICC support, tethering, soft proofing, print/contact-sheet output,
  GPU coverage, HDR display/export, LUT import, macros and plug-in APIs.

Feature-by-feature baseline evidence and implementation techniques remain in the linked
100-row comparison. See [ADR-0067](adr/ADR-0067-studio-authoring-and-export-watermarks.md)
for this batch's persistence, color and compatibility decisions.
