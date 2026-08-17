# AURA-RENDER-8005 - The AURA sidecar could not be read

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The photograph shows its unedited state, and a note says the saved edit could not be read
and will have to be made again. The RAW is untouched.

## This one costs work, and that is why it is `item_failed`

`<image>.aura.json` is the lossless copy of the edit - masks, retouch, provenance and the
protected-field list. When it cannot be read and the catalog row is also missing, the edit
is gone.

## Operator steps

1. **Check the catalog first.** `edit_recipes` in the project database holds the same
   document; `RecipeStore::load` prefers it, so a readable row means nothing was lost and
   this code came from an explicit sidecar read. Re-export the sidecar with
   `sidecar::write`.
2. If the row is missing too, look for `<image>.aura.json.bak`, which `sidecar::write`
   leaves behind on every overwrite.
3. `edit_history` holds every prior state of the recipe with its own body. The most recent
   entry there is a complete recipe and can be restored.
4. If all three are gone, the edit is genuinely lost. The photograph is not: invariant 1
   means the RAW was never modified.
