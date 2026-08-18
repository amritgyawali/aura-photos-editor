# AURA-RENDER-8003 - The edit recipe comes from a newer schema

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

A note on the photograph: this edit was made in a newer version of AURA, what this version
understands has been applied, and the rest has been kept.

## What it means

The document's `schema` field is greater than `aura_recipe::SCHEMA_VERSION`. This build
renders every field it recognises, and preserves every field it does not in
`Recipe::extra`, which round-trips verbatim on save.

That preservation is the point of the code. ADR-0029 section 7: opening a project in an
older build must not destroy work done in a newer one. Without `extra`, a v1 build reading
a v2 recipe and saving it would silently delete the v2 parameters.

## Operator steps

1. Compare `found` and `understood` in the error context.
2. Upgrade the build to the version that wrote the document; that is the whole fix.
3. If the version that wrote it is unknown, the `engine` field of the recipe names it.
4. Do **not** "clean" the document by deleting the unknown fields. They are the newer
   build's work.
