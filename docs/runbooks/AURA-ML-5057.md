# AURA-ML-5057 - A replayed decision no longer reproduces its stored outcome

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Nothing changes. The stored decision is still what the gallery was built from; this is a
report about the difference between then and now.

## The one question to ask first

**Did the question change, or did the answer?** The message says which, and the two are
opposite problems.

| What the message says | What it means | What to do |
|---|---|---|
| "the inputs are identical, so this is a determinism failure" | The same question was answered differently. Something in the pipeline is reading a clock, iterating a hash map, or depending on a thread count. | This is a defect. Invariant 4. Escalate. |
| "the inputs have changed, so this is an upgrade" | A model, a config table or an analysis pass moved underneath the decision. | Expected after an upgrade or a re-analysis. Not a defect. |

## How to tell what moved

The stored decision carries `model_versions` and `config_versions` as they were at the time.
Compare them with the current build's; the pair that differs is the thing that moved. A
`calibration_ver` change re-maps confidences that were already measured; a model version
change means the sub-scores themselves are stale.

## Operator steps

1. Read both hashes from the message. Equal hashes mean the determinism case.
2. For that case: export a support bundle, which carries the decision, its reasons and both
   version lists with every identifier anonymised, and escalate.
3. For the upgrade case: confirm the version lists differ, and record which release the
   drift starts at. Nothing needs fixing.

## What not to do

Do not "repair" the ledger row to match today's answer. The ledger is append-only and the
row is what actually happened; a correction is a new decision that supersedes the old one,
and the database rejects the UPDATE anyway.
