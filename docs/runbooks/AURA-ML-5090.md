# AURA-ML-5090 - Stored geometry plans came from different lens profiles, arithmetic or crop rules

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

A background re-check on a wedding that was already finished, and - while it runs - a
Geometry panel whose project header shows a version that does not match some of its rows.
Nothing is re-cropped that the photographer framed themselves.

## What actually happened

Phase 23 carries **three** version columns, because they invalidate three different things:

* `profile_ver` invalidates the lens corrections. A new bundled profile for a lens changes
  the distortion coefficients, the vignette strength and the per-channel CA scales, and a
  frame corrected under the old table renders differently from one corrected under the new.
* `analysis_ver` invalidates the rotation, the keystone and the crop search. It is the
  arithmetic: the horizon gate, the reduced-rotation loop, the candidate generator and the
  composition objective the candidates are scored against.
* `rules_ver` invalidates every safety margin those rectangles were checked against. A crop
  that passed the filter under a 60 % resolution floor has not passed it under a 70 % one.

Comparing across any of the three returns a plausible number that means nothing - a crop
score from one profile version against a crop score from another, a "kept original framing"
rate mixed across two different improvement margins - so the comparison raises this rather
than happening silently. It is the sixth phase to write this rule and the reason is unchanged
since phase 05.

## What to do

Nothing. The pass is resumable (invariant 5) and re-plans stale rows in the background at
`Priority::Background`. `user_edited = 1` rows are skipped by the statement itself, so a
photographer's own framing survives the re-check.

## How to confirm

```sql
SELECT profile_ver, analysis_ver, rules_ver, COUNT(*)
FROM   geometry_plan
GROUP  BY 1, 2, 3;
```

More than one row means a re-plan is outstanding. `v_geometry_coverage` reports the same
thing per project.

## If it does not clear

A row that never re-plans is a row whose photograph will not decode. That is
`AURA-ML-5092`, one frame at a time; look for it in the job log.
