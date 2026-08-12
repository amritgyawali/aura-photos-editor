# AURA-IO-1005 - Path exceeds the operating system limit

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The one-sentence message registered for this code in `errors.toml`.

## What actually happened

A file path is longer than Windows can open without the extended-length prefix, or exceeds the platform component limit.

## What AURA does automatically

That one file is quarantined with its reason. Every other file imports normally.

## Operator steps

1. Open the Problems list and copy the folder shape shown for the item.
2. Shorten the folder nesting on the source card, or move the shoot closer to the drive root, then re-run the import for that folder only.
3. AURA already applies the Windows extended-length prefix automatically; a failure here means the path is long even for that.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Constructors: `crates/aura-core/src/errors/`
