# AURA-IO-1003 - Volume disconnected during a run

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The one-sentence message registered for this code in `errors.toml`.

## What actually happened

A card reader, external SSD or network share went away mid-import. Common on bus-powered readers and sleeping USB hubs.

## What AURA does automatically

The run pauses and keeps everything already inserted. The ingest journal records where it stopped.

## Operator steps

1. Reconnect the volume. Prefer a powered hub or a direct port for card readers.
2. Press Resume import. Files already hashed are skipped by content hash, so the resume costs seconds, not minutes.
3. If the volume keeps dropping, copy the card to a local folder first and import from there.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Constructors: `crates/aura-core/src/errors/`
