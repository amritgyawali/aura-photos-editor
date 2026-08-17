# AURA-ML-5058 - A confidence was banded without a fitted calibration

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

A note beside the confidence, in the Explain panel and in the summary: AURA has not yet
learned how often it is right about this kind of decision, so the number is a rough guide.
AURA also asks for review slightly more often than the thresholds alone would require.

## This is expected in this build

Every calibration model ships as the identity map at version 0. Fitting one needs labelled
outcomes - a photographer's own overrides, or a labelled set from real weddings - and this
build has neither. The warning fires once per run, not once per decision, because an
unfitted map is a property of the build rather than of any photograph.

## Why it raises a band

Section 6.4's bands are defensible only if 90 % confidence means 90 % correct. Until a
calibration exists, nothing has established that, so `uncalibrated_raises` in
`autonomy_bands.toml` moves every decision one band toward review. The reason code
`uncalibrated_confidence` is attached to the decision, so the photographer reads *why* AURA
asked rather than seeing 0.99 beside a review request and concluding the product is broken.

## Operator steps

1. Confirm the calibration set is unfitted: `aura-cli verify --phase 13` prints it.
2. If a fitted set was expected, check that `calibration_models` has rows with `version > 0`
   and a `method` other than `identity`.
3. To silence it properly, ship a fitted calibration. To silence it improperly, set
   `uncalibrated_raises = false` - and do that only in the release that ships the fit, never
   before.
