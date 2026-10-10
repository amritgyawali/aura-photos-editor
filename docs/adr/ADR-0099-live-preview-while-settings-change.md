# ADR-0099: A live preview while a setting changes

Status: accepted
Date: 2026-10-07

## Problem

ADR-0098 made retouching by hand fast: a stroke, an Apply or a before view re-renders only
what changed. A change to a global setting - exposure, white balance, a curve, a profile -
still re-renders the whole retouch stack, because the stack runs after those stages and reads
the pixels they produce. On a retouched 6 MP portrait the quick look of a new exposure took
3.3 s. Lightroom answers a slider at once; a photographer feels the difference immediately.

## Decision

While the exact quick look of a new setting is being made, the window shows a **live
estimate** of it:

1. Every stage before the retouch stack is run with the new settings - the cheap part, a few
   hundred milliseconds at the quick look's size.
2. The stack's effect is carried over from the last render of the same photograph, size and
   stack (`retouch_cache::carry_over`): each channel is scaled by how much the stack scaled it
   last time, `after / before`, with a small floor so black stays black and a 0-8 clamp.
3. Every stage after the stack runs as usual.

The engine remembers the newest before-and-after pair per photograph and size (three
photographs) whenever it renders a stack, from the checkpoints ADR-0098 already keeps.
`CpuEngine::render_live` returns `None` when there is no pair for exactly this stack - a
different stack, a different size, a frame too large to render whole - and the window then
skips straight to the exact quick look.

The estimate is exact for any setting that scales the frame before the stack (exposure, white
balance) and close for the rest; `tests/retouch_checkpoints.rs` requires the estimate after an
exposure change to differ from the exact render by less than 1.5 code values on average. It is
never cached, never written, never exported and never mistaken for a finished render: the
progressive loader shows it as `live` ("Live preview - finishing the retouch…") and replaces it
with the exact quick look, then full quality.

## Measured

`crates/aura-app/tests/full_quality_preview.rs`, the retouched acne portrait after a restart:
moving exposure, the exact quick look takes 3.9-4.3 s; the live estimate of the next move
takes 0.45 s and is on screen while the exact one is made. The estimate needs one exact
render of the same stack at the same size in this session - the first move after opening a
photograph whose previews came from the disk cache has none, and shows the exact quick look.

Acne clear's independent measurements (its three colour planes, its five bounded-skin levels,
the redness evening's per-pixel pass) also run in parallel now, with the same result.

## Consequences

- A slider in the Studio or a setting under the retouch view answers in the time of the
  stages around the stack instead of the stack itself.
- What is on screen for the first moment after a change is an estimate. It is labelled, it is
  replaced within seconds, and nothing downstream reads it.
- Memory: the pairs are the checkpoints ADR-0098 already holds; at most three photographs keep
  theirs alive past the checkpoint store's own eviction.
