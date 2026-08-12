# AURA-ML-5012 - Delta patch did not apply cleanly

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence, and an update that takes longer because it transfers the whole model instead of the difference.

## What actually happened

Small model updates ship as an `AURADLT1` block delta against a named base version. Applying it failed: the base file on disk was not the version the delta was built against, the delta was truncated, or the reconstructed file did not match the target digest. The delta path is an optimisation, and an optimisation that cannot prove its result is discarded rather than trusted.

## What AURA does automatically

The reconstruction is thrown away, the full model is fetched instead, and `model.update` records `delta_used = false`. The installed version keeps working while this happens.

## Operator steps

1. Nothing, if the full transfer then succeeds.
2. If deltas fail on many machines for one version pair, the published delta was built against the wrong base: rebuild it and re-sign, or publish the pair as a full update.
3. Developers: `crates/aura-models/src/delta.rs` documents the format and round-trips it in tests. A delta that fails there is a code defect; one that fails only in the field is a publishing defect.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Model update failures: `docs/runbooks/model-update-failed.md`
