# Material and texture lab

Open a photo, create a selection in **Masks**, then open **Material & texture**.
Choose the selection and a tool, click **Add material operation**, adjust its
controls, and click **Apply material tools**. The native preview and exports use
the saved stack. One Apply creates one Undo step. Discard restores the saved
draft. An operation can be disabled, removed, moved in the stack, or assigned to
another mask. Those controls are not counted as extra tools.

| Tool | Method and useful limit |
| --- | --- |
| Directional hair smoothing | Range-gated integration along a chosen strand direction. It does not infer a hair flow field. |
| Silver hair darkening | Reduces the broad luminance of bright achromatic strands, retaining local residual detail and guarding brighter local highlights. It does not classify hair or age. |
| Fabric frequency balance | Independent fine and coarse band gains around selected-only spatial means. Fine/coarse retention of 1 is neutral for that band. |
| Luminance moiré notch | Three-point directional comb averaging at the selected spacing. Retains pixel chroma ratios; review real weave detail at full size. |
| Flat-area debanding | Smooths small residuals with a continuous contrast gate. It can soften low-contrast texture; select a smooth backdrop. |
| Median chroma repair | Median neighborhood chroma with original luminance. Intended for isolated colour noise rather than broad colour grading. |
| Isolated pixel repair | Replaces a strong impulse only when the surrounding luminance quartiles agree. Bright points of real detail can also qualify. |
| Line scratch repair | Repairs a thin horizontal/vertical line when samples across it agree. Direction chooses the nearest image axis. |
| Row and column stripe repair | Measures selected row/column residual bias against a smooth base and subtracts it. Scene structure can influence the estimate. |
| Checkerboard suppression | Removes measured brightness offsets of the four two-pixel parity groups. It targets fixed raster patterns, not arbitrary moiré. |
| Edge ringing suppression | Clamps luminance overshoot outside neighborhood quartiles where a substantial edge exists. This is an explicit repair, separate from sharpening safeguards. |
| Specular colour recovery | Blends the surrounding diffuse chroma into bright near-neutral highlights. It cannot recover clipped sensor data. |
| Colour mottle removal | Removes intermediate-scale chroma differences while keeping finer residuals and luminance. |
| Backdrop gradient leveling | Fits selected illumination to a plane and subtracts its bounded slope. It can remove intentional lighting gradients. |
| Cross-channel detail reconstruction | Rebuilds inconsistent channel detail from the median normalized channel residual while restoring luminance. It cannot recover missing photographed structure. |

All processing uses linear working pixels. Kernel samples outside the selection
are excluded, and pixels with zero coverage stay unchanged. Mask amount scales
the operation; disabled or missing selections produce no material effect. A
missing selection must be reassigned or its operation removed before applying
another stack draft. Other edits and originals stay intact. Old recipes have an
empty stack. Up to 32 operations are allowed, and reach is 0.1% to 10% of the
shorter image edge. Fixed-neighborhood repairs use a small raster neighborhood;
their appearance can differ between a proxy and full-resolution export.

These are independent AURA methods. Some extend observed Evoto material
categories; the remaining restoration tools are AURA additions. Existing AURA
denoising, frequency retouch, chroma moiré suppression and sharpening remain
distinct existing facilities. No private Evoto models or tuning data are used.
Full-frame processing can require substantial memory on large originals.
