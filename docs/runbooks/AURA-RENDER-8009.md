# AURA-RENDER-8009 - The render was streamed in tiles to stay inside the budget

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

A large file takes longer to export and a note says AURA developed it in pieces. The result
is identical to a whole-frame render.

## What it means

Section 6.3: a full-resolution export streams tiles so a 100 MP file never needs to fit in
memory at once. `aura_render::tiles` committed the frame in `TILE_SIZE` regions with a halo,
because the whole-frame working buffer would have exceeded the render budget.

Identical, not approximately identical: every stage in the graph is either point-wise or
declares a halo radius, `tiles::halo_for` sums them across the enabled stages, and
`tiled_render_equals_whole_render` in `crates/aura-render/tests/tiling.rs` asserts the two
paths are bit-identical on the fixture set.

## Operator steps

1. The context carries `pixels` and `budget_bytes`.
2. The budget comes from the phase 03 hardware plan. On a machine with more memory, raising
   it removes the tiling and the code.
3. If this fires on a *small* frame, the budget is misconfigured, not the frame. Check what
   `RenderCaps::max_working_bytes` reports.
