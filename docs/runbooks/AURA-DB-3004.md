# AURA-DB-3004 - Catalog was written by a newer build

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The one-sentence message registered for this code in `errors.toml`.

## What actually happened

The catalog schema version is higher than this build supports. Migrations are forward-only by design.

## What AURA does automatically

AURA refuses to open the catalog, because a downgrade would silently drop columns a newer build wrote. The file is left byte-identical.

## Operator steps

1. Update AURA to the newest version and open the catalog again.
2. If updating is not possible, use a copy of the catalog made before the upgrade.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Constructors: `crates/aura-core/src/errors/db.rs`
- Open sequence: `crates/aura-catalog/src/lib.rs`
