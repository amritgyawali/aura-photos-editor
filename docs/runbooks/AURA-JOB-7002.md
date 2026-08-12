# AURA-JOB-7002 - Worker lease expired and the task was reclaimed

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The one-sentence message registered for this code in `errors.toml`.

## What actually happened

A worker stopped renewing its lease. Usually machine sleep, a long stall on a network volume, or a crashed worker thread.

## What AURA does automatically

The scheduler reclaims the task and gives it to another worker. Work already committed is not repeated.

## Operator steps

1. No action for a single occurrence; this is the recovery working as designed.
2. Repeated occurrences on the same stage point at a slow or flaky volume: import from a local copy instead.
3. Disable sleep during large imports on laptops running on battery.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Constructors: `crates/aura-core/src/errors/`
