# Match a look: perceptual fitting and native quality checks

Date: 2026-10-24. Scope: the main reference-first Match a look workflow.

The main fitter now averages source pixels when reducing a native working-space
frame. Its former point sampling could select only one colour of striped fabric.
The implementation reuses the renderer's deterministic area reduction, validates
frame dimensions, and does not enlarge smaller inputs. The studio colour panel
already had area reduction and retains its existing implementation.

An aligned single reference can use a prepared OKLab objective at 32, 16 and 8
pixel scales, weighted 0.7, 0.2 and 0.1. Native acceptance uses the same objective.
Reference colour conversions are prepared once per fit. These scores are distances,
not percentages of accuracy, and cannot be compared directly to older RGB scores.

Pixel alignment requires global signed-gradient correlation of at least 0.985,
correlation of at least 0.97 after removing an affine lightness plane, residual RMS
of at least two RGB8 lightness units in both images, and agreement in at least 80%
of supported local tiles. At least six tiles must be supported. Local agreement
requires gradient correlation of at least 0.9. The energy thresholds and scale
weights are authored heuristics, not measured probabilities or identity proofs.
Other scenes retain aggregate appearance fitting and the existing regularisation
prior. A rejected alignment is not a rejected match.

Ordinary and calibrated main-workflow matches now detect guard regions again from
the native baseline and use the existing bounded shadow-contour guard, alongside
highlight clipping, skin checks and measured improvement. The contour check samples
at most 4,096 pixels and their neighbours; it cannot certify every output pixel.
Canonical recipes, protected manual fields, cancellation and transactional saving
continue through the existing paths.

## Evidence

- New regressions cover alternating fabric, malformed/empty frames, smooth ramps
  falsely sharing gradient correlation, a shared horizon concealing mismatched
  details, and multiscale perceptual scoring on supported structure.
- The new native 768 x 512 striped fixture measures a known rendered exposure/WB
  grade, repeated application, zero strength and unchanged source bytes. Its
  mean RGB8 error falls from 9.985 to 6.874; all these checks pass.
- The ordinary single-reference fixture's mean RGB8 error against its known grade
  is 4.593 before matching and 1.207 after. The earlier historical report recorded
  0.710 for an older fitter. This change does not establish uniformly better RGB
  reconstruction; conservative alignment can choose the aggregate objective.
- The two original/edited-pair fixtures measure RGB8 errors of 5.7882 -> 0.2237
  and 5.5648 -> 0.8216 on their different target scenes.
- The main native suite has five passes and one optional real-photo review
  skipped. The colour-panel native suite has ten passes and one optional RAW
  fixture skipped. The transfer native suite has two passes and the pre-existing
  fidelity failure described below.
- 54 focused UI tests passed. Strict workspace Clippy, formatting, the IPC name
  check (353 commands), the locked contract check (83 entries), the banned-pattern
  scan and `git diff --check` passed.
- The focused colour-unit run had 91 passes and the unchanged existing Natural
  batch fidelity failure: 1.319 -> 0.868 and 1.281 -> 1.791.

The broader transfer integration suite also retains its existing known-grade
fidelity failure (split tone 1.32 -> 0.89; warm matte 4.17 -> 3.35). These same
fidelity results reproduce in the pre-change test executable. Its clipping test
had an outdated message substring; the assertion now checks the refusal code and
quality-check wording while retaining the unchanged-recipe/history assertions.
No photographic accuracy assertion was relaxed.

These fixtures establish bounded behaviour on synthetic SDR images, not universal
matching, real-world RAW qualification, or recovery of an unknown editing operation.
For stronger evidence across different scenes, use an aligned original/edited pair.
