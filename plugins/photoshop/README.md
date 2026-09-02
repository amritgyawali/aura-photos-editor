# The Photoshop hand-off

A UXP plugin that opens an AURA delivery in Photoshop with the frame's regions as layer masks and
its retouch as separate layers — where that is possible, and saying so where it is not.

## What "where feasible" means, precisely

Section 2.1 says "a Photoshop plugin (open with masks and retouch layers where feasible)", and the
feasibility question has a real answer rather than a hedge.

**What travels.** The rendered pixels, the phase 18 region boundaries as layer masks, and the phase
20 and 21 retouch as a separate layer above the base — so a retoucher can turn AURA's work off,
compare, and take over.

**What does not.** AURA's *parameters*. Exposure, temperature, the tone curve and the HSL bands are
render-graph inputs, and Photoshop has no equivalent of the linear Rec.2020 working space they act
in. A translation would produce a Photoshop document that looks close and diverges the moment
anybody moves a slider, and the divergence would be invisible until the client saw two versions of
the same photograph.

So the hand-off is **pixels plus regions**, and the parameters stay in AURA. A retoucher who wants
AURA's grading changed changes it in AURA and re-exports, which takes about nine seconds and is
correct rather than approximate.

**What is refused outright.** There is no path in this plugin that writes back into an AURA catalog.
A Photoshop document is a fork: the moment a retoucher flattens a layer, the four values phase 14
needs to re-create the file no longer describe it. The delivery manifest records the export and the
PSD is downstream of it.

## Compatibility

| Photoshop | Status |
|---|---|
| 24.0 and newer (2023+) | Supported: layers, layer masks, smart objects |
| 23.x | Degrades to a flattened open, with a message naming the version |
| Below 23 | UXP is unavailable; the TIFF opens through **File → Open** like any other file |

Section 12's row: version detection, graceful degradation, a matrix.

## Installing it

`aura-uxp/` is a UXP plugin folder. Load it with the UXP Developer Tool, or package it as a `.ccx`
for distribution. It is unsigned in this repository; `ops/sign/` is where a release signs it.
