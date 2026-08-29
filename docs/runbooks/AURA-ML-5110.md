# AURA-ML-5110 - A change to a photograph restoration was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

A message saying the change was not recorded, and the panel showing what it showed before.

## What actually happened

`RestoreService::set_override` refused, for one of three reasons:

* the override set nothing - every field was absent, which is not a change;
* the photograph has no plan yet, so there is nothing to override;
* the write failed, which surfaces as `AURA-DB-3006` underneath this code.

`RestoreService::accept` uses the same code for the second and third of those.

## What to do

1. If the photograph has no plan, run the restoration pass first. A plan is what an override
   overrides.
2. There is deliberately no way to raise a ceiling from this surface. A photographer chooses
   *which* of the four tiers a frame gets and whether sharpening and face recovery may run; how
   far each goes at that tier is bounded by `aura_core::contract::restore`, and a studio may only
   lower it, in `restore_profiles.toml`. See `docs/restoration.md`.
