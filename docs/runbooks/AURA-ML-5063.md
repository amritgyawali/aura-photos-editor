# AURA-ML-5063 - The exposure target table was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The tone pass does not start. Existing estimates remain readable and unchanged.

## Common causes

`exposure_targets.toml` is missing, malformed, has a duplicate scene, uses version 0, or
contains a value outside its documented range. A row with a rationale shorter than nine
characters is also refused: every product decision in that file needs a written reason.

## Why this halts rather than degrading

The same argument `AURA-ML-5036` and `AURA-ML-5046` make, one phase further on and with
higher stakes. A half-loaded target table would expose the ceremony against measured bands
and the reception against neutral ones, and the result is a gallery whose brightness changes
at a chapter boundary. That looks like "the product does not understand receptions" and
nothing like a configuration error.

## Operator steps

1. Read the first validation error: it names the file, the key and the rule, in that order.
2. Restore the signed build's `exposure_targets.toml`, or remove the installation override
   at `<catalog>/config/exposure_targets.toml`. A malformed *override* is not fatal - it
   falls back to the shipped baseline with a warning. A malformed *baseline* is.
3. Bump `version` for an intentional band change and let the background pass re-analyse.

Do not fall back to constants. An exposure decision without a target version cannot be
reproduced or audited.
