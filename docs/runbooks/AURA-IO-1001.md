# AURA-IO-1001 - Source folder does not exist

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The one-sentence message registered for this code in `errors.toml`.

## What actually happened

The photographer picked a folder that has since been renamed, unmounted or deleted.

## What AURA does automatically

The run stops before any row is written. The catalog is untouched and resumable.

## Operator steps

1. Ask which drive or card the folder was on and whether it is still connected.
2. Reconnect the volume, then re-open the project and press Resume import.
3. If the folder was renamed, use Relink folder and point at the new location; files relink by content hash, so nothing is imported twice.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Constructors: `crates/aura-core/src/errors/`
