# AURA-ML-5048 - A stored selection no longer matches this build

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The gallery keeps working. A banner says AURA is choosing again in the background, and
every hand-made keep or removal survives the re-selection.

## Why the check exists

Four things invalidate a selection, and only three of them are versions:

* `model_ver` - one of the heads under the technical, emotion or composition sub-scores
  moved, so the fused score is a different number;
* `analysis_ver` - the passes themselves changed;
* `calibration_ver` - the per-scene mapping changed, so 0.8 no longer means what it did;
* the **content of `cull_weights.toml` or `coverage_rules.toml`**, which a release can
  change without touching a line of Rust.

The fourth is why `SelectionResult::deterministic_hash` covers the config digest as well
as the inputs. A comparison across any of these returns a plausible gallery that means
something else.

## Operator steps

1. Read the stored and current triples from the error context.
2. If only the config digest moved, `git log` the two TOML files; a threshold change needs
   an ADR and a re-selection, not a hotfix.
3. Re-run the cull. It re-reads the sub-scores rather than re-analysing pixels, so it is
   the cheap end of the pipeline.
4. Confirm the coverage panel still shows every must-have covered afterwards.

Never compare a stored selection with a fresh one across a version boundary and report the
difference as a quality change. It is a change of question.
