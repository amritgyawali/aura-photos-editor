# AURA-DB-3003 - Catalog failed its integrity check

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The one-sentence message registered for this code in `errors.toml`.

## What actually happened

PRAGMA integrity_check returned something other than ok, or foreign-key violations were found. Causes, in order of frequency: the catalog lived in a synced folder, the drive is failing, or power was lost on a drive that lies about flushing.

## What AURA does automatically

AURA refuses to open the catalog rather than write further into a damaged file.

## Operator steps

1. Open the newest file in the backups folder beside the catalog; every pre-migration backup was verified when it was written.
2. If no backup exists, run aura-cli catalog recover --in catalog.sqlite --out recovered.sqlite, which dumps and rebuilds what SQLite can still read.
3. Re-import the source folders into the recovered catalog; content hashing means only genuinely missing rows are re-created.
4. Check the drive health before continuing to work on it.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Constructors: `crates/aura-core/src/errors/db.rs`
- Open sequence: `crates/aura-catalog/src/lib.rs`
