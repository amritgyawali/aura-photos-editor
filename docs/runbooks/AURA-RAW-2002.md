# AURA-RAW-2002 - RAW file is corrupt or truncated

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The one-sentence message registered for this code in `errors.toml`. The file appears in the Problems list.

## What actually happened

The container opened but its internal structure does not hold together: an image file directory points past the end of the file, a strip offset overruns the data, a Huffman stream ends early, or the declared dimensions do not match the bytes present.

This is the code for *a real file that is damaged*, as opposed to `AURA-RAW-2001`, which is *not our format at all*.

## What AURA does automatically

The decoder refuses the file rather than guessing. Every read is bounds-checked, so a malformed file cannot make AURA read outside the buffer. The file is quarantined and the import continues.

## Operator steps

1. Do not format the card. Copy the whole card again with a different reader.
2. Re-run the import. A good copy of the same file replaces the quarantined row.
3. If several files from one card fail this way, retire the card; the failure is in the storage, not in AURA.
4. If a *good* file fails this check, that is a decoder bug: keep the sample and open a ticket with the camera model.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Container parsers: `crates/aura-raw/src/container/`
- Fuzz corpus: `crates/aura-raw/tests/fuzz_decode.rs`
