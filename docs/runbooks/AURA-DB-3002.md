# AURA-DB-3002 - Schema migration failed and was rolled back

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The one-sentence message registered for this code in `errors.toml`.

## What actually happened

A migration raised an error. The whole migration ran inside one transaction, so the catalog is still at its previous schema version.

## What AURA does automatically

The catalog is left exactly as it was. AURA will not open it with a newer build until the migration succeeds.

## Operator steps

1. Keep the catalog. Do not re-run the import.
2. Collect the diagnostics bundle from Help, Create support bundle; it contains the failing migration number and the SQLite message with paths redacted.
3. Re-open with the previous AURA build to keep working while support investigates.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Constructors: `crates/aura-core/src/errors/`
