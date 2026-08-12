# AURA-ML-5009 - New model failed its first real use and the previous version was restored

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence. Work continues on the older model, at the quality the older model had.

## What actually happened

A model version is only marked `confirmed` after it has completed one real inference. Until then the previous version stays on disk. The new version failed that first use - it threw, it timed out, or its output failed the shape contract - so the registry rolled back to the version that was known to work.

## What AURA does automatically

The active version pointer is moved back atomically, the failed version is kept on disk but marked `rejected` so it is not retried on every launch, and `model.update` is emitted with `ok = false`. No catalog row and no ledger entry is written from the failed version.

## Operator steps

1. Read the underlying failure recorded alongside this code; the rollback is the response, not the cause.
2. Re-install the pack once. A corrupted transfer that passed its digest is vanishingly unlikely, but a half-written file on a failing disk is not.
3. If two machines roll back the same version, the version is bad: pull it from distribution and escalate to MLOPS. Staged rollout exists precisely so this is a small number of machines.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Model update failures: `docs/runbooks/model-update-failed.md`
