# AURA-IO-1006 - File unreadable or truncated

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The one-sentence message registered for this code in `errors.toml`.

## What actually happened

The file could not be read to the end. Usually a failing card, an interrupted card transfer, or a file still being written by another program.

## What AURA does automatically

The file is quarantined with status error and its reason. The rest of the import continues.

## Operator steps

1. Do not format the card. Copy the whole card again with a different reader if possible.
2. Re-run the import; a good copy of the same file will be picked up and the quarantined row is replaced.
3. If several files on one card fail, treat the card as suspect and retire it.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Constructors: `crates/aura-core/src/errors/`
