# AURA-ML-5056 - A decision could not be replayed

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Usually nothing: replay is a support command rather than a step in the pipeline. If they ran
`aura-cli replay` themselves, it says the decision could not be worked out again and that
the decision itself is unchanged.

## What it means

`aura-cli replay --decision <id>` re-runs a decision from the ledger against the catalog as
it stands now, and compares. This code means the *re-run* could not happen at all - not that
it came out differently, which is `AURA-ML-5057`.

## The three ordinary causes

1. **The subject is gone.** The photograph was removed from the project, or the project was
   deleted. The ledger row survives - it is history - and there is nothing left to decide
   about.
2. **The analysis was cleared.** A decision cannot be re-run without the sub-scores it fused.
   Re-run the analysis passes and try again.
3. **The deciding phase does not implement replay.** Only phase 12's culling decisions are
   replayable in this build. A ledger row of kind `edit`, `retouch`, `qc`, `curate` or
   `export` has no source to re-run it, because the phases that write those do not exist
   yet.

## Operator steps

1. Read the `detail`; it names which of the three it is.
2. For case 2, re-run the cull and replay again.
3. For cases 1 and 3, the answer is that this decision is not replayable. Read it instead:
   the row carries the reasons, the evidence and the versions, which is what a support case
   usually needs.
