# Native portrait retouch

Open **Photo Studio → Develop → Retouch**. Processing runs locally without an
account or API key. The workspace provides 23 named workflows using AURA's own
image-processing algorithms. It does not contain Retouch4me's proprietary code
or trained models, and does not claim equivalent automatic results.

Select a tool and choose **Ellipse**, **Brush** or **Eraser**, then **Apply retouch**.
Strength controls the effect blend; Feather softens the selection edge. Ellipse
coordinates can be entered directly, and **Dab at target coordinates** provides
a keyboard alternative to drawing a brush stroke.
Clone and color matching require a source: Alt-click the photograph or choose
**Pick source on photo**. Healing can choose a nearby donor automatically.

| Workflow | What it does in the selected area |
| --- | --- |
| Heal | Blends a sampled or nearby donor with local tone matching |
| Clone | Copies a source patch with a feathered blend |
| Auto blemish | Detects and repairs small dark local spots; review permanent marks |
| Frequency separation | Adjusts low-frequency tone and high-frequency texture independently |
| Skin smoothing · protect detail | Reduces middle-scale variation with edge protection; fine detail is retained at 100% |
| Even sampled skin tone | Moves selected skin chroma toward a clean reference patch without changing luminance |
| Skin dodge and burn · protect edges | Balances local light within a bounded exposure range and preserves RGB proportions |
| Micro dodge and burn | Reduces small local luminance variations |
| Dodge | Lightens locally |
| Burn | Darkens locally |
| Skin color | Adjusts warmth/tint while retaining luminance |
| Color match | Transfers sampled chroma toward the target |
| Mattify | Reduces bright shine |
| Under-eye | Lifts local dark tones |
| Wrinkle | Applies gentler frequency-based smoothing |
| Teeth | Reduces yellow and gently brightens |
| Eye clean | Reduces excess red chroma |
| Eye detail | Enhances local contrast |
| Red-eye | Reduces red-dominant pupil color |
| Fabric | Provides tone/texture frequency controls for cloth |
| Backdrop | Smooths a selected background area |
| Glare | Attenuates bright reflections; cannot reconstruct obscured detail |
| Makeup | Applies a local cosmetic warmth/tint adjustment |

Frequency-based tools keep the original high-frequency band at **100% texture
gain**. Tone smoothing at 0% and texture gain at 100% are neutral. Natural and
Polished skin presets add frequency smoothing, micro dodge and burn, and shine
reduction to the selected region as one undoable change.

Saved operations can be selected and updated, disabled, removed, or cleared.
Use the up/down buttons to change processing order and Duplicate to copy an
operation with a new identity. These changes are also undoable.
Undo/redo uses the normal durable recipe history. **Show before retouch** compares
against the current global edit with native retouch removed. Original files are
never overwritten. Develop and export render the saved stack in order.

The retouch view shows the full photograph before crop/perspective and post-crop
effects, so selections stay anchored when a crop changes. Review the final
composition and decoration after returning to Develop.

## Precision controls

- **Brush / Eraser:** paint or subtract from the operation's mask. Brush opacity
  sets coverage per stroke; overlapping points within one stroke do not keep
  increasing opacity. Pressure from a pen changes radius, with a 10% minimum.
  Feather affects the whole mask. Erasing never changes the original photograph.
- **Undo brush stroke / Clear painted mask:** edit the draft selection before
  applying. Applying stores one operation with its complete mask.
- **Preview unsaved changes:** after a short pause, renders the draft through the
  native renderer without saving it. Requests run one at a time; obsolete results
  are discarded. Previewing a selected operation replaces it temporarily in its
  current stack position. Apply commits the result; Discard restores saved values.
- **Fit / zoom / 1:1 preview:** inspect the screen-resolution image and pan with
  Hand or middle-button dragging. Zoom does not modify output dimensions. The
  percentage refers to preview pixels, not necessarily full-resolution originals.
