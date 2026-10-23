# Calibrated reference colour and tone

The main Match a look workflow accepts an optional original/edited pair. This
addresses the ambiguity that made the experimental distribution matcher worse
on some known different-scene grades: object-colour proportions are not editing
instructions. An aligned pair provides actual input/output colour observations.
Single-reference and reference-folder workflows remain available as estimates.

The application fits a cubic RGB residual with 20 basis terms per channel in
encoded display sRGB. The solver uses double-precision pivoted elimination,
degree-dependent ridge regularisation toward the identity, colour-cell balancing
and four rounds of robust residual reweighting. Spatially separate validation
tiles never enter the fit. Their mean and p95 OKLab errors are measured through
the renderer's actual 17-cubed lookup evaluator. Current authored acceptance
limits are mean 3 and p95 8, in Euclidean OKLab distance times 100. These values
are not CIEDE2000 or calibrated probabilities. Native dimension, structure,
finite-value, coefficient-bound and neutral-tone-order checks also apply.

The original passes through the same automatic baseline as batch targets before
calibration. For each target, the learned mapping runs after ordinary colour
stages and before sharpening. It temporarily changes to display coordinates and
back using the existing output shoulder and matrix; the working buffer stays
scene linear. Gamut-bounded trilinear lookup avoids interpolation overshoot.
Preview and export use the same stage, recipe values and render-cache identity.
No reference pixels enter a target image or recipe.

Each target's native render checks clipping, detected-person skin readings and
distance to that learned operation on corresponding target pixels. Requested
strengths are applied explicitly; the model is never refitted to a different
target histogram. If necessary, the operation is attenuated to 75/50/25 percent
of the request, or the baseline is retained. A saved calibration replaces its
predecessor; repeat application is stable and zero strength restores the normal
automatic/profile baseline. Manual fields and a manually protected calibration
pass through the ordinary recipe merge. Save uses the existing transactional
starting-hash check.

Use **Use original + edited pair**, select the unedited photo, then select its
finished version with identical framing. Apply to existing collections or choose
new target photos through the existing batch workflow. Choose a pair with broad
colour coverage. Different target illuminants and camera profiles still need
appropriate baseline correction. A single global mapping cannot reproduce
spatial relighting, local retouch, missing/clipped data or every unknown LUT.

The single-reference native fitter now alternates tone/WB/colour fitting and
split-tone fitting for a second bounded round. Each round retains the earlier
answer unless a real-render score improves; duplicate canonical recipes share
scores. It continues to use native skin/highlight checks and conservative
different-scene regularisation.

The independent experimental distribution matcher remains separate. Its known
accuracy failures in the earlier reliability report are not resolved or hidden
by this calibrated workflow. Synthetic recovery and held-out pair residuals do
not establish universal photographic accuracy. Broad real-photo, camera/profile
and human visual evaluations remain necessary.

Related research distinguishes distribution matching from correspondence-based
transfer: [Pitie et al., colour distribution transfer](https://www.cs.jhu.edu/~misha/ReadingSeminar/Papers/Pitie07.pdf)
and [Liao et al., visual attribute transfer](https://www.microsoft.com/en-us/research/publication/visual-attribute-transfer-deep-image-analogy/).
This implementation uses explicit original/edited correspondence and regularised
regression; it does not implement either paper's algorithm or require a model download.

See the [validation report](reports/AURA-calibrated-reference-2026-10-23.md)
for controlled recovery results, native checks, the local-photo gallery and limits.
