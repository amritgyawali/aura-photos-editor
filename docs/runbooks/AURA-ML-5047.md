# AURA-ML-5047 - A scene has no composition rule row

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The affected photographs are judged cautiously with neutral rules. Their confidence is
lower, and they remain available for review.

## Cause

The scene service returned a vocabulary value that has no measured row in
`composition_rules.toml`, or no scene result was available.

## Operator steps

1. Read the `composition.unruled` telemetry for the scene slug and affected count.
2. Confirm the scene contract and rule-table spellings agree.
3. For a genuinely new scene, add a documented row with evidence-based bands and bump
   `rules_ver`; do not copy the nearest-looking ceremony by guess.
4. Re-run composition and confirm the outline's `unruled` count returns to zero.

An `unknown` scene can be expected while scene analysis is incomplete. Complete that pass
before treating this as a composition defect.

