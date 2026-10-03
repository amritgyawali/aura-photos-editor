# ADR-0070 — Sample-guided skin processing

- Date: 2026-09-30
- Status: accepted

The user asked whether AURA produces exactly the same results as Retouch4me and
SkinFiner. It does not. Their documented workflows guide this implementation;
their proprietary models and algorithms are not available to this project.
Comparative quality requires the same source images, explicit competitor
versions/settings and reference outputs. Function names are not evidence of parity.

Add three opt-in native operations: `skin_smooth`, `skin_uniformity`, and
`portrait_dodge_burn`. Existing operations keep their algorithms. An optional
`skin` object contains validated color tolerance and edge protection; omission
preserves old recipe serialization. New tools require a photographer-selected
`source` point. No frozen contract or model manifest changes.

The selected patch supplies the reference chromaticity. A smooth color-distance
range intersects the existing ellipse or painted mask. It is evaluated on both
original and neighborhood pixels, preventing blur from extending the selection
into distinctly colored features. It is a color range, **not** learned skin
segmentation, face detection, a demographic classification or a confidence score.
Similar-colored surroundings may match, so the photographer controls the region.
Use separate operations/samples for people with different complexions or lighting.

All output arithmetic remains in linear Rec.2020. Skin smoothing keeps the fine
residual and reduces the middle band using a self-guided filter per color channel.
The guided-filter construction follows the public
[He, Sun and Tang paper](https://people.csail.mit.edu/kaiming/publications/pami12guidedfilter.pdf);
this is AURA's implementation, with signal-relative regularization to avoid a
fixed brightness threshold. It does not claim that competitors use this method.
Running sums make neighborhood means linear in pixel count, independent of radius.

Uniformity moves low-frequency chroma toward the sampled reference with zero
luminance delta. Dodge/burn applies a bounded multiplicative exposure correction
(at most half a stop before strength blending), preserving RGB proportions.
Fine detail defaults to 100%; changing it is an explicit manual edit under ADR-0068.
Negative-channel prevention scales the whole correction, preserving its direction.

Previews, saved history and exports use the same renderer. Source samples, masks
and settings persist in recipes; named tool presets omit coordinates and masks.
Whole-frame CPU rendering remains a memory limitation. This adds no network
dependency or downloaded model. Reference portraits verify integration, not
commercial quality parity, population fairness or performance on all cameras.
