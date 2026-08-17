# AURA-ML-5051 - The culling weight table was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The cull does not start. Any gallery already stored remains readable and unchanged.

## Common causes

The embedded or installation override `cull_weights.toml` is missing, malformed, has a
duplicate scene, uses an unsupported version, carries a weight outside its allowed range,
asks for an aesthetic weight above the cap, or omits the required `rationale` on a row.

## Why refusal is whole-file

A half-loaded weight table would fuse the ceremony with measured weights and the reception
with neutral ones, and the gallery that came out would be culled differently by time of
day. That reads as a product opinion about receptions and nothing like a config error.

## Operator steps

1. Run `aura-cli verify --phase 12` and read the first validation error; it names the
   file, the key and the rule, in that order.
2. Restore the signed build's `cull_weights.toml` or remove the invalid override.
3. A deliberate weight change bumps `calibration_ver` and re-selects affected projects.

Do not fall back to constants. A keeper chosen without a recorded weight version cannot be
reproduced when the photographer asks why.
