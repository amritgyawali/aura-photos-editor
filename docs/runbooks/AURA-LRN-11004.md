# AURA-LRN-11004 - A change had no decision behind it, or the project has not consented, so nothing was learned

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

AURA kept your change and did not learn from it, because there was nothing of its own to compare it against or learning is off for this wedding.

## What actually happened

A change a photographer made was **kept** and **not learned from**, for one of two reasons:

* there is no phase 13 ledger decision behind it, so the change is not a correction *of* anything.
  A residual measured from no baseline is an absolute edit wearing a residual's shape, which is
  phase 17's condition C4 - and this is the phase that would carry it into every future wedding;
* the project's consent does not allow local learning, which is the default.

A `warning`: the photograph is exactly as the photographer left it. Only the loop declined.

## What to do

If the intent was to teach AURA something, run the pass that makes the decision first - a slider
moved on a photograph AURA has never analysed has nothing to be a correction of. If learning should
be on for this wedding, switch it on in the learning panel; it is per project and off by default.

## Where it comes from

PHASE-30. See `docs/adr/ADR-0061-delivery-learning-loop-and-release.md` and
`docs/plan/phases/PHASE-30-DELIVERY-INTEGRATIONS-LEARNING-LOOP.md`.
