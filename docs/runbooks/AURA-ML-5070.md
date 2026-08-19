# AURA-ML-5070 - A scene has no local light policy row

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Frames of that scene carry the `scene_strength_limited` reason and get the neutral row's
strengths, which are the gentlest in the file.

## What actually happened

`local_light.toml` has a `[neutral]` row and one `[[scene]]` row per scene AURA classifies.
A scene with no row of its own is judged against the neutral row - and the neutral row is
deliberately timid, because the cost of doing too little to a photograph is that a
photographer does it themselves, and the cost of confidently doing too much is that they undo
it and stop trusting the rest.

The two ways to get here:

1. **Phase 07 gained a scene and phase 19's table did not.** The fix is a row with a written
   rationale, approved by PM, and a `version` bump.
2. **A catalog-local override is missing rows the shipped file has.** An override replaces the
   table rather than merging into it.

`LocalOutline::unpolicied_scenes` lists every affected slug, so this is one query rather than
a hunt.

## Operator steps

1. `SELECT scene, COUNT(*) FROM local_light_plan GROUP BY scene` against the slugs in
   `local_light.toml`.
2. Add the missing rows to the catalog-local override, or remove the override to fall back to
   the shipped table.
3. Bump `version` in whichever file you edited; the background pass re-plans the affected
   frames and `AURA-ML-5066` records the drift.
