# AURA-ML-5100 - A scene has no retouch preset row

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Photographs of that kind are retouched very gently, and the plan says `scene_limited`. Nothing
is broken and nothing is skipped.

## What actually happened

`retouch_presets.toml` carries one row per scene in the phase 07 vocabulary. A scene with no row
falls back to the **neutral** row, which is deliberately the most conservative in the table, and
the plan's confidence is reduced so the frame surfaces in the review queue.

This is the same shape as `AURA-ML-5088` and `AURA-ML-5064`, and the direction of the fallback
is the part that matters: an unknown kind of photograph gets *less* retouching rather than the
average amount, because the cost of under-retouching is a frame somebody adjusts and the cost of
over-retouching is a frame somebody's client notices.

## What to do

1. `RetouchOutline::unpreset_scenes` lists every scene this happened for.
2. Adding a row is a product decision: it needs a strength, a per-operation limit and a written
   reason, and it bumps `preset_ver`, which re-plans the affected frames on the next pass.
