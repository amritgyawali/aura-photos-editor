# The Lightroom plugin

A Lightroom Classic plugin that imports AURA's selections, flags, colour labels and grading, and
returns a photographer's corrections as learning-loop input.

## Why this exists

Section 6.2: "XMP sidecars are the universal path: any photographer can take AURA's culling and
grading into Lightroom, which lowers adoption risk enormously."

That sentence is the whole business case for this directory. A photographer who has spent eight
years building a Lightroom workflow is not going to abandon it on the strength of a culling engine,
however good. What they will do is let something else make the first pass and keep working the way
they already work — and the difference between those two propositions is whether this plugin exists.

## What it does

| Operation | What travels | Direction |
|---|---|---|
| Import selections | Pick flags and reject flags, from `aura-delivery-manifest.json` and the sidecars | AURA → Lightroom |
| Import labels | Colour labels for the phase 29 sets: portfolio, album, social, teaser | AURA → Lightroom |
| Import grading | Exposure, temperature, tint, contrast, highlights, shadows, whites, blacks, vibrance, saturation and the tone curve, from the XMP sidecar | AURA → Lightroom |
| Export corrections | What the photographer changed after the import, as a correction file the learning loop reads | Lightroom → AURA |

## What it deliberately does not do

**It does not write to a RAW file.** Invariant 1, and the plugin has no code path that could:
everything it reads is a sidecar and everything it writes is Lightroom's own catalog through the
SDK's `photo:applyDevelopSettings`, which is Lightroom's business rather than the file's.

**It does not import masks, retouch or cleanup.** Lightroom's local adjustment model is not AURA's,
and a mask translated between them is a mask that looks approximately right and is wrong at the
boundary — which is the failure mode phase 18 spent a whole phase avoiding. A frame that AURA
retouched is exported as pixels, not as instructions.

**It does not round-trip a develop setting it did not write.** The correction export compares
against what the plugin imported, so a slider the photographer had already moved before the import
is not reported as a correction of AURA. That is the same distinction `AURA-LRN-11004` makes: a
change with no decision behind it is not a correction.

## Compatibility

| Lightroom Classic | Status |
|---|---|
| 13.x, 14.x | Supported |
| 12.x | Degrades to XMP-only: selections and grading import, correction export is unavailable |
| Below 12 | Refuses to load, with a message naming the version |

Section 12's row on plugin breakage: "version detection, graceful degradation to XMP-only, and a
compatibility matrix in CI". `Info.lua`'s `LrSdkMinimumVersion` is the detection; the degradation is
in `AuraImport.lua`; this table is the matrix.

## Installing it

Copy `aura.lrdevplugin` into Lightroom's plugin folder and add it in **File → Plug-in Manager**.
The plugin is unsigned, which is what Lightroom expects of a `.lrdevplugin`.

## The file format it reads

`aura-delivery-manifest.json`, written beside every delivery by `aura-export`. Its schema is
`aura.delivery-manifest/1` and it is documented in `docs/delivery.md`. The plugin reads the manifest
to find the files and the sidecars to find the edits; neither is Lightroom-specific, which is what
makes the same path work for Capture One or anything else that reads XMP.
