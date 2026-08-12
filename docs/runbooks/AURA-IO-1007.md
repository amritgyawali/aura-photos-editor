# AURA-IO-1007 - File changed while being read

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The one-sentence message registered for this code in `errors.toml`.

## What actually happened

The file size or modification time changed between the stat and the end of the hash. Typically a tethered-capture folder or a sync client writing in the background.

## What AURA does automatically

The file is retried with backoff. If it settles, it imports normally.

## Operator steps

1. Wait for the tethered capture or the copy to finish before importing.
2. Exclude the shoot folder from any sync client while importing.
3. Press Resume import; the retry is automatic and bounded.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Constructors: `crates/aura-core/src/errors/`
