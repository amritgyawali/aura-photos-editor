# AURA-RAW-2004 - Decode exceeded its per-file time limit

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The one-sentence message registered for this code in `errors.toml`. The file appears in the Problems list with a retry action.

## What actually happened

Every decode runs on a worker with a deadline (`DecodeLimits::timeout_ms`, default 5,000 ms for tier 1 and 20,000 ms for tier 3). The watchdog fired before the worker returned. Either the file is pathological, the drive stalled, or the machine is heavily oversubscribed.

The watchdog exists so that one bad file can never hang the application: the request returns this error while the worker is abandoned.

## What AURA does automatically

The request fails with this code, the file is quarantined for this stage only, and the rest of the batch continues. The abandoned worker is not killed - it is left to finish and its result is discarded - because cancelling a thread mid-decode is not safe.

## Operator steps

1. Retry the file from the Problems list. A stalled network drive or a sleeping USB disk usually succeeds on the second attempt.
2. If the file fails repeatedly, copy it to a local SSD and retry. If it then succeeds, the fault is the storage.
3. If it fails on local storage too, keep the sample: a decoder that cannot finish a valid file inside 20 seconds is a performance bug.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Watchdog: `crates/aura-raw/src/timeout.rs`
- Budgets: `perf/budgets.toml`
