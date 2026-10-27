# Spatial repair lab

Open **Spatial repair**, choose an existing authored mask, choose a method and
click **Add spatial operation**. Adjust the displayed method controls and save
with **Apply spatial tools**. One save is one reversible history step. Drafts
survive panel switching and equivalent recipe reloads. Missing masks must be
reassigned or their operations removed before saving another stack.

These are original deterministic AURA methods; they are not recovered Evoto
algorithms, trained skin classifiers, neural inpainting or recovered detail.
They run on a maximum **256-pixel working grid** and interpolate a correction
into the native image. Unselected full-resolution pixels remain unchanged.
This deliberately bounds expensive neighborhood work, and means a fine pore,
sensor pixel or native JPEG block may be below the working-grid resolution.
Review a full-size export and use existing precision retouch for fine defects.

| # | Method | Mechanism and limit |
| --- | --- | --- |
| 1 | Edge-stopped diffusion | Eight explicit iterations with exponential contrast conductance. |
| 2 | Total variation repair | Eight normalized-gradient relaxation steps; not a converged variational solver. |
| 3 | Patch similarity denoising | Similarity of 3 × 3 patches in a bounded 7 × 7 search. |
| 4 | Vector medoid cleanup | Chooses an actual local RGB sample minimizing total local colour distance. |
| 5 | Trimmed neighborhood repair | Removes the outer fifth of per-channel sample order statistics. |
| 6 | Adaptive Wiener restoration | Uses measured local variance and the explicit noise floor. |
| 7 | Haar residual thresholding | Soft-thresholds three Haar detail coefficients per 2 × 2 block. |
| 8 | Haar detail gain | Applies an independent gain to those coefficients; preserves the block mean. |
| 9 | Laplacian band contrast | Adjusts the difference of one- and three-cell neighborhood means. |
| 10 | Variance-region smoothing | Selects the lowest-variance quadrant mean (Kuwahara method). |
| 11 | Structure-guided strand smoothing | Estimates a local structure tensor and integrates along its tangent. This is not hair detection. |
| 12 | Block seam repair | Repairs weak discontinuities at eight-cell working-grid boundaries; not original-codec block localization. |
| 13 | Disc aperture bokeh | Integrates a disc-shaped 25-sample aperture inside the selection. |
| 14 | Diffraction star filter | Adds selected highlight energy along four rotated ray axes. It can intentionally brighten pixels. |
| 15 | Rotational motion integration | Integrates 13 small rotations around an explicit normalized source point. |
| 16 | Drawn-path motion integration | Integrates two to sixteen user-authored path samples. It is distinct from fixed linear/zoom motion. |
| 17 | Harmonic selection fill | Eight Jacobi steps from neighboring pixels, retaining the unselected boundary. Not semantic reconstruction. |
| 18 | Gradient-domain donor repair | Eight Poisson-style relaxation steps using explicit donor gradients and target boundary. Not an exact converged Poisson solve. |
| 19 | Local channel registration | Explicit opposite red/blue translation while green stays in place; distinct from radial lens-CA correction. |
| 20 | Bright/dark structure cleanup | Joint white/black top-hat residual suppression; real small details can qualify. |
| 21 | Local rank contrast | Adjusts luminance by its neighborhood rank and explicit contrast gain. |
| 22 | Edge-adjacent noise repair | Reduces weak local residuals near stronger image structure. |
| 23 | Directional edge interpolation | Chooses the smoother horizontal/vertical axis for interpolation. |
| 24 | Donor-guided low-frequency repair | Uses bilateral donor weighting while retaining target local residuals. |
| 25 | Measured texture patch quilting | Selects among nine deterministic donor patch offsets by a boundary score. No generated or trained texture. |

Zero strength and disabled/unavailable masks are neutral. Mask opacity scales
the final correction once. Structural gradient estimates and harmonic boundary
conditions may inspect surrounding image context; donor methods explicitly read
their chosen source footprint. Neighborhood colour averaging otherwise uses
selected samples. Thin selections may vanish on the bounded grid and then remain
unchanged. Source coordinates use the uncropped oriented image before geometry.
The stack is limited to 32 operations. Rendering follows extended local colour
and Colour lab, before sharpening and geometry, with full-frame export fallback
and checkpoint/hash invalidation. Originals are never written.
