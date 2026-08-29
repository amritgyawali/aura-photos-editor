# AURA-ML-5116 - A cleanup proposal broke one of this phase's own guarantees

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Nothing about that one photograph changes. There is no proposal for it, and the Cleanup panel says
so rather than showing an empty list.

## What actually happened

`CleanupProposal::new` refused to construct a proposal. It is the only constructor, and it refuses
four things:

* **a safety verdict that is not `allowed`.** A blocked candidate cannot become a proposal. This is
  the clause that makes "the safety filter runs before the score" a property of the type rather
  than an ordering in a function somebody could reorder later;
* **a verdict that is not self-consistent** - `allowed` disagreeing with its own checks, a check
  recorded twice, a check missing. A verdict that says `allowed` while carrying a failed check is
  the one row that would make the whole audit meaningless;
* **no reasons.** Invariant 2. Here it would be a removal nobody can account for;
* **a degenerate region, or one that leaves the frame.**

## Why this is `item_failed` and not `run_blocking`

One photograph is left exactly as it was shot. That is the correct outcome of this phase far more
often than a removal is, so a refusal here is not a degraded mode - it is the ordinary one.

## When to worry

If it fires on many photographs at once, something upstream is producing malformed verdicts.
`crates/aura-generative/src/safety.rs` is the only place that builds one; every path through it
should produce a verdict that `SafetyVerdict::is_well_formed` accepts, and a test sweeps it.
