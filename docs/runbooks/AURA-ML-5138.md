# AURA-ML-5138 - Something tried to edit or remove a recorded remediation round

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Almost certainly nothing. This is an internal guard and a correctly built product never reaches it.

## What actually happened

`qc_round` is append-only. Two triggers enforce it: `qc_round_no_update` aborts every UPDATE, and
`qc_round_no_direct_delete` aborts a DELETE against a round whose ticket still exists. A round is
removed only by the `ON DELETE CASCADE` that follows its ticket.

It is the second append-only table in the product, after migration 13's ledger.

## Why a round cannot be edited

Section 6.3 of the phase document bounds the re-edit loop at two rounds per image and requires that
"all rounds are recorded so the history of an image's edit is fully reconstructable".

A bound whose evidence a later pass can rewrite is not a bound. The specific failure it prevents:
a second pass that re-used round 1's row instead of writing round 2 would produce a catalog in which
every image has been remediated at most once, the loop bound is trivially satisfied on every query,
and a build that had started thrashing would look identical to one that had not.

The other half of the reason is that a reverted round is the most valuable row in this table. A
correction that was tried and put back is what tells a photographer - and phase 30's learning loop -
that a remedy family does not work for a kind of frame. Overwriting it loses exactly the failures
worth learning from.

## Fixing it

A caller reaching this trigger is a bug. Record a second round rather than editing the first;
`aura_qc::reedit` is the only writer and does this correctly.
