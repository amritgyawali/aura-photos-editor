# AURA-ML-5052 - The coverage rule table was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The cull does not start. Any gallery already stored remains readable and unchanged.

## Why this halts rather than degrading

`coverage_rules.toml` is the file that guarantees the vows, the rings and the kiss are in
the gallery. A partially loaded rule table would drop a guarantee silently, and a dropped
guarantee is invisible until a customer finds it. Section 12 of the phase document names
this as the failure that "loses the customer forever", so the loader refuses the file and
leaves the previous table in place rather than enforcing some of it.

## Common causes

An unknown must-have slug (the parser refuses rather than defaulting), a duplicate rule, a
`min` of zero, a match clause naming a scene or interaction outside the frozen vocabulary,
a missing `rationale`, or an unsupported table version.

## Operator steps

1. Run `aura-cli verify --phase 12`; it names the file, the key and the rule.
2. Restore the signed build's `coverage_rules.toml` or remove the invalid override.
3. Re-run and confirm the coverage panel lists all twelve rules before delivering.

Never ship a build that starts with fewer than twelve rules loaded.
