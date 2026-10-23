# Match an edited reference across a batch

The [refinement report](reports/AURA-reference-transfer-refinement-2026-10-23.md)
records the current safety checks and remaining grade-fidelity test failures.
The earlier [reliability report](reports/AURA-reference-grade-reliability-2026-10-23.md)
retains the baseline results for comparison.

Open **Match a look**, choose **Upload colour reference**, then select an opaque
JPEG, PNG or TIFF. Save initial corrections on the target photos first. Choose
**Analyse reference match** to review one photo, or expand **Match this collection**
and choose **Match colour across this collection**. Every target gets its own fit;
the batch does not copy the first target's white balance or exposure. For a shared
transformation learned from actual edits, use **Use original + edited pair** on
the main reference screen; see [calibrated matching](calibrated-reference-matching.md).

The distribution engine fits lightness, a neutral axis across shadows and
highlights, chroma, and supported hue adjustments in OKLab. The renderer and the
fitter share the same transform evaluator, including gamut mapping. The engine
keeps the best measured intermediate fit for strongly corresponding scenes and
checks cancellation between colour updates. An already matching measurement
retains the identity transform.

Version 3 keeps a different colour scene's tone curve, per-hue brightness and
per-zone colourfulness. Those statistics otherwise confuse a bright indoor wall
with an instruction to brighten a dark outdoor background. Shared hue sectors
provide a single bounded hue update and a robust global saturation estimate.
The unrelated-scene estimate is regularised halfway toward identity, an authored
strength prior rather than a calibrated confidence. Monochrome and toned-print
references retain their separate desaturation path, with a small tone adjustment.

Spatial sampling uses deterministic jitter within bounded image strata. This
avoids repeatedly selecting only one stripe of patterned clothing and respects
the sample budget on panoramas and narrow images. Monochrome classification
checks absolute chroma across all sampled pixels, so a small sampled colour detail
does not disappear merely because it falls outside a distribution percentile.

Neutral estimation checks for a supported low-chroma cluster instead of relying
only on the majority object colour, and records support per lightness interval.
A small supported neutral minority can provide an anchor. Inconsistent midtone
clusters are checked against bright neutral evidence while allowing larger shadow
differences for split toning. Unsupported intervals interpolate between supported
anchors; a majority object colour is never the fallback white balance.
This is an estimate: subject colour and illumination are
ambiguous when a photo has no reliable neutral surfaces. It is not a calibrated
measurement of the light source or the original editor's temperature slider.

References measured by an older distribution engine must be uploaded again to
refresh their measurements. Measurement version 3 also marks older saved analyses
stale and refuses to apply their strengths, so the native guard cannot compare
statistics produced by different estimators. Existing saved recipe renders remain
reproducible; analyse again to use the new fitter. Amount zero restores a current
session's saved baseline,
and normal Undo/Redo remains available. Source photos remain read-only.

Apply renders and measures the chosen strengths at native resolution. A result
that increases the measured reference distance is refused before any recipe or
history write. Lower the strengths or analyse again if this occurs.

Exact matching of every possible photograph cannot be guaranteed. A finished
reference alone cannot uniquely identify its original scene colours, lighting,
camera response and editing operations. Clipped channels contain no recoverable
detail, and statistical similarity is not photographic equivalence. The distance
shown in AURA is a measured difference, not a percentage of accuracy. Film grain,
spatial lighting, retouching and geometry require their own editing tools.

The colour coordinates follow [Ottosson's OKLab definition](https://bottosson.github.io/posts/oklab/).
The distinction between transferring colour statistics and recovering editing
operations is also relevant to [Reinhard et al.'s colour-transfer method](https://www.cs.tau.ac.il/~turkel/imagepapers/ColorTransfer.pdf).
