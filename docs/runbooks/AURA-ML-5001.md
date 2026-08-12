# AURA-ML-5001 - Requested model is not in the pinned registry

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence, on the feature that needed the model. Other features are unaffected.

## What actually happened

A caller asked `InferService` for a model by name and version, and `models.lock` has no entry that satisfies it. Models are pinned by name, version and digest; asking for something unpinned is refused rather than resolved loosely, because a loosely resolved model breaks determinism invariant 4 and every score derived from it.

## What AURA does automatically

The request fails with this code before any file is opened. The caller's feature reports the failure and the rest of the run continues; no partial result is written to the ledger.

## Operator steps

1. Compare the model name and version in the log line against `models/models.lock`.
2. If the entry is missing, the installed model pack is older than the application. Install the matching pack from Settings.
3. If the entry exists but the file does not, that is AURA-ML-5003 or AURA-ML-5004 instead - check which code was actually emitted.
4. Developers: adding a model is `docs/runbooks/adding-a-model.md`, and it is a signed change to `models.lock`, never a file dropped into the models directory.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Adding a model: `docs/runbooks/adding-a-model.md`
