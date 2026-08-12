# AURA-IO-1010 - Zero-byte file

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The one-sentence message registered for this code in `errors.toml`.

## What actually happened

The file exists but has no content, which almost always means a failed card transfer or a card pulled during writing.

## What AURA does automatically

The file is quarantined. The rest of the import continues.

## Operator steps

1. Re-copy the file from the original card.
2. Re-run the import; the real file replaces the quarantined row.
3. If the original is also empty, the frame is gone; record it in the shoot notes so the couple is not promised a photo that does not exist.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Constructors: `crates/aura-core/src/errors/`
