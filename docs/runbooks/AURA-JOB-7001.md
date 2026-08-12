# AURA-JOB-7001 - Run cancelled by the photographer

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The one-sentence message registered for this code in `errors.toml`.

## What actually happened

Someone pressed Stop, or the app was asked to quit during a run.

## What AURA does automatically

Workers observe the cancel token between units of work, finish the current batch, commit it and stop.

## Operator steps

1. Nothing to do. Press Resume import to continue from the last committed batch.
2. If the app was force-quit instead, the ingest journal still allows a resume; at most one batch of 500 rows is repeated.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Constructors: `crates/aura-core/src/errors/`
