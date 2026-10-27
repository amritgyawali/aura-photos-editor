# ADR-0139: Robust reference-grade estimation

Date: 2026-10-23. Extends ADR-0131 for whole-photo reference matching.

## Context

A fixed sampling grid can select only one colour in a repeated pattern and exceed
its nominal budget on narrow photos. A majority-colour cluster is insufficient
evidence of white balance. Residual chroma after subtracting that cluster can also
misclassify a coloured object as monochrome. Coupled fitting updates can regress
after an earlier good solution.

## Decision

Use bounded deterministic stratified sampling, with fixed integer mixing for
within-stratum offsets. Check neutral estimates against low-chroma support,
estimate support per lightness interval, and keep unsupported cast estimates
conservative. Use absolute sampled chroma for monochrome classification. Preserve
the best intermediate distribution fit, include the identity as a candidate, and
check cancellation between updates. Version the cached measurement separately
from the recipe transform so previously saved edits retain their evaluator.

Reject stale cached measurements with a request to upload the reference again;
do not silently select a weaker matching engine after a cache read failure.
These changes add no dependency, model, frozen-contract change or migration.

## Consequences

Neutral support is a heuristic rather than a calibrated confidence probability.
Sampling cannot guarantee detection of every subpixel or unsampled colour detail.
Distribution matching cannot uniquely recover a grade from an unrelated edited
photo. Controlled grade tests and native command tests remain necessary, alongside
dominant-colour, selective-colour, striped-pattern, panorama and cancellation tests.

Retain the existing no-regression rule when applying the distribution transform:
native-resolution reference distance must not increase beyond numerical tolerance.
The guard runs before recipe/history writes and never equates a low distance with
an exact photographic match.
