# AURA-RENDER-8001 - No accelerated render backend; the processor path was used

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

A line in Settings, then Hardware: develop is running on the processor. Sliders respond
more slowly than the product's stated budget, and exports take longer. Nothing about the
result differs - the processor path is the *reference* path, and the accelerator is
measured against it rather than the other way round.

## This is expected in this build

`aura-render` ships no `wgpu` backend. The reasoning is in
`docs/adr/ADR-0029-render-pipeline.md` section 4: the build machine has no Windows SDK, a
backend that cannot be run cannot be tested, and an untested renderer looks like a shipped
feature while being worse than an absent one. The WGSL sources are compiled in and gated by
`crates/aura-render/tests/shader_parity.rs`, so the shaders cannot drift from the reference
while the backend is absent.

The code is raised once per session at `Degraded`, not once per render. A per-render
warning about a permanent property of the build is noise that trains people to ignore
warnings.

## Operator steps

1. `aura-cli verify --phase 14` prints the backend and the measured proxy render time.
2. `RenderCaps::backend` on the IPC surface is what the Hardware panel reads.
3. When a GPU backend lands, the parity gate in `crates/aura-render/src/parity.rs` runs
   against this same path. Do not widen `parity::TOLERANCE` to make it pass.
