# AURA-ML-5077 - A profile was learned with a different render engine or fitter

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Photographs are still styled, and every one of them carries the `engine_mismatch` reason. The
profile report shows the engine the profile was fitted against beside the one this build runs.

## What actually happened

`StyleProfile::engine_ver` records `aura_recipe::contract::recipe::ENGINE` as it was when the
profile was fitted, and `analysis_ver` records which build's fitter and shrinkage produced it.
Both invalidate different things:

* **A different engine** means the recipes that were fitted reproduce their finals through a
  renderer that no longer exists, so the *measured* `overall_de00` is about that build. The
  delta itself is still the photographer's own lean and is still worth applying - which is why
  this is degraded rather than a refusal.
* **A different `analysis_ver`** means the shrinkage or the feature set moved, so two buckets
  fitted under different versions are not comparable with each other.

This is the ninth version-drift code in the product, after `AURA-ML-5015`, `5018`, `5022`,
`5028`, `5033`, `5038`, `5060` and `5071`, and it exists for the same reason all of them do: a
comparison across versions returns a plausible number that means nothing, and it must never
happen silently.

## Operator steps

1. Re-train the profile on the same archives. The scan is resumable and pair fits are cached by
   content hash, so this is much cheaper than the first run.
2. Until then the style is applied with a reduced confidence, so a bucket near the floor stops
   answering and falls back to its parent. That is the intended degradation.
3. A profile that was *imported* from another studio will show this whenever the two are on
   different builds. The honest advice is that the sender re-exports after upgrading.
4. Adoption of a mismatched profile is refused outright - `AURA-ML-5075` cause 3. This code is
   what fires for a profile that was already adopted when the engine moved underneath it.
