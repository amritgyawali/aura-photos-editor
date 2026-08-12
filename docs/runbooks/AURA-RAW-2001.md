# AURA-RAW-2001 - File is not a RAW format AURA can read

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The one-sentence message registered for this code in `errors.toml`. The file appears in the Problems list.

## What actually happened

The decoder sniffed the first bytes of the file and did not recognise a container it can open. The extension is never trusted, so a `.CR2` that is really a text file lands here rather than being mis-parsed.

Common causes: a card recovery tool wrote a stub, a video or audio file was renamed, or a camera launched after this build shipped and uses a container AURA has not yet learned.

## What AURA does automatically

The file is quarantined with this code, the import continues, and `preview.quarantined` is emitted with the camera model when one is known. Nothing is written to the file.

## Operator steps

1. Check the camera support matrix in `docs/camera-support.md`. A format listed as *not supported* is a known gap, not a defect.
2. If the format should be supported, collect one sample file and the camera model and open a decoder ticket. Never send the couple's imagery outside the studio.
3. If the file opens in no other program either, the copy is bad: copy it from the card again with a different reader.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Constructors: `crates/aura-core/src/errors/raw.rs`
- Format sniffing: `crates/aura-raw/src/format.rs`
- Preview troubleshooting: `docs/runbooks/previews.md`
