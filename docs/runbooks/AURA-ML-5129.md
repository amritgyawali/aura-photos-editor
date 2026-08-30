# AURA-ML-5129 - The gallery consistency policy table was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Nothing in the wedding was matched, and the panel says the settings could not be loaded.

## What actually happened

`consistency.toml` exists and did not validate. Four reasons, and the third is the one that
matters:

1. The file will not parse as TOML.
2. A tolerance is negative or not finite.
3. **A bound is wider than the contract's own ceiling.**
4. A damping factor is outside 0.30 to 0.90, or a scene name is not one of the 23.

## Why this halts rather than falling back on the defaults

A file that tries to raise a ceiling is not a file with a typo in it. It is a file that would let
this pass move a photograph further than `docs/gallery-consistency.md` promises, and falling back
on the bundled table would run the pass under settings nobody chose while a studio believed their
own were in force.

Phase 21 and phase 22 made the same call about their own tables, and the rule they wrote is the one
enforced here: **a ceiling can be lowered by a studio and raised by nobody.**

A *missing* file is the ordinary case and is not this error - most installations never write one,
and `Consistency::load_or_bundled` falls back on the compiled-in table. A file that is present and
wrong is refused.

## The five ceilings

| Bound | Ceiling |
|---|---|
| `max_d_cct_k` | 450 |
| `max_d_tint` | 12 |
| `max_d_exposure_ev` | 0.35 |
| `max_d_contrast` | 8 |
| `max_d_saturation` | 6 |

A studio may set any of them lower. `damping` is the one value bounded on both sides: zero is a
pass that is switched off, which is a feature flag rather than a config value somebody sets by
accident, and one moves every frame onto its target exactly, which is the flattening section 12
names as the first failure mode.

## Fixing it

The detail line names the offending key and its value. Restore the bundled table from
`crates/aura-brain-gallery/config/consistency.toml`, or lower the value rather than raising it, and
bump `version` so every stored row is re-solved.
