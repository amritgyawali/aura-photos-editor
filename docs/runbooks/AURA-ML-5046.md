# AURA-ML-5046 - The composition rule table was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The composition pass does not start. Existing results remain readable and unchanged.

## Common causes

The embedded or installation override TOML is missing, malformed, has duplicate scenes,
uses an unsupported version, or contains a band outside its allowed range.

## Operator steps

1. Run the phase 11 rule-table check and read the first validation error.
2. Restore the signed build's `composition_rules.toml` or remove the invalid override.
3. Re-run the check before starting analysis.
4. Bump `rules_ver` for an intentional threshold change and re-analyse stale rows.

Do not fall back to constants. A framing decision without the rule version cannot be
reproduced or audited.

