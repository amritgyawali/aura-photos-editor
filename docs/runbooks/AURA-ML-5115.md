# AURA-ML-5115 - Stored cleanup proposals came from different detectors, safety arithmetic or policy

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

A note that AURA is re-checking the wedding for distractions, and proposals reappearing over the
next few minutes. Anything they accepted or rejected themselves is kept and is not asked again.

## What actually happened

A stored `cleanup_proposal` row carries three version columns and one of them no longer matches
this build:

* `detector_ver` - which detector produced the candidate. Invalidates every candidate, because a
  different detector finds different regions.
* `analysis_ver` - which safety arithmetic judged it. Invalidates every verdict, which is the
  column that matters most: a proposal allowed under one version of the denylist intersection is
  not necessarily allowed under the next.
* `policy_ver` - which `cleanup_policy.toml` the caps and the denylist came from. Invalidates the
  size cap and the overlap threshold.

Three columns rather than one because they invalidate three different things, which is phase 06's
rule and the eighth time this product has needed it.

## What AURA does about it

Re-examines the affected photographs in the background, at low priority, off the interactive path.
A proposal a person has accepted or rejected is **not** re-proposed: `user_decided = 1` is checked
inside the statement that would delete it, which is the tenth migration to write that rule.

## When to worry

Never, on its own. Worry if it repeats on every run: that is a version column being written
inconsistently rather than a genuine improvement, and the pass will never converge.
