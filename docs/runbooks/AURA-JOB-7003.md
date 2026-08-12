# AURA-JOB-7003 - Task exceeded its retry budget

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The one-sentence message registered for this code in `errors.toml`.

## What actually happened

The same task failed more times than its retry budget allows, so the scheduler stopped retrying it.

## What AURA does automatically

The item is quarantined with its last error and the rest of the run finishes.

## Operator steps

1. Open the Problems list and read the underlying error code for the item; that code has its own runbook.
2. Fix the underlying cause, then use Retry failed items rather than re-importing the whole folder.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Constructors: `crates/aura-core/src/errors/`
