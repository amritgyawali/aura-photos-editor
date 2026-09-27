# Lightroom parity

What Adobe Lightroom Classic does, where the same thing lives in AURA, and whether AURA does it
automatically. The difference AURA is built around: **one click runs the whole edit**, and every
panel below is still there when you want to take over.

"Automatic" means the one-click path (the start screen, **Auto edit all photos**, or **Auto** on a
single photo) sets it without anybody touching a slider. Anything you move by hand is recorded as
your setting and no automatic pass changes it again.

## Develop

| Lightroom | AURA | Automatic |
|---|---|---|
| **Basic**: white balance (temp, tint), exposure, contrast, highlights, shadows, whites, blacks | Develop → Basic. Recipe `global.*` | Yes: exposure, highlights, shadows and contrast are measured per photo; the rest come from the chosen profile |
| Basic: texture, clarity, dehaze, vibrance, saturation | Develop → Basic | Yes, from the profile |
| Auto (Basic panel) | **Auto** button, and every one-click edit | Yes |
| Profile browser / creative profiles | 21 edit profiles on the start screen and in Develop → Presets | Yes, the chosen profile |
| Presets, preset amount slider | Develop → Presets with a 0-150 % strength | Yes |
| **Tone Curve**: point curve | Develop → Tone Curve, Luminance tab (drag, add, double-click to remove) | From profiles |
| Tone Curve: red, green, blue curves | Tone Curve → Red / Green / Blue tabs (`global.channel_curves`) | From profiles |
| Tone Curve: parametric (highlights, lights, darks, shadows, three splits) | Tone Curve sliders (`global.parametric`) | From profiles |
| **Color Mixer / HSL** (8 bands x hue, saturation, luminance) | Develop → Color Mixer | From profiles, with a skin guard on orange |
| **Black & White** + B&W mix | Develop → Black & White | From the two monochrome profiles |
| **Color Grading**: shadows, midtones, highlights, global wheels, blending, balance | Develop → Color Grading (`global.colour_grade`) | From profiles, with a skin ceiling on midtones and highlights |
| **Detail**: sharpening (amount, radius, detail, masking), noise reduction (luminance, detail, colour) | Develop → Detail | From profiles; high-ISO denoise tiers in the advanced workflow |
| AI Denoise | Restoration (phase 22) in the advanced workflow | Yes, where measured noise warrants it |
| **Lens Corrections**: profile, chromatic aberration, vignetting | Develop → Lens Corrections | Geometry pass in the advanced workflow |
| **Transform**: Upright, perspective | Geometry (phase 23): horizon and vertical correction | Yes, in the advanced workflow |
| Crop & straighten, aspect presets | Develop → Transform & Crop (Original, 1:1, 4:5, 3:2, 16:9, 9:16) | Crops are proposed by the geometry pass |
| **Effects**: post-crop vignette (amount, midpoint, roundness, feather, highlights) | Develop → Effects (`global.effects.vignette`) | From profiles, highlight priority |
| Effects: grain (amount, size, roughness) | Develop → Effects (`global.effects.grain`) | From film and monochrome profiles |
| **Calibration**: shadow tint, red/green/blue primary hue and saturation | Develop → Calibration (`global.calibration`) | From profiles |
| Before / after | Compare view with a divider | - |
| History, snapshots, reset | History panel, Undo / Redo / Reset | Every automatic edit is a history entry with its reason |
| Copy / paste, **Sync Settings** | Develop → Sync settings to all photos (crop optional) | - |
| Match Total Exposures | Gallery consistency (phase 25) | Yes, in the advanced workflow |
| Masking: sky, subject, background, people, linear, radial, brush | Masks (phase 18) and local light (phase 19) | **Partly**: the mask generators ship untrained, so AI masks are not produced on real photos yet |
| Healing, content-aware remove | Cleanup (phase 24) and micro-retouch (phase 21) | **Refuses** unclassified removals on real photos until its detector is trained |
| Red eye | Micro-retouch eyes | Advanced workflow |
| Match a look / reference | Instagram style matching (start screen, step 2) | Yes |

## Library and output

| Lightroom | AURA | Automatic |
|---|---|---|
| Import from folder or files | Start screen, step 3 | Editing starts when the import finishes |
| Flags, ratings, culling | Cull (phase 12) with reasons | Yes, in the advanced workflow |
| People / faces | People (phase 06) | Placeholder detector - see the README |
| Collections, albums | Curation (phase 29) | Proposed, never applied without you |
| Export presets, resize, sharpen for output, metadata | Export (phase 30) | Repeats the last export on re-runs |
| XMP sidecars | Written with Lightroom's own `crs:` names, including colour grading, parametric and RGB curves, calibration, effects and the B&W mix, so an AURA edit opens in Lightroom |  |

## Not in AURA yet

Stated plainly rather than implied:

- **HDR merge and panorama merge.** Not implemented.
- **Soft proofing.** Not implemented.
- **AI masks on real photographs.** The segmentation heads are untrained placeholders; linear and
  radial masks render, generated ones are reported as skipped.
- **Content-aware remove on real photographs.** Implemented and deliberately refused until its
  distraction detector is trained.
- **GPU rendering.** The shaders exist and are checked against the reference; no GPU backend is
  linked, so rendering runs on the CPU.
- **Camera-matching profiles measured from real cameras.** Every camera renders through the neutral
  reference profile.