- **My tool presets:** save up to 24 named settings presets on this device. Presets
  contain tool settings only; they do not copy selections, donor coordinates or
  photo identifiers to another photograph.
- **Shortcuts:** focus the photo, then B brush, E eraser, V ellipse, H hand,
  brackets for brush size, Enter to apply, plus/minus to zoom, 0 to fit, arrows to
  pan, and Ctrl/Cmd+Z to undo a draft stroke or saved edit. Shift+Ctrl/Cmd+Z redoes
  saved history. Inputs retain normal text-editing shortcuts.

Apply or discard a draft before leaving the photograph. A mask supports 128
strokes; the saved stack supports 8,192 total brush points. The UI limits each
gesture to 1,024 points. Larger work can be simplified into shorter strokes and
fewer operations. The selection guide approximates stroke shape; the native
preview shows the actual feathering and effect.

## Sampled skin tools

Choose a tool in **Sampled skin**, then **Pick skin sample on photo** and click a
clean patch of skin. Numeric source coordinates work too. Choose an ellipse or
painted mask; **Use full photo selection** extends the target to the whole image.
The renderer then limits the effect to colors close to the sample within that
target. The overlay shows the authored region; the preview shows the actual
color-limited effect. Sampled color matching is not automatic face detection.

Start with the default color tolerance and 100% fine detail. Increase tolerance
to include more color variation; reduce it if lips or surroundings are affected.
Edge protection reduces smoothing across abrupt brightness/color boundaries.
Frequency radius separates fine detail from broader variations, and Tonal/Color
evening controls correction before the overall Strength blend. Use Preview
unsaved changes to review and Apply to save an undoable operation.

Use a separate sample/selection for each person or lighting condition. Similar
colors in clothing and backgrounds can match, so use the brush/eraser to refine
the target. A black sample produces no change. These tools do not infer an
ideal complexion or automatically distinguish skin from permanent marks.

The [competitor audit](retouch-competitor-audit.md) records the remaining gaps
against documented Retouch4me and SkinFiner behavior. Exact equivalence has not
been established. [ADR-0070](adr/ADR-0070-sampled-skin-processing.md) describes
the independent processing implementation.

## Current boundaries

Targets are feathered ellipses or painted masks, with no automatic face/skin/eye
segmentation. Auto blemish uses local image statistics, not a trained classifier.
Several workflows share processing primitives. Backdrop smoothing is not object
removal, and Makeup is not facial landmark-aware makeup synthesis. Effects render
after applying, or through the optional unsaved-preview control.

This is a native tool foundation, not a completed 100-feature commercial suite.
Model-backed hair/dust/glasses reconstruction, subject extraction, face lifting,
editable frequency layers and batch face detection need separate
implementations and quality evaluation. Five portrait checks establish rendering,
history and export behavior; they do not establish professional retouch quality
across skin tones, lighting conditions or camera formats.

Retouch currently requires a whole-frame CPU render, including when export would
otherwise stream tiles. Large photographs therefore need more memory. Recipes
support up to 256 native operations. See [ADR-0068](adr/ADR-0068-native-retouch-workspace.md)
for persistence, coordinate and rendering decisions.

## Repeatable verification

`scripts/test-native-retouch.py` checks a running debug desktop with five real
JPEG portraits, creates a separate test collection, compares native exports with
the full renderer, exercises history, and checks original hashes. It requires
Playwright and Pillow. Start the desktop with
`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9223`, then run:

```powershell
python scripts/test-native-retouch.py --photos PATH_TO_FIVE_JPEGS --output OUTPUT_FOLDER
```

Evidence is written to the output folder rather than committed with the source.
`scripts/test-precision-retouch.py` additionally checks painted/erased regions,
draft history isolation, operation reordering/duplication, and real pointer
interaction after zooming. It accepts the same command-line arguments.
`scripts/test-sampled-skin.py` checks the three sampled-skin operations, their
non-persistent previews, saved history, five portrait exports and sample controls.
