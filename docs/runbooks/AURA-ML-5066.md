# AURA-ML-5066 - Stored local light plans came from different heads, arithmetic, policy or shaping

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Nothing immediately. A background pass re-plans the affected frames and the Local panel's
numbers move once. Anything they set by hand is kept: `local_light_plan.user_edited = 1` is
inside the `WHERE` of the statement that would replace a row.

## What actually happened

Phase 19 carries **four** version columns, one more than any phase before it, and they
invalidate four different things:

| Column | What it invalidates | Cost to re-do |
|---|---|---|
| `model_ver` | the learned face-lighting targets | a proxy read per frame |
| `analysis_ver` | every measurement and every decision | a proxy read per frame |
| `policy_ver` | the per-scene strengths and caps | arithmetic over stored measurements |
| `shaping_ver` | the grids derived from the stored shaping zones | arithmetic only, but it changes delivered pixels |

The fourth is the one to understand. A dodge-and-burn map is **derived** from the
`ShapingZone` rows rather than stored as a grid - phase 13's "evidence can never be a pixel"
applied to a decision - so a change to the derivation changes what a delivered JPEG looks
like without changing one stored number. `shaping_ver` is what makes that visible.

Comparing a plan made under one set of versions with one made under another returns a
plausible number that means nothing. This code exists so that comparison never happens
silently.

## Operator steps

1. `aura-cli verify --phase 19` prints the running build's four versions.
2. Compare with `SELECT DISTINCT model_ver, analysis_ver, policy_ver, shaping_ver FROM
   local_light_plan`.
3. A `policy_ver` or `shaping_ver` bump re-computes from stored measurements and is cheap. A
   `model_ver` or `analysis_ver` bump re-reads proxies and is not; on a 4,000-image wedding
   allow it to run overnight.
4. Nothing needs deleting. The pass replaces rows in place and skips the ones a person owns.

## What would make this go away

Nothing should. It is a working part rather than a fault: a build that changed how it shapes
light and did not say so is the failure this code prevents.
