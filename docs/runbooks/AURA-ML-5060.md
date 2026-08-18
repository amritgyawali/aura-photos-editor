# AURA-ML-5060 - Stored tone estimates came from a different build

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Nothing stops. Exposure and colour keep working with the stale numbers while a background
pass replaces them. The develop panel shows the caveat once per session.

## What actually happened

At least one of `model_ver`, `analysis_ver` and `targets_ver` on the stored rows disagrees
with this build's. The three invalidate three different things, and the first support
question is always *which one moved*:

* `model_ver` - the learned white-balance prediction and the faceless scene exposure. The
  pass re-runs both heads on every frame.
* `analysis_ver` - the statistics, the neutral detection, the hypothesis scoring and the
  solve. The pass re-reads pixels.
* `targets_ver` - the bands in `exposure_targets.toml`. Nothing is re-measured; the same
  measurements are compared against a different band, which is much cheaper.

All three numbers are in the error's detail and its context fields.

## Operator steps

1. Read `stored_model_ver`, `stored_analysis_ver` and `stored_targets_ver` from the context.
2. Confirm the background pass is progressing: `ToneOutline::coverage` rises as it goes.
3. If it is not, the pending query is `image_tone_estimate` rows whose versions differ, and
   the pass is resumable - restarting it recomputes nothing that is already current.

Never compare two estimates across a version boundary. The numbers are plausible and mean
nothing, which is exactly why this code exists.
