# Reference transfer refinement — 2026-10-23

Status: improved, **not qualified for universally correct photographic matching**.
The single-edited-reference engine still fails independent known-grade accuracy
gates. It remains separate from the original/edited-pair workflow on the main
Match a look screen. The existing same-photo, different-scene and native pixel
thresholds remain unchanged. The batch criterion changes explicitly as described
below; all failing accuracy checks remain enabled.

## Changes

- Estimate white balance and split toning from supported low-chroma clusters,
  including a neutral minority within a small lightness interval. Remove the
  dominant-object fallback, check midtone anchors against brighter evidence,
  allow shadow split-toning residuals, and interpolate unsupported intervals
  between actual anchors. No neutral evidence produces a zero axis.
- For strongly corresponding scenes, retain iterative tone, cast, chroma, hue
  and zone fitting through the renderer's evaluator. For unrelated colour scenes,
  keep the target's tone curve, per-sector brightness and per-zone colourfulness.
  Use robust shared-sector saturation ratios and one bounded shared-hue update,
  with regularisation toward identity. Monochrome/toned-print references keep
  their separate desaturation path.
- The unrelated-scene 0.5 strength prior and all estimator cutoffs are authored
  regularisation, not measured confidence or a guarantee of correct illumination.
  The content score is structural evidence, not proof that images correspond.
- Measurement version 3 refreshes references and marks earlier saved analyses
  stale. Apply refuses a session with incompatible measurements before rendering
  or writing an edit. Existing saved recipe rendering uses the unchanged stage.
- Recheck clipping and per-person skin readings on native-resolution baseline
  and candidate renders, including extrapolated strength controls. Reject a bad
  native result before recipe/history writes, even if its reference statistics
  would improve. Preparation-time skin protection alone was insufficient.
- Accuracy tests report every grade before failing. The batch test now compares
  target pixels with the known intended operation, rather than requiring a large
  reduction of reference-histogram distance. The older criterion could reward
  repainting unrelated objects. Its new gate requires at least a 40% reduction
  in actual grade error, while retaining descriptor non-regression. Native
  photographic gates run after the independent saved-edit checks, so their
  failures do not hide those results.

## Controlled photographic results

All values below are mean Euclidean OKLab difference multiplied by 100 against
the known operation applied to target pixels. They are not accuracy percentages,
CIEDE2000 values or human visual assessments. The two synthetic scenes have
different objects, colour proportions and brightness distributions.

| Known grade | Target before | Same-photo after | Different-scene after |
|---|---:|---:|---:|
| Warm linear-light balance | 2.49 | 0.25 | 1.53 |
| Cool balance + contrast | 4.21 | 0.42 | 4.14 |
| Matte | 5.38 | 0.25 | 4.87 |
| Split tone | 1.32 | 0.20 | 0.87 |
| Punchy | 4.31 | 0.82 | 4.22 |
| Monochrome | 8.43 | 0.11 | 0.68 |
| Sepia | 9.58 | 0.24 | 1.11 |
| Teal-orange | 3.55 | 1.31 | 3.53 |

The existing same-photo eight-grade test passes all thresholds: after < 40% of
before, after < 3.5, and improved reference distance. The existing different-scene
test passes its weaker requirement of improvement on each of eight grades.
Several improvements there are very small and do not establish close matching.

For comparison, the earlier report measured same-photo matte at 5.38 → 4.06,
different-scene warmth at 2.49 → 4.98, and native split tone at 1.32 → 11.63.
Those failures are retained in the historical report rather than overwritten.

## Accuracy failures retained

| Check | Before | After | Required |
|---|---:|---:|---|
| Batch split tone, ordinary target | 1.319 | 0.868 | After < 0.791 |
| Batch split tone, already warmed target | 1.281 | 1.791 | After < 0.769 |
| Native split tone | 1.32 | 0.89 | After < 60% of before |
| Native warm-matte replacement | 4.17 | 3.35 | After < 60% of before |

The warmed batch target exposes ambiguity about its existing cast: the requested
known operation in this test retains that cast, while an unpaired estimator may
interpret it as illumination to remove. A finished reference alone cannot decide
which interpretation the photographer intended. Lower reference distance is
therefore not proof of grade fidelity.

## Validation and practical limits

| Verification | Result |
|---|---|
| `cargo test -p aura-app --lib color_match -- --nocapture` | 59 passed, 1 failed (batch known-grade fidelity) |
| `cargo test -p aura-app --test reference_grade_transfer --test reference_style_native --test one_click --test photo_pixels --no-fail-fast -- --nocapture` | 9 passed, 1 failed (native known-grade fidelity), 1 pre-existing optional local-photo review ignored |
| Reference panel, original/edited-pair panel and collection UI tests | 22 passed |
| `npm.cmd run lint` in `ui` | Pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | Pass |
| `git diff --check` | Pass |

The native clipping test deliberately proposes an all-white output against an
all-white reference. Matching those statistics would destroy the target's detail;
Apply refuses it and preserves both recipe and history. The upgrade test likewise
checks that a version-2 session cannot be applied against version-3 measurements.
The existing native test reaches its retained photographic gates only after zero
strength, Undo/Redo, replacement, restart and source-byte checks have passed.

The main original/edited-pair and conservative single-reference native tests
pass. The pair's different-scene mean RGB byte error remains 5.7882 → 0.2236 in
its controlled case; these are RGB byte units, not the OKLab units above. That
pathway remains the stronger source of actual transformation evidence; see
[calibrated matching](../calibrated-reference-matching.md).

This change does not establish accuracy across real camera/profile combinations,
mixed lighting, local relighting, selective retouch, unknown arbitrary LUTs or
clipped data. Broad real-photo and human visual evaluation remains outstanding.
Exact transfer for every possible photo is not identifiable from one finished
reference. No "100% correct" result or release qualification is claimed.
