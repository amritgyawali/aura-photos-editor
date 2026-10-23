# AURA material and light tools — 19 October 2026

Five new native image operations and searchable tool selection are implemented.
This is additional working capability toward the user's requested Evoto coverage.
Complete feature parity and superior photographic output are not established.

## Implemented tools

Open **Auto edit → select a photo → Retouch**. Search under **Find a retouch tool**.
Every tool has strength and radius controls, supports existing selections and
saved masks, and uses the normal persisted retouch stack and Undo/Redo.

| Tool | What it does | Controls and limits |
|---|---|---|
| Fabric moiré reduction | Filters colour ripples independently of luminance, preserving photographed luminance weave | Strength and radius; manually select the material. Real coloured patterns can also be reduced. Does not reconstruct luminance moiré. |
| Fabric stain correction | Matches broad channel ratios toward a sampled clean fabric patch while retaining local detail | Pick a clean source from the same fabric and lighting; correction amount, radius and strength. Channel correction is capped at one stop before blending. No automatic sweat-stain classifier. |
| Shadow balancing | Raises a smooth luminance field toward the selected surface's measured upper midtones | Maximum lift 0–2 stops, radius and strength; preserves RGB proportions and black pixels. Select one surface and exclude eyes, hair and creases. No anatomical or generative relighting. |
| Background blur | Averages only selected background support, preventing excluded subject colours from bleeding into the blur | Radius and strength; saved background matte or manual mask. No depth estimation, focus plane or optical aperture model. |
| Highlight glow | Spreads scene-linear highlight energy above an adjustable threshold | Threshold 0–4 stops above middle grey, radius and strength. Keeps HDR values for the output transform. No complete mist/halation preset family. |

Tool search matches descriptions and categories as well as names. Filtering never
changes the authored tool. Clearing an unsuccessful search restores the choices.
Stain correction requires its own clean reference; choosing a clothing mask does
not silently reuse an automatic skin sample as clean fabric.

## Implementation and compatibility

- The shared native renderer processes the new recipe variants before cropping.
  Preview and delivery use the same implementation.
- Filters use normalized selected support and tiles with a three-pass halo.
  An immutable copy of the selection prevents tiles from sampling edited pixels.
  Scratch memory is tile-plus-halo sized; the source copy remains proportional
  to the selected rectangle. Full-resolution, maximum-radius performance is not
  certified on the reference hardware.
- Strength zero, disabled steps and zero-coverage pixels remain unchanged.
  Manual authoring creates undoable history; automatic retouch does not plan
  these new tools.
- No model download, cloud call, new dependency or database migration. Original
  files stay read-only. Existing recipe variants retain their rendering paths.
- Older binaries cannot decode the five new enum values. Remove these steps using
  the current version before opening affected recipes in an older build.

Design: [ADR-0127](../adr/ADR-0127-selected-material-and-light-effects.md).

## Validation

| Check | Measured result |
|---|---|
| Full UI suite | 805 passed in 96 files |
| Final targeted UI checks | 55 passed, including sample handling and saved-material-mask reuse |
| Full recipe and render suites | 465 passed, 12 ignored |
| New signal tests after final renderer adjustment | 9 passed |
| New tile-vs-whole-selection test | Passed within `2e-5` maximum linear-channel error |
| Native integration | Import, five tools, pixel changes, persisted recipes, Undo/Redo, invalid-source rejection, verified JPEG export and original-byte preservation passed |
| Frontend production build | Passed; existing bundle-size warning remains |
| Frozen contracts | All 83 entries locked; no contract relock |
| Bundled desktop build | Passed with `custom-protocol`; linker emitted the existing multiple-manifest resource warning |
| Rebuilt desktop app | 8 checks passed: tool search, authoring/pixel changes for each of five tools, Undo/Redo and source-byte preservation |
| Strict renderer Clippy | Blocked by existing findings in skin connectivity and under-eye rendering; no finding remains in the new material renderer |

