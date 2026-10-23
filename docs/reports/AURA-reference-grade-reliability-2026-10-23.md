# Reference-grade reliability checks — 2026-10-23

Status: reliability changes implemented; photographic grade fidelity is **not
qualified**. Universal correct matching from an edited reference alone is not
established. Three existing accuracy tests still fail; their thresholds were not
relaxed.

## Changes

- Deterministic stratified sampling stays within its budget, including narrow
  images and panoramas, and avoids sampling only one colour of repeated stripes.
- A dominant-colour interval can use a supported neutral minority. A wholly
  strongly chromatic frame does not provide invented neutral-axis evidence.
- Absolute sampled chroma distinguishes monochrome from selective colour. A guard
  limits colourisation of grey targets from multicolour references; toned prints
  can still supply their cast.
- Strongly corresponding scenes retain the best intermediate statistical fit.
  Identical measurements retain the identity transform. Cancellation is checked
  between colour updates.
- Native application refuses increased reference distance before recipe/history
  writes. A dedicated test injects a valid but bad black-output transform and
  verifies that both the saved edit and history remain unchanged.
- Measurement version 2 requires an older reference to be uploaded again. The
  native test verifies refresh of the same content hash and subsequent analysis.
- Related matcher utilities use checked access and deterministic proposal caching.

## Validation

| Check | Result |
|---|---|
| `cargo clippy -p aura-app --lib --no-deps -- -D warnings` | Pass |
| `rustfmt --edition 2021 --check crates/aura-app/src/color_match.rs crates/aura-app/tests/reference_grade_transfer.rs` | Pass |
| `git diff --check` | Pass |
| `cargo test -p aura-app --lib color_match -- --nocapture` | 50 passed, 2 failed |
| `cargo test -p aura-app --test reference_grade_transfer -- --nocapture` | 1 passed, 1 failed |
| `cargo test -p aura-render -p aura-recipe reference_grade -- --nocapture` | 10 passed |
| UI collection matching and reference panel tests | 13 passed |

The eight new robustness unit tests pass, as does the new native regression and
cache-refresh test. These safety checks do not replace the photographic accuracy
tests below.

## Accuracy failures retained

These errors compare the fitted pixels against a known synthetic grade applied
to the target, using mean Euclidean OKLab difference multiplied by 100. They are
not accuracy percentages, CIEDE2000 values or human preference scores.

| Known-grade case | Before | After | Result |
|---|---:|---:|---|
| Same photo, matte grade | 5.38 | 4.06 | Improves, misses the required 60% reduction |
| Different scene, warm grade | 2.49 | 4.98 | Actual grade fidelity gets worse |
| Different scene, split tone through native commands and renderer | 1.32 | 11.63 | Actual grade fidelity gets worse |

A temporary comparison using the original estimator, monochrome rule and final
fit selection also missed the matte and different-scene warm targets. That
comparison does not qualify these failures for release. Intermediate statistical
minima on unrelated scenes were therefore excluded from the new retention rule.

The native split-tone failure is particularly important: reference-distribution
distance improves while error against the known intended grade increases. The
native distance guard prevents regression of that measured objective; it cannot
prove transfer of the intended editing operation between different scenes.

An edited reference does not uniquely separate object colour, illumination,
camera response and grading. Original/edited reference pairs would permit
supervised calibration, with held-out target photos needed to test generalisation.
Broad real-photo, camera/profile and human visual evaluation were not performed
in this change. Use the [workflow guide](../reference-grade-matching.md) for the
supported controls and refresh instructions.
