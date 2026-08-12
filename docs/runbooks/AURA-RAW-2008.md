# AURA-RAW-2008 - The decode worker stopped before returning a result

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The one-sentence message registered for this code in `errors.toml`. The file appears in the Problems list.

## What actually happened

Decodes run on a worker thread behind a channel. The channel closed without a result, which means the worker unwound - an allocation failure, a stack overflow on a pathological file, or a panic in a dependency. AURA's own decode path contains no `unwrap`, no indexing without bounds checks and no `unsafe`, so the usual cause is an input that a third-party codec dislikes.

The isolation is the point: the worker dies, the application does not.

## What AURA does automatically

The request returns this code, the file is quarantined, and the pool starts a fresh worker for the next request. No partial buffer is ever published to the cache, so a half-decoded image cannot become a cached preview.

## Operator steps

1. Retry once from the Problems list; a genuine allocation failure under memory pressure may not repeat.
2. If it repeats on the same file, keep that file: it belongs in the fuzz corpus. Add it under `crates/aura-raw/tests/corpus/` (a redacted or synthetic reduction if it is a client's frame) and open a ticket.
3. Check whether several files fail together, which points at machine-wide memory pressure rather than the files.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Worker isolation: `crates/aura-raw/src/timeout.rs`
- Fuzz suite: `crates/aura-raw/tests/fuzz_decode.rs`
