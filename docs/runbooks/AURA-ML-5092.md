# AURA-ML-5092 - One photograph's geometry could not be planned

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

One photograph delivered exactly as it was shot, in a wedding where other frames were
levelled or cropped. The Geometry panel shows it with no plan rather than with an empty one.

## What actually happened

One of two things:

1. **The proxy would not decode.** The geometry pass reads a 2048 px proxy to find straight
   edges, verticals and the composition score of a candidate rectangle. A frame that will not
   decode has none of those, and the pass counts it, codes it and moves on. It is the same
   shape as `AURA-ML-5086`.
2. **The plan broke one of this phase's own guarantees.** `GeometryPlan::broken_guarantee`
   is checked before a plan is stored, and a plan that fails it is stored as **no plan**
   rather than as a weak one. Four of its six clauses are the crop safety filter restated as
   a post-condition:
   * the original framing is not the first crop, or there is no crop at all;
   * a crop that failed the safety filter is still in the list;
   * the primary index does not address one of the crops;
   * a rotation outside the 0.2 to 8 degree band, or below the 0.70 confidence gate;
   * a keystone whose stretch survived the 1.25 cap;
   * no reason, or a confidence outside `0..1`.

The second is the important one, and the reason it is a post-condition rather than a comment
is that the safety filter runs *before* the objective sees a candidate. A filter that runs
first is only a guarantee if nothing downstream can put a rejected rectangle back, and this
is what makes sure nothing did.

## What to do

For case 1, the frame is a phase 02 problem: see `docs/runbooks/previews.md`. For case 2 the
error's detail names the clause, and a plan that trips one is a bug in `aura-geometry`
rather than a property of the photograph - `tests/eval/geometry_eval.rs` has a gate for each
clause.

## How to confirm

```sql
SELECT COUNT(*) FROM photos p
LEFT JOIN geometry_plan g ON g.photo_id = p.id
WHERE  p.project_id = ? AND g.photo_id IS NULL;
```

`GeometryOutline::coverage` reports the same fraction. The denominator is **every**
photograph.
