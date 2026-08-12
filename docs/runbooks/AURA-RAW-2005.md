# AURA-RAW-2005 - Decode would exceed the per-file memory ceiling

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The one-sentence message registered for this code in `errors.toml`. The file appears in the Problems list.

## What actually happened

Before any buffer is allocated, the decoder multiplies the declared width, height and component count and compares the result with `DecodeLimits::max_pixels` and `max_alloc_bytes`. A file claiming 400,000 x 400,000 pixels is refused here rather than being allowed to ask the allocator for a terabyte.

This is a deliberate defence against hostile or corrupt headers, so it fires *before* the work starts and costs nothing.

## What AURA does automatically

The decode is refused, the file is quarantined, and peak RSS is unaffected. Tier 3 avoids this ceiling for legitimately huge files by decoding in 512 px tiles instead of whole frames.

## Operator steps

1. Check the declared dimensions in the Problems detail. A plausible size (say 60 MP) that trips the ceiling means the ceiling is set too low for this camera - raise `max_pixels` in the decode limits and record the change.
2. An implausible size (billions of pixels) means the header is corrupt; treat it as `AURA-RAW-2002` and re-copy the file.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Limits: `crates/aura-raw/src/timeout.rs`
- Tiled decode: `crates/aura-raw/src/full.rs`
