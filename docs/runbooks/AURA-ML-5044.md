# AURA-ML-5044 - A composition dismissal was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The framing note remains unchanged. No other photograph or note is affected.

## Common causes

The photo has no composition row, the requested value is empty or combines several
flags, the flag was not present, or the row changed concurrently.

## Operator steps

1. Refresh the Composition card and confirm the note is still present.
2. Retry one named note, not an aggregate such as “all problems”.
3. If the photograph is still being analysed, wait for that item to complete and retry.
4. If it persists, record the photo id and flag slug and inspect the catalog transaction.

Never edit the bit field by hand: `dismissed`, visible `flags`, and `reasons` must stay
consistent. Dismissal is a review projection; it deliberately leaves the original
measurements and composite score unchanged.
