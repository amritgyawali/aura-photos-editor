# ADR-0100: The graphics card, measured and not enabled

Status: accepted
Date: 2026-10-07

## Problem

After ADR-0098 and ADR-0099, the one render that is still slow is the first full-quality render
of a retouched photograph: the whole retouch stack, about 10.5 s on a 6 MP portrait on the
reference laptop. Lightroom, Retouch4me and Imagen all use the graphics card. ADR-0029 section
4 left a GPU backend for later. The question here is narrower: does moving the retouch stack's
heaviest shared arithmetic onto the card make a photographer wait less?

## What was built

An off-by-default `gpu` feature in `aura-render`, using `wgpu` (DirectX 12 and Vulkan,
WGSL). It moved two primitives to the card:

- **box blurs**: frequency separation, weighted skin means and guided filters all use them;
- **local percentiles**: acne clear's clean-skin levels.

Everything else stayed on the processor. The percentile counts are integers, so the card gave
*identical* levels. The blurs differed from the processor's by rounding only: at most 0.0045 on
a finished pixel, 4e-6 on average. A render made with the card carried `+gpu.<card name>` in its
engine string, so it could never claim to be a processor render.

## Measured

GeForce GTX 1650 with Max-Q Design, 8 processor threads, development build. The retouch stack
from Auto advanced retouch: 32 operations on the 2048 x 3072 acne portrait. The three ways of
running it were measured alternately, twice, to keep thermal drift out of the comparison:

| Whole stack | Round 1 | Round 2 |
|---|---:|---:|
| Processor only | 10.6 s | 11.1 s |
| Card for percentiles | 10.4 s | 10.8 s |
| Card for percentiles and blurs | 10.3 s | 10.7 s |

That is about 3 %. A single three-pass blur of a whole 6 MP plane took about 60 ms on the
processor and 50 to 75 ms through the card. The card's arithmetic is far faster, but each call
uploads a 24 MB plane and reads one back, and that transfer costs as much as the processor
spends computing. A breakdown of acne clear (5 s of the stack) explains why no single change
moves the total: it is about 120 full-plane passes of 20 to 80 ms each. Two passes over 3
million cells, each with percentiles, blurs, connected groups, a chamfer distance and
per-pixel arithmetic, with no one hotspot.

## Decision

**The card is not enabled**, and the feature is not merged. A new dependency tree of about
seventy crates, a second answer to every blur and a driver as a new way to fail are not worth
3 %. The experiment is kept on the local branch `experiment/gpu-compute`, with its equality
tests, as the starting point for the version that would pay off. That version keeps acne clear
**resident on the card**: one upload of the frame, every pass of both rounds as shaders, one
read-back.

Two processor-side findings from the profiling are kept, because they help every machine:

- A local percentile's bilinear spread now works out each column's position once and writes
  rows in place, instead of allocating and joining a vector per row. Time spent spreading,
  summed over threads, fell from 3.3 s to 0.9 s per stack.
- Binning a plane for a percentile runs on every core.

Both give the same pixels; the golden suite and the checkpoint test pass unchanged. Whole stack:
11.4-12.1 s before, 10.6-11.1 s after.

## Consequences

- The first full-quality render of a retouched 6 MP portrait is still about 10 s. ADR-0098's
  checkpoints and ADR-0099's live preview keep that time out of strokes and slider moves; it is
  paid once per photograph and setting.
- The next speed step is the resident acne-clear port above, measured the same way. Without it,
  the card is not worth the dependency.
