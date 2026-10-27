# Adaptive reference calibration — 2026-10-23

This change improves the original/edited-pair mode on the main Match a look screen.
It does not establish perfect matching from an arbitrary finished reference alone.
The separate unpaired engine's remaining accuracy failures are retained in the
[reference-transfer report](AURA-reference-transfer-refinement-2026-10-23.md).

## Implementation

- Keep the robust, regularised, 20-term cubic RGB model as the compact global prior.
  Add an optional 9-cubed residual lattice for supported colour-dependent departures
  that a cubic cannot express closely, including more detailed tone and colour effects.
- Fit the local residuals with a sparse, double-precision least-squares operator:
  four tetrahedral barycentric weights per observation, neighbour smoothness,
  shrinkage to the cubic, Jacobi-preconditioned conjugate gradients and two rounds
  of robust residual weighting. Cancellation is checked inside the solver. Node
  corrections are finite and bounded to ±0.35 encoded-RGB units.
- Split complete spatial tiles into regression, model-selection and final-validation
  sets. Neither selection nor final-validation pixels enter either regression. Choose
  the adaptive model only when selection mean error improves by the authored
  material-gain margin while p95 remains within a 0.01 tolerance. Final validation
  errors are evaluated after selecting the model; acceptance limits remain mean 3
  and p95 8 in OKLab × 100.
- Bake adaptive recipes into a bounded 33-cubed tetrahedral display lookup. Its
  neutral diagonal interpolates diagonal endpoints, avoiding unrelated off-diagonal
  colours during display lookup. Interpolation is convex and cannot overshoot the
  sampled gamut. Explicit partial strengths interpolate the same operation.
- Compute an RGB infinity-norm noise-gain bound over every cell of the actual baked
  lookup. Each adaptive tetrahedron is affine, so the maximum absolute Jacobian
  row sum bounds its response to small encoded-RGB differences. Legacy trilinear
  cells use conservative directional edge maxima. Reject gains above the authored
  limit of 8, alongside the existing neutral-tone-order check. This bounds this
  display lookup, not the complete RAW pipeline or grading correctness.
- Preserve version-1 recipe encoding and its 17-cubed trilinear arithmetic. Store
  adaptive models as version 2; validate their dimensions and finite bounds before
  writing. Residual-only grades are preserved rather than mistaken for identity.
  The render-engine cache version changes to `+calibrated.2`.

No new neural weights, external services or dependencies are introduced. The existing
native baseline correction, skin/highlight guards, manual-field merge, transactional
save, repeat application and zero-strength restoration remain in the application path.

## Controlled recovery

Errors below are mean Euclidean OKLab difference multiplied by 100 against a known
operation on an independent target palette. They are not CIEDE2000, percentages of
accuracy or human visual assessments.

| Case | Comparison | Result |
|---|---|---:|
| Detailed colour-dependent teal effect + two-cycle smooth tone curve | Same current training split: cubic vs adaptive on unseen target colours | 1.2121 → 0.3086 |
| The selected detailed model on untouched final validation pixels | Mean / p95 | 0.5330 / 1.2102 |
| Sinusoidal S-curve on another target palette | Previous cubic result vs adaptive result in the existing controlled case | 0.1827 → 0.0551 |
| Linear-light WB on another palette | No grade vs selected model | 2.5839 → 0.0080 |

The new native different-scene case reduces mean RGB byte error from 5.5648 to
0.8216. These are RGB byte units, not the OKLab units in the table. It uses the
actual main-screen analysis, automatic baseline, saved version-2 recipe, native
preview and full export render. Preview/export equality, repeat application, half
strength, exact zero restoration, Undo and unchanged source bytes all pass.

The earlier paired native case remains close: 5.7882 → 0.2237 mean RGB byte error.
The main single-reference native case remains 4.593 → 0.532 in its controlled
same-photo example. These individual cases do not qualify arbitrary photographs.

The detailed-grade test requires both mean error below 0.35 and more than 60%
improvement over the current cubic on the independent target. The six earlier
known-grade cases retain their existing mean-error and improvement thresholds.
The existing tests still reject unrelated, reframed and checkerboard-local-edit
pairs. Deterministic calibration and cancellation checks remain enabled.

The noise-gain test uses a valid transform whose red/green residual cancels on the
neutral ramp but strongly amplifies small off-neutral differences. A monotone grey
ramp alone passes; the whole-lookup stability bound rejects it.

## Verification

| Check | Result |
|---|---|
| Application calibration unit checks | 6 passed; all earlier accuracy limits retained |
| Calibrated recipe and renderer checks | 9 passed |
| Main reference native workflow | 4 passed, 1 pre-existing optional local-photo review ignored |
| Full colour-matching library checks | 61 passed, 1 existing unpaired batch accuracy failure |
| Native transfer regression, RAW/photo pixels and one-click export | 6 passed, 1 existing unpaired native accuracy failure |
| Original/edited-pair UI checks | 9 passed |
| UI TypeScript check | Pass |
| Workspace/all-target Clippy with warnings denied | Pass |
| `git diff --check` | Pass |

The additional deterministic-calibration check includes both the compact cubic
and the adaptive detailed grade. The separate unpaired engine remains unqualified;
neither its failing batch gate nor its native photographic gates were relaxed.
Its native split-tone error remains 1.32 → 0.89 and warm-matte error remains
4.17 → 3.35; both miss the retained requirement of after < 60% of before. The
existing batch gate also still fails for ordinary and already-warmed targets.
The main paired improvement does not resolve those single-reference failures.

## Practical limits

The learned operation transfers colour and tone; it does not infer lighting intent,
recover clipped information, reproduce spatial retouching or guarantee the original
editor's arbitrary processing chain. Colour coverage, baseline correction and camera
profiles still matter. Independent selection and validation reduce fitting optimism;
they do not qualify every possible target photograph. Broad real-photo, camera and
human visual evaluation remains outstanding.

Tetrahedral lookup is a standard interpolation option in
[OpenColorIO](https://opencolorio.readthedocs.io/en/v2.4.0/api/enums.html) and
[Colour](https://colour.readthedocs.io/en/develop/generated/colour.algebra.table_interpolation_tetrahedral.html).
The fitting, selection and stability-bound implementation here is local and uses
neither library at runtime.
