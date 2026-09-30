# Native portrait retouch

Open **Photo Studio → Develop → Retouch**. Processing runs locally without an
account or API key. The workspace provides 20 named workflows using AURA's own
image-processing algorithms. It does not contain Retouch4me's proprietary code
or trained models, and does not claim equivalent automatic results.

Select a tool, click its target on the photograph, adjust the horizontal and
vertical selection radii, then **Apply retouch**. Strength controls the blend;
Feather softens the selection edge. Coordinates can also be entered directly.
Clone and color matching require a source: Alt-click the photograph or choose
**Pick source on photo**. Healing can choose a nearby donor automatically.

| Workflow | What it does in the selected ellipse |
| --- | --- |
| Heal | Blends a sampled or nearby donor with local tone matching |
| Clone | Copies a source patch with a feathered blend |
| Auto blemish | Detects and repairs small dark local spots; review permanent marks |
| Frequency separation | Adjusts low-frequency tone and high-frequency texture independently |
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
Undo/redo uses the normal durable recipe history. **Show before retouch** compares
against the current global edit with native retouch removed. Original files are
never overwritten. Develop and export render the saved stack in order.

The retouch view shows the full photograph before crop/perspective and post-crop
effects, so selections stay anchored when a crop changes. Review the final
composition and decoration after returning to Develop.

## Current boundaries

Targets are feathered ellipses, not painted masks or automatic face/skin/eye
segmentation. Auto blemish uses local image statistics, not a trained classifier.
Several workflows share processing primitives. Backdrop smoothing is not object
removal, and Makeup is not facial landmark-aware makeup synthesis. Effects render
after applying; moving controls does not preview an unsaved effect.

This is a native tool foundation, not a completed 100-feature commercial suite.
Model-backed hair/dust/glasses reconstruction, subject extraction, face lifting,
editable frequency layers, brush masks, and batch face detection need separate
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
