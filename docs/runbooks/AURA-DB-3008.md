# AURA-DB-3008 - Catalog busy for longer than the write timeout

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The one-sentence message registered for this code in `errors.toml`.

## What actually happened

A writer could not acquire the database within the ten-second busy timeout. Usually an antivirus scanner holding the file, or a backup agent snapshotting the folder.

## What AURA does automatically

The write is retried with backoff. Persistent failures escalate to the user.

## Operator steps

1. Exclude the catalog folder from real-time antivirus scanning.
2. Pause backup agents during large imports.
3. Press Resume import; the retry is automatic.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Constructors: `crates/aura-core/src/errors/db.rs`
- Open sequence: `crates/aura-catalog/src/lib.rs`
