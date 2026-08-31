# AURA-ML-5132 - Stored camera fingerprints or transforms came from different arithmetic

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Nothing breaks. AURA re-checks the wedding in the background, and anything they chose or set
themselves is kept.

## What actually happened

`camera_transform.analysis_ver` or `camera_transform.policy_ver` does not match this build's.

* `analysis_ver` counts **measurements**: the fingerprint statistics, the pairing rule, the
  background verification, the appearance metric, the solver, the held-out split or the blend
  changed.
* `policy_ver` travels with `camera_match.toml`: a studio edited a bound, an evidence threshold or a
  shooter cap.

## Why the comparison is refused rather than performed

A transform solved under one policy and a transform solved under another are two answers to
different questions, and comparing them returns a plausible number that means nothing. This is the
seventh version-drift code in the product - after `AURA-ML-5015`, `5018`, `5022`, `5028`, `5033`,
`5038` and phase 25's `5127` - and it exists for the same reason as all of them: so that comparison
never happens silently.

## Why it is whole-project rather than per-photograph

Unlike phases 09 to 24, this check is at the level of the project. A transform is a statement about
a **body**, and a project whose Sony was solved under one policy and whose Canon under another has
been matched to two different promises. Phase 25 made the same call about its own tree.

## Fixing it

Run the matching pass again. It clears the project's five tables and re-solves; the reference the
photographer chose, the bodies they switched off and the transforms they set by hand are read out
first and put back. Invariant 5: the work remaining is a query, not a journal, so a `policy_ver`
bump heals itself.
