# AURA-DLV-10004 - No credential is saved for this gallery provider

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

There is no sign-in saved for that gallery. Add one in the delivery settings and the upload will start.

## What actually happened

No credential is stored for this provider.

Credentials live in the OS credential store and never in the catalog, never in a config file and
never in a log - phase 04's rule, which `scripts/check-banned.sh` enforces for every crate.

## What to do

Add the sign-in in the delivery settings. If one was added on another machine, it does not travel
with the catalog: the credential store is per machine, deliberately.

## Where it comes from

PHASE-30. See `docs/adr/ADR-0061-delivery-learning-loop-and-release.md` and
`docs/plan/phases/PHASE-30-DELIVERY-INTEGRATIONS-LEARNING-LOOP.md`.
