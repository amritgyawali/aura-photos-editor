# Calibrated reference colour transfer - 2026-10-23

Implemented in the main Match a look workflow. Universal correct matching from
an edited reference alone is **not established**. The new original/edited-pair
mode learns a shared global colour operation from corresponding pixels, while
single-reference matching remains an estimate. See the
[implementation notes](../calibrated-reference-matching.md) and
[workflow guide](../reference-first-matching.md).

## Changes

- Optional **Use original + edited pair** selection on the main screen, with
  correct original/edited picker order, cancellation and existing-reference retention.
- Automatic baseline normalisation, native dimension/alignment checks, a robust
  cubic residual fit with identity regularisation, colour-cell balancing, and
  spatially held-out validation. Invalid or unstable calibrations are refused.
- A bounded 17-cubed lookup in the common native preview/export renderer. The
  saved recipe stores coefficients, not reference image pixels. Cache identity,
  invalidation, validation and protected-field merge cover the new extension.
- Per-target native skin/highlight checks and explicit strength backtracking.
  Calibration replaces its predecessor; exact repetition, half strength, zero
  restoration and switching back to ordinary references are covered by tests.
- A second alternating tone/WB/colour and split-tone search for single-reference
  matching. It retains a real-render incumbent and cached canonical proposals.

## Controlled measurements

These are synthetic known-grade recovery tests, not photographic accuracy
percentages. The six first operations are within the fitted cubic family; two
additional operations test nonlinear functions outside that family. Reference
and target have different colour palettes, and the target truth never enters
calibration. Errors below are mean Euclidean OKLab distance multiplied by 100.

| Grade | Target error before | After |
|---|---:|---:|
| Warm encoded RGB | 3.1967 | 0.0070 |
| Matte/desaturation | 5.1356 | 0.0039 |
| Split tone | 0.8376 | 0.0043 |
| Cross-channel colour mixing | 1.9455 | 0.0040 |
| Monochrome | 11.6684 | 0.0037 |
| Cubic tone shape | 0.8713 | 0.0141 |
| Linear-light WB gains | 2.5839 | 0.0074 |
| Sinusoidal S-curve | 4.0227 | 0.1827 |

Through actual native analysis, recipe saving and rendering, a warm matte pair
transfers to a separate scene/aspect ratio with mean absolute RGB byte error
**5.7882 -> 0.2236** on a 0-255 channel scale. The same integration case checks
pixel-identical repeat application, no duplicate changed fields, native
preview/full-export-render agreement, half strength, exact zero restoration,
reference replacement and unchanged original/reference files.

The existing same-photo single-reference fixture improves from mean RGB byte
error **4.593 -> 0.532**, versus 0.710 reported for the previous single-pass
fitter. This is one known renderer grade, not independent photographic evaluation.

## Existing local photographs

Two local photo previews were processed with the existing warm and teal/orange
edited references at 80 percent requested strength, through isolated catalogues.
All input bytes remained unchanged. View the
[new comparison gallery](../../output/aura-joint-reference-2026-10-23/index.html)
and [native measurements](../../output/aura-joint-reference-2026-10-23/results.json).

| Case | Appearance distance before | After |
|---|---:|---:|
| Outdoor portrait / warm reference | 25.556 | 21.514 |
| Indoor portrait / teal-orange reference | 25.304 | 22.212 |

These results are unchanged from the earlier conservative fitter on these two
cases. The rendered outputs were inspected. They preserve the target content and
remain conservative; they do not reproduce the strong reference effects. There
is no independent intended grade for these different scenes, so the descriptor
decrease does not qualify colour fidelity. No original/edited pair for these
Internet references was supplied.

## Final validation

54 targeted tests passed:

| Command/scope | Result |
|---|---|
| `cargo test -p aura-app --lib color_match::calibration -- --nocapture` | 4 passed |
| `cargo test -p aura-app --lib color_match::grade -- --nocapture` | 4 passed |
| `cargo test -p aura-recipe -p aura-render --lib calibrated_look -- --nocapture` | 5 passed |
| `cargo test -p aura-recipe -p aura-render --lib reference_grade -- --nocapture` | 10 passed |
| `cargo test -p aura-app --test reference_style_native --test photo_pixels --test one_click -- --nocapture` | 7 passed; local review ignored in this normal run |
| Explicit ignored local-review test with two target/reference pairs | 1 passed |
| Main-screen, collection matching and collection preparation UI tests | 23 passed |
| `cargo clippy -p aura-app -p aura-render -p aura-recipe --lib --no-deps -- -D warnings` | Passed |
| UI `npm.cmd run build` | Passed; existing large-chunk warning remains |
| Targeted Rust formatting and `git diff --check` | Passed |

The full repository suite was not run. The separate experimental distribution
matcher still has the earlier documented accuracy failures; this change neither
qualifies that engine nor changes its tests' thresholds. The desktop executable
and installer were not rebuilt or relaunched. The production UI build and native
library tests validate the source implementation.

Broad real-camera, mixed-illumination, unsupported-colour and human visual
evaluation remain outstanding. Held-out pair validation cannot establish
generalisation to every future photo. Local effects, lost highlight/shadow data,
camera-response differences and incorrect input colour management can prevent
the requested match even when the shared global mapping is fitted correctly.
