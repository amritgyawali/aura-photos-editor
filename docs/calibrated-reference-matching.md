# Calibrated reference colour and tone

The main Match a look workflow accepts an optional original/edited pair. This
addresses the ambiguity that made the experimental distribution matcher worse
on some known different-scene grades: object-colour proportions are not editing
instructions. An aligned pair provides actual input/output colour observations.
Single-reference and reference-folder workflows remain available as estimates.

The application fits a cubic RGB residual with 20 basis terms per channel in
encoded display sRGB. The solver uses double-precision pivoted elimination,
degree-dependent ridge regularisation toward the identity, colour-cell balancing
and four rounds of robust residual reweighting. A smooth, regularised 9-cubed RGB
residual lattice can supplement the cubic for more detailed colour effects.
Its sparse least-squares system uses double-precision, preconditioned conjugate
gradients and robust reweighting, with shrinkage back to the cubic in unsupported
colours. The transform is still global; it never copies the reference's objects.

Entire spatial tiles form three disjoint sets: regression, model selection and
final validation. The adaptive model is retained only if selection mean error
improves materially while p95 remains within a 0.01 tolerance; otherwise the compact cubic is
kept. Final validation pixels participate in neither fitting nor model selection.
Their mean and p95 OKLab errors are measured through the renderer's actual lookup
evaluator. Current authored acceptance limits remain mean 3 and p95 8, in Euclidean
OKLab distance times 100. These values are not CIEDE2000 or calibrated probabilities.
Native dimension, structure, finite-value, coefficient-bound and neutral-tone-order
checks also apply.

Adaptive models use a bounded 33-cubed tetrahedral display lookup. Its piecewise
affine cells permit a Jacobian norm bound across every tetrahedron, including
colours absent from the reference. Models with an encoded-RGB infinity-norm noise
gain above the authored limit of 8 are refused. Legacy trilinear models use a
conservative bound from directional edge maxima. This bounds the lookup's response
to small encoded-RGB changes; it does not prove correct grading, baseline white
balance or accuracy in unobserved colours.

The original passes through the same automatic baseline as batch targets before
calibration. For each target, the learned mapping runs after ordinary colour
stages and before sharpening. It temporarily changes to display coordinates and
back using the existing output shoulder and matrix; the working buffer stays
scene linear. Gamut-bounded lookup avoids interpolation overshoot. Existing
version-1 recipes keep their original 17-cubed trilinear evaluation and encoding;
adaptive version-2 recipes use the new evaluator. The render cache version changes
with the new stage capability.
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

The independent experimental distribution matcher remains separate. Its
[later refinement report](reports/AURA-reference-transfer-refinement-2026-10-23.md)
records improvements to neutral estimation and conservative cross-scene fitting,
alongside remaining failures. This calibrated workflow does not qualify that
separate engine. Synthetic recovery and held-out pair residuals do
not establish universal photographic accuracy. Broad real-photo, camera/profile
and human visual evaluations remain necessary.

Related research distinguishes distribution matching from correspondence-based
transfer: [Pitie et al., colour distribution transfer](https://www.cs.jhu.edu/~misha/ReadingSeminar/Papers/Pitie07.pdf)
and [Liao et al., visual attribute transfer](https://www.microsoft.com/en-us/research/publication/visual-attribute-transfer-deep-image-analogy/).
This implementation uses explicit original/edited correspondence and regularised
regression; it does not implement either paper's algorithm or require a model download.

See the [validation report](reports/AURA-calibrated-reference-2026-10-23.md)
for the original controlled recovery results, native checks, local-photo gallery
and limits. The [adaptive calibration report](reports/AURA-adaptive-calibration-2026-10-23.md)
records the new detailed-grade checks. Tetrahedral lookup follows the standard
four-vertex interpolation supported by [OpenColorIO](https://opencolorio.readthedocs.io/en/v2.4.0/api/enums.html)
and [Colour](https://colour.readthedocs.io/en/develop/generated/colour.algebra.table_interpolation_tetrahedral.html).
The residual fitting and model selection here are local implementations; neither
library nor downloaded model weights are required.
