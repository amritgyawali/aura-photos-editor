# AURA-ML-5079 - A masking run was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The mask panel says there is nothing to mask, and offers the cull view. No masks are produced
and nothing is written.

## What actually happened

`Masks::pass` read `MaskOutline::selected` and found zero. Section 6.3 makes masking **lazy and
post-cull**: masks are generated for the frames phase 12 kept, because rejected frames never
need them and not computing four thousand of them is a large part of why the phase meets its
time budget.

A project with no selection is therefore not a project with nothing to do - it is a project
where the question has not been asked yet. Masking every frame would be the work the lazy policy
exists to avoid, so the run is refused rather than silently expanded.

## Operator steps

1. Run the cull (`aura-cli verify --phase 12` exercises the same path) or press **Cull** in the
   gallery view.
2. Re-run the masking pass. It resumes from the query, so a partial cull is fine - masks appear
   for whatever is selected now, and the rest arrive when more is selected.

## What would make this impossible

A default that masked everything when nothing was selected. It was rejected: on a 4,000-frame
wedding it is about eight minutes of work nobody asked for and 700 MB of payload, and the
photographer would have no way to tell that it had happened.
