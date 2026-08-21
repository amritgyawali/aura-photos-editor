# AURA-ML-5096 - Stored micro-retouch plans came from different heads, arithmetic or matrix

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

A short "re-checking the small fixes" progress line on a project that was analysed by an earlier
build. Nothing they changed by hand is lost.

## What actually happened

`micro_plan` carries three version columns and they invalidate three different things:

* `model_ver` - the flyaway, glare and lint heads. Every detection is stale.
* `analysis_ver` - this build's arithmetic: a threshold, a cap, a measurement or the way
  confidence is combined. Every magnitude is stale.
* `matrix_ver` - `crates/aura-retouch/config/micro_retouch.toml`. Every switch and every ceiling
  is stale.

`Micro::outline` compares the stored triple against the running build's and reports this code
when they differ. It is **degraded rather than fatal**: the stale plans keep working while the
background pass replaces them, and a caller about to draw a conclusion over a mixed set finds out
before it draws it.

## What to do

1. Nothing. `MicroPass::run` treats a version mismatch as pending work - the resumable pass is a
   query rather than a journal, so a `matrix_ver` bump heals itself.
2. If it persists after a full pass, check that `MicroTable::embedded` is loading the table you
   think it is: a table that fails to parse is `AURA-ML-5099` and blocks the pass entirely.
3. Never compare a metric across a version boundary. That is what this code exists to prevent.
