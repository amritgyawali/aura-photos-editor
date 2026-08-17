# AURA-RENDER-8007 - A re-render did not reproduce the recorded hash

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

AURA says it cannot re-create an exported file exactly as it was delivered. The delivered
file itself is untouched and still correct.

## What it means

`render_hash` is BLAKE3 over the RAW content hash, the canonical recipe JSON, the engine
version and the output spec (ADR-0029 section 5). It is stored with every export. A
mismatch means one of those four moved.

In order of likelihood:

1. **The engine version changed.** A new release that changes any stage's arithmetic changes
   `ENGINE`, and it is supposed to: a renderer producing different pixels must not claim the
   old ones. This is the expected cause after an upgrade and is not a defect.
2. **The recipe changed.** Someone edited the photograph after the export. `edit_history`
   shows when.
3. **The output spec differs.** Re-exporting to Display P3 a file delivered as sRGB is a
   different render, correctly hashed differently.
4. **The RAW differs.** A different file with the same name. The content hash is what
   catches it, and this is the only cause that is genuinely alarming.

## Operator steps

1. **`export_renders` stores all four inputs beside the hash** - `content_hash`,
   `recipe_hash`, `engine` and `output_spec` - for exactly this comparison.
   `RecipeStore::export_inputs(render_hash)` reads them back. Compare them one at a time
   against today's values; the one that differs is the cause.
2. Compare `expected` and `found` in the error context.
3. If cause 4, the original file has been replaced. Do not re-export; find the original.
