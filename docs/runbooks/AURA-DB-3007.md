# AURA-DB-3007 - Pre-migration backup could not be verified

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The one-sentence message registered for this code in `errors.toml`.

## What actually happened

The copy made before a schema migration either failed integrity_check or had a different row count from the source. Usually the disk filled up during the copy.

## What AURA does automatically

The migration does not run. The catalog stays at its previous schema version and is still openable by the previous build.

## Operator steps

1. Free space on the catalog volume; a backup needs roughly as much room as the catalog itself.
2. Delete old files from the backups folder beside the catalog if they are no longer needed; AURA keeps the newest five automatically.
3. Re-open the project to retry the upgrade.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Constructors: `crates/aura-core/src/errors/db.rs`
- Open sequence: `crates/aura-catalog/src/lib.rs`
