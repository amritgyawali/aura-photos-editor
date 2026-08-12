# AURA-DB-3001 - Catalog could not be opened or is not an AURA catalog

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The one-sentence message registered for this code in `errors.toml`.

## What actually happened

SQLite refused to open the file, or the file opened but has none of the AURA tables. Usually a wrong file was picked, or the catalog is on a volume that has gone read-only.

## What AURA does automatically

AURA stops before the writer thread is created. Nothing is written to the file.

## Operator steps

1. Confirm the chosen file is the catalog.sqlite inside a .aura project folder, not a sidecar or a backup fragment.
2. Check the volume is writable and not mounted read-only.
3. If the file is genuinely an AURA catalog, run the integrity path in AURA-DB-3003.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Constructors: `crates/aura-core/src/errors/db.rs`
- Open sequence: `crates/aura-catalog/src/lib.rs`
