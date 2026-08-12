# AURA-DB-3005 - Another AURA instance holds the catalog write lock

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The one-sentence message registered for this code in `errors.toml`.

## What actually happened

A second AURA process, or an orphaned one from a crash, owns the catalog lock file.

## What AURA does automatically

The second window refuses to open the catalog so two writers can never race.

## Operator steps

1. Close the other AURA window and try again.
2. If no other window exists, quit AURA completely, confirm no aura process is running in Task Manager or Activity Monitor, then re-open.
3. The lock file lives beside the catalog and is released automatically when the owning process exits.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Constructors: `crates/aura-core/src/errors/`
