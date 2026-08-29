# AURA-ML-5113 - A crop was dropped because it would have cut something that matters

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Photographs delivered at the framing they were shot at, with a line in the Framing panel saying
why a tighter crop was held back.

## What actually happened

**This is the product working.** A crop candidate was generated, scored, and then refused by the
safety filter because the rectangle would have cut a face, a pair of hands, joined hands, a
primary identity's body or the moment's key content - or because it kept less than 60 % of the
original long edge.

A refused variant is stored as a row with `safe = 0` and the code that refused it, rather than
being dropped, so the panel can answer "why is there no square crop of this photograph".

The delivered rectangle can never be one of these: `geometry_plan.primary_crop` is an ordinal into
the variant list and a database trigger aborts any statement that would point it at an unsafe row.

## What to do

1. Nothing, if the counts look sane. Most frames in most weddings should keep their framing;
   `GeometryStatusDto::conservatism` at or above 0.70 is the expected state.
2. If a photographer wants the tighter crop anyway, they can set it by hand on that photograph.
   Their rectangle is theirs and is not checked against the protected regions.
3. If *every* crop in a wedding is being refused, look at `facesChecked` first: a project where it
   is zero is a project where nothing was protected, so the refusals are coming from the scene
   rules or the improvement margin rather than from people.