The signal tests cover false-chroma suppression, luminance preservation, local
fabric texture ratios, selected-only blur, black preservation, bounded shadow
lift, partial-opacity masks, highlight gating, HDR retention, strength ordering,
determinism and erased/outside pixels. These tests do not measure subjective
portrait quality or establish a preference over another editor.

The full suites preceded the final source-selection refinement and removal of
redundant shadow scratch buffers. Relevant UI, signal and tile tests were rerun
after those changes. Ignored native tests require opt-in fixtures, performance
runs or hardware; an ignored test is not counted as passed.

The rebuilt executable was launched with a fresh isolated catalogue and a
synthetic material/lighting fixture. All five tools were authored through the
actual WebView controls with a 3% radius; each saved the expected native tool
and changed rendered pixels. The fabric source requirement was also checked.
Five Undo operations restored the original recipe hash and rendered pixels;
five Redo operations restored the edited recipe. Source PNG SHA-256 matched
before and after. This is desktop integration evidence, not a real-portrait
quality comparison.

Screenshots, native before/after renders and structured results are under
`.work-checks/material-effects-2026-10-19/desktop-195311/`.
The executable is `target/desktop-launch/debug/aura-desktop.exe`; open
`Start AURA.cmd` to use this build. The test app was closed after verification.

Logs: `.work-checks/aura-material-*`. The strict renderer check reports
`needless_pass_by_value` in `retouch_skin/connectivity.rs:37` and
`too_many_lines` in `retouch_undereye.rs:152`. Those existing changes belong to
the broader working tree and were not rewritten for this addition.

## Feature coverage and remaining work

The [machine-readable ledger](AURA-feature-coverage-2026-10-19.json) preserves all
143 previously inventoried public routes and adds six supplementary records.
Historical statuses retain their dates and are not represented as requalified.
Route counts overlap; ten are use cases and four refer to separate video
products. Counts must not be used as a feature-completion percentage.

| Area | AURA state and remaining acceptance work |
|---|---|
| Import, collection review, culling and story | Existing paths and audited import fixes; comparative culling, large shoots, search/tag completeness and linked review need qualification |
| RAW, colour, reference looks and presets | Existing controls and reference matcher; camera/compression coverage, ICC output, mixed lighting and learned-look equivalence remain open |
| Face/body skin and blemishes | Existing tools and protections; dense anatomy, learned defect classification and independent visual evaluation remain open |
| Eyes, teeth, makeup and expressions | Local tools are partial; gaze correction, expression generation, dental reconstruction and detailed makeup parsing are not supplied here |
| Hair, hands, feet and proportions | Existing local primitives are partial; dedicated anatomy, occlusion and articulated geometry remain open |
| Clothing | Adds manual chroma moiré and stain correction; automatic material detection, luminance moiré reconstruction and seam-aware wrinkle reconstruction remain open |
| Background and creative effects | Adds selected blur and highlight glow; depth, alpha extraction, retained shadows, background fusion, sky/lawn reconstruction and generative expansion remain open |
| Pets | Dedicated species-aware eyes, fur and leash repair remain open |
| Tethering and live capture | Camera SDK/transport, live view, disconnect recovery and physical-camera acceptance remain open |
| Delivery, cloud and collaboration | Existing export/delivery paths; hosted client galleries, cloud collaboration and live-service acceptance remain open |
| Quality and speed superiority | Requires representative reference photographs, blind evaluation and benchmarks on the reference machines; not established by current unit tests |

The current [Evoto desktop overview](https://www.evoto.ai/ai-photo-editor) remains
the broad workflow reference. Its [clothing documentation](https://support.evoto.ai/clothing-accessories-adjustments-module/)
describes automatic garment cleanup, which is broader than the new manual tools.
Its [creative effects documentation](https://support.evoto.ai/color-adjustments-module/)
includes depth and bokeh controls absent from this implementation.
[AI Lab](https://support.evoto.ai/ai-lab/) documents additional generative tasks
requiring independent models and evaluation. The [Desktop 8 launch](https://blog.evoto.ai/evoto-product-launch-2026/)
provides further organisation and workflow requirements. Public capability
descriptions guide the roadmap; no private Evoto model or implementation was used.
