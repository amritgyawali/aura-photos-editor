# AURA-ML-5136 - One project's quality-control pass could not run

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The QC panel says the wedding has not been checked. Every edit is exactly as it was; nothing has
been changed, replaced or lost, because this phase decides and something else applies.

## What actually happened

`aura_qc::api::QcPass::run` failed before it could finish. The pass is resumable and idempotent, so
a second attempt continues from wherever the first stopped rather than re-inspecting what it already
did.

The usual causes, in the order they are worth checking:

* **No culling run.** This phase inspects the *delivered gallery*, so a project phase 12 has not
  selected has nothing to inspect. `CullService::selection` returning `None` is not an error and
  produces an empty report rather than this code.
* **The catalog could not be read or written.** An `AURA-DB-*` code will be underneath.
* **The thresholds table was refused**, which is `AURA-ML-5140` and is raised in preference to this.

## What this code never means

It never means a photograph was damaged. Nothing in `aura-qc` opens a file, writes a recipe or
reaches a pixel - `crates/aura-qc/tests/no_pixel_ops.rs` is a grep as a test that fails the build if
that stops being true. A failed pass is a wedding that has not been checked, which is a different
and much smaller problem than a wedding that has been checked wrongly.

## Fixing it

Re-run the pass. If it fails repeatedly, run `aura-cli verify --phase 27`, which exercises the whole
assembly against a synthetic gallery and will usually name the stage that is failing.
