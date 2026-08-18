# AURA-RENDER-8010 - The processor and accelerator paths disagreed

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## This stops the build, and it is meant to

Section 6.2: GPU and CPU must agree within 1/1024 in linear light, per stage, and failure
blocks the build. The CPU path is the reference; the accelerator is measured against it.

## What it means

`aura_render::parity::compare` ran one stage on both paths over the fixture set and found a
maximum absolute difference above `parity::TOLERANCE`. The context names the stage, the
measured delta and the tolerance.

## This cannot fire in this build

There is no GPU backend (`AURA-RENDER-8001`, ADR-0029 section 4), so `parity::compare` is
exercised against a deliberately perturbed reference in
`crates/aura-render/tests/parity.rs` - which proves the harness detects drift, and says
nothing about any accelerator. The day a backend lands, this code becomes reachable and
some stage will fail it.

## Operator steps

1. **Do not widen the tolerance.** It is stated in linear light and is the number the golden
   suite's dE2000 bounds are derived from.
2. Fix the stage. The usual causes, in order: a non-deterministic reduction, an atomic in
   colour maths, a differing rounding mode, or `f16` arithmetic where the reference uses
   `f32` (ADR-0029 section 3).
3. If a stage genuinely cannot be made to agree at f16, mark it precision-sensitive and
   compute it in f32 on both sides. That is a documented escape; widening the gate is not.
