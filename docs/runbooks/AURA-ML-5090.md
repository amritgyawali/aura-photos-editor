# AURA-ML-5090 - Stored retouch plans came from a different build

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Nothing blocking. A background pass re-checks the wedding, and the retouch panel keeps working
from whatever is stored while it does. Anything set by hand is kept.

## What actually happened

A retouch plan carries three version columns and they invalidate three different things:

| Column | Invalidates |
|---|---|
| `model_ver` | every blemish and permanent-feature detection |
| `analysis_ver` | every measurement, cap, band ratio and confidence |
| `preset_ver` | the strengths, the per-scene limits and the texture floors |

`Retouch::outline` compares the stored triple against the running build and logs this code when
they differ. It is **degraded rather than blocking**, exactly as `AURA-ML-5033`, `AURA-ML-5060`
and `AURA-ML-5084` are: a stale plan is still a usable plan, and the alternative is a product
that refuses to show a wedding because it has improved.

The comparison is the point. A band ratio measured under one analysis version and one measured
under another are not the same quantity, and a mean over a mixed set is a number that looks fine
and means nothing.

## What to do

1. Let the pass finish. `RetouchPass::run` selects the frames whose versions do not match, so
   resuming and healing are the same operation.
2. If it does not clear, check that `retouch_presets.toml` loads - see `AURA-ML-5093`. A refused
   preset table leaves `preset_ver` at zero and every row stale.
3. Overridden rows are never overwritten. `user_edited = 1` is checked inside the statement that
   would replace the row.
