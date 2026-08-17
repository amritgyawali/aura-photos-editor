# AURA-RENDER-8002 - The edit recipe's shape is invalid and it was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

One photograph in the develop view shows its unedited state with a note that its edit could
not be read. Every other photograph is unaffected. The RAW is untouched, as it always is.

## What it means

A value being out of range does **not** raise this. `Recipe::clamped` pulls an exposure of
+9 stops back to +5 and renders, because refusing a wedding over one number is the wrong
trade. What raises this is a recipe whose *shape* is wrong and which therefore has no
correct interpretation:

- a curve with fewer than two points, or whose x values do not strictly increase;
- a crop whose right edge is at or left of its left edge, or bottom at or above top;
- a mask with an empty id, or two masks sharing one id;
- an `invert_of` pointing at a mask that is not in the document;
- a retouch operation naming a mask id that does not exist.

## Operator steps

1. The error context carries `field` and `why`. Both name the exact path.
2. The document itself is in `edit_recipes.body` for that photo, or in the
   `<image>.aura.json` sidecar beside the RAW.
3. `aura_recipe::schema::Validation::check` is the function that refused it; its unit tests
   in `crates/aura-recipe/src/schema.rs` enumerate every refusal above.
4. If the document came from an import or another tool, that tool wrote something the schema
   does not allow. `docs/recipe-schema-v1.md` is the published shape.
