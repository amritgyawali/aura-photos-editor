# AURA-ML-5069 - The local light policy table was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

No local adjustments happen at all, and the Local panel says so.

## What actually happened

`crates/aura-brain-photo/config/local_light.toml` would not parse, or a row in it is outside
its documented range, or a row has no `rationale`. **Nothing is loaded**, exactly as phase
15's exposure targets and phase 11's composition rules behave, and for the same reason: half
a policy table would shape the ceremony against measured strengths and the reception against
nothing, and the inconsistency would be invisible in the delivered gallery.

A copy at `<catalog>/config/local_light.toml` overrides the shipped file at run time. A
malformed **override** falls back to the shipped baseline with a warning; a malformed
**baseline** halts.

## Operator steps

1. The message names the row and the field.
2. Restore the shipped file from the installation, or delete the catalog-local override.
3. `aura-cli verify --phase 19` parses the table as its second check and prints the row count
   and `policy_ver`.

## The ranges

Every field's range is documented at the top of `local_light.toml` itself, beside the
argument for why the field exists. `rationale` is refused below nine characters, which is the
same floor phases 11, 12 and 15 use - a policy row nobody can explain is a product decision
nobody made.
