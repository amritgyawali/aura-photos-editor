# Match an edited reference across a batch

Open **Match a look**, choose **Upload colour reference**, then select an opaque
JPEG, PNG or TIFF. Save initial corrections on the target photos first. Choose
**Analyse reference match** to review one photo, or expand **Match this collection**
and choose **Match colour across this collection**.

## The best result: keep the reference's original in the collection

If you edited one photo of the shoot (in AURA, Lightroom, Evoto or anything else)
and exported it as your reference, keep its original in the collection and keep its
name in the export's name - `989A2698-edit.jpg` for `989A2698.CR3` is enough. AURA
finds the original by that name, confirms it is the same frame by its structure,
learns the exact edit from the pair (first from colour statistics, then pixel by
pixel), and applies **that edit** to every other photo. The panel says so: "The
edit was learned from 989A2698.CR3".

This is what makes a batch match reliably. On a real wedding shoot under eight known
looks, the learned edit left every other frame about 1.7 ΔE from the right answer on
average, against 4.8 with no match. The reference's own frame matches to well under
1 ΔE for most looks.

## Without the original

A reference from another shoot - a magazine page, a colleague's photo - carries its
scene and its edit mixed together: a beige wall and a warm grade measure the same.
AURA then matches its **appearance**, taking each part only as far as can be trusted
across different scenes: black and white points, an overall cast measured from both
photos' greys, colourfulness, tonal shape and per-hue character, each by a measured
amount. The photo always moves toward the reference's look and never away from it.
The panel says this happened and suggests importing the original.

## What is changed, and how it is kept safe

The match is one stage in the editor: a tone curve, white balance and split toning,
colourfulness, twelve hue sectors and five lightness zones, evaluated in
[OKLab](https://bottosson.github.io/posts/oklab/). The renderer and the fitter share
the same transform evaluator, including gamut mapping: a stronger colour keeps its
hue and lightness instead of clipping a channel. Each person's skin is measured, and
a face that would turn green, grey or orange is protected instead of refused.

Spatial sampling uses deterministic jitter within bounded image strata, so a
striped fabric is not measured as one stripe and narrow panoramas keep their
budget. Monochrome references are recognised from absolute chroma on every sample,
so a small selective-colour detail is kept.

The strength sliders move from the photo's previous match (or none) to the new one.
Amount zero restores the previous look exactly; matching a second reference replaces
the first rather than stacking on it. Apply renders and measures the result at
native resolution and reports the distance; it does not refuse a strength. Undo
and Redo work as for any edit. Source photos remain read-only.

References imported before this engine must be uploaded again to be measured.

## Limits

Exact matching of a photo of a different scene cannot be guaranteed: a finished
reference alone cannot identify its original scene colours, lighting, camera
response and editing operations. On nineteen expert-edited FiveK photos from
unrelated shoots, matching appearance moved each photo's look halfway to the
reference without bringing it closer to what the expert did to it - and the expert's
own edit of one photo did no better on another. Clipped channels contain no
recoverable detail. A matte look made by lifting blacks in RGB is the hardest to
reproduce exactly. Film grain, spatial lighting, retouching and geometry need their
own tools. The distance shown is a measured difference, not a percentage of accuracy.

Decisions and measurements: [ADR-0160](adr/ADR-0160-reference-colour-matching-by-learned-edit-and-distribution-transfer.md).
The difference between transferring colour statistics and recovering editing
operations is the one [Reinhard et al.](https://www.cs.tau.ac.il/~turkel/imagepapers/ColorTransfer.pdf)
describe.
