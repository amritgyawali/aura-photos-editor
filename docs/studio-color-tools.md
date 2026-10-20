# Levels, selective colour and extended local colour

The **Color & light** editor includes a draft panel for Levels and selective colour. Change controls, then choose **Apply colour tools** to store one undoable edit. **Discard colour draft** returns to the saved recipe. A changed recipe rejects an old draft rather than overwriting another edit.

| Addition | Operation |
| --- | --- |
| Full-image Levels | Joint RGB and independent red, green and blue input black/white, gamma, output black/white |
| Luminosity Levels | Applies joint Levels to working luminance while preserving RGB ratios; individual channel controls still follow |
| Selective CMYK colour | Cyan, magenta, yellow and black in nine soft colour ranges; relative or absolute ink changes |
| Percentile auto Levels | Proposes input endpoints at the measured 0.5% and 99.5% percentiles |
| Sample input black point | Proposes the selected channel's black endpoint from a 7 × 7 patch |
| Sample input white point | Proposes the selected channel's white endpoint from a 7 × 7 patch |
| Local point curve | Add, remove and move input/output points inside an existing mask |
| Local eight-band HSL | Independent hue, saturation and luminance shifts inside that mask |
| Local three-way grading | Shadow, midtone and highlight colour wheels with blending and balance |
| Local sharpening | Luminance sharpening blended only inside the mask |
| Local dehaze | Measured haze adjustment blended only inside the mask |
| Targeted HSL selector | Measures a coloured patch and selects its strongest HSL band; refuses neutral samples |

Sampling coordinates refer to percentages of the uncropped, oriented edited photograph. Measurements use its proxy before Levels, and only propose a draft. They do not alter the original or save history. Collapsed black/white endpoints and malformed curves are rejected. User-entered curves require increasing input coordinates and values between 0 and 255.

Stored settings live under `studio_color_v1`, outside the frozen v1 recipe contract. Old recipes have neutral settings. Extended local settings refer to existing mask IDs without changing the mask geometry or its manual parameters; the mask's enabled state and amount apply to all extended colour tools. Deleting a mask makes its colour settings inactive, and resetting that mask's colour removes its extension entry. Large renders with extended local colour use the whole-frame path so mask positions and sharpening do not acquire tile seams.

The operators share the native renderer for previews and exports. Levels work in the renderer's reversible curve coordinate domain, with linear Rec.2020 buffers on both sides; this is not a baked output transfer. Selective colour uses AURA's independently implemented six hue selections and three tonal selections.

[Evoto documents Levels and selective colour](https://support.evoto.ai/color-adjustment-feature-modules/) and [mask-based adjustments](https://support.evoto.ai/feature-introduction-masking/). These sources establish feature categories, not their private implementations or pixel equivalence. Luminosity Levels and percentile auto Levels are AURA additions to those documented workflows. No Evoto code or model weights are included.
