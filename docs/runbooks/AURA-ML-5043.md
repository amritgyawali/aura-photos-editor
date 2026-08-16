# AURA-ML-5043 - Composition versions differ

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Existing framing notes remain visible while AURA re-checks the wedding. Reviewed notes
remain dismissed.

## Cause

At least one stored row has a different `model_ver`, `analysis_ver`, or `rules_ver` from
the current build. Comparing its score with a current row would mix two definitions.

## Operator steps

1. Read the composition outline and identify the oldest reported version.
2. Let the background composition pass finish; it selects only missing or stale rows.
3. Confirm coverage returns to the photo count and all three minimum versions equal the
   current build.
4. If rows remain stale, inspect the item-level errors rather than deleting the project.

Do not clear `user_reviewed` or `dismissed`. They are photographer decisions and survive
re-analysis.

## Escalate when

The same row remains stale after a successful pass, or a current row is reported stale.
Capture the three stored and expected versions and the photo id; do not export pixels.

