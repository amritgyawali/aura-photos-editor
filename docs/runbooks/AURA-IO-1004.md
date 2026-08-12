# AURA-IO-1004 - Disk full

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The one-sentence message registered for this code in `errors.toml`.

## What actually happened

The volume holding the catalog, cache or logs has no free space left.

## What AURA does automatically

The run halts cleanly. The catalog is closed in a consistent state and everything already imported is kept.

## Operator steps

1. Free space on the catalog volume, or move the catalog to a larger local drive with Project, Move catalog.
2. Budget roughly 1 GB of cache per 1,000 images for phase 01 and 02 combined.
3. Press Resume import once there is space.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Constructors: `crates/aura-core/src/errors/`
