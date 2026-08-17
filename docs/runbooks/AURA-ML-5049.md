# AURA-ML-5049 - A culling override or a re-selection was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The keep, the removal, the mode switch or the size slider did nothing. The gallery is
exactly as it was.

## Common causes

* The photograph is unknown, or belongs to a project that has never been culled - there is
  no selection for an override to attach to.
* `clear_override` was called on a frame that carries no override.
* `resize` or `set_mode` was called before any scores were stored, so there is nothing to
  re-allocate.

## Operator steps

1. Confirm a selection exists: `aura-cli verify --phase 12` prints the outline, and
   `CullOutline::is_empty` is true when nothing has been culled.
2. Run the cull once, then retry the override.
3. If the project has been culled and the refusal persists, capture the photo id and the
   project id; a refusal on a frame that is in the selection is a bug in the store, not a
   user error.

An override is unbeatable once recorded. A refusal that silently succeeded would be worse
than this error, which is why the check is inside the statement rather than before it.
