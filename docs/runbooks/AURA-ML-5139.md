# AURA-ML-5139 - Something tried to edit a recorded frame replacement

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Nothing. This is an internal guard.

## What actually happened

Migration 27's `qc_replacement_is_immutable` trigger aborted an UPDATE against a stored swap.

## Why a replacement cannot be edited

This is the strongest disclosure guarantee in the phase, and it exists for the same reason phase 24
made a generative removal's disclosure immutable.

A replacement means the delivered gallery contains a photograph **the photographer never chose**.
Phase 12 selected frame A, this phase measured frame B in the same moment as clearly better, and B
is what the client receives. The `qc_replacement` row is the only place that fact is recorded -
phase 14's rule is that a delivered file is re-creatable from four values, and none of those four is
"and it was a different frame".

So an editable row would let a swap become invisible. The gallery would still contain B, the
photographer would still have never chosen it, and nothing in the catalog would say so.

## Related refusals

A replacement that *would have broken coverage* leaves no row here at all. It is refused before its
metrics are ever compared, and the refusal is a `replacement_breaks_coverage` reason on the ticket.
`coverage_held` is CHECKed to 1, so `SELECT MIN(coverage_held) FROM qc_replacement` is how "no
replacement broke coverage" is verified rather than asserted.

## Fixing it

A caller reaching this trigger is a bug. A swap that was wrong is undone by restoring phase 12's own
selection, which writes a new ticket and a new round - not by editing the record that it happened.
